//! Check that a browser executable can really be started.
//!
//! The setup wizard asks for this before it lets a path through: a file that
//! exists is not necessarily a browser, and a browser that exists may still
//! fail to start (missing libraries, a broken install, an antivirus block).
//! The check starts it headless against a throwaway profile, asks it to print
//! the DOM of `about:blank`, and requires a normal exit with an `<html` document
//! on stdout — everything the real launch needs short of the debugging port.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// How long a browser gets to start and print the page. A cold first start of
/// a freshly installed browser can take several seconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(25);

/// `Ok(())` when `path` starts as a Chromium-family browser.
pub async fn probe_executable(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("not a file: {}", path.display());
    }
    let profile = tempfile::Builder::new()
        .prefix("mokora-probe-")
        .tempdir()
        .context("create a temporary profile")?;

    let mut cmd = Command::new(path);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg(format!("--user-data-dir={}", profile.path().display()))
        .arg("--dump-dom")
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut child = cmd
        .spawn()
        .with_context(|| format!("start {}", path.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .context("capture the browser's output")?;
    let mut stderr = child
        .stderr
        .take()
        .context("capture the browser's errors")?;

    let run = async {
        // Both pipes at once: a browser that fills one while we wait on the
        // other would stall.
        let (mut out, mut err) = (Vec::new(), Vec::new());
        tokio::try_join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err))?;
        let status = child.wait().await?;
        anyhow::Ok((status, out, err))
    };
    let (status, out, err) = match tokio::time::timeout(PROBE_TIMEOUT, run).await {
        Ok(r) => r?,
        Err(_) => bail!(
            "the browser did not answer within {}s",
            PROBE_TIMEOUT.as_secs()
        ),
    };
    if !status.success() {
        if let Some(why) = known_failure(&String::from_utf8_lossy(&err)) {
            bail!("{why}");
        }
        bail!("the browser exited with {status}");
    }
    if !String::from_utf8_lossy(&out).contains("<html") {
        bail!("this does not look like a Chromium-based browser");
    }
    Ok(())
}

/// The check's whole error when the system will not let the browser start
/// its sandbox: Ubuntu 23.10+ (AppArmor) lets only browsers installed with a
/// profile of their own create the user namespaces Chromium's sandbox needs,
/// so an unpacked one (Chrome for Testing) aborts. The interface knows this
/// code and explains it in the user's language (`setup.chromium.probe_sandbox`).
pub const SANDBOX_BLOCKED: &str = "sandbox_blocked";

/// What a browser's error output says went wrong, when it is a failure the
/// user can do something about.
fn known_failure(stderr: &str) -> Option<&'static str> {
    stderr.contains("No usable sandbox").then_some(SANDBOX_BLOCKED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chrome's own words for the AppArmor sandbox block become advice; any
    /// other error output does not.
    #[test]
    fn a_blocked_sandbox_is_explained() {
        let line = "[70467:70467:1021/064109.834979:FATAL:content/browser/zygote_host/\
                    zygote_host_impl_linux.cc:130] No usable sandbox! If you are running \
                    on Ubuntu 23.10+ ...";
        assert_eq!(known_failure(line), Some(SANDBOX_BLOCKED));
        assert_eq!(known_failure("Received signal 6"), None);
    }

    #[tokio::test]
    async fn a_missing_file_is_refused() {
        let err = probe_executable(Path::new("Z:/no/such/browser.exe"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not a file"), "{err}");
    }

    #[tokio::test]
    async fn a_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(probe_executable(dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_program_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("notes.txt");
        std::fs::write(&f, "hello").unwrap();
        assert!(probe_executable(&f).await.is_err());
    }
}
