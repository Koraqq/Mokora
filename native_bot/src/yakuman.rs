//! Yakuman's side in Mokora: reading the table for the planner, and scoring
//! a win as a yakuman or not.
//!
//! The planner itself — which yakuman to aim for and how to order our
//! choices towards it — is a paid feature, in the module Mokora downloads
//! once a key is redeemed (MokoraPro, through `pro_api::yakuman`). Mokora
//! hands it in as a [`Planner`]: [`rank_3p`] / [`rank_4p`] read our hand and
//! the unseen tiles ([`view_3p`] / [`view_4p`]), score each win the model
//! offers ([`yakuman_of`] / [`yakuman_of_4p`]), and ask the planner for the
//! order. `None` — no yakuman reachable, or no planner — leaves the model's
//! own choice.

use pro_api::yakuman::{Choice, Hand, Kind, PlanRequest, PlanResponse, Table};
use riichienv_core::action::{Action, ActionType};
use riichienv_core::hand_evaluator::HandEvaluator;
use riichienv_core::hand_evaluator_3p::HandEvaluator3P;
use riichienv_core::state::GameState;
use riichienv_core::state_3p::GameState3P;
use riichienv_core::types::{Conditions, Meld, MeldType, Wind};

/// Orders our choices towards a yakuman: the paid module, as Mokora loaded it.
pub trait Planner: Send + Sync {
    fn plan(&self, req: &PlanRequest) -> PlanResponse;
}

/// Whether a tile34 exists at a table of `players` (sanma has no 2m–8m).
fn exists_at(players: u8, t: usize) -> bool {
    players == 4 || !(1..=7).contains(&t)
}

/// Our hand and the table as seen from `seat`, from every seat's hand,
/// melds, pond and kita. One reading for both table sizes.
struct SeatView<'a> {
    hand: &'a [u8],
    melds: &'a [Meld],
    discards: &'a [u8],
    kita: &'a [u8],
}

fn view_of(
    seats: &[SeatView],
    seat: usize,
    dora_indicators: &[u8],
    wall_left: u32,
    players: u8,
) -> (Hand, Table) {
    let me = &seats[seat];
    let mut hand = Hand {
        counts: vec![0; 34],
        ..Hand::default()
    };
    for &tid in me.hand {
        hand.counts[(tid / 4) as usize] += 1;
    }
    for m in me.melds {
        let low = m.tiles.iter().map(|&t| (t / 4) as usize).min().unwrap_or(0);
        if m.meld_type == MeldType::Chi {
            hand.runs.push(low as u8);
        } else {
            hand.melds.push((low as u8, m.opened || m.meld_type != MeldType::Ankan));
        }
    }

    let mut seen = [0u8; 34];
    let mut see = |tid: u8| seen[(tid / 4) as usize] += 1;
    me.hand.iter().for_each(|&t| see(t));
    dora_indicators.iter().for_each(|&t| see(t));
    for p in seats {
        p.discards.iter().for_each(|&t| see(t));
        p.kita.iter().for_each(|&t| see(t));
        for m in p.melds {
            m.tiles.iter().for_each(|&t| see(t));
        }
    }
    // A claimed tile is in the discarder's pond and in the meld.
    for p in seats {
        for m in p.melds {
            if let Some(t) = m.called_tile {
                let t = (t / 4) as usize;
                seen[t] = seen[t].saturating_sub(1);
            }
        }
    }
    let unseen = (0..34)
        .map(|t| {
            if exists_at(players, t) {
                4u8.saturating_sub(seen[t])
            } else {
                0
            }
        })
        .collect();
    (
        hand,
        Table {
            players,
            unseen,
            wall_left,
            own_draws: wall_left.div_ceil(u32::from(players)),
        },
    )
}

/// Our hand and the table as seen from `seat`, in sanma.
pub fn view_3p(state: &GameState3P, seat: u8) -> (Hand, Table) {
    let seats: Vec<SeatView> = state
        .players
        .iter()
        .map(|p| SeatView {
            hand: &p.hand,
            melds: &p.melds,
            discards: &p.discards,
            kita: &p.kita_tiles,
        })
        .collect();
    view_of(
        &seats,
        seat as usize,
        &state.wall.dora_indicators,
        u32::from(state.wall.drawable_count),
        3,
    )
}

/// Our hand and the table as seen from `seat`, in yonma.
pub fn view_4p(state: &GameState, seat: u8) -> (Hand, Table) {
    let seats: Vec<SeatView> = state
        .players
        .iter()
        .map(|p| SeatView {
            hand: &p.hand,
            melds: &p.melds,
            discards: &p.discards,
            kita: &[],
        })
        .collect();
    view_of(
        &seats,
        seat as usize,
        &state.wall.dora_indicators,
        u32::from(state.wall.drawable_count),
        4,
    )
}

/// The concealed tiles a win `action` is scored on, the winning tile, and
/// whether it is a tsumo; `None` for a win with no tile to name.
fn win_hand(hand: &[u8], drawn: Option<u8>, action: &Action) -> Option<(Vec<u8>, u8, bool)> {
    let tsumo = action.action_type == ActionType::Tsumo;
    let tile = action.tile.or(if tsumo { drawn } else { None })?;
    let mut hand = hand.to_vec();
    if tsumo {
        if let Some(i) = hand.iter().rposition(|&t| t == tile) {
            hand.remove(i);
        }
    }
    Some((hand, tile, tsumo))
}

/// Whether the win `action` (tsumo or ron) would score a yakuman, evaluated
/// the way the game state offers it.
pub fn wins_yakuman(state: &GameState3P, seat: u8, action: &Action) -> bool {
    yakuman_of(state, seat, action).is_some()
}

/// The yaku ids of the sanma win `action` when it scores a yakuman.
pub fn yakuman_of(state: &GameState3P, seat: u8, action: &Action) -> Option<Vec<u32>> {
    let me = &state.players[seat as usize];
    let (hand, tile, tsumo) = win_hand(&me.hand, state.drawn_tile, action)?;
    let last = state.wall.drawable_count == 0 && !state.is_rinshan_flag;
    let cond = Conditions {
        tsumo,
        riichi: me.riichi_declared,
        double_riichi: me.double_riichi_declared,
        ippatsu: me.ippatsu_cycle,
        player_wind: Wind::from((seat + 3 - state.oya) % 3),
        round_wind: Wind::from(state.round_wind),
        haitei: tsumo && last,
        houtei: !tsumo && last,
        rinshan: tsumo && state.is_rinshan_flag,
        tsumo_first_turn: tsumo && state.is_first_turn && me.discards.is_empty(),
        riichi_sticks: state.riichi_sticks,
        honba: state.honba as u32,
        is_sanma: true,
        num_players: 3,
        ..Default::default()
    };
    let res = HandEvaluator3P::new(hand, me.melds.clone()).calc(
        tile,
        state.wall.dora_indicators.clone(),
        vec![],
        Some(cond),
    );
    res.yakuman.then_some(res.yaku)
}

/// Whether the yonma win `action` would score a yakuman.
pub fn wins_yakuman_4p(state: &GameState, seat: u8, action: &Action) -> bool {
    yakuman_of_4p(state, seat, action).is_some()
}

/// The yaku ids of the yonma win `action` when it scores a yakuman.
pub fn yakuman_of_4p(state: &GameState, seat: u8, action: &Action) -> Option<Vec<u32>> {
    let me = &state.players[seat as usize];
    let (hand, tile, tsumo) = win_hand(&me.hand, state.drawn_tile, action)?;
    let last = state.wall.drawable_count == 0 && !state.is_rinshan_flag;
    let cond = Conditions {
        tsumo,
        riichi: me.riichi_declared,
        double_riichi: me.double_riichi_declared,
        ippatsu: me.ippatsu_cycle,
        player_wind: Wind::from((seat + 4 - state.oya) % 4),
        round_wind: Wind::from(state.round_wind),
        haitei: tsumo && last,
        houtei: !tsumo && last,
        rinshan: tsumo && state.is_rinshan_flag,
        tsumo_first_turn: tsumo && state.is_first_turn && me.discards.is_empty(),
        riichi_sticks: state.riichi_sticks,
        honba: state.honba as u32,
        is_sanma: false,
        num_players: 4,
        ..Default::default()
    };
    let res = HandEvaluator::new(hand, me.melds.clone()).calc(
        tile,
        state.wall.dora_indicators.clone(),
        vec![],
        Some(cond),
    );
    res.yakuman.then_some(res.yaku)
}

/// What a riichienv action is to the planner.
fn kind_of(t: ActionType) -> Kind {
    match t {
        ActionType::Discard => Kind::Discard,
        ActionType::Kita => Kind::Kita,
        ActionType::Ankan => Kind::Ankan,
        ActionType::Kakan => Kind::Kakan,
        ActionType::Pass => Kind::Pass,
        ActionType::Pon => Kind::Pon,
        ActionType::Chi => Kind::Chi,
        ActionType::Daiminkan => Kind::Daiminkan,
        ActionType::Tsumo => Kind::Tsumo,
        ActionType::Ron => Kind::Ron,
        ActionType::Riichi => Kind::Riichi,
        _ => Kind::Other,
    }
}

/// Ask `planner` to order `ranked` (every legal action, best first by the
/// model) for our hand; its answer as `(action, prob)`.
fn ask(
    planner: &dyn Planner,
    (hand, table): (Hand, Table),
    riichi: bool,
    ranked: &[(Action, f32)],
    is_yakuman: impl Fn(&Action) -> bool,
) -> Option<Vec<(Action, f32)>> {
    let choices = ranked
        .iter()
        .map(|(a, _)| Choice {
            kind: kind_of(a.action_type),
            tile: a.tile,
            consume: a.consume_tiles.clone(),
            yakuman_win: matches!(a.action_type, ActionType::Tsumo | ActionType::Ron)
                && is_yakuman(a),
        })
        .collect();
    let order = planner.plan(&PlanRequest {
        hand,
        table,
        riichi,
        choices,
    })?;
    let out: Vec<(Action, f32)> = order
        .into_iter()
        .filter_map(|(i, p)| ranked.get(i).map(|(a, _)| (a.clone(), p)))
        .collect();
    (!out.is_empty()).then_some(out)
}

/// The planner's order of `ranked` for a sanma seat, or `None` when no
/// yakuman is reachable and the model should decide.
pub fn rank_3p(
    state: &GameState3P,
    seat: u8,
    ranked: &[(Action, f32)],
    planner: &dyn Planner,
) -> Option<Vec<(Action, f32)>> {
    let me = &state.players[seat as usize];
    ask(
        planner,
        view_3p(state, seat),
        me.riichi_declared || me.riichi_stage,
        ranked,
        |a| wins_yakuman(state, seat, a),
    )
}

/// [`rank_3p`] for a yonma seat.
pub fn rank_4p(
    state: &GameState,
    seat: u8,
    ranked: &[(Action, f32)],
    planner: &dyn Planner,
) -> Option<Vec<(Action, f32)>> {
    let me = &state.players[seat as usize];
    ask(
        planner,
        view_4p(state, seat),
        me.riichi_declared || me.riichi_stage,
        ranked,
        |a| wins_yakuman_4p(state, seat, a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tile34 of an mjai-ish short name: "1p", "9s", "E", "P"…
    fn t34(s: &str) -> usize {
        match s {
            "E" => 27,
            "S" => 28,
            "W" => 29,
            "N" => 30,
            "P" => 31,
            "F" => 32,
            "C" => 33,
            _ => {
                let n = s[..1].parse::<usize>().unwrap() - 1;
                let suit = match &s[1..] {
                    "m" => 0,
                    "p" => 9,
                    "s" => 18,
                    x => panic!("suit {x}"),
                };
                suit + n
            }
        }
    }

    /// tile136 of the `copy`th copy of a tile34.
    fn tid(t: usize, copy: u8) -> u8 {
        t as u8 * 4 + copy
    }

    fn yonma() -> GameState {
        GameState::new(1, true, Some(7), 0, riichienv_core::rule::GameRule::default_mjsoul())
    }

    /// Answers by a fixed rule and keeps what it was asked.
    struct Echo(std::sync::Mutex<Option<PlanRequest>>);

    impl Planner for Echo {
        fn plan(&self, req: &PlanRequest) -> PlanResponse {
            *self.0.lock().unwrap() = Some(req.clone());
            // The last choice first.
            Some(vec![(req.choices.len() - 1, 1.0)])
        }
    }

    /// The yonma scorer recognises a yakuman win and says so to the planner;
    /// an ordinary win on the same table is not one.
    #[test]
    fn a_yakuman_win_is_marked_for_the_planner() {
        let mut state = yonma();
        let tiles = ["1m", "9m", "1p", "9p", "1s", "9s", "E", "S", "W", "N", "P", "F", "C"];
        state.players[0].hand = tiles.iter().map(|t| tid(t34(t), 0)).collect();
        state.players[0].melds.clear();
        let ron = Action::new(ActionType::Ron, Some(tid(t34("C"), 1)), vec![], Some(0));
        assert!(yakuman_of_4p(&state, 0, &ron).is_some_and(|ids| !ids.is_empty()));
        let pass = Action::new(ActionType::Pass, None, vec![], Some(0));
        let planner = Echo(Default::default());
        let out = rank_4p(&state, 0, &[(pass, 0.9), (ron.clone(), 0.1)], &planner).unwrap();
        assert_eq!(out, vec![(ron, 1.0)], "the planner's order, as actions");
        let asked = planner.0.lock().unwrap().clone().unwrap();
        assert_eq!(asked.choices[1].kind, Kind::Ron);
        assert!(asked.choices[1].yakuman_win);
        assert!(!asked.choices[0].yakuman_win);
        assert_eq!(asked.hand.counts.len(), 34);
        assert_eq!(asked.table.unseen.len(), 34);

        let plain = ["2m", "3m", "4m", "5p", "6p", "7p", "3s", "4s", "5s", "6s", "7s", "8s", "9s"];
        state.players[0].hand = plain.iter().map(|t| tid(t34(t), 0)).collect();
        let ron = Action::new(ActionType::Ron, Some(tid(t34("9s"), 1)), vec![], Some(0));
        assert!(yakuman_of_4p(&state, 0, &ron).is_none());
    }

    #[test]
    fn a_yonma_table_draws_a_quarter_of_the_wall() {
        let (h, tb) = view_4p(&yonma(), 0);
        assert_eq!(tb.players, 4);
        assert_eq!(tb.own_draws, tb.wall_left.div_ceil(4));
        assert!(h.counts.iter().map(|&c| u32::from(c)).sum::<u32>() >= 13);
        assert!(
            (1..=7).any(|t| tb.unseen[t] > 0),
            "2m-8m are in play in yonma"
        );
    }
}
