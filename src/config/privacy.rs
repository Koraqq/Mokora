use serde::{Deserialize, Serialize};

/// What Mokora lets leave the machine, beyond what playing needs.
///
/// Gathered in one place so the user can see what Mokora writes about the
/// game's traffic. None of these change how a game is read or played: capture
/// still sees every frame. The game client's own telemetry is switched by
/// `[proxy] block_telemetry` (it predates this section, and moving it would
/// reset the choice in every existing config file); the Settings page shows
/// it here alongside the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Mask login credentials (access tokens, passwords, connect tokens) in
    /// everything Mokora writes about the game's traffic: the app log, the
    /// per-connection protocol logs, and the inspector timeline. On by
    /// default. Anyone holding a token can sign in as you, and logs are what
    /// people paste when they report a problem. Parsing is unaffected — only
    /// the written copies are masked.
    pub redact_credentials: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            redact_credentials: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config written before the section existed gains the protective
    /// default, not the permissive one.
    #[test]
    fn missing_section_defaults_to_protective() {
        let cfg: PrivacyConfig = toml::from_str("").unwrap();
        assert!(cfg.redact_credentials);
    }

    /// The upload switches of the cloud-inference days are not read any more,
    /// but a config that still has them loads.
    #[test]
    fn old_upload_switches_are_ignored() {
        let cfg: PrivacyConfig =
            toml::from_str("share_player_names = true\nallow_review_upload = false").unwrap();
        assert!(cfg.redact_credentials);
    }
}
