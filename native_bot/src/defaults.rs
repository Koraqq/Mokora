//! Bundled default model weights + a convenience engine constructor.
//!
//! The trained `.safetensors` are embedded directly into the binary so the
//! built-in bot needs no external files at runtime.

use crate::engine::Engine;

/// 4-player weights (behavior-cloned from Tenhou logs).
pub static WEIGHTS_4P: &[u8] = include_bytes!("../weights/base4p.safetensors");
/// 3-player (sanma) weights.
pub static WEIGHTS_3P: &[u8] = include_bytes!("../weights/base3p.safetensors");
/// Mora: 4-player weights on the V2 feature planes, with a danger head.
/// 4-player only.
pub static WEIGHTS_MORA_4P: &[u8] = include_bytes!("../weights/mora4p.safetensors");
/// Mora: 3-player weights, trained on the V2 feature planes
/// (`obs::Features::V2`: discard order, tedashi, passed tiles, shanten,
/// ukeire…). 3-player only.
pub static WEIGHTS_MORA_3P: &[u8] = include_bytes!("../weights/mora3p.safetensors");

/// Build an [`Engine`] for the given player count, seated at `seat`, using the
/// bundled default weights.
pub fn engine(num_players: u8, seat: u8) -> anyhow::Result<Engine> {
    let bytes = if num_players == 3 {
        WEIGHTS_3P
    } else {
        WEIGHTS_4P
    };
    Engine::new(bytes.to_vec(), num_players, seat)
}

/// Build a 3-player [`Engine`] seated at `seat` using Mora's weights. The
/// engine feeds them the V2 planes (read off their input width).
pub fn mora_engine_3p(seat: u8) -> anyhow::Result<Engine> {
    Engine::new(WEIGHTS_MORA_3P.to_vec(), 3, seat)
}

/// Build a 4-player [`Engine`] seated at `seat` using Mora's weights. The
/// engine feeds them the V2 planes (read off their input width).
pub fn mora_engine_4p(seat: u8) -> anyhow::Result<Engine> {
    Engine::new(WEIGHTS_MORA_4P.to_vec(), 4, seat)
}

/// Build Yakuman seated at `seat`: Mora's weights with every decision
/// handed to `planner` ([`crate::yakuman`]; the paid module's); Mora plays
/// only the hands where no yakuman is reachable, and reads the danger as
/// usual.
pub fn yakuman_engine_3p(
    seat: u8,
    planner: std::sync::Arc<dyn crate::yakuman::Planner>,
) -> anyhow::Result<Engine> {
    let mut engine = mora_engine_3p(seat)?;
    engine.set_planner(Some(planner));
    Ok(engine)
}

/// [`yakuman_engine_3p`] for yonma: Mora 4p's weights, every decision
/// handed to the planner.
pub fn yakuman_engine_4p(
    seat: u8,
    planner: std::sync::Arc<dyn crate::yakuman::Planner>,
) -> anyhow::Result<Engine> {
    let mut engine = mora_engine_4p(seat)?;
    engine.set_planner(Some(planner));
    Ok(engine)
}
