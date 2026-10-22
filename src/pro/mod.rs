//! The paid module (MokoraPro): the paid features, downloaded once a key is
//! redeemed, not part of this source.
//!
//! A paid account gets the module from the license server (`/v1/pro/`): a
//! library for this platform, signed with the module key
//! ([`MODULE_PUBKEY_B64`]). It is kept
//! beside the accounts (`accounts/pro/`) and loaded at start-up, or as soon
//! as it arrives, after its signature checks out. Mokora and the module speak
//! only `pro_api` (a call by name, JSON in and out), and only a module of the
//! same `pro_api::ABI_VERSION` is taken.
//!
//! What it holds stays locked without the paid feature (`license::allows`):
//! the module answers, but nothing asks it. [`planner`] is Yakuman's planner
//! for the engine.
//!
//! [`Installer`] keeps it current: after start-up, whenever the license
//! changes (a key redeemed) and every few hours, a paid account asks for the
//! server's module and downloads it when it differs, telling the interface
//! how far along it is ([`PRO_STATUS`]). A module already loaded stays in use
//! until the next start (a library is never unloaded under running code).

mod module;

use crate::license::{client::ProManifest, LicenseManager, PRO};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

pub use module::Module;

/// The minisign public key the module is signed with (MokoraPro's
/// `package keygen`; its secret half stays with MokoraPro, apart from the
/// release key).
pub const MODULE_PUBKEY_B64: &str = "RWS4GilRE1F/exmP6XDCc3E004I+RAD+mtMoSp1c8+sQJszeRNk6QBOx";

/// Sent to the interface whenever [`ProStatus`] changes.
pub const PRO_STATUS: &str = "pro-status";

/// How often a paid account looks for a newer module while Mokora runs.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Where the module stands, for the interface (its download toast).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProStatus {
    /// No module (not paid yet, or none for this platform).
    None,
    /// Downloading `version`: `received` of `total` bytes.
    Downloading {
        version: String,
        received: u64,
        total: u64,
    },
    /// In use.
    Ready { version: String },
    /// `version` is downloaded; it is used from the next start.
    Restart { version: String },
    /// Getting it failed; tried again later or on [`Installer::retry`].
    Failed { error: String },
}

/// The module in use, once loaded: for the rest of the run.
static LOADED: OnceLock<Arc<Module>> = OnceLock::new();
static STATUS: Mutex<Option<ProStatus>> = Mutex::new(None);

/// The module in use, if any.
pub fn loaded() -> Option<Arc<Module>> {
    LOADED.get().cloned()
}

/// Whether the paid features can run now: paid, and the module loaded.
pub fn available() -> bool {
    LOADED.get().is_some() && crate::license::allows(PRO)
}

/// The error a paid feature that runs in the module answers when it cannot:
/// not paid (`pro_required`), or paid and the module not here yet
/// (`pro_not_ready`). `None` when it can run.
pub fn refusal() -> Option<&'static str> {
    if !crate::license::allows(PRO) {
        Some(crate::license::PRO_REQUIRED)
    } else if LOADED.get().is_none() {
        Some(PRO_NOT_READY)
    } else {
        None
    }
}

/// See [`refusal`].
pub const PRO_NOT_READY: &str = "pro_not_ready";

/// Yakuman's planner from the module, for the engine; `None` when it cannot
/// run ([`available`]).
pub fn planner() -> Option<Arc<dyn native_bot::yakuman::Planner>> {
    available().then(|| Arc::new(module::ModulePlanner) as Arc<dyn native_bot::yakuman::Planner>)
}

/// Full auto's map of the lobby, from the module; `None` without one. Asked
/// once a run: it never changes while Mokora runs.
pub fn lobby_layout() -> Option<Arc<pro_api::lobby::Layout>> {
    static LAYOUT: OnceLock<Arc<pro_api::lobby::Layout>> = OnceLock::new();
    if let Some(l) = LAYOUT.get() {
        return Some(l.clone());
    }
    match loaded()?.call::<_, pro_api::lobby::Layout>(pro_api::method::LOBBY_LAYOUT, &()) {
        Ok(l) => Some(LAYOUT.get_or_init(|| Arc::new(l)).clone()),
        Err(e) => {
            warn!("paid module: no lobby layout: {e:#}");
            None
        }
    }
}

/// Review a game in the module (`pro_api::method::REVIEW_RUN`), lending it
/// the model's `weights`; `tick` is asked between events and says stop.
/// `Ok(None)` when it did.
pub fn review(
    req: &pro_api::review::ReviewRequest,
    weights: &[u8],
    tick: &mut dyn FnMut() -> bool,
) -> Result<Option<pro_api::review::ReviewRun>> {
    let module = loaded().context(crate::license::PRO_REQUIRED)?;
    module.call_with(pro_api::method::REVIEW_RUN, req, weights, &mut |method, _| {
        if method == pro_api::host::REVIEW_TICK {
            serde_json::to_vec(&tick()).map_err(|e| e.to_string())
        } else {
            Err(format!("{method}: not offered here"))
        }
    })
}

/// Tests: load the module built at `MOKORA_PRO_DLL` (unsigned, as built) as
/// the one in use. False when the variable is not set.
#[cfg(test)]
pub fn load_for_tests() -> bool {
    let Ok(path) = std::env::var("MOKORA_PRO_DLL") else {
        return false;
    };
    if LOADED.get().is_none() {
        let m = Module::load(Path::new(&path), "test".into()).expect("MOKORA_PRO_DLL loads");
        let _ = LOADED.set(Arc::new(m));
    }
    true
}

pub fn status() -> ProStatus {
    STATUS
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or(ProStatus::None)
}

/// The folder the module is kept in.
fn dir() -> PathBuf {
    crate::util::resolve_dir(Path::new(crate::accounts::ACCOUNTS_DIR)).join("pro")
}

/// What is installed: the module's file and signature, by version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Installed {
    version: String,
    file: String,
    sha256: String,
    minisig: String,
}

const INSTALLED: &str = "installed.json";

fn read_installed(dir: &Path) -> Option<Installed> {
    serde_json::from_slice(&std::fs::read(dir.join(INSTALLED)).ok()?).ok()
}

/// The file of `version` on this platform: versioned, so a newer one never
/// has to overwrite a library in use.
fn file_name(version: &str) -> String {
    let base = pro_api::file_name();
    let (stem, ext) = base.rsplit_once('.').unwrap_or((base, ""));
    format!("{stem}-{version}.{ext}")
}

/// The installed module's file, when its signature checks out for this
/// platform and ABI.
fn verified(dir: &Path) -> Result<(PathBuf, Installed)> {
    let installed = read_installed(dir).context("no module installed")?;
    let path = dir.join(&installed.file);
    verify(&path, &installed.minisig, &installed.version)?;
    Ok((path, installed))
}

/// `path` is the module of `version`, signed with the module key for this
/// platform and ABI (`pro_api::trusted_comment`).
fn verify(path: &Path, minisig: &str, version: &str) -> Result<()> {
    let comment = pro_api::trusted_comment(pro_api::platform(), version);
    crate::github::signing::verify_with_key(MODULE_PUBKEY_B64, path, minisig, &comment)
        .with_context(|| format!("module {} is not signed for this Mokora", path.display()))
}

/// Load the installed module, at start-up (before any account opens). Files
/// of other versions left behind are removed first: none is loaded yet.
pub fn load_installed() {
    let dir = dir();
    let Ok((path, installed)) = verified(&dir) else {
        return;
    };
    remove_others(&dir, &installed.file);
    match Module::load(&path, installed.version.clone()) {
        Ok(m) => {
            info!(version = %installed.version, "paid module loaded");
            let _ = LOADED.set(Arc::new(m));
            *STATUS.lock().expect("pro status") = Some(ProStatus::Ready {
                version: installed.version,
            });
        }
        Err(e) => warn!("paid module could not be loaded: {e:#}"),
    }
}

fn remove_others(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let base = pro_api::file_name();
    let stem = base.rsplit_once('.').map_or(base, |(s, _)| s);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(stem) && name != keep {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Keeps the module current for a paid account (see the module docs).
#[derive(Clone)]
pub struct Installer {
    app: AppHandle,
    license: LicenseManager,
    again: Arc<tokio::sync::Notify>,
}

impl Installer {
    pub fn new(app: AppHandle, license: LicenseManager) -> Self {
        Self {
            app,
            license,
            again: Arc::new(tokio::sync::Notify::new()),
        }
    }

    fn set(&self, status: ProStatus) {
        let mut s = STATUS.lock().expect("pro status");
        if s.as_ref() != Some(&status) {
            *s = Some(status.clone());
            drop(s);
            let _ = self.app.emit(PRO_STATUS, &status);
        }
    }

    /// Look again now (the interface's "try again").
    pub fn retry(&self) {
        self.again.notify_one();
    }

    /// Run for as long as Mokora runs.
    pub fn start(&self) {
        let this = self.clone();
        let mut changes = self.license.changes();
        tauri::async_runtime::spawn(async move {
            loop {
                this.ensure().await;
                tokio::select! {
                    _ = changes.changed() => {}
                    _ = this.again.notified() => {}
                    _ = tokio::time::sleep(CHECK_EVERY) => {}
                }
            }
        });
    }

    /// Get the server's module if this account may and it differs from ours.
    async fn ensure(&self) {
        if !crate::license::allows(PRO) || pro_api::platform() == "unsupported" {
            return;
        }
        let manifest = match self.license.pro_manifest(pro_api::platform()).await {
            Ok(m) => m,
            Err(e) => {
                // None up for this platform, not paid after all, or a server
                // without modules yet (`not_found`): nothing to say. Anything
                // else is said only while there is no module.
                let code = e.ui_code();
                let quiet = matches!(code.as_str(), "no_module" | "not_paid" | "not_found");
                if !quiet && LOADED.get().is_none() {
                    self.set(ProStatus::Failed { error: code });
                }
                return;
            }
        };
        let dir = dir();
        if read_installed(&dir).is_some_and(|i| i.version == manifest.version)
            && verified(&dir).is_ok()
        {
            if LOADED.get().is_none() {
                self.load_now(&dir);
            }
            return;
        }
        match self.download(&dir, &manifest).await {
            Ok(()) => {
                if LOADED.get().is_none() {
                    self.load_now(&dir);
                } else {
                    self.set(ProStatus::Restart {
                        version: manifest.version,
                    });
                }
            }
            Err(e) => {
                warn!("paid module download failed: {e:#}");
                self.set(ProStatus::Failed {
                    error: "download_failed".into(),
                });
            }
        }
    }

    fn load_now(&self, dir: &Path) {
        match verified(dir).and_then(|(path, i)| Module::load(&path, i.version.clone()).map(|m| (m, i))) {
            Ok((m, i)) => {
                info!(version = %i.version, "paid module loaded");
                let _ = LOADED.set(Arc::new(m));
                self.set(ProStatus::Ready { version: i.version });
            }
            Err(e) => {
                warn!("paid module could not be loaded: {e:#}");
                self.set(ProStatus::Failed {
                    error: "load_failed".into(),
                });
            }
        }
    }

    /// Download `m` into `dir`, check its size, hash and signature, and make
    /// it the installed one.
    async fn download(&self, dir: &Path, m: &ProManifest) -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let file = file_name(&m.version);
        let part = dir.join(format!("{file}.part"));
        let progress = |received| ProStatus::Downloading {
            version: m.version.clone(),
            received,
            total: m.size,
        };
        self.set(progress(0));

        let client = crate::github::build_client()?;
        let mut response = client
            .get(format!("{}{}", crate::license::client::server(), m.url))
            .send()
            .await
            .context("ask for the module")?
            .error_for_status()
            .context("the server refused the module")?;
        let mut out = tokio::fs::File::create(&part)
            .await
            .with_context(|| format!("create {}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        let mut last = std::time::Instant::now();
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(60), response.chunk())
            .await
            .context("the download stalled")?
            .context("read the module")?
        {
            out.write_all(&chunk).await.context("write the module")?;
            hasher.update(&chunk);
            received += chunk.len() as u64;
            if received > m.size {
                bail!("the module is larger than announced");
            }
            if last.elapsed() >= Duration::from_millis(150) {
                last = std::time::Instant::now();
                self.set(progress(received));
            }
        }
        out.flush().await.ok();
        drop(out);
        self.set(progress(received));

        let sha = hex(&hasher.finalize());
        if received != m.size || !sha.eq_ignore_ascii_case(&m.sha256) {
            let _ = std::fs::remove_file(&part);
            bail!("the module arrived damaged ({received} bytes, sha256 {sha})");
        }
        if let Err(e) = verify(&part, &m.minisig, &m.version) {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        let path = dir.join(&file);
        std::fs::rename(&part, &path).with_context(|| format!("keep {}", path.display()))?;
        let installed = Installed {
            version: m.version.clone(),
            file,
            sha256: m.sha256.clone(),
            minisig: m.minisig.clone(),
        };
        std::fs::write(dir.join(INSTALLED), serde_json::to_vec_pretty(&installed)?)
            .context("record the module")?;
        info!(version = %m.version, "paid module downloaded");
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_files_carry_their_version() {
        let name = file_name("0.1.0");
        assert!(name.ends_with(if cfg!(windows) { "-0.1.0.dll" } else { "-0.1.0.so" }), "{name}");
    }

    #[test]
    fn nothing_installed_is_not_verified() {
        let dir = tempfile::tempdir().unwrap();
        assert!(verified(dir.path()).is_err());
    }

    /// A file whose signature does not match is refused, however it is named.
    #[test]
    fn an_unsigned_module_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_name("0.1.0");
        std::fs::write(dir.path().join(&file), b"not a module").unwrap();
        let installed = Installed {
            version: "0.1.0".into(),
            file,
            sha256: String::new(),
            minisig: "untrusted comment: x\nRUQ=\ntrusted comment: y\nAA==\n".into(),
        };
        std::fs::write(dir.path().join(INSTALLED), serde_json::to_vec(&installed).unwrap()).unwrap();
        assert!(verified(dir.path()).is_err());
    }

    /// Mokora's loader and call against a real module: run with
    /// `MOKORA_PRO_DLL=<a built mokora_pro library> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_built_module_loads_and_plans() {
        use pro_api::yakuman::{Choice, Hand, Kind, PlanRequest, PlanResponse, Table};
        let path = std::env::var("MOKORA_PRO_DLL").expect("MOKORA_PRO_DLL");
        let m = Module::load(Path::new(&path), "test".into()).expect("loads");
        let mut counts = vec![0u8; 34];
        for t in [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33] {
            counts[t] = 1;
        }
        let choice = |kind, yakuman_win| Choice {
            kind,
            tile: Some(33 * 4 + 1),
            consume: vec![],
            yakuman_win,
        };
        let req = PlanRequest {
            hand: Hand {
                counts,
                melds: vec![],
                runs: vec![],
            },
            table: Table {
                players: 4,
                unseen: vec![3; 34],
                wall_left: 60,
                own_draws: 15,
            },
            riichi: false,
            choices: vec![choice(Kind::Pass, false), choice(Kind::Ron, true)],
        };
        let order: PlanResponse = m.call(pro_api::method::YAKUMAN_RANK, &req).expect("answers");
        assert_eq!(order, Some(vec![(1, 1.0)]), "a yakuman ron is taken");
        assert!(m.call::<_, PlanResponse>("nope", &req).is_err());
    }

    /// What the server hands out checks out with the key built in here, for
    /// each platform: `MOKORA_PRO_SERVED=<MokoraKeys/server/pro> cargo test
    /// -- --ignored`.
    #[test]
    #[ignore = "needs the served modules (MOKORA_PRO_SERVED)"]
    fn the_served_modules_verify() {
        let root = PathBuf::from(std::env::var("MOKORA_PRO_SERVED").expect("MOKORA_PRO_SERVED"));
        for platform in ["windows-x64", "linux-x64"] {
            let dir = root.join(platform);
            let m: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
            let (version, file, minisig) = (
                m["version"].as_str().unwrap(),
                m["file"].as_str().unwrap(),
                m["minisig"].as_str().unwrap(),
            );
            let path = dir.join(file);
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(bytes.len() as u64, m["size"].as_u64().unwrap(), "{platform}");
            assert_eq!(hex(&Sha256::digest(&bytes)), m["sha256"].as_str().unwrap(), "{platform}");
            let comment = pro_api::trusted_comment(platform, version);
            crate::github::signing::verify_with_key(MODULE_PUBKEY_B64, &path, minisig, &comment)
                .unwrap_or_else(|e| panic!("{platform}: {e:#}"));
            // Signed for its own platform only.
            let other = pro_api::trusted_comment(if platform == "linux-x64" { "windows-x64" } else { "linux-x64" }, version);
            assert!(crate::github::signing::verify_with_key(MODULE_PUBKEY_B64, &path, minisig, &other).is_err());
        }
    }

    #[test]
    fn other_versions_are_cleared_and_the_kept_one_stays() {
        let dir = tempfile::tempdir().unwrap();
        let keep = file_name("0.2.0");
        for f in [file_name("0.1.0"), keep.clone(), format!("{}.part", file_name("0.3.0"))] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        std::fs::write(dir.path().join(INSTALLED), b"{}").unwrap();
        remove_others(dir.path(), &keep);
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left, vec![INSTALLED.to_string(), keep]);
    }
}
