//! The `V2` planes must come out the same whichever way the game state was
//! built. The `arena` lets riichienv-core step the game itself; the extractor
//! and live play drive a state with mjai events instead, and riichienv-core
//! records different things on the two paths (tedashi, the riichi-declaring
//! discard, passed wins). This plays seeded sanma games on the stepping path,
//! replays their mjai log on the event path, and compares the V2 planes at
//! every decision.

use native_bot::adapt::{obs_and_legal_3p_with, sync_timeline_3p};
use native_bot::mjai_compat::parse_line;
use native_bot::obs::{channels, Features, GameLength};
use native_bot::tiles::tile_dim;
use native_bot::timeline::DiscardTimeline;
use riichienv_core::rule::GameRule;
use riichienv_core::state_3p::GameState3P;
use std::collections::HashMap;

/// A decision seen on the stepping path: after `events` mjai log lines,
/// `seat` saw `planes`.
struct Seen {
    events: usize,
    seat: u8,
    planes: Vec<f32>,
}

fn v2_planes(buf: &[f32]) -> Vec<f32> {
    buf[channels(3) * tile_dim(3)..].to_vec()
}

/// Play one east game with a seeded pseudo-random policy; return its mjai log
/// and every decision's V2 planes.
fn play(seed: u64) -> (Vec<String>, Vec<Seen>) {
    let mut state = GameState3P::new(4, false, Some(seed), 0, GameRule::default_mjsoul());
    let mut timeline = DiscardTimeline::new();
    let mut rng = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut seen = Vec::new();
    for _ in 0..20_000 {
        if state.is_done {
            return (state.mjai_log.clone(), seen);
        }
        let mut actions = HashMap::new();
        for seat in 0..3u8 {
            let (obs, legal) = obs_and_legal_3p_with(
                &mut state,
                seat,
                Features::V2,
                &timeline,
                GameLength::default(),
            );
            if legal.is_empty() {
                continue;
            }
            seen.push(Seen {
                events: state.mjai_log.len(),
                seat,
                planes: v2_planes(&obs),
            });
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let pick = (rng >> 33) as usize % legal.len();
            actions.insert(seat, legal[pick].clone());
        }
        state.step(&actions);
        sync_timeline_3p(&mut timeline, &state);
    }
    panic!("seed {seed}: game did not finish");
}

#[test]
fn v2_planes_match_between_the_stepping_and_event_paths() {
    let mut compared = 0;
    for seed in 0..20u64 {
        let (log, seen) = play(seed);
        let mut state = GameState3P::new(0, true, None, 0, GameRule::default_mjsoul());
        let mut timeline = DiscardTimeline::new();
        let mut applied = 0;
        for s in &seen {
            while applied < s.events {
                if let Some(ev) = parse_line(&log[applied], 3) {
                    timeline.note_mjai(&ev);
                    state.apply_mjai_event(ev);
                    sync_timeline_3p(&mut timeline, &state);
                }
                applied += 1;
            }
            let (obs, _) = obs_and_legal_3p_with(
                &mut state,
                s.seat,
                Features::V2,
                &timeline,
                GameLength::default(),
            );
            let planes = v2_planes(&obs);
            if planes != s.planes {
                let t = tile_dim(3);
                let ch = (0..planes.len() / t)
                    .find(|c| planes[c * t..(c + 1) * t] != s.planes[c * t..(c + 1) * t])
                    .unwrap();
                panic!(
                    "seed {seed}, after {} events, seat {}: V2 plane {ch} differs\n event path: {:?}\n step path:  {:?}\n last event: {}",
                    s.events,
                    s.seat,
                    &planes[ch * t..(ch + 1) * t],
                    &s.planes[ch * t..(ch + 1) * t],
                    log[s.events.saturating_sub(1)],
                );
            }
            compared += 1;
        }
    }
    assert!(compared > 1000, "only {compared} decisions compared");
}
