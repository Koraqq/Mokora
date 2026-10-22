//! What opponents did to the recorded player across recorded games: how
//! often their discards were called (pon, open kan, and chi in 4p) and how
//! many yakuman they paid for — and how many yakuman the player won, told
//! the same way (see [`is_yakuman`]).
//!
//! Read straight from each game's saved event log, like
//! [`deal_in`](super::deal_in), so every game already recorded counts with no
//! migration of `index.jsonl`. A game with no recorded seat (observed) counts
//! as read but adds nothing.
//!
//! A yakuman is told from what was paid, since `hora` carries no yaku. The
//! payer, not the winner, is read: the winner's gain also holds the riichi
//! sticks on the table, and a sanma tsumo pays less than a ron (nobody pays
//! the missing north seat's share). A yakuman ron costs its payer 32,000
//! (48,000 to a dealer); a yakuman tsumo costs the dealer — or, when the
//! dealer won, everyone — 16,000. A sanbaiman stops at 24,000 / 36,000 /
//! 12,000, far enough below for honba not to blur them.

use crate::schema::{AgainstStats, MjaiEvent};

/// A ron the payer paid at least this much for, to a non-dealer, was a yakuman.
const YAKUMAN_RON_KO: i64 = 32_000;
/// The same, to the dealer.
const YAKUMAN_RON_OYA: i64 = 48_000;
/// A tsumo where any one payer paid at least this much was a yakuman.
const YAKUMAN_TSUMO_SHARE: i64 = 16_000;

/// What seat `seat` paid in `deltas` (zero if it gained or is missing).
fn paid(deltas: &[i32], seat: u8) -> i64 {
    -i64::from(deltas.get(seat as usize).copied().unwrap_or(0)).min(0)
}

/// Was this win a yakuman? `oya` is the round's dealer. Read off what was
/// paid rather than what the winner gained: a sanma tsumo yakuman gains a
/// non-dealer only 24,000 (16,000 + 8,000, nobody pays the north seat's
/// share), short of the 32,000 a ron brings. Also used by the game
/// [`aggregator`](super::aggregator).
pub(super) fn is_yakuman(actor: u8, target: u8, deltas: &[i32], oya: u8) -> bool {
    if actor == target {
        return (0..deltas.len() as u8)
            .filter(|&s| s != actor)
            .any(|s| paid(deltas, s) >= YAKUMAN_TSUMO_SHARE);
    }
    // A double ron may sum both wins into one event's payer: the winner's
    // own gain bounds it. A single ron's gain is the payment plus sticks.
    let gained = i64::from(deltas.get(actor as usize).copied().unwrap_or(0));
    let cost = paid(deltas, target).min(gained);
    cost >= if actor == oya {
        YAKUMAN_RON_OYA
    } else {
        YAKUMAN_RON_KO
    }
}

/// Add one game to `stats`; `our_seat` is the recorded player's seat, if the
/// game has one.
pub fn count_game(events: &[MjaiEvent], our_seat: Option<u8>, stats: &mut AgainstStats) {
    stats.games += 1;
    let Some(me) = our_seat else { return };
    let mut oya = 0;
    for ev in events {
        match ev {
            MjaiEvent::StartKyoku { oya: o, .. } => oya = *o,
            MjaiEvent::Pon { actor, target, .. } if *target == me && *actor != me => {
                stats.pon += 1;
            }
            MjaiEvent::Chi { actor, target, .. } if *target == me && *actor != me => {
                stats.chi += 1;
            }
            MjaiEvent::Daiminkan { actor, target, .. } if *target == me && *actor != me => {
                stats.kan += 1;
            }
            MjaiEvent::Hora {
                actor,
                target,
                deltas: Some(deltas),
                ..
            } if *actor != me
                && paid(deltas, me) > 0
                && is_yakuman(*actor, *target, deltas, oya) =>
            {
                stats.yakuman += 1;
            }
            MjaiEvent::Hora {
                actor,
                target,
                deltas: Some(deltas),
                ..
            } if *actor == me && is_yakuman(*actor, *target, deltas, oya) => {
                stats.won_yakuman += 1;
            }
            _ => {}
        }
    }
}

/// The counts over many games, each with the recorded player's seat.
pub fn summarize<'a>(
    games: impl IntoIterator<Item = (&'a [MjaiEvent], Option<u8>)>,
) -> AgainstStats {
    let mut stats = AgainstStats::default();
    for (events, our_seat) in games {
        count_game(events, our_seat, &mut stats);
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(oya: u8) -> MjaiEvent {
        MjaiEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "1p".into(),
            kyoku: oya + 1,
            honba: 0,
            kyotaku: 0,
            oya,
            scores: vec![35_000; 3],
            tehais: vec![vec![]; 3],
            num_players: 3,
        }
    }

    fn pon(actor: u8, target: u8) -> MjaiEvent {
        MjaiEvent::Pon {
            actor,
            target,
            pai: "C".into(),
            consumed: ["C".into(), "C".into()],
        }
    }

    fn chi(actor: u8, target: u8) -> MjaiEvent {
        MjaiEvent::Chi {
            actor,
            target,
            pai: "3m".into(),
            consumed: ["4m".into(), "5m".into()],
        }
    }

    fn kan(actor: u8, target: u8) -> MjaiEvent {
        MjaiEvent::Daiminkan {
            actor,
            target,
            pai: "C".into(),
            consumed: ["C".into(), "C".into(), "C".into()],
        }
    }

    fn hora(actor: u8, target: u8, deltas: &[i32]) -> MjaiEvent {
        MjaiEvent::Hora {
            actor,
            target,
            deltas: Some(deltas.to_vec()),
            ura_markers: None,
        }
    }

    fn count(game: &[MjaiEvent], seat: Option<u8>) -> AgainstStats {
        summarize([(game, seat)])
    }

    /// Only calls on the recorded player's own discards count.
    #[test]
    fn calls_on_our_discards_count() {
        let game = vec![
            start(0),
            pon(0, 1),
            pon(2, 1),
            pon(1, 0),
            chi(2, 1),
            chi(1, 0),
            kan(2, 1),
            kan(1, 2),
        ];
        let s = count(&game, Some(1));
        assert_eq!((s.pon, s.chi, s.kan), (2, 1, 1));
    }

    /// Seen in the saved logs: a non-dealer's yakuman ron costs 32,000, a
    /// sanbaiman 24,000 (plus 200 a honba in sanma).
    #[test]
    fn a_yakuman_ron_is_told_from_a_sanbaiman() {
        let yakuman = vec![start(1), hora(0, 2, &[33_000, 0, -32_000])];
        let sanbaiman = vec![start(1), hora(0, 2, &[25_200, 0, -24_200])];
        assert_eq!(count(&yakuman, Some(2)).yakuman, 1);
        assert_eq!(count(&sanbaiman, Some(2)).yakuman, 0);
    }

    /// A dealer's sanbaiman ron (36,000) is not a yakuman; 48,000 is.
    #[test]
    fn a_dealer_ron_needs_48000() {
        let sanbaiman = vec![start(0), hora(0, 1, &[36_000, -36_000, 0])];
        let yakuman = vec![start(0), hora(0, 1, &[49_000, -48_000, 0])];
        assert_eq!(count(&sanbaiman, Some(1)).yakuman, 0);
        assert_eq!(count(&yakuman, Some(1)).yakuman, 1);
    }

    /// Sanma tsumo, no north share: a non-dealer's yakuman is 8,000 / 16,000,
    /// short of the 32,000 a ron costs; a sanbaiman 6,000 / 12,000.
    #[test]
    fn a_sanma_tsumo_yakuman_is_told_by_the_largest_share() {
        let yakuman = vec![start(0), hora(2, 2, &[-16_000, -8_000, 24_000])];
        let sanbaiman = vec![start(0), hora(2, 2, &[-12_000, -6_000, 18_000])];
        assert_eq!(count(&yakuman, Some(1)).yakuman, 1, "we paid 8,000 of it");
        assert_eq!(count(&yakuman, Some(0)).yakuman, 1);
        assert_eq!(count(&sanbaiman, Some(0)).yakuman, 0);
    }

    /// Our own yakuman count as won — a sanma tsumo too, which gains a
    /// non-dealer only 24,000 (as logged: 24,200 with a honba).
    #[test]
    fn our_yakuman_count_as_won() {
        let tsumo = vec![start(0), hora(1, 1, &[-16_100, 24_200, -8_100])];
        let ron = vec![start(1), hora(0, 2, &[33_000, 0, -32_000])];
        let mangan = vec![start(0), hora(1, 1, &[-4_000, 8_000, -4_000])];
        assert_eq!(count(&tsumo, Some(1)).won_yakuman, 1);
        assert_eq!(count(&tsumo, Some(1)).yakuman, 0, "ours, not against us");
        assert_eq!(count(&ron, Some(0)).won_yakuman, 1);
        assert_eq!(count(&mangan, Some(1)).won_yakuman, 0);
    }

    /// A yakuman we did not pay for, or our own, is not one against us.
    #[test]
    fn only_yakuman_we_paid_for_count() {
        let between_others = vec![start(1), hora(0, 2, &[32_000, 0, -32_000])];
        let ours = vec![start(1), hora(0, 2, &[32_000, 0, -32_000])];
        assert_eq!(count(&between_others, Some(1)).yakuman, 0);
        assert_eq!(count(&ours, Some(0)).yakuman, 0);
    }

    /// Two wins summed into one payer: the winner's gain bounds the cost, so
    /// a sanbaiman and a mangan do not add up to a yakuman.
    #[test]
    fn a_double_ron_is_read_per_winner() {
        let game = vec![start(1), hora(2, 1, &[8_000, -32_000, 24_000])];
        assert_eq!(count(&game, Some(1)).yakuman, 0);
    }

    #[test]
    fn an_observed_game_adds_nothing_but_is_read() {
        let game = vec![start(0), pon(0, 1), hora(0, 1, &[32_000, -32_000, 0])];
        let s = count(&game, None);
        assert_eq!((s.games, s.pon, s.chi, s.yakuman), (1, 0, 0, 0));
    }
}
