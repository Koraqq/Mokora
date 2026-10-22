//! How long each round (局) takes, measured live as a game is recorded.
//!
//! A round runs from its deal (`start_kyoku`) to its end (`end_kyoku`, sent
//! the moment someone wins or the wall runs out). The recorder sees each event
//! as it arrives, so the wall-clock gap between the two is the round's length
//! — the mjai stream itself carries no times, so this is the only place it
//! can be had.
//!
//! A reconnect replays the round in progress from its start in one burst, so
//! that round's start is not when it really began: the round a reconnect lands
//! in is not measured, and the ones after it are.

use crate::schema::history::RoundTimes;
use crate::schema::MjaiEvent;
use std::time::{Duration, Instant};

/// Shorter than any real round (the deal alone animates for longer): a round
/// this short is a replay.
pub const MIN_ROUND: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
pub struct RoundClock {
    game_id: Option<u64>,
    started: bool,
    /// When the round in progress was dealt, if it is being measured.
    round_start: Option<Instant>,
    /// A reconnect replayed the round in progress: leave it unmeasured.
    after_reconnect: bool,
    count: u32,
    total_ms: u64,
    min_ms: u64,
    max_ms: u64,
}

impl RoundClock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Take in one event, as it arrives at `now`.
    pub fn on_event(&mut self, ev: &MjaiEvent, now: Instant) {
        match ev {
            MjaiEvent::StartGame { game_meta, .. } => {
                let game_id = game_meta.as_ref().and_then(|m| m.game_id);
                if game_id.is_some() && game_id == self.game_id && self.started {
                    // The same table again: a reconnect. Keep what was measured.
                    self.after_reconnect = true;
                    self.round_start = None;
                    return;
                }
                self.reset();
                self.game_id = game_id;
                self.started = true;
            }
            MjaiEvent::StartKyoku { .. } => {
                self.round_start = (!self.after_reconnect).then_some(now);
            }
            MjaiEvent::EndKyoku => {
                self.after_reconnect = false;
                if let Some(start) = self.round_start.take() {
                    let length = now.saturating_duration_since(start);
                    if length >= MIN_ROUND {
                        self.note(length);
                    }
                }
            }
            _ => {}
        }
    }

    fn note(&mut self, length: Duration) {
        let ms = u64::try_from(length.as_millis()).unwrap_or(u64::MAX);
        self.min_ms = if self.count == 0 {
            ms
        } else {
            self.min_ms.min(ms)
        };
        self.max_ms = self.max_ms.max(ms);
        self.total_ms = self.total_ms.saturating_add(ms);
        self.count += 1;
    }

    /// The game's round times, if any round was measured.
    pub fn finish(&self) -> Option<RoundTimes> {
        (self.count > 0).then_some(RoundTimes {
            count: self.count,
            total_ms: self.total_ms,
            min_ms: self.min_ms,
            max_ms: self.max_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::mjai::GameMeta;

    fn start_game(game_id: u64) -> MjaiEvent {
        MjaiEvent::StartGame {
            names: vec![],
            kyoku_first: None,
            aka_flag: None,
            id: Some(0),
            num_players: 3,
            game_meta: Some(GameMeta {
                game_id: Some(game_id),
                match_mode: None,
                match_info: None,
            }),
        }
    }

    fn start_kyoku() -> MjaiEvent {
        MjaiEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "1m".into(),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: vec![35_000; 3],
            tehais: vec![vec![]; 3],
            num_players: 3,
        }
    }

    fn at(t0: Instant, secs: f64) -> Instant {
        t0 + Duration::from_secs_f64(secs)
    }

    #[test]
    fn a_round_runs_from_its_deal_to_its_end() {
        let t0 = Instant::now();
        let mut c = RoundClock::default();
        c.on_event(&start_game(7), t0);
        c.on_event(&start_kyoku(), at(t0, 1.0));
        c.on_event(&MjaiEvent::EndKyoku, at(t0, 91.0));
        c.on_event(&start_kyoku(), at(t0, 100.0));
        c.on_event(&MjaiEvent::EndKyoku, at(t0, 130.0));
        let r = c.finish().unwrap();
        assert_eq!(
            (r.count, r.total_ms, r.min_ms, r.max_ms),
            (2, 120_000, 30_000, 90_000)
        );
        assert_eq!(RoundClock::default().finish(), None);
    }

    #[test]
    fn the_round_a_reconnect_lands_in_is_not_measured() {
        let t0 = Instant::now();
        let mut c = RoundClock::default();
        c.on_event(&start_game(7), t0);
        c.on_event(&start_kyoku(), at(t0, 1.0));
        c.on_event(&MjaiEvent::EndKyoku, at(t0, 61.0)); // 60s: measured
        c.on_event(&start_kyoku(), at(t0, 70.0));
        // Reconnect mid-round: the round replayed from its deal.
        c.on_event(&start_game(7), at(t0, 120.0));
        c.on_event(&start_kyoku(), at(t0, 120.01));
        c.on_event(&MjaiEvent::EndKyoku, at(t0, 150.0)); // not measured
        c.on_event(&start_kyoku(), at(t0, 160.0));
        c.on_event(&MjaiEvent::EndKyoku, at(t0, 200.0)); // 40s: measured
        let r = c.finish().unwrap();
        assert_eq!((r.count, r.min_ms, r.max_ms), (2, 40_000, 60_000));
    }
}
