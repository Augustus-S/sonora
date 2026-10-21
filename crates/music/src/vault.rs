//! Tracks kept on disk after they have played, so the next play needs no download.
//!
//! A kept track is never a file another program can play. Each one is sealed with
//! ChaCha20-Poly1305 under a key derived from a master key that the OS keychain holds, so the
//! folder read without Sonora, or copied to another machine, is noise. File names are keyed
//! hashes, so the folder does not say which tracks are in it either.
//!
//! What goes in is what the provider sent, before its [`Body`](crate::stream::Body) touched it.
//! A Deezer track stays striped with Blowfish inside the vault, so its decrypted audio still
//! never reaches the disk. A provider whose stream is under DRM would keep its tracks that way
//! too, and peeling off the vault's layer would leave nothing easier to play than the stream.
//!
//! The owner of the machine can always read the master key out of their keychain, or the audio
//! out of the process. The vault only makes the cache no easier to rip than the stream.
//!
//! An entry is the file's random salt, then the track in sealed chunks of [`CHUNK`] bytes, then
//! the provider's facts about the track sealed on their own, then the length of that last block.
//! Every chunk's nonce carries its index and whether it is the last, so a truncated, reordered or
//! swapped entry fails to open rather than playing wrong.

use std::fmt;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result, anyhow, bail};
use bytes::Bytes;
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom as _, SystemRandom};
use ring::{hkdf, hmac};

use crate::stream::Source;

/// How much of a track one sealed chunk holds. Big enough that the tags cost nothing, small
/// enough that a damaged chunk is found long before the decoder needs it.
const CHUNK: usize = 64 * 1024;
/// The bytes of a ChaCha20-Poly1305 tag.
const TAG: usize = 16;
/// The bytes of the random salt every entry starts with.
const SALT: usize = 32;
/// The bytes of the master key.
pub const MASTER: usize = 32;
/// Mixed into every key the vault derives, so a change of layout cannot read an older one.
const VERSION: &[u8] = b"sonora vault 1";
/// What one track may take of the whole vault at most, as a fraction of the limit.
const SHARE: u64 = 4;
/// How long a half-written entry left by a crash stays before a sweep removes it.
const ABANDONED: Duration = Duration::from_secs(24 * 60 * 60);

/// Tells apart the temporary files of two recordings running at once in one process.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// What a sealed block is, which its nonce carries so one cannot stand in for another.
#[derive(Clone, Copy)]
enum Block {
    Chunk = 0,
    Last = 1,
    Facts = 2,
}

/// Every kept track on this machine, under one master key, within one size limit.
///
/// The vault exists before its key does. Until [`unlock`](Self::unlock) runs, nothing is found
/// and nothing is kept, so playback never waits on the keychain.
pub struct Vault {
    root: PathBuf,
    limit: u64,
    keys: OnceLock<hkdf::Prk>,
    /// What the entries take on disk, once a sweep has counted them.
    used: Mutex<Option<u64>>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("root", &self.root)
            .field("limit", &self.limit)
            .field("unlocked", &self.keys.get().is_some())
            .finish()
    }
}

impl Vault {
    /// A vault in `root` that keeps at most `limit` bytes.
    pub fn new(root: PathBuf, limit: u64) -> Self {
        Self {
            root,
            limit,
            keys: OnceLock::new(),
            used: Mutex::new(None),
        }
    }

    /// A vault in Sonora's cache folder, `$XDG_CACHE_HOME/sonora/vault`.
    pub fn standard(limit: u64) -> Self {
        Self::new(crate::credentials::root().join("vault"), limit)
    }

    /// A fresh random master key, for a machine whose keychain has none yet.
    pub fn new_master() -> Result<[u8; MASTER]> {
        let mut master = [0; MASTER];
        SystemRandom::new()
            .fill(&mut master)
            .map_err(|_| anyhow!("cannot draw a random master key"))?;
        Ok(master)
    }

    /// Lets the vault find and keep tracks under `master`. Only the first call counts.
    pub fn unlock(&self, master: &[u8]) {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, VERSION).extract(master);
        if self.keys.set(prk).is_err() {
            log::debug!("vault: already unlocked");
        }
    }

    /// The part of the vault one provider keeps its tracks in. `provider` names its folder and
    /// goes into every key, so no provider can open another's entries.
    pub(crate) fn shelf(self: &Arc<Self>, provider: &'static str) -> Shelf {
        Shelf {
            vault: self.clone(),
            provider,
        }
    }

    /// Removes every kept track.
    pub fn forget(&self) {
        match fs::remove_dir_all(&self.root) {
            Ok(()) => log::info!("vault: forgot every kept track"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => log::warn!("vault: cannot forget the kept tracks: {error}"),
        }
        *self.used.lock().unwrap_or_else(|held| held.into_inner()) = Some(0);
    }

    /// Counts a new entry of `size` bytes and evicts the least recently played ones once the
    /// vault holds more than its limit.
    fn settle(&self, size: u64) {
        let over = {
            let mut used = self.used.lock().unwrap_or_else(|held| held.into_inner());
            *used = used.map(|used| used.saturating_add(size));
            used.is_none_or(|used| used > self.limit)
        };
        if over {
            self.sweep();
        }
    }

    /// Walks every entry, drops what a crash left half-written and the least recently played
    /// entries past the limit, and answers what is left.
    fn sweep(&self) -> u64 {
        let now = SystemTime::now();
        let mut entries = Vec::new();
        for shelf in fs::read_dir(&self.root).into_iter().flatten().flatten() {
            for item in fs::read_dir(shelf.path()).into_iter().flatten().flatten() {
                let Ok(metadata) = item.metadata() else {
                    continue;
                };
                let used = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let partial = item.file_name().to_string_lossy().starts_with('.');
                if partial {
                    if now.duration_since(used).is_ok_and(|age| age > ABANDONED) {
                        fs::remove_file(item.path()).ok();
                    }
                    continue;
                }
                entries.push((item.path(), used, metadata.len()));
            }
        }
        entries.sort_unstable_by_key(|(_, used, _)| *used);
        let mut total = entries.iter().map(|(_, _, size)| size).sum::<u64>();
        for (path, _, size) in entries {
            if total <= self.limit {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        *self.used.lock().unwrap_or_else(|held| held.into_inner()) = Some(total);
        total
    }
}

/// One provider's tracks in the vault.
#[derive(Clone)]
pub(crate) struct Shelf {
    vault: Arc<Vault>,
    provider: &'static str,
}

impl fmt::Debug for Shelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shelf")
            .field("provider", &self.provider)
            .finish()
    }
}

impl Shelf {
    /// The kept copy of track `id` as `variant` describes it, when there is a whole one. An
    /// entry that fails to open is removed, so a damaged file costs one download and no more.
    ///
    /// `variant` is whatever changes the bytes for the same id, such as the quality asked for.
    pub(crate) fn find(&self, id: &str, variant: &str) -> Option<Kept> {
        let prk = self.vault.keys.get()?;
        let name = self.name(prk, id, variant)?;
        let path = self.folder().join(&name);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                log::warn!("vault: cannot open a kept track: {error}");
                return None;
            }
        };
        match self.open(prk, &name, &path, file) {
            Ok(kept) => {
                let now = SystemTime::now();
                if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                    file.set_times(FileTimes::new().set_accessed(now).set_modified(now))
                        .ok();
                }
                Some(kept)
            }
            Err(error) => {
                log::warn!(
                    "vault: dropping a damaged {} track: {error:#}",
                    self.provider
                );
                fs::remove_file(&path).ok();
                None
            }
        }
    }

    /// Wraps `source` so that everything it yields is sealed into the vault as it passes, and
    /// the entry is kept once the source has yielded its last byte. `facts` is whatever the
    /// provider needs to play the track again without asking the network, and comes back
    /// from [`Kept::facts`].
    ///
    /// The source passes through untouched whatever happens to the vault. A disk that refuses
    /// the entry only means the track is not kept.
    pub(crate) fn keep<S: Source>(
        &self,
        id: &str,
        variant: &str,
        facts: Vec<u8>,
        source: S,
    ) -> Recording<S> {
        let pending = match self.begin(id, variant, facts) {
            Ok(pending) => pending,
            Err(error) => {
                log::debug!("vault: not keeping a {} track: {error:#}", self.provider);
                None
            }
        };
        Recording { source, pending }
    }

    fn folder(&self) -> PathBuf {
        self.vault.root.join(self.provider)
    }

    /// The file name of a track, a keyed hash that says nothing to anyone without the key.
    fn name(&self, prk: &hkdf::Prk, id: &str, variant: &str) -> Option<String> {
        let key: hmac::Key = prk
            .expand(&[b"name", self.provider.as_bytes()], hmac::HMAC_SHA256)
            .ok()?
            .into();
        let mut context = hmac::Context::with_key(&key);
        context.update(id.as_bytes());
        context.update(&[0]);
        context.update(variant.as_bytes());
        let tag = context.sign();
        Some(hex(&tag.as_ref()[..16]))
    }

    /// The key that seals one entry, from the master key, this shelf and the entry's salt.
    fn key(&self, prk: &hkdf::Prk, salt: &[u8]) -> Result<LessSafeKey> {
        let info = [b"track".as_slice(), self.provider.as_bytes(), salt];
        let okm = prk
            .expand(&info, &CHACHA20_POLY1305)
            .map_err(|_| anyhow!("cannot derive a track key"))?;
        Ok(LessSafeKey::new(UnboundKey::from(okm)))
    }

    /// Opens a temporary file for a new entry, or answers `None` while the vault is locked.
    fn begin(&self, id: &str, variant: &str, facts: Vec<u8>) -> Result<Option<Pending>> {
        let Some(prk) = self.vault.keys.get() else {
            return Ok(None);
        };
        let name = self
            .name(prk, id, variant)
            .context("cannot name the entry")?;
        let folder = self.folder();
        create_private(&self.vault.root)?;
        create_private(&folder)?;
        let mut salt = [0; SALT];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| anyhow!("cannot draw a salt"))?;
        let key = self.key(prk, &salt)?;
        let temporary = folder.join(format!(
            ".{name}.{}.{}",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = private_file(&temporary)?;
        file.write_all(&salt).context("cannot start the entry")?;
        Ok(Some(Pending {
            vault: self.vault.clone(),
            path: folder.join(&name),
            temporary,
            name,
            file,
            key,
            facts,
            buffer: Vec::with_capacity(CHUNK + TAG),
            index: 0,
            written: SALT as u64,
            ceiling: self.vault.limit / SHARE,
        }))
    }

    /// Reads the salt and the facts of an entry, and leaves a source at its first chunk.
    fn open(&self, prk: &hkdf::Prk, name: &str, path: &Path, mut file: File) -> Result<Kept> {
        let size = file.metadata()?.len();
        let mut salt = [0; SALT];
        file.read_exact(&mut salt).context("cannot read the salt")?;
        let key = self.key(prk, &salt)?;
        if size < (SALT + TAG + 4) as u64 {
            bail!("the entry is too short");
        }
        let mut length = [0; 4];
        file.seek(SeekFrom::End(-4))?;
        file.read_exact(&mut length)?;
        let sealed = u64::from(u32::from_le_bytes(length));
        let facts_at = size
            .checked_sub(4 + sealed)
            .filter(|at| *at >= SALT as u64)
            .context("the entry's facts run past its start")?;
        let mut facts = vec![0; sealed as usize];
        file.seek(SeekFrom::Start(facts_at))?;
        file.read_exact(&mut facts)?;
        let opened = open(&key, name, Block::Facts, 0, &mut facts)?;
        let (length, facts) = opened
            .split_first_chunk::<8>()
            .context("the entry's facts have no length")?;
        let length = u64::from_le_bytes(*length);
        let facts = facts.to_vec();
        file.seek(SeekFrom::Start(SALT as u64))?;
        Ok(Kept {
            facts,
            length,
            source: Sealed {
                file,
                key,
                name: name.to_owned(),
                path: path.to_owned(),
                index: 0,
                left: facts_at - SALT as u64,
                buffer: Vec::with_capacity(CHUNK + TAG),
            },
        })
    }
}

/// A kept track, ready to be played again.
pub(crate) struct Kept {
    /// What the provider stored beside the track when it was kept.
    pub(crate) facts: Vec<u8>,
    /// The length of the track as the provider sent it.
    pub(crate) length: u64,
    /// The track, chunk by chunk, as the provider sent it.
    pub(crate) source: Sealed,
}

/// The bytes of a kept track, opened a chunk at a time as the stream asks for them. A chunk
/// that fails to open removes the entry, so the next play downloads the track again.
pub(crate) struct Sealed {
    file: File,
    key: LessSafeKey,
    name: String,
    /// Where the entry lies, so a chunk that fails to open can take the entry with it.
    path: PathBuf,
    index: u64,
    /// The sealed bytes still to read.
    left: u64,
    buffer: Vec<u8>,
}

impl Source for Sealed {
    async fn chunk(&mut self) -> Result<Option<Bytes>> {
        let chunk = self.open_next();
        if let Err(error) = &chunk {
            log::warn!("vault: dropping a damaged track: {error:#}");
            fs::remove_file(&self.path).ok();
        }
        chunk
    }
}

impl Sealed {
    /// Reads and opens the next chunk, or answers `None` once there are none left.
    fn open_next(&mut self) -> Result<Option<Bytes>> {
        if self.left == 0 {
            return Ok(None);
        }
        let take = self.left.min((CHUNK + TAG) as u64) as usize;
        self.buffer.resize(take, 0);
        self.file
            .read_exact(&mut self.buffer)
            .context("cannot read a kept track")?;
        self.left -= take as u64;
        let block = match self.left {
            0 => Block::Last,
            _ => Block::Chunk,
        };
        let opened = open(&self.key, &self.name, block, self.index, &mut self.buffer)?;
        self.index += 1;
        Ok(Some(Bytes::copy_from_slice(opened)))
    }
}

/// A source whose bytes are sealed into the vault as they pass through.
pub(crate) struct Recording<S> {
    source: S,
    /// The entry being written, until it is kept or given up on.
    pending: Option<Pending>,
}

impl<S> Recording<S> {
    /// A source that passes through without keeping anything, for a provider with no shelf.
    pub(crate) fn through(source: S) -> Self {
        Self {
            source,
            pending: None,
        }
    }
}

impl<S: Source> Source for Recording<S> {
    async fn chunk(&mut self) -> Result<Option<Bytes>> {
        let chunk = self.source.chunk().await?;
        if let Some(mut pending) = self.pending.take() {
            let written = match &chunk {
                Some(bytes) => pending.write(bytes).map(|()| Some(pending)),
                None => pending.commit().map(|()| None),
            };
            match written {
                Ok(pending) => self.pending = pending,
                Err(error) => log::warn!("vault: cannot keep the track: {error:#}"),
            }
        }
        Ok(chunk)
    }
}

/// An entry being written. Dropping it before [`commit`](Self::commit) removes the file, so
/// a skipped or broken track leaves nothing behind.
struct Pending {
    vault: Arc<Vault>,
    path: PathBuf,
    temporary: PathBuf,
    name: String,
    file: File,
    key: LessSafeKey,
    facts: Vec<u8>,
    /// What has arrived since the last sealed chunk, and the room to seal it in.
    buffer: Vec<u8>,
    index: u64,
    /// The bytes of the track the provider has sent so far, salt included.
    written: u64,
    /// The most one entry may take before it is given up on.
    ceiling: u64,
}

impl Pending {
    /// Seals every whole chunk that has built up. One is always held back, so the last chunk
    /// sealed is never empty unless the whole track is.
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.written += bytes.len() as u64;
        if self.written > self.ceiling {
            bail!("the track is larger than one entry may be");
        }
        let mut bytes = bytes;
        while !bytes.is_empty() {
            let room = CHUNK + 1 - self.buffer.len().min(CHUNK + 1);
            let (head, rest) = bytes.split_at(room.min(bytes.len()));
            self.buffer.extend_from_slice(head);
            bytes = rest;
            if self.buffer.len() > CHUNK {
                let spill = self.buffer.split_off(CHUNK);
                self.seal(Block::Chunk)?;
                self.buffer = spill;
            }
        }
        Ok(())
    }

    /// Seals what is held as the last chunk, then the facts, and moves the entry into place.
    fn commit(mut self) -> Result<()> {
        let length = self.written - SALT as u64;
        self.seal(Block::Last)?;
        let mut facts = Vec::with_capacity(8 + self.facts.len() + TAG);
        facts.extend_from_slice(&length.to_le_bytes());
        facts.extend_from_slice(&self.facts);
        self.key
            .seal_in_place_append_tag(
                nonce(Block::Facts, 0),
                Aad::from(self.name.as_bytes()),
                &mut facts,
            )
            .map_err(|_| anyhow!("cannot seal the facts"))?;
        let sealed = u32::try_from(facts.len()).context("the facts are too long")?;
        self.file.write_all(&facts)?;
        self.file.write_all(&sealed.to_le_bytes())?;
        self.file.flush()?;
        let size = self.file.metadata()?.len();
        fs::rename(&self.temporary, &self.path).context("cannot move the entry into place")?;
        log::debug!(
            "vault: kept a track, {:.1} MiB",
            size as f64 / (1024.0 * 1024.0)
        );
        self.vault.settle(size);
        Ok(())
    }

    fn seal(&mut self, block: Block) -> Result<()> {
        self.key
            .seal_in_place_append_tag(
                nonce(block, self.index),
                Aad::from(self.name.as_bytes()),
                &mut self.buffer,
            )
            .map_err(|_| anyhow!("cannot seal a chunk"))?;
        self.file
            .write_all(&self.buffer)
            .context("cannot write a chunk")?;
        self.buffer.clear();
        self.index += 1;
        Ok(())
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        // Once the entry is in place the temporary name is gone and this finds nothing.
        fs::remove_file(&self.temporary).ok();
    }
}

/// The nonce of one sealed block. Every entry has its own key, so the index alone keeps a
/// nonce from repeating.
fn nonce(block: Block, index: u64) -> Nonce {
    let mut nonce = [0; NONCE_LEN];
    nonce[..8].copy_from_slice(&index.to_be_bytes());
    nonce[8] = block as u8;
    Nonce::assume_unique_for_key(nonce)
}

/// Opens one sealed block in place and answers the plain bytes inside it.
fn open<'a>(
    key: &LessSafeKey,
    name: &str,
    block: Block,
    index: u64,
    sealed: &'a mut [u8],
) -> Result<&'a mut [u8]> {
    key.open_in_place(nonce(block, index), Aad::from(name.as_bytes()), sealed)
        .map_err(|_| anyhow!("a sealed block does not open"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Creates `path` readable by the owner alone, when it does not exist yet.
fn create_private(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(path)
        .with_context(|| format!("cannot create {}", path.display()))
}

/// A new file readable and writable by the owner alone.
fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .with_context(|| format!("cannot create {}", path.display()))
}
