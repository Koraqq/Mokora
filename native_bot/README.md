# native_bot

A **built-in, libriichi-free** mahjong bot for Mokora. It runs a small
behavior-cloned convolutional net entirely in Rust (via [candle]) — no Python,
no libriichi, no subprocess. **Mora** is the shipped model: a larger network
on the V2 feature planes, trained on Tenhou Phoenix-room logs (see
[`train/README.md`](train/README.md)): `weights/mora3p.safetensors` (3-player
only, bot name `mora-3p`) and `weights/mora4p.safetensors` (4-player only, bot
name `mora-4p`; 7.8M parameters). Both carry a danger head.
`weights/base3p.safetensors` and `weights/base4p.safetensors` (the original
size) stay in the tree for the tests and as the baseline the 4-player training
compares against, but are not offered as bots. The earlier Mokora models were
removed. **Yakuman** (bot names `yakuman-3p` and
`yakuman-4p`, one per player count) ships no weights of its own: it is Mora
with every decision handed to the yakuman planner, a paid module
([`yakuman`](#yakuman)).

The bot is intentionally only "default strength": it imitates human play from
Tenhou logs with a compact model. It exists so Mokora has a sane, zero-install
default bot, not to be state of the art.

## How it fits together

```
              (training, offline)                       (runtime, in Mokora)
  dataset/*.json.gz ─► extract (Rust) ─► shards ─► train.py ─► *.safetensors
                          │                                        │
                          └──────── obs.rs / action_codec ─────────┘  (shared code = parity)
                                                                     │
   mjai events ─► Engine::feed ─► get_observation ─► obs.rs ─► candle Model ─► legal Action
```

The **same** Rust observation encoder ([`obs`]) and action codec
([`action_codec`]) are used to build training samples and to run inference, so
the model always sees identical features — parity is guaranteed by
construction, not by re-implementing the encoding twice.

## Modules

| Module | Role |
|---|---|
| `tiles` | tile id ↔ tile34 / sanma compact-27, red-five + dora helpers |
| `obs` | our own observation feature encoder (`EncInput` → `[C, T]` planes) |
| `adapt` | build an `EncInput` from a riichienv-core observation (shared by extractor + inference) |
| `action_codec` | action indexing, legal masking, logit → legal-action decoding (reuses riichienv-core action ids) |
| `mjai_compat` | event fixups applied before an mjai stream reaches a game state (sanma 4-seat arrays) |
| `replay` | drive an mjai game log through the engine, emit `(obs, action, mask)` samples incl. synthesized `Pass` |
| `timeline` | the table-wide discard order (and tedashi / riichi flags) the `V2` features need |
| `yakuman` | Yakuman's side here: reads the table for its (paid) planner, scores wins |
| `model` (`infer` feature) | candle CNN: load `.safetensors`, forward → logits |
| `engine` (`infer` feature) | live inference: feed events → pick a legal action (handles riichi two-step) |
| `defaults` (`infer` feature) | embedded default weights + `engine(num_players, seat)` |
| `bin/extract` (`extract` feature) | the offline training-data extractor |

The engine is deliberately decoupled from Mokora: it consumes
`riichienv_core::replay::MjaiEvent` and returns a schema-agnostic
`engine::BotAction`. Mokora's `src/bot/native.rs` maps that to Mokora's own
`MjaiEvent`.

## Geometry

| | 4-player | 3-player |
|---|---|---|
| tile axis `T` | 34 | 27 (2m–8m removed) |
| obs channels `C` | 39 | 37 (adds per-player kita) |
| action space `A` | 82 | 60 |

Feature planes are channel-major (`buf[ch*T + tile]`) and fully self-relative
(index 0 = the deciding seat). See the doc comment in `src/obs.rs` for the
exact channel layout.

### Feature sets

The channels above are feature set **V1**, what every bundled weight file was
trained on. **V2** keeps them unchanged and appends 29 planes in sanma (36 in
yonma): discard order, tedashi / tsumogiri, the tiles each player can no
longer ron (after their riichi or their latest discard), meld counts,
placements, unseen copies, shanten, furiten, and for each possible discard
whether it keeps the best shanten and how much ukeire it leaves.

**V3** keeps V2 and appends two broadcast planes: the rounds the game has
left after this one, and whether this is the last round. Round wind and kyoku
index alone cannot tell East 3 of an east-only game (the last round) from
East 3 of a hanchan (three to go). The game's planned length
(`obs::GameLength`) is not in the events: the extractor reads it off the
Tenhou room tag in the log's file name, `arena` from its east / south mode,
and Mokora from Mahjong Soul's room settings (`Engine::set_game_length`).

| | 4-player | 3-player |
|---|---|---|
| V1 channels | 39 | 37 |
| V2 channels | 75 | 66 |
| V3 channels | 77 | 68 |

A weight file's input width names its set (`obs::Features::of_channels`), so
`Model` / `Engine` / `arena` pick the encoder per file and both sets run side
by side. The table-wide discard order is not in riichienv-core's state, and an
mjai-driven state (extractor, live play) records tedashi and the riichi tile
differently from an engine-stepped one (`arena`), so `timeline` keeps them for
every path; `tests/v2_paths.rs` checks the V2 planes agree between the two.

## Yakuman

Yakuman is a paid feature. `yakuman.rs` here only reads the table for its
planner (our hand, the unseen tiles, the wall) and scores a win as a
yakuman or not; the planner that decides — which yakuman to aim for and how
to order our choices towards it — is in the module Mokora downloads once a
key is redeemed, and is asked through `pro_api::yakuman`.
`Engine::set_planner` hands it in (`defaults::yakuman_engine_3p`).

## Retraining / extending

See [`train/README.md`](train/README.md) for the end-to-end extract → train →
export flow.

> **Weights are layout-specific — and the layout changed.** The "last discard"
> feature plane used to be fed the *discarder's seat* instead of the discarded
> tile (riichienv-core 0.4.8's `Observation::last_discard` mislabels the
> `(pid, tile)` tuple; see the module docs in `src/adapt.rs`). `adapt.rs` now
> reads the tile from the game state, and the bundled `weights/*.safetensors`
> were retrained against the corrected encoder. Do **not** pair the current
> `adapt.rs` with weights predating the fix: they load fine (the geometry is
> unchanged) but silently disagree on ~2% of call decisions, and the parity
> fixtures won't catch it — they check candle-vs-PyTorch numerics, not feature
> semantics.

**To change the observation features**: edit `src/obs.rs` (`channels()` and
`EncInput::encode`, keeping the `debug_assert_eq!(ch, c)` cursor honest) and the
adapters in `src/adapt.rs`. Because the encoder is shared, re-extract and
re-train after any change — old weights will not match a new layout.

**Decode-side behaviour needs no retraining**: `action_codec` collapses the legal
actions that share one action id (a red five and its plain twins) to a single
candidate, preferring the plain copy so a "discard the five" prediction never
throws away a dora. Labels are unaffected — `Action::encode` already mapped both
to the same id — so this is purely how a logit is turned back into a tile.

**To change the model size/shape**: keep `CONV`/`BLOCKS`/`FC` in `src/model.rs`
in lock-step with the same constants in `train/train.py`. The safetensors key
names (`conv_in`, `res.{i}.conv{1,2}`, `fc`, `head`) are the contract between
the two.

## Building

```sh
# core codec + inference (default features = infer)
cargo test

# the offline extractor (no candle needed)
cargo run --release --no-default-features --features extract --bin extract -- \
    <dataset_dir> <num_players> <out_dir> [max_games] [max_samples]
```

`riichienv-core` (Apache-2.0) provides the pure-Rust game engine, legal-action
enumeration, and canonical action ids. This crate is part of Mokora and inherits
its licence.

[candle]: https://github.com/huggingface/candle
