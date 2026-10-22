//! The server's license token: `base64url(payload JSON) "." base64url(sig)`,
//! signed with Ed25519 by the license server. Checked here with the public
//! halves, so nobody without a server's private key can make or change one.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

/// The license servers' public keys (MokoraKeys `server/public-key*.txt`):
/// production, then the test server.
pub const PUBLIC_KEYS: &[&str] = &[
    "VrsgVeKJY7weaNEC2sSsRGKTYaStQUSc5Q8APdrc2Vc=",
    "GArC2PQXy1vKAFYx6bssPadS0AC74LSxKaWGfC06gCs=",
];

/// What a token says.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub v: u32,
    /// The account's id on the server.
    pub uid: i64,
    /// The machine it was issued for.
    pub m: String,
    /// Unlocked features (empty when the account has no time).
    #[serde(default)]
    pub f: Vec<String>,
    pub iat: i64,
    /// When this token stops working unless renewed.
    pub exp: i64,
    /// When the account's time ends.
    pub paid_until: Option<i64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    Signature,
    #[error("issued for another computer")]
    OtherMachine,
}

/// Check `token`'s signature and that it was issued for `machine`. Expiry is
/// the caller's to compare (it knows the time).
pub fn verify(token: &str, machine: &str) -> Result<Claims, TokenError> {
    let mut last = TokenError::Malformed;
    for key in PUBLIC_KEYS {
        match verify_with(key, token, machine) {
            Err(TokenError::Signature) => last = TokenError::Signature,
            other => return other,
        }
    }
    Err(last)
}

fn verify_with(public_key: &str, token: &str, machine: &str) -> Result<Claims, TokenError> {
    let (body, sig) = token.split_once('.').ok_or(TokenError::Malformed)?;
    let key: [u8; 32] = STANDARD
        .decode(public_key)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(TokenError::Malformed)?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| TokenError::Malformed)?;
    let sig: [u8; 64] = URL_SAFE_NO_PAD
        .decode(sig)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(TokenError::Malformed)?;
    key.verify_strict(body.as_bytes(), &Signature::from_bytes(&sig))
        .map_err(|_| TokenError::Signature)?;
    let payload = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| TokenError::Malformed)?;
    let claims: Claims = serde_json::from_slice(&payload).map_err(|_| TokenError::Malformed)?;
    if claims.m != machine {
        return Err(TokenError::OtherMachine);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sign(key: &SigningKey, payload: &str) -> String {
        let body = URL_SAFE_NO_PAD.encode(payload);
        let sig = key.sign(body.as_bytes());
        format!("{body}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
    }

    fn pair() -> (SigningKey, String) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        (key, public)
    }

    const PAYLOAD: &str =
        r#"{"v":2,"uid":3,"m":"abc","f":["pro"],"iat":10,"exp":20,"paid_until":30}"#;

    #[test]
    fn a_signed_token_for_this_machine_verifies() {
        let (key, public) = pair();
        let c = verify_with(&public, &sign(&key, PAYLOAD), "abc").unwrap();
        assert_eq!(
            (c.uid, c.exp, c.paid_until, c.f),
            (3, 20, Some(30), vec!["pro".to_string()])
        );
    }

    #[test]
    fn another_machine_or_a_changed_payload_is_refused() {
        let (key, public) = pair();
        let token = sign(&key, PAYLOAD);
        assert_eq!(
            verify_with(&public, &token, "xyz"),
            Err(TokenError::OtherMachine)
        );
        let (_, sig) = token.split_once('.').unwrap();
        let forged = format!(
            "{}.{sig}",
            URL_SAFE_NO_PAD.encode(PAYLOAD.replace("20", "99"))
        );
        assert_eq!(
            verify_with(&public, &forged, "abc"),
            Err(TokenError::Signature)
        );
        assert_eq!(
            verify_with(&public, "nonsense", "abc"),
            Err(TokenError::Malformed)
        );
    }

    /// A token signed by a key that is not built in is refused.
    #[test]
    fn an_unknown_signer_is_refused() {
        let (key, _) = pair();
        assert_eq!(
            verify(&sign(&key, PAYLOAD), "abc"),
            Err(TokenError::Signature)
        );
    }

    #[test]
    fn the_built_in_keys_are_valid_ed25519_keys() {
        for k in PUBLIC_KEYS {
            let raw: [u8; 32] = STANDARD.decode(k).unwrap().try_into().unwrap();
            assert!(VerifyingKey::from_bytes(&raw).is_ok());
        }
    }
}
