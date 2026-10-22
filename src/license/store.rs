//! The license kept on this computer: `accounts/license.json`, beside the
//! other app-wide choices (`update.json`, `startup.json`). One license for
//! every account.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "license.json";

/// The signed-in account on this computer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Stored {
    /// The account's email (kept after signing out, to fill the form).
    pub email: Option<String>,
    /// The session's refresh token; None when signed out. It changes at
    /// every renewal and must be saved before the next one.
    pub refresh: Option<String>,
    /// The last license token.
    pub token: Option<String>,
    /// The account as last reported.
    pub account: Option<crate::license::client::AccountView>,
    /// Why the server stopped this computer (locked, blocked), if it did.
    #[serde(default)]
    pub problem: Option<String>,
    /// Device tokens by email: a computer that signed in with a code signs
    /// in again without one for 7 days. Kept across signing out.
    #[serde(default)]
    pub devices: std::collections::BTreeMap<String, String>,
}

/// `accounts/license.json`; pointed at another server
/// (`MOKORA_LICENSE_SERVER`), `accounts/license.test.json`, so the two
/// sessions never mix.
pub fn default_path() -> PathBuf {
    let file = if std::env::var_os("MOKORA_LICENSE_SERVER").is_some() {
        "license.test.json"
    } else {
        FILE
    };
    crate::util::resolve_dir(Path::new(crate::accounts::ACCOUNTS_DIR)).join(file)
}

pub fn load(path: &Path) -> Option<Stored> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(path: &Path, stored: &Stored) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(stored)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))
}

pub fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join(FILE);
        assert_eq!(load(&path), None);
        let s = Stored {
            email: Some("me@example.com".into()),
            refresh: Some("r".into()),
            token: Some("t".into()),
            account: None,
            problem: None,
            devices: Default::default(),
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), Some(s));
        remove(&path).unwrap();
        remove(&path).unwrap();
        assert_eq!(load(&path), None);
    }
}
