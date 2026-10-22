//! Installing a newer release from inside the app ("Update" in the prompt).
//!
//! The release's own package for this platform is downloaded and handed over,
//! then Mokora closes so the files can be replaced:
//!
//! - Windows: the NSIS installer (`*_x64-setup.exe`), run passive with
//!   `/UPDATE` (install over the old version, no uninstall first), `/R` (start
//!   Mokora again when done) and `/D=` the folder Mokora runs from, so the
//!   update lands where the program and its data are, whatever the registry
//!   says.
//! - Linux, run as an AppImage (`$APPIMAGE`): the new AppImage replaces the
//!   file and is started.
//! - Linux, installed from the .deb: `pkexec dpkg -i` (the desktop asks for
//!   the password), then Mokora is started again.
//!
//! Integrity: the bytes must match the SHA-256 digest GitHub publishes for the
//! asset. That digest is trusted only when the release metadata came straight
//! from GitHub; the download itself may go through a mirror, since the digest
//! settles what was received. Metadata from a mirror, a release without a
//! digest, or no package for this platform: nothing is installed, and the
//! caller sends the user to the release page instead.

use super::check::{is_newer, parse_sha256_digest};
use crate::config::NetworkConfig;
use crate::github::mirror::{self, Source};
use crate::github::{build_client, download_with_fallback, fetch_latest_release_mirrored, Asset};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

/// How this copy of Mokora was installed, which decides the package it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Package {
    /// The Windows installer.
    WindowsSetup,
    /// An AppImage, at this path.
    AppImage(PathBuf),
    /// The Debian package.
    Deb,
}

impl Package {
    /// The package for the running copy, if there is one to take.
    pub fn current() -> Option<Self> {
        if cfg!(windows) {
            return Some(Self::WindowsSetup);
        }
        if cfg!(target_os = "linux") {
            if let Some(path) = std::env::var_os("APPIMAGE").filter(|p| !p.is_empty()) {
                return Some(Self::AppImage(PathBuf::from(path)));
            }
            return Some(Self::Deb);
        }
        None
    }

    /// Whether `name` is this package's release asset.
    fn matches(&self, name: &str) -> bool {
        match self {
            Self::WindowsSetup => name.ends_with("_x64-setup.exe"),
            Self::AppImage(_) => name.ends_with("_amd64.AppImage"),
            Self::Deb => name.ends_with("_amd64.deb"),
        }
    }
}

/// The asset of `assets` that is `package`, with its trusted digest.
pub fn pick_asset<'a>(assets: &'a [Asset], package: &Package) -> Result<(&'a Asset, String)> {
    let asset = assets
        .iter()
        .find(|a| package.matches(&a.name))
        .context("the release has no package for this system")?;
    let digest = asset
        .digest
        .as_deref()
        .and_then(parse_sha256_digest)
        .context("the release does not publish a checksum for its package")?;
    Ok((asset, digest))
}

/// Download the latest release's package and start installing it. Returns
/// once the installer is running (Windows, .deb) or the new AppImage is in
/// place; the caller then closes Mokora.
pub async fn download_and_install(repo: &str, net: &NetworkConfig) -> Result<()> {
    let package = Package::current().context("updating is not supported on this system")?;
    let client = build_client()?;
    let (release, source) = fetch_latest_release_mirrored(&client, repo, net).await?;
    if source != Source::Direct {
        bail!("the release information came through a mirror, so its checksum cannot be trusted");
    }
    let tag = release
        .tag_name
        .clone()
        .context("the release has no version")?;
    if !is_newer(env!("CARGO_PKG_VERSION"), &tag) {
        bail!("Mokora is already up to date");
    }
    let (asset, digest) = pick_asset(&release.assets, &package)?;

    let dir = std::env::temp_dir().join("mokora-update");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let file = dir.join(&asset.name);
    info!("updater: downloading {} ({tag})", asset.name);
    let (_, got) = download_with_fallback(
        &client,
        &mirror::candidates(net, &asset.browser_download_url),
        &file,
    )
    .await?;
    if got != digest {
        let _ = std::fs::remove_file(&file);
        bail!("the download does not match the release's checksum");
    }

    info!("updater: installing {tag}");
    // Waiting on pkexec (the .deb) blocks; keep it off the async workers.
    tokio::task::spawn_blocking(move || install(&package, &file))
        .await
        .context("the install step stopped")?
}

fn install(package: &Package, file: &Path) -> Result<()> {
    match package {
        Package::WindowsSetup => run_windows_setup(file),
        Package::AppImage(target) => replace_appimage(file, target),
        Package::Deb => install_deb(file),
    }
}

/// The folder the running program is in (where the Windows installer goes).
#[cfg(windows)]
fn program_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("find the running program")?;
    exe.parent()
        .map(Path::to_path_buf)
        .context("the running program has no folder")
}

#[cfg(windows)]
fn run_windows_setup(setup: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let dir = program_dir()?;
    // `/D=` must be last and unquoted: NSIS takes the rest of the line.
    Command::new(setup)
        .args(["/P", "/UPDATE", "/R"])
        .raw_arg(format!("/D={}", dir.display()))
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .with_context(|| format!("start {}", setup.display()))?;
    Ok(())
}

#[cfg(not(windows))]
fn run_windows_setup(_setup: &Path) -> Result<()> {
    bail!("the Windows installer cannot run here")
}

#[cfg(unix)]
fn replace_appimage(new: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // Copied beside the target first, so the final step is a rename on one
    // file system: the running AppImage keeps its mounted old inode.
    let staged = target.with_extension("AppImage.new");
    std::fs::copy(new, &staged).with_context(|| format!("write {}", staged.display()))?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("make {} runnable", staged.display()))?;
    std::fs::rename(&staged, target).with_context(|| format!("replace {}", target.display()))?;
    let _ = std::fs::remove_file(new);
    start_after_exit(target)
}

/// Start `program` once this Mokora has ended: only one may run, so one
/// started before this one quits would just hand over to it and end.
#[cfg(unix)]
fn start_after_exit(program: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new("sh")
        .arg("-c")
        .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; exec "$1""#)
        .arg(std::process::id().to_string())
        .arg(program)
        .process_group(0)
        .spawn()
        .with_context(|| format!("start {}", program.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn replace_appimage(_new: &Path, _target: &Path) -> Result<()> {
    bail!("an AppImage cannot be replaced here")
}

#[cfg(unix)]
fn install_deb(deb: &Path) -> Result<()> {
    let status = Command::new("pkexec")
        .arg("dpkg")
        .arg("-i")
        .arg(deb)
        .status()
        .context("run pkexec (needed to install the .deb)")?;
    if !status.success() {
        bail!("installing the package did not finish ({status})");
    }
    let _ = std::fs::remove_file(deb);
    // The program file was replaced; its old path names the new one.
    let exe = std::env::current_exe().context("find the running program")?;
    let exe = PathBuf::from(exe.to_string_lossy().trim_end_matches(" (deleted)"));
    start_after_exit(&exe)
}

#[cfg(not(unix))]
fn install_deb(_deb: &Path) -> Result<()> {
    bail!("a .deb cannot be installed here")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, digest: Option<&str>) -> Asset {
        Asset {
            name: name.into(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: None,
            digest: digest.map(str::to_owned),
        }
    }

    const SHA: &str = "sha256:6b566ded8af2440f5aac4ef614a6aa66951586fca30931e9c9339bc3b8140c23";

    fn release_assets() -> Vec<Asset> {
        vec![
            asset("Mokora_0.1.2_amd64.AppImage", Some(SHA)),
            asset("Mokora_0.1.2_amd64.deb", Some(SHA)),
            asset("Mokora_0.1.2_x64-setup.exe", Some(SHA)),
            asset("SHA256SUMS-0.1.2.txt", Some(SHA)),
        ]
    }

    /// Each kind of install takes its own package.
    #[test]
    fn picks_the_package_of_the_install() {
        let assets = release_assets();
        let name = |p: Package| pick_asset(&assets, &p).unwrap().0.name.clone();
        assert_eq!(name(Package::WindowsSetup), "Mokora_0.1.2_x64-setup.exe");
        assert_eq!(name(Package::Deb), "Mokora_0.1.2_amd64.deb");
        assert_eq!(
            name(Package::AppImage(PathBuf::from("/a/Mokora.AppImage"))),
            "Mokora_0.1.2_amd64.AppImage"
        );
    }

    /// The digest comes back bare, and a package without one is refused.
    #[test]
    fn needs_the_published_checksum() {
        let (_, digest) = pick_asset(&release_assets(), &Package::WindowsSetup).unwrap();
        assert_eq!(digest, SHA.trim_start_matches("sha256:"));
        let unsigned = vec![asset("Mokora_0.1.2_x64-setup.exe", None)];
        assert!(pick_asset(&unsigned, &Package::WindowsSetup).is_err());
        assert!(pick_asset(&[], &Package::Deb).is_err());
    }
}
