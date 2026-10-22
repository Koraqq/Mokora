use std::path::{Path, PathBuf};

/// True when running inside an AppImage runtime (read-only squashfs mount).
/// AppImage sets `$APPIMAGE` to the .AppImage path before exec.
pub fn is_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some()
}

/// Whether Mokora keeps its data under [`user_config_root`] rather than next
/// to its executable: inside an AppImage (a read-only mount), or wherever the
/// executable's folder cannot be written — a system install such as the
/// .deb's `/usr/bin`. A folder Mokora was unpacked or installed into for the
/// current user (Windows' `dist\Mokora`, the NSIS per-user install) keeps
/// its data beside the program, as it always has.
pub fn data_in_user_dir() -> bool {
    static IN_USER_DIR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *IN_USER_DIR.get_or_init(|| {
        is_appimage()
            || !std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(dir_writable))
                .unwrap_or(false)
    })
}

/// Whether a file can be created in `dir`, tried for real: permission bits
/// alone miss read-only mounts, ACLs and the like.
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".mokora-write-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// `~/.config/mokora` (Linux) or platform equivalent. None if unresolvable.
/// Where every runtime path (config, logs, history, bot dir, CA dir) anchors
/// when [`data_in_user_dir`].
pub fn user_config_root() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("mokora"))
}

/// Write `body` to `path` through a sibling temp file and a rename, so a crash
/// or a full disk mid-write leaves the old file in place instead of a cut-off
/// one.
pub fn write_atomic(path: &Path, body: impl AsRef<[u8]>) -> std::io::Result<()> {
    let tmp = tmp_sibling(path);
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// [`write_atomic`] for a secret: the file is restricted to the current
/// user (see [`restrict_to_owner`]) before it is renamed into place, so the
/// final path never exists with looser permissions. A rename on the same
/// volume keeps the restriction.
pub fn write_private(path: &Path, body: impl AsRef<[u8]>) -> std::io::Result<()> {
    let tmp = tmp_sibling(path);
    let _ = std::fs::remove_file(&tmp);
    let result = write_new_private(&tmp, body.as_ref())
        .and_then(|()| restrict_to_owner(&tmp))
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn tmp_sibling(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

#[cfg(unix)]
fn write_new_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?
        .write_all(body)
}

#[cfg(not(unix))]
fn write_new_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, body)
}

/// Let only the current user open `path`: mode `0600` on Unix; on Windows a
/// protected DACL with a single full-access entry for the user running
/// Mokora, so nothing is inherited from the folder.
#[cfg(unix)]
pub fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
pub fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::ffi::c_void;
    use std::io::Error;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(handle: isize) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn GetTokenInformation(
            token: isize,
            class: i32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
        fn GetLengthSid(sid: *mut c_void) -> u32;
        fn InitializeAcl(acl: *mut c_void, len: u32, revision: u32) -> i32;
        fn AddAccessAllowedAce(acl: *mut c_void, revision: u32, mask: u32, sid: *mut c_void)
            -> i32;
        fn SetNamedSecurityInfoW(
            name: *const u16,
            object_type: i32,
            info: u32,
            owner: *mut c_void,
            group: *mut c_void,
            dacl: *const c_void,
            sacl: *const c_void,
        ) -> u32;
    }
    const TOKEN_QUERY: u32 = 0x8;
    const TOKEN_USER: i32 = 1;
    const ACL_REVISION: u32 = 2;
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    const SE_FILE_OBJECT: i32 = 1;
    const DACL_SECURITY_INFORMATION: u32 = 0x4;
    const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;
    // sizeof(ACL) and sizeof(ACCESS_ALLOWED_ACE) minus its `SidStart` DWORD.
    const ACL_HEADER: u32 = 8;
    const ACE_WITHOUT_SID: u32 = 8;

    // SAFETY: plain Win32 calls. The token buffer is u64-backed so the
    // TOKEN_USER it holds (a pointer first) is aligned, and the SID it
    // points into lives in that buffer, which outlives every use. The ACL
    // buffer is u32-backed (DWORD alignment) and sized for exactly one ACE.
    unsafe {
        let mut token = 0isize;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(Error::last_os_error());
        }
        let mut len = 0u32;
        GetTokenInformation(token, TOKEN_USER, null_mut(), 0, &mut len);
        let mut user = vec![0u64; (len as usize).div_ceil(8)];
        let ok = GetTokenInformation(token, TOKEN_USER, user.as_mut_ptr().cast(), len, &mut len);
        let err = Error::last_os_error();
        CloseHandle(token);
        if ok == 0 {
            return Err(err);
        }
        // TOKEN_USER { User: SID_AND_ATTRIBUTES { Sid, Attributes } }.
        let sid = *user.as_ptr().cast::<*mut c_void>();

        let acl_len = (ACL_HEADER + ACE_WITHOUT_SID + GetLengthSid(sid)).next_multiple_of(4);
        let mut acl = vec![0u32; acl_len as usize / 4];
        if InitializeAcl(acl.as_mut_ptr().cast(), acl_len, ACL_REVISION) == 0
            || AddAccessAllowedAce(acl.as_mut_ptr().cast(), ACL_REVISION, FILE_ALL_ACCESS, sid) == 0
        {
            return Err(Error::last_os_error());
        }

        // `canonicalize` gives the `\\?\` form, which lifts MAX_PATH for this
        // call the way `std::fs` already does for its own.
        let full = std::fs::canonicalize(path)?;
        let wide: Vec<u16> = full
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let rc = SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl.as_ptr().cast(),
            null(),
        );
        if rc != 0 {
            return Err(Error::from_raw_os_error(rc as i32));
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Strip a leading `./` so `./logs` joins cleanly under a user dir.
pub fn strip_leading_dot(p: &Path) -> &Path {
    p.strip_prefix("./").unwrap_or(p)
}

/// `<user_config_root>/<name>` if available. Use this for runtime data
/// that must always live in a writable XDG-style location regardless of
/// AppImage / system-install / cwd. Unlike [`resolve_dir`], there is no
/// exe-dir-first fallback — callers like the Chromium profile and CfT
/// install dir explicitly want the user dir every time.
pub fn user_subdir(name: &str) -> Option<PathBuf> {
    user_config_root().map(|r| r.join(name))
}

/// Resolve a configured directory path with the standard fallback chain:
///
/// 1. Absolute path → used as-is
/// 2. `<exe_dir>/<configured>` if it exists
/// 3. When the data lives in the user dir ([`data_in_user_dir`]):
///    `<user_data_root>/<configured>` (writable XDG location). The working
///    directory is not looked at — an installed app is started from anywhere,
///    and a `logs/` that happens to be there is not Mokora's.
/// 4. Otherwise `<cwd>/<configured>` if it exists, else `<exe_dir>/<configured>`
///    (falling back to `<cwd>/<configured>` if the executable path is
///    unavailable). The caller is responsible for creating the directory.
pub fn resolve_dir(configured: &Path) -> PathBuf {
    resolve_dir_inner(configured, data_in_user_dir(), user_config_root())
}

fn resolve_dir_inner(configured: &Path, in_user_dir: bool, user_root: Option<PathBuf>) -> PathBuf {
    if configured.is_absolute() {
        return configured.to_path_buf();
    }

    let exe_candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join(strip_leading_dot(configured))));

    if let Some(p) = &exe_candidate {
        if p.exists() {
            return p.clone();
        }
    }

    if in_user_dir {
        if let Some(root) = user_root {
            return root.join(strip_leading_dot(configured));
        }
    }

    let cwd_candidate = configured.to_path_buf();
    if cwd_candidate.exists() {
        return cwd_candidate;
    }

    exe_candidate.unwrap_or(cwd_candidate)
}

/// Extension for [`std::process::Command`] that stops Windows from briefly
/// flashing a console window when the GUI app spawns a console helper
/// (`tasklist`, `taskkill`, `reg`, …). Chainable and a **no-op off Windows**:
///
/// ```ignore
/// Command::new("tasklist").args(["/FI", filter]).no_console_window().output()
/// ```
pub trait NoConsoleWindow {
    fn no_console_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for std::process::Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the child gets no console, so no window flashes.
            self.creation_flags(0x0800_0000);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The secret lands whole, replaces an older one, and leaves no temp
    /// file behind.
    #[test]
    fn write_private_replaces_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("secret.key");
        write_private(&p, "old").unwrap();
        write_private(&p, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        assert!(!tmp_sibling(&p).exists());
    }

    /// Only the current user may open the file: exactly one access entry,
    /// nothing inherited from the folder (no `(I)` flag).
    #[cfg(windows)]
    #[test]
    fn write_private_leaves_a_single_owner_entry() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("secret.key");
        write_private(&p, "x").unwrap();
        let out = std::process::Command::new("icacls").arg(&p).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let entries: Vec<&str> = text.lines().filter(|l| l.contains(":(")).collect();
        assert_eq!(entries.len(), 1, "icacls output:\n{text}");
        assert!(entries[0].contains("(F)"), "icacls output:\n{text}");
        assert!(!entries[0].contains("(I)"), "icacls output:\n{text}");
    }

    /// Past MAX_PATH (260) the plain Win32 path form fails; `std::fs` copes,
    /// so the permission step must too.
    #[cfg(windows)]
    #[test]
    fn write_private_handles_long_paths() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("d".repeat(200)).join("e".repeat(60));
        std::fs::create_dir_all(&deep).unwrap();
        let p = deep.join("secret.key");
        assert!(p.as_os_str().len() > 260);
        write_private(&p, "x").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "x");
    }

    #[cfg(unix)]
    #[test]
    fn write_private_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("secret.key");
        write_private(&p, "x").unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn strip_leading_dot_removes_dotslash() {
        assert_eq!(strip_leading_dot(Path::new("./logs")), Path::new("logs"));
        assert_eq!(strip_leading_dot(Path::new("logs")), Path::new("logs"));
        assert_eq!(strip_leading_dot(Path::new("./a/b")), Path::new("a/b"));
    }

    #[test]
    fn appimage_routes_relative_path_to_user_config() {
        let user_root = PathBuf::from("/home/u/.config/mokora");
        let resolved = resolve_dir_inner(Path::new("./logs"), true, Some(user_root.clone()));
        assert_eq!(resolved, user_root.join("logs"));
    }

    /// The probe creates (and removes) a real file: a writable folder says
    /// so and is left as it was; a missing one is not writable.
    #[test]
    fn dir_writable_tries_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(dir_writable(dir.path()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "probe left behind");
        assert!(!dir_writable(&dir.path().join("missing")));
    }

    #[test]
    fn appimage_preserves_absolute_path() {
        let resolved = resolve_dir_inner(
            Path::new("/var/log/mokora"),
            true,
            Some(PathBuf::from("/home/u/.config/mokora")),
        );
        assert_eq!(resolved, PathBuf::from("/var/log/mokora"));
    }

    #[test]
    fn non_appimage_does_not_use_user_config_for_dirs() {
        // Non-appimage with a path that doesn't exist anywhere should NOT
        // fall back to user_root; it returns the exe-relative candidate.
        let user_root = PathBuf::from("/home/u/.config/mokora");
        let resolved = resolve_dir_inner(
            Path::new("./nonexistent-xyz"),
            false,
            Some(user_root.clone()),
        );
        assert!(!resolved.starts_with(&user_root));
    }

    #[test]
    fn relative_dot_path_does_not_leak_into_absolute() {
        // Regression: `./logs` joined with absolute exe-parent used to keep
        // the literal `./` component, producing paths like
        // `/home/.../mokora/./logs/...` that surfaced in the UI as broken.
        let resolved = resolve_dir_inner(Path::new("./logs"), false, None);
        let s = resolved.to_string_lossy();
        assert!(!s.contains("/./"), "got: {s}");
        assert!(!s.contains("\\.\\"), "got: {s}");
    }
}
