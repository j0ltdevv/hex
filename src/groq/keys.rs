//! Credential sources for Groq, and the owner-only key file.
//!
//! Resolution order is `GROQ_API_KEY`, then `groq.key` under Hex's data root,
//! then the legacy `groq_api_key` setting. A run with an overridden data root
//! and `HEX_GROQ_ALLOW_SYSTEM_KEYS=0` ignores process-level credentials (the
//! environment) but keeps the key file inside that root and the saved setting.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::GroqApiKey;

pub(crate) const KEY_FILE_NAME: &str = "groq.key";
const MAX_KEY_BYTES: u64 = 4096;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

/// The key file for this Hex data root.
pub(crate) fn key_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE_NAME)
}

/// Writes a trimmed key in a file only its owner can read.
#[allow(dead_code)]
pub(crate) fn write_key_file(path: &Path, key: &GroqApiKey) -> io::Result<()> {
    let key = key.as_str().trim();
    if key.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the API key is empty",
        ));
    }
    if let Some(parent) = path.parent() {
        crate::app_paths::create_private_dir(parent)?;
    }

    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(key.as_bytes())?;
    file.flush()?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Reads a data-root key file only when it is a small, owner-only regular file.
/// On Unix the descriptor is opened with `O_NOFOLLOW` and checked with
/// `fstat`, so a symlink swap after a path check cannot borrow a key.
pub(crate) fn read_key_file(path: &Path) -> Option<GroqApiKey> {
    #[cfg(unix)]
    let mut file = {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        options.open(path).ok()?
    };
    #[cfg(not(unix))]
    let mut file = fs::File::open(path).ok()?;

    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_KEY_BYTES {
        return None;
    }
    #[cfg(unix)]
    {
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::getuid() } {
            return None;
        }
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize + 1);
    file.read_to_end(&mut bytes).ok()?;
    nonempty_key(std::str::from_utf8(&bytes).ok()?)
}

/// Picks the first trimmed, nonempty credential in the configured order.
pub(crate) fn resolve_key(
    env: Option<&str>,
    file: Option<&str>,
    saved: Option<&str>,
    allow_system: bool,
) -> Option<GroqApiKey> {
    let env = if allow_system { env } else { None };
    env.into_iter()
        .chain(file)
        .chain(saved)
        .map(str::trim)
        .find(|key| !key.is_empty())
        .map(GroqApiKey::from)
}

#[allow(dead_code)]
pub(crate) fn system_keys_allowed() -> bool {
    let data_root_overridden = std::env::var_os("HEX_APPLICATION_SUPPORT_DIR")
        .filter(|value| !value.is_empty())
        .is_some();
    if !data_root_overridden {
        return true;
    }
    std::env::var("HEX_GROQ_ALLOW_SYSTEM_KEYS")
        .ok()
        .is_none_or(|value| value.trim() != "0")
}

/// Where the active credential came from, with no key bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum KeyStatus {
    #[default]
    None,
    Environment,
    KeyFile,
    Settings,
    /// An environment key exists but isolation rules suppress it.
    Blocked,
}

struct Sources {
    env: Option<GroqApiKey>,
    file: Option<GroqApiKey>,
    saved: Option<GroqApiKey>,
    allow_system: bool,
}

impl Sources {
    fn current() -> Self {
        let env = std::env::var("GROQ_API_KEY")
            .ok()
            .and_then(|key| nonempty_key(&key));
        let file = crate::app_paths::support_dir()
            .ok()
            .and_then(|dir| read_key_file(&key_file_path(&dir)));
        Self {
            env,
            file,
            saved: super::saved_key(),
            allow_system: system_keys_allowed(),
        }
    }

    fn resolve(&self) -> Option<GroqApiKey> {
        resolve_key(
            self.env.as_ref().map(GroqApiKey::as_str),
            self.file.as_ref().map(GroqApiKey::as_str),
            self.saved.as_ref().map(GroqApiKey::as_str),
            self.allow_system,
        )
    }
}

/// The effective key, without exposing it in logs or diagnostics.
pub(crate) fn active_key() -> Option<GroqApiKey> {
    Sources::current().resolve()
}

/// Status-only view of the current key sources.
#[allow(dead_code)]
pub(crate) fn key_status() -> KeyStatus {
    let sources = Sources::current();
    let env = sources.env.as_ref().filter(|_| sources.allow_system);
    if env.is_some() {
        return KeyStatus::Environment;
    }
    if sources.file.is_some() {
        return KeyStatus::KeyFile;
    }
    if sources.saved.is_some() {
        return KeyStatus::Settings;
    }
    if sources.env.is_some() {
        return KeyStatus::Blocked;
    }
    KeyStatus::None
}

fn nonempty_key(text: &str) -> Option<GroqApiKey> {
    let text = text.trim();
    (!text.is_empty()).then(|| GroqApiKey::from(text))
}
