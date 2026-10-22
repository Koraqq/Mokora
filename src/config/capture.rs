//! Capture-mode configuration: which transport supplies WebSocket frames
//! to the bridge layer.
//!
//! Three modes:
//! - `Embedded` (the default where it exists): the game shown inside
//!   Mokora's own window. Windows reads the window's WebView2 over DevTools
//!   (`capture::chromium::embedded`), Linux its WebKitGTK view through WebKit
//!   (`capture::webkit`). macOS (WKWebView) has neither yet: there `Chromium`
//!   is the default, and a config naming `embedded` reads as it.
//! - `Mitm`: hudsucker MITM proxy — see `[proxy]`.
//! - `Chromium`: a Chromium browser launched and controlled by Mokora via
//!   the Chrome DevTools Protocol.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub mode: CaptureMode,
    pub chromium: ChromiumConfig,
    pub http: HttpCaptureConfig,
}

/// What to record of the HTTP traffic a backend intercepts.
///
/// Mokora used to record WebSocket frames and discard everything else,
/// which hid the game's own HTTP entirely — route topology, version
/// endpoints, and the analytics beacons through which the client reports
/// on itself. This turns that back on.
///
/// `record_all` defaults to **off** on purpose. Full HTTP capture puts
/// access tokens, cookies and authorization headers into the session
/// file, and nothing is redacted — redaction would re-create the blind
/// spot this exists to remove. So the default keeps only the exchanges a
/// recognizer understood (analytics beacons, and Mokora's own notes about
/// traffic it declined to intercept), and the firehose is opt-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpCaptureConfig {
    /// Record every intercepted exchange, not just recognized ones.
    /// Off by default — see the struct docs.
    pub record_all: bool,
    /// Keep textual request/response bodies within `max_body_bytes`.
    /// Only consulted when the exchange is being recorded at all.
    pub bodies: bool,
    /// Ceiling on a buffered body. Larger ones are recorded with their
    /// size and the reason they were skipped, never truncated silently.
    pub max_body_bytes: usize,
    /// Chromium backend only: also record static subresources (images,
    /// fonts, media, stylesheets). Off by default — a WebGL client pulls
    /// enough of them to bury everything else, and none of it says
    /// anything about the client.
    pub static_assets: bool,
}

impl Default for HttpCaptureConfig {
    fn default() -> Self {
        Self {
            record_all: false,
            bodies: true,
            max_body_bytes: 256 * 1024,
            static_assets: false,
        }
    }
}

impl HttpCaptureConfig {
    pub fn policy(&self) -> crate::capture::http::HttpCapturePolicy {
        crate::capture::http::HttpCapturePolicy {
            record_all: self.record_all,
            bodies: self.bodies,
            max_body_bytes: self.max_body_bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", from = "ModeName")]
pub enum CaptureMode {
    /// The game in Mokora's own window (WebView2), read over DevTools.
    /// Windows and Linux ([`CaptureMode::EMBEDDED_SUPPORTED`]).
    Embedded,
    Mitm,
    Chromium,
}

impl CaptureMode {
    /// Whether this build's window can show and read the game: WebView2 on
    /// Windows, WebKitGTK on Linux.
    pub const EMBEDDED_SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));
}

impl Default for CaptureMode {
    fn default() -> Self {
        if Self::EMBEDDED_SUPPORTED {
            Self::Embedded
        } else {
            Self::Chromium
        }
    }
}

/// A mode as written in a config, before it is checked against this build.
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModeName {
    Embedded,
    Mitm,
    Chromium,
}

impl From<ModeName> for CaptureMode {
    /// `embedded` where it does not exist is the launched browser: the same
    /// DevTools capture, in a window of its own.
    fn from(name: ModeName) -> Self {
        match name {
            ModeName::Embedded if Self::EMBEDDED_SUPPORTED => Self::Embedded,
            ModeName::Embedded | ModeName::Chromium => Self::Chromium,
            ModeName::Mitm => Self::Mitm,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChromiumConfig {
    /// Absolute path to a chrome/chromium binary. `""` = auto-detect.
    pub executable: String,
    /// User-data-dir for the controlled profile. `""` = exe-adjacent
    /// `chrome-profile/` (resolved via `util::resolve_dir`, with an
    /// AppImage / read-only fallback to `<user_config_root>/chrome-profile`).
    pub user_data_dir: String,
    /// URL to navigate to on launch. `""` = don't auto-navigate (open new tab page).
    pub start_url: String,
    /// Chrome-for-Testing channel/version to download as fallback.
    /// `"stable"` / `"beta"` resolve to the latest channel pin; otherwise
    /// treated as a literal version (e.g. `"131.0.6778.85"`).
    pub cft_channel: String,
    /// Force using Chrome-for-Testing even when system Chrome is detected.
    pub force_cft: bool,
    /// Extra CLI args appended after our defaults. Advanced users only.
    pub extra_args: Vec<String>,
}

impl Default for ChromiumConfig {
    fn default() -> Self {
        Self {
            executable: String::new(),
            user_data_dir: String::new(),
            start_url: "https://game.maj-soul.com/1/".to_string(),
            cft_channel: "stable".to_string(),
            force_cft: false,
            extra_args: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let cfg = CaptureConfig::default();
        let s = toml::to_string(&cfg).unwrap();
        let back: CaptureConfig = toml::from_str(&s).unwrap();
        assert_eq!(back.mode, CaptureMode::default());
        assert_eq!(back.chromium.cft_channel, "stable");
    }

    #[test]
    fn mode_serialises_lowercase() {
        let cfg = CaptureConfig {
            mode: CaptureMode::Chromium,
            chromium: Default::default(),
            http: Default::default(),
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("mode = \"chromium\""), "got: {s}");
    }

    /// The default is deliberately conservative: full HTTP capture would
    /// put access tokens and cookies into every session file, and nothing
    /// is redacted (redaction would re-create the blind spot the capture
    /// exists to remove). Recognized exchanges are still recorded.
    #[test]
    fn http_capture_is_opt_in_but_bodies_are_on_once_it_is() {
        let cfg = CaptureConfig::default();
        assert!(!cfg.http.record_all, "full capture must be opt-in");
        assert!(cfg.http.bodies);
        assert!(!cfg.http.static_assets);
        assert_eq!(cfg.http.max_body_bytes, 256 * 1024);

        let s = toml::to_string(&cfg).unwrap();
        let back: CaptureConfig = toml::from_str(&s).unwrap();
        assert_eq!(back.http, cfg.http);
    }

    #[test]
    fn chromium_config_round_trip() {
        let original = CaptureConfig {
            mode: CaptureMode::Chromium,
            chromium: ChromiumConfig {
                executable: "/opt/chrome/chrome".into(),
                user_data_dir: "/tmp/profile".into(),
                start_url: "https://example.test/".into(),
                cft_channel: "131.0.6778.85".into(),
                force_cft: true,
                extra_args: vec!["--lang=ja".into()],
            },
            http: Default::default(),
        };
        let s = toml::to_string(&original).unwrap();
        let back: CaptureConfig = toml::from_str(&s).unwrap();
        assert_eq!(back.mode, CaptureMode::Chromium);
        assert_eq!(back.chromium.executable, "/opt/chrome/chrome");
        assert!(back.chromium.force_cft);
        assert_eq!(back.chromium.extra_args, vec!["--lang=ja"]);
    }

    #[test]
    fn missing_section_uses_defaults() {
        // Older configs that lack [capture] entirely should still parse.
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(default)]
            capture: CaptureConfig,
        }
        let s = "";
        let w: Wrap = toml::from_str(s).unwrap();
        assert_eq!(w.capture.mode, CaptureMode::default());
    }

    /// Embedded is the default where the window can show the game; elsewhere
    /// a config naming it gets the launched browser.
    #[test]
    fn embedded_only_where_the_window_can_show_the_game() {
        let read = |s: &str| toml::from_str::<CaptureConfig>(s).unwrap().mode;
        if CaptureMode::EMBEDDED_SUPPORTED {
            assert_eq!(CaptureMode::default(), CaptureMode::Embedded);
            assert_eq!(read("mode = 'embedded'"), CaptureMode::Embedded);
        } else {
            assert_eq!(CaptureMode::default(), CaptureMode::Chromium);
            assert_eq!(read("mode = 'embedded'"), CaptureMode::Chromium);
        }
        assert_eq!(read("mode = 'mitm'"), CaptureMode::Mitm);
        assert_eq!(read("mode = 'chromium'"), CaptureMode::Chromium);
        assert!(toml::from_str::<CaptureConfig>("mode = 'other'").is_err());
    }
}
