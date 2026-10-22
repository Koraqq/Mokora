//! Chrome-for-Testing (CfT) downloader.
//!
//! Lets users without a system Chrome install one on demand from
//! Google's official CfT distribution. Avoids bundling a ~150MB browser
//! with the Mokora binary; downloads are runtime-only into a
//! `chrome-for-testing/` directory next to the binary (or the user
//! config dir as a fallback — see [`install_root`]).
//!
//! Manifest URLs (Google):
//! - All versions: <https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json>
//! - Channel pins: <https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json>
//!
//! Install layout under `<install_root>/<version>/`:
//! - Linux: `chrome-linux64/chrome`
//! - macOS: `chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`
//!   (or `chrome-mac-x64/...` on Intel)
//! - Windows: `chrome-win64\chrome.exe`
//!
//! Triggers (per design):
//! - Settings UI button (explicit)
//! - First-run wizard button (explicit)
//! - **Never** automatic on supervisor start — if no system browser AND
//!   no CfT installed, the supervisor surfaces a notification and
//!   refuses to start. The user clicks Download themselves.

use crate::event_bus::NotifyBus;
use crate::schema::Notification;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// CfT release channel + literal-version pin format used by
/// `ChromiumConfig.cft_channel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
    Dev,
    Canary,
    Literal(String),
}

impl Channel {
    pub fn parse(input: &str) -> Self {
        match input.to_ascii_lowercase().as_str() {
            "stable" | "" => Channel::Stable,
            "beta" => Channel::Beta,
            "dev" => Channel::Dev,
            "canary" => Channel::Canary,
            _ => Channel::Literal(input.to_string()),
        }
    }
}

/// CfT platform identifier expected by the manifest (e.g. `linux64`,
/// `mac-arm64`). Returns `None` on unsupported platforms.
pub fn cft_platform() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some("linux64")
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("mac-arm64")
        } else {
            Some("mac-x64")
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("win64")
        } else {
            Some("win32")
        }
    } else {
        None
    }
}

/// Top-level CfT install dir.
///
/// Resolved via [`crate::util::resolve_dir`] so a portable zip keeps the
/// downloaded browser next to the binary (single-folder install). On
/// AppImage / read-only mounts the resolver falls back to
/// `<user_config_root>/chrome-for-testing/`.
pub fn install_root() -> Result<PathBuf> {
    Ok(crate::util::resolve_dir(Path::new("./chrome-for-testing")))
}

/// Per-version install dir: `<install_root>/<version>/`.
pub fn install_dir_for(version: &str) -> Result<PathBuf> {
    Ok(install_root()?.join(version))
}

/// Map an install dir + platform to the chrome executable inside.
pub fn executable_path(install_dir: &Path, platform: &str) -> PathBuf {
    match platform {
        "linux64" => install_dir.join("chrome-linux64").join("chrome"),
        "mac-arm64" => install_dir
            .join("chrome-mac-arm64")
            .join("Google Chrome for Testing.app")
            .join("Contents/MacOS/Google Chrome for Testing"),
        "mac-x64" => install_dir
            .join("chrome-mac-x64")
            .join("Google Chrome for Testing.app")
            .join("Contents/MacOS/Google Chrome for Testing"),
        "win64" => install_dir.join("chrome-win64").join("chrome.exe"),
        "win32" => install_dir.join("chrome-win32").join("chrome.exe"),
        _ => install_dir.join(platform).join("chrome"),
    }
}

/// List installed CfT versions, newest first (lex-sort descending —
/// `131.0.6778.85 > 130.0.6723.92` because the dotted segments are
/// fixed-width-ish for the same major). Skips entries whose chrome
/// executable for the current platform is missing.
pub fn list_installed() -> Vec<String> {
    let Ok(root) = install_root() else {
        return vec![];
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return vec![];
    };
    let Some(platform) = cft_platform() else {
        return vec![];
    };
    let mut versions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let exe = executable_path(&path, platform);
        if !exe.exists() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            versions.push(name.to_string());
        }
    }
    versions.sort_by_key(|v| std::cmp::Reverse(version_sort_key(v)));
    versions
}

/// Best-effort `127.0.6778.85` → `[127, 0, 6778, 85]` for comparison.
/// Non-numeric components sort as `i64::MIN` so malformed names trail.
fn version_sort_key(v: &str) -> Vec<i64> {
    v.split('.')
        .map(|seg| seg.parse::<i64>().unwrap_or(i64::MIN))
        .collect()
}

/// Find an installed CfT executable. `pinned` (from
/// `ChromiumConfig.cft_channel`) is preferred when it parses as a
/// literal version and that version is installed; otherwise returns
/// the newest installed version's executable. `None` if nothing is
/// installed for the current platform.
pub fn installed_executable(pinned: &Channel) -> Option<PathBuf> {
    let platform = cft_platform()?;
    let installed = list_installed();
    if installed.is_empty() {
        return None;
    }
    let pick = match pinned {
        Channel::Literal(v) if installed.iter().any(|x| x == v) => v.clone(),
        _ => installed[0].clone(),
    };
    let dir = install_dir_for(&pick).ok()?;
    Some(executable_path(&dir, platform))
}

/// Every installed CfT version's executable, newest first.
pub fn installed_executables() -> Vec<PathBuf> {
    let Some(platform) = cft_platform() else {
        return vec![];
    };
    list_installed()
        .iter()
        .filter_map(|v| install_dir_for(v).ok())
        .map(|dir| executable_path(&dir, platform))
        .collect()
}

/// Remove a single installed CfT version. Idempotent — missing dir
/// returns Ok.
pub fn remove(version: &str) -> Result<()> {
    let dir = install_dir_for(version)?;
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))
}

// ---------- Manifest fetching ----------

#[derive(Debug, Clone, Deserialize)]
struct ManifestDownload {
    platform: String,
    url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ManifestDownloads {
    #[serde(default)]
    chrome: Vec<ManifestDownload>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChannelEntry {
    version: String,
    /// Absent in npmmirror's `last-known-good-versions.json` (the
    /// downloads-free variant is the only channels manifest the mirror
    /// carries) — the binary URL is constructed from the version instead.
    #[serde(default)]
    downloads: ManifestDownloads,
}

#[derive(Debug, Clone, Deserialize)]
struct ChannelsManifest {
    channels: ChannelsBag,
}

#[derive(Debug, Clone, Deserialize)]
struct ChannelsBag {
    #[serde(rename = "Stable")]
    stable: Option<ChannelEntry>,
    #[serde(rename = "Beta")]
    beta: Option<ChannelEntry>,
    #[serde(rename = "Dev")]
    dev: Option<ChannelEntry>,
    #[serde(rename = "Canary")]
    canary: Option<ChannelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionEntry {
    version: String,
    downloads: ManifestDownloads,
}

#[derive(Debug, Clone, Deserialize)]
struct AllVersionsManifest {
    versions: Vec<VersionEntry>,
}

const CHANNELS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
const ALL_VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json";

/// Official binary bucket — every `downloads.chrome[].url` in the
/// manifests points here. Hard-blocked in mainland China.
const GCS_BASE: &str = "https://storage.googleapis.com/chrome-for-testing-public";
/// npmmirror (Alibaba) mirrors the GCS bucket layout 1:1 plus a subset
/// of the metadata files: `last-known-good-versions.json` (channels,
/// no download URLs) and `known-good-versions-with-downloads.json`.
/// Verified 2026-08-12. The `-with-downloads` channels variant and the
/// `LATEST_RELEASE_*` pins are NOT mirrored.
const NPMMIRROR_BASE: &str = "https://registry.npmmirror.com/-/binary/chrome-for-testing";

/// Per-attempt ceiling on manifest fetches. The all-versions manifest is
/// ~5 MB, so this is generous; the point is that a black-holed
/// connection fails over to the mirror instead of hanging forever.
const MANIFEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Rewrite an official GCS binary URL to its npmmirror twin. `None`
/// when the URL isn't under the GCS bucket.
fn npmmirror_rewrite(url: &str) -> Option<String> {
    url.strip_prefix(GCS_BASE)
        .map(|rest| format!("{NPMMIRROR_BASE}{rest}"))
}

/// Binary URL constructed from scratch — used when the manifest came
/// from npmmirror's downloads-free channels file. The path layout
/// `<ver>/<platform>/chrome-<platform>.zip` is shared by both hosts.
fn binary_url(base: &str, version: &str, platform: &str) -> String {
    format!("{base}/{version}/{platform}/chrome-{platform}.zip")
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    what: &str,
) -> Result<T> {
    client
        .get(url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("fetch {what}"))?
        .error_for_status()
        .with_context(|| format!("{what} endpoint returned an error"))?
        .json()
        .await
        .with_context(|| format!("parse {what}"))
}

fn pick_channel(bag: ChannelsBag, channel: &Channel) -> Result<ChannelEntry> {
    match channel {
        Channel::Stable => bag.stable,
        Channel::Beta => bag.beta,
        Channel::Dev => bag.dev,
        Channel::Canary => bag.canary,
        Channel::Literal(_) => None,
    }
    .ok_or_else(|| anyhow!("channel not present in CfT manifest"))
}

/// Resolve a `Channel` to a concrete `(version, download candidates)`
/// for the host platform. Candidates are tried in order by the caller;
/// the second entry is always the other host's URL for the same file,
/// so a user who can reach the manifest but not the binary bucket (or
/// vice versa) still succeeds.
///
/// Official endpoints are tried first; when they are unreachable the
/// resolve falls back to npmmirror's mirrored metadata (channel pins
/// without download URLs, so the binary URL is constructed from the
/// shared bucket layout).
async fn resolve_asset(
    client: &reqwest::Client,
    channel: &Channel,
) -> Result<(String, Vec<String>)> {
    let platform = cft_platform()
        .ok_or_else(|| anyhow!("Chrome-for-Testing is not available for this platform"))?;
    match channel {
        Channel::Stable | Channel::Beta | Channel::Dev | Channel::Canary => {
            match fetch_json::<ChannelsManifest>(client, CHANNELS_URL, "CfT channels manifest")
                .await
            {
                Ok(m) => {
                    let entry = pick_channel(m.channels, channel)?;
                    let url = entry
                        .downloads
                        .chrome
                        .into_iter()
                        .find(|d| d.platform == platform)
                        .ok_or_else(|| {
                            anyhow!("CfT manifest has no {platform} asset for {}", entry.version)
                        })?
                        .url;
                    let mut candidates = vec![url.clone()];
                    candidates.extend(npmmirror_rewrite(&url));
                    Ok((entry.version, candidates))
                }
                Err(e) => {
                    warn!("official CfT channels manifest unreachable ({e:#}); trying npmmirror");
                    let m: ChannelsManifest = fetch_json(
                        client,
                        &format!("{NPMMIRROR_BASE}/last-known-good-versions.json"),
                        "npmmirror CfT channels manifest",
                    )
                    .await?;
                    let entry = pick_channel(m.channels, channel)?;
                    let candidates = vec![
                        binary_url(NPMMIRROR_BASE, &entry.version, platform),
                        binary_url(GCS_BASE, &entry.version, platform),
                    ];
                    Ok((entry.version, candidates))
                }
            }
        }
        Channel::Literal(v) => {
            let (m, from_mirror): (AllVersionsManifest, bool) =
                match fetch_json(client, ALL_VERSIONS_URL, "CfT all-versions manifest").await {
                    Ok(m) => (m, false),
                    Err(e) => {
                        warn!(
                        "official CfT all-versions manifest unreachable ({e:#}); trying npmmirror"
                    );
                        let m = fetch_json(
                            client,
                            &format!("{NPMMIRROR_BASE}/known-good-versions-with-downloads.json"),
                            "npmmirror CfT all-versions manifest",
                        )
                        .await?;
                        (m, true)
                    }
                };
            let entry = m
                .versions
                .into_iter()
                .find(|e| &e.version == v)
                .ok_or_else(|| anyhow!("CfT version {v:?} not found in manifest"))?;
            let url = entry
                .downloads
                .chrome
                .into_iter()
                .find(|d| d.platform == platform)
                .ok_or_else(|| anyhow!("CfT manifest has no {platform} asset for {v}"))?
                .url;
            let candidates = match (npmmirror_rewrite(&url), from_mirror) {
                // Manifest came from the mirror → the binary bucket is
                // likely blocked too; try the mirror first.
                (Some(alt), true) => vec![alt, url],
                (Some(alt), false) => vec![url, alt],
                (None, _) => vec![url],
            };
            Ok((entry.version, candidates))
        }
    }
}

/// The concrete version `channel` names, without downloading anything — for
/// telling the user whether what they typed is a Chrome for Testing release.
pub async fn resolve_version(channel: &Channel) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("mokora-cft-downloader")
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build http client")?;
    Ok(resolve_asset(&client, channel).await?.0)
}

// ---------- Download + extract ----------

/// Download + extract the requested CfT into the install dir. Reports
/// progress via NotifyBus with sticky id `capture-cft-download` so the
/// frontend can show a single live toast. Idempotent: if the version
/// is already installed, returns early.
pub async fn install(channel: &Channel, notify: &NotifyBus) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("mokora-cft-downloader")
        // Short connect ceiling: blocked hosts black-hole rather than
        // reset, and fallback to the mirror needs the attempt to *fail*.
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build http client")?;
    let toast = "capture-cft-download";

    let _ = notify.send(
        Notification::info("Resolving Chrome for Testing")
            .body(format!("Channel: {channel:?}"))
            .sticky()
            .id(toast),
    );
    let (version, url_candidates) = resolve_asset(&client, channel).await?;
    let install_dir = install_dir_for(&version)?;
    if install_dir.exists() && installed_chrome_exists(&install_dir) {
        let _ = notify.send(
            Notification::success(format!("Chrome for Testing {version} already installed"))
                .id(toast),
        );
        return Ok(version);
    }

    let _ = notify.send(
        Notification::info(format!("Downloading Chrome for Testing {version}"))
            .body("0%")
            .sticky()
            .id(toast),
    );

    // Stage download under <root>/.downloads/ so a partial transfer
    // doesn't pollute the version dir.
    let staging_root = install_root()?.join(".downloads");
    std::fs::create_dir_all(&staging_root)
        .with_context(|| format!("create {}", staging_root.display()))?;
    let zip_path = staging_root.join(format!("cft-{version}.zip"));
    // Google's own MD5 for the zip, read off its bucket before anything is
    // downloaded: the one thing that lets a mirror's copy be trusted.
    let official_url = url_candidates
        .iter()
        .find(|u| u.starts_with(GCS_BASE))
        .cloned()
        .or_else(|| cft_platform().map(|p| binary_url(GCS_BASE, &version, p)));
    let expected = match &official_url {
        Some(url) => match official_md5(&client, url).await {
            Ok(md5) => Some(md5),
            Err(e) => {
                warn!("could not read Google's checksum for CfT {version}: {e:#}");
                None
            }
        },
        None => None,
    };
    let mut last_err: Option<anyhow::Error> = None;
    let mut downloaded = false;
    for url in &url_candidates {
        // Not worth 200 MB of download to refuse it afterwards.
        if let Err(e) = may_install_from(url, expected.as_ref()) {
            warn!("skipping CfT download from {url}: {e:#}");
            last_err = Some(e);
            continue;
        }
        info!("downloading CfT {version} from {url}");
        let outcome = download_with_progress(&client, url, &zip_path, notify, toast, &version)
            .await
            .and_then(|actual| check_download(url, expected.as_ref(), &actual));
        match outcome {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => {
                warn!("CfT download from {url} failed: {e:#}");
                let _ = std::fs::remove_file(&zip_path);
                last_err = Some(e.context(format!("download {url}")));
            }
        }
    }
    if !downloaded {
        return Err(last_err.unwrap_or_else(|| anyhow!("no CfT download candidates")));
    }

    let _ = notify.send(
        Notification::info(format!("Installing Chrome for Testing {version}"))
            .body("Extracting…")
            .sticky()
            .id(toast),
    );
    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("create {}", install_dir.display()))?;
    crate::github::extract_zip_safe(&zip_path, &install_dir).context("extract CfT zip")?;
    let _ = std::fs::remove_file(&zip_path);

    if let Some(platform) = cft_platform() {
        let exe = executable_path(&install_dir, platform);
        if !exe.exists() {
            // Cleanup the half-installed dir to avoid `list_installed`
            // returning a broken version.
            let _ = std::fs::remove_dir_all(&install_dir);
            bail!(
                "CfT extract finished but expected executable {} is missing",
                exe.display()
            );
        }
        post_extract_fixup(&exe, &install_dir);
    }

    let _ = notify
        .send(Notification::success(format!("Chrome for Testing {version} installed")).id(toast));
    Ok(version)
}

fn installed_chrome_exists(install_dir: &Path) -> bool {
    let Some(platform) = cft_platform() else {
        return false;
    };
    executable_path(install_dir, platform).exists()
}

/// Defensive `chmod +x` (Unix) and macOS quarantine strip after
/// extraction. CfT zips DO carry unix mode bits via `extract_zip_safe`,
/// but a stray archive without them shouldn't leave a non-executable
/// binary behind. macOS Gatekeeper blocks unsigned binaries with the
/// quarantine xattr — strip it so first launch doesn't show a
/// "cannot verify developer" prompt.
#[cfg_attr(not(unix), allow(unused_variables))]
fn post_extract_fixup(exe: &Path, install_dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(exe) {
            let mut perms = meta.permissions();
            // Add user/group/other execute bits without dropping read/write.
            perms.set_mode(perms.mode() | 0o111);
            let _ = std::fs::set_permissions(exe, perms);
        }
    }
    if cfg!(target_os = "macos") {
        let status = std::process::Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(install_dir)
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {
                debug!("stripped quarantine xattr from {}", install_dir.display())
            }
            Ok(s) => warn!("xattr exited {s} (non-fatal)"),
            Err(e) => warn!("xattr not found / failed: {e} (non-fatal)"),
        }
    }
}

async fn download_with_progress(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    notify: &NotifyBus,
    toast: &str,
    version: &str,
) -> Result<[u8; 16]> {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    let mut response = client
        .get(url)
        .send()
        .await
        .context("send CfT download request")?
        .error_for_status()
        .context("CfT download endpoint returned error")?;
    let total = response.content_length();

    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("create {}", dest.display()))?;
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    // Bound the quiet time between chunks (not total transfer time) so a
    // stalled connection fails over to the next URL candidate.
    while let Some(chunk) =
        tokio::time::timeout(std::time::Duration::from_secs(60), response.chunk())
            .await
            .context("download stalled")?
            .context("read body chunk")?
    {
        file.write_all(&chunk)
            .await
            .with_context(|| format!("write {}", dest.display()))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        // Cap UI updates to ~4Hz; otherwise we drown the broadcast bus.
        if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            last_emit = std::time::Instant::now();
            let body = match total {
                Some(t) if t > 0 => format!(
                    "{:.0}% / {:.1} MB",
                    (downloaded as f64 / t as f64) * 100.0,
                    t as f64 / 1_048_576.0,
                ),
                _ => format!("{:.1} MB", downloaded as f64 / 1_048_576.0),
            };
            let _ = notify.send(
                Notification::info(format!("Downloading Chrome for Testing {version}"))
                    .body(body)
                    .sticky()
                    .id(toast),
            );
        }
    }
    file.flush().await.ok();
    Ok(hasher
        .finalize()
        .as_slice()
        .try_into()
        .expect("MD5 is 16 bytes"))
}

/// The MD5 Google's bucket holds for the object at `url`, from the
/// `x-goog-hash` header of a HEAD request. The Chrome for Testing manifests
/// list no checksums; the bucket's own object metadata is the one official
/// record of what the file is.
async fn official_md5(client: &reqwest::Client, url: &str) -> Result<[u8; 16]> {
    let response = client
        .head(url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .context("ask Google's bucket for the checksum")?
        .error_for_status()
        .context("Google's bucket returned an error")?;
    md5_from_goog_hash(
        response
            .headers()
            .get_all("x-goog-hash")
            .iter()
            .filter_map(|v| v.to_str().ok()),
    )
    .ok_or_else(|| anyhow!("no MD5 in the bucket's x-goog-hash header"))
}

/// The MD5 among `x-goog-hash` values (`crc32c=…`, `md5=…`, either one per
/// header or comma-joined), base64-decoded.
fn md5_from_goog_hash<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<[u8; 16]> {
    use base64::Engine;
    values
        .into_iter()
        .flat_map(|v| v.split(','))
        .find_map(|part| {
            let b64 = part.trim().strip_prefix("md5=")?;
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .ok()?
                .try_into()
                .ok()
        })
}

/// Whether a download from `url` can be checked at all. Google's own bucket
/// is trusted over TLS even without the checksum; a mirror's copy is only
/// ever installed once Google's checksum is in hand to check it against.
fn may_install_from(url: &str, expected: Option<&[u8; 16]>) -> Result<()> {
    if expected.is_none() && !url.starts_with(GCS_BASE) {
        bail!(
            "the copy at {url} cannot be checked against Google's checksum (Google is \
             unreachable), and an unchecked browser is not installed. Install Google \
             Chrome instead — Mokora uses an installed Chrome first."
        );
    }
    Ok(())
}

/// Whether the downloaded bytes (MD5 `actual`) are Google's file.
fn check_download(url: &str, expected: Option<&[u8; 16]>, actual: &[u8; 16]) -> Result<()> {
    may_install_from(url, expected)?;
    match expected {
        Some(e) if e != actual => bail!(
            "the file from {url} does not match Google's checksum; it was discarded"
        ),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIRROR: &str = "https://registry.npmmirror.com/-/binary/chrome-for-testing/1/win64/chrome-win64.zip";
    const OFFICIAL: &str = "https://storage.googleapis.com/chrome-for-testing-public/1/win64/chrome-win64.zip";

    /// Real header values from Google's bucket (checked 2026-10-21 against
    /// the 155.0.8059.84 win64 zip), one value per header.
    #[test]
    fn md5_is_read_from_the_bucket_header() {
        let got = md5_from_goog_hash(["crc32c=K2Cazg==", "md5=Li+h/75z5JN0lGJFOJ1i0w=="]).unwrap();
        assert_eq!(
            got,
            [
                0x2e, 0x2f, 0xa1, 0xff, 0xbe, 0x73, 0xe4, 0x93, 0x74, 0x94, 0x62, 0x45, 0x38,
                0x9d, 0x62, 0xd3
            ]
        );
        // Comma-joined in one header works the same.
        assert_eq!(
            md5_from_goog_hash(["crc32c=K2Cazg==, md5=Li+h/75z5JN0lGJFOJ1i0w=="]),
            Some(got)
        );
        assert_eq!(md5_from_goog_hash(["crc32c=K2Cazg=="]), None);
        assert_eq!(md5_from_goog_hash(["md5=notbase64!"]), None);
    }

    /// Choice 1: a mirror's copy is only installed against Google's checksum.
    #[test]
    fn a_mirror_copy_needs_googles_checksum() {
        let sum = [7u8; 16];
        assert!(may_install_from(MIRROR, None).is_err(), "unverifiable: not even downloaded");
        assert!(may_install_from(MIRROR, Some(&sum)).is_ok());
        assert!(may_install_from(OFFICIAL, None).is_ok(), "Google's bucket over TLS");

        assert!(check_download(MIRROR, Some(&sum), &sum).is_ok());
        assert!(check_download(MIRROR, Some(&sum), &[0u8; 16]).is_err(), "tampered");
        assert!(check_download(OFFICIAL, Some(&sum), &[0u8; 16]).is_err(), "corrupted");
        assert!(check_download(OFFICIAL, None, &[0u8; 16]).is_ok());
    }

    #[test]
    fn channel_parses() {
        assert_eq!(Channel::parse("stable"), Channel::Stable);
        assert_eq!(Channel::parse("STABLE"), Channel::Stable);
        assert_eq!(Channel::parse(""), Channel::Stable);
        assert_eq!(Channel::parse("beta"), Channel::Beta);
        assert_eq!(Channel::parse("dev"), Channel::Dev);
        assert_eq!(Channel::parse("canary"), Channel::Canary);
        assert_eq!(
            Channel::parse("131.0.6778.85"),
            Channel::Literal("131.0.6778.85".into())
        );
    }

    #[test]
    fn cft_platform_returns_known_string_on_supported_targets() {
        // Just exercise the cfg branches — actual value depends on host.
        let p = cft_platform();
        if cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        )) {
            assert!(p.is_some());
            let s = p.unwrap();
            assert!(["linux64", "mac-arm64", "mac-x64", "win64", "win32"].contains(&s));
        }
    }

    #[test]
    fn executable_path_per_platform() {
        let dir = PathBuf::from("/tmp/cft/131.0.6778.85");
        assert_eq!(
            executable_path(&dir, "linux64"),
            PathBuf::from("/tmp/cft/131.0.6778.85/chrome-linux64/chrome")
        );
        assert_eq!(
            executable_path(&dir, "win64"),
            PathBuf::from("/tmp/cft/131.0.6778.85/chrome-win64/chrome.exe")
        );
        let mac = executable_path(&dir, "mac-arm64");
        assert!(
            mac.ends_with("Contents/MacOS/Google Chrome for Testing"),
            "got {}",
            mac.display()
        );
    }

    #[test]
    fn version_sort_orders_numerically() {
        let mut v = vec![
            "129.0.6668.58".to_string(),
            "131.0.6778.85".into(),
            "130.0.6723.92".into(),
        ];
        v.sort_by_key(|x| std::cmp::Reverse(version_sort_key(x)));
        assert_eq!(
            v,
            vec![
                "131.0.6778.85".to_string(),
                "130.0.6723.92".into(),
                "129.0.6668.58".into(),
            ]
        );
    }

    #[test]
    fn version_sort_handles_garbage_gracefully() {
        let key_good = version_sort_key("131.0.6778.85");
        let key_garbage = version_sort_key("not-a-version");
        // garbage should sort *less* than well-formed (i64::MIN component)
        assert!(key_garbage < key_good);
    }

    #[test]
    fn npmmirror_rewrite_maps_gcs_urls_only() {
        assert_eq!(
            npmmirror_rewrite(
                "https://storage.googleapis.com/chrome-for-testing-public/151.0.7922.138/linux64/chrome-linux64.zip"
            )
            .as_deref(),
            Some(
                "https://registry.npmmirror.com/-/binary/chrome-for-testing/151.0.7922.138/linux64/chrome-linux64.zip"
            )
        );
        assert_eq!(npmmirror_rewrite("https://example.com/chrome.zip"), None);
    }

    #[test]
    fn binary_url_matches_bucket_layout() {
        assert_eq!(
            binary_url(GCS_BASE, "151.0.7922.138", "win64"),
            "https://storage.googleapis.com/chrome-for-testing-public/151.0.7922.138/win64/chrome-win64.zip"
        );
        assert_eq!(
            binary_url(NPMMIRROR_BASE, "151.0.7922.138", "mac-arm64"),
            "https://registry.npmmirror.com/-/binary/chrome-for-testing/151.0.7922.138/mac-arm64/chrome-mac-arm64.zip"
        );
    }

    /// npmmirror only carries the downloads-free channels manifest
    /// (`last-known-good-versions.json`); its entries must parse with
    /// `downloads` defaulting to empty.
    #[test]
    fn channels_manifest_parses_npmmirror_shape_without_downloads() {
        let body = r#"{
            "timestamp": "2026-08-11T22:20:47.187Z",
            "channels": {
                "Stable": { "channel": "Stable", "version": "151.0.7922.138", "revision": "1654411" },
                "Beta":   { "channel": "Beta",   "version": "152.0.7977.30",  "revision": "1669021" }
            }
        }"#;
        let m: ChannelsManifest = serde_json::from_str(body).unwrap();
        let stable = pick_channel(m.channels.clone(), &Channel::Stable).unwrap();
        assert_eq!(stable.version, "151.0.7922.138");
        assert!(stable.downloads.chrome.is_empty());
        let beta = pick_channel(m.channels, &Channel::Beta).unwrap();
        assert_eq!(beta.version, "152.0.7977.30");
    }

    #[test]
    fn pick_channel_missing_channel_errors() {
        let body = r#"{ "channels": { "Stable": { "version": "1.2.3.4", "downloads": {} } } }"#;
        let m: ChannelsManifest = serde_json::from_str(body).unwrap();
        assert!(pick_channel(m.channels, &Channel::Canary).is_err());
    }

    #[test]
    fn channels_manifest_parses_real_shape() {
        // Real-world shape sample (truncated). Keep the test offline.
        let body = r#"{
            "timestamp": "2026-01-01",
            "channels": {
                "Stable": {
                    "channel": "Stable",
                    "version": "131.0.6778.85",
                    "revision": "1",
                    "downloads": {
                        "chrome": [
                            { "platform": "linux64", "url": "https://example.test/chrome-linux64.zip" },
                            { "platform": "mac-arm64", "url": "https://example.test/chrome-mac-arm64.zip" }
                        ]
                    }
                }
            }
        }"#;
        let m: ChannelsManifest = serde_json::from_str(body).unwrap();
        let stable = m.channels.stable.unwrap();
        assert_eq!(stable.version, "131.0.6778.85");
        assert_eq!(stable.downloads.chrome.len(), 2);
    }
}
