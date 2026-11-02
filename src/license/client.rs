//! The license server's calls. Errors keep the server's short codes
//! (`wrong_code`, `bound_elsewhere`, …) so the interface can word them.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

/// Where the license server answers. `MOKORA_LICENSE_SERVER` points a build
/// at the test server; tokens must still verify with a built-in key.
pub fn server() -> String {
    std::env::var("MOKORA_LICENSE_SERVER")
        .ok()
        .filter(|s| s.starts_with("https://") || s.starts_with("http://127.0.0.1"))
        .unwrap_or_else(|| "https://license.korapages.com".to_string())
}

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerError {
    /// The server answered and said no: its error code.
    #[error("{0}")]
    Refused(String),
    /// The server could not be reached, or answered nonsense.
    #[error("network: {0}")]
    Network(String),
}

impl ServerError {
    pub fn code(code: &str) -> Self {
        Self::Refused(code.to_string())
    }

    /// The code the interface words: the server's, or `network`.
    pub fn ui_code(&self) -> String {
        match self {
            Self::Refused(c) => c.clone(),
            Self::Network(_) => "network".to_string(),
        }
    }
}

/// One redeemed key, as the account lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyView {
    pub hint: String,
    pub days: i64,
    /// A free key's length in minutes (its `days` is then 0).
    #[serde(default)]
    pub minutes: Option<i64>,
    /// A free key: the account is free while it runs, its paid time waiting.
    #[serde(default)]
    pub free: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// queued | running | used | revoked
    pub state: String,
}

/// The account as the server reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountView {
    pub email: String,
    pub state: String,
    pub paid_until: Option<i64>,
    #[serde(default)]
    pub keys: Vec<KeyView>,
}

/// A signed-in session: the next refresh token, a fresh license token and
/// the account; after a code sign-in, also the device token.
#[derive(Debug, Deserialize)]
pub struct Session {
    pub refresh: String,
    pub token: String,
    pub account: AccountView,
    #[serde(default)]
    pub device: Option<String>,
}

/// What Mokora tells the server about this computer, for the admin.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub machine: String,
    pub os: Option<String>,
    pub version: &'static str,
    pub names: Vec<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

async fn post<T: for<'de> Deserialize<'de>>(
    path: &str,
    body: serde_json::Value,
) -> Result<T, ServerError> {
    let client =
        crate::github::build_client().map_err(|e| ServerError::Network(format!("{e:#}")))?;
    let response = client
        .post(format!("{}{path}", server()))
        .json(&body)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| ServerError::Network(e.to_string()))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ServerError::Network(e.to_string()))?;
    if status.is_success() {
        return serde_json::from_slice(&bytes).map_err(|e| ServerError::Network(e.to_string()));
    }
    match serde_json::from_slice::<ErrorBody>(&bytes) {
        // A 5xx is the server's own trouble: retry later, never sign out.
        Ok(b) if status.is_client_error() => Err(ServerError::Refused(b.error)),
        _ => Err(ServerError::Network(format!("HTTP {status}"))),
    }
}

#[derive(Deserialize)]
struct Ok_ {
    #[allow(dead_code)]
    ok: bool,
}

/// Mail a sign-in code to `email`.
pub async fn login_start(email: &str) -> Result<(), ServerError> {
    post::<Ok_>("/v1/login/start", json!({ "email": email })).await?;
    Ok(())
}

/// Sign this computer in with the mailed `code`.
pub async fn login(email: &str, code: &str, report: &Report) -> Result<Session, ServerError> {
    post(
        "/v1/login",
        json!({
            "email": email,
            "code": code,
            "machine": report.machine,
            "os": report.os,
            "version": report.version,
            "names": report.names,
        }),
    )
    .await
}

/// Sign this computer in with its device token (no code).
pub async fn login_device(
    email: &str,
    device: &str,
    report: &Report,
) -> Result<Session, ServerError> {
    post(
        "/v1/login/device",
        json!({
            "email": email,
            "device": device,
            "machine": report.machine,
            "os": report.os,
            "version": report.version,
            "names": report.names,
        }),
    )
    .await
}

/// Renew the license token; the refresh token turns over.
pub async fn renew(refresh: &str, report: &Report) -> Result<Session, ServerError> {
    post(
        "/v1/renew",
        json!({
            "refresh": refresh,
            "machine": report.machine,
            "os": report.os,
            "version": report.version,
            "names": report.names,
        }),
    )
    .await
}

/// Redeem a key onto the signed-in account.
pub async fn redeem(refresh: &str, machine: &str, key: &str) -> Result<Session, ServerError> {
    post(
        "/v1/redeem",
        json!({ "refresh": refresh, "machine": machine, "key": key }),
    )
    .await
}

/// Sign out, which unbinds this computer.
pub async fn logout(refresh: &str, machine: &str) -> Result<(), ServerError> {
    post::<Ok_>(
        "/v1/logout",
        json!({ "refresh": refresh, "machine": machine }),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_gets_short_codes() {
        assert_eq!(ServerError::code("wrong_code").ui_code(), "wrong_code");
        assert_eq!(ServerError::Network("x".into()).ui_code(), "network");
    }

    #[test]
    fn the_account_answer_parses() {
        let a: AccountView = serde_json::from_str(
            r#"{"email":"me@x.com","state":"active","paid_until":5,
                "keys":[{"hint":"Mokora-AB…","days":30,"start":1,"end":5,"state":"running"}]}"#,
        )
        .unwrap();
        assert_eq!(a.keys[0].state, "running");
        assert!(!a.keys[0].free);
        // A free key running, the paid time waiting behind it.
        let a: AccountView = serde_json::from_str(
            r#"{"email":"me@x.com","state":"no_time","paid_until":null,"free_until":9,
                "keys":[{"hint":"Mokora-CD…","days":0,"minutes":60,"free":true,"start":1,"end":9,"state":"running"},
                        {"hint":"Mokora-AB…","days":30,"free":false,"start":9,"end":99,"state":"queued"}]}"#,
        )
        .unwrap();
        assert!(a.keys[0].free && !a.keys[1].free);
        assert_eq!(a.paid_until, None);
    }
}

/// The paid module the server has for a platform (`/v1/pro/manifest`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProManifest {
    pub version: String,
    pub size: u64,
    /// Hex SHA-256 of the file.
    pub sha256: String,
    /// The file's minisign signature (a `.minisig` document).
    pub minisig: String,
    /// Where to download it, on the server, for a few minutes.
    pub url: String,
}

/// What paid module there is for `platform` (paid accounts only).
pub async fn pro_manifest(
    refresh: &str,
    machine: &str,
    platform: &str,
) -> Result<ProManifest, ServerError> {
    post(
        "/v1/pro/manifest",
        json!({ "refresh": refresh, "machine": machine, "platform": platform }),
    )
    .await
}

/// What the server answers when it keeps a share.
#[derive(Debug, Clone, Deserialize)]
pub struct ShareMade {
    /// The eight-character id that goes into the code.
    pub id: String,
    /// When it stops answering (unix seconds): three days on.
    pub expires_at: i64,
    /// A free account's shares still to make (it may make ten in all); `None`
    /// for a paid one.
    #[serde(default)]
    pub free_left: Option<i64>,
}

/// Keep `data` (sealed bytes, see `history::share`) for three days. Only a
/// signed-in account may. `source` is the stamp of the player shared: a free
/// account may share ten times in all, a paid one any number of times but each
/// player once in twelve hours (`free_limit`, `cooldown`).
pub async fn share_put(
    refresh: &str,
    machine: &str,
    source: &str,
    data: &[u8],
    games: usize,
) -> Result<ShareMade, ServerError> {
    use base64::Engine as _;
    post(
        "/v1/share",
        json!({
            "refresh": refresh,
            "machine": machine,
            "source": source,
            "games": games,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    )
    .await
}

/// The sealed bytes of share `id`. No account is needed: the code is the key.
pub async fn share_get(id: &str) -> Result<Vec<u8>, ServerError> {
    use base64::Engine as _;
    #[derive(Deserialize)]
    struct Body {
        data: String,
    }
    let body: Body = post("/v1/share/get", json!({ "id": id })).await?;
    base64::engine::general_purpose::STANDARD
        .decode(body.data)
        .map_err(|e| ServerError::Network(e.to_string()))
}

/// Take share `id` off the server now. Only the account that made it may.
pub async fn share_revoke(refresh: &str, machine: &str, id: &str) -> Result<(), ServerError> {
    let _: serde_json::Value = post(
        "/v1/share/revoke",
        json!({ "refresh": refresh, "machine": machine, "id": id }),
    )
    .await?;
    Ok(())
}
