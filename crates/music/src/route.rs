//! What an engine knows about each track it plays, for the signal path: how it is encoded, what
//! it decodes to, and what normalisation does to it. Engines report it as
//! `PlaybackEvent::Signal`.

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
