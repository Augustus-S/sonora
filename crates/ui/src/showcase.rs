use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Hsla, MouseButton, Pixels,
    SharedString, SpringConfig, Stateful, StyleRefinement, Window, div, linear_color_stop,
    linear_gradient, px, relative,
};

use crate::metrics::Text;
use crate::motion::{css_ease_out, mix, paced};
use crate::spring::{Beat, Spring};
use crate::theme::ActiveTheme as _;
use crate::veil::{VeilEdge, effects, veil};

/// How tall a tile stands, in covers.
const TALL: f32 = 1.3;
/// How much larger the art grows once the tile is hovered.
const LIFT: f32 = 0.035;
/// The spring the art grows and shrinks back on.
const LIFT_SPRING: SpringConfig = SpringConfig::new(380., 28., 1.);
/// How small the whole tile goes while it is pressed, and the spring it gets there on.
const PRESSED: f32 = 0.985;
const PRESS_SPRING: SpringConfig = SpringConfig::new(600., 42., 1.);
/// The glow behind a hovered tile: how strong it gets, how soft it is, how far past the tile it
/// spreads, and how long it takes to come up.
const GLOW_ALPHA: f32 = 0.35;
const GLOW_BLUR: Pixels = px(32.);
const GLOW_SPREAD: Pixels = px(10.);
const GLOW_FADE: Duration = Duration::from_millis(180);
/// How strongly the foreground shows through the border of a hovered tile.
const BORDER_LIT: f32 = 0.2;
/// The shadow under the title: the background's strength at the bottom edge, and how far up
/// the tile it fades out.
const SHADE: f32 = 0.85;
const SHADE_REACH: f32 = 0.55;
/// The blur under the title: how far up the tile it rises and how hard it blurs at the bottom.
const VEIL_REACH: f32 = 0.4;
const VEIL_BLUR: Pixels = px(12.);

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type Art = Box<dyn FnOnce(Hover, &mut Window, &mut App) -> AnyElement + 'static>;

/// What a tile's art is told about the hover on every frame it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hover {
    /// How far the lift has gone, from 0 at rest to 1 hovered. It overshoots a little on the
    /// way, as springs do.
    pub lift: f32,
    /// The seconds the hover clock has run. It only runs while the pointer is over the tile
    /// and motion is allowed, and it holds where it stopped once the pointer leaves, so art
    /// driven by it freezes rather than snapping back.
    pub clock: f32,
    /// Whether the pointer is over the tile.
    pub hovered: bool,
    /// How much larger than the tile the art is laid out. It is 1 at rest and while the lift
    /// moves, when the compositor scales the art instead, and the full lift once it has settled.
    /// Art that sizes its parts in pixels multiplies them by this, so the settled frame matches
    /// the moving one.
    pub scale: f32,
}

impl Default for Hover {
    fn default() -> Self {
        Self {
            lift: 0.,
            clock: 0.,
            hovered: false,
            scale: 1.,
        }
    }
}

/// A large tile that is an illustration and a title and nothing else, for a grid of places to
/// go. The art is static until the tile is hovered, then it grows a little on a spring, a soft
/// glow of the tile's colour comes up behind it and its art's clock starts. The title sits over
/// a shadow and a rising blur, so it reads over any art.
///
/// The tile asks for frames only while it is hovered or something on it still moves. Under
/// reduced motion the art never moves and nothing scales, and only the glow fades.
#[derive(IntoElement)]
pub struct Showcase {
    base: Stateful<Div>,
    id: ElementId,
    title: SharedString,
    glow: Option<Hsla>,
    art: Option<Art>,
    on_click: Option<Click>,
    pinned: Option<f32>,
}

/// What a tile keeps between frames: the pointer, the springs and the clocks.
struct Showing {
    hovered: bool,
    pressed: bool,
    lift: Spring,
    press: Spring,
    /// How far the glow has faded in, from 0 to 1 before its easing.
    glow: f32,
    clock: f32,
    beat: Beat,
}

/// One frame of a tile's motion, read out of `Showing`.
struct Frame {
    hover: Hover,
    /// The glow and the lit border, eased, from 0 to 1.
    glow: f32,
    /// How far the press has gone, from 0 to 1.
    press: f32,
    /// Whether the lift spring is still on its way, so the art scales as a layer.
    lifting: bool,
    /// Whether anything still moves, so the tile wants another frame.
    moving: bool,
}

impl Showing {
    fn new() -> Self {
        Self {
            hovered: false,
            pressed: false,
            lift: Spring::new(LIFT_SPRING),
            press: Spring::new(PRESS_SPRING),
            glow: 0.,
            clock: 0.,
            beat: Beat::default(),
        }
    }

    /// Steps every spring and clock by the time since the last frame and reports where they are.
    fn advance(&mut self, still: bool) -> Frame {
        let elapsed = self.beat.tick();
        self.lift.aim(f32::from(u8::from(self.hovered && !still)));
        self.press.aim(f32::from(u8::from(self.pressed && !still)));
        for spring in [&mut self.lift, &mut self.press] {
            match still {
                true => spring.finish(),
                false => spring.step(elapsed),
            }
        }
        let goal = f32::from(u8::from(self.hovered));
        let fade = paced(GLOW_FADE).as_secs_f32().max(f32::EPSILON);
        let step = elapsed.as_secs_f32() / fade;
        self.glow = match goal > self.glow {
            true => (self.glow + step).min(goal),
            false => (self.glow - step).max(goal),
        };
        let running = self.hovered && !still;
        if running {
            self.clock += elapsed.as_secs_f32();
        }

        let lifting = !self.lift.settled();
        let moving = lifting || !self.press.settled() || self.glow != goal || running;
        if !moving {
            self.beat.stop();
        }
        Frame {
            hover: Hover {
                lift: self.lift.at(),
                clock: self.clock,
                hovered: self.hovered,
                scale: 1.,
            },
            glow: css_ease_out(self.glow),
            press: self.press.at(),
            lifting,
            moving,
        }
    }
}

impl Showcase {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            title: title.into(),
            glow: None,
            art: None,
            on_click: None,
            pinned: None,
        }
    }

    /// The colour of the glow that comes up behind the tile while it is hovered.
    pub fn glow(mut self, color: Hsla) -> Self {
        self.glow = Some(color);
        self
    }

    /// Draws the illustration, which fills the tile edge to edge. The builder runs on every
    /// frame the tile draws and is told the hover, so the art can move with its clock.
    pub fn art(
        mut self,
        art: impl FnOnce(Hover, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.art = Some(Box::new(art));
        self
    }

    /// Holds the tile hovered and at rest, with the art's clock at `clock` seconds, for a still
    /// of what the motion looks like at that moment.
    pub fn pinned(mut self, clock: f32) -> Self {
        self.pinned = Some(clock);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for Showcase {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Showcase {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            id,
            title,
            glow,
            art,
            on_click,
            pinned,
        } = self;
        let theme = *cx.theme();
        let radius = theme.radius;
        let height = theme.metrics.cover * TALL;

        let showing = window.use_keyed_state((id, "showcase"), cx, |_, _| Showing::new());
        let frame = match pinned {
            Some(clock) => Frame {
                hover: Hover {
                    lift: 1.,
                    clock,
                    hovered: true,
                    scale: 1.,
                },
                glow: 1.,
                press: 0.,
                lifting: false,
                moving: false,
            },
            None => {
                let still = cx.reduce_motion();
                showing.update(cx, |showing, _| showing.advance(still))
            }
        };
        if frame.moving {
            window.request_animation_frame();
        }

        // While the lift moves the art scales as one layer. At rest it is laid out at its size,
        // so a hovered tile's art sits on whole pixels instead of being resampled.
        let grown = frame.hover.lift >= 1. && !frame.lifting;
        let hover = Hover {
            scale: match grown {
                true => 1. + LIFT,
                false => 1.,
            },
            ..frame.hover
        };
        let art = art.map(|art| art(hover, window, cx));
        let art = div()
            .absolute()
            .map(|layer| match (frame.lifting, grown) {
                (true, _) => layer.inset_0().layer_scale(1. + LIFT * frame.hover.lift),
                (false, true) => layer
                    .left(relative(-LIFT / 2.))
                    .top(relative(-LIFT / 2.))
                    .w(relative(1. + LIFT))
                    .h(relative(1. + LIFT)),
                (false, false) => layer.inset_0(),
            })
            .children(art);

        let lit = glow.filter(|_| frame.glow > 0. && effects()).map(|color| {
            div()
                .absolute()
                .top(-GLOW_SPREAD)
                .left(-GLOW_SPREAD)
                .right(-GLOW_SPREAD)
                .bottom(-GLOW_SPREAD)
                .rounded(radius + GLOW_SPREAD)
                .bg(color.opacity(color.a * GLOW_ALPHA * frame.glow))
                .blur(GLOW_BLUR)
        });
        let shade = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(relative(SHADE_REACH))
            .bg(linear_gradient(
                180.,
                linear_color_stop(theme.background.opacity(0.), 0.),
                linear_color_stop(theme.background.opacity(SHADE), 1.),
            ));
        let band = height * VEIL_REACH;
        let haze = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(band)
            .child(veil(
                VeilEdge::Bottom,
                band,
                VEIL_BLUR,
                theme.background.opacity(0.),
                None,
                window,
            ));
        let caption = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .p_5()
            .truncate()
            .text_size(theme.text(Text::Large))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.foreground)
            .child(title);
        let edge = mix(
            theme.border,
            theme.foreground.opacity(BORDER_LIT),
            frame.glow,
        );

        let pressing = showing.clone();
        let releasing = showing.clone();
        let leaving = showing.clone();
        let overrides = std::mem::take(base.style());
        let mut tile = base
            .relative()
            .w_full()
            .h(height)
            .cursor_pointer()
            .when(frame.press > 0., |tile| {
                tile.layer_scale(1. - (1. - PRESSED) * frame.press)
            })
            .on_hover(move |hovered, _, cx| {
                leaving.update(cx, |showing, cx| {
                    showing.hovered = *hovered;
                    showing.pressed &= *hovered;
                    cx.notify();
                });
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                pressing.update(cx, |showing, cx| {
                    showing.pressed = true;
                    cx.notify();
                });
            })
            .on_mouse_up(MouseButton::Left, {
                let releasing = releasing.clone();
                move |_, _, cx| {
                    releasing.update(cx, |showing, cx| {
                        showing.pressed = false;
                        cx.notify();
                    });
                }
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                releasing.update(cx, |showing, cx| {
                    showing.pressed = false;
                    cx.notify();
                });
            })
            .when_some(on_click, |tile, handler| {
                tile.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .children(lit)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius)
                    .overflow_hidden()
                    .bg(theme.secondary)
                    .child(art)
                    .child(shade)
                    .child(haze)
                    .child(caption),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius)
                    .border_1()
                    .border_color(edge),
            );

        tile.style().refine(&overrides);
        tile
    }
}
