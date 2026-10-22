//! The review's self-check, run on the player's real games.
//!
//! It answers, with the games on this machine rather than with a promise:
//!
//! - **No leak.** The recorded logs hold only what the player could see (the
//!   censor changes nothing in them). Every hidden tile of a copy is then
//!   planted with a real one and the copy reviewed: the observation the model
//!   was shown must be identical at every decision, because the review blanks
//!   what the player could not see. (Run without the censor, the same copy
//!   shows how many decisions a leak would have changed — for information.)
//! - **The same answer twice.** Two reviews of a game are identical.
//! - **Passes are found.** Every call window the model was asked at is
//!   settled as a pass or a call, and the played action is among the model's
//!   ranked ones.
//! - **No cost to a live game.** The model's call timed alone, then timed
//!   again while a review runs beside it as it would in the app (below normal
//!   priority, in its own thread, between events).

use super::job::{Job, ReviewManager};
use super::censor;
use super::store::ReviewStore;
use super::{game_length, model_info, now_ms};
use crate::bot::native;
use crate::history::HistoryStore;
use crate::schema::review::{LatencyStats, ReviewRun, SelfCheckReport};
use crate::schema::{GameRecord, MjaiEvent};
use anyhow::{Context, Result};
use native_bot::engine::Engine;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// How much of the history the self-check reads.
#[derive(Clone, Copy)]
pub struct Options {
    /// Newest games of the player's own to read.
    pub max_games: usize,
    /// Of them, how many get the leak plant.
    pub leak_games: usize,
    /// Of them, how many are reviewed twice.
    pub determinism_games: usize,
    /// Model calls timed in each of the two latency runs.
    pub latency_calls: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_games: 30,
            leak_games: 6,
            determinism_games: 6,
            latency_calls: 150,
        }
    }
}

struct Game {
    record: GameRecord,
    events: Vec<MjaiEvent>,
    seat: u8,
    model: String,
}

/// A copy of `events` with every tile of a seat other than `seat`'s that the
/// log hides replaced by a real-looking one.
fn planted(events: &[MjaiEvent], seat: u8) -> Vec<MjaiEvent> {
    const TILES: [&str; 9] = ["1m", "9m", "1p", "9p", "1s", "9s", "E", "S", "C"];
    events
        .iter()
        .enumerate()
        .map(|(i, ev)| match ev {
            MjaiEvent::StartKyoku { tehais, .. } => {
                let mut out = ev.clone();
                if let MjaiEvent::StartKyoku { tehais: t, .. } = &mut out {
                    for (s, hand) in tehais.iter().enumerate() {
                        if s != usize::from(seat) {
                            t[s] = (0..hand.len())
                                .map(|k| TILES[(s + k + i) % TILES.len()].to_string())
                                .collect();
                        }
                    }
                }
                out
            }
            MjaiEvent::Tsumo { actor, .. } if *actor != seat => MjaiEvent::Tsumo {
                actor: *actor,
                pai: TILES[i % TILES.len()].to_string(),
            },
            other => other.clone(),
        })
        .collect()
}

/// Review `events` of `game` (the game's own or a planted copy) in the paid
/// module, with the censor on or off.
fn replay(game: &Game, events: &[MjaiEvent], censor_on: bool) -> Result<ReviewRun> {
    let info = model_info(&game.model, game.record.num_players)?;
    let length = game_length(game.record.kyoku_mode);
    super::run(events, game.seat, game.record.num_players, length, &info, censor_on, &mut || false)?
        .context("never stopped")
}

fn engine_for(game: &Game) -> Result<Engine> {
    let info = model_info(&game.model, game.record.num_players)?;
    native::engine_for(&info.name, game.record.num_players, game.seat)
}

fn stats(mut ms: Vec<f64>) -> LatencyStats {
    if ms.is_empty() {
        return LatencyStats::default();
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    LatencyStats {
        n: ms.len() as u32,
        mean_ms: ms.iter().sum::<f64>() / ms.len() as f64,
        p50_ms: at(0.5),
        p95_ms: at(0.95),
        max_ms: *ms.last().unwrap(),
    }
}

/// Time the calls the live bot makes at each of its decisions (the decision
/// and the danger head), over `games` in turn, up to `calls` of them.
fn time_calls(games: &[Game], calls: usize) -> Vec<f64> {
    let mut out = Vec::new();
    'games: for g in games.iter().cycle().take(games.len() * 8) {
        let Ok(mut engine) = engine_for(g) else {
            continue;
        };
        engine.reset();
        engine.set_seat(g.seat);
        engine.set_game_length(game_length(g.record.kyoku_mode));
        for ev in &g.events {
            let ev = censor(ev, g.seat).unwrap_or_else(|| ev.clone());
            if let Ok(Some(ri)) = crate::game_state::convert::to_riichienv(&ev) {
                engine.feed(ri);
            }
            let opens = match &ev {
                MjaiEvent::Tsumo { actor, .. }
                | MjaiEvent::Pon { actor, .. }
                | MjaiEvent::Chi { actor, .. } => *actor == g.seat,
                MjaiEvent::Dahai { actor, .. } | MjaiEvent::Kakan { actor, .. } => *actor != g.seat,
                _ => false,
            };
            if !opens {
                continue;
            }
            let t = Instant::now();
            let decided = engine.decide().ok().flatten().is_some();
            if decided {
                let _ = engine.danger();
                out.push(t.elapsed().as_secs_f64() * 1000.0);
                if out.len() >= calls {
                    break 'games;
                }
            }
        }
    }
    out
}

/// Run the self-check over the newest games of `history`. `scratch` is a
/// folder the review run beside the timing may write its reports to (it is
/// emptied afterwards).
pub fn run(history: &HistoryStore, scratch: &Path, opts: Options) -> Result<SelfCheckReport> {
    let mut report = SelfCheckReport {
        at_ms: now_ms(),
        ..SelfCheckReport::default()
    };
    let mut records = history.read_all()?;
    records.retain(|r| r.our_seat.is_some());
    records.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    records.truncate(opts.max_games);

    let mut games = Vec::new();
    for record in records {
        let Some(events) = history.get_events(&record.id)? else {
            continue;
        };
        let seat = record.our_seat.expect("filtered");
        let model = super::job::model_for(None, record.num_players);
        games.push(Game {
            record,
            events,
            seat,
            model,
        });
    }
    report.games = games.len() as u32;
    if games.is_empty() {
        report
            .notes
            .push("no games of the player's own to check".into());
        return Ok(report);
    }

    // The logs are the player's own view already.
    report.logs_changed = games
        .iter()
        .filter(|g| g.events.iter().any(|e| censor(e, g.seat).is_some()))
        .count() as u32;
    report.logs_clean = report.logs_changed == 0;

    // Every call and ron the logs hold, to hold the review's reading to.
    report.calls_in_logs = games
        .iter()
        .map(|g| {
            g.events
                .iter()
                .filter(|e| match e {
                    MjaiEvent::Pon { actor, .. }
                    | MjaiEvent::Chi { actor, .. }
                    | MjaiEvent::Daiminkan { actor, .. } => *actor == g.seat,
                    MjaiEvent::Hora { actor, target, .. } => *actor == g.seat && *target != g.seat,
                    _ => false,
                })
                .count() as u32
        })
        .sum();

    // Review every game once: counts, passes, calls.
    let mut reference = Vec::new();
    for g in &games {
        let run = replay(g, &g.events, true)?;
        report.decisions += run.decisions.len() as u32;
        report.call_windows += run.call_windows;
        report.passes += run.passes;
        report.calls += run.calls;
        report.unresolved += run.diagnostics.no_actual;
        report.not_ranked += run.diagnostics.not_ranked;
        reference.push(run.decisions);
    }

    // The same answer twice.
    report.deterministic = true;
    for (g, first) in games.iter().zip(&reference).take(opts.determinism_games) {
        let again = replay(g, &g.events, true)?;
        report.determinism_games += 1;
        if serde_json::to_vec(&again.decisions)? != serde_json::to_vec(first)? {
            report.deterministic = false;
        }
    }

    // Nothing leaks: plant the hidden tiles, review, compare the observations.
    for (g, first) in games.iter().zip(&reference).take(opts.leak_games) {
        let leaked = planted(&g.events, g.seat);
        let with_censor = replay(g, &leaked, true)?;
        report.leak_games += 1;
        report.leak_decisions += first.len() as u32;
        report.leak_mismatches += mismatches(first, &with_censor.decisions);
        // For information: what the leak would have changed without the censor.
        let uncensored = catch_unwind(AssertUnwindSafe(|| replay(g, &leaked, false).ok()));
        report.leak_uncensored_mismatches += match uncensored {
            Ok(Some(r)) => mismatches(first, &r.decisions),
            _ => first.len() as u32,
        };
    }

    // The model call's timing, alone and beside a running review.
    let sample: Vec<Game> = games
        .iter()
        .take(4)
        .map(|g| Game {
            record: g.record.clone(),
            events: g.events.clone(),
            seat: g.seat,
            model: g.model.clone(),
        })
        .collect();
    report.latency_alone = stats(time_calls(&sample, opts.latency_calls));

    std::fs::create_dir_all(scratch)?;
    let manager = Arc::new(ReviewManager::new());
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let manager = manager.clone();
        let stop = stop.clone();
        let reviews = ReviewStore::new(scratch);
        let todo: Vec<(GameRecord, String)> = games
            .iter()
            .map(|g| (g.record.clone(), g.model.clone()))
            .collect();
        let history_root = history.root().to_path_buf();
        std::thread::spawn(move || {
            crate::platform::lower_current_thread_priority();
            let Ok(history) = HistoryStore::new(history_root) else {
                return;
            };
            let job = Job {
                history: Arc::new(history),
                reviews,
                ids: Vec::new(),
                model: None,
                pause_in_game: false,
                live: Arc::new(|| false),
            };
            while !stop.load(Ordering::SeqCst) {
                manager.run(&job, todo.clone(), &|_| {});
            }
        })
    };
    // Let the review get going before timing.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let before = manager.events_processed();
    report.latency_with_review = stats(time_calls(&sample, opts.latency_calls));
    report.concurrent_events = manager.events_processed().saturating_sub(before) as u32;
    stop.store(true, Ordering::SeqCst);
    manager.cancel();
    let _ = worker.join();
    let _ = std::fs::remove_dir_all(scratch);

    report.notes = notes_of(&report);
    Ok(report)
}

/// How many decisions of `b` differ from `a` in what the model was shown or
/// said (a different length counts the difference too).
fn mismatches(
    a: &[crate::schema::review::ReviewDecision],
    b: &[crate::schema::review::ReviewDecision],
) -> u32 {
    let differing = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x.obs != y.obs || x.actual_prob != y.actual_prob || x.top != y.top)
        .count();
    (differing + a.len().abs_diff(b.len())) as u32
}

fn notes_of(r: &SelfCheckReport) -> Vec<String> {
    let mut n = Vec::new();
    if r.logs_clean {
        n.push(format!(
            "{} games read: no hidden tile in any log.",
            r.games
        ));
    } else {
        n.push(format!(
            "{} of {} logs held tiles the player could not see; the review blanks them.",
            r.logs_changed, r.games
        ));
    }
    n.push(format!(
        "Leak plant: {} games, {} decisions, {} observations changed with the censor on (must be 0); {} would have changed without it.",
        r.leak_games, r.leak_decisions, r.leak_mismatches, r.leak_uncensored_mismatches
    ));
    n.push(format!(
        "Call windows: {} read, {} passes, {} calls ({} in the logs), {} unresolved, {} not among the model's ranked actions.",
        r.call_windows, r.passes, r.calls, r.calls_in_logs, r.unresolved, r.not_ranked
    ));
    if r.latency_alone.p50_ms > 0.0 {
        n.push(format!(
            "Model call p50 {:.1} ms alone, {:.1} ms with a review running; p95 {:.1} → {:.1} ms; max {:.1} → {:.1} ms.",
            r.latency_alone.p50_ms,
            r.latency_with_review.p50_ms,
            r.latency_alone.p95_ms,
            r.latency_with_review.p95_ms,
            r.latency_alone.max_ms,
            r.latency_with_review.max_ms
        ));
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::testkit;

    /// On a small game the whole self-check runs through and finds what it
    /// is meant to: nothing hidden in the log, nothing changed by a plant,
    /// the same answer twice.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn the_self_check_passes_on_a_clean_game() {
        assert!(crate::pro::load_for_tests(), "set MOKORA_PRO_DLL");
        let dir = tempfile::tempdir().unwrap();
        let (history, _) = testkit::history_with_one_game(dir.path());
        let opts = Options {
            latency_calls: 4,
            ..Options::default()
        };
        let report = run(&history, &dir.path().join("scratch"), opts).unwrap();
        assert_eq!(report.games, 1);
        assert!(report.decisions > 0);
        assert!(report.logs_clean);
        assert!(report.deterministic);
        assert_eq!(report.leak_games, 1);
        assert_eq!(report.leak_mismatches, 0);
        assert!(report.call_windows >= 1);
        assert!(report.passes >= 1, "the declined pon is a pass");
        assert_eq!(report.unresolved, 0);
        assert_eq!(
            report.calls, report.calls_in_logs,
            "every call in the log is found"
        );
        assert!(report.latency_alone.n > 0 && report.latency_with_review.n > 0);
    }

    /// The plant really does put real tiles where the log hides them, and
    /// only there.
    #[test]
    fn the_plant_fills_only_hidden_seats() {
        let events = testkit::game_events();
        let leaked = planted(&events, 0);
        assert_eq!(events.len(), leaked.len());
        let differs = events.iter().zip(&leaked).filter(|(a, b)| a != b).count();
        assert!(differs >= 2);
        for (a, b) in events.iter().zip(&leaked) {
            if let (
                MjaiEvent::StartKyoku { tehais: x, .. },
                MjaiEvent::StartKyoku { tehais: y, .. },
            ) = (a, b)
            {
                assert_eq!(x[0], y[0], "the player's own hand is left alone");
                assert!(y[1].iter().all(|t| t != "?"));
            }
        }
        // …and the censor puts every one of them back.
        for (a, b) in events.iter().zip(&leaked) {
            assert_eq!(&censor(b, 0).unwrap_or_else(|| b.clone()), a);
        }
    }

    /// The self-check on the games of a real history: set `REVIEW_HISTORY_DIR`
    /// to a copy of a `history/` folder and run with `--ignored --nocapture`.
    /// Prints the report; fails if anything it checks is not as it must be.
    #[test]
    #[ignore = "needs a real history folder (REVIEW_HISTORY_DIR)"]
    fn on_real_games() {
        let Ok(dir) = std::env::var("REVIEW_HISTORY_DIR") else {
            return;
        };
        let history = HistoryStore::new(dir.into()).unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let report = run(&history, scratch.path(), Options::default()).unwrap();
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        assert!(report.games > 0);
        assert!(
            report.logs_clean,
            "{} logs held hidden tiles",
            report.logs_changed
        );
        assert_eq!(report.leak_mismatches, 0);
        assert!(report.deterministic);
        assert_eq!(report.unresolved, 0);
        assert_eq!(report.calls, report.calls_in_logs);
        assert!(
            report.concurrent_events > 0,
            "the review ran beside the timing"
        );
    }
}
