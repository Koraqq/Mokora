use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BotConfig {
    /// Master switch. When `false`, no `BotManager` is spawned and the
    /// MJAI event bus runs without a consumer.
    pub enabled: bool,
    /// Active bot for 4-player (yonma) games. Subdirectory of `dir`.
    ///
    /// Reads the legacy `active` key on first load (see `migrate_legacy_active`)
    /// so existing config files keep working.
    pub active_4p: String,
    /// Active bot for 3-player (sanma) games. Empty string ⇒ no bot
    /// configured for 3p (analysis-only mode in 3p matches).
    pub active_3p: String,
    /// Legacy field, kept for one release for migration purposes. New code
    /// reads `active_4p` / `active_3p`. If the on-disk config has only
    /// `active` set, `active_4p` is populated from it during deserialise.
    #[serde(skip_serializing)]
    pub active: String,
    /// Run `uv sync` automatically before spawning the bot. Disabling
    /// makes startup faster on slow disks but assumes the venv is
    /// already in sync — usually for advanced users.
    pub auto_sync: bool,
    /// Root directory containing one subdir per bot. Resolved with the
    /// same fallback chain as other directory configs (`util::resolve_dir`).
    pub dir: String,
}

impl BotConfig {
    /// Pick the active bot name for the given player count. 3 ⇒ `active_3p`,
    /// anything else ⇒ `active_4p`.
    pub fn active_for(&self, num_players: u8) -> &str {
        if num_players == 3 {
            &self.active_3p
        } else {
            &self.active_4p
        }
    }

    /// Give the built-in bots their current names (`mora-native3p` →
    /// `mora-3p`, `mokora-4p` → `mora-4p`, see `bot::native::renamed`); written
    /// back with the next save.
    pub fn migrate_legacy_names(&mut self) {
        for name in [&mut self.active_4p, &mut self.active_3p] {
            if let Some(now) = crate::bot::native::renamed(name) {
                *name = now.to_string();
            }
        }
    }

    /// Migrate the legacy `active` field into `active_4p` if the user's
    /// config file predates the per-mode split.
    pub fn migrate_legacy_active(&mut self) {
        if !self.active.is_empty() && self.active_4p.is_empty() {
            self.active_4p = std::mem::take(&mut self.active);
        } else {
            // Drop any legacy value we read; future writes won't include it.
            self.active.clear();
        }
    }
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            // Enabled by default: the built-in native bots (below) are always
            // available and need no install, so a fresh install shows bot
            // recommendations out of the box.
            enabled: true,
            // Built-in, pure-Rust default bots (no Python / libriichi). See
            // `crate::bot::native`. They are always available and need no
            // install, so they make sensible out-of-the-box defaults.
            active_4p: crate::bot::native::MORA_4P.to_string(),
            active_3p: crate::bot::native::MORA_3P.to_string(),
            active: String::new(),
            auto_sync: true,
            dir: "mjai_bot".to_string(),
        }
    }
}
