mod link;
mod navigation;
mod uri;

pub use link::Link;
pub use navigation::{Navigation, NavigationEvent};
pub use uri::destination;

use gpui::{App, AppContext as _, Entity, Global, SharedString};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryTab {
    Songs,
    Albums,
    Playlists,
    Artists,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEntry {
    Home,
    Search,
    Library,
    History,
    Local,
    Pins,
}

impl NavEntry {
    pub const ALL: [Self; 6] = [
        Self::Home,
        Self::Search,
        Self::Library,
        Self::History,
        Self::Local,
        Self::Pins,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Search => "search",
            Self::Library => "library",
            Self::History => "history",
            Self::Local => "local",
            Self::Pins => "sidebar-pins",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Search => "nav-search",
            Self::Library => "nav-library",
            Self::History => "nav-history",
            Self::Local => "nav-local",
            Self::Pins => "nav-pinned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    History,
    Songs,
    Albums,
    Playlists,
    Artists,
    Imported,
}

impl Screen {
    pub const ALL: [Self; 8] = [
        Self::Home,
        Self::Search,
        Self::Songs,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
        Self::Imported,
        Self::History,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Search => "search",
            Self::History => "history",
            Self::Songs => "songs",
            Self::Albums => "albums",
            Self::Playlists => "playlists",
            Self::Artists => "artists",
            Self::Imported => "imported",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Search => "nav-search",
            Self::History => "nav-history",
            Self::Songs => "nav-songs",
            Self::Albums => "nav-albums",
            Self::Playlists => "nav-playlists",
            Self::Artists => "nav-artists",
            Self::Imported => "nav-local",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|screen| screen.id() == id)
    }

    /// Whether the screen shows account library data, and so stays empty in guest mode.
    pub fn needs_account(self) -> bool {
        match self {
            Self::Home => false,
            Self::Search => false,
            Self::History => false,
            Self::Songs => true,
            Self::Albums => true,
            Self::Playlists => true,
            Self::Artists => true,
            Self::Imported => false,
        }
    }

    pub fn destination(self) -> Destination {
        match self {
            Self::Home => Destination::Home,
            Self::Search => Destination::Search,
            Self::History => Destination::History,
            Self::Songs => Destination::Library(LibraryTab::Songs),
            Self::Albums => Destination::Library(LibraryTab::Albums),
            Self::Playlists => Destination::Library(LibraryTab::Playlists),
            Self::Artists => Destination::Library(LibraryTab::Artists),
            Self::Imported => Destination::Local(LibraryTab::Songs),
        }
    }
}

/// One page of the settings takeover. The hub shows the first ten as tiles in `HUB` order, and
/// the last two are drill-ins that open from a row on their parent page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    Providers,
    General,
    Appearance,
    NowPlaying,
    Lyrics,
    Playback,
    Sound,
    Discord,
    Scrobbling,
    About,
    LyricsSources,
    MusicFolders,
}

impl SettingsPage {
    /// The pages the hub shows as tiles, in reading order.
    pub const HUB: [Self; 10] = [
        Self::Providers,
        Self::General,
        Self::Appearance,
        Self::NowPlaying,
        Self::Lyrics,
        Self::Playback,
        Self::Sound,
        Self::Discord,
        Self::Scrobbling,
        Self::About,
    ];

    /// Every page, the hub's first and the drill-ins after them.
    pub const ALL: [Self; 12] = [
        Self::Providers,
        Self::General,
        Self::Appearance,
        Self::NowPlaying,
        Self::Lyrics,
        Self::Playback,
        Self::Sound,
        Self::Discord,
        Self::Scrobbling,
        Self::About,
        Self::LyricsSources,
        Self::MusicFolders,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Providers => "providers",
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::NowPlaying => "now-playing",
            Self::Lyrics => "lyrics",
            Self::Playback => "playback",
            Self::Sound => "sound",
            Self::Discord => "discord",
            Self::Scrobbling => "scrobbling",
            Self::About => "about",
            Self::LyricsSources => "lyrics-sources",
            Self::MusicFolders => "music-folders",
        }
    }

    /// The Fluent key of the page's name.
    pub fn key(self) -> &'static str {
        match self {
            Self::Providers => "settings-page-providers",
            Self::General => "settings-page-general",
            Self::Appearance => "settings-page-appearance",
            Self::NowPlaying => "settings-page-now-playing",
            Self::Lyrics => "settings-page-lyrics",
            Self::Playback => "settings-page-playback",
            Self::Sound => "settings-page-sound",
            Self::Discord => "settings-page-discord",
            Self::Scrobbling => "settings-page-scrobbling",
            Self::About => "settings-page-about",
            Self::LyricsSources => "settings-page-lyrics-sources",
            Self::MusicFolders => "settings-page-music-folders",
        }
    }

    /// The icon that stands for the page beside a search result found on it.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Providers => "icons/user-round.svg",
            Self::General => "icons/settings.svg",
            Self::Appearance => "icons/palette.svg",
            Self::NowPlaying => "icons/maximize.svg",
            Self::Lyrics | Self::LyricsSources => "icons/mic-vocal.svg",
            Self::Playback => "icons/play.svg",
            Self::Sound => "icons/volume-2.svg",
            Self::Discord => "icons/radio.svg",
            Self::Scrobbling => "icons/list-music.svg",
            Self::About => "icons/info.svg",
            Self::MusicFolders => "icons/file-music.svg",
        }
    }

    /// The page a drill-in returns to, or `None` for a page the hub lists.
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::LyricsSources => Some(Self::Lyrics),
            Self::MusicFolders => Some(Self::Providers),
            _ => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Home,
    History,
    Library(LibraryTab),
    Local(LibraryTab),
    Album(SharedString),
    Song(SharedString),
    Playlist(SharedString),
    Artist(SharedString),
    User(SharedString),
    Genre(SharedString),
    Search,
    /// The tiles every settings page opens from, inside the workspace.
    SettingsHub,
    Settings(SettingsPage),
    /// The equalizer's own page in the settings takeover, opened from Sound.
    Equalizer,
    Fullscreen,
}

impl From<&ui::Pin> for Destination {
    fn from(pin: &ui::Pin) -> Self {
        let id = SharedString::from(pin.id.clone());
        match pin.kind {
            ui::PinKind::Album => Destination::Album(id),
            ui::PinKind::Artist => Destination::Artist(id),
            ui::PinKind::Playlist => Destination::Playlist(id),
            ui::PinKind::Song => Destination::Song(id),
        }
    }
}

impl Destination {
    pub fn same_section(&self, other: &Destination) -> bool {
        match (self, other) {
            (Destination::Library(_), Destination::Library(_))
            | (Destination::Local(_), Destination::Local(_)) => true,
            _ if self.settings_related() && other.settings_related() => true,
            _ => self == other,
        }
    }

    /// Whether the settings takeover draws this destination rather than the workspace.
    pub fn in_settings(&self) -> bool {
        matches!(self, Destination::Settings(_) | Destination::Equalizer)
    }

    /// Whether this is the settings hub or a page of the takeover, which the sidebar's Settings
    /// entry lights for alike.
    pub fn settings_related(&self) -> bool {
        self.in_settings() || matches!(self, Destination::SettingsHub)
    }

    /// The settings page a drill-in's back button returns to. Only drill-ins have one.
    pub fn parent(&self) -> Option<Destination> {
        match self {
            Destination::Equalizer => Some(Destination::Settings(SettingsPage::Sound)),
            Destination::Settings(page) => page.parent().map(Destination::Settings),
            _ => None,
        }
    }

    /// The hub page this destination belongs to: the page itself, or a drill-in's parent.
    pub fn settings_page(&self) -> Option<SettingsPage> {
        match self {
            Destination::Equalizer => Some(SettingsPage::Sound),
            Destination::Settings(page) => Some(page.parent().unwrap_or(*page)),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct Router(Entity<Navigation>);

impl Global for Router {}

pub fn init(start: Destination, cx: &mut App) {
    let navigation = cx.new(|_| Navigation::new(start));
    cx.set_global(Router(navigation));
}

pub fn trail(cx: &App) -> Entity<Navigation> {
    cx.global::<Router>().0.clone()
}

pub fn navigate(destination: Destination, cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.go(destination, cx));
}

/// Replaces the current destination so Back and Forward do not revisit a page that disappeared.
pub fn replace(destination: Destination, cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.replace(destination, cx));
}

pub fn back(cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.back(cx));
}

pub fn forward(cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.forward(cx));
}

/// Steps up from a drill-in to `parent`, going back when it is the page before so the history
/// holds no loop, and replacing the drill-in otherwise.
pub fn up(parent: Destination, cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.up(parent, cx));
}

/// Leaves the settings takeover for the hub, however many settings pages were visited in
/// between.
pub fn close_settings(cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.leave_settings(cx));
}
