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
    /// A test key's length in minutes (its `days` is then 0).
    #[serde(default)]
    pub minutes: Option<i64>,
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
