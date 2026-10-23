//! Running in the background: what the window's × does, the tray icon that
//! brings the window back, and the power saving while nothing is being played.
//!
//! Two switches in `[general]` drive it (see `config::GeneralConfig`):
//!
//! - `keep_running_on_close` — × hides the window to the tray instead of ending
//!   Mokora. Ending it is the power button's job (or the tray's quit).
//! - `idle_power_saving` — with the window minimised or hidden and no game
//!   being played, the game pages are put to rest. A game in progress is never
//!   slowed: the web views are also started with Chromium's background
//!   throttling off (see `views::browser_args`), and while the window is out of
//!   sight in a game the pages are kept "visible" to the browser.
//!
//! Ending Mokora while a game is being played always asks first: the interface
//! shows the same confirmation as the power button (`quit-requested`).

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tracing::{info, warn};

use super::{views, Hub};
use crate::config::GeneralConfig;

const TRAY_ID: &str = "mokora-tray";
/// The interface is asked to show the quit confirmation.
pub const QUIT_REQUESTED: &str = "quit-requested";
/// How often the window's state is looked at.
const WATCH_EVERY: Duration = Duration::from_secs(2);

impl Hub {
    /// The app-wide switches, from the shown account's config.
    async fn general(&self) -> GeneralConfig {
        match self.active() {
            Some(account) => account.state.config.read().await.general.clone(),
            None => GeneralConfig::default(),
        }
    }

    /// A game is being played on any account. Like the account list, it counts
    /// only an account whose game is connected: after a dropped connection the
    /// lobby can still think a game is on until the next one starts.
    pub async fn any_in_game(&self) -> bool {
        for account in self.open_list() {
            if account.state.autoplay_context.lobby.in_game() && account.online().await {
                return true;
            }
        }
        false
    }

    /// Whether any open account plays in full auto (autoplay that also starts
    /// the next game by itself).
    pub async fn any_full_auto(&self) -> bool {
        if !crate::pro::available() {
            return false;
        }
        for account in self.open_list() {
            let cfg = account.state.config.read().await;
            if cfg.autoplay.enabled && cfg.autoplay.next_game.enabled {
                return true;
            }
        }
        false
    }

    /// Bring the window back from the tray or the taskbar.
    pub fn show_window(&self) {
        let hub = self.clone();
        if let Err(error) = self
            .0
            .app
            .run_on_main_thread(move || hub.show_window_on_main_thread())
        {
            warn!("hub: could not schedule the window restore: {error}");
        }
    }

    /// Window and child-WebView operations belong to the Windows UI thread.
    /// The single-instance callback can arrive elsewhere, so doing the whole
    /// restore here avoids an intermittent no-op when a hidden app is opened
    /// again from its shortcut.
    fn show_window_on_main_thread(&self) {
        if let Some(window) = self.0.app.get_window(views::MAIN) {
            if let Err(error) = window.show() {
                warn!("hub: could not show the main window: {error}");
                return;
            }
            if let Err(error) = window.unminimize() {
                warn!("hub: could not restore the main window: {error}");
            }
            self.apply_visibility();
            if let Err(error) = window.set_focus() {
                warn!("hub: could not focus the main window: {error}");
            }
        } else {
            warn!("hub: cannot restore the missing main window");
        }
    }

    /// What the window's × does.
    pub async fn close_pressed(&self) {
        if self.general().await.keep_running_on_close {
            self.hide_to_background();
        } else {
            self.request_quit().await;
        }
    }

    /// End Mokora — after asking, when a game is being played.
    pub async fn request_quit(&self) {
        if self.any_in_game().await {
            self.show_window();
            let _ = self.0.app.emit(QUIT_REQUESTED, ());
        } else {
            self.quit_all().await;
        }
    }

    fn hide_to_background(&self) {
        if let Err(e) = self.ensure_tray() {
            // Without a way back, hiding would strand the window.
            warn!("hub: no tray icon, the window stays: {e}");
            return;
        }
        if let Some(window) = self.0.app.get_window(views::MAIN) {
            // On Linux a hidden window's web views are unmapped, which stops
            // the games' pages outright; a minimised one keeps them running
            // (`gtk_layout::keep_games_running_when_minimized`), and stays in
            // the dock where a desktop shows no tray icons.
            #[cfg(target_os = "linux")]
            let _ = window.minimize();
            #[cfg(not(target_os = "linux"))]
            let _ = window.hide();
            info!("Mokora keeps running in the background");
        }
    }

    fn ensure_tray(&self) -> tauri::Result<()> {
        let app = &self.0.app;
        if app.tray_by_id(TRAY_ID).is_some() {
            return Ok(());
        }
        let show = MenuItem::with_id(app, "show", "開啟 Mokora", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "關機", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &quit])?;

        let for_menu = self.clone();
        let for_click = self.clone();
        let mut tray = TrayIconBuilder::with_id(TRAY_ID)
            .tooltip("Mokora")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(move |_app, event| match event.id().as_ref() {
                "show" => for_menu.show_window(),
                "quit" => {
                    let hub = for_menu.clone();
                    tauri::async_runtime::spawn(async move { hub.request_quit().await });
                }
                _ => {}
            })
            .on_tray_icon_event(move |_tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    for_click.show_window();
                }
            });
        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }
        tray.build(app)?;
        Ok(())
    }

    /// Look at the window every few seconds: take the tray icon away once the
    /// window is back and the switch is off, and rest or wake the games.
    pub fn watch_window(&self) {
        let hub = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut resting = false;
            loop {
                tokio::time::sleep(WATCH_EVERY).await;
                if hub.0.quitting.load(Ordering::SeqCst) {
                    break;
                }
                let Some(window) = hub.0.app.get_window(views::MAIN) else {
                    continue;
                };
                let out_of_sight = window.is_minimized().unwrap_or(false)
                    || !window.is_visible().unwrap_or(true);
                let general = hub.general().await;
                let in_game = hub.any_in_game().await;

                if !general.keep_running_on_close && !out_of_sight {
                    let _ = hub.0.app.remove_tray_by_id(TRAY_ID);
                }

                let rest = decide_rest(general.idle_power_saving, out_of_sight, in_game);
                // A game out of sight keeps being told it is visible; otherwise
                // only a change is acted on.
                if rest != resting || (out_of_sight && in_game) {
                    views::set_games_visible(&hub.0.app, !rest);
                    resting = rest;
                }
            }
        });
    }
}

/// Whether the game pages should rest: only when the switch is on, the window
/// is out of sight and nothing is being played.
fn decide_rest(power_saving: bool, out_of_sight: bool, in_game: bool) -> bool {
    power_saving && out_of_sight && !in_game
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_in_progress_is_never_put_to_rest() {
        for power_saving in [false, true] {
            for out_of_sight in [false, true] {
                assert!(!decide_rest(power_saving, out_of_sight, true));
            }
        }
    }

    #[test]
    fn rest_needs_the_switch_and_a_window_out_of_sight() {
        assert!(decide_rest(true, true, false));
        assert!(!decide_rest(false, true, false), "switch off");
        assert!(!decide_rest(true, false, false), "window in sight");
    }
}
