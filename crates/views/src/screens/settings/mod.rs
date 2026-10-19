use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::chrome::OutputPicker;
use crate::shared::confirm::{Confirm, Kind};
use crate::shared::device_code::device_code;
use crate::shared::effects;
use crate::shared::equalizer::sound_picker;
use crate::shared::local;
use crate::shared::popups::{AccountPicker, CookiePrompt, SearchPopup, matches_query};
use crate::shared::text;
use crate::shared::veil::{Edge, veil};
use gpui::{
    AnyElement, App, ClickEvent, Context, DragMoveEvent, Entity, FocusHandle, FontWeight,
    MouseButton, MouseUpEvent, Pixels, Point, Render, SharedString, Task, Window, div, px,
    relative,
};
use gpui::{ScrollHandle, prelude::*, svg};
use i18n::{Language, t};
use music::drm::Origin;
use music::scrobble::{Link, Secret};
use music::{
    AccountChoice, Codec, Encoding, Offer, Quality, SignIn, SignInPrompt, Transcode, WritingSystem,
};
use router::{Destination, NavEntry, Screen, SettingsTab, navigate};
use state::{
    AppSettings, CdmState, DiscordName, Drm, Equalizer, Failure, FullscreenControlsAutohide, Io,
    LYRICS_SOURCES, Outputs, Playback, SYSTEM_FONT, Scan, ScrobbleState, Scrobbling, Session,
    SessionState, Sleep, Sonora,
};
use ui::{
    ActiveTheme as _, Deck, Ghost, LEADING, Scrollbar, Scroller, drop_gap, drop_marker, eyebrow,
    snapped, tabular,
};
use ui::{
    Avatar, Button, Dismiss, InfoCard, Initials, Input, Look, MAX_FONT, MAX_LYRICS_SCALE,
    MAX_TRANSPARENCY, MIN_FONT, MIN_LYRICS_SCALE, MenuItem, Modal, Pace, Picker, Popovers, Radio,
    Rounding, Saver, Scrubber, ScrubberState, Separator, Skeleton, Stillness, Switch, TabBar, Text,
    Theme, ThemeKind, Vacancy, VisualizerStyle,
};

mod about;
mod accounts;
mod appearance;
mod discord;
mod general;
mod lyrics;
mod now_playing;
mod playback;
mod rows;
mod scrobbling;
mod sound;

use accounts::*;
use appearance::*;
use playback::*;
use rows::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How wide the settings column, the search field and the category bar grow at most.
const WIDTH: Pixels = px(640.);
/// How much a query found in a setting's title counts over one found in its detail.
const TITLE_WEIGHT: u32 = 2;
/// How far the rows are blurred where they pass under the header. One blur pass serves every
/// strip, so this is also the widest the haze gets. The renderer cuts a kernel off at 24 taps
/// on a quarter-resolution frame, so past about 32px a wider radius only flattens the curve.
const HEADER_BLUR: Pixels = px(1.);
/// How far past the header the rows keep dissolving, so the handoff under the blur has
/// no hard edge where sharp content emerges from the haze.
const HEADER_FADE_TAIL: Pixels = px(48.);
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const SOURCE_URL: &str = "https://github.com/sonorahq/sonora";

const THEMES: &str = "themes";
const PACKS: &str = "packs";
const CORNERS: &str = "corners";
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
const WINDOW_ROUNDING: &str = "window-rounding";
const FULLSCREEN_CONTROLS_AUTOHIDE: &str = "fullscreen-controls-autohide";
const VISUALIZER_STYLE: &str = "visualizer-style";
const LANGUAGES: &str = "languages";
const TYPEFACES: &str = "typefaces";
const TYPEFACE_LIMIT: usize = 200;
const TYPEFACE_LEAD: usize = 2;
// faces previewed before a measurement
const TYPEFACE_GUESS: usize = 24;
// faces loaded per frame
const TYPEFACE_BATCH: usize = 3;
const STARTUP: &str = "startup";
const ENTRIES: &str = "entries";
const DISCORD_NAME: &str = "discord-name";
const DISCORD_BUTTONS: &str = "discord-buttons";
const MOTION: &str = "motion";
const PACE: &str = "pace";
const SAVER: &str = "saver";
const SLEEP: &str = "sleep";
const STREAM_QUALITIES: &str = "stream-qualities";
const STREAM_FORMATS: &str = "stream-formats";
const STREAM_BITRATES: &str = "stream-bitrates";
const EQUALIZER_PRESETS: &str = "equalizer-presets";
const SLEEP_MAX_MINUTES: u64 = 120;
const SLEEP_MAGNETS: [u64; 4] = [15, 30, 45, 60];
const SLEEP_MAGNET_WEIGHT: usize = 4;
const SLEEP_LAST: usize =
    SLEEP_MAX_MINUTES as usize + SLEEP_MAGNETS.len() * (SLEEP_MAGNET_WEIGHT - 1) + 1;

/// How tall one line of `step` text stands. The deck needs every row's height before it
/// builds anything, so wherever a height is summed from text, that text pins itself to this
/// leading and truncates to one line.
fn line(theme: &Theme, step: Text) -> Pixels {
    px((theme.text(step) / px(1.) * LEADING).round())
}

/// How tall a standard row draws: its padding over one title line and one detail line, the
/// layout `SettingsView::row` builds.
fn standard_height(theme: &Theme) -> Pixels {
    SECTION_GAP + line(theme, Text::Body) + ROW_GAP + line(theme, Text::Small) + SECTION_GAP
}

/// The deck draws only the rows in view, so every row's height is fixed by construction:
/// standard rows and titles keep one height each, separators are one pixel, and the three
/// composite rows sum theirs from fixed parts in `SettingsView::slot_height`.
const SEPARATOR_HEIGHT: Pixels = px(1.);
/// Slack kept under the accounts block, so a border the formula counts differently still
/// cannot clip the last card.
const ACCOUNTS_SLACK: Pixels = px(2.);
/// `gap_1`, the breathing room inside a summed row.
const ROW_GAP: Pixels = px(4.);
/// `gap_2`, the step between stacked buttons and band labels.
const BLOCK_GAP: Pixels = px(8.);
/// `gap_3` and `py_3`, the step between blocks of a composite row.
const SECTION_GAP: Pixels = px(12.);
/// `gap_0p5`, the step between a provider's name and its status.
const HALF_GAP: Pixels = px(2.);
/// The icon beside a provider's sign-in error.
const ERROR_ICON: Pixels = px(14.);
/// The arrow at the end of a card that signs in when it is clicked.
const ARROW: Pixels = px(14.);
/// How many buttons tall the sign-in choice stands, however few it lists.
const CHOICES: f32 = 3.;

/// One row of the settings page, described without building anything. The deck scores and
/// measures slots and only builds the ones in view, so typing a search or scrolling never
/// constructs the rows it does not show.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Title(&'static str),
    Sep,
    Startup,
    Entries,
    Language,
    Tray,
    TrayIcon,
    Accounts,
    LocalFolder,
    Theme,
    Adaptive,
    Ambient,
    AmbientMotion,
    Visualizer,
    VisualizerAbsolute,
    Icons,
    Opacity,
    WindowBlur,
    Blur,
    Corners,
    FullscreenControlsAutohide,
    OsFullscreen,
    PanelLyricsSize,
    FullscreenLyricsSize,
    BlurLyrics,
    Font,
    Typeface,
    Motion,
    Pace,
    Saver,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    ServerSideDecorations,
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    Decorations,
    #[cfg(not(target_os = "macos"))]
    Side,
    #[cfg(not(target_os = "macos"))]
    TrafficLights,
    #[cfg(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos"
    ))]
    WindowRounding,
    AdaptiveMenu,
    Normalisation,
    Gapless,
    StreamQuality,
    StreamFormat,
    StreamBitrate,
    Sleep,
    StayAwake,
    Widevine,
    OutputDevice,
    OutputFailure,
    SampleRate,
    Buffer,
    Equalizer,
    EqualizerPreset,
    LyricsProviders,
    LyricsPriority,
    Karaoke,
    Romanized,
    LyricsForLocal,
    ArtworkForLocal,
    Discord,
    DiscordName,
    DiscordShowPaused,
    DiscordBadge,
    DiscordAnonymous,
    DiscordButtons,
    Scrobble(usize),
    Version,
    Updates,
    Log,
    License,
    Source,
}

/// One setting on the page: the row that draws it, and the title and detail a search is
/// matched against.
struct Setting {
    title: SharedString,
    detail: SharedString,
    element: AnyElement,
}

/// One field of the scrobbling link dialog: which hint it carries, what it starts with, and
/// whether it holds a secret and so is drawn as dots.
struct Field {
    hint: &'static str,
    value: String,
    masked: bool,
}

/// What the guest card answers to, where a provider card answers to its slug.
const GUEST: &str = "guest";

/// What a whole account card does when it is clicked.
type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

struct Account {
    slug: &'static str,
    name: &'static str,
    options: Vec<SignIn>,
    web_sign_in: bool,
    stored: bool,
    active: bool,
    cancel: bool,
    error: Option<Failure>,
}

/// The guest entry the accounts block draws under the providers. It is a card of the app's
/// own, not a provider: it stands for the anonymous session `slug` offers, so signing out of
/// it signs that provider out.
#[derive(Clone, Copy)]
struct Guest {
    slug: &'static str,
    stored: bool,
    active: bool,
}

/// Which sign-in methods a provider card lists. Anonymous never appears there, because guest
/// mode has a card of its own, and a stored account is asked for nothing.
fn offered(method: &SignIn, stored: bool) -> bool {
    match method {
        SignIn::Default | SignIn::Secret | SignIn::Credentials { .. } => !stored,
        SignIn::Anonymous | SignIn::Path(_) => false,
    }
}

#[derive(Clone, Copy)]
struct Member {
    login: &'static str,
    avatar: &'static str,
    profile: &'static str,
    role: Role,
}

#[derive(Clone, Copy)]
enum Role {
    LeadMaintainer,
    Maintainer,
    Contributor,
}

impl Role {
    fn label(self) -> SharedString {
        match self {
            Self::LeadMaintainer => t!("settings-role-lead-maintainer"),
            Self::Maintainer => t!("settings-role-maintainer"),
            Self::Contributor => t!("settings-role-contributor"),
        }
    }
}

macro_rules! member {
    ($login:literal, $role:expr) => {
        Member {
            login: $login,
            avatar: concat!("https://github.com/", $login, ".png"),
            profile: concat!("https://github.com/", $login),
            role: $role,
        }
    };
}

const MEMBERS: [Member; 5] = [
    member!("nolight132", Role::LeadMaintainer),
    member!("zxsleebu", Role::Maintainer),
    member!("fx-got", Role::Maintainer),
    member!("Makakashan", Role::Contributor),
    member!("imizgun", Role::Contributor),
];

/// A lyrics source being dragged within the priority list. It draws as a chip under the cursor
/// and changes the saved order only when dropped.
#[derive(Clone)]
struct DraggedSource {
    provider: String,
    label: SharedString,
    /// Where the cursor grabbed the entry, so the chip starts under it.
    position: Point<Pixels>,
}

pub struct SettingsView {
    session: Entity<Session>,
    playback: Entity<Playback>,
    drm: Entity<Drm>,
    equalizer: Entity<Equalizer>,
    settings: Entity<AppSettings>,
    outputs: Entity<Outputs>,
    output_picker: Entity<OutputPicker>,
    tab: SettingsTab,
    search: Entity<Input>,
    /// The search field's text, trimmed. Empty means the page shows one category.
    query: String,
    /// Where a lyrics source dragged over the priority list would land, as a gap between the
    /// enabled sources. `None` while the drop would leave the order as it is.
    lyrics_drop_gap: Option<usize>,
    /// How tall the header floating over the page measured last, so the rows start beneath it.
    header_height: Pixels,
    /// Whether the header has measured itself at least once. Until then the height is a
    /// zero stand-in, and the page stays hidden rather than flashing unpadded for a frame.
    header_measured: bool,
    /// How wide the rows measured last. The Widevine row wraps its explanation to this width
    /// and needs it before the deck is built, so the first frame falls back to the widest the
    /// column can be.
    column: Option<Pixels>,
    scrollbar: Entity<Scrollbar>,
    opacity: ScrubberState,
    sleep: ScrubberState,
    pending_sleep: Option<Option<Sleep>>,
    popovers: Popovers,
    server: Entity<Input>,
    username: Entity<Input>,
    password: Entity<Input>,
    credentials_for: Option<&'static str>,
    secret: Entity<Input>,
    manual_secret: Option<(&'static str, &'static str)>,
    scrobbling: Entity<Scrobbling>,
    scrobble_first: Entity<Input>,
    scrobble_second: Entity<Input>,
    /// The service whose link dialog is open, by slug.
    scrobble_prompt: Option<&'static str>,
    /// The provider whose sign-in choice is up, by slug and name.
    sign_in_for: Option<(&'static str, &'static str)>,
    /// Holds the key focus while a dialog is up, so escape reaches the page and closes it,
    /// and is where the language and typeface pickers put it back when they close.
    focus: FocusHandle,
    /// Whether the focus has already been taken for the dialog that is up.
    grabbed: bool,
    /// Whether the choice the dialog offered has been taken. The dialog stays up until the
    /// sign-in it started is over, so the veil never blinks away between it and the prompt
    /// that follows.
    sign_in_running: bool,
    /// The card the user just switched to, by slug or `guest`. Its radio fills while the
    /// session tears the old provider down and brings the new one up, which reports nothing
    /// active in between.
    chosen: Option<&'static str>,
    languages: SearchPopup,
    typefaces: SearchPopup,
    typeface_faced: RefCell<HashSet<SharedString>>,
    installed: Option<Vec<SharedString>>,
    loading_fonts: bool,
    font_task: Option<Task<()>>,
}

impl SettingsView {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        let scrobbling = Sonora::global(cx).scrobbling.clone();
        let drm = Sonora::global(cx).drm.clone();
        let equalizer = Sonora::global(cx).equalizer.clone();
        let outputs = Sonora::global(cx).outputs.clone();
        cx.observe(&drm, |_, _, cx| cx.notify()).detach();
        cx.observe(&equalizer, |_, _, cx| cx.notify()).detach();
        cx.observe(&outputs, |_, _, cx| cx.notify()).detach();
        cx.observe(&session, |_, _, cx| cx.notify()).detach();
        cx.observe(&scrobbling, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&Scan::global(cx), |_, _, cx| cx.notify())
            .detach();
        let me = cx.entity_id();
        let focus = cx.focus_handle();
        let languages = SearchPopup::new("settings-language-search", me, focus.clone(), cx);
        cx.observe(&languages.input(), |this, _, cx| {
            this.languages.changed(cx);
            cx.notify();
        })
        .detach();
        let typefaces = SearchPopup::new("settings-typeface-search", me, focus.clone(), cx);
        cx.observe(&typefaces.input(), |this, _, cx| {
            this.typefaces.changed(cx);
            cx.notify();
        })
        .detach();
        let search = cx.new(|cx| {
            Input::new("settings-search", cx)
                .icon("icons/search.svg")
                .clearable()
                // the field floats over the rows the page scrolls beneath it, so it frosts
                // them the way the category bar under it does
                .blurred()
        });
        cx.observe(&search, |this, input, cx| {
            let query = input.read(cx).text().trim().to_owned();
            // A search swaps the rows for hits from every category, so wherever the page was
            // scrolled to answers for nothing once one starts.
            if this.query.is_empty() && !query.is_empty() {
                this.scrollbar.update(cx, |bar, _| bar.place(Pixels::ZERO));
            }
            this.query = query;
            cx.notify();
        })
        .detach();

        let popovers = Popovers::default();
        let output_picker = cx.new(|cx| OutputPicker::new(outputs.clone(), popovers.clone(), cx));

        Self {
            session,
            playback,
            drm,
            equalizer,
            settings,
            outputs,
            output_picker,
            tab: SettingsTab::General,
            search,
            query: String::new(),
            lyrics_drop_gap: None,
            header_height: Pixels::ZERO,
            header_measured: false,
            column: None,
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
            opacity: ScrubberState::new("opacity"),
            sleep: ScrubberState::new("sleep"),
            pending_sleep: None,
            popovers,
            server: cx.new(|cx| Input::new("login-server-hint", cx)),
            username: cx.new(|cx| Input::new("login-username-hint", cx)),
            password: cx.new(|cx| Input::new("login-password-hint", cx).masked()),
            credentials_for: None,
            secret: cx.new(|cx| Input::new("login-cookie-hint", cx)),
            manual_secret: None,
            sign_in_for: None,
            focus,
            grabbed: false,
            sign_in_running: false,
            chosen: None,
            scrobbling,
            scrobble_first: cx.new(|cx| Input::new("settings-scrobble-key", cx)),
            scrobble_second: cx.new(|cx| Input::new("settings-scrobble-secret", cx)),
            scrobble_prompt: None,
            languages,
            typefaces,
            typeface_faced: RefCell::new(HashSet::new()),
            installed: None,
            loading_fonts: false,
            font_task: None,
        }
    }

    /// Shows one category and ends any search, so a route always lands on a plain page.
    pub(crate) fn select(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.popovers.close();
        // a device plugged in since the list was read is only found by asking again
        if tab == SettingsTab::Playback {
            self.outputs.update(cx, |outputs, cx| outputs.refresh(cx));
        }
        if !self.search.read(cx).text().is_empty() {
            self.search.update(cx, |input, cx| input.set_text("", cx));
        }
        cx.notify();
    }

    fn searching(&self) -> bool {
        !self.query.is_empty()
    }

    /// Takes the header's measured height. The header floats over the page, so the rows pad
    /// themselves by this much and scroll beneath it, and the scrollbar starts below it so
    /// the bar never slides under the blur.
    fn set_header_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        if self.header_height == height {
            return;
        }
        self.header_height = height;
        self.header_measured = true;
        self.scrollbar.update(cx, |bar, cx| {
            bar.set_track_top(height, cx);
        });
        cx.notify();
    }

    /// Takes the rows' measured width, which only the Widevine row's height depends on.
    fn set_column(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.column == Some(width) {
            return;
        }
        self.column = Some(width);
        cx.notify();
    }

    /// The rows of the page as a deck: the current category's, or, while a search is on,
    /// every category's that answers it, best match first and without the group titles. Only
    /// the rows in view are ever built.
    fn panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let bare = match self.searching() {
            false => self.tab_slots(self.tab, cx),
            true => self.found(cx),
        };
        if bare.is_empty() {
            return Vacancy::new(t!("search-no-matches"))
                .icon("icons/search.svg")
                .py_6()
                .into_any_element();
        }

        let mut slots = Vec::with_capacity(bare.len() * 2);
        let mut parted = false;
        for slot in bare {
            let titled = matches!(slot, Slot::Title(_));
            if parted && !titled {
                slots.push(Slot::Sep);
            }
            parted = !titled;
            slots.push(slot);
        }

        let heights = slots
            .iter()
            .map(|slot| self.slot_height(*slot, window, cx))
            .collect::<Vec<_>>();
        Deck::new("settings-deck")
            .rows(heights)
            .draw(cx.processor(
                move |this: &mut Self, index: usize, _, cx| match slots.get(index) {
                    Some(slot) => this.slot_element(*slot, cx),
                    None => div().into_any_element(),
                },
            ))
            .into_any_element()
    }

    /// Every setting of every category that answers the query, best match first. Ties keep
    /// the order of the categories. Only titles and details are read; no row is built.
    fn found(&self, cx: &App) -> Vec<Slot> {
        let mut hits: Vec<(u32, Slot)> = SettingsTab::ALL
            .into_iter()
            .flat_map(|tab| self.tab_slots(tab, cx))
            .filter_map(|slot| {
                self.slot_score(slot, &self.query, cx)
                    .map(|score| (score, slot))
            })
            .collect();
        hits.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        hits.into_iter().map(|(_, slot)| slot).collect()
    }

    /// How well the slot answers `query`, or `None` when it does not. The query may span
    /// the title and the detail, and a hit in the title alone counts on top.
    fn slot_score(&self, slot: Slot, query: &str, cx: &App) -> Option<u32> {
        let (title, detail) = self.slot_meta(slot, cx)?;
        let both = format!("{} {}", title, detail);
        let whole = text::fuzzy(&both, query)?;
        let titled = text::fuzzy(&title, query).unwrap_or(0) * TITLE_WEIGHT;
        Some(whole + titled)
    }

    fn tab_slots(&self, tab: SettingsTab, cx: &App) -> Vec<Slot> {
        match tab {
            SettingsTab::General => vec![
                Slot::Startup,
                Slot::Entries,
                Slot::Language,
                Slot::Title("settings-group-window"),
                Slot::Tray,
                Slot::TrayIcon,
                Slot::Title("settings-group-accounts"),
                Slot::Accounts,
                Slot::Title("settings-group-library"),
                Slot::LocalFolder,
            ],
            SettingsTab::Appearance => vec![
                Slot::Title("settings-tab-general"),
                Slot::Theme,
                Slot::Adaptive,
                Slot::Icons,
                Slot::Opacity,
            ]
            .into_iter()
            .chain(ui::WINDOW_BLUR.then_some(Slot::WindowBlur))
            .chain([
                Slot::Blur,
                Slot::Corners,
                Slot::Title("settings-group-fullscreen"),
                Slot::Ambient,
            ])
            .chain(
                self.settings
                    .read(cx)
                    .ambient()
                    .then_some(Slot::AmbientMotion),
            )
            .chain([Slot::Visualizer])
            .chain(
                self.settings
                    .read(cx)
                    .visualizer_style()
                    .shown()
                    .then_some(Slot::VisualizerAbsolute),
            )
            .chain([
                Slot::FullscreenControlsAutohide,
                Slot::OsFullscreen,
                Slot::Title("settings-group-lyrics"),
                Slot::PanelLyricsSize,
                Slot::FullscreenLyricsSize,
                Slot::BlurLyrics,
                Slot::Title("settings-group-text"),
                Slot::Font,
                Slot::Typeface,
                Slot::Title("settings-group-motion"),
                Slot::Motion,
                Slot::Pace,
                Slot::Saver,
            ])
            .chain(self.decoration_slots(cx))
            .chain([Slot::Title("settings-advanced"), Slot::AdaptiveMenu])
            .collect(),
            SettingsTab::Playback => {
                let mut slots = vec![
                    Slot::Title("settings-tab-general"),
                    Slot::Normalisation,
                    Slot::Gapless,
                    Slot::Sleep,
                    Slot::StayAwake,
                ];
                if self.drm.read(cx).shown(cx) {
                    slots.push(Slot::Widevine);
                }
                let capabilities = self.session.read(cx).capabilities();
                let qualities = !capabilities.qualities.is_empty();
                if qualities || capabilities.transcode {
                    slots.push(Slot::Title("settings-group-streaming"));
                }
                if qualities {
                    slots.push(Slot::StreamQuality);
                }
                if capabilities.transcode {
                    slots.extend([Slot::StreamFormat, Slot::StreamBitrate]);
                }
                slots.extend([Slot::Title("settings-group-devices"), Slot::OutputDevice]);
                let outputs = self.outputs.read(cx);
                if outputs.failure().is_some() {
                    slots.push(Slot::OutputFailure);
                }
                // with nothing open there is no rate or buffer to show
                if outputs.failure().is_none() {
                    slots.extend([Slot::SampleRate, Slot::Buffer]);
                }
                slots.extend([Slot::Title("settings-group-equalizer"), Slot::Equalizer]);
                if self.equalizer.read(cx).enabled() {
                    slots.push(Slot::EqualizerPreset);
                }
                slots.extend([Slot::Title("settings-group-lyrics"), Slot::LyricsProviders]);
                if self
                    .settings
                    .read(cx)
                    .enabled_lyrics_provider_priority()
                    .len()
                    > 1
                {
                    slots.push(Slot::LyricsPriority);
                }
                slots.extend([Slot::Karaoke, Slot::Romanized]);
                slots
            }
            SettingsTab::Privacy => {
                vec![
                    Slot::Title("settings-group-lyrics"),
                    Slot::LyricsForLocal,
                    Slot::Title("settings-group-discord"),
                    Slot::ArtworkForLocal,
                ]
            }
            SettingsTab::Integrations => self
                .discord_slots(cx)
                .into_iter()
                .chain([Slot::Title("settings-group-scrobbling")])
                .chain(self.scrobble_slots(cx))
                .collect(),
            SettingsTab::About => vec![
                Slot::Title("settings-tab-general"),
                Slot::Version,
                Slot::Updates,
                Slot::Log,
            ]
            .into_iter()
            .chain([
                Slot::Title("settings-group-project"),
                Slot::License,
                Slot::Source,
            ])
            .collect(),
        }
    }

    /// The title and detail a search is matched against, without building the row. Kept next
    /// to the builders by key: a new row needs its pair here to be found.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per row keeps the keys beside the rows"
    )]
    fn slot_meta(&self, slot: Slot, cx: &App) -> Option<(SharedString, SharedString)> {
        let meta = match slot {
            Slot::Title(_) | Slot::Sep => return None,
            Slot::Startup => (t!("settings-startup"), t!("settings-startup-detail")),
            Slot::Entries => (t!("settings-entries"), t!("settings-entries-detail")),
            Slot::Language => (t!("settings-language"), t!("settings-language-detail")),
            Slot::Tray => (
                t!("settings-close-to-tray"),
                t!("settings-close-to-tray-detail"),
            ),
            Slot::TrayIcon => (t!("settings-tray-icon"), t!("settings-tray-icon-detail")),
            Slot::Accounts => {
                let detail = t!("settings-accounts-detail");
                let names = self.account_words(cx);
                (t!("settings-accounts"), format!("{detail} {names}").into())
            }
            Slot::LocalFolder => (
                t!("settings-local-folder"),
                match self.session.read(cx).local_paths().is_empty() {
                    true => t!("settings-local-folder-empty"),
                    false => SharedString::default(),
                },
            ),
            Slot::Theme => (t!("settings-theme"), t!("settings-theme-detail")),
            Slot::Adaptive => (t!("settings-adaptive"), t!("settings-adaptive-detail")),
            Slot::Ambient => (t!("settings-ambient"), t!("settings-ambient-detail")),
            Slot::AmbientMotion => (
                t!("settings-ambient-motion"),
                t!("settings-ambient-motion-detail"),
            ),
            Slot::Visualizer => (t!("settings-visualizer"), t!("settings-visualizer-detail")),
            Slot::VisualizerAbsolute => (
                t!("settings-visualizer-absolute"),
                t!("settings-visualizer-absolute-detail"),
            ),
            Slot::Icons => (t!("settings-icons"), t!("settings-icons-detail")),
            Slot::Opacity => (t!("settings-opacity"), t!("settings-opacity-detail")),
            Slot::WindowBlur => (
                t!("settings-blur-window"),
                t!("settings-blur-window-detail"),
            ),
            Slot::Blur => (t!("settings-blur"), t!("settings-blur-detail")),
            Slot::Corners => (t!("settings-corners"), t!("settings-corners-detail")),
            Slot::FullscreenControlsAutohide => (
                t!("settings-fullscreen-controls-autohide"),
                t!("settings-fullscreen-controls-autohide-detail"),
            ),
            Slot::OsFullscreen => (
                t!("settings-os-fullscreen"),
                t!("settings-os-fullscreen-detail"),
            ),
            Slot::PanelLyricsSize => (
                i18n::lookup("settings-panel-lyrics-size", None),
                i18n::lookup("settings-panel-lyrics-size-detail", None),
            ),
            Slot::FullscreenLyricsSize => (
                i18n::lookup("settings-fullscreen-lyrics-size", None),
                i18n::lookup("settings-fullscreen-lyrics-size-detail", None),
            ),
            Slot::BlurLyrics => (
                t!("settings-blur-lyrics"),
                t!("settings-blur-lyrics-detail"),
            ),
            Slot::Font => (t!("settings-font"), t!("settings-font-detail")),
            Slot::Typeface => (t!("settings-typeface"), t!("settings-typeface-detail")),
            Slot::Motion => (t!("settings-motion"), t!("settings-motion-detail")),
            Slot::Pace => (t!("settings-pace"), t!("settings-pace-detail")),
            Slot::Saver => (t!("settings-saver"), t!("settings-saver-detail")),
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            Slot::ServerSideDecorations => (
                t!("settings-server-side-decorations"),
                t!("settings-server-side-decorations-detail"),
            ),
            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            Slot::Decorations => (
                t!("settings-window-controls"),
                t!("settings-window-controls-detail"),
            ),
            #[cfg(not(target_os = "macos"))]
            Slot::Side => (
                t!("settings-controls-side"),
                t!("settings-controls-side-detail"),
            ),
            #[cfg(not(target_os = "macos"))]
            Slot::TrafficLights => (
                t!("settings-traffic-light-controls"),
                t!("settings-traffic-light-controls-detail"),
            ),
            #[cfg(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "freebsd",
                target_os = "macos"
            ))]
            Slot::WindowRounding => (
                t!("settings-window-rounding"),
                t!("settings-window-rounding-detail"),
            ),
            Slot::AdaptiveMenu => (
                t!("settings-adaptive-menu"),
                t!("settings-adaptive-menu-detail"),
            ),
            Slot::Normalisation => (
                t!("settings-normalisation"),
                t!("settings-normalisation-detail"),
            ),
            Slot::Gapless => (t!("settings-gapless"), t!("settings-gapless-detail")),
            Slot::StreamQuality => (
                t!("settings-stream-quality"),
                t!("settings-stream-quality-detail"),
            ),
            Slot::StreamFormat => (
                t!("settings-stream-format"),
                t!("settings-stream-format-detail"),
            ),
            Slot::StreamBitrate => (
                t!("settings-stream-bitrate"),
                t!("settings-stream-bitrate-detail"),
            ),
            Slot::Sleep => (t!("settings-sleep"), t!("settings-sleep-detail")),
            Slot::StayAwake => (t!("settings-stay-awake"), t!("settings-stay-awake-detail")),
            Slot::Widevine => {
                let (detail, _) = widevine_copy(self.drm.read(cx).state());
                (t!("settings-widevine"), i18n::lookup(detail, None))
            }
            Slot::OutputDevice => (
                t!("settings-output-device"),
                t!("settings-output-device-detail"),
            ),
            Slot::OutputFailure => self.output_failure(cx),
            Slot::SampleRate => (
                t!("settings-sample-rate"),
                t!("settings-sample-rate-detail"),
            ),
            Slot::Buffer => (t!("settings-buffer"), t!("settings-buffer-detail")),
            Slot::Equalizer => (t!("settings-equalizer"), t!("settings-equalizer-detail")),
            Slot::EqualizerPreset => (
                t!("settings-equalizer-preset"),
                t!("settings-equalizer-preset-detail"),
            ),
            Slot::LyricsProviders => (
                t!("settings-lyrics-providers"),
                t!("settings-lyrics-providers-detail"),
            ),
            Slot::LyricsPriority => (
                t!("settings-lyrics-priority"),
                t!("settings-lyrics-priority-detail"),
            ),
            Slot::Karaoke => (
                t!("settings-karaoke-lyrics"),
                t!("settings-karaoke-lyrics-detail"),
            ),
            Slot::Romanized => (
                t!("settings-romanized-lyrics"),
                t!("settings-romanized-lyrics-detail"),
            ),
            Slot::LyricsForLocal => (
                t!("settings-lyrics-for-local-files"),
                t!("settings-lyrics-for-local-files-detail"),
            ),
            Slot::Discord => (t!("settings-discord"), t!("settings-discord-detail")),
            Slot::DiscordName => (
                t!("settings-discord-name"),
                t!("settings-discord-name-detail"),
            ),
            Slot::DiscordShowPaused => (
                t!("settings-discord-show-paused"),
                t!("settings-discord-show-paused-detail"),
            ),
            Slot::DiscordBadge => (
                t!("settings-discord-badge"),
                t!("settings-discord-badge-detail"),
            ),
            Slot::ArtworkForLocal => (
                t!("settings-artwork-for-local-files"),
                t!("settings-artwork-for-local-files-detail"),
            ),
            Slot::DiscordAnonymous => (
                t!("settings-discord-anonymous"),
                t!("settings-discord-anonymous-detail"),
            ),
            Slot::DiscordButtons => (
                t!("settings-discord-buttons"),
                t!("settings-discord-buttons-detail"),
            ),
            Slot::Scrobble(index) => {
                let rows = self.scrobbling.read(cx).rows();
                let row = rows.get(index)?;
                let service = row.id();
                let detail = match row.state() {
                    ScrobbleState::Off => t!("settings-scrobble-off"),
                    ScrobbleState::Linking => t!("settings-scrobble-waiting"),
                    ScrobbleState::On(name) => match name.is_empty() {
                        true => t!("settings-scrobble-on"),
                        false => t!("settings-scrobble-as", name = name.as_ref()),
                    },
                    ScrobbleState::Failed(key) => i18n::lookup(key, None),
                };
                (i18n::lookup(&format!("settings-{service}"), None), detail)
            }
            Slot::Version => (t!("settings-version"), t!("settings-version-detail")),
            Slot::Updates => (
                t!("settings-check-updates"),
                t!("settings-check-updates-detail"),
            ),
            Slot::Log => (t!("settings-log"), t!("settings-log-detail")),
            Slot::License => (t!("settings-license"), t!("settings-license-detail")),
            Slot::Source => (t!("settings-source"), t!("settings-source-detail")),
        };
        Some(meta)
    }

    /// How tall the slot draws. Standard rows share one height with the row builder, and
    /// the composite rows sum theirs from the same fixed parts their elements are built of.
    fn slot_height(&self, slot: Slot, window: &Window, cx: &App) -> Pixels {
        let theme = *cx.theme();
        match slot {
            Slot::Title(_) => snapped(theme.metrics.row, window),
            Slot::Sep => SEPARATOR_HEIGHT,
            Slot::Accounts => snapped(self.accounts_height(&theme, cx), window),
            Slot::LocalFolder => snapped(self.local_height(&theme, cx), window),
            Slot::LyricsPriority => snapped(self.lyrics_priority_height(&theme, cx), window),
            Slot::Widevine => snapped(self.widevine_height(&theme, window, cx), window),
            _ => snapped(standard_height(&theme), window),
        }
    }

    /// The accounts block: the header over one card per provider. Summed from the same fixed
    /// parts the element is built of, so the deck never clips a card.
    fn accounts_height(&self, theme: &Theme, cx: &App) -> Pixels {
        let head = line(theme, Text::Body) + ROW_GAP + line(theme, Text::Small);
        let mut total = SECTION_GAP + head + SECTION_GAP;
        for account in self.providers(cx) {
            total += SECTION_GAP + card_height(theme, account.error.is_some());
        }
        if self.guest(cx).is_some() {
            total += SECTION_GAP + card_height(theme, false);
        }
        total + ACCOUNTS_SLACK
    }

    /// The Widevine row: its title and action over every line its explanation wraps to.
    fn widevine_height(&self, theme: &Theme, window: &Window, cx: &App) -> Pixels {
        let (detail, _) = widevine_copy(self.drm.read(cx).state());
        let width = self.column.unwrap_or(WIDTH);
        let lines = wrapped_lines(
            i18n::lookup(detail, None),
            theme.text(Text::Small),
            width,
            window,
        );
        SECTION_GAP
            + widevine_head(theme)
            + ROW_GAP
            + line(theme, Text::Small) * lines as f32
            + SECTION_GAP
    }

    /// The local folder block: the header over one line per watched folder.
    fn local_height(&self, theme: &Theme, cx: &App) -> Pixels {
        let mut total = standard_height(theme);
        let paths = self.session.read(cx).local_paths().len();
        if paths > 0 {
            total += ROW_GAP
                + theme.metrics.control_small * paths as f32
                + ROW_GAP * paths.saturating_sub(1) as f32
                + BLOCK_GAP;
        }
        total
    }

    /// The lyrics priority block: the header over one entry per enabled source.
    fn lyrics_priority_height(&self, theme: &Theme, cx: &App) -> Pixels {
        let sources = self
            .settings
            .read(cx)
            .enabled_lyrics_provider_priority()
            .len();
        standard_height(theme) + ROW_GAP + theme.metrics.row * sources as f32 + BLOCK_GAP
    }

    /// Builds the slot's row. Asked only for the rows in view, every frame they are.
    fn slot_element(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        match slot {
            Slot::Title(key) => div()
                .h(cx.theme().metrics.row)
                .flex()
                .flex_col()
                .justify_end()
                .pb_1()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(eyebrow(i18n::lookup(key, None), cx)),
                )
                .into_any_element(),
            Slot::Sep => Separator::horizontal().w_full().into_any_element(),
            Slot::Startup => self.startup_row(cx).element,
            Slot::Entries => self.entries_row(cx).element,
            Slot::Language => self.language_row(cx).element,
            Slot::Tray => self.tray_row(cx).element,
            Slot::TrayIcon => self.tray_icon_row(cx).element,
            Slot::Accounts => self.accounts_row(cx).element,
            Slot::LocalFolder => self.local_folder_row(cx).element,
            Slot::Theme => self.theme_row(cx).element,
            Slot::Adaptive => self.adaptive_row(cx).element,
            Slot::Ambient => self.ambient_row(cx).element,
            Slot::AmbientMotion => self.ambient_motion_row(cx).element,
            Slot::Visualizer => self.visualizer_style_row(cx).element,
            Slot::VisualizerAbsolute => self.visualizer_absolute_row(cx).element,
            Slot::Icons => self.icons_row(cx).element,
            Slot::Opacity => self.opacity_row(cx).element,
            Slot::WindowBlur => self.blur_window_row(cx).element,
            Slot::Blur => self.blur_row(cx).element,
            Slot::Corners => self.corners_row(cx).element,
            Slot::FullscreenControlsAutohide => self.fullscreen_controls_autohide_row(cx).element,
            Slot::OsFullscreen => self.os_fullscreen_row(cx).element,
            Slot::PanelLyricsSize => self.panel_lyrics_size_row(cx).element,
            Slot::FullscreenLyricsSize => self.fullscreen_lyrics_size_row(cx).element,
            Slot::BlurLyrics => self.blur_lyrics_row(cx).element,
            Slot::Font => self.font_row(cx).element,
            Slot::Typeface => self.typeface_row(cx).element,
            Slot::Motion => self.motion_row(cx).element,
            Slot::Pace => self.pace_row(cx).element,
            Slot::Saver => self.saver_row(cx).element,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            Slot::ServerSideDecorations => self.server_side_decorations_row(cx).element,
            #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
            Slot::Decorations => self.decorations_row(cx).element,
            #[cfg(not(target_os = "macos"))]
            Slot::Side => self.side_row(cx).element,
            #[cfg(not(target_os = "macos"))]
            Slot::TrafficLights => self.traffic_light_controls_row(cx).element,
            #[cfg(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "freebsd",
                target_os = "macos"
            ))]
            Slot::WindowRounding => self.window_rounding_row(cx).element,
            Slot::AdaptiveMenu => self.adaptive_menu_row(cx).element,
            Slot::Normalisation => self.playback_row(cx).element,
            Slot::Gapless => self.gapless_row(cx).element,
            Slot::StreamQuality => self.stream_quality_row(cx).element,
            Slot::StreamFormat => self.stream_format_row(cx).element,
            Slot::StreamBitrate => self.stream_bitrate_row(cx).element,
            Slot::Sleep => self.sleep_row(cx).element,
            Slot::StayAwake => self.stay_awake_row(cx).element,
            Slot::OutputDevice => self.output_device_row(cx).element,
            Slot::OutputFailure => self.output_failure_row(cx).element,
            Slot::SampleRate => self.sample_rate_row(cx).element,
            Slot::Buffer => self.buffer_row(cx).element,
            Slot::Widevine => self.widevine_row(cx).element,
            Slot::Equalizer => self.equalizer_row(cx).element,
            Slot::EqualizerPreset => self.equalizer_preset_row(cx).element,
            Slot::LyricsProviders => self.lyrics_providers_row(cx).element,
            Slot::LyricsPriority => self.lyrics_priority_row(cx).element,
            Slot::Karaoke => self.karaoke_lyrics_row(cx).element,
            Slot::Romanized => self.romanized_lyrics_row(cx).element,
            Slot::LyricsForLocal => self.lyrics_for_local_files_row(cx).element,
            Slot::Discord => self.discord_row(cx).element,
            Slot::DiscordName => self.discord_name_row(cx).element,
            Slot::DiscordShowPaused => self.discord_show_paused_row(cx).element,
            Slot::DiscordBadge => self.discord_badge_row(cx).element,
            Slot::ArtworkForLocal => self.artwork_for_local_files_row(cx).element,
            Slot::DiscordAnonymous => self.discord_anonymous_row(cx).element,
            Slot::DiscordButtons => self.discord_buttons_row(cx).element,
            Slot::Scrobble(index) => match index < self.scrobbling.read(cx).rows().len() {
                true => self.scrobble_row(index, cx).element,
                false => div().into_any_element(),
            },
            Slot::Version => self.version_row(cx).element,
            Slot::Updates => self.updates_row(cx).element,
            Slot::Log => self.log_row(cx).element,
            Slot::License => self.license_row(cx).element,
            Slot::Source => self.source_row(cx).element,
        }
    }

    fn look(&self, cx: &Context<Self>) -> Look {
        Look {
            tint: cx.theme().tint,
            tint_secondary: cx.theme().tint_secondary,
            ..self.settings.read(cx).look()
        }
    }

    #[allow(
        unused_variables,
        reason = "cx is only read on Linux and FreeBSD, where rounding follows the decorations"
    )]
    fn decoration_slots(&self, cx: &App) -> Vec<Slot> {
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let mut slots = vec![
            Slot::Title("settings-group-window-style"),
            Slot::ServerSideDecorations,
            Slot::Side,
            Slot::TrafficLights,
        ];
        // Server-side decorations put the compositor in charge of the frame, so window
        // rounding is only ever this app's call with client-side ones.
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if !self.settings.read(cx).server_side_decorations() {
            slots.push(Slot::WindowRounding);
        }
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
        let slots = vec![
            Slot::Title("settings-group-title-bar"),
            Slot::Decorations,
            Slot::Side,
            Slot::TrafficLights,
            Slot::WindowRounding,
        ];
        #[cfg(target_os = "macos")]
        let slots = vec![
            Slot::Title("settings-group-window-style"),
            Slot::Decorations,
            Slot::WindowRounding,
        ];
        slots
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !cx.has_active_drag() {
            self.lyrics_drop_gap = None;
        }
        let searching = self.searching();
        // a search can list the typeface row from any category
        let in_appearance = self.tab == SettingsTab::Appearance || searching;
        let picking_typefaces = self.popovers.shows(TYPEFACES);
        if (in_appearance || picking_typefaces) && self.installed.is_none() && !self.loading_fonts {
            self.loading_fonts = true;
            let text_system = cx.text_system().clone();
            let io = Io::global(cx);
            self.font_task = Some(cx.spawn(async move |this, cx| {
                let names = io
                    .spawn_blocking(move || usable_fonts(text_system))
                    .await
                    .unwrap_or_default();
                this.update(cx, |this, cx| {
                    this.installed = Some(names);
                    this.loading_fonts = false;
                    this.font_task = None;
                    // the picker may already be open on an empty list: put the
                    // cursor on the chosen face now that it can be found
                    if this.popovers.shows(TYPEFACES) {
                        let chosen = this.settings.read(cx).font();
                        let place = this
                            .typeface_entries()
                            .iter()
                            .position(|name| name.as_ref() == chosen);
                        this.typefaces.place(place, cx);
                    }
                    cx.notify();
                })
                .ok();
            }));
        }

        let picking_languages = self.popovers.shows(LANGUAGES);
        if picking_languages {
            let chosen_language = self.settings.read(cx).language();
            let language_selected = Language::ALL
                .into_iter()
                .position(|language| language.id() == chosen_language)
                .map(|place| place + 1)
                .or(Some(0));
            self.languages.sync(true, language_selected, window, cx);
        } else {
            self.languages.sync(false, None, window, cx);
        }

        if picking_typefaces {
            let chosen_typeface = self.settings.read(cx).font();
            let typeface_selected = self
                .typeface_entries()
                .iter()
                .position(|name| name.as_ref() == chosen_typeface);
            self.typefaces.sync(true, typeface_selected, window, cx);
        } else {
            self.typefaces.sync(false, None, window, cx);
        }

        let accounts = match self.session.read(cx).state() {
            SessionState::Authorizing(Some(SignInPrompt::Accounts(accounts))) => {
                Some(accounts.clone())
            }
            _ => None,
        };
        let code = match self.session.read(cx).state() {
            SessionState::Authorizing(Some(SignInPrompt::Code { code, url })) => {
                Some((code.clone(), url.clone()))
            }
            _ => None,
        };
        if self.sign_in_running && !self.session.read(cx).is_pending() {
            self.sign_in_running = false;
            self.sign_in_for = None;
        }
        let manual_secret = self.manual_secret.filter(|_| {
            matches!(
                self.session.read(cx).state(),
                SessionState::Authorizing(Some(SignInPrompt::Secret))
            )
        });

        // only one of these is ever up: a prompt the sign-in raised hides the choice behind
        // it, and the choice holds the veil until that prompt arrives
        let taken = accounts.is_some()
            || code.is_some()
            || manual_secret.is_some()
            || self.credentials_for.is_some();
        let sign_in_for = self.sign_in_for.filter(|_| !taken);

        // a dialog takes the key focus, since escape only reaches the page from inside it
        let dialog = taken || sign_in_for.is_some() || self.scrobble_prompt.is_some();
        match (dialog, self.grabbed) {
            (true, false) => {
                window.focus(&self.focus, cx);
                self.grabbed = true;
            }
            (false, true) => self.grabbed = false,
            _ => {}
        }

        let general = self.tab == SettingsTab::General && !searching;
        let about = self.tab == SettingsTab::About && !searching;
        let view = cx.entity().downgrade();

        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                cx.stop_propagation();
                this.escape(cx);
            }))
            // the padding, the fade and the scrollbar all hang off the header's measured
            // height, so the page sits out the first frame rather than snapping into place
            .when(!self.header_measured, |this| this.invisible())
            .child(
                Scroller::new("settings", &self.scrollbar)
                    .flex()
                    .flex_col()
                    .items_center()
                    // the rows dissolve over the header's height as they scroll up beneath
                    // it, the way the verse sheet fades its edges, so no sharp content ever
                    // sits over the blur. The tail reaches past the header so the handoff
                    // has no hard edge.
                    .when(effects(), |this| {
                        this.fade_edges(self.header_height + HEADER_FADE_TAIL, px(0.))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_6()
                            .w_full()
                            .max_w(WIDTH)
                            .px_6()
                            .pb_6()
                            .pt(self.header_height)
                            // every child stretches across the column, so the widest is the
                            // width the rows are laid out at
                            .on_children_prepainted(move |bounds, _, cx| {
                                let Some(width) = bounds
                                    .iter()
                                    .map(|bounds| bounds.size.width)
                                    .reduce(Pixels::max)
                                else {
                                    return;
                                };
                                view.update(cx, |view, cx| view.set_column(width, cx)).ok();
                            })
                            .when(general, |this| {
                                this.child(self.profile(cx))
                                    .child(Separator::horizontal().w_full())
                            })
                            .child(self.panel(window, cx))
                            .when(about, |this| {
                                this.child(self.team(cx)).child(self.notice(cx))
                            }),
                    ),
            )
            .when_some(accounts, |this, accounts| {
                this.child(self.account_modal(accounts, cx).into_any_element())
            })
            .when_some(code, |this, (code, url)| {
                this.child(self.code_prompt(code, url, cx).into_any_element())
            })
            .when_some(manual_secret, |this, (slug, provider)| {
                this.child(self.secret_prompt(slug, provider, cx).into_any_element())
            })
            .when(self.credentials_for.is_some(), |this| {
                this.child(self.credentials_prompt(cx).into_any_element())
            })
            .when_some(self.scrobble_prompt, |this, service| {
                this.child(self.scrobble_modal(service, cx).into_any_element())
            })
            .when_some(sign_in_for, |this, (slug, provider)| {
                this.child(self.sign_in_prompt(slug, provider, cx).into_any_element())
            })
    }
}

/// The search field and the category bar over the settings page. `Workspace` floats it over
/// the page, outside the transition, so a category switch fades the rows and nothing else, and
/// the rows scroll beneath it. It reads the page's state, writes back through
/// `SettingsView::select`, and reports its height so the page can start below it.
pub struct SettingsHeader {
    view: Entity<SettingsView>,
    /// The category clicked last, kept free of the hover shade until the pointer leaves it,
    /// so the change of selection never flashes.
    calm: Option<SettingsTab>,
}

impl SettingsHeader {
    pub fn new(view: Entity<SettingsView>, cx: &mut Context<Self>) -> Self {
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        Self { view, calm: None }
    }
}

impl Render for SettingsHeader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.view.read(cx);
        let search = page.search.clone();
        let chosen = page.tab;
        let searching = page.searching();
        let height = page.header_height;
        let calm = self.calm;
        let view = self.view.clone();
        let theme = *cx.theme();

        // a search lights no category, since its rows come from all of them, and picking
        // one ends the search
        let categories = TabBar::new("settings-categories")
            .max_w_full()
            .blurred()
            .items(SettingsTab::ALL.map(|tab| {
                Button::new(tab.id())
                    .label(i18n::lookup(tab.key(), None))
                    .icon(tab.icon())
                    .small()
                    .ghost()
                    .selected(!searching && tab == chosen)
                    .when(calm == Some(tab), Button::hoverless)
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if !hovered && this.calm == Some(tab) {
                            this.calm = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.calm = Some(tab);
                        this.view.update(cx, |view, cx| view.select(tab, cx));
                        navigate(Destination::Settings(tab), cx);
                    }))
            }));

        div()
            .relative()
            .flex()
            .justify_center()
            .px_6()
            // the veil is absolute and takes the header's own size, so the last child, the
            // column, is the one to measure
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(height) = bounds.last().map(|bounds| bounds.size.height) else {
                    return;
                };
                view.update(cx, |view, cx| view.set_header_height(height, cx));
            })
            // The haze follows the window: a see-through page has content of its own passing
            // under the header and reads worse without it. Only the flat fallback is dropped
            // there, since a solid band over a see-through page is a slab.
            .when(effects() || !theme.transparent, |this| {
                this.child(veil(
                    Edge::Top,
                    height,
                    HEADER_BLUR,
                    theme.background,
                    None,
                    window,
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(WIDTH)
                    .gap_2()
                    .pt_6()
                    .pb_6()
                    .child(search)
                    .child(div().flex().justify_center().child(categories)),
            )
    }
}
