//! The review queue: one game at a time on a low-priority thread.
//!
//! - Games that already have a current review are skipped, so pressing the
//!   button again only does what is left.
//! - While a live game is on (an event on the mjai bus within
//!   [`LIVE_WINDOW_MS`], or the lobby flow says it is in a game) the queue
//!   waits, unless the caller said not to pause. A review never competes with
//!   the move the player is waiting for.
//! - The thread runs below normal priority and gives the CPU back between
//!   events (see `platform::lower_current_thread_priority`).
//! - Progress is pushed after every game; a cancel stops within one event.

use super::store::ReviewStore;
use super::{current_key, review_game};
use crate::bot::native;
use crate::event_bus::MjaiBus;
use crate::history::HistoryStore;
use crate::schema::review::ReviewProgress;
use crate::schema::GameRecord;
use anyhow::{bail, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

/// How long after the last event of a live game it still counts as on.
pub const LIVE_WINDOW_MS: u64 = 45_000;
/// Rest between events, so the review leaves the CPU to a live decision.
const THROTTLE: Duration = Duration::from_micros(500);
/// How often a paused queue looks again.
const PAUSE_POLL: Duration = Duration::from_secs(1);

/// What to review.
pub struct Job {
    pub history: Arc<HistoryStore>,
    pub reviews: ReviewStore,
    pub ids: Vec<String>,
    /// The model to use where it fits the game; each game's own default
    /// (the strongest built-in one of its player count) otherwise.
    pub model: Option<String>,
    /// Wait while a live game is on.
    pub pause_in_game: bool,
    /// Is a live game on, besides what the mjai bus shows.
    pub live: Arc<dyn Fn() -> bool + Send + Sync>,
}

pub struct ReviewManager {
    progress: Mutex<ReviewProgress>,
    cancel: AtomicBool,
    running: AtomicBool,
    last_live_ms: AtomicU64,
    watching: AtomicBool,
    /// Events worked through so far, over every game: how much the queue has
    /// done, whether or not a game has finished.
    events: AtomicU64,
}

impl Default for ReviewManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewManager {
    pub fn new() -> Self {
        Self {
            progress: Mutex::new(ReviewProgress::idle()),
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
            last_live_ms: AtomicU64::new(0),
            watching: AtomicBool::new(false),
            events: AtomicU64::new(0),
        }
    }

    pub fn progress(&self) -> ReviewProgress {
        self.progress.lock().expect("review progress lock").clone()
    }

    /// Events of games worked through so far.
    pub fn events_processed(&self) -> u64 {
        self.events.load(Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Ask the running queue to stop.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Note that a live game is producing events.
    pub fn note_live(&self) {
        self.last_live_ms.store(super::now_ms(), Ordering::SeqCst);
    }

    /// An event of a live game came within [`LIVE_WINDOW_MS`].
    pub fn live_recently(&self) -> bool {
        let last = self.last_live_ms.load(Ordering::SeqCst);
        last != 0 && super::now_ms().saturating_sub(last) < LIVE_WINDOW_MS
    }

    /// Start watching the mjai bus for a live game (once).
    pub fn ensure_watch(self: &Arc<Self>, bus: &MjaiBus) {
        if self.watching.swap(true, Ordering::SeqCst) {
            return;
        }
        use tokio::sync::broadcast::error::RecvError;
        let mut rx = bus.subscribe();
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(_) | Err(RecvError::Lagged(_)) => me.note_live(),
                    Err(RecvError::Closed) => return,
                }
            }
        });
    }

    fn set(&self, p: ReviewProgress, emit: &dyn Fn(&ReviewProgress)) {
        *self.progress.lock().expect("review progress lock") = p.clone();
        emit(&p);
    }

    /// Queue the games of `job` that have no current review and start on
    /// them. Returns how many were queued (0 = nothing left to do). Fails when
    /// a queue is already running.
    pub fn start<F>(self: &Arc<Self>, job: Job, emit: F) -> Result<u32>
    where
        F: Fn(&ReviewProgress) + Send + 'static,
    {
        if self.running.swap(true, Ordering::SeqCst) {
            bail!("a review is already running");
        }
        let records = match job.history.read_all() {
            Ok(r) => r,
            Err(e) => {
                self.running.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        let todo = todo_of(&job, &records);
        if todo.is_empty() {
            self.running.store(false, Ordering::SeqCst);
            self.set(
                ReviewProgress {
                    state: "done".into(),
                    ..ReviewProgress::idle()
                },
                &emit,
            );
            return Ok(0);
        }
        self.cancel.store(false, Ordering::SeqCst);
        let queued = todo.len() as u32;
        self.set(
            ReviewProgress {
                state: "running".into(),
                total: queued,
                done: 0,
                current: None,
                message: None,
            },
            &emit,
        );
        let me = self.clone();
        std::thread::Builder::new()
            .name("review".into())
            .spawn(move || {
                crate::platform::lower_current_thread_priority();
                me.run(&job, todo, &emit);
                me.running.store(false, Ordering::SeqCst);
            })?;
        Ok(queued)
    }

    /// The queue itself; public for the self-check and the tests.
    pub fn run(&self, job: &Job, todo: Vec<(GameRecord, String)>, emit: &dyn Fn(&ReviewProgress)) {
        let total = todo.len() as u32;
        let mut failed = 0u32;
        let mut cancelled = false;
        for (n, (record, model)) in todo.iter().enumerate() {
            let at = |state: &str, current: Option<&str>| ReviewProgress {
                state: state.into(),
                total,
                done: n as u32,
                current: current.map(String::from),
                message: None,
            };
            if self.cancel.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            self.set(at("running", Some(&record.id)), emit);
            let outcome = self.review_one(job, record, model, &|paused| {
                self.set(
                    at(if paused { "paused" } else { "running" }, Some(&record.id)),
                    emit,
                );
            });
            match outcome {
                Ok(true) => {}
                Ok(false) => {
                    cancelled = true;
                    break;
                }
                Err(e) => {
                    failed += 1;
                    warn!("review of {} failed: {e:#}", record.id);
                }
            }
        }
        let done = if cancelled { "cancelled" } else { "done" };
        let finished = self.progress();
        self.set(
            ReviewProgress {
                state: done.into(),
                total,
                done: if cancelled { finished.done } else { total },
                current: None,
                message: (failed > 0).then(|| format!("{failed} failed")),
            },
            emit,
        );
        info!("review queue {done}: {total} queued, {failed} failed");
    }

    /// Review one game and save it. `Ok(false)` when it was cancelled.
    fn review_one(
        &self,
        job: &Job,
        record: &GameRecord,
        model: &str,
        on_pause: &dyn Fn(bool),
    ) -> Result<bool> {
        if record.our_seat.is_none() {
            return Ok(true);
        }
        let Some(events) = job.history.get_events(&record.id)? else {
            bail!("no event log");
        };
        let mut paused = false;
        let mut stop = || {
            if self.cancel.load(Ordering::SeqCst) {
                return true;
            }
            while job.pause_in_game && (self.live_recently() || (job.live)()) {
                if !paused {
                    paused = true;
                    on_pause(true);
                }
                std::thread::sleep(PAUSE_POLL);
                if self.cancel.load(Ordering::SeqCst) {
                    return true;
                }
            }
            if paused {
                paused = false;
                on_pause(false);
            }
            std::thread::sleep(THROTTLE);
            self.events.fetch_add(1, Ordering::SeqCst);
            false
        };
        // The module keeps the model built from one game to the next.
        let Some(report) = review_game(record, &events, model, &mut stop)? else {
            return Ok(false);
        };
        job.reviews.save(&report)?;
        self.set_done_one();
        Ok(true)
    }

    fn set_done_one(&self) {
        let mut p = self.progress.lock().expect("review progress lock");
        p.done = (p.done + 1).min(p.total);
    }
}

/// The games of `job` still to review, each with the model to use: the ones
/// with a seat of the player's and no current review.
pub fn todo_of(job: &Job, records: &[GameRecord]) -> Vec<(GameRecord, String)> {
    let wanted: std::collections::HashSet<&str> = job.ids.iter().map(String::as_str).collect();
    records
        .iter()
        .filter(|r| wanted.contains(r.id.as_str()) && r.our_seat.is_some())
        .filter_map(|r| {
            let model = model_for(job.model.as_deref(), r.num_players);
            (current_key(r, &model).is_some() && !job.reviews.is_current(r, &model))
                .then(|| (r.clone(), model))
        })
        .collect()
}

/// `picked` if it can review a game of `num_players`, else the default.
pub fn model_for(picked: Option<&str>, num_players: u8) -> String {
    picked
        .map(native::canonical_name)
        .filter(|m| {
            native::review_models()
                .iter()
                .any(|(n, p)| n == m && *p == num_players)
        })
        .unwrap_or_else(|| native::default_review_model(num_players))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::testkit;

    fn job(dir: &std::path::Path, history: Arc<HistoryStore>, ids: Vec<String>) -> Job {
        Job {
            history,
            reviews: ReviewStore::new(dir),
            ids,
            model: None,
            pause_in_game: true,
            live: Arc::new(|| false),
        }
    }

    /// A model that does not fit the game's player count falls back to the
    /// default for it; one that fits is used.
    #[test]
    fn the_picked_model_is_used_only_where_it_fits() {
        assert_eq!(model_for(None, 3), native::MORA_3P);
        assert_eq!(model_for(None, 4), native::MORA_4P);
        assert_eq!(model_for(Some(native::MORA_3P), 3), native::MORA_3P);
        assert_eq!(model_for(Some(native::MORA_3P), 4), native::MORA_4P);
        // The dropped base bot is answered by its replacement.
        assert_eq!(model_for(Some("base-3p"), 3), native::MORA_3P);
        // Yakuman plays by a planner, not the model's policy: never a reviewer.
        assert_eq!(model_for(Some(native::YAKUMAN_3P), 3), native::MORA_3P);
        // The old names are still understood.
        assert_eq!(model_for(Some("mora-native3p"), 3), native::MORA_3P);
    }

    /// The queue reviews each game once: asked again, it finds nothing left.
    /// (Reviews in the paid module: `MOKORA_PRO_DLL=… cargo test -- --ignored`.)
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn a_reviewed_game_is_not_reviewed_again() {
        assert!(crate::pro::load_for_tests(), "set MOKORA_PRO_DLL");
        let dir = tempfile::tempdir().unwrap();
        let (history, id) = testkit::history_with_one_game(dir.path());
        let j = job(dir.path(), history.clone(), vec![id.clone()]);
        let records = history.read_all().unwrap();
        let todo = todo_of(&j, &records);
        assert_eq!(todo.len(), 1);

        let manager = ReviewManager::new();
        manager.run(&j, todo, &|_| {});
        assert_eq!(manager.progress().state, "done");
        let record = &records[0];
        assert!(j
            .reviews
            .is_current(record, &model_for(None, record.num_players)));
        assert!(todo_of(&j, &records).is_empty(), "nothing left to review");

        // Another weights version would make the saved one stale.
        let mut summary = j.reviews.summary(&id, &model_for(None, 4)).unwrap();
        summary.key = "an older key".into();
        std::fs::write(
            j.reviews
                .dir()
                .join(format!("{id}.{}.sum.json", model_for(None, 4))),
            serde_json::to_vec(&summary).unwrap(),
        )
        .unwrap();
        assert_eq!(
            todo_of(&j, &records).len(),
            1,
            "a stale review is made again"
        );
    }

    /// A cancel before the first game leaves nothing saved.
    #[test]
    fn a_cancelled_queue_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (history, id) = testkit::history_with_one_game(dir.path());
        let j = job(dir.path(), history.clone(), vec![id.clone()]);
        let records = history.read_all().unwrap();
        let todo = todo_of(&j, &records);
        let manager = ReviewManager::new();
        manager.cancel();
        manager.run(&j, todo, &|_| {});
        assert_eq!(manager.progress().state, "cancelled");
        assert!(j.reviews.summary(&id, &model_for(None, 4)).is_none());
    }

    /// The queue on the games of a real history (`REVIEW_HISTORY_DIR`, a copy
    /// of a `history/` folder): each of the newest few is reviewed once,
    /// timed, and a second pass finds nothing left. Prints the timings.
    #[test]
    #[ignore = "needs a real history folder (REVIEW_HISTORY_DIR)"]
    fn the_queue_on_real_games() {
        let Ok(dir) = std::env::var("REVIEW_HISTORY_DIR") else {
            return;
        };
        let history = Arc::new(HistoryStore::new(dir.into()).unwrap());
        let mut records = history.read_all().unwrap();
        records.retain(|r| r.our_seat.is_some());
        records.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        records.truncate(8);
        let scratch = tempfile::tempdir().unwrap();
        let j = job(
            scratch.path(),
            history.clone(),
            records.iter().map(|r| r.id.clone()).collect(),
        );
        let all = history.read_all().unwrap();
        let todo = todo_of(&j, &all);
        assert_eq!(todo.len(), records.len());
        let manager = ReviewManager::new();
        let t = std::time::Instant::now();
        manager.run(&j, todo, &|_| {});
        let took = t.elapsed();
        let decisions: usize = records
            .iter()
            .map(|r| {
                let m = model_for(None, r.num_players);
                j.reviews
                    .report(&r.id, &m)
                    .map_or(0, |rep| rep.decisions.len())
            })
            .sum();
        println!(
            "{} games, {decisions} decisions, {:.1}s ({:.2}s per game)",
            records.len(),
            took.as_secs_f64(),
            took.as_secs_f64() / records.len() as f64
        );
        assert_eq!(manager.progress().state, "done");
        assert!(
            todo_of(&j, &all).is_empty(),
            "a second pass finds nothing to do"
        );
    }
}
