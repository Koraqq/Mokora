//! Masking of login credentials in what Mokora writes about the game's
//! traffic.
//!
//! The bridge logs every decoded message — to the app log, the per-flow
//! protocol log, and the inspector timeline — and the client's login carries
//! a reusable access token in plain sight (`oauth2Check`, `oauth2Login`).
//! Anyone holding that token can sign in as the player, and logs are exactly
//! what gets pasted into bug reports. So, unless `[privacy]
//! redact_credentials` is off, the *written* copies are masked; the message
//! the bridge dispatches on is never touched.
//!
//! A frame whose decoded form had something masked also carries the secret
//! in its raw bytes, so capture backends drop those bytes too — see
//! [`crate::bridge::ParseResult::redacted`].
//!
//! The switch is process-wide (an atomic, set from config at startup and on
//! every save) because the bridges that read it are built deep inside the
//! capture backends, which take no config of their own.

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

/// What a masked value is replaced with.
pub const MASK: &str = "***";

/// What a dropped raw frame is recorded as.
pub const RAW_PLACEHOLDER: &str = "[redacted: frame carries login credentials]";

/// Field names whose string values are credentials, as named in the Majsoul
/// protocol (`liqi.proto`): the OAuth access token, account passwords, and
/// the per-session tokens that let a holder join as the player.
const SECRET_KEYS: &[&str] = &[
    "access_token",
    "password",
    "new_password",
    "old_password",
    "token",
    "connect_token",
    "secure_token",
    "nsa_token",
];

/// HTTP headers whose whole value is a credential. Recorded HTTP traffic is
/// the game's analytics beacons by default, which carry none; with
/// `[capture.http] record_all` on, everything is recorded, and these are what
/// would put a login into the session file.
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
];

/// A header's value as it may be written down: masked for a
/// credential-carrying header while masking is on, unchanged otherwise.
pub fn header_value(name: &str, value: String) -> String {
    mask_header(enabled(), name, value)
}

fn mask_header(on: bool, name: &str, value: String) -> String {
    let secret = SECRET_HEADERS.iter().any(|h| name.eq_ignore_ascii_case(h));
    if on && secret && !value.is_empty() {
        MASK.to_string()
    } else {
        value
    }
}

/// Follow `[privacy] redact_credentials`.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A copy of `value` with every credential masked, or `None` when there was
/// nothing to mask (or masking is off) — so the common case costs no clone.
pub fn redact(value: &Value) -> Option<Value> {
    if !enabled() || !has_secret(value) {
        return None;
    }
    let mut copy = value.clone();
    mask(&mut copy);
    Some(copy)
}

fn is_secret(key: &str, value: &Value) -> bool {
    SECRET_KEYS.contains(&key) && value.as_str().is_some_and(|s| !s.is_empty())
}

fn has_secret(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| is_secret(k, v) || has_secret(v)),
        Value::Array(items) => items.iter().any(has_secret),
        _ => false,
    }
}

fn mask(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_secret(k, v) {
                    *v = Value::String(MASK.to_string());
                } else {
                    mask(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credential_headers_are_masked_only_while_masking_is_on() {
        assert_eq!(mask_header(true, "Cookie", "sid=abc".into()), MASK);
        assert_eq!(mask_header(true, "authorization", "Bearer x".into()), MASK);
        assert_eq!(mask_header(true, "Set-Cookie", "a=b".into()), MASK);
        assert_eq!(mask_header(true, "content-type", "text/plain".into()), "text/plain");
        assert_eq!(mask_header(true, "cookie", String::new()), "", "nothing to hide");
        assert_eq!(mask_header(false, "cookie", "sid=abc".into()), "sid=abc");
    }

    /// The shapes seen in real logs: the token at the top of `oauth2Check`,
    /// and nested inside a login response.
    #[test]
    fn credentials_are_masked_wherever_they_sit() {
        let check = json!({ "access_token": "968c01fc-39e5", "type": 0 });
        assert_eq!(
            redact(&check),
            Some(json!({ "access_token": MASK, "type": 0 }))
        );
        let nested =
            json!({ "account": { "nickname": "x" }, "list": [{ "connect_token": "abc" }] });
        let masked = redact(&nested).unwrap();
        assert_eq!(masked["list"][0]["connect_token"], MASK);
        assert_eq!(
            masked["account"]["nickname"], "x",
            "everything else is kept"
        );
    }

    /// Nothing to hide ⇒ no copy; an empty token or a flag that merely
    /// *names* one (`gen_access_token: true`) is not a secret.
    #[test]
    fn frames_without_credentials_pass_untouched() {
        assert_eq!(redact(&json!({ "tile": "5m" })), None);
        assert_eq!(redact(&json!({ "access_token": "" })), None);
        assert_eq!(redact(&json!({ "gen_access_token": true })), None);
    }
}
