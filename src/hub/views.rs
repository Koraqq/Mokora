//! The web views of the one window.
//!
//! The window holds, for every open account, two web views side by side in
//! the same window (children of it): the **interface** (`ui-<id>`, the React
//! app, filling the window) and the **game** (`game-<id>`, laid over the
//! interface wherever the interface keeps a placeholder for it). Only the
//! shown account's interface is on screen. On Windows, the other interfaces
//! stay rendered but are *parked* off the window's edge: WebView2 explicitly
//! does not render a controller whose `IsVisible` is false, which leaves its
//! last composed frame stale when shared shell state changes. Their games are
//! parked for the same reason: a hidden game would stop its loop and stall a
//! match. Other platforms keep hiding inactive interfaces for now.
//!
//! Every web view of an account shares that account's storage (its login) and
//! its DevTools port, so the account's capture reads the game through its own
//! port and no other account's.

use std::path::PathBuf;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, Window};

/// The one window.
pub const MAIN: &str = "main";

/// Where a parked game waits: far outside the window, still "visible" to the
/// browser.
const PARKED_AT: f64 = -30_000.0;

/// Label of account `id`'s interface.
pub fn ui_label(id: &str) -> String {
    format!("ui-{id}")
}

/// Label of account `id`'s game.
pub fn game_label(id: &str) -> String {
    format!("game-{id}")
}

/// The account a web view label belongs to (`ui-3` → `3`).
pub fn account_of_label(label: &str) -> Option<&str> {
    label
        .strip_prefix("ui-")
        .or_else(|| label.strip_prefix("game-"))
}

/// WebView2's browser arguments for one account: Tauri's own defaults, and the
/// account's DevTools port on the loopback address. Given to every web view of
/// the account (views that share storage must share their arguments).
///
/// Chromium's background throttling is off too: a game must not slow down
/// because the window is minimised, hidden or covered (see `hub::background`,
/// which rests the games on purpose when nothing is being played).
fn browser_args(port: u16) -> String {
    format!(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion \
         --disable-background-timer-throttling --disable-renderer-backgrounding \
         --disable-backgrounding-occluded-windows \
         --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
    )
}

/// Tell every game page whether it is visible to the browser. A page that is
/// not visible stops its render loop and slows its timers — the power saving
/// of `hub::background`; visible again, it carries on.
pub fn set_games_visible(app: &AppHandle, visible: bool) {
    for (label, view) in app.webviews() {
        if !label.starts_with("game-") {
            continue;
        }
        #[cfg(windows)]
        {
            let _ = view.with_webview(move |w| {
                // SAFETY: the controller is used on the main thread, inside
                // `with_webview`, while the web view is alive.
                unsafe {
                    let _ = w.controller().SetIsVisible(visible);
                }
            });
        }
        // WebKitGTK keeps no such switch: the page rests itself on a flag of
        // Mokora's hook (`capture::webkit`), its animation frames slowed to
        // one a second.
        #[cfg(target_os = "linux")]
        let _ = view.eval(format!("window.__mokoraRest = {}", !visible));
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = (view, visible);
    }
}

/// What an account's views are made from.
pub struct ViewSpec {
    pub id: String,
    /// The account's storage directory; `None` for the first account, which
    /// keeps the app's default (so a login made before accounts is kept).
    pub data_dir: Option<PathBuf>,
    pub port: u16,
    /// The game's address, when the game is shown in this window.
    pub game_url: Option<String>,
    /// The language a still-empty interface starts in (see
    /// `accounts::ui_language`).
    pub language: Option<String>,
    /// The shared first chip in the account strip, available before React's
    /// first paint so a newly opened account does not jump from zero.
    pub account_tabs_first: usize,
    /// Where the game view's capture is read from on Linux (see
    /// `capture::webkit`).
    #[cfg(target_os = "linux")]
    pub webkit_tap: std::sync::Arc<crate::capture::webkit::WebKitTap>,
}

/// Make the window from `tauri.conf.json`. No web view yet: they are added per
/// account by [`create`].
pub fn create_main_window(app: &AppHandle) -> tauri::Result<Window> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let window = tauri::window::WindowBuilder::from_config(app, &config)?.build()?;
    // Before any web view joins it (see the function).
    #[cfg(target_os = "linux")]
    super::gtk_layout::keep_games_running_when_minimized(&window);
    Ok(window)
}

fn main_window(app: &AppHandle) -> tauri::Result<Window> {
    app.get_window(MAIN).ok_or(tauri::Error::WindowNotFound)
}

/// Add an account's interface and game to the window. Must run on the main
/// thread (see `Hub::on_main`).
pub fn create(app: &AppHandle, spec: &ViewSpec) -> tauri::Result<()> {
    let window = main_window(app)?;
    let size: LogicalSize<f64> = window.inner_size()?.to_logical(window.scale_factor()?);
    let args = browser_args(spec.port);

    // The interface comes first: views made later paint above it.
    let mut ui = WebviewBuilder::new(ui_label(&spec.id), WebviewUrl::App("index.html".into()))
        .additional_browser_args(&args)
        .initialization_script(format!(
            "window.__MOKORA_ACCOUNT_TABS_FIRST__ = {};",
            spec.account_tabs_first
        ));
    // A parked Windows interface must keep its off-screen bounds across a
    // resize, so `resize_ui_views` owns those bounds. Other platforms retain
    // Tauri's automatic full-window sizing and their existing hide/show path.
    #[cfg(not(windows))]
    {
        ui = ui.auto_resize();
    }
    if let Some(lang) = &spec.language {
        // The interface keeps its language in this key (`i18n/index.ts`); only
        // fill it in when the account has not chosen one yet.
        let lang = serde_json::to_string(lang).unwrap_or_default();
        ui = ui.initialization_script(format!(
            "try {{ if (!localStorage.getItem('mokora.lang')) localStorage.setItem('mokora.lang', {lang}) }} catch (e) {{}}"
        ));
    }
    if let Some(dir) = &spec.data_dir {
        ui = ui.data_directory(dir.clone());
    }
    let ui = window.add_child(ui, LogicalPosition::new(0.0, 0.0), size)?;
    #[cfg(target_os = "linux")]
    super::gtk_layout::adopt(
        &window,
        &ui,
        true,
        Bounds {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        },
    );
    // Not on show until this account is the shown one.
    ui.hide()?;

    if let Some(url) = &spec.game_url {
        let url: tauri::Url = url.parse().map_err(tauri::Error::InvalidUrl)?;
        let mut game = WebviewBuilder::new(game_label(&spec.id), WebviewUrl::External(url))
            .additional_browser_args(&args);
        // WebKitGTK has no DevTools port: the game is read by a script of
        // Mokora's in the page instead (see `capture::webkit`).
        #[cfg(target_os = "linux")]
        {
            game = game.initialization_script(crate::capture::webkit::HOOK);
        }
        if let Some(dir) = &spec.data_dir {
            game = game.data_directory(dir.clone());
        }
        let game = window.add_child(
            game,
            LogicalPosition::new(PARKED_AT, 0.0),
            LogicalSize::new(800.0, 600.0),
        )?;
        #[cfg(target_os = "linux")]
        {
            super::gtk_layout::adopt(
                &window,
                &game,
                false,
                Bounds {
                    x: PARKED_AT,
                    y: 0.0,
                    width: 800.0,
                    height: 600.0,
                },
            );
            crate::capture::webkit::attach(&game, &spec.webkit_tap)?;
        }
        game.show()?;
    }
    Ok(())
}

/// Take an account's views out of the window for good.
pub fn destroy(app: &AppHandle, id: &str) {
    for label in [game_label(id), ui_label(id)] {
        if let Some(view) = app.get_webview(&label) {
            let _ = view.close();
        }
    }
}

/// Show or hide account `id`'s interface.
pub fn set_ui_visible(app: &AppHandle, id: &str, visible: bool) {
    if let Some(view) = app.get_webview(&ui_label(id)) {
        #[cfg(windows)]
        {
            if let Ok(window) = main_window(app) {
                if let Ok(physical) = window.inner_size() {
                    let size: LogicalSize<f64> =
                        physical.to_logical(window.scale_factor().unwrap_or(1.0));
                    let x = if visible { 0.0 } else { PARKED_AT };
                    let _ = view.set_bounds(tauri::Rect {
                        position: LogicalPosition::new(x, 0.0).into(),
                        size: LogicalSize::new(size.width.max(1.0), size.height.max(1.0)).into(),
                    });
                    // Newly created views begin hidden. Once shown, inactive
                    // views remain visible off-screen so WebView2 keeps their
                    // compositor surfaces current.
                    let _ = view.show();
                }
            }
        }
        #[cfg(not(windows))]
        let _ = if visible { view.show() } else { view.hide() };
    }
    // Showing a view raises it on Linux; the games stay above the interfaces.
    #[cfg(target_os = "linux")]
    if visible {
        for (label, view) in app.webviews() {
            if label.starts_with("game-") {
                super::gtk_layout::raise(&view);
            }
        }
    }
}

/// Keep every Windows interface fitted to the window after a resize while
/// preserving the off-screen parking of inactive accounts.
#[cfg(windows)]
pub fn resize_ui_views(app: &AppHandle, active: &str) {
    let Ok(window) = main_window(app) else { return };
    let Ok(physical) = window.inner_size() else {
        return;
    };
    let size: LogicalSize<f64> = physical.to_logical(window.scale_factor().unwrap_or(1.0));
    for (label, view) in app.webviews() {
        let Some(id) = label.strip_prefix("ui-") else {
            continue;
        };
        let x = if id == active { 0.0 } else { PARKED_AT };
        let _ = view.set_bounds(tauri::Rect {
            position: LogicalPosition::new(x, 0.0).into(),
            size: LogicalSize::new(size.width.max(1.0), size.height.max(1.0)).into(),
        });
    }
}

/// Give another account interface the shared account-strip position before it
/// is brought on screen. The synchronous DOM event lets React finish this tiny
/// update while the view is still parked or hidden.
pub fn sync_account_tabs_first(app: &AppHandle, id: &str, first: usize) {
    if let Some(view) = app.get_webview(&ui_label(id)) {
        let _ = view.eval(format!(
            "window.dispatchEvent(new CustomEvent('mokora-account-tabs-sync', {{ detail: {first} }}));"
        ));
    }
}

/// Put account `id`'s game at `rect` (logical pixels from the window's
/// top-left) and show it.
pub fn place_game(app: &AppHandle, id: &str, rect: Bounds) {
    if let Some(view) = app.get_webview(&game_label(id)) {
        #[cfg(target_os = "linux")]
        super::gtk_layout::place(&view, rect);
        #[cfg(not(target_os = "linux"))]
        let _ = view.set_bounds(tauri::Rect {
            position: LogicalPosition::new(rect.x, rect.y).into(),
            size: LogicalSize::new(rect.width.max(1.0), rect.height.max(1.0)).into(),
        });
        let _ = view.show();
    }
}

/// Take account `id`'s game off the screen without stopping it.
pub fn park_game(app: &AppHandle, id: &str) {
    if let Some(view) = app.get_webview(&game_label(id)) {
        #[cfg(target_os = "linux")]
        super::gtk_layout::move_to(&view, PARKED_AT, 0.0);
        #[cfg(not(target_os = "linux"))]
        let _ = view.set_position(LogicalPosition::new(PARKED_AT, 0.0));
        let _ = view.show();
    }
}

/// Where the game goes in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The window's title.
pub fn set_title(app: &AppHandle, title: &str) {
    if let Ok(window) = main_window(app) {
        let _ = window.set_title(title);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_names_its_account() {
        assert_eq!(account_of_label(&ui_label("main")), Some("main"));
        assert_eq!(account_of_label(&game_label("12")), Some("12"));
        assert_eq!(account_of_label("overlay"), None);
        assert_eq!(account_of_label("main"), None);
    }

    #[test]
    fn views_of_an_account_share_one_set_of_browser_arguments_with_its_own_port() {
        let a = browser_args(40001);
        assert!(a.contains("--remote-debugging-port=40001"));
        assert!(a.contains("--remote-debugging-address=127.0.0.1"));
        // Another account's port never leaks into it.
        assert!(!a.contains("40002"));
        // Tauri's own default argument is kept.
        assert!(a.contains("--disable-features="));
        // Nothing slows a game that is out of sight.
        assert!(a.contains("--disable-renderer-backgrounding"));
        assert!(a.contains("--disable-background-timer-throttling"));
        assert!(a.contains("CalculateNativeWinOcclusion"));
    }
}
