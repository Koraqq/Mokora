//! Cross-platform startup hooks. Called once from `run()` before any GUI
//! subsystem (Tauri / webkit2gtk / wry / WebView2) is initialised.

/// Apply environment workarounds required for the current OS / display server.
///
/// - **Linux + Wayland**: webkit2gtk's default dmabuf renderer regularly trips
///   `Gdk-Message: Error 71 (Protocol error)` against modern Wayland compositors
///   (Mutter / KWin / Hyprland). Force-disable it. Also disable the GL
///   compositing mode as a secondary fallback. Both are no-ops on X11.
/// - **Windows**: take Ctrl+C back. `start-Mokora.bat` starts Mokora through
///   `start /B`, which leaves it ignoring Ctrl+C — so the console's cmd never
///   asks "Terminate batch job (Y/N)?" and stops before cleaning up — and
///   Mokora alone answers Ctrl+C, closing every account (see
///   `accounts::watch_console`).
/// - **macOS**: nothing required at the moment.
pub fn setup() {
    #[cfg(target_os = "linux")]
    setup_linux();
    #[cfg(windows)]
    setup_windows();
}

#[cfg(windows)]
fn setup_windows() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(handler: *const core::ffi::c_void, add: i32) -> i32;
    }
    // A null handler with FALSE clears the inherited "ignore Ctrl+C" flag.
    // Without a console (an account the picker started) it changes nothing.
    // SAFETY: plain Win32 call; no handler pointer is passed.
    unsafe {
        SetConsoleCtrlHandler(std::ptr::null(), 0);
    }
}

/// Run the calling thread below everything else the app does, so background
/// work (the game review) yields the CPU to a live game's decisions. Windows
/// only; elsewhere the thread keeps its priority.
pub fn lower_current_thread_priority() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        // THREAD_PRIORITY_LOWEST.
        // SAFETY: plain Win32 calls on the current thread's pseudo-handle.
        unsafe {
            SetThreadPriority(GetCurrentThread(), -2);
        }
    }
}

#[cfg(target_os = "linux")]
fn setup_linux() {
    for var in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if std::env::var_os(var).is_none() {
            std::env::set_var(var, "1");
        }
    }
}
