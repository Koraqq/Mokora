//! The decision the player is waiting on, kept for autoplay that is switched on
//! in the middle of it.
//!
//! A bot answer is planned the moment it arrives, and only if autoplay is on.
//! Switch it on while it is already the player's turn and that answer has gone
//! by: nothing would be pressed until the next decision. So the last answer is
//! kept here, with the decision window it answered, and the manager plays it
//! when it finds autoplay newly on — if that window is still open.

use crate::autoplay::budget::TimeBudget;
use crate::bot::BotResponse;
use crate::event_bus::BotResponseBus;
use crate::autoplay::AutoplayContext;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast::error::RecvError;

/// A bot answer and the decision window it was given in.
#[derive(Debug, Clone)]
pub struct LastDecision {
    pub response: BotResponse,
    /// When that window opened (`TimeBudget::opened_at`): its identity.
    pub opened_at: Instant,
}

/// The answer to play now, if there is one: it answered the window that is
/// still open (the server's budget slot still holds it, and its time is not
/// up), and autoplay has not already played in that window (`handled`).
pub fn resumable(
    last: Option<&LastDecision>,
    window: Option<&TimeBudget>,
    handled: Option<Instant>,
) -> Option<BotResponse> {
    let (last, window) = (last?, window?);
    let open = window.opened_at == last.opened_at
        && window.elapsed_ms() < window.fixed_ms.saturating_add(window.add_ms);
    (open && handled != Some(last.opened_at)).then(|| last.response.clone())
}

/// Keep the newest bot answer in `ctx.last_decision`, for as long as the
/// account runs. An answer with no decision window open (off-Majsoul, between
/// hands) is not kept.
pub async fn record(ctx: Arc<AutoplayContext>, bus: BotResponseBus) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(response) => {
                let window = ctx.time_budget.read().ok().and_then(|g| *g);
                let kept = window.map(|w| LastDecision {
                    response,
                    opened_at: w.opened_at,
                });
                if let Ok(mut slot) = ctx.last_decision.lock() {
                    *slot = kept;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoplay::budget::BudgetSource;
    use crate::schema::MjaiEvent;
    use std::time::Duration;

    fn window(opened_ago: Duration, fixed_ms: u32, add_ms: u32) -> TimeBudget {
        TimeBudget {
            fixed_ms,
            add_ms,
            opened_at: Instant::now() - opened_ago,
            source: BudgetSource::DealTile,
        }
    }

    fn answer(w: &TimeBudget) -> LastDecision {
        LastDecision {
            response: BotResponse {
                action: MjaiEvent::None,
                meta: None,
            },
            opened_at: w.opened_at,
        }
    }

    #[test]
    fn the_answer_to_the_open_window_is_played() {
        let w = window(Duration::from_secs(2), 5_000, 20_000);
        assert!(resumable(Some(&answer(&w)), Some(&w), None).is_some());
    }

    #[test]
    fn an_answer_to_an_earlier_window_is_not() {
        let old = window(Duration::from_secs(9), 5_000, 20_000);
        let now = window(Duration::from_secs(1), 5_000, 20_000);
        assert!(resumable(Some(&answer(&old)), Some(&now), None).is_none());
    }

    #[test]
    fn nothing_is_played_once_the_window_has_closed_or_run_out() {
        let w = window(Duration::from_secs(2), 5_000, 20_000);
        assert!(resumable(Some(&answer(&w)), None, None).is_none(), "no window open");
        let late = window(Duration::from_secs(30), 5_000, 20_000);
        assert!(resumable(Some(&answer(&late)), Some(&late), None).is_none(), "time is up");
        assert!(resumable(None, Some(&w), None).is_none(), "nothing was answered");
    }

    #[test]
    fn a_window_autoplay_already_played_in_is_not_played_again() {
        let w = window(Duration::from_secs(2), 5_000, 20_000);
        assert!(resumable(Some(&answer(&w)), Some(&w), Some(w.opened_at)).is_none());
    }
}
