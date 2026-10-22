# Training the built-in bot

The built-in bot is trained by **behavior cloning** (supervised imitation) of
human Tenhou logs. The pipeline is: extract `(obs, action, mask)` samples in
Rust → train a small CNN in Python on GPU → export `.safetensors` that the Rust
`native_bot::model` (candle) loads directly.

Everything except the ~minute of GPU training is pure Rust. Python is used
**only** offline to fit weights.

## 0. Download Tenhou Phoenix logs (Rust)

`fetch` collects the Phoenix-room games of one player count (三鳳 for
3-player, 四鳳 for 4-player) from the daily `scc` listings Tenhou still serves
(older yearly archives are no longer available) and downloads each game's
mjlog XML, gzip-compressed, one file per game:

```sh
cd native_bot
cargo run --release --no-default-features --features fetch --bin fetch -- dataset/tenhou/p3 10
cargo run --release --no-default-features --features fetch --bin fetch -- dataset/tenhou/p4 10 4
```

- `10` is the number of parallel downloads; at that setting a full run took
  about 40 minutes for ~83k games (~1 GB).
- Re-runnable: games already on disk are skipped, so an interrupted run
  resumes, and a later run fetches only the new days. Logs that still fail
  after retries are listed in `dataset/tenhou/p3/failed.txt` — run again.
- Raw mjlogs live under `dataset/tenhou/`, apart from the mjai logs in
  `dataset/p3/`, because `extract` reads every `.gz` under the directory it is
  given. `native_bot/dataset/` is gitignored.

Then convert them to the mjai logs `extract` reads:

```sh
cargo run --release --no-default-features --features extract --bin convert -- dataset/tenhou/p3 dataset/p3
cargo run --release --no-default-features --features extract --bin convert -- dataset/tenhou/p4 dataset/p4
```

`convert` takes the player count from each log's own rules and writes plain
mjai with one entry per real seat (`kita` for nukidora in sanma). It replays every
game through the extractor's engine before writing it, so a game the engine
would reject is reported in `dataset/p3/convert_failed.txt` instead of
silently thinning the dataset. Re-runnable like `fetch`: converted games are
skipped. The 2026 Phoenix logs (82,848 games) converted with no failures into
29.8M samples.

## 1. Extract training data (Rust)

The extractor reads mjai `.json.gz` game logs, replays each through
riichienv-core, and writes compact binary shards. Observations are stored as
`u8` (`value/4*255`, `obs_scale = 4.0`); actions as `u16`; masks as `u8`.

```sh
cd native_bot
cargo build --release --no-default-features --features extract --bin extract

# 4-player: first 40k games, capped at 6M samples
./target/release/extract  <dataset>/p4  4  out/p4  40000  6000000
# 3-player: capped at 4M samples
./target/release/extract  <dataset>/p3  3  out/p3  40000  4000000
```

`--features v2` (or `v3`) writes those planes instead (see the crate README;
the default is `v1`). Progress — games done, samples, time left — is one line
rewritten every two seconds.

Each output dir gets `N.obs` / `N.act` / `N.msk` shard files plus a `meta.json`
describing the geometry and sample counts. Extraction is multi-threaded (rayon)
and fast — millions of samples in seconds.

Notes:
- One file = one game; malformed logs are skipped (guarded by `catch_unwind`).
- Tenhou **sanma** logs use a 4-seat layout with a dummy 4th player and spell
  nukidora as `nukidora`. Both are handled by `mjai_compat::parse_line`: the
  rename runs *before* serde (riichienv's `MjaiEvent` has a `#[serde(other)]`
  catch-all, so `nukidora` would otherwise deserialize to `Other` and be
  dropped), the 4-seat arrays are truncated after.
- Discards dominate (~77–81%), with `pass` synthesized for every seat that
  could have called a discard but didn't. Sanma additionally spends ~6% of its
  samples on kita.
- Shards land in `native_bot/out/` (gitignored) — tens of GB. Only the trained
  `weights/*.safetensors` are committed.

## 2. Train (Python, GPU)

Create/activate a venv with `torch` (CUDA), `numpy`, `safetensors`.

```sh
python train.py --data out/p4 --out ../weights/base4p.safetensors \
    --epochs 10 --batch 2048 --lr 1.5e-3
python train.py --data out/p3 --out ../weights/base3p.safetensors \
    --epochs 10 --batch 2048 --lr 1.5e-3
```

The model is a 1-D ResNet over the tile axis (`conv 64`, `3` residual blocks,
`fc 256`) with a masked cross-entropy loss over legal actions. After training,
BatchNorm is **folded into the preceding convolutions** so the exported model is
pure Conv1d + Linear — the candle side needs no BatchNorm op. A parity check
asserts the fold preserves the argmax decisions.

Reference numbers for exactly the commands above (40k games → 6.0M/4.0M samples,
10 epochs, RTX 4070 Ti) — these are what the committed weights were trained with:

| | val top-1 | params | weights | train time |
|---|---|---|---|---|
| 4p | 75.9% | 660,050 | 2.6 MB | ~5 min |
| 3p | 77.8% | 539,324 | 2.2 MB | ~3 min |

### Model size

`--conv`, `--blocks` and `--fc` train another size than the defaults above.
The Rust loader reads the size back off the exported tensors
(`native_bot::model::Dims`), so weights of any size load with no code change.
For the full 2026 sanma Phoenix set:

```sh
python train.py --data ../out/p3 --out ../out/base3p-5m.safetensors \
    --epochs 10 --batch 2048 --lr 1.5e-3 --conv 160 --blocks 10 --fc 768
```

(4.9M parameters, ~28 min per epoch on an RTX 5070 Ti; the whole set loads
into ~32 GB of RAM.)

### Datasets larger than RAM

The 2026 4-player Phoenix set is ~146k games / ~90M samples (~127 GB of
shards), more than fits in memory. `--stream-shards K` keeps only K shards in
memory at a time, shuffled together, and holds out whole shards (so whole
games) for validation instead of a random slice of samples:

```sh
python train.py --data ../out/p4 --out ../out/base4p-7m.safetensors     --epochs 5 --batch 2048 --lr 1.5e-3 --conv 192 --blocks 12 --fc 768 --stream-shards 8
```

(7.7M parameters; 8 shards is ~11 GB of RAM.) Without the flag, training loads
everything as before.

Every streamed batch is full: the samples left over at the end of a group of
shards carry into the next. And before export, `--bn-recal-batches N`
(default 200) recomputes every BatchNorm's running statistics as a plain
average over N training batches, replacing the moving average. The first
4-player run, before both existed, lost its BatchNorm statistics to one tiny
leftover batch: val top-1 fell from 79.5% to 44% between two epochs while the
training loss stayed normal.

### Progress and resuming

`train.py` rewrites one progress line every couple of seconds (epoch, bar,
running loss, time left for the epoch and the run, expected finish), and
appends the summary lines to `<out>.log`. After every epoch it saves the
model, optimizer and schedule to `<out>.ckpt`; `--resume` continues a stopped
run from there.

### Mora (`訓練Mora三麻.bat`)

Mora is the sanma model on the V2 features, at Mokora's size (`--conv 160
--blocks 10 --fc 768`) so the comparison isolates the features. Double-click
`train\訓練Mora三麻.bat`: it builds the tools, extracts `out/p3v2` (V2, ~6 min),
trains 10 epochs to `out/mora3p.safetensors` (streamed 16 shards at a time —
the V2 set is ~55 GB), then plays it against `weights/mokora3p.safetensors`
in `arena` (east and south, 2,000 walls each) into `out/mora3p-arena.txt`.
Closing the window stops it; running it again skips finished steps and
resumes training from the last epoch.

> The Mokora models (`mokora3p` / `mokora4p`) were removed from the bot: Mora
> replaces the sanma one and there is no yonma bot for now. The sections below
> that describe Mokora are kept as the record of how the models were trained.
> The arena opponent of the Mora scripts is now the shipped Mora (3p) or the
> original `base4p` (4p).

### Mora 4p (`訓練Mora四麻.bat`)

Shipped as `weights/mora4p.safetensors` (bot `mora-4p`, with the danger head from
`訓練Mora四麻危險度.bat`). Result: val top-1 81.35% (Mokora 4p, V1: 79.79%); in
`arena` against Mokora 4p (1,000 walls x 14 seatings = 14,000 games each) 2.4907
vs 2.5093 east and 2.4883 vs 2.5117 south, within the +-0.013 noise (a tie in
strength). Danger head: val AUC 0.947, mean prediction 0.75% against a real
0.74%.

The yonma counterpart: the V2 features at Mokora 4p's size (`--conv 192
--blocks 12 --fc 768`, 5 epochs, 8 shards streamed at a time, as Mokora 4p
was trained). Double-click `train\訓練Mora四麻.bat`: it builds the tools,
extracts `out/p4v2` from `dataset/p4`, trains to `out/mora4p.safetensors`
(the progress line shows time left) and plays it against
`weights/mokora4p.safetensors` in `arena` (east and south, 1,000 walls
each) into `out/mora4p-arena.txt`. The 4-player V2 set is large (well over
100 GB): check the free space first. Closing the window stops it; running
it again skips finished steps and resumes training. `MORA_QUICK=1` runs
every step tiny to check the script. The file is pure ASCII.

`train\訓練Mora四麻危險度.bat` then puts the danger head on `out/mora4p.safetensors`
the way `訓練Mora危險度.bat` does for Mora (the four-player labels, `labels_4p`,
already see chi and the 2m-8m tiles through the rules engine). The training
script extracts `out/p4v2d` with `--danger` (about 1% bigger than without), so
both scripts share one set. The result is `out/mora4p-danger.safetensors`; since
Mora 4p is not installed into `weights/` yet, nothing there is touched.

### Mora V3 (script removed)

> The V3 script, its data (`out/p3v3`) and its model were deleted: the experiment was not adopted
> (see the result below). The old V1 sets `out/p3`, `out/p4`, and `out/p3v2` (a copy of `p3v2d` without danger labels) were deleted too;
> every script can make what it needs again from `dataset/`.

The same recipe on the V3 features (V2 plus the rounds left): extracts
`out/p3v3`, trains `out/mora3p-v3.safetensors`, and plays it against the
current Mora (`weights/mora3p.safetensors`, left as it is) in east and south
games into `out/mora3p-v3-arena.txt`. 99.8% of the Phoenix logs are hanchan,
so the last-round planes are learned mostly from South 3; the east arena is
the one that says whether that carries over to East 3 of an east-only game.
`MORA_QUICK=1` runs every step tiny to check the script.

## 2b. Evaluate head-to-head (Rust)

Validation top-1 says how often a model agrees with the humans it learned
from, not whether it wins. `arena` plays two weight files against each other
in riichienv-core's own engine, through the same decision path the live bot
uses, in duplicate (every wall dealt to all six seatings of the two models):

```sh
cargo run --release --features extract --bin arena -- \
    ../out/base3p-5m.safetensors weights/base3p.safetensors 2000 east
```

It prints each model's average placement with a 95% interval. Two copies of
the same weights come out at exactly 2.000 — a quick check that the arena
itself is fair.

### Mokora (the shipped result)

`weights/mokora3p.safetensors` is the 4.9M model above (`--conv 160 --blocks 10
--fc 768`, 10 epochs, val top-1 81.31%). In `arena` (2,000 walls × 6 seatings
= 12,000 games each) it placed:

| vs | east | south |
|---|---|---|
| original `base3p` | 1.936 vs 2.064 | 1.905 vs 2.095 |
| same data at 2.0M (`--conv 128 --blocks 6 --fc 384`) | 1.989 vs 2.011 | — |

(±0.012 at 95%.) In an unoptimised debug build a decision takes ~25 ms,
against ~3 ms for the original — still far inside a turn.

`weights/mokora4p.safetensors` is the 4-player model, trained on the 4-player
Phoenix logs streamed 8 shards at a time (`--conv 192 --blocks 12 --fc 768
--epochs 5 --batch 2048 --lr 1.5e-3 --stream-shards 8`; 7.76M parameters,
89.8M training samples, val top-1 79.79%). In `arena` (1,000 walls × 14
seatings = 14,000 games each) against the original `base4p`:

| | east | south |
|---|---|---|
| average placement | 2.404 vs 2.596 | 2.349 vs 2.651 |
| 1st / 4th place | 28.2% / 22.6% vs 21.8% / 27.4% | 30.0% / 20.9% vs 20.0% / 29.1% |

(±0.013 at 95%.) The export's parity check runs in full float32: with
PyTorch's default TF32 convolutions a model this size (logits reach ~1e5)
showed 0.02% of choices moved by rounding alone, which the bot's CPU float32
inference never sees.

### Mora (the V2 result)

`weights/mora3p.safetensors` is Mokora's 3-player size (`--conv 160
--blocks 10 --fc 768`, 4.94M parameters) trained on the V2 planes
(`train/訓練Mora三麻.bat`: `out/p3v2`, 10 epochs, 16 shards streamed at a
time; val top-1 82.42%, against Mokora's 81.31% on V1). In `arena` against
Mokora (2,000 walls × 6 seatings = 12,000 games each):

| | east | south |
|---|---|---|
| average placement | 1.988 vs 2.012 | 1.990 vs 2.010 |
| 1st / 3rd place | 33.9% / 32.7% vs 32.8% / 34.0% | 33.7% / 32.7% vs 33.0% / 33.9% |

(±0.012 at 95%.) The added planes cost nothing measurable at play time: on a
recorded game a decision took 3.9 ms (median) against Mokora's 3.7 ms.

### Danger head (`訓練Mora危險度.bat`)

A second head on Mora's last hidden layer predicts, for every tile and every
opponent, whether that opponent would win on it now — tenpai, waiting on it,
not furiten. The Phoenix logs show every hand, so the extractor knows the
truth at each decision (`extract … --danger`, `native_bot::danger`, one byte
per tile per opponent in `N.dgr`; yaku is not checked). `train_danger.py`
trains only the head (BCE) on the model as exported, frozen and with its
BatchNorm already folded, so the policy tensors in the output are the input's
bit for bit. The head works tile by tile: the feature map after the residual
blocks next to the input planes, convolved along the tile axis (7, then 3
wide) down to one output per opponent per tile. A first version on the
policy's last hidden layer (768 → 256 → 54) ranked tiles worse than the
rule-based risk — that layer has already squeezed out facts like "this tile
is in that opponent's pond" — so it was replaced. The script then copies the result
over `weights/mora3p.safetensors` (the old file kept in
`out/mora3p-before-danger.safetensors`) and runs the parity test, which puts
the old file back if a single logit moved. Each epoch logs the validation AUC
and the mean predicted rate against the real one; a trial on 2,000 games gave
AUC 0.91 after two epochs (0.79 for the first version).

Trained on the full set (`out/p3v2d`, 4 epochs, ~5 min each): validation AUC
0.940, mean prediction 1.22% against a real rate of 1.24%. Against the
rule-based risk (`src/analysis/risk`, per opponent `risk × tenpai rate`) on
the same 400 games / 116,813 discard decisions, scored on the same labels:

| | danger head | rules |
|---|---|---|
| AUC, every tile | **0.942** | 0.926 |
| AUC, tiles in hand | **0.929** | 0.916 |
| the safest tile in hand by each deals in | **0.210%** | 0.289% |

The first version's figures there were 0.865 / 0.865 / 0.651%.

`Model::forward_danger` / `Engine::danger` run it live; Mokora sends the result
with each reply (`meta.danger`) to the "Danger (model)" tile.

### Mora V3 (not adopted)

The same size and recipe on the V3 planes (`train/訓練Mora三麻V3.bat`:
`out/p3v3`, 10 epochs): val top-1 82.47% against Mora's 82.42%, and in
`arena` against Mora (12,000 games each):

| | east | south |
|---|---|---|
| average placement | 2.003 vs 1.997 | 1.998 vs 2.002 |

Both within the ±0.012 noise: telling the model how many rounds are left
changed nothing measurable, so `weights/mora3p.safetensors` stays V2. The
model imitates humans, and the Phoenix logs say little about East 3 of an
east-only game (99.8% are hanchan); the planes only pay once something
rewards the placement itself (self-play on Mahjong Soul PT), which is where
they are kept for.

## 3. Verify parity (Rust ⇄ Python)

`parity_check.py` dumps a deterministic input + reference logits so the Rust
loader can be checked numerically:

```sh
python parity_check.py ../weights/base4p.safetensors 39 34 ../weights/parity_4p.json
python parity_check.py ../weights/base3p.safetensors 37 27 ../weights/parity_3p.json
python parity_check.py ../weights/mora3p.safetensors 66 27 ../weights/parity_mora3p.json
cd .. && cargo test --test parity     # candle must match PyTorch (max|diff| < 2e-3)
```

The exported `weights/*.safetensors` are `include_bytes!`-embedded by
`src/defaults.rs`, so rebuilding `native_bot` after retraining ships the new
weights automatically.

## Keeping the two sides in sync

`CONV` / `BLOCKS` / `FC` in `train.py` **must** equal the constants in
`src/model.rs`, and the safetensors key names must line up (`conv_in`,
`res.{i}.conv{1,2}`, `fc`, `head`). If you change the observation layout in
`src/obs.rs`, re-extract and re-train — weights are layout-specific.
