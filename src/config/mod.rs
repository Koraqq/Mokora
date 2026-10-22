mod autoplay;
mod bot;
mod capture;
mod general;
mod logging;
mod merge;
mod network;
mod overlay;
mod platform;
mod privacy;
mod proxy;

pub use autoplay::{
    AutoplayConfig, DelayDistribution, DelayMode, DelayModelConfig, ListScroll,
    MajsoulAutoplayConfig, NextGameConfig, NextGameRule, RankRoom, RoomMode, MAX_GAMES_PER_DAY,
    MAX_RELOGINS,
};
pub use bot::BotConfig;
pub use capture::{CaptureConfig, CaptureMode, ChromiumConfig, HttpCaptureConfig};
pub use general::GeneralConfig;
pub use logging::LoggingConfig;
pub use merge::merge_into;
pub use network::{GithubMirrorMode, NetworkConfig};
pub use overlay::{OverlayConfig, TOP_N_MAX, TOP_N_MIN};
pub use platform::{Platform, PlatformConfig};
pub use privacy::PrivacyConfig;
pub use proxy::{ProxyConfig, DEFAULT_MITM_DOMAINS};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub logging: LoggingConfig,
    pub platform: PlatformConfig,
    pub proxy: ProxyConfig,
    pub bot: BotConfig,
    pub capture: CaptureConfig,
    pub autoplay: AutoplayConfig,
    pub overlay: OverlayConfig,
    pub network: NetworkConfig,
    pub privacy: PrivacyConfig,
}

enum ResolvedPath {
    Existing(PathBuf),
    Missing(PathBuf),
}

fn resolve_config_path(cli_path: Option<&Path>) -> ResolvedPath {
    resolve_config_path_inner(
        cli_path,
        crate::util::user_config_root(),
        crate::util::data_in_user_dir(),
    )
}

fn resolve_config_path_inner(
    cli_path: Option<&Path>,
    user_cfg_root: Option<PathBuf>,
    in_user_dir: bool,
) -> ResolvedPath {
    if let Some(p) = cli_path {
        if p.exists() {
            return ResolvedPath::Existing(p.to_path_buf());
        }
        return ResolvedPath::Missing(p.to_path_buf());
    }

    // Existing-file search: prefer user config dir, then exe-dir, then cwd.
    if let Some(user_cfg) = &user_cfg_root {
        let candidate = user_cfg.join("config.toml");
        if candidate.exists() {
            return ResolvedPath::Existing(candidate);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let candidate = exe_dir.join("configs").join("config.toml");
            if candidate.exists() {
                return ResolvedPath::Existing(candidate);
            }
        }
    }

    let cwd_candidate = PathBuf::from("configs.toml");
    if cwd_candidate.exists() {
        return ResolvedPath::Existing(cwd_candidate);
    }

    // No existing config. Choose a writable target: the user config dir
    // under AppImage or whenever the exe dir is read-only (`data_in_user_dir`).
    if in_user_dir {
        if let Some(user_cfg) = user_cfg_root {
            return ResolvedPath::Missing(user_cfg.join("config.toml"));
        }
    }

    let target = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("configs").join("config.toml")))
        .unwrap_or(cwd_candidate);
    ResolvedPath::Missing(target)
}

/// The config `load_config(None)` would load (or create): the first
/// account's, which the account picker lists as "main" (see `crate::accounts`).
pub fn default_config_path() -> PathBuf {
    match resolve_config_path(None) {
        ResolvedPath::Existing(p) | ResolvedPath::Missing(p) => p,
    }
}

fn write_default_config(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let defaults = AppConfig::default();
    let body = toml::to_string_pretty(&defaults).map_err(std::io::Error::other)?;
    crate::util::write_atomic(path, body)
}

/// Keep a copy of a config that cannot be read as TOML before anything
/// replaces it with defaults: `config.toml.bad`, or `config.toml.bad-<secs>`
/// when a different broken copy is already there. Returns where the copy went;
/// `None` when the file cannot be read or the same copy is already kept.
pub(crate) fn back_up_unreadable(path: &Path) -> Option<PathBuf> {
    let bytes = std::fs::read(path).ok()?;
    let mut name = path.as_os_str().to_owned();
    name.push(".bad");
    let mut target = PathBuf::from(name);
    if let Ok(kept) = std::fs::read(&target) {
        if kept == bytes {
            return None;
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".bad-{secs}"));
        target = PathBuf::from(name);
    }
    std::fs::write(&target, bytes).ok()?;
    Some(target)
}

/// Load and parse the config. Returns the parsed `AppConfig` and the
/// path it was loaded from (so callers can persist updates back to the
/// same file via `commands::update_config`).
///
/// On any failure path the in-memory default is returned, but the path
/// returned is the one we *would* have written to — keeping `update_config`
/// from silently writing to an unexpected location.
pub fn load_config(cli_path: Option<&Path>) -> (AppConfig, PathBuf) {
    let path = match resolve_config_path(cli_path) {
        ResolvedPath::Existing(p) => p,
        ResolvedPath::Missing(target) => {
            eprintln!(
                "No config file found, writing defaults to: {}",
                target.display()
            );
            match write_default_config(&target) {
                Ok(()) => target,
                Err(e) => {
                    // Read-only fs (AppImage on hosts that don't set $APPIMAGE,
                    // or system installs in /usr): retry under user config dir.
                    if let Some(user_cfg) = crate::util::user_config_root() {
                        let fallback = user_cfg.join("config.toml");
                        if fallback != target {
                            eprintln!(
                                "Write to {} failed: {e}. Retrying at {}",
                                target.display(),
                                fallback.display()
                            );
                            match write_default_config(&fallback) {
                                Ok(()) => fallback,
                                Err(e2) => {
                                    eprintln!(
                                        "Failed to write default config: {e2}, using in-memory defaults"
                                    );
                                    return (AppConfig::default(), fallback);
                                }
                            }
                        } else {
                            eprintln!(
                                "Failed to write default config: {e}, using in-memory defaults"
                            );
                            return (AppConfig::default(), target);
                        }
                    } else {
                        eprintln!("Failed to write default config: {e}, using in-memory defaults");
                        return (AppConfig::default(), target);
                    }
                }
            }
        }
    };

    eprintln!("Loading config from: {}", path.display());

    let mut cfg = match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<AppConfig>(&content) {
            Ok(config) => config,
            Err(e) => {
                let kept = back_up_unreadable(&path);
                eprintln!(
                    "Failed to parse config: {e}, using defaults{}",
                    kept.map(|k| format!(" (the file is kept as {})", k.display()))
                        .unwrap_or_default()
                );
                AppConfig::default()
            }
        },
        Err(e) => {
            eprintln!("Failed to read config: {e}, using defaults");
            AppConfig::default()
        }
    };
    // Migrate legacy `[bot] active = "..."` into `active_4p` once.
    cfg.bot.migrate_legacy_active();
    // The built-in bots' old names (`mokora-native`…) to their current ones.
    cfg.bot.migrate_legacy_names();
    // Pre-existing configs (created before the first-run wizard landed)
    // shouldn't be hijacked into the wizard. Detect by presence of any
    // non-default field that the user must have written deliberately.
    migrate_first_run_marker(&mut cfg, &path);
    (cfg, path)
}

/// Existing users upgrading to a build that introduces the wizard get
/// `first_run_completed = true` automatically — they've already run the
/// app at least once and don't need onboarding. Detected by the config
/// file existing on disk *and* not being a freshly-written defaults file.
///
/// A defaults file written by `write_default_config` contains the full
/// serialised AppConfig; we treat any config file that lacks the new
/// `general.first_run_completed` key as legacy (serde fills the default
/// `false` on parse, so we flip it to `true` after parsing).
fn migrate_first_run_marker(cfg: &mut AppConfig, path: &Path) {
    if cfg.general.first_run_completed {
        return;
    }
    let Ok(body) = std::fs::read_to_string(path) else {
        return;
    };
    // If the file lacks the explicit key, it's a legacy file: respect the
    // user's prior config (whatever is on disk works for them already)
    // and skip the wizard. Fresh defaults files written by us *do* contain
    // the explicit `first_run_completed = false` line, so they still trigger
    // the wizard.
    if !body.contains("first_run_completed") {
        cfg.general.first_run_completed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder for one test, removed when the test ends — passed or
    /// failed. (A hand-rolled folder plus a trailing `remove_dir_all` left
    /// one behind in %TEMP% whenever a test had no cleanup line or panicked
    /// before reaching it.)
    struct TestDir(tempfile::TempDir);

    impl std::ops::Deref for TestDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            self.0.path()
        }
    }

    impl AsRef<Path> for TestDir {
        fn as_ref(&self) -> &Path {
            self.0.path()
        }
    }

    fn temp_dir(tag: &str) -> TestDir {
        TestDir(
            tempfile::Builder::new()
                .prefix(&format!("mokora-cfg-{tag}-"))
                .tempdir()
                .unwrap(),
        )
    }

    #[test]
    fn writes_defaults_when_cli_path_missing() {
        let dir = temp_dir("cli-missing");
        let target = dir.join("nested").join("config.toml");
        assert!(!target.exists());

        let (cfg, path) = load_config(Some(&target));

        assert!(target.exists(), "default config file should be created");
        assert_eq!(path, target);
        let body = std::fs::read_to_string(&target).unwrap();
        let round_trip: AppConfig = toml::from_str(&body).unwrap();
        assert_eq!(
            round_trip.general.first_run_completed,
            cfg.general.first_run_completed
        );
    }

    /// A config written while the cloud-inference settings existed still loads:
    /// its `[bot.api]` table (server URL, key, models) is simply not read.
    #[test]
    fn an_old_bot_api_section_is_ignored() {
        let old = "[bot]\nenabled = true\nactive_4p = \"my-bot\"\n\n[bot.api]\nenabled = true\nbase_url = \"http://127.0.0.1:8080\"\nkey = \"x\"\n";
        let cfg: AppConfig = toml::from_str(old).unwrap();
        assert!(cfg.bot.enabled);
        assert_eq!(cfg.bot.active_4p, "my-bot");
    }

    /// A config naming a built-in bot by its old name loads with the new one.
    #[test]
    fn old_bot_names_load_as_the_new_ones() {
        let dir = temp_dir("old-bot-names");
        let target = dir.join("config.toml");
        std::fs::write(
            &target,
            "[general]\nfirst_run_completed = true\n[bot]\nactive_4p = \"mokora-native4p\"\nactive_3p = \"mora-native3p\"\n",
        )
        .unwrap();
        let (cfg, _) = load_config(Some(&target));
        assert_eq!(cfg.bot.active_4p, "mora-4p");
        assert_eq!(cfg.bot.active_3p, "mora-3p");
    }

    /// A config that cannot be parsed loads as defaults, and the broken file is
    /// kept beside it (once: loading it again adds no second copy).
    #[test]
    fn an_unparseable_config_is_kept_when_defaults_take_over() {
        const BROKEN: &str = "[bot\nenabled = = true";
        let dir = temp_dir("unparseable");
        let target = dir.join("config.toml");
        std::fs::write(&target, BROKEN).unwrap();

        let (cfg, _) = load_config(Some(&target));
        assert_eq!(cfg.bot.active_4p, AppConfig::default().bot.active_4p);
        let kept = dir.join("config.toml.bad");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), BROKEN);

        load_config(Some(&target));
        let copies = std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().contains(".bad"))
            })
            .count();
        assert_eq!(copies, 1, "the same broken file is kept once");

        // A different broken file is kept as well, under its own name.
        std::fs::write(&target, "different = = nonsense").unwrap();
        load_config(Some(&target));
        assert_eq!(
            std::fs::read_to_string(&kept).unwrap(),
            BROKEN,
            "the first copy stays"
        );
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp() {
        let dir = temp_dir("atomic");
        let target = dir.join("config.toml");
        crate::util::write_atomic(&target, "one").unwrap();
        crate::util::write_atomic(&target, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two");
        assert!(!dir.join("config.toml.tmp").exists());
    }

    #[test]
    fn reuses_existing_cli_path() {
        let dir = temp_dir("cli-existing");
        let target = dir.join("config.toml");
        std::fs::write(&target, "[general]\nfirst_run_completed = true\n").unwrap();

        let (cfg, path) = load_config(Some(&target));
        assert!(cfg.general.first_run_completed);
        assert_eq!(path, target);
    }

    /// Regression: under AppImage the exe dir is read-only squashfs. The
    /// missing-config target must point at the user config dir instead of
    /// `<exe_dir>/configs/config.toml`, otherwise the default-write fails
    /// with `Read-only file system (os error 30)`.
    #[test]
    fn appimage_routes_missing_target_to_user_config_dir() {
        let user_cfg = temp_dir("appimage-user-cfg");
        let resolved = resolve_config_path_inner(None, Some(user_cfg.to_path_buf()), true);
        match resolved {
            ResolvedPath::Missing(p) => {
                assert_eq!(p, user_cfg.join("config.toml"));
            }
            ResolvedPath::Existing(p) => panic!("expected Missing, got Existing({})", p.display()),
        }
    }

    #[test]
    fn non_appimage_does_not_route_to_user_config_dir() {
        let user_cfg = temp_dir("non-appimage-user-cfg");
        let resolved = resolve_config_path_inner(None, Some(user_cfg.to_path_buf()), false);
        match resolved {
            ResolvedPath::Missing(p) => {
                assert!(
                    !p.starts_with(&user_cfg),
                    "non-appimage should not route to user cfg dir, got {}",
                    p.display()
                );
            }
            ResolvedPath::Existing(p) => panic!("expected Missing, got Existing({})", p.display()),
        }
    }
}
