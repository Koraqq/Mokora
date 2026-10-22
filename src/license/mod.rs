//! Mokora's license: an email account signed in on this computer, with the
//! keys redeemed onto it, checked against the license server
//! (`license.korapages.com`, the MokoraKeys project).
//!
//! - [`machine`] — this computer's id as the server sees it (a hash).
//! - [`os`] — the system's name, reported for the admin.
//! - [`token`] — the server's signed license token, checked with the public
//!   keys built in here, so a license works offline.
//! - [`store`] — the session kept beside the accounts (`accounts/license.json`):
//!   one license for the whole app.
//! - [`client`] — the server calls: sign-in code, sign in, renew, redeem,
//!   sign out.
//!
//! [`LicenseManager`] holds the state the rest of the app asks about and
//! renews in the background. Server rules: one computer per account (another
//! computer is refused until this one signs out or the admin unbinds it);
//! each key adds a month after the time already bought; a token lasts 7 days,
//! so Mokora works up to 7 days without the server. The refresh token
//! changes at every renewal and an old one coming back stops the account, so
//! every server call here is serialized and the new token saved before the
//! next call.

pub mod client;
pub mod machine;
pub mod os;
pub mod store;
pub mod token;

use crate::license::client::{KeyView, Report, ServerError, Session};
use crate::license::store::Stored;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

/// Sent to the interface whenever the license state changes.
pub const LICENSE_CHANGED: &str = "license-changed";

/// How often the token is renewed while Mokora runs.
const RENEW_EVERY: Duration = Duration::from_secs(60 * 60);

/// The feature a paid account unlocks: full auto (starting the next game by
/// itself) and the game review.
pub const PRO: &str = "pro";

/// The error a locked command answers with (the panel has words for it).
pub const PRO_REQUIRED: &str = "pro_required";

/// The app's one license, for the parts of the app that only ask whether a
/// feature is unlocked. Set once at start-up ([`LicenseManager::install`]).
static CURRENT: std::sync::OnceLock<LicenseManager> = std::sync::OnceLock::new();

/// Whether the app's license unlocks `feature` right now. False before the
/// license is installed, so nothing locked runs during start-up.
pub fn allows(feature: &str) -> bool {
    CURRENT.get().is_some_and(|l| l.has(feature))
}

/// What the interface shows, and what the rest of the app asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LicenseStatus {
    /// signed_out | active | no_time | offline | locked | blocked
    pub state: &'static str,
    pub signed_in: bool,
    pub email: Option<String>,
    /// When the account's time ends (unix seconds).
    pub paid_until: Option<i64>,
    /// Until when it works without the server (unix seconds).
    pub token_expires_at: Option<i64>,
    pub keys: Vec<KeyView>,
    pub features: Vec<String>,
}

impl LicenseStatus {
    pub fn is_active(&self) -> bool {
        self.state == "active"
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// How long until a token that ends at `expires` has ended (a second past
/// it); with none, or one already over, the next look is a renewal's time away.
fn until_expiry(expires: Option<i64>, now: i64) -> Duration {
    match expires {
        Some(exp) if exp >= now => Duration::from_secs((exp - now) as u64 + 1),
        _ => RENEW_EVERY,
    }
}

/// The status a stored session gives right now, without the server.
fn status_of(stored: &Stored, machine: Option<&str>, now: i64) -> LicenseStatus {
    let signed_in = stored.refresh.is_some();
    let account = stored.account.as_ref();
    let claims = stored
        .token
        .as_deref()
        .zip(machine)
        .and_then(|(t, m)| token::verify(t, m).ok())
        .filter(|c| c.exp > now);
    let paid_until = account
        .and_then(|a| a.paid_until)
        .or(claims.as_ref().and_then(|c| c.paid_until));
    let state = match stored.problem.as_deref() {
        Some("locked") => "locked",
        Some("blocked") => "blocked",
        _ if !signed_in => "signed_out",
        _ => match &claims {
            Some(c) if !c.f.is_empty() => "active",
            Some(_) => "no_time",
            None if paid_until.is_some_and(|p| p > now) => "offline",
            None => "no_time",
        },
    };
    LicenseStatus {
        state,
        signed_in,
        email: stored.email.clone(),
        paid_until,
        token_expires_at: claims.as_ref().map(|c| c.exp),
        keys: account.map(|a| a.keys.clone()).unwrap_or_default(),
        features: if state == "active" {
            claims.map(|c| c.f).unwrap_or_default()
        } else {
            Vec::new()
        },
    }
}

/// The game nicknames of the accounts on this computer, for the admin.
fn game_names() -> Vec<String> {
    crate::accounts::known()
        .iter()
        .filter_map(|(_, path)| crate::accounts::read_identity(path))
        .map(|i| i.nickname)
        .filter(|n| !n.is_empty())
        .collect()
}

/// What "verify" did for an email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Started {
    /// Signed in with this computer's device token: no code needed.
    SignedIn { status: LicenseStatus },
    /// A sign-in code was mailed.
    CodeSent,
}

/// The app's license: state, the calls that change it, background renewal.
#[derive(Clone)]
pub struct LicenseManager(Arc<Inner>);

struct Inner {
    app: AppHandle,
    file: PathBuf,
    machine: Option<String>,
    os: Option<String>,
    stored: Mutex<Stored>,
    /// One server call at a time: two renewals presenting the same refresh
    /// token would look like a copied session to the server.
    calls: tokio::sync::Mutex<()>,
    /// Moved on whenever what is stored changes: the expiry watch plans
    /// again, the paid module's installer looks again ([`Self::changes`]).
    changed: tokio::sync::watch::Sender<u64>,
}

impl LicenseManager {
    pub fn new(app: AppHandle) -> Self {
        let file = store::default_path();
        let machine = match machine::id() {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("license: no machine id, signing in unavailable: {e:#}");
                None
            }
        };
        let stored = store::load(&file).unwrap_or_default();
        Self(Arc::new(Inner {
            app,
            file,
            machine,
            os: os::name(),
            stored: Mutex::new(stored),
            calls: tokio::sync::Mutex::new(()),
            changed: tokio::sync::watch::Sender::new(0),
        }))
    }

    fn stored(&self) -> Stored {
        self.0.stored.lock().expect("license lock").clone()
    }

    pub fn status(&self) -> LicenseStatus {
        status_of(&self.stored(), self.0.machine.as_deref(), now())
    }

    /// Whether the license unlocks `feature` right now.
    pub fn has(&self, feature: &str) -> bool {
        self.status().features.iter().any(|f| f == feature)
    }

    fn machine(&self) -> Result<String, ServerError> {
        self.0
            .machine
            .clone()
            .ok_or(ServerError::code("no_machine_id"))
    }

    fn report(&self) -> Result<Report, ServerError> {
        Ok(Report {
            machine: self.machine()?,
            os: self.0.os.clone(),
            version: env!("CARGO_PKG_VERSION"),
            names: game_names(),
        })
    }

    /// Replace what is stored (and on disk), telling the interface if the
    /// status changed.
    fn save(&self, next: Stored) {
        let before = self.status();
        *self.0.stored.lock().expect("license lock") = next.clone();
        if let Err(e) = store::save(&self.0.file, &next) {
            warn!("license: could not save: {e:#}");
        }
        let after = self.status();
        if after != before {
            let _ = self.0.app.emit(LICENSE_CHANGED, &after);
        }
        self.0.changed.send_modify(|n| *n = n.wrapping_add(1));
    }

    /// A session the server issued, checked and kept.
    fn keep(&self, session: Session) -> Result<LicenseStatus, ServerError> {
        let machine = self.machine()?;
        token::verify(&session.token, &machine).map_err(|_| ServerError::code("bad_token"))?;
        let mut devices = self.stored().devices;
        if let Some(device) = session.device {
            devices.insert(session.account.email.clone(), device);
        }
        self.save(Stored {
            email: Some(session.account.email.clone()),
            refresh: Some(session.refresh),
            token: Some(session.token),
            account: Some(session.account),
            problem: None,
            devices,
        });
        Ok(self.status())
    }

    /// The server stopped this session: keep only the email, the device
    /// tokens (and why).
    fn drop_session(&self, problem: Option<&str>) {
        let stored = self.stored();
        self.save(Stored {
            email: stored.email,
            problem: problem.map(str::to_string),
            devices: stored.devices,
            ..Stored::default()
        });
    }

    fn forget_device(&self, email: &str) {
        let mut stored = self.stored();
        if stored.devices.remove(email).is_some() {
            self.save(stored);
        }
    }

    /// React to a refusal that ends the session here.
    fn on_refusal(&self, code: &str) {
        match code {
            "locked" => self.drop_session(Some("locked")),
            "blocked" => self.drop_session(Some("blocked")),
            "signed_out" | "invalid_session" => self.drop_session(None),
            _ => {}
        }
    }

    /// "Verify" `email`: sign in at once with this computer's device token
    /// when it still works (signed in with a code here within 7 days), else
    /// mail a sign-in code.
    pub async fn start(&self, email: &str) -> Result<Started, ServerError> {
        let _call = self.0.calls.lock().await;
        let device = self.stored().devices.get(email).cloned();
        if let Some(device) = device {
            match client::login_device(email, &device, &self.report()?).await {
                Ok(session) => {
                    info!("license: signed in with this computer's device token");
                    return Ok(Started::SignedIn {
                        status: self.keep(session)?,
                    });
                }
                Err(ServerError::Refused(code)) if code == "device_expired" => {
                    self.forget_device(email);
                }
                Err(e) => return Err(e),
            }
        }
        client::login_start(email).await?;
        Ok(Started::CodeSent)
    }

    /// Sign this computer in with the mailed `code`.
    pub async fn login(&self, email: &str, code: &str) -> Result<LicenseStatus, ServerError> {
        let _call = self.0.calls.lock().await;
        let session = client::login(email, code, &self.report()?).await?;
        info!("license: signed in");
        self.keep(session)
    }

    /// Redeem a key onto the signed-in account.
    pub async fn redeem(&self, key: &str) -> Result<LicenseStatus, ServerError> {
        let _call = self.0.calls.lock().await;
        let refresh = self
            .stored()
            .refresh
            .ok_or(ServerError::code("signed_out"))?;
        match client::redeem(&refresh, &self.machine()?, key).await {
            Ok(session) => self.keep(session),
            Err(ServerError::Refused(code)) => {
                self.on_refusal(&code);
                Err(ServerError::Refused(code))
            }
            Err(e) => Err(e),
        }
    }

    /// Ask the server for a fresh token. A network failure keeps what there
    /// is; a refusal ends the session here.
    pub async fn renew(&self) -> LicenseStatus {
        let _call = self.0.calls.lock().await;
        let Some(refresh) = self.stored().refresh else {
            return self.status();
        };
        let Ok(report) = self.report() else {
            return self.status();
        };
        match client::renew(&refresh, &report).await {
            Ok(session) => {
                if let Err(e) = self.keep(session) {
                    warn!("license: renewal answer refused: {e}");
                }
            }
            Err(ServerError::Refused(code)) => {
                info!("license: the server refused the renewal ({code})");
                self.on_refusal(&code);
            }
            Err(e) => warn!("license: renew failed, keeping the current token: {e}"),
        }
        self.status()
    }

    /// Sign out on the server (which unbinds this computer), then here.
    pub async fn logout(&self) -> Result<LicenseStatus, ServerError> {
        let _call = self.0.calls.lock().await;
        if let Some(refresh) = self.stored().refresh {
            match client::logout(&refresh, &self.machine()?).await {
                Ok(()) => {}
                // Already signed out there: just forget it here.
                Err(ServerError::Refused(code))
                    if matches!(code.as_str(), "signed_out" | "invalid_session" | "locked") => {}
                Err(e) => return Err(e),
            }
        }
        self.drop_session(None);
        Ok(self.status())
    }

    /// Told whenever the license changes (signed in or out, renewed,
    /// redeemed): `changed()` on it waits for the next change.
    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.0.changed.subscribe()
    }

    /// What paid module the server has for `platform` (paid accounts only),
    /// with a link to download it.
    pub async fn pro_manifest(&self, platform: &str) -> Result<client::ProManifest, ServerError> {
        let _call = self.0.calls.lock().await;
        let refresh = self
            .stored()
            .refresh
            .ok_or(ServerError::code("signed_out"))?;
        let result = client::pro_manifest(&refresh, &self.machine()?, platform).await;
        if let Err(ServerError::Refused(code)) = &result {
            self.on_refusal(code);
        }
        result
    }

    /// Make this the license [`allows`] answers from.
    pub fn install(&self) {
        let _ = CURRENT.set(self.clone());
    }

    /// Renew now and then every hour, for as long as Mokora runs.
    pub fn start_background(&self) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                this.renew().await;
                tokio::time::sleep(RENEW_EVERY).await;
            }
        });
        self.watch_expiry();
    }

    /// Tell the interface the moment the token runs out. The status is worked
    /// out from the token whenever it is asked for, so the paid features
    /// already stop then; this is for what the interface shows (a test key
    /// lasts minutes, far less than a renewal's hour).
    fn watch_expiry(&self) {
        let this = self.clone();
        let mut changes = self.changes();
        tauri::async_runtime::spawn(async move {
            loop {
                let wait = until_expiry(this.status().token_expires_at, now());
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {
                        let _ = this.0.app.emit(LICENSE_CHANGED, &this.status());
                    }
                    // Signed in, renewed, redeemed: plan again.
                    _ = changes.changed() => {}
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expiry_watch_wakes_just_after_the_token_ends() {
        assert_eq!(until_expiry(Some(1_300), 1_000), Duration::from_secs(301));
        assert_eq!(until_expiry(Some(1_000), 1_000), Duration::from_secs(1));
        assert_eq!(until_expiry(Some(900), 1_000), RENEW_EVERY);
        assert_eq!(until_expiry(None, 1_000), RENEW_EVERY);
    }

    #[test]
    fn nothing_is_unlocked_before_the_license_is_installed() {
        assert!(!allows(PRO));
    }

    fn signed_in(token: Option<&str>, paid_until: Option<i64>) -> Stored {
        Stored {
            email: Some("me@example.com".into()),
            refresh: Some("r".into()),
            token: token.map(str::to_string),
            account: Some(client::AccountView {
                email: "me@example.com".into(),
                state: "active".into(),
                paid_until,
                keys: vec![],
            }),
            problem: None,
            devices: Default::default(),
        }
    }

    #[test]
    fn nobody_signed_in_is_signed_out() {
        let s = status_of(&Stored::default(), Some("m"), 0);
        assert_eq!((s.state, s.signed_in), ("signed_out", false));
    }

    #[test]
    fn a_stop_from_the_server_wins() {
        let s = Stored {
            problem: Some("locked".into()),
            ..signed_in(None, Some(100))
        };
        assert_eq!(status_of(&s, Some("m"), 0).state, "locked");
    }

    #[test]
    fn without_a_valid_token_paid_time_is_offline_and_none_is_no_time() {
        assert_eq!(
            status_of(&signed_in(Some("bad.token"), Some(100)), Some("m"), 0).state,
            "offline"
        );
        assert_eq!(
            status_of(&signed_in(None, Some(100)), Some("m"), 200).state,
            "no_time"
        );
        assert_eq!(
            status_of(&signed_in(None, None), Some("m"), 0).features,
            Vec::<String>::new()
        );
    }
}
