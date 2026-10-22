//! "Don't remind me again" for the update prompt.
//!
//! Kept in `accounts/update.json` beside the other app-wide choices
//! (`startup.json`, `ui.json`), not in an account's config: it is one
//! answer for the whole app. What is stored is the version the user was
//! running when they asked — the reminder stays off while that version is
//! the one installed, and comes back by itself once a newer one is, since
//! the running version then no longer matches.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "update.json";

#[derive(Debug, Serialize, Deserialize)]
struct Reminders {
    /// The version that was running when reminders were turned off.
    muted_on_version: String,
}

fn path_in(accounts_root: &Path) -> PathBuf {
    accounts_root.join(FILE)
}

fn default_root() -> PathBuf {
    crate::util::resolve_dir(Path::new(crate::accounts::ACCOUNTS_DIR))
}

/// Whether the user turned update reminders off on this version.
pub fn muted() -> bool {
    muted_in(&default_root(), env!("CARGO_PKG_VERSION"))
}

/// Turn update reminders off until a newer version is installed.
pub fn mute() -> Result<()> {
    mute_in(&default_root(), env!("CARGO_PKG_VERSION"))
}

fn muted_in(accounts_root: &Path, running: &str) -> bool {
    std::fs::read(path_in(accounts_root))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Reminders>(&bytes).ok())
        .is_some_and(|r| r.muted_on_version == running)
}

fn mute_in(accounts_root: &Path, running: &str) -> Result<()> {
    std::fs::create_dir_all(accounts_root)
        .with_context(|| format!("creating {}", accounts_root.display()))?;
    let path = path_in(accounts_root);
    let body = serde_json::to_vec(&Reminders {
        muted_on_version: running.to_string(),
    })?;
    crate::util::write_atomic(&path, body).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminders_are_on_until_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!muted_in(dir.path(), "0.3.0"));
        mute_in(dir.path(), "0.3.0").unwrap();
        assert!(muted_in(dir.path(), "0.3.0"));
    }

    /// Installing a newer version brings the reminder back by itself.
    #[test]
    fn a_new_install_brings_reminders_back() {
        let dir = tempfile::tempdir().unwrap();
        mute_in(dir.path(), "0.3.0").unwrap();
        assert!(!muted_in(dir.path(), "0.3.1"));
    }

    #[test]
    fn an_unreadable_file_leaves_reminders_on() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert!(!muted_in(dir.path(), "0.3.0"));
    }

    #[test]
    fn muting_creates_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("accounts");
        mute_in(&root, "0.3.0").unwrap();
        assert!(muted_in(&root, "0.3.0"));
    }
}
