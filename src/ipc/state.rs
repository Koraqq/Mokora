//! Per-account state, and what the accounts share.
//!
//! Every open account has its own [`AppState`] (see `crate::hub`); a command
//! gets the one of the web view that called it. What the accounts share — the
//! log session, the history store, the Python runtime, the updater and the
//! review queue — lives in [`Shared`] and is handed to each.
//!
//! `AppState` owns the long-lived handles every IPC command and forwarder
//! needs: a clone of each `event_bus` `Sender`, the live `AppConfig`, the
//! log session directory, and the runtime handle that lets commands
//! start / stop the capture backend on demand.
//!
//! Snapshots vs streams: the `*_bus` channels are the canonical event
//! stream, but a frontend that opens a window mid-game needs a one-shot
//! "what's the current state?" answer. To serve that, the IPC forwarder
//! task mirrors every status event into the `bot_status` /
//! `capture_control.status` slots, and the `get_status` command reads
//! them back. Commands that *change* state (set_active_bot,
//! start/stop_capture) also write the snapshot synchronously so a
//! follow-up `get_status` is always consistent with the action that just
//! succeeded.

use crate::analysis::runner::AnalysisCache;
use crate::autoplay::AutoplayContext;
use crate::bot::PythonRuntime;
use crate::config::AppConfig;
use crate::event_bus::{
    AnalysisBus, BotResponseBus, BotStatusBus, CaptureStatusBus, HistoryBus, MjaiBus, NotifyBus,
    PostTrackerBus,
};
use crate::game_state::GameTracker;
use crate::history::recorder::{SharedBots, SharedPlatform};
use crate::history::HistoryStore;
use crate::logger::Session;
use crate::schema::{BotStatus, CaptureStatus};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, Notify, RwLock};

/// Per-running-capture-backend control handle.
///
/// `stop` is `Some` while the backend task is alive; sending `()`
/// triggers graceful shutdown and the task exits. After exit the
/// supervisor sets `stop = None` and updates `status`.
///
/// `force_close` is shared with the hudsucker handler when MITM is the
/// active backend; calling `notify_waiters()` kicks every in-flight
/// WebSocket flow so the game client actually disconnects (graceful
/// shutdown alone only blocks new connections — existing flows would
/// otherwise drain naturally). The Chromium backend ignores it.
pub struct CaptureControl {
    pub status: CaptureStatus,
    pub stop: Option<oneshot::Sender<()>>,
    pub force_close: Arc<Notify>,
    /// The last run ended without a stop request — for the browser backend,
    /// the game browser was closed (or died). Set before the end's status is
    /// sent, so a status listener can read it (see `accounts`).
    pub ended_by_itself: bool,
}

impl Default for CaptureControl {
    fn default() -> Self {
        Self {
            status: CaptureStatus::Stopped,
            stop: None,
            force_close: Arc::new(Notify::new()),
            ended_by_itself: false,
        }
    }
}

/// What every account uses the one of: a single log session, history store,
/// Python runtime, updater and review queue.
#[derive(Clone)]
pub struct Shared {
    pub log_session: Arc<Session>,
    pub history_store: Arc<HistoryStore>,
    /// Bundled-or-system Python + uv; see [`AppState::runtime`].
    pub runtime: Option<PythonRuntime>,
    pub syncs_in_flight: Arc<Mutex<HashSet<String>>>,
    pub updater_lock: Arc<Mutex<()>>,
    pub review: Arc<crate::review::job::ReviewManager>,
}

impl Shared {
    pub fn new(
        log_session: Arc<Session>,
        history_store: Arc<HistoryStore>,
        runtime: Option<PythonRuntime>,
    ) -> Self {
        Self {
            log_session,
            history_store,
            runtime,
            syncs_in_flight: Arc::new(Mutex::new(HashSet::new())),
            updater_lock: Arc::new(Mutex::new(())),
            review: Arc::new(crate::review::job::ReviewManager::new()),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    /// Which account this is (`accounts::MAIN_ID` or its folder number).
    pub account_id: Arc<str>,
    /// The DevTools port of this account's web views (see
    /// `capture::chromium::embedded`).
    pub debug_port: u16,
    /// This account's game view as the embedded capture reads it on Linux,
    /// where the view has no DevTools port (see `capture::webkit`).
    #[cfg(target_os = "linux")]
    pub webkit_tap: Arc<crate::capture::webkit::WebKitTap>,
    pub config: Arc<RwLock<AppConfig>>,
    pub config_path: Arc<PathBuf>,
    pub log_session: Arc<Session>,

    pub mjai_bus: MjaiBus,
    /// Events re-emitted after the tracker applied them. The bot manager
    /// runs off this rather than `mjai_bus` — see `event_bus::TrackedEvent`.
    pub post_tracker_bus: PostTrackerBus,
    pub bot_response_bus: BotResponseBus,
    pub bot_status_bus: BotStatusBus,
    pub capture_status_bus: CaptureStatusBus,
    pub notify_bus: NotifyBus,
    pub analysis_bus: AnalysisBus,
    pub history_bus: HistoryBus,

    /// Latest BotStatus seen on the bus. Forwarder writes; commands read.
    pub bot_status: Arc<RwLock<BotStatus>>,
    pub capture_control: Arc<Mutex<CaptureControl>>,
    /// Live game-state mirror. Future IPC commands lock this and call
    /// `snapshot()` to expose hands/scores/dora to the frontend.
    pub game_tracker: Arc<Mutex<GameTracker>>,
    /// Latest analysis result, populated by the analysis runner. Read by
    /// the `get_analysis` Tauri command for one-shot queries.
    pub analysis_cache: AnalysisCache,
    /// Persistent game-history store. Written by the recorder task,
    /// read by `list_game_history` / `get_game_history_*` IPC commands.
    pub history_store: Arc<HistoryStore>,
    /// Shared cell holding the platform tag the history recorder stamps
    /// onto each finalised game. `update_config` updates this when the
    /// user switches bridges so subsequent records pick up the new tag
    /// without a relaunch.
    pub history_platform: SharedPlatform,
    /// Bots in use, stamped on every recorded game; written on config save.
    pub history_bots: SharedBots,

    /// Bundled-or-system Python + uv. `None` on dev boxes lacking both —
    /// install/sync commands surface a friendly error instead of panicking.
    /// The bot manager still starts: the built-in native bot needs no Python,
    /// and only a `mjai_bot/*` subprocess bot fails to spawn without a runtime.
    pub runtime: Option<PythonRuntime>,
    /// Names of bots whose `uv sync` is currently in flight. Both the
    /// `sync_bot_deps` command and `BotManager::spawn_runner` acquire-or-bail
    /// so two parallel syncs against the same venv can't trample each other.
    pub syncs_in_flight: Arc<Mutex<HashSet<String>>>,
    /// Set to `true` once a `BotManager` has been spawned for this process.
    /// Used by `update_config` to start the manager on a runtime
    /// false→true flip of `bot.enabled` (e.g. when the first-run wizard
    /// finishes) without ever spawning twice. There is no off-switch:
    /// once started, the manager runs for the lifetime of the process.
    pub bot_manager_started: Arc<AtomicBool>,
    /// Shared with the chromium capture backend (which writes the page
    /// handle on Majsoul WS open) and the `AutoplayManager` (which reads
    /// it to dispatch input). Always present; `enabled = false` in
    /// config simply suppresses the manager spawn — the context still
    /// exists so a runtime toggle works without re-plumbing.
    pub autoplay_context: Arc<AutoplayContext>,
    /// Set to `true` once an `AutoplayManager` has been spawned for this
    /// process. Used by `update_config` to start the manager on a
    /// runtime false→true flip of `autoplay.enabled` without re-spawning.
    pub autoplay_manager_started: Arc<AtomicBool>,
    /// One update check at a time: `check_for_update` `try_lock()`s it so
    /// a mashed button cannot start two fetches.
    pub updater_lock: Arc<Mutex<()>>,
    /// The game-review queue (see `crate::review`).
    pub review: Arc<crate::review::job::ReviewManager>,
}

impl AppState {
    /// The state of a newly opened account: its own buses, tracker and
    /// capture control, and the shared parts from `shared`.
    pub fn new(
        config: AppConfig,
        config_path: PathBuf,
        account_id: &str,
        debug_port: u16,
        shared: &Shared,
    ) -> Self {
        let history_platform = crate::history::recorder::shared_platform(
            crate::schema::Platform::from(config.platform.kind),
        );
        let history_bots = crate::history::recorder::shared_bots(
            crate::history::recorder::ActiveBots::from_config(&config.bot),
        );
        Self {
            account_id: Arc::from(account_id),
            debug_port,
            #[cfg(target_os = "linux")]
            webkit_tap: crate::capture::webkit::WebKitTap::new(),
            config: Arc::new(RwLock::new(config)),
            config_path: Arc::new(config_path),
            log_session: shared.log_session.clone(),
            mjai_bus: crate::event_bus::mjai_bus(),
            post_tracker_bus: crate::event_bus::post_tracker_bus(),
            bot_response_bus: crate::event_bus::bot_response_bus(),
            bot_status_bus: crate::event_bus::bot_status_bus(),
            capture_status_bus: crate::event_bus::capture_status_bus(),
            notify_bus: crate::event_bus::notify_bus(),
            analysis_bus: crate::event_bus::analysis_bus(),
            history_bus: crate::event_bus::history_bus(),
            bot_status: Arc::new(RwLock::new(BotStatus::Idle)),
            capture_control: Arc::new(Mutex::new(CaptureControl::default())),
            game_tracker: crate::game_state::tracker::new_handle(),
            analysis_cache: Arc::new(RwLock::new(None)),
            history_store: shared.history_store.clone(),
            history_platform,
            history_bots,
            runtime: shared.runtime.clone(),
            syncs_in_flight: shared.syncs_in_flight.clone(),
            bot_manager_started: Arc::new(AtomicBool::new(false)),
            autoplay_context: Arc::new(AutoplayContext::with_history(shared.history_store.clone())),
            autoplay_manager_started: Arc::new(AtomicBool::new(false)),
            updater_lock: shared.updater_lock.clone(),
            review: shared.review.clone(),
        }
    }
}
