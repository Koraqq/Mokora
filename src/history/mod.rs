//! Persisted game history (`history/index.jsonl` + `history/games/*.mjai.jsonl`).
//!
//! Wired in `lib.rs`: a single `recorder::drive_loop` subscribes to the
//! `MjaiBus` and finalises each `EndGame`-terminated stream by:
//!
//! 1. running [`aggregator::aggregate`] over the buffered events to produce
//!    a `crate::schema::GameRecord` (authoritative platform standings when
//!    available, otherwise Mortal-style score normalisation and ranking,
//!    plus per-game stat counters mirroring `libriichi/src/stat.rs`),
//! 2. writing the full event stream as `games/<id>.mjai.jsonl`,
//! 3. appending the record JSON line to `index.jsonl`,
//! 4. fan-out via `HistoryBus` so the IPC forwarder can emit
//!    `history-recorded` to the frontend.
//!
//! A Mahjong Soul reconnect carries a stable private table id: the recorder
//! keeps completed rounds and replaces the server-restored copy of the current
//! round. Only `NotifyGameEndResult` finalises a record; an explicit
//! `NotifyGameTerminate`, a different next game, or shutdown drops an
//! incomplete buffer.
//!
//! See `src/history/README.md` for module-extension guidance.

pub mod against;
pub mod aggregator;
pub mod daily;
pub mod deal_in;
pub mod recorder;
pub mod rounds;
pub mod store;

pub use store::HistoryStore;
