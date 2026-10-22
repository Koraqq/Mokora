//! Built-in, in-process bot runner backed by the pure-Rust `native_bot`
//! crate (a small behavior-cloned CNN run via candle — no Python, no
//! libriichi, no subprocess).
//!
//! Unlike [`crate::bot::runner::SubprocessBot`], this runner keeps a live
//! `native_bot::Engine` in-process: each `react()` feeds the batch through the
//! engine's riichienv-core game state and, at our decision points, runs the net
//! to pick a legal action. Bundled model weights are embedded in the binary, so
//! there is no venv, no `uv sync`, and nothing to install.
//!
//! Four reserved bot names select it: [`MORA_3P`] (sanma only) and
//! [`MORA_4P`] (yonma only), the models trained on the richer V2 feature planes
//! (see `native_bot/train/README.md`), and [`YAKUMAN_3P`] / [`YAKUMAN_4P`],
//! the same weights with every decision handed to the yakuman planner
//! (`native_bot::yakuman`). `BotManager::spawn_runner` recognises them
//! and constructs this runner directly, bypassing the `bot.py`/registry path.

use crate::bot::runner::BotRunner;
use crate::bot::types::BotResponse;
use crate::game_state::convert;
use crate::schema::{GameMeta, MjaiEvent};
use anyhow::Result;
use async_trait::async_trait;
use native_bot::engine::{BotAction, Decision, Engine};
use native_bot::obs::GameLength;
use serde_json::Value;
use tracing::warn;

/// Former name of the original 4-player bot, no longer offered: it is now
/// answered by [`MORA_4P`] (see `LEGACY_NAMES`).
pub const NATIVE_4P: &str = "base-4p";
/// Former name of the original sanma bot, no longer offered: it is now
/// answered by [`MORA_3P`] (see `LEGACY_NAMES`).
pub const NATIVE_3P: &str = "base-3p";
/// Reserved name for Mora, the sanma bot trained on the V2 feature planes
/// (`native_bot::obs::Features::V2`). 3-player only.
pub const MORA_3P: &str = "mora-3p";
/// Reserved name for Mora's yonma model: the same V2 planes for four players,
/// with a danger head. 4-player only.
pub const MORA_4P: &str = "mora-4p";
/// Reserved name for Yakuman: Mora's weights playing every hand for a
/// yakuman (`native_bot::yakuman`); Mora decides only the hands where none is
/// reachable. 3-player only.
pub const YAKUMAN_3P: &str = "yakuman-3p";
/// Reserved name for Yakuman's yonma side: Mora 4p's weights with the same
/// planner. 4-player only.
pub const YAKUMAN_4P: &str = "yakuman-4p";

/// The names the built-in bots went by before they were named
/// `<name>-3p` / `<name>-4p`, and what each is called now. Configs and game
/// records written before still carry them.
const LEGACY_NAMES: [(&str, &str); 8] = [
    (NATIVE_3P, MORA_3P),
    ("mokora-3p", MORA_3P),
    ("mokora-native3p", MORA_3P),
    ("mora-native3p", MORA_3P),
    ("yakuman-native3p", YAKUMAN_3P),
    (NATIVE_4P, MORA_4P),
    ("mokora-4p", MORA_4P),
    ("mokora-native4p", MORA_4P),
];

/// The current name of a built-in bot `name` names by its old name.
pub fn renamed(name: &str) -> Option<&'static str> {
    LEGACY_NAMES
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, now)| *now)
}

/// `name`, or a built-in bot's current name for its old one.
pub fn canonical_name(name: &str) -> &str {
    renamed(name).unwrap_or(name)
}

/// Whether `name` selects the built-in native bot (any of its weights), by
/// its current name or its old one.
pub fn is_native(name: &str) -> bool {
    let name = canonical_name(name);
    builtin_models().iter().any(|(n, _, _)| *n == name)
}

/// Every built-in model: its reserved name, player count and embedded
/// weights, newest first within each player count.
pub fn builtin_models() -> [(&'static str, u8, &'static [u8]); 4] {
    use native_bot::defaults as w;
    [
        (YAKUMAN_3P, 3, w::WEIGHTS_MORA_3P),
        (MORA_3P, 3, w::WEIGHTS_MORA_3P),
        (YAKUMAN_4P, 4, w::WEIGHTS_MORA_4P),
        (MORA_4P, 4, w::WEIGHTS_MORA_4P),
    ]
}

/// FNV-1a (64-bit) of `bytes`, hex. A cheap fingerprint, not a defence.
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The fingerprint of a built-in model's weights; `None` for another name.
/// The weights of the built-in model `name` (embedded in Mokora).
pub fn weights_of(name: &str) -> Option<&'static [u8]> {
    let name = canonical_name(name);
    builtin_models()
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, bytes)| *bytes)
}

pub fn weights_hash(name: &str) -> Option<String> {
    let name = canonical_name(name);
    builtin_models()
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, bytes)| fingerprint(bytes))
}

/// The built-in models a game can be reviewed with: every one but Yakuman,
/// whose planner plays by something other than the model's policy.
pub fn review_models() -> Vec<(&'static str, u8)> {
    builtin_models()
        .iter()
        .filter(|(n, _, _)| !is_yakuman(n))
        .map(|(n, players, _)| (*n, *players))
        .collect()
}

/// Whether `name` is one of Yakuman's sides.
pub fn is_yakuman(name: &str) -> bool {
    matches!(canonical_name(name), YAKUMAN_3P | YAKUMAN_4P)
}

/// The bot that plays for `name` under the license now. Yakuman is a paid
/// feature, its planner in the paid module: without both its side's Mora
/// plays (the same weights, without the planner). Every other name is itself.
pub fn in_force(name: &str) -> &str {
    in_force_with(name, crate::pro::available())
}

fn in_force_with(name: &str, paid: bool) -> &str {
    match canonical_name(name) {
        YAKUMAN_3P if !paid => MORA_3P,
        YAKUMAN_4P if !paid => MORA_4P,
        _ => name,
    }
}

/// The model a game of `num_players` is reviewed with unless another is
/// picked: the strongest built-in one of that player count.
pub fn default_review_model(num_players: u8) -> &'static str {
    if num_players == 3 {
        MORA_3P
    } else {
        MORA_4P
    }
}

/// Display label for a reserved native-bot name, for the Bots UI.
pub fn display_name(name: &str) -> Option<&'static str> {
    match canonical_name(name) {
        MORA_3P => Some("Mora (built-in, 3p)"),
        MORA_4P => Some("Mora (built-in, 4p)"),
        YAKUMAN_3P => Some("Yakuman (built-in, 3p)"),
        YAKUMAN_4P => Some("Yakuman (built-in, 4p)"),
        _ => None,
    }
}

/// The engine a reserved name runs. Each built-in model is for one player
/// count, so asking for it in the other is refused rather than quietly answered
/// by another model.
pub(crate) fn engine_for(name: &str, num_players: u8, actor_id: u8) -> Result<Engine> {
    use native_bot::defaults as d;
    let name = canonical_name(name);
    let (players, make): (u8, fn(u8) -> Result<Engine>) = match name {
        MORA_3P => (3, d::mora_engine_3p),
        YAKUMAN_3P => (3, yakuman_3p),
        MORA_4P => (4, d::mora_engine_4p),
        YAKUMAN_4P => (4, yakuman_4p),
        _ => anyhow::bail!("{name} is not a built-in bot"),
    };
    if num_players != players {
        let other = 7 - players;
        anyhow::bail!(
            "{name} is {players}-player only — pick a {other}-player bot (or none) under Settings → Bots"
        );
    }
    make(actor_id)
}

/// Yakuman with the paid module's planner; an error without it (the callers
/// ask [`in_force`] first, so this is not reached then).
fn yakuman_3p(seat: u8) -> Result<Engine> {
    let planner = crate::pro::planner().ok_or_else(|| anyhow::anyhow!("Yakuman needs the paid module"))?;
    native_bot::defaults::yakuman_engine_3p(seat, planner)
}

fn yakuman_4p(seat: u8) -> Result<Engine> {
    let planner = crate::pro::planner().ok_or_else(|| anyhow::anyhow!("Yakuman needs the paid module"))?;
    native_bot::defaults::yakuman_engine_4p(seat, planner)
}

/// Construct the built-in bot runner `name` (one of the reserved names) for a
/// game of `num_players` seated at `actor_id`.
pub async fn build(name: &str, actor_id: u8, num_players: u8) -> Result<Box<dyn BotRunner>> {
    Ok(Box::new(NativeBot::new(in_force(name), actor_id, num_players)?))
}

pub struct NativeBot {
    engine: Engine,
    seat: u8,
    /// The table the engine was built for; an event that does not fit it is
    /// refused rather than fed (see `convert::check_fits`).
    num_players: u8,
}

impl NativeBot {
    /// Build the in-process bot `name` for a game of `num_players` with our
    /// seat at `actor_id`, loading the bundled weights that name selects.
    pub fn new(name: &str, actor_id: u8, num_players: u8) -> Result<Self> {
        let engine = engine_for(name, num_players, actor_id)?;
        Ok(Self {
            engine,
            seat: actor_id,
            num_players,
        })
    }
}

/// The planned length of a game from its room settings: Mahjong Soul's
/// `match_mode` is 1 / 11 for east-only (4p / 3p), 2 / 12 for hanchan.
/// Anything else — another platform, a mode we do not know — is taken for a
/// hanchan, what the models mostly learned from.
fn game_length(meta: Option<&GameMeta>) -> GameLength {
    match meta.and_then(|m| m.match_mode) {
        Some(1 | 11) => GameLength::East,
        _ => GameLength::EastSouth,
    }
}

#[async_trait]
impl BotRunner for NativeBot {
    async fn react(&mut self, events: &[MjaiEvent]) -> Result<BotResponse> {
        for ev in events {
            // Keep our seat current if a start_game tags a (possibly new) seat.
            if let MjaiEvent::StartGame { id: Some(seat), .. } = ev {
                self.seat = *seat;
                self.engine.set_seat(*seat);
            }
            // East-only or hanchan, for weights that read the rounds left.
            if let MjaiEvent::StartGame { game_meta, .. } = ev {
                self.engine.set_game_length(game_length(game_meta.as_ref()));
            }
            // The engine panics on a seat past the table or a list of the
            // wrong length; such an event belongs to another game.
            if let Err(e) = convert::check_fits(ev, self.num_players) {
                warn!("native bot: {e:#}; event not fed: {ev:?}");
                continue;
            }
            if let Some(ri) = convert::to_riichienv(ev)? {
                engine_step(|| self.engine.feed(ri))?;
            }
        }

        // These events close or postpone a decision; they do not open one.
        // BotManager normally buffers them, but guard direct BotRunner users as
        // well because riichienv may retain the preceding legal-action set.
        let waits_for_rinshan = matches!(
            events.last(),
            Some(
                MjaiEvent::Daiminkan { actor, .. }
                    | MjaiEvent::Ankan { actor, .. }
                    | MjaiEvent::Kakan { actor, .. }
            ) if *actor == self.seat
        );
        if matches!(events.last(), Some(MjaiEvent::ReachAccepted { .. })) || waits_for_rinshan {
            return Ok(BotResponse {
                action: MjaiEvent::None,
                meta: None,
            });
        }

        // Nothing to decide ⇒ reply `none` and leave whatever card is on
        // screen alone.
        let local = match engine_step(|| self.engine.decide())?? {
            Some(d) if is_decision_point(&d.candidates) => d,
            _ => {
                return Ok(BotResponse {
                    action: MjaiEvent::None,
                    meta: None,
                })
            }
        };
        let (action, meta) = local_reply(&local, self.seat);
        // The danger head's read of the table, for the danger tile.
        let danger = engine_step(|| self.engine.danger())?.unwrap_or_else(|e| {
            warn!("native bot: danger head failed: {e:#}");
            None
        });
        Ok(BotResponse {
            action,
            meta: with_danger(meta, danger),
        })
    }

    async fn reset(&mut self) -> Result<()> {
        self.engine.reset();
        Ok(())
    }
}

/// Did the bot actually choose anything here?
///
/// This is the precondition for touching the HUD card: it must change when the
/// bot made a choice, and *only* then. A real decision has something to choose
/// between — our own turn (a discard, riichi, kan, tsumo…), or a call window
/// where a pon / chi / kan / ron sits next to the pass.
///
/// Two things are not decisions. An empty legal set, obviously. And a legal set
/// of exactly `[Pass]`: the engine offers `Pass` unconditionally in its response
/// phase, so a seat standing in someone *else's* call window still gets one
/// handed back. Rendering that as "Pass 100%" would replace real advice with a
/// choice the bot never had.
///
/// The bare-`[Pass]` case is not reachable in live play today — the bridge feeds
/// opponents' hands as `?`, so the engine can only ever see *our* claims, and it
/// only opens a response window when a claim exists, which means the pass always
/// arrives next to the call it was weighed against. But that is a fact about
/// what the engine currently knows, not about what the card promises. Encode the
/// promise.
/// Run one engine call, turning a panic into an error.
///
/// The engine indexes its tables without bounds checks; `check_fits` keeps
/// the known causes out, and this keeps an unknown one from ending the bot
/// manager's task (which would leave the bot silent until a restart). The
/// error makes the manager rebuild the bot and replay the game into it, so
/// the half-updated engine is never asked again.
fn engine_step<T>(step: impl FnOnce() -> T) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(step))
        .map_err(|_| anyhow::anyhow!("the native bot engine failed (see the panic in the log)"))
}

pub(crate) fn is_decision_point(candidates: &[(BotAction, f32)]) -> bool {
    !matches!(candidates, [] | [(BotAction::Pass, _)])
}

/// `meta` with the danger head's probabilities under `danger`:
/// `{"tiles": ["1m", …], "opponents": [[p, …], …]}`, one row per opponent
/// (shimocha first), one probability per tile. `meta` as is without them.
fn with_danger(meta: Option<Value>, danger: Option<Vec<Vec<(String, f32)>>>) -> Option<Value> {
    let Some(danger) = danger.filter(|d| !d.is_empty()) else {
        return meta;
    };
    let tiles: Vec<&str> = danger[0].iter().map(|(t, _)| t.as_str()).collect();
    let opponents: Vec<Vec<f32>> = danger
        .iter()
        .map(|row| row.iter().map(|&(_, p)| p).collect())
        .collect();
    let mut meta = match meta {
        Some(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    meta.insert(
        "danger".into(),
        serde_json::json!({ "tiles": tiles, "opponents": opponents }),
    );
    Some(Value::Object(meta))
}

/// The reply pair (mjai action + HUD card) from the model's decision.
fn local_reply(local: &Decision, seat: u8) -> (MjaiEvent, Option<Value>) {
    let meta = build_show_meta(&local.candidates);
    (bot_action_to_mjai(local.action.clone(), seat), meta)
}

/// Prepend `lead` to `rest`, forming a meld's tile list (called tile first).
fn with_lead(lead: &str, rest: &[String]) -> Vec<String> {
    std::iter::once(lead.to_string())
        .chain(rest.iter().cloned())
        .collect()
}

/// Card title of the HUD "Bot Show" tile.
const SHOW_TITLE_LOCAL: &str = "Mokora · Local";

/// Row label for declining a call. "None" — the mjai wire word — is
/// accurate and useless: on a call window what the player wants to read is
/// "Pass 87%" against "Pon 13%", not "None 87%".
const PASS_LABEL: &str = "Pass";

/// Build one `show.items` entry: a label, optional tiles, optional probability
/// (rendered as a whole-percent string).
fn make_show_item(label: &str, pais: &[String], prob: Option<f64>) -> Value {
    use serde_json::json;
    let mut item = serde_json::Map::new();
    item.insert("label".into(), json!(label));
    if pais.iter().any(|p| !p.is_empty()) {
        item.insert("pais".into(), json!(pais));
    }
    if let Some(p) = prob {
        item.insert("value".into(), json!(format!("{:.0}%", p * 100.0)));
        // Raw probability for machine consumers (`autoplay::delay::probs`);
        // the formatted `value` above only has integer-percent resolution.
        item.insert("prob".into(), json!(p));
    }
    Value::Object(item)
}

/// Wrap `items` in the `{ "show": { title, items } }` envelope, or `None` when
/// there is nothing to show. The title names the inference source (see
/// [`SHOW_TITLE_LOCAL`] / [`api_show_title`]); the frontend renders it as the
/// Bot Show tile's header.
fn wrap_show(items: Vec<Value>, title: &str) -> Option<Value> {
    use serde_json::json;
    if items.is_empty() {
        return None;
    }
    Some(json!({ "show": { "title": title, "items": items } }))
}

fn take_n<const N: usize>(v: Vec<String>) -> [String; N] {
    let mut it = v.into_iter();
    std::array::from_fn(|_| it.next().unwrap_or_default())
}

/// Build the HUD "Bot Show" recommendation card (`meta.show`) from the ranked
/// candidates, so the built-in bot surfaces its **top-N** suggestions (each with
/// its policy probability) like other bots.
///
/// Every candidate becomes a row, the pass included. Declining a call *is* the
/// decision on a call window, and its probability is the most interesting number
/// on screen at that moment — "Pass 87% / Pon 13%" is the answer to "should I
/// have ponned?". Suppressing it left the previous turn's discard advice sitting
/// there, reading as live advice for a decision that was already over.
///
/// Callers must have established a decision point first ([`is_decision_point`]);
/// a lone pass is not one, and must not reach here.
fn build_show_meta(candidates: &[(BotAction, f32)]) -> Option<serde_json::Value> {
    let items: Vec<Value> = candidates
        .iter()
        .map(|(a, p)| {
            let (label, pais) = label_pais_bot_action(a);
            make_show_item(label, &pais, Some(*p as f64))
        })
        .collect();
    wrap_show(items, SHOW_TITLE_LOCAL)
}

/// Label + tiles for one bot action.
fn label_pais_bot_action(a: &BotAction) -> (&'static str, Vec<String>) {
    match a {
        BotAction::Dahai { pai, .. } => ("Discard", vec![pai.clone()]),
        BotAction::Reach { pai } => (
            "Riichi",
            if pai.is_empty() {
                vec![]
            } else {
                vec![pai.clone()]
            },
        ),
        BotAction::Pon { pai, consumed, .. } => ("Pon", with_lead(pai, consumed)),
        BotAction::Chi { pai, consumed, .. } => ("Chi", with_lead(pai, consumed)),
        BotAction::Daiminkan { pai, consumed, .. } => ("Kan", with_lead(pai, consumed)),
        BotAction::Ankan { consumed } => ("Ankan", consumed.clone()),
        BotAction::Kakan { pai, consumed } => ("Kakan", with_lead(pai, consumed)),
        BotAction::Hora { .. } => ("Hora", vec![]),
        BotAction::Kyushu => ("Ryukyoku", vec![]),
        BotAction::Kita => ("Kita", vec!["N".into()]),
        // No tiles: the row is about *declining* the discard on the table, and
        // drawing that tile here would read as a recommendation to play it.
        BotAction::Pass => (PASS_LABEL, vec![]),
    }
}

/// Map a schema-agnostic [`BotAction`] to Mokora's `MjaiEvent` reply.
fn bot_action_to_mjai(a: BotAction, actor: u8) -> MjaiEvent {
    match a {
        BotAction::Dahai { pai, tsumogiri } => MjaiEvent::Dahai {
            actor,
            pai,
            tsumogiri,
        },
        BotAction::Reach { pai } => MjaiEvent::Reach {
            actor,
            pai: Some(pai),
        },
        BotAction::Pon {
            target,
            pai,
            consumed,
        } => MjaiEvent::Pon {
            actor,
            target,
            pai,
            consumed: take_n(consumed),
        },
        BotAction::Chi {
            target,
            pai,
            consumed,
        } => MjaiEvent::Chi {
            actor,
            target,
            pai,
            consumed: take_n(consumed),
        },
        BotAction::Daiminkan {
            target,
            pai,
            consumed,
        } => MjaiEvent::Daiminkan {
            actor,
            target,
            pai,
            consumed: take_n(consumed),
        },
        BotAction::Ankan { consumed } => MjaiEvent::Ankan {
            actor,
            consumed: take_n(consumed),
        },
        BotAction::Kakan { pai, consumed } => MjaiEvent::Kakan {
            actor,
            pai,
            consumed: take_n(consumed),
        },
        BotAction::Hora { target } => MjaiEvent::Hora {
            actor,
            target,
            deltas: None,
            ura_markers: None,
        },
        BotAction::Kyushu => MjaiEvent::Ryukyoku { deltas: None },
        BotAction::Kita => MjaiEvent::Kita {
            actor,
            pai: Some("N".into()),
        },
        BotAction::Pass => MjaiEvent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_panic_becomes_an_error() {
        assert_eq!(engine_step(|| 5).unwrap(), 5);
        let err = engine_step(|| -> u8 { panic!("index out of bounds") }).unwrap_err();
        assert!(format!("{err:#}").contains("engine failed"));
    }

    /// Mora too is 3-player only.
    #[test]
    fn danger_rides_along_in_the_meta() {
        let danger = vec![
            vec![("1m".to_string(), 0.5f32), ("9m".to_string(), 0.0)],
            vec![("1m".to_string(), 0.25f32), ("9m".to_string(), 1.0)],
        ];
        let meta = with_danger(Some(serde_json::json!({"show": 1})), Some(danger)).unwrap();
        assert_eq!(meta["show"], 1, "what was there stays");
        assert_eq!(meta["danger"]["tiles"], serde_json::json!(["1m", "9m"]));
        assert_eq!(
            meta["danger"]["opponents"][1],
            serde_json::json!([0.25, 1.0])
        );
        assert_eq!(with_danger(None, None), None);
        assert!(with_danger(None, Some(vec![vec![("1m".into(), 0.1)]])).is_some());
    }

    #[test]
    fn the_room_mode_names_the_game_length() {
        let meta = |mode: u8| GameMeta {
            match_mode: Some(mode),
            ..Default::default()
        };
        assert_eq!(game_length(Some(&meta(11))), GameLength::East);
        assert_eq!(game_length(Some(&meta(1))), GameLength::East);
        assert_eq!(game_length(Some(&meta(12))), GameLength::EastSouth);
        assert_eq!(game_length(Some(&meta(2))), GameLength::EastSouth);
        assert_eq!(game_length(None), GameLength::EastSouth);
    }

    /// Every reserved name is listed once, and every list entry is one.
    #[test]
    fn builtin_models_cover_every_native_name() {
        let names: Vec<&str> = builtin_models().iter().map(|(n, _, _)| *n).collect();
        for name in [MORA_3P, MORA_4P, YAKUMAN_3P, YAKUMAN_4P] {
            assert_eq!(names.iter().filter(|n| **n == name).count(), 1, "{name}");
        }
        assert!(names.iter().all(|n| is_native(n)));
    }

    /// Mora 4p is the yonma model: refused in a sanma game.
    #[test]
    fn mora_4p_is_four_player_only() {
        assert!(is_native(MORA_4P));
        assert_eq!(display_name(MORA_4P), Some("Mora (built-in, 4p)"));
        assert!(engine_for(MORA_4P, 4, 0).is_ok());
        let err = engine_for(MORA_4P, 3, 0).err().expect("3p must be refused");
        assert!(err.to_string().contains("4-player only"), "{err}");
    }

    #[test]
    fn mora_is_three_player_only() {
        assert!(is_native(MORA_3P));
        assert_eq!(display_name(MORA_3P), Some("Mora (built-in, 3p)"));
        assert!(engine_for(MORA_3P, 3, 0).is_ok());
        let err = engine_for(MORA_3P, 4, 0).err().expect("4p must be refused");
        assert!(err.to_string().contains("3-player only"), "{err}");
    }

    /// The old names still select the same bots, and say what they are now.
    #[test]
    fn the_old_names_still_select_the_bots() {
        assert_eq!(renamed("mora-native3p"), Some(MORA_3P));
        assert_eq!(renamed("mokora-native3p"), Some(MORA_3P));
        // The base and Mokora sanma bots are gone; their names land on Mora.
        assert_eq!(renamed("mokora-3p"), Some(MORA_3P));
        assert_eq!(renamed("base-3p"), Some(MORA_3P));
        assert!(is_native("mokora-3p") && is_native("base-3p"));
        // The yonma ones land on Mora 4p.
        for old in ["base-4p", "mokora-4p", "mokora-native4p"] {
            assert_eq!(renamed(old), Some(MORA_4P), "{old}");
            assert!(is_native(old), "{old}");
        }
        assert_eq!(renamed(MORA_3P), None, "a current name is not renamed");
        assert_eq!(renamed("my-bot"), None);
        assert!(is_native("mokora-native3p"));
        assert_eq!(display_name("mora-native3p"), Some("Mora (built-in, 3p)"));
        assert!(engine_for("mora-native3p", 3, 0).is_ok());
        assert!(LEGACY_NAMES
            .iter()
            .all(|(old, now)| renamed(old) == Some(now)));
    }

    /// Without the paid feature Yakuman's sides are played by Mora's.
    #[test]
    fn yakuman_is_mora_without_the_paid_feature() {
        assert_eq!(in_force_with(YAKUMAN_3P, false), MORA_3P);
        assert_eq!(in_force_with("yakuman-native3p", false), MORA_3P);
        assert_eq!(in_force_with(YAKUMAN_4P, false), MORA_4P);
        assert_eq!(in_force_with(YAKUMAN_3P, true), YAKUMAN_3P);
        assert_eq!(in_force_with(MORA_4P, false), MORA_4P);
        assert_eq!(in_force_with("some-python-bot", false), "some-python-bot");
    }

    /// Yakuman is sanma only, and without the paid module (none in tests)
    /// it is not built at all: its planner is the module's. (MokoraPro's
    /// tests build it with the planner.)
    #[test]
    fn yakuman_needs_the_paid_module() {
        assert!(is_native(YAKUMAN_3P));
        assert_eq!(display_name(YAKUMAN_3P), Some("Yakuman (built-in, 3p)"));
        let err = engine_for(YAKUMAN_3P, 3, 0).err().expect("no module here");
        assert!(err.to_string().contains("paid module"), "{err}");
        assert_eq!(in_force(YAKUMAN_3P), MORA_3P, "Mora plays instead");
        assert!(!engine_for(MORA_3P, 3, 0).unwrap().is_yakuman());
        let err = engine_for(YAKUMAN_3P, 4, 0)
            .err()
            .expect("4p must be refused");
        assert!(err.to_string().contains("3-player only"), "{err}");
    }

    /// Yakuman 4p is yonma only, needs the paid module the same way, and is
    /// not a review model.
    #[test]
    fn yakuman_4p_needs_the_paid_module() {
        assert!(is_native(YAKUMAN_4P));
        assert_eq!(display_name(YAKUMAN_4P), Some("Yakuman (built-in, 4p)"));
        let err = engine_for(YAKUMAN_4P, 4, 0).err().expect("no module here");
        assert!(err.to_string().contains("paid module"), "{err}");
        assert_eq!(in_force(YAKUMAN_4P), MORA_4P, "Mora 4p plays instead");
        assert!(!engine_for(MORA_4P, 4, 0).unwrap().is_yakuman());
        let err = engine_for(YAKUMAN_4P, 3, 0)
            .err()
            .expect("3p must be refused");
        assert!(err.to_string().contains("4-player only"), "{err}");
        assert!(review_models().iter().all(|(n, _)| !is_yakuman(n)));
        assert_eq!(weights_hash(YAKUMAN_4P), weights_hash(MORA_4P));
    }

    fn new_bot() -> NativeBot {
        NativeBot::new(MORA_4P, 0, 4).unwrap()
    }

    /// The opening batch up to our first draw, as the manager would deliver it.
    fn opening() -> Vec<MjaiEvent> {
        vec![
            start_game_4p(0),
            start_kyoku_4p(),
            MjaiEvent::Tsumo {
                actor: 0,
                pai: "5p".into(),
            },
        ]
    }

    fn start_game_4p(seat: u8) -> MjaiEvent {
        MjaiEvent::StartGame {
            names: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            kyoku_first: None,
            aka_flag: None,
            id: Some(seat),
            num_players: 4,
            game_meta: None,
        }
    }

    /// A deliberately shapeless hand (~4 shanten): on our draw the model has
    /// only plain discards to choose from, never riichi. Tests that assert "the
    /// local model played" would otherwise have to accept a `reach` too, which
    /// hides whichever path actually produced the move.
    fn start_kyoku_4p() -> MjaiEvent {
        let hand: Vec<String> = [
            "1m", "1m", "3m", "5m", "7m", "9m", "2p", "4p", "6p", "8p", "1s", "3s", "5s",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        MjaiEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "2m".into(),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: vec![25000, 25000, 25000, 25000],
            tehais: vec![hand.clone(), hand.clone(), hand.clone(), hand],
            num_players: 4,
        }
    }

    /// A `start_kyoku` shaped like the ones the bridge actually emits: our hand
    /// is known, the opponents' are `?`. It matters. Handing the engine four
    /// concrete hands (as `start_kyoku_4p` does, for convenience) lets it see
    /// claims it can never see in a real game, so a test built on that is
    /// testing a game we never get to play.
    fn start_kyoku_4p_hidden(our_hand: [&str; 13]) -> MjaiEvent {
        // The bridge's placeholder for a tile we cannot see (mjai's `?`).
        let hidden = vec!["?".to_string(); 13];
        MjaiEvent::StartKyoku {
            bakaze: "E".into(),
            dora_marker: "2m".into(),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: vec![25000; 4],
            tehais: vec![
                our_hand.iter().map(|s| s.to_string()).collect(),
                hidden.clone(),
                hidden.clone(),
                hidden,
            ],
            num_players: 4,
        }
    }

    fn seat1_discards(pai: &str) -> [MjaiEvent; 2] {
        [
            MjaiEvent::Tsumo {
                actor: 1,
                pai: pai.into(),
            },
            MjaiEvent::Dahai {
                actor: 1,
                pai: pai.into(),
                tsumogiri: true,
            },
        ]
    }

    /// Regression (#190), end to end. We hold two 9s, seat 1 discards a 9s: a pon
    /// window. Whatever the model picks, the card must show the pass ranked
    /// against the pon — that comparison *is* the decision. Before this fix the
    /// bot emitted no card at all when it declined, so the tile kept displaying
    /// the previous turn's discard advice as if it were live.
    #[tokio::test]
    async fn a_declined_call_still_produces_a_card_ranking_the_pass() {
        let mut bot = new_bot();
        let mut events = vec![
            start_game_4p(0),
            start_kyoku_4p_hidden([
                "9s", "9s", "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m", "1p", "2p",
            ]),
        ];
        events.extend(seat1_discards("9s"));

        let resp = bot.react(&events).await.unwrap();

        let meta = resp
            .meta
            .as_ref()
            .expect("a pon window is a decision — it must refresh the card");
        let items = meta["show"]["items"].as_array().unwrap();
        let labels: Vec<&str> = items
            .iter()
            .map(|i| i["label"].as_str().unwrap_or_default())
            .collect();
        assert!(
            labels.contains(&"Pass"),
            "the pass must be a ranked row, got {labels:?}"
        );
        assert!(
            labels.contains(&"Pon"),
            "…alongside the call it is weighed against, got {labels:?}"
        );
        assert!(
            items.iter().all(|i| i["value"].is_string()),
            "every row carries the probability the player is here to read"
        );
    }

    /// The other half of the contract: an opponent discard we cannot call is not
    /// a decision, so the card must be left exactly as it was.
    #[tokio::test]
    async fn an_uncallable_opponent_discard_leaves_the_card_alone() {
        let mut bot = new_bot();
        let mut events = vec![
            start_game_4p(0),
            // No 9s in hand, and seat 1 is our shimocha so a chi is impossible.
            start_kyoku_4p_hidden([
                "1m", "3m", "5m", "7m", "9m", "2p", "4p", "6p", "8p", "1s", "3s", "5s", "7s",
            ]),
        ];
        events.extend(seat1_discards("9s"));

        let resp = bot.react(&events).await.unwrap();

        assert!(matches!(resp.action, MjaiEvent::None));
        assert!(
            resp.meta.is_none(),
            "nothing was decided, so nothing should replace the card on screen"
        );
    }

    #[tokio::test]
    async fn native_bot_returns_legal_discard_on_own_tsumo() {
        let mut bot = new_bot();
        // Feed the opening up to our first draw in one batch (as the manager would).
        let resp = bot.react(&opening()).await.unwrap();
        // On our own tsumo we must act — a discard (or riichi/kan/hora), never None.
        assert!(
            !matches!(resp.action, MjaiEvent::None),
            "bot must act on its own tsumo, got None"
        );
        match resp.action {
            MjaiEvent::Dahai { actor, .. } | MjaiEvent::Reach { actor, .. } => {
                assert_eq!(actor, 0)
            }
            MjaiEvent::Ankan { .. } | MjaiEvent::Kakan { .. } | MjaiEvent::Hora { .. } => {}
            other => panic!("unexpected reply on own turn: {other:?}"),
        }
    }

    #[tokio::test]
    async fn native_bot_passes_when_not_its_turn() {
        let mut bot = new_bot();
        // Opponent (seat 1) draws and discards; we (seat 0) usually can't act.
        let resp = bot
            .react(&[
                start_game_4p(0),
                start_kyoku_4p(),
                MjaiEvent::Tsumo {
                    actor: 1,
                    pai: "9s".into(),
                },
                MjaiEvent::Dahai {
                    actor: 1,
                    pai: "9s".into(),
                    tsumogiri: true,
                },
            ])
            .await
            .unwrap();
        // Either None (nothing to do) or a legal call — must not be one of our
        // own-turn-only actions.
        assert!(
            !matches!(
                resp.action,
                MjaiEvent::Dahai { .. } | MjaiEvent::Reach { .. }
            ),
            "must not discard on someone else's turn: {:?}",
            resp.action
        );
    }

    // ---------- circuit breaker ----------

    // ---------- live reconfiguration mid-game ----------

    // ---------- API-backed native bot: request shaping ----------

    #[test]
    fn local_show_meta_lists_top_candidates_with_probs() {
        let cands = vec![
            (
                BotAction::Dahai {
                    pai: "1m".into(),
                    tsumogiri: false,
                },
                0.6f32,
            ),
            (BotAction::Reach { pai: "2p".into() }, 0.3f32),
            (
                BotAction::Dahai {
                    pai: "9s".into(),
                    tsumogiri: true,
                },
                0.1f32,
            ),
        ];
        let meta = build_show_meta(&cands).unwrap();
        assert_eq!(
            meta["show"]["title"], SHOW_TITLE_LOCAL,
            "a local decision must be titled as such"
        );
        let items = meta["show"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 3, "should surface all three candidates");
        assert_eq!(items[0]["label"], "Discard");
        assert_eq!(items[0]["pais"][0], "1m");
        assert_eq!(items[0]["value"], "60%");
        assert_eq!(items[1]["label"], "Riichi");
        assert_eq!(items[1]["pais"][0], "2p");
        assert_eq!(items[1]["value"], "30%");
        assert_eq!(items[2]["pais"][0], "9s");
        assert_eq!(items[2]["value"], "10%");
    }

    /// Regression (#190): declining a call used to produce no card at all, so
    /// the tile kept showing the previous turn's discard advice — live-looking
    /// advice for a decision that was already over. The pass is the decision
    /// here, and its probability against the call it turned down is the whole
    /// point of the card.
    #[test]
    fn local_show_meta_ranks_the_pass_against_the_call_it_declined() {
        let cands = vec![
            (BotAction::Pass, 0.9f32),
            (
                BotAction::Pon {
                    target: 0,
                    pai: "1m".into(),
                    consumed: vec!["1m".into(), "1m".into()],
                },
                0.1f32,
            ),
        ];

        let meta = build_show_meta(&cands).expect("a declined call still has a card to show");
        let items = meta["show"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "both the pass and the pon it beat");
        assert_eq!(items[0]["label"], "Pass");
        assert_eq!(items[0]["value"], "90%");
        assert!(
            items[0].get("pais").is_none(),
            "the pass row must draw no tile — it would read as a discard suggestion"
        );
        assert_eq!(items[1]["label"], "Pon");
        assert_eq!(items[1]["value"], "10%");
    }

    /// The other half of #190: a lone pass is *not* a decision. `WaitResponse`
    /// opens for every seat as soon as any seat can claim, and the engine always
    /// offers `Pass` there — so a seat with nothing to claim gets `[Pass]` back.
    /// Showing "Pass 100%" for that would replace the card on every opponent
    /// discard someone *else* could call.
    #[test]
    fn a_lone_pass_is_not_a_decision_point() {
        assert!(
            !is_decision_point(&[]),
            "no legal action at all is not a decision"
        );
        assert!(
            !is_decision_point(&[(BotAction::Pass, 1.0)]),
            "someone else's call window is not our decision"
        );
    }

    #[test]
    fn a_choice_is_a_decision_point() {
        let call_window = [
            (BotAction::Pass, 0.9f32),
            (
                BotAction::Pon {
                    target: 0,
                    pai: "1m".into(),
                    consumed: vec!["1m".into(), "1m".into()],
                },
                0.1f32,
            ),
        ];
        assert!(
            is_decision_point(&call_window),
            "pass vs pon is exactly the decision the card exists to explain"
        );

        // Our own turn: no pass on offer, and even a single forced discard is a
        // decision the player wants to see.
        let forced_discard = [(
            BotAction::Dahai {
                pai: "1m".into(),
                tsumogiri: true,
            },
            1.0f32,
        )];
        assert!(is_decision_point(&forced_discard));
    }

    /// A reach acceptance repeats nothing: the call window before it already
    /// got its answer, so the card is left alone.
    #[tokio::test]
    async fn reach_accepted_does_not_repeat_the_previous_call_window() {
        let mut bot = new_bot();
        let first = bot
            .react(&[
                start_game_4p(0),
                start_kyoku_4p_hidden([
                    "9s", "9s", "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m", "1p", "2p",
                ]),
                MjaiEvent::Tsumo {
                    actor: 1,
                    pai: "9s".into(),
                },
                MjaiEvent::Reach {
                    actor: 1,
                    pai: None,
                },
                MjaiEvent::Dahai {
                    actor: 1,
                    pai: "9s".into(),
                    tsumogiri: true,
                },
            ])
            .await
            .unwrap();
        assert!(first.meta.is_some());

        let duplicate = bot
            .react(&[MjaiEvent::ReachAccepted { actor: 1 }])
            .await
            .unwrap();
        assert_eq!(duplicate.action, MjaiEvent::None);
        assert!(duplicate.meta.is_none());
    }

    /// After our own daiminkan the bot waits for the rinshan draw: no answer
    /// and no card yet.
    #[tokio::test]
    async fn own_daiminkan_waits_for_rinshan() {
        let mut bot = new_bot();
        let events = [
            start_game_4p(0),
            start_kyoku_4p_hidden([
                "5m", "5m", "5m", "1m", "2m", "3m", "4p", "5p", "6p", "7s", "8s", "9s", "E",
            ]),
            MjaiEvent::Tsumo {
                actor: 1,
                pai: "5m".into(),
            },
            MjaiEvent::Dahai {
                actor: 1,
                pai: "5m".into(),
                tsumogiri: true,
            },
            MjaiEvent::Daiminkan {
                actor: 0,
                target: 1,
                pai: "5m".into(),
                consumed: ["5m".into(), "5m".into(), "5m".into()],
            },
        ];

        let resp = bot.react(&events).await.unwrap();
        assert_eq!(resp.action, MjaiEvent::None);
        assert!(resp.meta.is_none());
    }
}
