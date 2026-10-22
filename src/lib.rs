// `async_trait` expands to a `#[must_use]` on futures that are already must-use.
#![allow(clippy::double_must_use)]

pub mod accounts;
pub mod analysis;
pub mod autoplay;
pub mod bot;
pub mod bridge;
pub mod capture;
pub mod cli;
pub mod config;
pub mod event_bus;
pub mod game_state;
pub mod github;
pub mod history;
pub mod hub;
pub mod inspector;
pub mod ipc;
pub mod license;
pub mod logger;
pub mod platform;
pub mod pro;
pub mod proxy;
pub mod review;
pub mod schema;
pub mod updater;
pub mod util;

use clap::Parser;
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;
use tracing::{error, info, warn};

pub fn run() {
    platform::setup();

    let args = cli::Cli::parse();
    // Started with no `--config`: the startup account's (the picker's star), or
    // the first account's.
    let config = args.config.or_else(accounts::startup_config);
    let (cfg, _) = config::load_config(config.as_deref());
    // Before any capture starts, so the first login is already masked.
    bridge::redact::set_enabled(cfg.privacy.redact_credentials);

    let log_dir = util::resolve_dir(&cfg.logging.dir);
    let targets = [
        logger::LogTarget::new("proxy", "mokora::proxy"),
        logger::LogTarget::new("bot", "mokora::bot"),
    ];
    let session = match logger::init(
        &log_dir,
        &cfg.logging.level,
        &cfg.logging.all_level,
        &targets,
    ) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            // Read-only fs fallback: retry under user data dir.
            if let Some(fallback) =
                util::user_config_root().map(|r| r.join(util::strip_leading_dot(&cfg.logging.dir)))
            {
                if fallback != log_dir {
                    eprintln!(
                        "Logger init at {} failed: {e:?}. Retrying at {}",
                        log_dir.display(),
                        fallback.display()
                    );
                    match logger::init(
                        &fallback,
                        &cfg.logging.level,
                        &cfg.logging.all_level,
                        &targets,
                    ) {
                        Ok(s) => Arc::new(s),
                        Err(e2) => {
                            eprintln!("Failed to initialise logger: {e2:?}");
                            return;
                        }
                    }
                } else {
                    eprintln!("Failed to initialise logger: {e:?}");
                    return;
                }
            } else {
                eprintln!("Failed to initialise logger: {e:?}");
                return;
            }
        }
    };

    // First thing once the log exists: a panic anywhere from here on is
    // written to it (see `logger::install_panic_hook`).
    logger::install_panic_hook();
    info!("Config loaded: {cfg:?}");
    info!("Log session at {}", session.dir().display());

    // Persistent game-history store. Lives under `<config_root>/history/`
    // by default — independent of the volatile session log dir. Failure
    // to create the dir is fatal: the recorder task and IPC commands
    // both depend on it.
    let history_root = util::resolve_dir(Path::new("./history"));
    let history_store = match history::HistoryStore::new(history_root.clone()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!(
                "Failed to initialise history store at {}: {e:#}",
                history_root.display()
            );
            return;
        }
    };
    info!("History store at {}", history_root.display());

    let overlay_cfg = cfg.overlay.clone();
    // The account to open first, and show.
    let first = accounts::startup_account();

    tauri::Builder::default()
        // One Mokora at a time (it holds every account already): starting it
        // again brings the running one to the front instead. Registered first,
        // so a second start ends before it sets anything up.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(hub) = app.try_state::<hub::Hub>() {
                hub.show_window();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // The overlay's geometry is still saved and restored, but not
                // by the plugin's automatic pass: that restores `StateFlags::all()`,
                // which would put decorations back on a deliberately frameless
                // window. `ipc::overlay::open` restores position+size itself.
                .skip_initial_state(ipc::overlay::LABEL)
                // Whether a window was on show is not remembered: a saved
                // "hidden" would start the next launch with no window at all
                // and nothing to click.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .invoke_handler(crate::ipc_handlers!())
        .setup(move |app| {
            // The Python + uv runtime is located here because bundled binaries
            // live under `<resource_dir>/runtime/...`, which is only resolvable
            // once the AppHandle exists.
            let resource_dir = app.path().resource_dir().ok();
            let runtime = bot::PythonRuntime::locate(resource_dir.as_deref()).ok();
            match &runtime {
                Some(rt) => info!(
                    "bot runtime: python={} uv={} mode={:?}",
                    rt.python().display(),
                    rt.uv().display(),
                    rt.mode()
                ),
                None => warn!(
                    "no python3+uv runtime found (neither bundled nor on PATH); bot install/sync will be unavailable"
                ),
            }

            let hub = hub::Hub::new(
                app.handle().clone(),
                ipc::Shared::new(session.clone(), history_store.clone(), runtime),
            );
            app.manage(hub.clone());

            // The license: one for the whole app, renewed in the background.
            let license = license::LicenseManager::new(app.handle().clone());
            app.manage(license.clone());
            license.install();
            license.start_background();
            // The paid module: the one kept here loads before any account
            // opens; the installer fetches a new one for a paid account.
            pro::load_installed();
            let installer = pro::Installer::new(app.handle().clone(), license.clone());
            app.manage(installer.clone());
            installer.start();

            // One window for every account; each account adds its views to it.
            hub::views::create_main_window(app.handle())?;
            hub::handle_close(&hub);
            hub.watch_window();
            let id = if accounts::config_path_of(&first).is_none() {
                warn!("account {first} does not exist; opening the first account");
                accounts::MAIN_ID.to_string()
            } else if !accounts::may_open(&first) {
                warn!("account {first} is past the free limit; opening the first account");
                accounts::MAIN_ID.to_string()
            } else {
                first.clone()
            };
            hub.open_now(&id)?;
            hub.show_now(&id);

            // Reopen the suggestion overlay if it was left enabled. Safe before
            // any game data exists — it renders its empty state and fills in on
            // the first `bot-response`.
            ipc::overlay::reconcile(app.handle(), &overlay_cfg);

            // Best-effort terminal ctrl_c → graceful shutdown. GUI close goes
            // through the window's close event, not here.
            let for_signal = hub.clone();
            tauri::async_runtime::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                for_signal.quit_all().await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
