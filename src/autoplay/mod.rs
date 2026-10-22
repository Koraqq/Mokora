//! Autoplay: perform the bot's decisions in the real client, via CDP.
//!
//! Data flow: bot decision (an `MjaiEvent`) joins the latest
//! `GameStateSnapshot` and `legal_actions` from the `riichienv-core` game
//! state, and the platform impl plans a sequence of `Step`s. Action
//! availability is sourced from the riichi engine, not from the platform
//! protocol parser, so the same logic works across platforms.
//!
//! How a step reaches the game differs by platform. **Mahjong Soul** renders
//! to a canvas and exposes nothing to script, so the only way in is
//! synthesised mouse input at reconstructed coordinates. Everything expensive
//! here exists because of that: coordinate tables, candidate-row index
//! arithmetic, click verification and retry. Reach is two inputs — declare,
//! then discard — taken from the bot's own `Reach { pai }` and performed in
//! one plan; Mahjong Soul holds its reach popup open for the tile.
//!
//! Other games plug in the same way (a `PlatformAutoplay` impl plus whatever
//! `Step` kinds their client needs); `_parked/other-games/` holds a DOM-driven
//! web client (Tenhou) and a frame-injected native one (Riichi City) as worked
//! examples, with the list of what to put back.
//!
//! Every response arriving here is a decision the riichi engine asked for —
//! `bot::manager` does not flush to the bot otherwise — so an `MjaiEvent::None`
//! means *decline*, and never "the bot had nothing to say".
//!
//! Module layout:
//! - [`context`] — shared state between the chromium capture backend
//!   and this manager (page handle + canvas rect cache).
//! - [`platform`] — `PlatformAutoplay` trait + the `Step` types
//!   (`Click` / `Sleep`).
//! - [`majsoul`] — the production Majsoul implementation: 16:9
//!   coordinate tables (ported from an earlier Python autoplay) + plan dispatch covering all mjai
//!   action types.
//! - [`cdp_input`] — chromiumoxide wrappers (`dispatch_click_shaped_guarded`,
//!   `evaluate_canvas_rect`) and the screenshot reads the next-game flow
//!   steers by.
//! - [`lobby`] / [`next_game`] — Majsoul only: start the next ranked game
//!   once one ends, steering by the client's own lobby uplink rather than by
//!   anything on screen.
//! - [`manager`] — the long-lived `AutoplayManager` task that owns
//!   per-game state and drives the plan.
//! - [`verify`] — did the click land? Counts the client's own uplink
//!   input commands (bumped by the Majsoul bridge) so the manager can
//!   tell a click that registered from one the UI swallowed, and retry.
//!
//! Entry point: [`manager::run_autoplay_manager`].

pub mod budget;
pub mod cdp_input;
pub mod context;
pub mod delay;
pub mod lobby;
pub mod majsoul;
pub mod manager;
pub mod next_game;
pub mod page;
pub mod platform;
pub mod resize;
pub mod route;
pub mod verify;

pub use budget::{BudgetSource, SharedTimeBudget, TimeBudget};
pub use context::{AutoplayContext, CanvasRect};
pub use manager::run_autoplay_manager;
pub use platform::{ActionContext, PlanResult, PlatformAutoplay, Step};
pub use verify::{InputKind, InputWatch, SharedInputWatch};
