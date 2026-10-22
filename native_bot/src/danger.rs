//! Danger labels: which tiles each opponent would win on right now.
//!
//! The training logs show every hand, so at each decision the truth is known:
//! an opponent who is tenpai and not furiten would ron any of its waits. The
//! model's danger head learns to predict exactly this from what the deciding
//! player can see — the same planes the policy reads — which makes its output
//! a deal-in probability for every tile, not only the ones in hand.
//!
//! Layout: one block of `tile_dim` per opponent in relative order (shimocha
//! first, as in the encoder), `1` on each tile it waits on. Yaku is not
//! checked (a dama hand without one cannot ron), so a few labels are slightly
//! pessimistic; furiten is — own discards and the tiles it let pass, the same
//! way the encoder's furiten plane is derived (see `adapt`).

use riichienv_core::state::GameState;
use riichienv_core::state_3p::GameState3P;

use crate::tiles::{tile_dim, tile_index};
use crate::timeline::DiscardTimeline;

/// Length of one sample's labels: `tile_dim` per opponent.
pub fn label_len(num_players: u8) -> usize {
    (num_players as usize - 1) * tile_dim(num_players)
}

macro_rules! labels {
    ($state:ident, $seat:ident, $timeline:ident, $np:expr) => {{
        let np: u8 = $np;
        let t = tile_dim(np);
        let discards: Vec<&[u8]> = $state
            .players
            .iter()
            .map(|p| p.discards.as_slice())
            .collect();
        let passed = $timeline.passed_tiles(&discards);
        // Owned copies: the observations below borrow the state mutably.
        let seen: Vec<Vec<u8>> = (0..np as usize)
            .map(|s| {
                let mut v = discards[s].to_vec();
                v.extend_from_slice(&passed[s]);
                v
            })
            .collect();
        let mut out = vec![0u8; label_len(np)];
        for k in 1..np as usize {
            let opp = ($seat as usize + k) % np as usize;
            let obs = $state.get_observation(opp as u8);
            if !obs.is_tenpai {
                continue;
            }
            let furiten = seen[opp].iter().any(|tid| obs.waits.contains(&(tid / 4)));
            if furiten {
                continue;
            }
            for &w in &obs.waits {
                if let Some(x) = tile_index(w * 4, np) {
                    out[(k - 1) * t + x] = 1;
                }
            }
        }
        out
    }};
}

/// Danger labels for `seat` in a 4-player game; `timeline` synced with it.
pub fn labels_4p(state: &mut GameState, seat: u8, timeline: &DiscardTimeline) -> Vec<u8> {
    labels!(state, seat, timeline, 4)
}

/// Danger labels for `seat` in a sanma game; `timeline` synced with it.
pub fn labels_3p(state: &mut GameState3P, seat: u8, timeline: &DiscardTimeline) -> Vec<u8> {
    labels!(state, seat, timeline, 3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::sync_timeline_3p;
    use riichienv_core::replay::MjaiEvent;
    use riichienv_core::rule::GameRule;

    fn ev(line: &str) -> MjaiEvent {
        serde_json::from_str(line).expect("valid mjai event")
    }

    /// Seat 1 is tenpai on 3s (12s waiting), seat 2 is not; seat 0 asks.
    fn state() -> (GameState3P, DiscardTimeline) {
        let tenpai = r#"["1m","1m","1p","2p","3p","4p","5p","6p","7p","8p","9p","1s","2s"]"#;
        let scattered = r#"["1m","9m","1p","3p","5p","7p","9p","2s","4s","6s","E","S","W"]"#;
        let ours = r#"["9m","9m","2p","2p","4p","4p","6p","6p","8p","8p","3s","5s","7s"]"#;
        let mut state = GameState3P::new(0, true, None, 0, GameRule::default_tenhou());
        let mut timeline = DiscardTimeline::new();
        for line in [
            r#"{"type":"start_game","names":["a","b","c"]}"#.to_string(),
            format!(
                r#"{{"type":"start_kyoku","bakaze":"E","dora_marker":"1s","kyoku":1,"honba":0,
                     "kyotaku":0,"oya":0,"scores":[35000,35000,35000],
                     "tehais":[{ours},{tenpai},{scattered}]}}"#
            ),
        ] {
            let e = ev(&line);
            timeline.note_mjai(&e);
            state.apply_mjai_event(e);
            sync_timeline_3p(&mut timeline, &state);
        }
        (state, timeline)
    }

    #[test]
    fn a_tenpai_opponent_marks_its_waits() {
        let (mut state, timeline) = state();
        let labels = labels_3p(&mut state, 0, &timeline);
        let t = tile_dim(3);
        assert_eq!(labels.len(), label_len(3));
        let three_sou = tile_index(80, 3).unwrap(); // 3s = tile34 20
        let lit: Vec<usize> = (0..t).filter(|&x| labels[x] == 1).collect();
        assert_eq!(lit, vec![three_sou], "shimocha (seat 1) waits on 3s");
        assert!(labels[t..].iter().all(|&v| v == 0), "seat 2 is not tenpai");
    }

    #[test]
    fn seen_from_another_seat_the_blocks_rotate() {
        let (mut state, timeline) = state();
        // Asked by seat 2, seat 1 is its kamicha: the second block.
        let labels = labels_3p(&mut state, 2, &timeline);
        let t = tile_dim(3);
        assert!(labels[..t].iter().all(|&v| v == 0), "seat 0 is not tenpai");
        assert_eq!(labels[t..].iter().filter(|&&v| v == 1).count(), 1);
    }

    #[test]
    fn a_furiten_opponent_is_safe() {
        let (mut state, mut timeline) = state();
        // Seat 1 discards a 3s itself: its own wait in its pond.
        for line in [
            r#"{"type":"tsumo","actor":0,"pai":"N"}"#,
            r#"{"type":"dahai","actor":0,"pai":"N","tsumogiri":true}"#,
            r#"{"type":"tsumo","actor":1,"pai":"3s"}"#,
            r#"{"type":"dahai","actor":1,"pai":"3s","tsumogiri":true}"#,
        ] {
            let e = ev(line);
            timeline.note_mjai(&e);
            state.apply_mjai_event(e);
            sync_timeline_3p(&mut timeline, &state);
        }
        let labels = labels_3p(&mut state, 0, &timeline);
        assert!(labels.iter().all(|&v| v == 0));
    }
}
