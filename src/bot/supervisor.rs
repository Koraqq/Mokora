//! Spawns + drives a `BotManager`.
//!
//! Lives outside `lib::run` so the same entry point can be reused both at
//! startup (when `bot.enabled` is true on first config load) and at
//! runtime (when the user flips `bot.enabled` via the first-run wizard or
//! settings page — `update_config` calls this instead of forcing the user
//! to restart Mokora).

use crate::bot::registry::BotRegistry;
use crate::bot::runtime::PythonRuntime;
use crate::bot::BotManager;
use crate::config::AppConfig;
use crate::event_bus::{BotResponseBus, BotStatusBus, NotifyBus, PostTrackerBus};
use crate::inspector::InspectorWriter;
use crate::util;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::warn;

/// Build a `BotManager` from the shared `config` + `runtime` and run it
/// until the post-tracker bus closes. Returns `Err` only on setup failure (missing
/// runtime, unscannable bot dir); transient runtime errors are absorbed by
/// the manager itself.
///
/// The manager holds the shared `Arc<RwLock<AppConfig>>` and re-reads the
/// active-bot selection at each `start_game`, so a model switch made while
/// the manager is running takes effect on the next game without a relaunch.
#[allow(clippy::too_many_arguments)]
pub async fn run_bot_manager(
    config: Arc<RwLock<AppConfig>>,
    events: PostTrackerBus,
    response_bus: BotResponseBus,
    status_bus: BotStatusBus,
    notify_bus: NotifyBus,
    inspector: InspectorWriter,
    runtime: Option<PythonRuntime>,
    syncs_in_flight: Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    let bot_dir = {
        let cfg = config.read().await;
        let bot_dir = util::resolve_dir(Path::new(&cfg.bot.dir));
        // Diagnostic-only: warn early if the configured bots aren't present
        // *now*. The manager rescans on every spawn so a bot installed after
        // this point is still picked up — the warning here is just to surface
        // mis-config quickly in logs, not to gate startup.
        let registry = BotRegistry::scan(&bot_dir)?;
        for (label, name) in [("4p", &cfg.bot.active_4p), ("3p", &cfg.bot.active_3p)] {
            // Built-in native bots aren't in the `mjai_bot/` registry — don't
            // warn about them being "missing".
            if !name.is_empty()
                && !crate::bot::native::is_native(name)
                && registry.find(name).is_none()
            {
                warn!(
                    "configured {} bot {:?} not found under {}; available: {:?}",
                    label,
                    name,
                    bot_dir.display(),
                    registry.names().collect::<Vec<_>>()
                );
            }
        }
        bot_dir
    };

    // A Python runtime is optional: the built-in native bot needs none. Only
    // Python `mjai_bot/*` subprocess bots require it, and that's enforced
    // per-spawn inside the manager (a missing runtime fails just that spawn,
    // not the whole manager) so the native bot always runs.
    if runtime.is_none() {
        warn!(
            "no python3+uv runtime found; the built-in bot will still work, but Python mjai_bot subprocess bots cannot be spawned"
        );
    }

    let manager = BotManager::new(
        runtime,
        bot_dir,
        config,
        response_bus,
        status_bus,
        notify_bus,
        inspector,
        syncs_in_flight,
    );
    let rx = events.subscribe();
    manager.run(rx).await
}
