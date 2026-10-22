use serde::{Deserialize, Serialize};

use crate::schema;

/// Game platform whose traffic the proxy intercepts. Determines which
/// [`crate::bridge::Bridge`] is instantiated per WebSocket flow.
///
/// Only Mahjong Soul is supported. To add a game: a variant here, its arms in
/// [`Platform::subdir`], `From<Platform>` below, `bridge::for_platform` and
/// `AutoplayManager::handle_bot_response`, and the entry in the frontend's
/// platform list (`lib/platforms.ts`). A config naming a platform this build
/// does not know (an older Tenhou or Riichi City one) reads as Mahjong Soul.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    #[default]
    #[serde(other)]
    Majsoul,
}

impl Platform {
    /// Short name used as a subdirectory under the log session
    /// (e.g. `<session>/<subdir>/<flow>.log`).
    pub fn subdir(self) -> &'static str {
        match self {
            Platform::Majsoul => "majsoul",
        }
    }
}

/// Map the bridge selector to its history-record tag. Keep the two enums
/// separate: the schema enum is the stored data format (it still names
/// Tenhou and Riichi City so records of them keep reading) and has variants
/// (`Mjai`, `Unknown`) that don't make sense as a runtime bridge selector.
impl From<Platform> for schema::Platform {
    fn from(p: Platform) -> Self {
        match p {
            Platform::Majsoul => schema::Platform::Majsoul,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlatformConfig {
    pub kind: Platform,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_to_schema_platform_round_trip() {
        assert_eq!(
            schema::Platform::from(Platform::Majsoul),
            schema::Platform::Majsoul
        );
    }

    /// A config written when other games were supported still loads: the
    /// platform it names reads as Mahjong Soul.
    #[test]
    fn an_old_platform_name_reads_as_majsoul() {
        for name in ["Tenhou", "RiichiCity", "SomethingNew"] {
            let cfg: PlatformConfig = toml::from_str(&format!("kind = \"{name}\"")).unwrap();
            assert_eq!(cfg.kind, Platform::Majsoul, "{name}");
        }
        let cfg: PlatformConfig = toml::from_str("kind = \"Majsoul\"").unwrap();
        assert_eq!(cfg.kind, Platform::Majsoul);
    }
}
