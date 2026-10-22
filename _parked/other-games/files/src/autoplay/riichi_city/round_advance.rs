//! Riichi City round advance: after each hand's settlement the client waits
//! on a "ready" (`req_user_prepare`) before dealing the next round. Majsoul
//! and Tenhou have no equivalent step, so this lives with the Riichi City
//! autoplay rather than in the platform-agnostic manager.
//!
//! The advance is timed off the protocol (`EndKyoku`) with no screen
//! capture: wait a human-like delay past the settlement animation, then
//! inject the ready frame — aborting if the round moves on first.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::autoplay::inject::{InjectFrame, SharedInjectBus};
use crate::bridge::riichi_city::build;
use crate::config::{AppConfig, Platform};
use crate::event_bus::MjaiBus;
use crate::schema::MjaiEvent;

async fn autoplay_enabled_for_riichi(cfg: &Arc<RwLock<AppConfig>>) -> bool {
    let guard = cfg.read().await;
    guard.autoplay.enabled && guard.platform.kind == Platform::RiichiCity
}

/// Dedicated round-advance loop, on its own `MjaiBus` subscription so
/// end-of-hand plan backlogs cannot delay it. Advancing past the scoring
/// screen is one `req_user_prepare` per `EndKyoku`, sent after a human-like
/// delay timed off the protocol event itself — no screen capture. The delay
/// covers the score breakdown rendering plus a reading beat, and stays well
/// under the client's own ~59s auto-advance countdown.
///
/// If any further mjai event arrives before the delay elapses, the round
/// already moved on (the server countdown fired, the user clicked through,
/// or the game ended with `cmd_room_end`/`EndGame`) — the cycle is voided so
/// a stale advance never lands. `EndGame` needs nothing on its own: the
/// client tears its end screens down when the next match's `cmd_enter_room`
/// arrives.
pub async fn round_advance_watcher(
    cfg: Arc<RwLock<AppConfig>>,
    inject: SharedInjectBus,
    bus: MjaiBus,
) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(ev) => match ev {
                MjaiEvent::EndKyoku => {
                    if !autoplay_enabled_for_riichi(&cfg).await {
                        continue;
                    }
                    let (_, yakus) = inject.settlement();
                    tokio::select! {
                        _ = tokio::time::sleep(round_advance_delay(yakus)) => {
                            info!("autoplay: sending round-advance (req_user_prepare)");
                            if !inject.send(InjectFrame {
                                gameplay: true,
                                bytes: build::user_prepare(),
                            }) {
                                warn!("autoplay: no injection relay for the round-advance press");
                            }
                        }
                        msg = rx.recv() => match msg {
                            // The round already moved on (server countdown,
                            // manual click, or game over) before our delay
                            // elapsed — void this cycle.
                            Ok(_) | Err(RecvError::Lagged(_)) => {}
                            Err(RecvError::Closed) => return,
                        }
                    }
                }
                MjaiEvent::EndGame { .. } => {}
                _ => {}
            },
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return,
        }
    }
}

/// Human-like wait before injecting the settlement OK (`req_user_prepare`),
/// timed off `EndKyoku` with no screen capture. A 10s floor first, because
/// the settlement animation (win reveal, score tally, ura-dora flip) plays
/// out over several seconds after `cmd_game_end` and the OK press must land
/// after it, not during. On top of the floor: uniform jitter so the timing
/// is not a fixed signature, plus a reading beat that grows with the number
/// of yaku lines in the winning hand (each line past two adds half a second,
/// capped; draws add nothing). The total stays well below the client's ~59s
/// auto-advance countdown, so we press before it, never after.
fn round_advance_delay(yakus: u32) -> Duration {
    /// Floor covering the settlement animation; the press never lands sooner.
    const FLOOR_MS: u64 = 15_000;
    /// Uniform jitter added on top of the floor.
    const JITTER_MS: u64 = 2_000;
    /// Extra reading time for bigger hands, capped.
    const READ_CAP_MS: u64 = 2_500;

    let jitter = rand::random::<u64>() % (JITTER_MS + 1);
    let read = (u64::from(yakus.saturating_sub(2)) * 500).min(READ_CAP_MS);
    Duration::from_millis(FLOOR_MS + jitter + read)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The round-advance delay = a 15s animation floor + jitter [0,2000] + a
    /// reading beat that is zero for ≤2 yaku lines and grows half a second
    /// per extra line (capped at 2500). Randomized, so assert the bounds over
    /// samples, that nothing ever fires under the 15s floor, that more yaku
    /// lines shift the window up, and that everything stays under the
    /// client's ~59s countdown.
    #[test]
    fn round_advance_delay_bounds_scale_with_yaku_count() {
        let bounds = |yakus: u32| -> (u64, u64) {
            let read = (u64::from(yakus.saturating_sub(2)) * 500).min(2_500);
            (15_000 + read, 17_000 + read)
        };
        for yakus in [0u32, 2, 3, 5, 9, 20] {
            let (lo, hi) = bounds(yakus);
            for _ in 0..200 {
                let ms = round_advance_delay(yakus).as_millis() as u64;
                assert!(
                    (lo..=hi).contains(&ms),
                    "yakus={yakus}: {ms} not in [{lo},{hi}]"
                );
                assert!(
                    ms >= 15_000,
                    "yakus={yakus}: {ms} under the animation floor"
                );
            }
            assert!(
                hi < 59_000,
                "yakus={yakus}: must fire before the server countdown"
            );
        }
        // A draw (0 yaku) reads faster than a big hand.
        assert!(bounds(0).1 < bounds(9).0);
    }
}
