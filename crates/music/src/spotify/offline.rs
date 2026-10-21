//! Spotify tracks downloaded for offline play, kept the way Spotify's own clients keep theirs:
//! the file as the CDN sent it, still encrypted, with its key and metadata beside it. The vault
//! seals all of that again under this machine's key, so a download is no easier to play outside
//! Sonora than the stream.

use anyhow::{Context as _, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use librespot_core::audio_key::AudioKey;
use librespot_core::{FileId, Session, SpotifyId};
use librespot_playback::config::Bitrate;
use librespot_playback::offline::{self, Offline, OfflineTrack};
use serde::{Deserialize, Serialize};

use crate::Throttled;
use crate::spotify::playback::track_uri;
use crate::vault::Shelf;

/// What is kept beside a downloaded file, which is everything librespot asks the network for
/// when it plays one.
#[derive(Serialize, Deserialize)]
struct Facts {
    /// The track's `TRACK_V4` metadata, in base64.
    metadata: String,
    /// The file's format, as Spotify's protocol numbers it.
    format: i32,
    file_id: String,
    key: Option<[u8; 16]>,
}

/// The downloads librespot plays before it reaches for the network.
pub(crate) struct Kept(pub(crate) Shelf);

impl Offline for Kept {
    fn track(&self, track: SpotifyId) -> Option<OfflineTrack> {
        let (facts, file) = self.0.open_pinned(&track.to_base62())?;
        let facts = serde_json::from_slice::<Facts>(&facts)
            .inspect_err(|error| log::warn!("spotify: the kept facts do not read: {error}"))
            .ok()?;
        let metadata = STANDARD.decode(&facts.metadata).ok()?;
        let format = offline::AudioFileFormat::try_from(facts.format).ok()?;
        let file_id = FileId::from(hex(&facts.file_id)?.as_slice());
        let len = file.length();
        log::debug!("spotify: {} plays from its download", track.to_base62());
        Some(OfflineTrack {
            metadata,
            format,
            file_id,
            key: facts.key.map(AudioKey),
            file: Box::new(file),
            len,
        })
    }
}

/// Fetches all of track `id` at `bitrate` on a connected session and keeps it as a download.
pub(crate) async fn download(
    session: &Session,
    shelf: &Shelf,
    id: &str,
    bitrate: Bitrate,
) -> Result<()> {
    if shelf.holds(id) {
        return Ok(());
    }
    let fetched = match offline::download(session, track_uri(id)?, bitrate).await {
        Ok(fetched) => fetched,
        Err(error) if session.audio_key().is_throttled() => {
            return Err(anyhow::Error::new(error).context(Throttled));
        }
        Err(error) => {
            return Err(anyhow::Error::new(error).context(format!("cannot download {id}")));
        }
    };
    let facts = serde_json::to_vec(&Facts {
        metadata: STANDARD.encode(&fetched.metadata),
        format: fetched.format.into(),
        file_id: fetched.file_id.to_base16(),
        key: fetched.key.map(|key| key.0),
    })?;
    let shelf = shelf.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || shelf.pin(&id, facts, &fetched.file))
        .await
        .context("the download stopped")?
}

/// The bytes a run of hex digits spells, or `None` for anything else.
fn hex(digits: &str) -> Option<Vec<u8>> {
    (0..digits.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok())
        .collect()
}
