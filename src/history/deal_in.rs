//! Which tiles were dealt in (放銃) across recorded games.
//!
//! Read straight from each game's saved event log rather than stored in its
//! [`GameRecord`](crate::schema::GameRecord), so every game already recorded
//! counts, with no migration of `index.jsonl`. A deal-in is a `hora` whose
//! `target` is not its `actor` (a tsumo names the winner as both); the mjai
//! `hora` does not carry the winning tile, so it is the last tile the target
//! let go of — a discard, the added tile of a kakan (chankan), or, in sanma,
//! a kita.
//!
//! Every seat's deal-ins count, split by who dealt in: the recorded player
//! (`mine`) or an opponent (`others`, whoever they dealt in to — the recorded
//! player included). A game with no recorded seat (observed) counts entirely
//! as `others`. A double ron off one discard is one tile dealt in, counted
//! once. Red fives count as their plain five.

use crate::schema::{DealInStats, DealInTally, DealInTile, MjaiEvent};
use std::collections::HashMap;

/// The plain tile a red five stands for (`5pr` → `5p`); anything else as is.
fn plain(tile: &str) -> &str {
    tile.strip_suffix('r').unwrap_or(tile)
}

/// Deal-in counts per tile, by who dealt in.
#[derive(Default)]
pub struct Counts {
    pub mine: HashMap<String, u32>,
    pub others: HashMap<String, u32>,
}

/// Add one game's deal-ins to `counts`; `our_seat` is the recorded player's
/// seat, if the game has one.
pub fn count_game(events: &[MjaiEvent], our_seat: Option<u8>, counts: &mut Counts) {
    // The last tile let go of, by whom, and whether a ron on it was counted.
    let mut last: Option<(u8, String)> = None;
    let mut counted = false;
    for ev in events {
        match ev {
            MjaiEvent::StartKyoku { .. } => {
                last = None;
                counted = false;
            }
            MjaiEvent::Dahai { actor, pai, .. } | MjaiEvent::Kakan { actor, pai, .. } => {
                last = Some((*actor, plain(pai).to_string()));
                counted = false;
            }
            MjaiEvent::Kita { actor, pai } => {
                let pai = pai.as_deref().unwrap_or("N");
                last = Some((*actor, plain(pai).to_string()));
                counted = false;
            }
            MjaiEvent::Hora { actor, target, .. } if actor != target => {
                if let Some((from, tile)) = &last {
                    if from == target && !counted {
                        let side = if Some(*from) == our_seat {
                            &mut counts.mine
                        } else {
                            &mut counts.others
                        };
                        *side.entry(tile.clone()).or_default() += 1;
                        counted = true;
                    }
                }
            }
            _ => {}
        }
    }
}

/// One side's counts, most frequent tile first (ties in tile order, so the
/// list is stable).
fn tally(counts: HashMap<String, u32>) -> DealInTally {
    let mut tiles: Vec<DealInTile> = counts
        .into_iter()
        .map(|(tile, count)| DealInTile { tile, count })
        .collect();
    tiles.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tile.cmp(&b.tile)));
    DealInTally {
        deal_ins: tiles.iter().map(|t| t.count).sum(),
        tiles,
    }
}

/// Deal-in counts over many games, each with the recorded player's seat.
pub fn summarize<'a>(
    games: impl IntoIterator<Item = (&'a [MjaiEvent], Option<u8>)>,
) -> DealInStats {
    let mut counts = Counts::default();
    let mut games_read = 0;
    for (events, our_seat) in games {
        games_read += 1;
        count_game(events, our_seat, &mut counts);
    }
    DealInStats {
        games: games_read,
        mine: tally(counts.mine),
        others: tally(counts.others),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dahai(actor: u8, pai: &str) -> MjaiEvent {
        MjaiEvent::Dahai {
            actor,
            pai: pai.into(),
            tsumogiri: false,
        }
    }

    fn hora(actor: u8, target: u8) -> MjaiEvent {
        MjaiEvent::Hora {
            actor,
            target,
            deltas: None,
            ura_markers: None,
        }
    }

    /// Both sides together, as the tests below mostly don't care who.
    fn tiles(stats: &DealInStats) -> Vec<(&str, u32)> {
        let mut all: Vec<(&str, u32)> = stats
            .mine
            .tiles
            .iter()
            .chain(stats.others.tiles.iter())
            .map(|t| (t.tile.as_str(), t.count))
            .collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        all
    }

    fn side(tally: &DealInTally) -> Vec<(&str, u32)> {
        tally
            .tiles
            .iter()
            .map(|t| (t.tile.as_str(), t.count))
            .collect()
    }

    /// The shape seen in the saved logs: a ron names the discarder as
    /// `target`, and the tile is that player's last discard.
    #[test]
    fn a_ron_counts_the_discard_it_was_won_on() {
        let game = vec![dahai(0, "3m"), dahai(1, "7p"), hora(2, 1)];
        let stats = summarize([(game.as_slice(), None)]);
        assert_eq!(tiles(&stats), vec![("7p", 1)]);
        assert_eq!(stats.games, 1);
    }

    #[test]
    fn a_tsumo_is_not_a_deal_in() {
        let game = vec![dahai(1, "7p"), hora(2, 2)];
        let stats = summarize([(game.as_slice(), None)]);
        assert_eq!(stats.mine.deal_ins + stats.others.deal_ins, 0);
    }

    /// Two winners off one discard: one tile dealt in.
    #[test]
    fn a_double_ron_counts_once() {
        let game = vec![dahai(1, "E"), hora(0, 1), hora(2, 1)];
        assert_eq!(tiles(&summarize([(game.as_slice(), None)])), vec![("E", 1)]);
    }

    #[test]
    fn chankan_and_kita_are_the_tiles_let_go_of() {
        let kakan = MjaiEvent::Kakan {
            actor: 1,
            pai: "5sr".into(),
            consumed: ["5s".into(), "5s".into(), "5s".into()],
        };
        let kita = MjaiEvent::Kita {
            actor: 0,
            pai: Some("N".into()),
        };
        let a = vec![kakan, hora(0, 1)];
        let b = vec![kita, hora(2, 0)];
        assert_eq!(
            tiles(&summarize([(a.as_slice(), None), (b.as_slice(), None)])),
            vec![("5s", 1), ("N", 1)],
            "a red five counts as its plain five",
        );
    }

    /// Who dealt in decides the side — the recorded player's own deal-ins
    /// are `mine`, an opponent's are `others` even when they dealt in to us.
    #[test]
    fn deal_ins_are_split_by_who_dealt_in() {
        let game = vec![
            dahai(1, "3s"),
            hora(0, 1), // seat 1 (us) dealt in
            MjaiEvent::StartKyoku {
                bakaze: "E".into(),
                dora_marker: "1p".into(),
                kyoku: 2,
                honba: 0,
                kyotaku: 0,
                oya: 1,
                scores: vec![35_000; 3],
                tehais: vec![vec![]; 3],
                num_players: 3,
            },
            dahai(2, "C"),
            hora(1, 2), // an opponent dealt in to us
            dahai(0, "9s"),
            hora(2, 0), // between opponents
        ];
        let stats = summarize([(game.as_slice(), Some(1))]);
        assert_eq!(side(&stats.mine), vec![("3s", 1)]);
        assert_eq!(side(&stats.others), vec![("9s", 1), ("C", 1)]);
        assert_eq!((stats.mine.deal_ins, stats.others.deal_ins), (1, 2));
    }

    #[test]
    fn most_dealt_in_tile_comes_first() {
        let a = vec![dahai(1, "1s"), hora(0, 1)];
        let b = vec![dahai(0, "9m"), hora(1, 0)];
        let c = vec![dahai(2, "9m"), hora(0, 2)];
        let stats = summarize([
            (a.as_slice(), None),
            (b.as_slice(), None),
            (c.as_slice(), None),
        ]);
        assert_eq!(tiles(&stats), vec![("9m", 2), ("1s", 1)]);
    }
}
