//! Backend ↔ frontend integration over Tauri.
//!
//! Two halves, wired per account by [`install_account`]:
//!
//! - **Outbound (backend → frontend)**: forwarder tasks subscribe to each of an
//!   account's `event_bus` channels and emit the payload to that account's
//!   interface (and to the overlay while the account is the shown one) — see
//!   [`Route`]. Notifications are the exception: they are application-level
//!   UI state and are broadcast to every account interface. Status buses also
//!   mirror the latest value into `AppState` so a fresh frontend can ask
//!   `get_status` for a snapshot.
//! - **Inbound (frontend → backend)**: `#[tauri::command]` handlers in
//!   [`commands`]; register them via the `ipc_handlers!()` macro on the
//!   `tauri::Builder`. A command takes [`crate::hub::Acct`] to get the state of
//!   the account whose interface called it.
//!
//! Event names (kebab-case, Tauri convention):
//!
//! | Event             | Payload type                  |
//! |-------------------|-------------------------------|
//! | `mjai-event`       | `schema::MjaiEvent`           |
//! | `bot-response`     | `bot::BotResponse`            |
//! | `bot-status`       | `schema::BotStatus`           |
//! | `capture-status`   | `schema::CaptureStatus`       |
//! | `notify`           | `schema::Notification`        |
//! | `analysis-result`  | `analysis::AnalysisResult`    |
//! | `history-recorded` | `schema::HistoryEvent`        |
//! | `overlay-config`   | `config::OverlayConfig`       |
//!
//! All account-scoped events go to the account's own interface, and to the
//! [`overlay`] window while that account is shown. `notify` and
//! `accounts-changed` go to every interface; `overlay-config` goes only to the
//! overlay label.

pub mod capture_supervisor;
pub mod commands;
pub mod overlay;
pub mod state;

pub use state::{AppState, Shared};

use crate::schema::Notification;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast;
use tracing::warn;

static NEXT_NOTIFICATION_ID: AtomicU64 = AtomicU64::new(1);

/// Give every toast a process-wide identity before it is broadcast. Producers
/// may provide a stable id for progress updates; one-shot notifications get a
/// generated id so dismissing them can be mirrored across every account UI.
pub(crate) fn ensure_notification_id(notification: &mut Notification) {
    if notification.id.is_none() {
        let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed);
        notification.id = Some(format!("mokora-notify-{id}"));
    }
}

/// Where one account's events go: its own interface, plus the overlay while it
/// is the account the window shows.
#[derive(Clone)]
pub struct Route {
    ui_label: Arc<str>,
    account: Arc<str>,
    /// The id of the shown account (shared with the hub).
    active: Arc<RwLock<String>>,
}

impl Route {
    pub fn new(ui_label: &str, account: &str, active: Arc<RwLock<String>>) -> Self {
        Self {
            ui_label: Arc::from(ui_label),
            account: Arc::from(account),
            active,
        }
    }

    fn is_active(&self) -> bool {
        self.active
            .read()
            .map(|id| *id == *self.account)
            .unwrap_or(false)
    }

    pub fn emit<R: Runtime, T: Serialize + Clone>(
        &self,
        app: &AppHandle<R>,
        event: &str,
        payload: &T,
    ) {
        if let Err(e) = app.emit_to(&*self.ui_label, event, payload.clone()) {
            warn!("ipc emit {event} to {} failed: {e}", self.ui_label);
        }
        if self.is_active() {
            let _ = app.emit_to(overlay::LABEL, event, payload.clone());
        }
    }
}

/// Spawn one account's forwarder tasks (the caller aborts them when the
/// account closes).
pub fn install_account<R: Runtime>(
    app: &AppHandle<R>,
    route: Route,
    state: &AppState,
) -> Vec<JoinHandle<()>> {
    vec![
        forward(
            app.clone(),
            route.clone(),
            state.mjai_bus.subscribe(),
            "mjai-event",
        ),
        forward(
            app.clone(),
            route.clone(),
            state.bot_response_bus.subscribe(),
            "bot-response",
        ),
        forward_notifications(app.clone(), state.notify_bus.subscribe()),
        forward(
            app.clone(),
            route.clone(),
            state.analysis_bus.subscribe(),
            "analysis-result",
        ),
        forward(
            app.clone(),
            route.clone(),
            state.history_bus.subscribe(),
            "history-recorded",
        ),
        // Status buses: forward AND snapshot into AppState.
        spawn_bot_status_forwarder(app.clone(), route.clone(), state.clone()),
        spawn_next_game_status_forwarder(app.clone(), route.clone(), state.clone()),
        spawn_capture_status_forwarder(app.clone(), route, state.clone()),
    ]
}

/// Notifications are application-level UI state, not account-local game
/// state. Broadcast each account's notification to every webview so all
/// account interfaces show the same toast and can dismiss it together.
fn forward_notifications<R: Runtime>(
    app: AppHandle<R>,
    mut rx: broadcast::Receiver<Notification>,
) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(mut notification) => {
                    ensure_notification_id(&mut notification);
                    if let Err(e) = app.emit("notify", notification) {
                        warn!("ipc broadcast notify failed: {e}");
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("ipc forwarder notify lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

/// The next-game flow's status is a `watch` (it is its own snapshot): emit
/// each change as `next-game-status`.
fn spawn_next_game_status_forwarder<R: Runtime>(
    app: AppHandle<R>,
    route: Route,
    state: AppState,
) -> JoinHandle<()> {
    let mut rx = state.autoplay_context.next_game_status.subscribe();
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let s = rx.borrow_and_update().clone();
            route.emit(&app, "next-game-status", &s);
        }
    })
}

/// Plain forwarder: subscribe, drain, emit. `Lagged` is logged once and
/// the loop continues (broadcast auto-resumes after lag).
fn forward<R, T>(
    app: AppHandle<R>,
    route: Route,
    mut rx: broadcast::Receiver<T>,
    event: &'static str,
) -> JoinHandle<()>
where
    R: Runtime,
    T: Clone + Serialize + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(payload) => route.emit(&app, event, &payload),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("ipc forwarder {event} lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

fn spawn_bot_status_forwarder<R: Runtime>(
    app: AppHandle<R>,
    route: Route,
    state: AppState,
) -> JoinHandle<()> {
    let mut rx = state.bot_status_bus.subscribe();
    let snapshot = state.bot_status.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(s) => {
                    *snapshot.write().await = s.clone();
                    route.emit(&app, "bot-status", &s);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("ipc forwarder bot-status lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

fn spawn_capture_status_forwarder<R: Runtime>(
    app: AppHandle<R>,
    route: Route,
    state: AppState,
) -> JoinHandle<()> {
    let mut rx = state.capture_status_bus.subscribe();
    let control = state.capture_control.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(s) => {
                    control.lock().await.status = s.clone();
                    route.emit(&app, "capture-status", &s);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("ipc forwarder capture-status lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_ids_are_added_once_and_preserved() {
        let mut generated = Notification::info("hello");
        ensure_notification_id(&mut generated);
        assert!(generated.id.as_deref().is_some_and(|id| !id.is_empty()));

        let mut stable = Notification::warn("warning").id("stable");
        ensure_notification_id(&mut stable);
        assert_eq!(stable.id.as_deref(), Some("stable"));
    }

    /// The overlay follows the shown account, so a route is "active" exactly
    /// while its account is the one the hub says is shown.
    #[test]
    fn a_route_is_active_while_its_account_is_the_shown_one() {
        let shown = Arc::new(RwLock::new("main".to_string()));
        let main = Route::new("ui-main", "main", shown.clone());
        let two = Route::new("ui-2", "2", shown.clone());
        assert!(main.is_active() && !two.is_active());
        *shown.write().unwrap() = "2".to_string();
        assert!(!main.is_active() && two.is_active());
    }
}
