//! Discovery of bot directories under `./mjai_bot/`.
//!
//! A *bot* is any direct subdirectory of the registry root that contains a
//! `bot.py` file. Hidden dirs (`.foo`), the reserved `base/` name (kept for
//! parity with v2), and entries lacking `bot.py` are ignored.
//!
//! No Python execution happens here — registry only enumerates layout. The
//! `BotRunner` / `PythonRuntime` machinery is responsible for actually
//! launching anything.

use crate::bot::manifest::Manifest;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// One discovered bot.
#[derive(Debug, Clone, PartialEq)]
pub struct BotEntry {
    /// Subdirectory name (the user-facing identifier, e.g. `"mortal"`).
    pub name: String,
    /// Absolute path to the bot directory.
    pub dir: PathBuf,
    /// Path to `pyproject.toml` if present, else `None`.
    pub pyproject: Option<PathBuf>,
    /// Parsed `manifest.toml` if the bot ships one. Bots without a
    /// manifest still appear in the registry — they just lack any
    /// configurable settings.
    pub manifest: Option<Manifest>,
}

/// Snapshot of bot directories at scan time.
///
/// Cheap to clone (small struct + `Vec<BotEntry>`); rescan when the user
/// drops a new bot folder in, rather than holding watchers.
#[derive(Debug, Clone, Default)]
pub struct BotRegistry {
    root: PathBuf,
    entries: Vec<BotEntry>,
}

impl BotRegistry {
    /// Walk `root` and collect every subdir that contains a `bot.py`.
    ///
    /// `root` not existing is *not* an error — first-run users may not have
    /// created `mjai_bot/` yet. An empty registry is returned in that case.
    pub fn scan(root: &Path) -> Result<Self> {
        let root = root.to_path_buf();
        let mut entries = Vec::new();

        if !root.exists() {
            return Ok(Self { root, entries });
        }

        let read =
            std::fs::read_dir(&root).with_context(|| format!("read_dir {}", root.display()))?;

        for item in read {
            let item = item.with_context(|| format!("dir entry under {}", root.display()))?;
            let path = item.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.starts_with('.') || name.starts_with("__") || name == "base" {
                continue;
            }
            let bot_py = path.join("bot.py");
            if !bot_py.is_file() {
                continue;
            }
            let pyproject = path.join("pyproject.toml");
            let manifest = match Manifest::load(&path) {
                Ok(m) => m,
                Err(e) => {
                    // A malformed manifest shouldn't yank the bot off the
                    // list — just log and continue without one. The user
                    // sees the bot but can't open its settings panel.
                    warn!(
                        bot = %name,
                        "failed to load manifest.toml: {e:#}",
                    );
                    None
                }
            };
            entries.push(BotEntry {
                name: name.to_owned(),
                dir: path.clone(),
                pyproject: pyproject.is_file().then_some(pyproject),
                manifest,
            });
        }

        // Stable, alphabetical order so UI lists don't reshuffle.
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { root, entries })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[BotEntry] {
        &self.entries
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    pub fn find(&self, name: &str) -> Option<&BotEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"").unwrap();
    }

    #[test]
    fn missing_root_returns_empty_not_error() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does_not_exist");
        let reg = BotRegistry::scan(&missing).unwrap();
        assert!(reg.entries().is_empty());
        assert_eq!(reg.root(), missing);
    }

    #[test]
    fn empty_root_yields_no_entries() {
        let tmp = TempDir::new().unwrap();
        let reg = BotRegistry::scan(tmp.path()).unwrap();
        assert!(reg.entries().is_empty());
    }

    #[test]
    fn detects_bot_with_and_without_pyproject() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("alpha").join("bot.py"));
        touch(&tmp.path().join("beta").join("bot.py"));
        touch(&tmp.path().join("beta").join("pyproject.toml"));

        let reg = BotRegistry::scan(tmp.path()).unwrap();
        let names: Vec<_> = reg.names().collect();
        assert_eq!(names, vec!["alpha", "beta"]);

        assert!(reg.find("alpha").unwrap().pyproject.is_none());
        assert!(reg.find("beta").unwrap().pyproject.is_some());
    }

    #[test]
    fn skips_hidden_reserved_and_botless_dirs() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join(".hidden").join("bot.py"));
        touch(&tmp.path().join("__pycache__").join("bot.py"));
        touch(&tmp.path().join("base").join("bot.py"));
        touch(&tmp.path().join("no_bot").join("README.md"));
        touch(&tmp.path().join("real").join("bot.py"));

        let reg = BotRegistry::scan(tmp.path()).unwrap();
        let names: Vec<_> = reg.names().collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn entries_sorted_alphabetically() {
        let tmp = TempDir::new().unwrap();
        for n in ["zulu", "alpha", "mike"] {
            touch(&tmp.path().join(n).join("bot.py"));
        }
        let reg = BotRegistry::scan(tmp.path()).unwrap();
        let names: Vec<_> = reg.names().collect();
        assert_eq!(names, vec!["alpha", "mike", "zulu"]);
    }

    #[test]
    fn ignores_files_named_bot_py_at_root() {
        // bot.py directly in root is NOT a bot — bots are subdirs.
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("bot.py"), b"").unwrap();
        let reg = BotRegistry::scan(tmp.path()).unwrap();
        assert!(reg.entries().is_empty());
    }
}
