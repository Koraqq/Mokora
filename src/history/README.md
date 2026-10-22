# `src/history/` — Game History Recorder

This module persists every cleanly-ended mjai game to a stable, frontend-readable
store. The on-disk shape (under `<config_root>/history/`) is:

```
history/
  index.jsonl              # one JSON-encoded GameRecord per line, append-only
  games/
    <ulid>.mjai.jsonl      # full event stream for that record
```

`<ulid>` is a [ULID](https://github.com/ulid/spec) generated at finalisation
time — lexicographically sortable by start time, doubles as the filename stem.

## Pipeline

```
                    ┌──────────────┐
   MjaiBus  ──────▶ │  recorder::  │ ──▶ HistoryStore.append() ──▶ index.jsonl
                    │  drive_loop  │                                games/<id>.mjai.jsonl
                    └──────────────┘
                            │
                            ▼
                       HistoryBus  ──▶  ipc forwarder  ──▶  "history-recorded" Tauri event
```

A `RecorderState` machine buffers events between `StartGame` and a confirmed
`EndGame`. On a Mahjong Soul reconnect to the same table it retains completed
rounds and replaces the server-restored copy of the current round. A different
next game, explicit server termination, or shutdown drops the incomplete
buffer, so the History tab still contains only complete games.

## Modules

- **`aggregator.rs`** — Pure function `aggregate(events, ...) -> Option<GameRecord>`.
  Ports the `Stat::from_game` aggregator from Mortal's `libriichi`. It walks
  `Reach`/`ReachAccepted`/`Hora`/`Ryukyoku` deltas for statistics, prefers a
  platform's authoritative final standings when supplied, and otherwise
  normalises totals to 100k (4p) / 105k (3p). Stat counts mirror `Stat`'s field
  set, scoped to one game.
- **`store.rs`** — `HistoryStore` (JSONL append-only index + per-game event
  files). Single mutex serialises writes; reads are lock-free (re-open).
- **`deal_in.rs`** — Which tiles were dealt in (every seat's ron) across a
  set of games, read from their event logs on request
  (`history_deal_in_tiles`), so games recorded before it existed count too.
- **`recorder.rs`** — `drive_loop(store, history_bus, platform, mjai_rx)`. The
  long-running task spawned in `lib.rs::run`.

## Adding a new platform

When a non-Majsoul bridge lands (Tenhou, RiichiCity, ...):

1. Add the variant to `crate::schema::Platform`.
2. Pass that variant when spawning a *per-bridge* recorder. The current code
   spawns one global recorder tagged `Platform::Majsoul` because that's the
   only bridge in v3 — for multiple bridges, mux by source via per-bridge
   `tokio::sync::broadcast` channels or a wrapped event type.

## Adding a new GameStats field

1. Add the field to `crate::schema::history::GameStats` (default-initialised
   to keep deserialisation backward-compatible with old index lines).
2. Update `aggregator.rs` to populate it.
3. Mirror in the frontend `types/history.ts`, `lib/ptCalc.ts` (if it affects
   PT), and `components/history/StatsTable.tsx` (display).
4. Older `index.jsonl` lines will deserialise with the new field defaulting;
   no migration step is required so long as the new field has a `Default`.

## Match identity (`GameRecord.match_info`)

`schema::history::MatchInfo` is a platform-tagged enum carrying the raw room
/ rank-lobby ids and the platform's own game (paifu) id — e.g. Majsoul
`mode_id` + `game_uuid`, Tenhou `<GO type>` bitfield + log id. Bridges attach
it to `StartGame.game_meta` (a `#[serde(skip)]` field, so mjai logs and the
inference API never see it) and the aggregator copies it into the record.

Rules of the road:

- Store **raw platform values**; label mapping lives in the frontend
  (`frontend/src/lib/matchInfo.ts` + `history.room.*` locale keys) so an
  unknown id shows as a number instead of a wrong name.
- Every field is `Option` + `#[serde(default)]` — bridges fill what their
  wire carried, old index lines deserialise with `match_info: None`.
- To extend: add fields/variants in `schema/history.rs`, fill them in the
  bridge's StartGame emission, mirror in `frontend/src/types.ts`.

## Whose game (`MatchInfo::Majsoul.account_id`)

The Majsoul bridge also stores the recorded player's `account_id` (from
`ReqAuthGame`) in `match_info`, so the History page can tell accounts apart —
a nickname can change and two accounts can share one. The player picker is
derived entirely in the frontend (`frontend/src/lib/historyPlayers.ts`):
records with an id are keyed by it; older records without one are folded
into the account last seen under the same nickname, or keyed by nickname if
none was.

## Rank at the time (`MatchInfo::Majsoul.level_id`)

The Majsoul bridge also stores the rank level id the recorded player held
when the game started — `ResAuthGame.players[].level` on a 4p table,
`.level3` on a 3p one (e.g. `10301` = 4p 雀傑1, `20202` = 3p 雀士2). With the
History page's "use current rank" switch on (`PtRule.auto`, see
`frontend/src/lib/ptCalc.ts`), each game is scored at its own room
(`mode_id`) and rank; games without them use the player's newest known
ones. The PT rule itself is kept per mode (3p / 4p).

## Game kind (`MatchInfo::Majsoul.category` / `rules`)

The Majsoul bridge also keeps the table's `game_config.category` (1 = friendly
room, 2 = matchmaking, other values for the 比賽場) and the special-rule
switches it had on (`SPECIAL_RULES`: `wanxiangxiuluo_mode`, `chuanma`, …). The
History page tells ranked games from friendly rooms and events by them
(`frontend/src/lib/matchInfo.ts::gameKind`) and leaves the latter two out of
its rank, PT and stats cards unless their switches are on.

## Round times (`GameRecord.round_times`)

`rounds::RoundClock` rides along in the recorder and times each round (局) as
the events arrive — from its deal (`start_kyoku`) to its win or exhaustive draw
(`end_kyoku`) — since the mjai stream carries no times. The round a reconnect
lands in is not measured (the replay restarts it). The record keeps `{count,
total_ms, min_ms, max_ms}`; the History page's detail stats pool them into
average / shortest / longest round.

## Which bot played (`GameRecord.bot`)

`recorder` stamps each record with the bot active for its mode
(`bot.active_3p` / `active_4p`, `None` when bots are off), read at finalise
time from a `SharedBots` cell that `update_config` rewrites on every save —
the same pattern as `SharedPlatform`. Records from before the field existed
have `bot: None`. The cumulative-PT chart draws Mokora's games as triangles.

## Adding a filter dimension

1. Add the field to `HistoryFilter` (in `schema/history.rs`).
2. Update `HistoryFilter::matches`.
3. Add the corresponding form control in `frontend/src/routes/History.tsx`.

## Don't

- Don't backfill from `<log_dir>/majsoul/*.mjai.jsonl` — those are bridge debug
  logs and may include incomplete / duplicated streams. The history store is
  populated *only* by the live recorder.
- Don't write into `<log_dir>` from this module — keep the two surfaces
  decoupled.
