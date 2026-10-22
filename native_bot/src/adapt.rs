//! Adapters that turn a riichienv-core observation into our [`EncInput`].
//!
//! Used identically by the extractor and by live inference, which is what
//! guarantees train/inference feature parity.
//!
//! ## `last_discard` is read from the state, not the observation
//!
//! `GameState::last_discard` (and its sanma twin) is a `(discarder_pid, tile)`
//! pair, but riichienv-core 0.4.8's `get_observation` destructures it as
//! `(tile, _pid)` — so `Observation::last_discard` actually carries the
//! *discarder's seat*, not the tile. Feeding that into the encoder collapses the
//! "last discard" plane onto tile34 0 (seats 0..3 are tile ids 0..3, all `1m`)
//! and the model never sees which tile it is being asked to call. We therefore
//! read the tile straight off the owning state and pass it in explicitly, the
//! same way `turn_count` and the sanma kita counts are already handled.
//!
//! ## Feature sets
//!
//! `obs_and_legal_*` encode [`Features::V1`]. The `_with` variants take the
//! set to encode and the round's [`DiscardTimeline`], which every caller keeps
//! in step with its state through [`sync_timeline_4p`] / [`sync_timeline_3p`]
//! after each mutation (and, where the state is driven by mjai events,
//! [`DiscardTimeline::note_mjai`] before each one); [`Features::V2`] reads the
//! claimed tiles from the state and the tedashi flags and passed tiles from
//! the timeline. Furiten is derived from those too — a wait among our own
//! discards or the tiles we let pass — because the state tracks the
//! passed-win flags only when the engine steps the game itself.
//! [`Features::V3`] adds the rounds the game has left, from the round on the
//! state and the game's planned [`GameLength`], which the caller knows.

use riichienv_core::action::Action;
use riichienv_core::observation::Observation;
use riichienv_core::observation_3p::Observation3P;
use riichienv_core::state::GameState;
use riichienv_core::state_3p::GameState3P;

use crate::obs::{EncInput, Features, GameLength, SeatFeat};
use crate::timeline::{DiscardFlags, DiscardTimeline};

/// Fill the [`Features::V2`] inputs of `$inp` (seats already in relative
/// order) from the owning state — the 4p and 3p player states have the same
/// fields but no common trait.
macro_rules! fill_v2 {
    ($inp:ident, $state:ident, $seat:ident, $timeline:ident, $np:expr) => {{
        let pid = $seat as usize;
        let discards: Vec<&[u8]> = $state
            .players
            .iter()
            .map(|p| p.discards.as_slice())
            .collect();
        let mut passed = $timeline.passed_tiles(&discards);
        let is_wait = |tid: &u8| $inp.waits.contains(&(tid / 4));
        $inp.self_furiten = discards[pid].iter().any(is_wait) || passed[pid].iter().any(is_wait);
        for (k, feat) in $inp.seats.iter_mut().enumerate() {
            let rel = (pid + k) % $np;
            let player = &$state.players[rel];
            feat.tedashi = (0..player.discards.len())
                .map(|i| $timeline.flags(rel, i).tedashi)
                .collect();
            feat.called_tiles = player.melds.iter().filter_map(|m| m.called_tile).collect();
            feat.meld_count = player.melds.len() as u8;
            feat.passed = std::mem::take(&mut passed[rel]);
        }
    }};
}

/// Snapshot `(encoded obs, legal actions)` for `seat` from a 4-player engine.
/// Shared by the extractor and live inference so features always match.
pub fn obs_and_legal_4p(state: &mut GameState, seat: u8) -> (Vec<f32>, Vec<Action>) {
    obs_and_legal_4p_with(
        state,
        seat,
        Features::V1,
        &DiscardTimeline::new(),
        GameLength::default(),
    )
}

/// [`obs_and_legal_4p`] encoding `features`; `timeline` must be synced with
/// `state` (it is only read from [`Features::V2`] on), and `length` is the
/// game's planned length (read by [`Features::V3`]).
pub fn obs_and_legal_4p_with(
    state: &mut GameState,
    seat: u8,
    features: Features,
    timeline: &DiscardTimeline,
    length: GameLength,
) -> (Vec<f32>, Vec<Action>) {
    let turn = state.turn_count;
    // `(discarder_pid, tile)` — see the module docs on why this can't come from
    // `Observation::last_discard`.
    let last_discard = state.last_discard.map(|(_pid, tile)| tile);
    let obs = state.get_observation(seat);
    let legal = obs.legal_actions_method();
    let mut inp = enc_input_4p(&obs, turn, last_discard);
    if features.has_v2() {
        fill_v2!(inp, state, seat, timeline, 4);
    }
    inp.rounds_left = length.rounds_left(4, inp.round_wind, inp.kyoku_index);
    (inp.encode_as(features), legal)
}

/// Snapshot `(encoded obs, legal actions)` for `seat` from a 3-player engine.
/// The `Action3P` legal set is unwrapped to plain `Action`s (their inner value).
pub fn obs_and_legal_3p(state: &mut GameState3P, seat: u8) -> (Vec<f32>, Vec<Action>) {
    obs_and_legal_3p_with(
        state,
        seat,
        Features::V1,
        &DiscardTimeline::new(),
        GameLength::default(),
    )
}

/// [`obs_and_legal_3p`] encoding `features`; `timeline` and `length` as for
/// [`obs_and_legal_4p_with`].
pub fn obs_and_legal_3p_with(
    state: &mut GameState3P,
    seat: u8,
    features: Features,
    timeline: &DiscardTimeline,
    length: GameLength,
) -> (Vec<f32>, Vec<Action>) {
    let turn = state.turn_count;
    let kita: [u8; 3] = std::array::from_fn(|i| state.players[i].kita_tiles.len() as u8);
    let last_discard = state.last_discard.map(|(_pid, tile)| tile);
    let obs = state.get_observation(seat);
    let legal: Vec<Action> = obs
        .legal_actions_method()
        .into_iter()
        .map(|a| a.0)
        .collect();
    let mut inp = enc_input_3p(&obs, turn, kita, last_discard);
    if features.has_v2() {
        fill_v2!(inp, state, seat, timeline, 3);
    }
    inp.rounds_left = length.rounds_left(3, inp.round_wind, inp.kyoku_index);
    (inp.encode_as(features), legal)
}

/// Number the discards that appeared since the last call; call after every
/// change to `state`. The flags come from the state where it records them
/// (an engine-stepped game), else from the noted mjai events.
pub fn sync_timeline_4p(timeline: &mut DiscardTimeline, state: &GameState) {
    let lens: Vec<usize> = state.players.iter().map(|p| p.discards.len()).collect();
    timeline.sync(&lens, |seat, i| {
        let p = &state.players[seat];
        p.discard_from_hand.get(i).map(|&tedashi| DiscardFlags {
            tedashi,
            riichi: p.riichi_declaration_index == Some(i),
        })
    });
}

/// [`sync_timeline_4p`] for a 3-player engine.
pub fn sync_timeline_3p(timeline: &mut DiscardTimeline, state: &GameState3P) {
    let lens: Vec<usize> = state.players.iter().map(|p| p.discards.len()).collect();
    timeline.sync(&lens, |seat, i| {
        let p = &state.players[seat];
        p.discard_from_hand.get(i).map(|&tedashi| DiscardFlags {
            tedashi,
            riichi: p.riichi_declaration_index == Some(i),
        })
    });
}

/// Build an [`EncInput`] from a 4-player observation.
///
/// `turn_count` and `last_discard` are read from the owning `GameState` (the
/// former is absent from the observation, the latter is mislabeled there — see
/// the module docs). Seats are placed in relative order (index 0 = deciding
/// player).
pub fn enc_input_4p(obs: &Observation, turn_count: u32, last_discard: Option<u8>) -> EncInput {
    let pid = obs.player_id as usize;
    let oya = obs.oya as usize;

    let seats: Vec<SeatFeat> = (0..4)
        .map(|k| {
            let rel = (pid + k) % 4;
            SeatFeat {
                discards: obs.discards[rel].iter().map(|&t| t as u8).collect(),
                meld_tiles: obs.melds[rel]
                    .iter()
                    .flat_map(|m| m.tiles.iter().copied())
                    .collect(),
                riichi_declared: obs.riichi_declared[rel],
                riichi_tile: obs.riichi_sutehais[rel],
                score: obs.scores[rel],
                kita_count: 0,
                ..Default::default()
            }
        })
        .collect();

    EncInput {
        num_players: 4,
        hand: obs.hands[pid].iter().map(|&t| t as u8).collect(),
        drawn_tile: obs.drawn_tile,
        waits: obs.waits.clone(),
        is_tenpai: obs.is_tenpai,
        dora_indicators: obs.dora_indicators.iter().map(|&t| t as u8).collect(),
        seats,
        last_discard,
        round_wind: obs.round_wind,
        seat_wind: ((pid + 4 - oya) % 4) as u8,
        honba: obs.honba,
        riichi_sticks: obs.riichi_sticks,
        turn_count,
        is_dealer: pid == oya,
        kyoku_index: obs.kyoku_index,
        self_riichi: obs.riichi_declared[pid],
        self_furiten: false,
        rounds_left: 0,
    }
}

/// Build an [`EncInput`] from a 3-player (sanma) observation.
///
/// `kita_counts` holds each seat's nukidora (kita) count in **absolute** seat
/// order; it is not carried on the observation, so the caller reads it from the
/// owning `GameState3P`, as it does `turn_count` and `last_discard`.
pub fn enc_input_3p(
    obs: &Observation3P,
    turn_count: u32,
    kita_counts: [u8; 3],
    last_discard: Option<u8>,
) -> EncInput {
    let pid = obs.player_id as usize;
    let oya = obs.oya as usize;

    let seats: Vec<SeatFeat> = (0..3)
        .map(|k| {
            let rel = (pid + k) % 3;
            SeatFeat {
                discards: obs.discards[rel].iter().map(|&t| t as u8).collect(),
                meld_tiles: obs.melds[rel]
                    .iter()
                    .flat_map(|m| m.tiles.iter().copied())
                    .collect(),
                riichi_declared: obs.riichi_declared[rel],
                riichi_tile: obs.riichi_sutehais[rel],
                score: obs.scores[rel],
                kita_count: kita_counts[rel],
                ..Default::default()
            }
        })
        .collect();

    EncInput {
        num_players: 3,
        hand: obs.hands[pid].iter().map(|&t| t as u8).collect(),
        drawn_tile: obs.drawn_tile,
        waits: obs.waits.clone(),
        is_tenpai: obs.is_tenpai,
        dora_indicators: obs.dora_indicators.iter().map(|&t| t as u8).collect(),
        seats,
        last_discard,
        round_wind: obs.round_wind,
        seat_wind: ((pid + 3 - oya) % 3) as u8,
        honba: obs.honba,
        riichi_sticks: obs.riichi_sticks,
        turn_count,
        is_dealer: pid == oya,
        kyoku_index: obs.kyoku_index,
        self_riichi: obs.riichi_declared[pid],
        self_furiten: false,
        rounds_left: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obs::last_discard_channel;
    use crate::tiles::{tile_dim, tile_index};
    use riichienv_core::replay::MjaiEvent;
    use riichienv_core::rule::GameRule;

    fn ev(line: &str) -> MjaiEvent {
        serde_json::from_str(line).expect("valid mjai event")
    }

    /// Regression: the "last discard" plane must one-hot the **tile that was
    /// discarded**, not the discarder's seat. riichienv-core 0.4.8's
    /// `Observation::last_discard` mislabels the `(pid, tile)` tuple and yields
    /// the pid, which `tile_index` folds onto tile34 0 for every seat — so the
    /// plane lit `1m` on every call decision and the model could never see the
    /// tile it was being asked to pon/chi/ron.
    #[test]
    fn last_discard_plane_holds_the_tile_not_the_discarder_seat() {
        let hand = r#"["1m","2m","3m","4m","5m","6m","7m","8m","9m","1p","2p","3p","4p"]"#;
        let mut state = GameState::new(0, true, None, 0, GameRule::default_tenhou());
        state.apply_mjai_event(ev(r#"{"type":"start_game","names":["a","b","c","d"]}"#));
        state.apply_mjai_event(ev(&format!(
            r#"{{"type":"start_kyoku","bakaze":"E","dora_marker":"2m","kyoku":1,"honba":0,
                 "kyotaku":0,"oya":0,"scores":[25000,25000,25000,25000],
                 "tehais":[{hand},{hand},{hand},{hand}]}}"#
        )));
        // Seat 1 discards 9s (tile34 26) — a tile whose index can never be
        // confused with a seat number.
        state.apply_mjai_event(ev(r#"{"type":"tsumo","actor":1,"pai":"9s"}"#));
        state.apply_mjai_event(ev(
            r#"{"type":"dahai","actor":1,"pai":"9s","tsumogiri":true}"#,
        ));

        let (buf, _legal) = obs_and_legal_4p(&mut state, 0);
        let t = tile_dim(4);
        let base = last_discard_channel(4) * t;
        let nine_sou = tile_index(104, 4).unwrap(); // 9s -> tile34 26

        assert_eq!(buf[base + nine_sou], 1.0, "9s cell must be set");
        assert_eq!(
            buf[base], 0.0,
            "tile34 0 (1m) must be clear — it is what the discarder's pid 1 \
             would have encoded to"
        );
        assert_eq!(
            buf[base..base + t].iter().filter(|v| **v != 0.0).count(),
            1,
            "the plane is a one-hot"
        );
    }

    /// V2 through a real sanma state: tedashi, latest discard and the tiles
    /// each seat let pass, all in relative seat order.
    #[test]
    fn v2_reads_tedashi_and_passed_tiles_from_the_state() {
        use crate::obs::{channels, Features};
        use riichienv_core::state_3p::GameState3P;

        let hand = r#"["1m","9m","1p","2p","3p","4p","5p","6p","7p","8p","9p","2s","E"]"#;
        let mut state = GameState3P::new(0, true, None, 0, GameRule::default_tenhou());
        let mut timeline = DiscardTimeline::new();
        let mut feed = |state: &mut GameState3P, line: &str| {
            let e = ev(line);
            timeline.note_mjai(&e);
            state.apply_mjai_event(e);
            sync_timeline_3p(&mut timeline, state);
        };
        feed(&mut state, r#"{"type":"start_game","names":["a","b","c"]}"#);
        feed(
            &mut state,
            &format!(
                r#"{{"type":"start_kyoku","bakaze":"E","dora_marker":"1s","kyoku":1,"honba":0,
                     "kyotaku":0,"oya":0,"scores":[35000,35000,35000],
                     "tehais":[{hand},{hand},{hand}]}}"#
            ),
        );
        // Seat 0 tsumogiri 9s, seat 1 tedashi 2s, seat 2 tsumogiri W.
        feed(&mut state, r#"{"type":"tsumo","actor":0,"pai":"9s"}"#);
        feed(
            &mut state,
            r#"{"type":"dahai","actor":0,"pai":"9s","tsumogiri":true}"#,
        );
        feed(&mut state, r#"{"type":"tsumo","actor":1,"pai":"5s"}"#);
        feed(
            &mut state,
            r#"{"type":"dahai","actor":1,"pai":"2s","tsumogiri":false}"#,
        );
        feed(&mut state, r#"{"type":"tsumo","actor":2,"pai":"W"}"#);
        feed(
            &mut state,
            r#"{"type":"dahai","actor":2,"pai":"W","tsumogiri":true}"#,
        );
        feed(&mut state, r#"{"type":"tsumo","actor":0,"pai":"5s"}"#);

        let (buf, _legal) = obs_and_legal_3p_with(
            &mut state,
            0,
            Features::V2,
            &timeline,
            GameLength::default(),
        );
        let t = tile_dim(3);
        assert_eq!(buf.len(), Features::V2.channels(3) * t);
        let n = 3;
        let tedashi = channels(3);
        let latest = tedashi + 2 * n;
        let passed = tedashi + 4 * n;
        let x = |tid: u8| tile_index(tid, 3).unwrap();
        let (s2, w, s9) = (x(76), x(116), x(104));
        let plane = |ch: usize| &buf[ch * t..(ch + 1) * t];
        let lit = |ch: usize| -> Vec<usize> {
            plane(ch)
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0.0)
                .map(|(i, _)| i)
                .collect()
        };

        assert!(
            lit(tedashi).is_empty(),
            "seat 0 only discarded off the draw"
        );
        assert_eq!(
            lit(tedashi + 1),
            vec![s2],
            "seat 1 (shimocha) cut 2s from the hand"
        );
        assert!(lit(tedashi + 2).is_empty());
        assert_eq!(lit(latest + 2), vec![w], "seat 2's latest discard");
        let mut after_ours = vec![s2, w];
        after_ours.sort();
        assert_eq!(lit(passed), after_ours, "both came after our 9s");
        assert_eq!(lit(passed + 1), vec![w], "only W came after seat 1's 2s");
        assert!(lit(passed + 2).is_empty(), "nothing after seat 2's W");
        assert_eq!(plane(latest)[s9], 1.0);
    }

    /// No discard on the table yet (our own draw) ⇒ the plane is all zeros.
    #[test]
    fn last_discard_plane_is_empty_before_any_discard() {
        let hand = r#"["1m","2m","3m","4m","5m","6m","7m","8m","9m","1p","2p","3p","4p"]"#;
        let mut state = GameState::new(0, true, None, 0, GameRule::default_tenhou());
        state.apply_mjai_event(ev(r#"{"type":"start_game","names":["a","b","c","d"]}"#));
        state.apply_mjai_event(ev(&format!(
            r#"{{"type":"start_kyoku","bakaze":"E","dora_marker":"2m","kyoku":1,"honba":0,
                 "kyotaku":0,"oya":0,"scores":[25000,25000,25000,25000],
                 "tehais":[{hand},{hand},{hand},{hand}]}}"#
        )));
        state.apply_mjai_event(ev(r#"{"type":"tsumo","actor":0,"pai":"5p"}"#));

        let (buf, _legal) = obs_and_legal_4p(&mut state, 0);
        let t = tile_dim(4);
        let base = last_discard_channel(4) * t;
        assert!(
            buf[base..base + t].iter().all(|v| *v == 0.0),
            "no discard on the table yet"
        );
    }
}
