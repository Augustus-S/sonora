//! The master key of the vault played tracks are kept in.
//!
//! The key lives in the OS keychain, so a kept track opens only for this user on this machine.
//! A desktop with no keychain service, such as a bare Wayland compositor with no Secret Service
//! running, gets a key file readable by the owner alone instead. That still keeps the tracks
//! from playing anywhere but Sonora, though anyone who copies both folders can open them.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use gpui::{App, AppContext as _, AsyncApp, Task};
use music::vault::{MASTER, Vault};

/// The keychain entry the master key is stored under.
const KEYCHAIN_URL: &str = "sonora://kept-tracks";
const KEYCHAIN_USER: &str = "sonora";
/// What the kept tracks may take on disk.
pub(crate) const LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Finds the master key, or makes and stores one on a first run, and unlocks `vault` with it.
/// Until the task ends the vault keeps nothing, and a keychain that refuses leaves it locked.
pub(crate) fn unlock(vault: Arc<Vault>, cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx| match master(cx).await {
        Ok(master) => {
            vault.unlock(&master);
            log::info!("vault: unlocked");
        }
        Err(error) => log::warn!("vault: stays locked, no track is kept: {error:#}"),
    })
}

/// The master key from the keychain, or from the key file when there is no keychain to ask.
async fn master(cx: &mut AsyncApp) -> Result<Vec<u8>> {
    let read = cx.update(|cx| cx.read_credentials(KEYCHAIN_URL)).await;
    match read {
        Ok(Some((_, master))) if master.len() == MASTER => Ok(master),
        Ok(Some(_)) => bail!("the keychain holds a key of the wrong length"),
        Ok(None) => {
            let master = Vault::new_master()?;
            let written = cx
                .update(|cx| cx.write_credentials(KEYCHAIN_URL, KEYCHAIN_USER, &master))
                .await;
            match written {
                Ok(()) => Ok(master.to_vec()),
                Err(error) => {
                    log::warn!("vault: the keychain will not store a key, using a file: {error:#}");
                    cx.background_spawn(async { from_file() }).await
                }
            }
        }
        Err(error) => {
            log::warn!("vault: no keychain to ask, using a file: {error:#}");
            cx.background_spawn(async { from_file() }).await
        }
    }
}

/// The master key from a file readable by the owner alone, written on first use.
fn from_file() -> Result<Vec<u8>> {
    let path = key_file().context("cannot find a folder for the vault key")?;
    match std::fs::read(&path) {
        Ok(master) if master.len() == MASTER => return Ok(master),
        Ok(_) => bail!("the vault key file has the wrong length"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("cannot read the vault key file"),
    }
    let master = Vault::new_master()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("cannot create the vault key folder")?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&path)
        .context("cannot create the vault key file")?;
    std::io::Write::write_all(&mut file, &master).context("cannot write the vault key file")?;
    Ok(master.to_vec())
}

/// The key file sits in the data folder rather than beside the tracks in the cache folder, so
/// clearing or copying the cache alone never carries the key with it.
fn key_file() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("sonora").join("vault.key"))
}
