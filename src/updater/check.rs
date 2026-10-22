//! Version check against the latest GitHub release. The only IO is the
//! release-metadata fetch (a thin wrapper around `reqwest`, with mirror
//! fallback); the comparison is pure and unit-tested.

use crate::config::NetworkConfig;
use crate::github::{build_client, fetch_latest_release_mirrored, ReleaseJson};
use anyhow::{Context, Result};
use semver::Version;
use serde::Serialize;

/// A newer release than the one running, as the update prompt shows it.
///
/// Only a version, its notes and where to get it: the user downloads and
/// runs the installer from the release page themselves, so nothing here is
/// ever acted on beyond opening `html_url`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// Version we're running right now (`env!("CARGO_PKG_VERSION")`).
    pub current: String,
    /// Release tag verbatim — e.g. `v0.3.0`.
    pub latest_tag: String,
    /// `latest_tag` with any leading `v` stripped, validated as semver.
    pub latest_version: String,
    /// The release notes (`release.body`), shown as plain text. Empty if
    /// the release has none.
    pub body: String,
    /// The repository's latest-release page. Built from the repository name,
    /// never taken from the release metadata, so metadata fetched through a
    /// mirror cannot choose where the "Go" button sends the user.
    pub html_url: String,
}

/// `true` iff `latest` is a strictly-newer semver than `current`. Both
/// inputs accept a leading `v`. Malformed inputs return `false` — we'd
/// rather skip an update than crash.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let Ok(c) = Version::parse(current.trim_start_matches('v')) else {
        return false;
    };
    let Ok(l) = Version::parse(latest.trim_start_matches('v')) else {
        return false;
    };
    l > c
}

/// Extract the hex SHA-256 portion of a GitHub asset `digest` string,
/// which is shaped like `"sha256:abcdef..."`. Returns `None` for an
/// empty / wrong-algorithm / wrong-shape input.
pub fn parse_sha256_digest(raw: &str) -> Option<String> {
    let (algo, hex) = raw.split_once(':')?;
    if !algo.eq_ignore_ascii_case("sha256") {
        return None;
    }
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// Ask GitHub (directly or via mirrors, per `net`) for the latest release
/// and compare it with the running version. `Ok(None)` when up to date.
pub async fn check_for_update(repo: &str, net: &NetworkConfig) -> Result<Option<UpdateInfo>> {
    let client = build_client()?;
    let (release, _source) = fetch_latest_release_mirrored(&client, repo, net).await?;
    build_update_info(env!("CARGO_PKG_VERSION"), repo, &release)
}

/// Pure half of `check_for_update`, split out so tests can drive it
/// without a network.
///
/// The release's attachments play no part: the prompt only points at the
/// release page, so it works whatever the installer is called.
pub fn build_update_info(
    current: &str,
    repo: &str,
    release: &ReleaseJson,
) -> Result<Option<UpdateInfo>> {
    let tag = release
        .tag_name
        .clone()
        .context("release has no tag_name; can't compare versions")?;
    if !is_newer(current, &tag) {
        return Ok(None);
    }
    Ok(Some(UpdateInfo {
        current: current.to_owned(),
        latest_version: tag.trim_start_matches('v').to_owned(),
        latest_tag: tag,
        body: release.body.clone().unwrap_or_default(),
        html_url: format!("https://github.com/{repo}/releases/latest"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Asset;

    #[test]
    fn is_newer_basic_bumps() {
        assert!(is_newer("3.0.11", "3.0.12"));
        assert!(is_newer("3.0.11", "3.1.0"));
        assert!(is_newer("3.0.11", "4.0.0"));
        assert!(!is_newer("3.0.12", "3.0.11"));
        assert!(!is_newer("3.0.12", "3.0.12"));
    }

    #[test]
    fn is_newer_strips_v_prefix() {
        assert!(is_newer("3.0.11", "v3.0.12"));
        assert!(is_newer("v3.0.11", "3.0.12"));
        assert!(is_newer("v3.0.11", "v3.0.12"));
    }

    /// Pre-release tags stay numeric (`3.0.0-1`, `3.0.0-2`, …). Semver's
    /// numeric identifier ordering must order these correctly — string
    /// comparison would put `3.0.0-10` *before* `3.0.0-2`.
    #[test]
    fn is_newer_numeric_prerelease_ordering() {
        assert!(is_newer("3.0.0-2", "3.0.0-10"));
        assert!(is_newer("3.0.0-1", "3.0.0-2"));
        assert!(!is_newer("3.0.0-10", "3.0.0-2"));
        // Pre-release is always less than the corresponding release.
        assert!(is_newer("3.0.0-5", "3.0.0"));
        assert!(!is_newer("3.0.0", "3.0.0-5"));
    }

    #[test]
    fn is_newer_malformed_returns_false() {
        assert!(!is_newer("3.0.11", "not-a-version"));
        assert!(!is_newer("garbage", "3.0.12"));
        assert!(!is_newer("", ""));
    }

    #[test]
    fn parse_sha256_digest_accepts_valid_form() {
        let raw = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert_eq!(parse_sha256_digest(raw), Some(raw[7..].to_owned()));
    }

    #[test]
    fn parse_sha256_digest_normalises_case() {
        let raw = "SHA256:ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert_eq!(
            parse_sha256_digest(raw).unwrap(),
            raw[7..].to_ascii_lowercase()
        );
    }

    #[test]
    fn parse_sha256_digest_rejects_wrong_algo_or_shape() {
        assert!(parse_sha256_digest("md5:abc").is_none());
        assert!(parse_sha256_digest("sha256:").is_none());
        assert!(parse_sha256_digest("sha256:notenoughhex").is_none());
        assert!(parse_sha256_digest("nocolonhere").is_none());
        // Not 64 chars
        assert!(parse_sha256_digest("sha256:abc123").is_none());
        // Non-hex char
        let bad = "sha256:".to_owned() + &"z".repeat(64);
        assert!(parse_sha256_digest(&bad).is_none());
    }

    fn release_with(tag: &str, assets: Vec<Asset>) -> ReleaseJson {
        ReleaseJson {
            tag_name: Some(tag.to_owned()),
            name: None,
            body: Some("notes".into()),
            assets,
        }
    }

    fn setup_exe(version: &str) -> Asset {
        Asset {
            name: format!("Mokora_{version}_x64-setup.exe"),
            browser_download_url: format!("https://example.com/Mokora_{version}_x64-setup.exe"),
            size: Some(1),
            digest: None,
        }
    }

    #[test]
    fn same_version_is_no_update() {
        let release = release_with("v0.3.0", vec![setup_exe("0.3.0")]);
        assert!(build_update_info("0.3.0", "owner/repo", &release)
            .unwrap()
            .is_none());
    }

    /// The shape releases actually have since 0.2.2 — a setup.exe and
    /// SHA256SUMS, no zip. The old check looked for
    /// `mokora-<version>-windows-x64.zip` and so never found an update.
    #[test]
    fn a_newer_release_with_only_an_installer_is_found() {
        let release = release_with("v0.3.0", vec![setup_exe("0.3.0")]);
        let info = build_update_info("0.2.4", "owner/repo", &release)
            .unwrap()
            .expect("should detect newer version");
        assert_eq!(info.current, "0.2.4");
        assert_eq!(info.latest_tag, "v0.3.0");
        assert_eq!(info.latest_version, "0.3.0");
        assert_eq!(info.body, "notes");
        // Canonical, not release-supplied: metadata fetched through a
        // mirror must not choose where the user is sent.
        assert_eq!(info.html_url, "https://github.com/owner/repo/releases/latest");
    }

    #[test]
    fn a_release_without_attachments_still_counts() {
        let release = release_with("v0.3.1", vec![]);
        assert!(build_update_info("0.3.0", "owner/repo", &release)
            .unwrap()
            .is_some());
    }

    #[test]
    fn an_older_release_is_no_update() {
        let release = release_with("v0.2.4", vec![setup_exe("0.2.4")]);
        assert!(build_update_info("0.3.0", "owner/repo", &release)
            .unwrap()
            .is_none());
    }

    #[test]
    fn missing_tag_is_an_error() {
        let release = ReleaseJson {
            tag_name: None,
            name: None,
            body: None,
            assets: vec![],
        };
        assert!(build_update_info("3.0.11", "owner/repo", &release).is_err());
    }
}
