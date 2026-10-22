//! Release-asset signature verification (minisign format).
//!
//! Release zips are signed in CI (see `.github/workflows/release.yml`);
//! the matching public key is embedded below and also tracked at the
//! repo root as `minisign.pub` for manual verification. The signature —
//! not the SHA-256 digest from release metadata — is what makes
//! third-party download mirrors safe to use: both the metadata and the
//! bytes can come from an untrusted mirror, but only the holder of the
//! secret key can produce a valid signature.
//!
//! The signature's *trusted comment* is set to the asset filename at
//! signing time and checked here, so a mirror cannot answer a request
//! for `mokora-3.6.0-…` with a validly-signed zip from an older release.
//!
//! Signatures must be in minisign's pre-hashed mode (the default for
//! every modern minisign/rsign2); legacy non-prehashed signatures are
//! rejected because they cannot be verified in a stream.

use anyhow::{bail, Context, Result};
use minisign_verify::{PublicKey, Signature};
use std::io::Read;
use std::path::Path;

/// Minisign public key for Mokora release assets (the base64 line of
/// `minisign.pub` at the repo root).
pub const RELEASE_PUBKEY_B64: &str = "RWRqps4WlCQRRgfCHe0qjYF4dDV81//o7Ew5740uZYPt7ENKsrjOCZxM";

/// Verify `file` against `minisig` (the text of a `.minisig` document)
/// using the embedded Mokora release key. `expected_trusted_comment` is
/// the asset filename the caller intended to download.
pub fn verify_release_asset(
    file: &Path,
    minisig: &str,
    expected_trusted_comment: &str,
) -> Result<()> {
    verify_with_key(RELEASE_PUBKEY_B64, file, minisig, expected_trusted_comment)
}

/// Key-parameterised implementation, split out so tests can use a
/// throwaway keypair instead of the production key.
pub fn verify_with_key(
    pubkey_b64: &str,
    file: &Path,
    minisig: &str,
    expected_trusted_comment: &str,
) -> Result<()> {
    let pk = PublicKey::from_base64(pubkey_b64).context("decode minisign public key")?;
    let sig = Signature::decode(minisig).context("decode .minisig document")?;

    // Checked *before* the expensive hash: a mismatched comment can never
    // become valid, and the error message is the more actionable one.
    // The comment itself is covered by the global signature, which
    // `finalize()` verifies — a forged comment fails there.
    if sig.trusted_comment() != expected_trusted_comment {
        bail!(
            "signature is for {:?}, expected {:?} — refusing (possible version swap by a mirror)",
            sig.trusted_comment(),
            expected_trusted_comment
        );
    }

    // Stream the file through the verifier — release zips are hundreds
    // of MB and never need to be resident in memory.
    let mut verifier = pk.verify_stream(&sig).context(
        "initialise signature verifier (legacy non-prehashed signatures are unsupported)",
    )?;
    let f = std::fs::File::open(file).with_context(|| format!("open {}", file.display()))?;
    let mut reader = std::io::BufReader::with_capacity(1 << 20, f);
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("read {}", file.display()))?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier
        .finalize()
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Throwaway test keypair (rsign2-generated; NOT the release key —
    // its secret half was discarded after producing these fixtures).
    const TEST_PUBKEY_B64: &str = "RWQ6eM2SnUt9bjjgf1W1fDUfBXfDACotVVC8r/EzcXqRfa2LpMAJafjv";
    const TEST_PAYLOAD: &[u8] = b"mokora signing test payload\n";
    /// Signature over TEST_PAYLOAD, trusted comment `mokora-9.9.9-linux-x64.zip`.
    const SIG_GOOD: &str = "untrusted comment: signature from rsign secret key\n\
RUQ6eM2SnUt9btGiGqhRhF2pXsM2UTaCcOedKEiXMim/rDqIBJCDl/rXDn8avQIoRSkLdzCQpR/zRj3e2JRuSm8wMrAQpasdLw8=\n\
trusted comment: mokora-9.9.9-linux-x64.zip\n\
EDEZouuPZC1aYOmbrXwVis7xwq2uMZsAlBcS+1iy5E0YChTFe7dIfQm5LV/gRXLlTw7QLN+RXd93tUyE4U9kAQ==\n";
    /// Same payload, validly signed, but trusted comment names an older
    /// asset (`mokora-0.0.1-linux-x64.zip`) — the version-swap case.
    const SIG_WRONG_NAME: &str = "untrusted comment: signature from rsign secret key\n\
RUQ6eM2SnUt9bqauI3EkBBJP2l/nMNxSFC0eH/Td6rLIduUggdT5DlSYVqTopDm/RQvPpcE/NUVHNRfkyebzHkVqDglQsP0wKgo=\n\
trusted comment: mokora-0.0.1-linux-x64.zip\n\
PBstXUZxlBOIvXAdOxER0A+HvkYwAJgiJC/9RbIR+q3zVtANLbP5Zl/zXaB1rBMwvEIC6NGa1W/IH32LGtaiAg==\n";

    fn payload_file(bytes: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn valid_signature_verifies() {
        let f = payload_file(TEST_PAYLOAD);
        verify_with_key(
            TEST_PUBKEY_B64,
            f.path(),
            SIG_GOOD,
            "mokora-9.9.9-linux-x64.zip",
        )
        .expect("good signature must verify");
    }

    #[test]
    fn tampered_payload_fails() {
        let f = payload_file(b"mokora signing test payload!\n");
        let err = verify_with_key(
            TEST_PUBKEY_B64,
            f.path(),
            SIG_GOOD,
            "mokora-9.9.9-linux-x64.zip",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("verification failed"),
            "got: {err:#}"
        );
    }

    #[test]
    fn wrong_trusted_comment_fails_even_when_validly_signed() {
        let f = payload_file(TEST_PAYLOAD);
        let err = verify_with_key(
            TEST_PUBKEY_B64,
            f.path(),
            SIG_WRONG_NAME,
            "mokora-9.9.9-linux-x64.zip",
        )
        .unwrap_err();
        assert!(err.to_string().contains("version swap"), "got: {err:#}");
    }

    #[test]
    fn wrong_key_fails() {
        let f = payload_file(TEST_PAYLOAD);
        // The production key did not sign this fixture.
        let err = verify_with_key(
            RELEASE_PUBKEY_B64,
            f.path(),
            SIG_GOOD,
            "mokora-9.9.9-linux-x64.zip",
        )
        .unwrap_err();
        // key-id mismatch surfaces from Signature/PublicKey pairing
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn garbage_signature_document_fails_to_decode() {
        let f = payload_file(TEST_PAYLOAD);
        let err = verify_with_key(TEST_PUBKEY_B64, f.path(), "not a minisig", "x").unwrap_err();
        assert!(err.to_string().contains("decode"), "got: {err:#}");
    }
}
