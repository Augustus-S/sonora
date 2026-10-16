//! What one engine knows about the tracks it plays, for the signal path: how each is encoded,
//! what it decodes to, and what normalisation does to it.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

/// How many tracks' facts are kept: the one playing, the one queued behind it, and room for a
/// skip that has not been heard yet.
const TRACKS: usize = 4;

/// Shared by an engine and the UI, newest track last.
#[derive(Clone, Default)]
pub struct Route(Arc<Mutex<VecDeque<(String, TrackSignal)>>>);

/// What an engine knows about one track.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackSignal {
    /// How the provider encodes it, when it says.
    pub encoding: Option<Encoding>,
    /// The rate and channel count it decodes to, before Sonora resamples it to the device's.
    pub rate: u32,
    pub channels: u16,
    pub normalisation: Normalisation,
}

/// What loudness normalisation does to a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Normalisation {
    Off,
    /// A fixed gain in decibels, zero when nothing says how loud the track is.
    Gain(f32),
    /// librespot normalises inside its own player with a limiter, so the gain is not known
    /// and changes with the music.
    Limited,
}

/// How a track is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub codec: Codec,
    /// The bits per sample of a lossless encoding.
    pub bits: Option<u8>,
    /// The bit rate of a lossy encoding, in kilobits a second.
    pub kbps: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Flac,
    Alac,
    /// Uncompressed, as in WAV or AIFF.
    Pcm,
    Mp3,
    Aac,
    Vorbis,
    Opus,
    Other,
}

impl Route {
    /// What is known about track `id`, if it played or is about to.
    pub fn track(&self, id: &str) -> Option<TrackSignal> {
        let tracks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        tracks
            .iter()
            .rev()
            .find(|(known, _)| known == id)
            .map(|(_, track)| track.clone())
    }

    /// Notes what track `id` is, as it starts decoding. Never called from the audio callback.
    pub(crate) fn learn(&self, id: &str, track: TrackSignal) {
        let mut tracks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        tracks.retain(|(known, _)| known != id);
        if tracks.len() == TRACKS {
            tracks.pop_front();
        }
        tracks.push_back((id.to_owned(), track));
    }
}
