//! 對局覆盤: replay a recorded game through a built-in model and keep, for
//! every decision the recorded player had, the model's ranking next to what
//! was played.
//!
//! This is analysis only. A review is read-only: it never changes a model's
//! weights, and nothing that decides a live move reads it. What it says is
//! how far the play was from that model's own policy ("AI 一致度") — not a win
//! rate, an expected value, or a verdict on right and wrong. To make a model
//! stronger is a job for training, validated in the arena.
//!
//! ```text
//! history/games/<id>.mjai.jsonl ──▶ replay ──▶ history/reviews/<id>.<model>.json
//!                                                          └ <id>.<model>.sum.json
//! ```
//!
//! - the replay itself — the game fed through the engine from the player's
//!   seat, the decision points, what was played — is a paid feature, in the
//!   module Mokora downloads once a key is redeemed (`pro_api::method::
//!   REVIEW_RUN`, through [`crate::pro::review`]); [`run`] hands it the
//!   game and the model's weights, [`review_game`] makes the report of it;
//! - [`store`] — the saved reviews: a report and a small summary per game and
//!   model, current only while their key (version, model, weights, seat, game
//!   length) matches;
//! - [`job`] — the background queue: one game at a time on a low-priority
//!   thread, paused while a live game is on, skipping games already reviewed;
//! - [`selfcheck`] — the checks run on the player's real games: nothing leaks,
//!   two runs agree, every pass is found, and the model call's timing alone
//!   and beside a running review.

pub mod job;
pub mod selfcheck;
pub mod store;

use crate::bot::native;
use crate::schema::review::{ReviewModelInfo, ReviewReport, ReviewRequest, ReviewRun, ReviewSettings};
use crate::schema::{GameRecord, KyokuMode, MjaiEvent};
use anyhow::{Context, Result};
use native_bot::obs::GameLength;

/// Bumped whenever the review's logic or format changes: every saved review of
/// an older version is stale.
pub const REVIEW_VERSION: u32 = 1;

/// The game's planned length as the live bot reads it from the room.
pub fn game_length(mode: KyokuMode) -> GameLength {
    match mode {
        KyokuMode::EastOnly => GameLength::East,
        KyokuMode::EastSouth | KyokuMode::Other => GameLength::EastSouth,
    }
}

fn length_name(length: GameLength) -> &'static str {
    match length {
        GameLength::East => "east",
        GameLength::EastSouth => "hanchan",
    }
}

/// Name and weights of a review model, for a game of `num_players`.
pub fn model_info(name: &str, num_players: u8) -> Result<ReviewModelInfo> {
    let name = native::canonical_name(name);
    anyhow::ensure!(
        native::review_models()
            .iter()
            .any(|(n, p)| *n == name && *p == num_players),
        "{name} cannot review a {num_players}-player game"
    );
    Ok(ReviewModelInfo {
        name: name.to_string(),
        num_players,
        weights_hash: native::weights_hash(name).context("unknown model")?,
    })
}

/// What makes a saved review current for this game and model.
pub fn review_key(model: &ReviewModelInfo, seat: u8, length: GameLength) -> String {
    format!(
        "v{REVIEW_VERSION}|{}|{}|s{seat}|{}",
        model.name,
        model.weights_hash,
        length_name(length)
    )
}

/// The key a review of `record` with `model` has now; `None` for an observer
/// game (no seat of the player's) or a model that cannot review it.
pub fn current_key(record: &GameRecord, model: &str) -> Option<String> {
    let seat = record.our_seat?;
    let info = model_info(model, record.num_players).ok()?;
    Some(review_key(&info, seat, game_length(record.kyoku_mode)))
}

/// `ev` with every tile the seat `seat` could not see blanked, or `None` when
/// it held no such tile (the usual case: the log is the player's own view).
/// The review blanks the same itself; the self-check uses this to find logs
/// that are not the player's view and to time the live calls.
pub fn censor(ev: &MjaiEvent, seat: u8) -> Option<MjaiEvent> {
    match ev {
        MjaiEvent::StartKyoku { tehais, .. }
            if tehais
                .iter()
                .enumerate()
                .any(|(i, hand)| i != usize::from(seat) && hand.iter().any(|t| t != "?")) =>
        {
            let mut out = ev.clone();
            if let MjaiEvent::StartKyoku { tehais, .. } = &mut out {
                for (i, hand) in tehais.iter_mut().enumerate() {
                    if i != usize::from(seat) {
                        for t in hand.iter_mut() {
                            *t = "?".to_string();
                        }
                    }
                }
            }
            Some(out)
        }
        MjaiEvent::Tsumo { actor, pai } if *actor != seat && pai != "?" => Some(MjaiEvent::Tsumo {
            actor: *actor,
            pai: "?".to_string(),
        }),
        _ => None,
    }
}

/// Replay `events` through `model` from `seat` in the paid module.
/// `censor_on` is the review's own setting; only the self-check turns it off,
/// to see what a leak would have changed. `stop` is asked between events;
/// `Ok(None)` when it said so.
pub fn run(
    events: &[MjaiEvent],
    seat: u8,
    num_players: u8,
    length: GameLength,
    model: &ReviewModelInfo,
    censor_on: bool,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<ReviewRun>> {
    let weights = native::weights_of(&model.name).context("unknown model")?;
    let req = ReviewRequest {
        events: events
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<_, _>>()?,
        seat,
        num_players,
        length: length_name(length).to_string(),
        censor: censor_on,
        weights_hash: model.weights_hash.clone(),
    };
    crate::pro::review(&req, weights, stop)
}

/// Review one game with `model`. `Ok(None)` when `stop` asked to.
pub fn review_game(
    record: &GameRecord,
    events: &[MjaiEvent],
    model: &str,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<ReviewReport>> {
    let seat = record
        .our_seat
        .context("an observed game has no seat to review")?;
    let info = model_info(model, record.num_players)?;
    let length = game_length(record.kyoku_mode);
    let Some(run) = run(events, seat, record.num_players, length, &info, true, stop)? else {
        return Ok(None);
    };
    Ok(Some(ReviewReport {
        key: review_key(&info, seat, length),
        game_id: record.id.clone(),
        game_hash: native::fingerprint(&serde_json::to_vec(events)?),
        created_ms: now_ms(),
        model: info,
        live_bot: record.bot.clone(),
        our_seat: seat,
        num_players: record.num_players,
        settings: ReviewSettings {
            version: REVIEW_VERSION,
            top_k: run.top_k,
            game_length: length_name(length).to_string(),
            deterministic: true,
        },
        decisions: run.decisions,
        kinds: run.kinds,
        diagnostics: run.diagnostics,
    }))
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::review::ReviewKind;

    // These run the review in the paid module: `MOKORA_PRO_DLL=<a built
    // mokora_pro library> cargo test -- --ignored`.

    fn reviewed(model: &str) -> (GameRecord, Vec<MjaiEvent>, ReviewReport) {
        assert!(crate::pro::load_for_tests(), "set MOKORA_PRO_DLL");
        let dir = tempfile::tempdir().unwrap();
        let (history, id) = testkit::history_with_one_game(dir.path());
        let record = history.get(&id).unwrap().unwrap();
        let events = history.get_events(&id).unwrap().unwrap();
        let report = review_game(&record, &events, model, &mut || false)
            .unwrap()
            .unwrap();
        (record, events, report)
    }

    /// The small game reviewed end to end: the player's turns are read, and
    /// the pon they did not take is a pass the model had ranked against it.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn a_game_is_reviewed_turn_by_turn_with_its_pass() {
        let (_, _, report) = reviewed(native::MORA_4P);
        assert_eq!(report.model.name, native::MORA_4P);
        assert!(report.settings.deterministic);
        assert_eq!(
            report.settings.game_length, "east",
            "a game of one east round is east-only"
        );

        let turn = report
            .decisions
            .iter()
            .find(|d| d.idx == 2)
            .expect("own draw");
        assert!(!turn.window);
        assert_eq!(turn.actual.kind, ReviewKind::Discard);
        assert_eq!(turn.actual.pai.as_deref(), Some("3p"));
        assert!(turn.actual_prob.is_some());

        let window = report
            .decisions
            .iter()
            .find(|d| d.idx == 5)
            .expect("the pon window");
        assert!(window.window);
        assert_eq!(
            window.actual.kind,
            ReviewKind::Pass,
            "no call by the player is a pass"
        );
        assert!(
            window.actual_prob.is_some(),
            "the pass is ranked against the pon"
        );
        let kinds: Vec<ReviewKind> = window.top.iter().map(|o| o.action.kind).collect();
        assert!(
            kinds.contains(&ReviewKind::Pass) && kinds.contains(&ReviewKind::Pon),
            "{kinds:?}"
        );
        assert_eq!(
            window.matched,
            window.top[0].action.kind == ReviewKind::Pass
        );

        // Statistics are per kind: discards and the pass are counted apart.
        let of = |k| report.kinds.iter().find(|s| s.kind == k);
        assert!(of(ReviewKind::Discard).is_some() && of(ReviewKind::Pass).is_some());
        assert_eq!(report.diagnostics.no_actual, 0);
        assert_eq!(report.diagnostics.not_ranked, 0);
    }

    /// What the model was shown at each decision is exactly what the live bot
    /// is shown: the live path feeds the events up to the decision at once and
    /// asks for the top few; the review feeds them one at a time and asks for
    /// all. The final observation (and the answer) must be the same.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn the_review_sees_what_the_live_bot_sees() {
        let (record, events, report) = reviewed(native::MORA_4P);
        let seat = record.our_seat.unwrap();
        assert!(!report.decisions.is_empty());
        for d in &report.decisions {
            let mut engine = native::engine_for(native::MORA_4P, 4, seat).unwrap();
            engine.set_game_length(game_length(record.kyoku_mode));
            for ev in &events[..=d.idx as usize] {
                let ev = censor(ev, seat).unwrap_or_else(|| ev.clone());
                if let Some(ri) = crate::game_state::convert::to_riichienv(&ev).unwrap() {
                    engine.feed(ri);
                }
            }
            let live = engine
                .decide()
                .unwrap()
                .expect("the live bot decides here too");
            assert_eq!(
                format!("{:016x}", live.obs_digest),
                d.obs,
                "observation at {}",
                d.idx
            );
            assert_eq!(live.candidates.len().min(3), d.top.len());
            assert!((live.candidates[0].1 - d.top[0].prob).abs() < 1e-6);
        }
    }

    /// Two reviews of the same game are the same, and a model that cannot
    /// play the game's player count is refused. (The censor's own tests and
    /// the replay's are MokoraPro's.)
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn a_review_is_repeatable_and_checks_the_model() {
        let (record, events, first) = reviewed(native::MORA_4P);
        let again = review_game(&record, &events, native::MORA_4P, &mut || false)
            .unwrap()
            .unwrap();
        assert_eq!(first.decisions, again.decisions);
        assert_eq!(first.key, again.key);
        assert!(review_game(&record, &events, native::MORA_3P, &mut || false).is_err());
        assert!(review_game(&record, &events, native::YAKUMAN_3P, &mut || false).is_err());
        // A stop request ends it without a report.
        assert!(
            review_game(&record, &events, native::MORA_4P, &mut || true)
                .unwrap()
                .is_none()
        );
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A small 4-player game of the recorded player (seat 0), written the way
    //! the bridge logs it: the other seats' hands and draws are `?`. The
    //! player passes a pon of the 9s they hold a pair of.

    use crate::history::aggregator::{aggregate, AggregateInput};
    use crate::history::HistoryStore;
    use crate::schema::{MjaiEvent, Platform};
    use std::path::Path;
    use std::sync::Arc;

    fn tsumo(actor: u8, pai: &str) -> MjaiEvent {
        MjaiEvent::Tsumo {
            actor,
            pai: pai.into(),
        }
    }

    fn dahai(actor: u8, pai: &str, tsumogiri: bool) -> MjaiEvent {
        MjaiEvent::Dahai {
            actor,
            pai: pai.into(),
            tsumogiri,
        }
    }

    pub fn game_events() -> Vec<MjaiEvent> {
        let hand = [
            "9s", "9s", "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m", "1p", "2p",
        ];
        let hidden = vec!["?".to_string(); 13];
        vec![
            MjaiEvent::StartGame {
                names: vec!["A".into(), "B".into(), "C".into(), "D".into()],
                kyoku_first: Some(0),
                aka_flag: Some(true),
                id: Some(0),
                num_players: 4,
                game_meta: None,
            },
            MjaiEvent::StartKyoku {
                bakaze: "E".into(),
                dora_marker: "2m".into(),
                kyoku: 1,
                honba: 0,
                kyotaku: 0,
                oya: 0,
                scores: vec![25000; 4],
                tehais: vec![
                    hand.iter().map(|s| s.to_string()).collect(),
                    hidden.clone(),
                    hidden.clone(),
                    hidden,
                ],
                num_players: 4,
            },
            tsumo(0, "3p"),
            dahai(0, "3p", true),
            tsumo(1, "?"),
            dahai(1, "9s", false),
            tsumo(2, "?"),
            dahai(2, "E", false),
            tsumo(3, "?"),
            dahai(3, "C", false),
            tsumo(0, "4p"),
            dahai(0, "4p", true),
            MjaiEvent::Ryukyoku { deltas: None },
            MjaiEvent::EndKyoku,
            MjaiEvent::confirmed_game(Some(vec![25000; 4]), Some(vec![1, 2, 3, 4])),
        ]
    }

    /// A saved-review stand-in for `record` (no decisions), for the tests
    /// that only need one to exist; real ones come from the paid module.
    pub fn report_of(record: &crate::schema::GameRecord) -> crate::schema::review::ReviewReport {
        use crate::schema::review::*;
        let model = super::model_info(crate::bot::native::MORA_4P, record.num_players).unwrap();
        let seat = record.our_seat.unwrap_or(0);
        let length = super::game_length(record.kyoku_mode);
        ReviewReport {
            key: super::review_key(&model, seat, length),
            game_id: record.id.clone(),
            game_hash: String::new(),
            created_ms: 0,
            model,
            live_bot: record.bot.clone(),
            our_seat: seat,
            num_players: record.num_players,
            settings: ReviewSettings {
                version: super::REVIEW_VERSION,
                top_k: 3,
                game_length: "east".into(),
                deterministic: true,
            },
            decisions: vec![],
            kinds: vec![],
            diagnostics: ReviewDiagnostics::default(),
        }
    }

    /// A history under `dir` holding [`game_events`] as one recorded game.
    pub fn history_with_one_game(dir: &Path) -> (Arc<HistoryStore>, String) {
        let store = HistoryStore::new(dir.to_path_buf()).unwrap();
        let events = game_events();
        let now = chrono::Utc::now();
        let id = "01TESTGAME0000000000000000".to_string();
        let record = aggregate(AggregateInput {
            events: &events,
            platform: Platform::Majsoul,
            started_at: now,
            ended_at: now,
            id: id.clone(),
        })
        .expect("the game aggregates");
        store.append(&record, &events).unwrap();
        (Arc::new(store), id)
    }
}

#[cfg(test)]
mod censor_tests {
    use super::*;

    fn tsumo(actor: u8, pai: &str) -> MjaiEvent {
        MjaiEvent::Tsumo {
            actor,
            pai: pai.into(),
        }
    }

    /// The censor leaves a log that is the player's own view alone, and
    /// blanks what a seat other than theirs holds or draws.
    #[test]
    fn the_censor_blanks_only_what_the_player_could_not_see() {
        let events = testkit::game_events();
        assert!(events.iter().all(|e| censor(e, 0).is_none()), "the player's own view");
        assert_eq!(censor(&tsumo(1, "5m"), 0), Some(tsumo(1, "?")));
        let mut leaky = events[1].clone();
        if let MjaiEvent::StartKyoku { tehais, .. } = &mut leaky {
            tehais[2] = vec!["1s".to_string(); 13];
        }
        assert_eq!(censor(&leaky, 0).expect("a leaked hand is blanked"), events[1]);
    }
}
