pub(crate) mod about;
pub(crate) mod adaptive;
pub(crate) mod album_grid;
pub(crate) mod ambient;
pub(crate) mod cards;
pub(crate) mod cells;
pub(crate) mod confirm;
pub(crate) mod device_code;
pub(crate) mod equalizer;
pub(crate) mod hero;
pub(crate) mod local;
pub(crate) mod menus;
pub(crate) mod nav;
pub(crate) mod page;
pub(crate) mod picks;
pub(crate) mod pins;
pub(crate) mod playlist_editor;
pub(crate) mod popups;
pub(crate) mod shelves;
pub(crate) mod steps;
pub(crate) mod tag_editor;
pub(crate) mod text;
pub(crate) mod track_card;
pub(crate) mod tracks;
pub(crate) mod transport;
pub(crate) mod trouble;
pub(crate) mod visualizer;
pub(crate) mod widevine;

pub(crate) use ui::effects;

pub(crate) fn provider_logo(slug: &str) -> &'static str {
    match slug {
        "spotify" => "icons/spotify.svg",
        "youtube" => "icons/youtubemusic.svg",
        "subsonic" => "icons/subsonic.svg",
        "deezer" => "icons/deezer.svg",
        "apple" => "icons/applemusic.svg",
        "tidal" => "icons/tidal.svg",
        _ => "icons/music.svg",
    }
}
