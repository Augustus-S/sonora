use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, Pixels, Render, Window};
use i18n::t;
use music::Level;
use music::equalizer::{
    Curve, FREQUENCY_RANGE, Filter, GAIN_RANGE, Invalid, Kind, OCTAVES_RANGE, Q_RANGE, Response,
    Width,
};
use state::{AppSettings, Equalizer, Outputs, Signal};
use ui::{
    ActiveTheme as _, GraphEdit, ResponseAxes, ResponseGraph, ResponseGraphState, ResponseNode,
    ResponsePoint, Room,
};

use super::rows::{NEW_FILTER, figure};

/// The graph's frequency axis in hertz, the band people hear.
const GRAPH_FREQUENCIES: (f64, f64) = (20., 20_000.);
/// How many points the response is sampled at across the band, each enabled filter's centre
/// coming on top, so a narrow notch reaches the floor.
const POINTS: usize = 2048;
/// The rate the response is drawn at while no stream is open.
const IDLE_RATE: u32 = 48_000;
/// How much one wheel step over a node narrows or widens it, as a power of two.
const WIDTH_STEP: f64 = 1. / 12.;
/// How many significant figures a dragged frequency keeps, so the field reads 1230 rather than
/// 1234.5678.
const FREQUENCY_FIGURES: i32 = 3;
/// The levels at the bottom and the top of the plot that the spectrum is drawn between, in dBFS.
const SPECTRUM_SCALE: (f32, f32) = (-90., 0.);
/// How tall the graph is in cover sizes, with the room of a wide window, and with less.
const GRAPH_TALL: f32 = 2.8;
const GRAPH_SHORT: f32 = 2.;
/// The gain axis's half spans in decibels, narrowest first. The axis shows the narrowest one the
/// curve, the nodes and the preamp fit in, so a gentle preset fills the plot.
const GAIN_SPANS: [f32; 3] = [12., 18., 24.];
/// How far past a span the curve may reach and still be shown in it, so a filter at exactly the
/// span does not tip the axis over on the rounding of its response.
const SPAN_SLACK: f32 = 0.05;

/// The response graph as a view of its own, which the screen lays out at a fixed height. It
/// owns the ids and the selection of the filters, follows the playing curve, and plays the
/// edits made on it.
pub(super) struct EqualizerGraph {
    equalizer: Entity<Equalizer>,
    settings: Entity<AppSettings>,
    outputs: Entity<Outputs>,
    signal: Entity<Signal>,
    graph: Graph,
    /// What the graph last drew, so a notify that changed none of it draws nothing.
    drawn: Option<Drawn>,
}

/// Emitted when the selected filter changes, which the filter list highlights too.
pub(super) struct Selected;

/// What the graph shows from the equalizer, the output and the spectrum.
#[derive(PartialEq)]
struct Drawn {
    curve: Curve,
    on: bool,
    preamp: f64,
    rate: u32,
    spectrum: Option<Vec<Level>>,
    /// Whether advanced mode lets nodes be added, removed and widened on the graph.
    advanced: bool,
}

/// What the graph keeps between renders: an id per filter of the playing curve, the selected
/// filter, and the response as last sampled. Filters that came from anywhere but this screen
/// get new ids, so a selection or a drag never carries into another profile's filters.
struct Graph {
    state: ResponseGraphState,
    /// One id per filter of `filters`, in its order.
    ids: Vec<u64>,
    next_id: u64,
    selected: Option<u64>,
    /// The filters the ids were last brought in step with.
    filters: Vec<Filter>,
    /// The filters this screen put in place last, until the playing curve shows them.
    own: Option<Vec<Filter>>,
    plotted: Option<Plotted>,
    /// The half span of the gain axis last shown. It holds still while a node is held, so the
    /// scale never shifts under a drag.
    span: f32,
}

/// The response the graph draws, and what it was sampled for.
struct Plotted {
    curve: Curve,
    rate: u32,
    response: Response,
    points: Vec<ResponsePoint>,
    /// How far from 0 dB the curve reaches, leaving out the dips of filters with no gain, which
    /// fall without limit towards a notch's centre or past a pass filter's corner.
    reach: f32,
}

impl Graph {
    fn new() -> Self {
        Self {
            state: ResponseGraphState::new("equalizer-graph"),
            ids: Vec::new(),
            next_id: 0,
            selected: None,
            filters: Vec::new(),
            own: None,
            plotted: None,
            span: GAIN_SPANS[0],
        }
    }

    /// Brings the ids in step with `curve`, which plays now. Filters this screen did not put in
    /// place get new ids and lose the selection.
    fn follow(&mut self, curve: &Curve) {
        let own = self.own.take();
        if curve.filters() == self.filters {
            return;
        }
        self.filters = curve.filters().to_vec();
        if own.as_deref() == Some(curve.filters()) && self.ids.len() == self.filters.len() {
            return;
        }
        let first = self.next_id;
        self.next_id += self.filters.len() as u64;
        self.ids = (first..self.next_id).collect();
        self.selected = None;
    }

    /// Notes `curve` as this screen's own edit, to be applied next.
    fn edited(&mut self, curve: &Curve) {
        self.own = Some(curve.filters().to_vec());
    }

    /// Selects filter `id`, or none. Returns whether that changed anything.
    fn select(&mut self, id: Option<u64>) -> bool {
        std::mem::replace(&mut self.selected, id) != id
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|known| *known == id)
    }

    /// `curve` with `filter` after the others, given a new id and selected, unless it is full.
    fn add(&mut self, curve: &Curve, filter: Filter) -> Option<Curve> {
        let mut filters = curve.filters().to_vec();
        filters.push(filter);
        let curve = Curve::new(curve.preamp(), filters).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.ids.push(id);
        self.selected = Some(id);
        self.edited(&curve);
        Some(curve)
    }

    /// `curve` without filter `index`, its id going with it, and the selection when it was that
    /// filter.
    fn remove(&mut self, curve: &Curve, index: usize) -> Option<Curve> {
        if index >= curve.filters().len() || index >= self.ids.len() {
            return None;
        }
        let mut filters = curve.filters().to_vec();
        filters.remove(index);
        let curve = Curve::new(curve.preamp(), filters).ok()?;
        let id = self.ids.remove(index);
        if self.selected == Some(id) {
            self.selected = None;
        }
        self.edited(&curve);
        Some(curve)
    }

    /// `curve` with filter `id` made into what `change` makes of it, when the model accepts it.
    fn change(
        &mut self,
        curve: &Curve,
        id: u64,
        change: impl FnOnce(Filter) -> Result<Filter, Invalid>,
    ) -> Option<Curve> {
        let index = self.index(id)?;
        let mut curve = curve.clone();
        let filter = curve.filters_mut().get_mut(index)?;
        *filter = change(*filter).ok()?;
        self.edited(&curve);
        Some(curve)
    }

    /// The response of `curve` at `rate`, sampled again only when either changed.
    fn plotted(&mut self, curve: &Curve, rate: u32) -> &Plotted {
        let stale = self
            .plotted
            .as_ref()
            .is_none_or(|plotted| plotted.rate != rate || &plotted.curve != curve);
        if stale {
            let response = curve.response(rate);
            let points = sample(curve, &response, rate);
            let reach = reach(curve, rate, &points);
            self.plotted = Some(Plotted {
                curve: curve.clone(),
                rate,
                response,
                points,
                reach,
            });
        }
        self.plotted.as_ref().expect("sampled above")
    }
}

impl EventEmitter<Selected> for EqualizerGraph {}

impl EqualizerGraph {
    pub(super) fn new(
        equalizer: Entity<Equalizer>,
        settings: Entity<AppSettings>,
        outputs: Entity<Outputs>,
        signal: Entity<Signal>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&equalizer, |this, _, cx| this.redraw(cx))
            .detach();
        cx.observe(&outputs, |this, _, cx| this.redraw(cx)).detach();
        cx.observe(&signal, |this, _, cx| this.redraw(cx)).detach();
        cx.observe(&settings, |this, _, cx| this.redraw(cx))
            .detach();
        let mut graph = Self {
            equalizer,
            settings,
            outputs,
            signal,
            graph: Graph::new(),
            drawn: None,
        };
        graph.redraw(cx);
        graph
    }

    /// Brings the ids in step with a curve that changed, and redraws when anything the graph
    /// shows changed since it last drew.
    fn redraw(&mut self, cx: &mut Context<Self>) {
        let equalizer = self.equalizer.read(cx);
        let drawn = Drawn {
            curve: equalizer.curve().clone(),
            on: equalizer.enabled(),
            preamp: equalizer.preamp_db(),
            rate: rate(&self.outputs, cx),
            spectrum: self.signal.read(cx).levels().map(<[Level]>::to_vec),
            advanced: self.settings.read(cx).equalizer_advanced(),
        };
        if self.drawn.as_ref() == Some(&drawn) {
            return;
        }
        if self
            .drawn
            .as_ref()
            .is_none_or(|last| last.curve != drawn.curve)
        {
            self.graph.follow(&drawn.curve);
        }
        self.drawn = Some(drawn);
        cx.notify();
    }

    /// The id of filter `index` of the playing curve.
    pub(super) fn id(&self, index: usize) -> Option<u64> {
        self.graph.ids.get(index).copied()
    }

    pub(super) fn selected(&self) -> Option<u64> {
        self.graph.selected
    }

    /// Selects filter `id`, or none, and tells the screen when that changed the selection.
    pub(super) fn select(&mut self, id: Option<u64>, cx: &mut Context<Self>) {
        if self.graph.select(id) {
            cx.emit(Selected);
            cx.notify();
        }
    }

    /// Plays `curve`, an edit made on this screen.
    pub(super) fn apply(&mut self, curve: Curve, cx: &mut Context<Self>) {
        self.graph.edited(&curve);
        self.equalizer
            .update(cx, |equalizer, cx| equalizer.set_curve(curve, cx));
    }

    /// Appends `filter` to the playing curve and selects it, unless the curve is full.
    pub(super) fn add_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        let curve = self.equalizer.read(cx).curve().clone();
        if let Some(curve) = self.graph.add(&curve, filter) {
            self.apply(curve, cx);
        }
    }

    pub(super) fn remove_filter(&mut self, index: usize, cx: &mut Context<Self>) {
        let curve = self.equalizer.read(cx).curve().clone();
        if let Some(curve) = self.graph.remove(&curve, index) {
            self.apply(curve, cx);
        }
    }

    /// Carries out what was done on the graph. A drag or a wheel step glides the filter it
    /// moves, and adding and removing cross-fade like the list's own buttons. A dragged or added
    /// gain stays within the axis shown, a shelf's whole step included.
    fn graph_edit(&mut self, edit: &GraphEdit, cx: &mut Context<Self>) {
        let curve = self.equalizer.read(cx).curve().clone();
        let span = self.graph.span;
        let glided = match *edit {
            GraphEdit::Select(id) => {
                self.select(Some(id), cx);
                None
            }
            GraphEdit::Deselect => {
                self.select(None, cx);
                None
            }
            GraphEdit::Move { id, position } => self.graph.change(&curve, id, |filter| {
                // An axis the drag did not move along comes back as given, and keeps the
                // filter's own value rather than a rounded one.
                let frequency = match position.frequency_hz == filter.frequency() as f32 {
                    true => filter.frequency(),
                    false => placed_frequency(position.frequency_hz),
                };
                let lift = lift(filter.kind());
                let moved = position.gain_db != plotted_db(filter.gain() * lift);
                let gain = match filter.kind().has_gain() && moved {
                    true => placed_gain(position.gain_db / lift as f32, span),
                    false => filter.gain(),
                };
                Filter::new(
                    filter.kind(),
                    frequency,
                    gain,
                    filter.width(),
                    filter.enabled(),
                )
            }),
            GraphEdit::Width { id, steps } => {
                let factor = 2f64.powf(f64::from(steps) * WIDTH_STEP);
                self.graph.change(&curve, id, |filter| {
                    let width = match filter.width() {
                        Width::Q(q) => {
                            Width::Q((q / factor).clamp(*Q_RANGE.start(), *Q_RANGE.end()))
                        }
                        Width::Octaves(octaves) => Width::Octaves(
                            (octaves * factor).clamp(*OCTAVES_RANGE.start(), *OCTAVES_RANGE.end()),
                        ),
                    };
                    Filter::new(
                        filter.kind(),
                        filter.frequency(),
                        filter.gain(),
                        width,
                        filter.enabled(),
                    )
                })
            }
            GraphEdit::Add(position) => {
                let (kind, _, _, q) = NEW_FILTER;
                let frequency = placed_frequency(position.frequency_hz);
                let gain = placed_gain(position.gain_db, span);
                if let Ok(filter) = Filter::new(kind, frequency, gain, Width::Q(q), true) {
                    self.add_filter(filter, cx);
                }
                None
            }
            GraphEdit::Remove(id) => {
                if let Some(index) = self.graph.index(id) {
                    self.remove_filter(index, cx);
                }
                None
            }
        };
        if let Some(curve) = glided {
            self.equalizer
                .update(cx, |equalizer, cx| equalizer.glide_curve(curve, cx));
        }
    }
}

/// The live response of the filters, a node per filter on it, and the preamp as a level across
/// it. A node sits on its own filter's curve at its frequency. The graph fills the bounds the
/// screen lays it out at.
impl Render for EqualizerGraph {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let equalizer = self.equalizer.read(cx);
        let on = equalizer.enabled();
        let advanced = self.settings.read(cx).equalizer_advanced();
        let curve = equalizer.curve().clone();
        let rate = rate(&self.outputs, cx);
        let preamp = equalizer.preamp_db();
        let spectrum: Vec<_> = self
            .signal
            .read(cx)
            .levels()
            .unwrap_or_default()
            .iter()
            .map(|level| ResponsePoint {
                frequency_hz: level.frequency,
                gain_db: level.dbfs,
            })
            .collect();
        let (ids, selected) = (self.graph.ids.clone(), self.graph.selected);
        let plotted = self.graph.plotted(&curve, rate);
        let nodes: Vec<_> = curve
            .filters()
            .iter()
            .zip(ids)
            .map(|(filter, id)| {
                let gain = match filter.kind().has_gain() {
                    true => filter.gain(),
                    false => plotted.response.db(filter.frequency()),
                };
                ResponseNode {
                    id,
                    position: ResponsePoint {
                        frequency_hz: filter.frequency() as f32,
                        gain_db: plotted_db(gain * lift(filter.kind())),
                    },
                    readout_db: plotted_db(gain),
                    gain_adjustable: filter.kind().has_gain(),
                    selected: selected == Some(id),
                    enabled: on && filter.enabled(),
                }
            })
            .collect();
        let points = plotted.points.clone();
        let reach = nodes
            .iter()
            .filter(|node| node.gain_adjustable)
            .map(|node| node.position.gain_db.abs())
            .fold(plotted.reach.max(preamp.abs() as f32), f32::max);
        let span = match self.graph.state.dragging() {
            true => self.graph.span,
            false => fitted_span(reach),
        };
        self.graph.span = span;
        let (low, high) = GRAPH_FREQUENCIES;
        let axes = ResponseAxes::new(low as f32..=high as f32, -span..=span)
            .expect("the graph's axes are valid");

        let (bottom, top) = SPECTRUM_SCALE;
        ResponseGraph::new(&self.graph.state, axes)
            .size_full()
            .when(!spectrum.is_empty(), |graph| {
                graph.spectrum(spectrum, bottom..=top)
            })
            .response(points)
            .nodes(nodes)
            .reference(
                preamp as f32,
                t!("equalizer-graph-preamp", db = figure(preamp, 1)),
            )
            .bypassed(!on)
            .reshape(advanced)
            .on_edit(cx.listener(|this, edit: &GraphEdit, _, cx| this.graph_edit(edit, cx)))
    }
}

/// How tall the graph stands in `room`, which the screen lays its view out at.
pub(super) fn height(room: Room, cx: &App) -> Pixels {
    let height = match room.fits(Room::Wide) {
        true => GRAPH_TALL,
        false => GRAPH_SHORT,
    };
    cx.theme().metrics.cover * height
}

/// The rate the output plays at, or the idle one while no stream is open.
fn rate(outputs: &Entity<Outputs>, cx: &App) -> u32 {
    outputs
        .read(cx)
        .current()
        .map_or(IDLE_RATE, |device| device.rate)
}

/// The response sampled on a log grid up to the axis's high end or the rate's Nyquist frequency,
/// whichever is lower, plus each enabled filter's centre in that band.
fn sample(curve: &Curve, response: &Response, rate: u32) -> Vec<ResponsePoint> {
    let (low, high) = GRAPH_FREQUENCIES;
    let high = high.min(f64::from(rate) / 2.);
    let ratio = high / low;
    let grid = (0..POINTS).map(|point| low * ratio.powf(point as f64 / (POINTS - 1) as f64));
    let centres = curve
        .filters()
        .iter()
        .filter(|filter| filter.enabled())
        .map(|filter| filter.frequency())
        .filter(|frequency| (low..=high).contains(frequency));
    let mut frequencies: Vec<f64> = grid.chain(centres).collect();
    frequencies.sort_by(f64::total_cmp);
    frequencies.dedup();
    frequencies
        .into_iter()
        .map(|frequency| ResponsePoint {
            frequency_hz: frequency as f32,
            gain_db: plotted_db(response.db(frequency)),
        })
        .collect()
}

/// A dragged frequency to a few significant figures, in range.
fn placed_frequency(frequency_hz: f32) -> f64 {
    let frequency = f64::from(frequency_hz);
    let scale = 10f64.powi(FREQUENCY_FIGURES - 1 - frequency.log10().floor() as i32);
    ((frequency * scale).round() / scale).clamp(*FREQUENCY_RANGE.start(), *FREQUENCY_RANGE.end())
}

/// How far from 0 dB the response sampled at `points` reaches. That is its highest boost, or the
/// deepest cut of its peaks and shelves alone, read from the same curve without the gainless
/// filters.
fn reach(curve: &Curve, rate: u32, points: &[ResponsePoint]) -> f32 {
    let filters = curve
        .filters()
        .iter()
        .filter(|filter| filter.kind().has_gain())
        .copied()
        .collect();
    let gains = Curve::new(curve.preamp(), filters)
        .ok()
        .map(|curve| curve.response(rate));
    points
        .iter()
        .map(|point| {
            let cut = gains
                .as_ref()
                .map_or(0., |gains| gains.db(f64::from(point.frequency_hz)).abs());
            point.gain_db.max(cut as f32)
        })
        .fold(0., f32::max)
}

/// The narrowest of `GAIN_SPANS` that `reach` fits in, or the widest when none does.
fn fitted_span(reach: f32) -> f32 {
    GAIN_SPANS
        .into_iter()
        .find(|span| reach <= span + SPAN_SLACK)
        .unwrap_or(GAIN_SPANS[GAIN_SPANS.len() - 1])
}

/// A dragged gain to a tenth of a decibel, within `span` either side of 0 dB and in range.
fn placed_gain(gain_db: f32, span: f32) -> f64 {
    let span = f64::from(span);
    ((f64::from(gain_db) * 10.).round() / 10.)
        .clamp(-span, span)
        .clamp(*GAIN_RANGE.start(), *GAIN_RANGE.end())
}

/// As low as a curve or a node is handed to the graph, so a notch's centre at minus infinity
/// stays finite and draws at the floor of whichever axis is shown.
fn gain_floor() -> f32 {
    *GAIN_RANGE.start() as f32
}

/// The share of its gain a filter's curve reaches at its frequency, where its node sits: half
/// for a shelf, whose centre is halfway up its step.
fn lift(kind: Kind) -> f64 {
    match kind {
        Kind::LowShelf | Kind::HighShelf => 0.5,
        _ => 1.,
    }
}

fn plotted_db(db: f64) -> f32 {
    (db as f32).max(gain_floor())
}
