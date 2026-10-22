//! Convert Mokora's `schema::MjaiEvent` into `riichienv_core::replay::MjaiEvent`.
//!
//! Both enums are `serde(tag = "type")` with the mjai protocol's
//! snake-case discriminants, so a JSON round-trip handles 14 of the 15
//! variants directly. Two cases need care:
//!
//! - **`StartGame.id`** — Mokora types it as `Option<u8>` (seat number);
//!   riichienv types it as `Option<String>`. We patch the JSON before
//!   deserialization.
//! - **`MjaiEvent::None`** — Mokora-only sentinel for a bot's "no action"
//!   reply. Not a protocol event; we return `Ok(None)` so the tracker
//!   skips it without producing an error.
//!
//! Choosing JSON-roundtrip over a hand-written `From` keeps this file
//! ~30 lines. Cost is one `serde_json::to_value` + one
//! `serde_json::from_value` per event — negligible against a hand of
//! mahjong (a few thousand events per game).

use crate::schema::MjaiEvent as MokoraEvent;
use anyhow::{Context, Result};
use riichienv_core::replay::MjaiEvent as RiEvent;
use serde_json::Value;

/// Returns `Ok(None)` for `MjaiEvent::None` (intentional skip), `Ok(Some(_))`
/// for any protocol event, `Err` for malformed input.
pub fn to_riichienv(ev: &MokoraEvent) -> Result<Option<RiEvent>> {
    if matches!(ev, MokoraEvent::None) {
        return Ok(None);
    }

    let mut v = serde_json::to_value(ev).context("serialize mokora mjai event")?;

    // Patch StartGame.id: Mokora `u8` → riichienv `String`.
    if v.get("type").and_then(Value::as_str) == Some("start_game") {
        if let Some(id) = v.get_mut("id") {
            if let Some(n) = id.as_u64() {
                *id = Value::String(n.to_string());
            }
        }
    }

    let ri: RiEvent = serde_json::from_value(v).context("deserialize as riichienv mjai event")?;
    Ok(Some(ri))
}

/// Whether `ev` fits a `num_players` game, checked before it reaches the
/// engine.
///
/// riichienv-core indexes its seat arrays with an event's seats and copies
/// `start_kyoku`'s `scores` / `tehais` by position, with no bounds checks:
/// a seat past the table or a list of the wrong length panics. Such an event
/// is not a game state to apply but a sign that two games' events got mixed
/// (several flows publish onto one bus — a reconnect, a spectated table), so
/// it is refused rather than applied.
pub fn check_fits(ev: &MokoraEvent, num_players: u8) -> Result<()> {
    let seat = |name: &str, s: u8| -> Result<()> {
        anyhow::ensure!(
            s < num_players,
            "{name} {s} is not a seat in a {num_players}-player game"
        );
        Ok(())
    };
    let len = |name: &str, n: usize| -> Result<()> {
        anyhow::ensure!(
            n == usize::from(num_players),
            "{name} has {n} entries for a {num_players}-player game"
        );
        Ok(())
    };
    match ev {
        MokoraEvent::StartKyoku {
            oya,
            scores,
            tehais,
            num_players: n,
            ..
        } => {
            anyhow::ensure!(
                *n == num_players,
                "start_kyoku for {n} players in a {num_players}-player game"
            );
            seat("oya", *oya)?;
            len("scores", scores.len())?;
            len("tehais", tehais.len())?;
        }
        MokoraEvent::Tsumo { actor, .. }
        | MokoraEvent::Dahai { actor, .. }
        | MokoraEvent::Kakan { actor, .. }
        | MokoraEvent::Ankan { actor, .. }
        | MokoraEvent::Reach { actor, .. }
        | MokoraEvent::ReachAccepted { actor }
        | MokoraEvent::Kita { actor, .. } => seat("actor", *actor)?,
        MokoraEvent::Chi { actor, target, .. }
        | MokoraEvent::Pon { actor, target, .. }
        | MokoraEvent::Daiminkan { actor, target, .. } => {
            seat("actor", *actor)?;
            seat("target", *target)?;
        }
        MokoraEvent::Hora {
            actor,
            target,
            deltas,
            ..
        } => {
            seat("actor", *actor)?;
            seat("target", *target)?;
            if let Some(d) = deltas {
                len("deltas", d.len())?;
            }
        }
        MokoraEvent::Ryukyoku { deltas: Some(d) } => len("deltas", d.len())?,
        MokoraEvent::StartGame { .. }
        | MokoraEvent::Ryukyoku { deltas: None }
        | MokoraEvent::Dora { .. }
        | MokoraEvent::EndKyoku
        | MokoraEvent::EndGame { .. }
        | MokoraEvent::None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_kyoku(scores: usize, tehais: usize, num_players: u8, oya: u8) -> MokoraEvent {
        MokoraEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "2m".into(),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya,
            scores: vec![25000; scores],
            tehais: vec![vec!["1m".to_string(); 13]; tehais],
            num_players,
        }
    }

    #[test]
    fn events_that_fit_the_table_pass() {
        assert!(check_fits(&start_kyoku(4, 4, 4, 3), 4).is_ok());
        assert!(check_fits(&start_kyoku(3, 3, 3, 2), 3).is_ok());
        assert!(check_fits(&dahai(2), 3).is_ok());
        assert!(check_fits(&MokoraEvent::None, 3).is_ok());
    }

    /// The two cases that panicked the engine in a probe: a seat past a
    /// sanma table, and a three-score start_kyoku fed to a 4-player game.
    #[test]
    fn events_that_would_panic_the_engine_are_refused() {
        assert!(check_fits(&dahai(3), 3).is_err());
        assert!(check_fits(&start_kyoku(3, 3, 3, 0), 4).is_err());
        assert!(check_fits(&start_kyoku(4, 3, 4, 0), 4).is_err(), "short tehais");
        assert!(check_fits(&start_kyoku(4, 4, 4, 4), 4).is_err(), "oya off the table");
        let pon = MokoraEvent::Pon {
            actor: 0,
            target: 3,
            pai: "1m".into(),
            consumed: ["1m".into(), "1m".into()],
        };
        assert!(check_fits(&pon, 3).is_err(), "target off the table");
        let hora = MokoraEvent::Hora {
            actor: 0,
            target: 1,
            deltas: Some(vec![0, 0, 0]),
            ura_markers: None,
        };
        assert!(check_fits(&hora, 4).is_err(), "deltas of the wrong length");
    }

    fn dahai(actor: u8) -> MokoraEvent {
        MokoraEvent::Dahai {
            actor,
            pai: "1m".into(),
            tsumogiri: false,
        }
    }

    #[test]
    fn none_returns_none() {
        assert!(to_riichienv(&MokoraEvent::None).unwrap().is_none());
    }

    #[test]
    fn dahai_round_trips() {
        let out = to_riichienv(&dahai(2)).unwrap().unwrap();
        match out {
            RiEvent::Dahai {
                actor,
                pai,
                tsumogiri,
            } => {
                assert_eq!(actor, 2);
                assert_eq!(pai, "1m");
                assert!(!tsumogiri);
            }
            other => panic!("expected Dahai, got {other:?}"),
        }
    }

    #[test]
    fn start_game_id_u8_becomes_string() {
        let ev = MokoraEvent::StartGame {
            names: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            kyoku_first: None,
            aka_flag: None,
            id: Some(2),
            num_players: 4,
            game_meta: None,
        };
        let out = to_riichienv(&ev).unwrap().unwrap();
        match out {
            RiEvent::StartGame { id, .. } => {
                assert_eq!(id.as_deref(), Some("2"));
            }
            other => panic!("expected StartGame, got {other:?}"),
        }
    }

    #[test]
    fn start_game_no_id_works() {
        let ev = MokoraEvent::StartGame {
            names: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            kyoku_first: None,
            aka_flag: None,
            id: None,
            num_players: 4,
            game_meta: None,
        };
        let out = to_riichienv(&ev).unwrap().unwrap();
        assert!(matches!(out, RiEvent::StartGame { id: None, .. }));
    }

    #[test]
    fn daiminkan_maps_to_kan_via_alias() {
        // Mokora serializes the variant name (snake_case) → "daiminkan".
        // riichienv accepts that as an alias for its `Kan` variant.
        let ev = MokoraEvent::Daiminkan {
            actor: 0,
            target: 2,
            pai: "5m".into(),
            consumed: ["5m".into(), "5m".into(), "5mr".into()],
        };
        let out = to_riichienv(&ev).unwrap().unwrap();
        match out {
            RiEvent::Kan {
                actor,
                target,
                pai,
                consumed,
            } => {
                assert_eq!(actor, 0);
                assert_eq!(target, 2);
                assert_eq!(pai, "5m");
                assert_eq!(consumed, vec!["5m", "5m", "5mr"]);
            }
            other => panic!("expected Kan, got {other:?}"),
        }
    }

    #[test]
    fn start_kyoku_round_trips() {
        let one_hand: Vec<String> = (0..13).map(|i| format!("{}m", (i % 9) + 1)).collect();
        let ev = MokoraEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "2m".into(),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: vec![25000, 25000, 25000, 25000],
            tehais: vec![
                one_hand.clone(),
                one_hand.clone(),
                one_hand.clone(),
                one_hand,
            ],
            num_players: 4,
        };
        let out = to_riichienv(&ev).unwrap().unwrap();
        match out {
            RiEvent::StartKyoku {
                bakaze,
                kyoku,
                honba,
                kyoutaku,
                oya,
                scores,
                ..
            } => {
                assert_eq!(bakaze, "E");
                assert_eq!(kyoku, 1);
                assert_eq!(honba, 0);
                assert_eq!(kyoutaku, 0);
                assert_eq!(oya, 0);
                assert_eq!(scores, vec![25000, 25000, 25000, 25000]);
            }
            other => panic!("expected StartKyoku, got {other:?}"),
        }
    }

    #[test]
    fn end_game_works() {
        let out = to_riichienv(&MokoraEvent::end_game()).unwrap().unwrap();
        assert!(matches!(out, RiEvent::EndGame));
    }

    #[test]
    fn hora_with_deltas_round_trips() {
        let ev = MokoraEvent::Hora {
            actor: 2,
            target: 3,
            deltas: Some(vec![0, 0, 2300, -1300]),
            ura_markers: Some(vec!["C".into()]),
        };
        let out = to_riichienv(&ev).unwrap().unwrap();
        match out {
            RiEvent::Hora {
                actor,
                target,
                delta,
                uradora_markers,
                ..
            } => {
                assert_eq!(actor, 2);
                assert_eq!(target, 3);
                assert_eq!(delta, Some(vec![0, 0, 2300, -1300]));
                assert_eq!(uradora_markers, Some(vec!["C".into()]));
            }
            other => panic!("expected Hora, got {other:?}"),
        }
    }
}
