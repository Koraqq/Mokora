//! One open account's runtime: everything that used to start with the app —
//! the game tracker, the analysis runner, the history recorder, the bot and
//! autoplay managers, capture — started for this account alone, plus the
//! watcher that keeps its login, profile and status current.

use super::{Account, Hub};
use crate::ipc::{self, AppState};
use crate::{analysis, autoplay, bot, game_state, history};
use std::sync::Arc;
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::Emitter;
use tracing::{error, info, warn};

/// Start `account`'s runtime. The returned handles are aborted when the
/// account closes.
pub(super) async fn start(hub: &Hub, account: &Arc<Account>) -> Vec<JoinHandle<()>> {
    let app = hub.app().clone();
    let state = account.state.clone();
    let cfg = state.config.read().await.clone();
    let mut tasks = ipc::install_account(&app, hub.route(account), &state);

    tasks.push(spawn(game_state::tracker::drive_loop(
        state.game_tracker.clone(),
        state.mjai_bus.subscribe(),
        Some(state.post_tracker_bus.clone()),
    )));
    tasks.push(spawn(analysis::runner::drive_loop(
        state.post_tracker_bus.subscribe(),
        state.game_tracker.clone(),
        state.analysis_bus.clone(),
        state.analysis_cache.clone(),
    )));

    // History recorder. Subscribes to the account's MjaiBus and finalises one
    // GameRecord per `EndGame`, tagged with the platform and bots in use.
    tasks.push(spawn(history::recorder::drive_loop(
        state.history_store.clone(),
        state.history_bus.clone(),
        state.history_platform.clone(),
        state.history_bots.clone(),
        state.mjai_bus.subscribe(),
    )));

    if cfg.bot.enabled {
        tasks.push(start_bot_manager(&state));
    }
    if cfg.autoplay.enabled {
        tasks.push(start_autoplay_manager(&state));
    }

    // Inspector mjai-bus subscriber: every event the bridges emit is mirrored
    // into the inspector timeline, so the user sees frames → mjai events → bot
    // reactions in one place.
    {
        let mut rx = state.mjai_bus.subscribe();
        let inspector = state.log_session.inspector();
        tasks.push(spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        inspector.record(crate::schema::InspectorEntry::MjaiEvent {
                            ts_ms: chrono::Local::now().timestamp_millis(),
                            event,
                        });
                    }
                    // Lossy — the inspector file already tells the durable
                    // story; the real-time view tolerates gaps.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        }));
    }

    if cfg.proxy.enabled {
        let for_capture = state.clone();
        tasks.push(spawn(async move {
            if let Err(e) = ipc::capture_supervisor::spawn_capture_supervisor(for_capture).await {
                error!("Capture supervisor failed: {e:#}");
            }
        }));
    }

    tasks.push(spawn(watch(hub.clone(), account.clone())));
    tasks
}

/// The bot manager for `state`; also started when `bot.enabled` is switched on
/// later (see `update_config`).
pub fn start_bot_manager(state: &AppState) -> JoinHandle<()> {
    state
        .bot_manager_started
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (cfg, events, resp, status, notify) = (
        state.config.clone(),
        state.post_tracker_bus.clone(),
        state.bot_response_bus.clone(),
        state.bot_status_bus.clone(),
        state.notify_bus.clone(),
    );
    let inspector = state.log_session.inspector();
    let (runtime, syncs) = (state.runtime.clone(), state.syncs_in_flight.clone());
    spawn(async move {
        if let Err(e) =
            bot::run_bot_manager(cfg, events, resp, status, notify, inspector, runtime, syncs).await
        {
            error!("Bot manager failed: {e:#}");
        }
    })
}

/// The autoplay manager for `state`; also started when `autoplay.enabled` is
/// switched on later (see `update_config`).
pub fn start_autoplay_manager(state: &AppState) -> JoinHandle<()> {
    state
        .autoplay_manager_started
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (cfg, ctx, tracker, mjai, resp, notify) = (
        state.config.clone(),
        state.autoplay_context.clone(),
        state.game_tracker.clone(),
        state.mjai_bus.clone(),
        state.bot_response_bus.clone(),
        state.notify_bus.clone(),
    );
    // The default delay-script location is resolved relative to the loaded
    // config file's directory.
    let config_dir = state
        .config_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    spawn(async move {
        if let Err(e) =
            autoplay::run_autoplay_manager(cfg, ctx, tracker, mjai, resp, notify, config_dir).await
        {
            error!("Autoplay manager failed: {e:#}");
        }
    })
}

/// Keep the account's saved files and the tabs in step with what the game
/// shows: the profile (whenever the traffic moved it), whether the game is
/// connected and in a game, and who is logged in.
async fn watch(hub: Hub, account: Arc<Account>) {
    let state = account.state.clone();
    let config_path = state.config_path.as_ref().clone();
    let lobby = state.autoplay_context.lobby.clone();
    let app = hub.app().clone();
    let mut was = (false, false);
    let mut profile_seen = 0;
    loop {
        tokio::time::sleep(crate::accounts::IDENTITY_POLL).await;

        // The profile, saved whenever the traffic moved it.
        let (changes, live) = lobby.profile();
        if changes != profile_seen && live.account_id.is_some() {
            profile_seen = changes;
            let merged =
                crate::accounts::merge_profile(crate::accounts::read_profile(&config_path), live);
            if let Err(e) = crate::accounts::write_profile(&config_path, &merged) {
                warn!("accounts: could not save the profile: {e:#}");
            }
            let _ = app.emit_to(super::views::ui_label(&account.id), "profile-changed", ());
        }

        // Is the game connected, and is a game being played (the tabs' light)?
        let online = account.online().await;
        let in_game = online && lobby.in_game();
        if (online, in_game) != was {
            if online && !was.0 {
                if let Err(e) = crate::accounts::note_login(&config_path) {
                    warn!("accounts: could not note the login time: {e:#}");
                }
            }
            was = (online, in_game);
            let _ = app.emit("accounts-changed", ());
        }

        let Some((account_id, nickname)) = lobby.account() else {
            continue;
        };
        let identity = crate::accounts::Identity {
            account_id,
            nickname,
        };
        match crate::accounts::note_identity(&config_path, &identity) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(e) => {
                warn!("accounts: could not store the login: {e:#}");
                continue;
            }
        }
        info!(
            "accounts: logged in as {} ({})",
            identity.nickname, identity.account_id
        );
        hub.update_title();
        if hub.is_duplicate(&account.id, identity.account_id) {
            let _ = state.notify_bus.send(
                crate::schema::Notification::warn("Account already open elsewhere").body(format!(
                    "{} is also logged in under another account in Mokora. Mahjong Soul keeps one session per account, so the two will keep logging each other out — log this one in to a different account.",
                    identity.nickname
                )),
            );
        }
        let _ = app.emit("accounts-changed", ());
    }
}
