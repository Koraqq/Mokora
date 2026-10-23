//! The accounts open in this Mokora, and which one the window shows.
//!
//! One process, one window, any number of Mahjong Soul accounts. An open
//! account is an [`Account`]: its own [`AppState`] (tracker, bot, autoplay,
//! capture…) and its own web views (see [`views`]) whose storage keeps its
//! login. All of them play at once; the window shows one. Switching is a matter
//! of which account's interface is visible — nothing restarts, nothing is
//! launched.
//!
//! ```text
//!            Hub ── open accounts ── Account "main" ── AppState, ui-main, game-main
//!             │                  └── Account "2"    ── AppState, ui-2,    game-2
//!             └── the shown account's id
//! ```
//!
//! A command knows its account from the web view that called it ([`Acct`]); the
//! events an account emits go to its own interface (see `ipc::Route`).

mod account;
mod arg;
mod background;
#[cfg(target_os = "linux")]
mod gtk_layout;
pub mod views;

pub use arg::Acct;

use crate::accounts::{self, AccountInfo};
use crate::config::{self, CaptureMode};
use crate::ipc::{AppState, Route, Shared};
use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};
use views::Bounds;

/// How long a closing account waits for its capture to stop.
const CAPTURE_STOP_WAIT: Duration = Duration::from_secs(3);

/// Where the interface last put an account's game, and whether it wants it
/// shown (it does not on the settings pages).
#[derive(Default)]
struct GameSlot {
    bounds: Option<Bounds>,
    wanted: bool,
}

/// One open account.
pub struct Account {
    pub id: String,
    pub state: AppState,
    game: Mutex<GameSlot>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Account {
    /// Its game is connected and logged in: capture is running, the game's page
    /// is bound, and this session has seen who the player is. (Until then the
    /// game is on its login screen.)
    pub async fn online(&self) -> bool {
        let running = matches!(
            self.state.capture_control.lock().await.status,
            crate::schema::CaptureStatus::Running { .. }
        );
        running
            && self.state.autoplay_context.page.read().await.is_some()
            && self.state.autoplay_context.lobby.account().is_some()
    }
}

struct Inner {
    app: AppHandle,
    shared: Shared,
    open: Mutex<Vec<Arc<Account>>>,
    /// The id of the account the window shows; shared with the event routes.
    active: Arc<RwLock<String>>,
    /// The first account chip in the shared three-chip viewport.
    account_tabs_first: AtomicUsize,
    quitting: AtomicBool,
}

/// The accounts, managed by Tauri.
#[derive(Clone)]
pub struct Hub(Arc<Inner>);

impl Hub {
    pub fn new(app: AppHandle, shared: Shared) -> Self {
        Self(Arc::new(Inner {
            app,
            shared,
            open: Mutex::new(Vec::new()),
            active: Arc::new(RwLock::new(String::new())),
            account_tabs_first: AtomicUsize::new(0),
            quitting: AtomicBool::new(false),
        }))
    }

    pub fn app(&self) -> &AppHandle {
        &self.0.app
    }

    pub fn shared(&self) -> &Shared {
        &self.0.shared
    }

    pub(crate) fn route(&self, account: &Account) -> Route {
        Route::new(
            &views::ui_label(&account.id),
            &account.id,
            self.0.active.clone(),
        )
    }

    pub fn active_id(&self) -> String {
        self.0.active.read().map(|a| a.clone()).unwrap_or_default()
    }

    pub fn account_tabs_first(&self) -> usize {
        self.0.account_tabs_first.load(Ordering::Relaxed)
    }

    pub fn set_account_tabs_first(&self, first: usize) {
        self.0.account_tabs_first.store(first, Ordering::Relaxed);
    }

    fn open_list(&self) -> Vec<Arc<Account>> {
        self.0.open.lock().map(|o| o.clone()).unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Account>> {
        self.open_list().into_iter().find(|a| a.id == id)
    }

    /// The account a web view belongs to, by its label.
    pub fn for_label(&self, label: &str) -> Option<Arc<Account>> {
        self.get(views::account_of_label(label)?)
    }

    /// The account the window shows.
    pub fn active(&self) -> Option<Arc<Account>> {
        self.get(&self.active_id())
    }

    /// Run `f` on the main thread and wait for it. Web views can only be made
    /// there; from a Tokio worker the call would deadlock the event loop.
    async fn on_main<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.0
            .app
            .run_on_main_thread(move || {
                let _ = tx.send(f());
            })
            .context("asking the main thread to run")?;
        rx.await.context("the main thread dropped the request")
    }

    /// Make account `id`'s state and register it, ahead of its views (which
    /// call commands the moment they load).
    fn prepare(&self, id: &str) -> Result<(Arc<Account>, views::ViewSpec)> {
        // Every way of opening an account comes through here.
        if !accounts::may_open(id) {
            bail!(crate::license::PRO_REQUIRED);
        }
        let path = accounts::config_path_of(id).with_context(|| format!("no account {id}"))?;
        accounts::prepare(&path)?;
        let (cfg, path) = if id == accounts::MAIN_ID {
            config::load_config(None)
        } else {
            config::load_config(Some(&path))
        };
        let port = free_port().context("finding a port for the game's DevTools")?;
        let game_url = (cfg.capture.mode == CaptureMode::Embedded)
            .then(|| cfg.capture.chromium.start_url.clone());
        let data_dir = accounts::webview_dir(&path);
        let state = AppState::new(cfg, path, id, port, &self.0.shared);
        let spec = views::ViewSpec {
            id: id.to_string(),
            data_dir,
            port,
            game_url,
            language: accounts::ui_language(),
            account_tabs_first: self.account_tabs_first(),
            #[cfg(target_os = "linux")]
            webkit_tap: state.webkit_tap.clone(),
        };
        let account = Arc::new(Account {
            id: id.to_string(),
            state,
            game: Mutex::default(),
            tasks: Mutex::default(),
        });
        if let Ok(mut open) = self.0.open.lock() {
            open.push(account.clone());
        }
        Ok((account, spec))
    }

    /// Open account `id` from the main thread (start-up). Returns the open one
    /// if it already was.
    pub fn open_now(&self, id: &str) -> Result<Arc<Account>> {
        if let Some(account) = self.get(id) {
            return Ok(account);
        }
        let (account, spec) = self.prepare(id)?;
        if let Err(e) = views::create(&self.0.app, &spec) {
            self.forget(id);
            return Err(e).context("creating the account's views");
        }
        self.start(&account);
        Ok(account)
    }

    /// Open account `id`: its runtime starts and its views are made.
    pub async fn open(&self, id: &str) -> Result<Arc<Account>> {
        if let Some(account) = self.get(id) {
            return Ok(account);
        }
        let (account, spec) = self.prepare(id)?;
        let app = self.0.app.clone();
        let made = self.on_main(move || views::create(&app, &spec)).await;
        if let Err(e) = made.and_then(|r| r.context("creating the account's views")) {
            self.forget(id);
            return Err(e);
        }
        self.start(&account);
        Ok(account)
    }

    fn forget(&self, id: &str) {
        if let Ok(mut open) = self.0.open.lock() {
            open.retain(|a| a.id != id);
        }
    }

    /// Start the runtime (needs the Tokio runtime, which Tauri's is).
    fn start(&self, account: &Arc<Account>) {
        let hub = self.clone();
        let account = account.clone();
        tauri::async_runtime::spawn(async move {
            let tasks = account::start(&hub, &account).await;
            if let Ok(mut t) = account.tasks.lock() {
                t.extend(tasks);
            }
            info!("account {} is open", account.id);
            let _ = hub.0.app.emit("accounts-changed", ());
        });
    }

    /// Show account `id` (open it first if it is not).
    pub async fn activate(&self, id: &str) -> Result<()> {
        self.open(id).await?;
        self.show_now(id);
        Ok(())
    }

    /// Show the open account `id`.
    pub fn show_now(&self, id: &str) {
        if let Ok(mut active) = self.0.active.write() {
            *active = id.to_string();
        }
        self.apply_visibility();
        self.update_title();
        let _ = self.0.app.emit("accounts-changed", ());
    }

    /// Make the views match the shown account: its interface visible (and its
    /// game where the interface last put it), every other account's interface
    /// hidden and its game parked.
    fn apply_visibility(&self) {
        let active = self.active_id();
        let accounts = self.open_list();
        let target = accounts.iter().find(|account| account.id == active);

        // Paint the destination interface first so the shell never has a blank
        // frame. Then remove every old game view from the window before placing
        // the destination game. WebView2 otherwise races the old and new child
        // windows for z-order and can briefly cover the account bar.
        if let Some(account) = target {
            views::sync_account_tabs_first(&self.0.app, &account.id, self.account_tabs_first());
            views::set_ui_visible(&self.0.app, &account.id, true);
        }
        for account in accounts.iter().filter(|account| account.id != active) {
            views::set_ui_visible(&self.0.app, &account.id, false);
            views::park_game(&self.0.app, &account.id);
        }
        if let Some(account) = target {
            let slot = account.game.lock().ok();
            match slot.as_deref() {
                Some(GameSlot {
                    bounds: Some(bounds),
                    wanted: true,
                }) => views::place_game(&self.0.app, &account.id, *bounds),
                _ => views::park_game(&self.0.app, &account.id),
            }
        }
    }

    /// The interface of `id` put its game at `bounds`.
    pub fn place_game(&self, id: &str, bounds: Bounds) {
        let Some(account) = self.get(id) else { return };
        if let Ok(mut slot) = account.game.lock() {
            slot.bounds = Some(bounds);
            slot.wanted = true;
        }
        if self.active_id() == id {
            views::place_game(&self.0.app, id, bounds);
        }
    }

    /// The interface of `id` does not want its game on screen (it is on a page
    /// without it).
    pub fn hide_game(&self, id: &str) {
        let Some(account) = self.get(id) else { return };
        if let Ok(mut slot) = account.game.lock() {
            slot.wanted = false;
        }
        views::park_game(&self.0.app, id);
    }

    /// The window's title: Mokora, and the shown account's login.
    pub fn update_title(&self) {
        let nickname = self
            .active()
            .and_then(|a| accounts::read_identity(&a.state.config_path))
            .map(|i| i.nickname);
        let title = match nickname {
            Some(name) => format!("Mokora — {name}"),
            None => "Mokora".to_string(),
        };
        views::set_title(&self.0.app, &title);
    }

    /// Is another account logged in as Mahjong Soul account `account_id`?
    pub fn is_duplicate(&self, id: &str, account_id: u64) -> bool {
        accounts::known().into_iter().any(|(other, path)| {
            other != id
                && accounts::read_identity(&path).is_some_and(|i| i.account_id == account_id)
        })
    }

    /// Every account, open or not.
    pub async fn list(&self, caller: &str) -> Vec<AccountInfo> {
        let startup = accounts::startup_account();
        let mut out = Vec::new();
        for (id, config_path) in accounts::known() {
            let open = self.get(&id);
            let (running, online, in_game) = match &open {
                Some(account) => {
                    let online = account.online().await;
                    (
                        true,
                        online,
                        online && account.state.autoplay_context.lobby.in_game(),
                    )
                }
                None => (false, false, false),
            };
            out.push(AccountInfo {
                identity: accounts::read_identity(&config_path),
                running,
                online,
                browser: running,
                current: id == caller,
                startup: id == startup,
                duplicate: false,
                last_login: accounts::read_login(&config_path),
                avatar: accounts::read_avatar(&config_path),
                in_game,
                character: accounts::read_profile(&config_path).and_then(|p| p.main_character),
                // An open one stays open: only opening it again is refused.
                locked: open.is_none() && !accounts::may_open(&id),
                id,
                config_path,
            });
        }
        accounts::mark_duplicates(&mut out);
        accounts::apply_order(&mut out);
        out
    }

    /// Whether the open account `id` is in a match. Kept on the Hub so the
    /// logout command checks the live lobby state immediately before closing,
    /// rather than trusting the interface's periodically refreshed copy.
    pub fn account_in_game(&self, id: &str) -> bool {
        self.get(id)
            .is_some_and(|account| account.state.autoplay_context.lobby.in_game())
    }

    /// Close account `id`: its capture stops and its views go (the login is
    /// kept in its storage). The last open account stays.
    pub async fn close(&self, id: &str) -> Result<()> {
        let open = self.open_list();
        let Some(account) = open.iter().find(|a| a.id == id).cloned() else {
            return Ok(());
        };
        if open.len() == 1 {
            bail!("the last open account stays open");
        }
        if self.active_id() == id {
            let next = open
                .iter()
                .find(|a| a.id != id)
                .map(|a| a.id.clone())
                .context("no other account")?;
            self.activate(&next).await?;
        }
        crate::ipc::capture_supervisor::stop_and_wait(&account.state, CAPTURE_STOP_WAIT).await;
        if let Ok(mut tasks) = account.tasks.lock() {
            for task in tasks.drain(..) {
                task.abort();
            }
        }
        self.forget(id);
        let app = self.0.app.clone();
        let id = id.to_string();
        self.on_main(move || views::destroy(&app, &id)).await?;
        let _ = self.0.app.emit("accounts-changed", ());
        Ok(())
    }

    /// Stop every account's capture and end Mokora.
    pub async fn quit_all(&self) {
        if self.0.quitting.swap(true, Ordering::SeqCst) {
            return;
        }
        info!("closing Mokora");
        for account in self.open_list() {
            crate::ipc::capture_supervisor::stop_and_wait(&account.state, CAPTURE_STOP_WAIT).await;
        }
        let _ = crate::ipc::overlay::close(&self.0.app);
        // Let the log reach its file before the process goes.
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.0.app.exit(0);
    }
}

/// A free loopback port, found by asking the OS for one and letting it go (a
/// few milliseconds before WebView2 binds it).
fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// What the window's × does: end Mokora, or keep it running in the background
/// (see [`background`]). The window itself never closes on its own.
pub fn handle_close(hub: &Hub) {
    use tauri::WindowEvent;
    let Some(window) = hub.0.app.get_window(views::MAIN) else {
        warn!("hub: no main window to watch");
        return;
    };
    let for_event = hub.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let hub = for_event.clone();
            tauri::async_runtime::spawn(async move { hub.close_pressed().await });
        }
        #[cfg(windows)]
        WindowEvent::Resized(_) => {
            views::resize_ui_views(&for_event.0.app, &for_event.active_id());
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_port_can_be_bound_and_two_differ() {
        let a = free_port().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", a)).expect("the port is free");
        // While `a` is held, another request cannot get it.
        let b = free_port().unwrap();
        assert_ne!(a, b);
        drop(listener);
    }
}
