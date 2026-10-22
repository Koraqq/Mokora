# MokoraV3 Frontend Reference

Full Tauri command + event surface for the frontend designer. Backend-side references in `src/ipc/`, `src/schema/`, `src/event_bus.rs`, `src/analysis/result.rs`, `src/game_state/snapshot.rs`, `src/game_state/mahgen_view.rs`.

---

## Architecture

```
                        ┌──────────────────┐
                        │   Tauri Webview  │
                        └────┬─────────┬───┘
                       events│         │invoke
                             ▼         ▼
  ┌──────────────────────────────────────────────────────┐
  │                   Backend (Rust)                      │
  │                                                       │
  │  proxy / bridge → MjaiBus → GameTracker → PostBus    │
  │                                  └→ analysis::runner │
  │                                       └→ AnalysisBus │
  │                                                       │
  │  IPC forwarders subscribe each bus → app.emit(event)  │
  │  IPC commands read state on demand                    │
  └──────────────────────────────────────────────────────┘
```

Two interfaces:
- **Events** — backend pushes to all webviews via `app.emit(event_name, payload)`. Frontend listens with `appWindow.listen(event_name, callback)`.
- **Commands** — frontend pulls/triggers via `invoke(cmd_name, args)` returning a `Promise<T>`.

Events are **pull-by-stream** (live updates). Commands are **pull-on-demand** (one-shot snapshot).

---

## Setup boilerplate

```ts
import { invoke } from '@tauri-apps/api/tauri';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// On app start: get current state.
const status = await invoke<Snapshot>('get_status');

// Subscribe to live updates.
const unlisten = await listen<MjaiEvent>('mjai-event', (e) => {
  console.log('mjai:', e.payload);
});

// On unmount: clean up.
unlisten();
```

---

## Events (backend → frontend)

Six events, all kebab-case. Subscribe with `listen<T>(name, cb)`.

| Event | Payload | When |
|---|---|---|
| `mjai-event` | `MjaiEvent` | Every parsed mjai event from the bridge |
| `bot-response` | `BotResponse` | Every bot reaction |
| `bot-status` | `BotStatus` | Bot subprocess lifecycle change |
| `proxy-status` | `ProxyStatus` | MITM proxy lifecycle change |
| `notify` | `Notification` | Toast / notification |
| `analysis-result` | `AnalysisResult` | After every game-state update |

### `mjai-event`

Live game events. Internally tagged enum — discriminant on `type` field.

```ts
type MjaiEvent =
  | { type: 'start_game';   names: [string,string,string,string]; kyoku_first?: number; aka_flag?: boolean; id?: number /* our seat */ }
  | { type: 'start_kyoku';  bakaze: string; dora_marker: string; kyoku: number; honba: number; kyotaku: number; oya: number; scores: [number,number,number,number]; tehais: [string[],string[],string[],string[]] }
  | { type: 'tsumo';        actor: number; pai: string }
  | { type: 'dahai';        actor: number; pai: string; tsumogiri: boolean }
  | { type: 'chi';          actor: number; target: number; pai: string; consumed: [string,string] }
  | { type: 'pon';          actor: number; target: number; pai: string; consumed: [string,string] }
  | { type: 'daiminkan';    actor: number; target: number; pai: string; consumed: [string,string,string] }
  | { type: 'kakan';        actor: number; pai: string; consumed: [string,string,string] }
  | { type: 'ankan';        actor: number; consumed: [string,string,string,string] }
  | { type: 'dora';         dora_marker: string }
  | { type: 'reach';        actor: number }
  | { type: 'reach_accepted'; actor: number }
  | { type: 'hora';         actor: number; target: number; deltas?: [number,number,number,number]; ura_markers?: string[] }
  | { type: 'ryukyoku';     deltas?: [number,number,number,number] }
  | { type: 'end_kyoku' }
  | { type: 'end_game' }
  | { type: 'none' };  // bot's "no action this turn"
```

Tile strings: mjai format. `1m`-`9m`, `1p`-`9p`, `1s`-`9s`, `E S W N P F C` (winds + dragons), `5mr`/`5pr`/`5sr` (red five), `?` (unknown).

### `bot-response`

```ts
type BotResponse = {
  // The flattened mjai action object — same shape as MjaiEvent, plus optional meta.
  ...MjaiEvent;  // {type: 'dahai', actor, pai, tsumogiri, ...}
  meta?: Record<string, unknown>;  // bot-defined; backend does not interpret
};
```

`meta` is a free-form JSON object emitted by the bot — confidence scores,
reasoning text, q-values, anything the bot wants the HUD to render. Mokora
forwards it verbatim. Different bots emit different keys; the frontend
should treat unknown keys defensively.

### `bot-status`

```ts
type BotStatus =
  | { state: 'idle' }
  | { state: 'loading'; bot: string; stage: 'syncing_deps' | 'spawning' }
  | { state: 'ready';   bot: string; actor_id: number }
  | { state: 'error';   bot: string; error: string }
  | { state: 'stopped'; bot: string };
```

`syncing_deps` is the slow first-spawn (uv sync). Show progress spinner.

### `proxy-status`

```ts
type ProxyStatus =
  | { state: 'stopped' }
  | { state: 'starting'; addr: string }
  | { state: 'running';  addr: string }
  | { state: 'error';    addr: string | null; error: string };
```

### `notify`

```ts
type Notification = {
  level: 'info' | 'success' | 'warn' | 'error';
  title: string;
  body?: string;
  sticky: boolean;   // persist until user dismiss; used for long-running ops
  id?: string;       // stable key — replace any prior toast with same id
};
```

Use `id` to deduplicate progress toasts. Sticky toasts must be cleared by the next non-sticky `notify` with the same `id`.

### `analysis-result`

Fires after every event the analysis engine processes. See [`AnalysisResult` schema](#analysisresult) below.

---

## Commands (frontend → backend)

Fifteen commands. All async. Errors return as `string` (Tauri convention).

| Command | Args | Returns | Purpose |
|---|---|---|---|
| `get_config` | — | `AppConfig` | Current loaded config |
| `update_config` | `(new_config: AppConfig)` | `void` | Persist new config to file |
| `list_bots` | — | `BotInfo[]` | Scan `mjai_bot/` for available bots (each entry includes its manifest, if any) |
| `set_active_bot` | `(mode: '4p' \| '3p', name: string)` | `void` | Set `bot.active_4p` or `bot.active_3p` in config; empty `name` clears the slot |
| `get_bot_settings` | `(name: string)` | `BotSettings` | Manifest schema + current values for one bot |
| `update_bot_settings` | `(name: string, values: Record<string, unknown>)` | `void` | Validate against manifest + persist to bot's `settings.toml` |
| `install_bot_from_github` | `(repo: string, assetGlob?: string, name?: string)` | `BotInfo` | Download latest release zip + extract into `mjai_bot/<name>/` |
| `update_bot_from_manifest` | `(name: string)` | `BotInfo` | Reinstall using `[bot.source]` from the bot's existing manifest |
| `start_proxy` | — | `void` | Spawn MITM proxy |
| `stop_proxy` | — | `void` | Graceful shutdown |
| `get_status` | — | `Snapshot` | One-shot dump of current config + bot + proxy + log dir |
| `get_log_dir` | — | `string` (path) | Current log session directory |
| `get_analysis` | — | `AnalysisResult \| null` | Latest analysis output |
| `get_game_snapshot` | — | `GameStateSnapshot \| null` | Live game-state mirror |
| `get_mahgen_view` | — | `MahgenView \| null` | Pre-encoded mahgen DSL strings ready for `<mah-gen>` |

### Examples

```ts
// On startup: hydrate UI.
const snap: Snapshot = await invoke('get_status');
applyConfig(snap.config);
setBotStatus(snap.bot_status);
setProxyStatus(snap.proxy_status);

// User picks a bot for 4p or 3p (separate slots).
await invoke('set_active_bot', { mode: '4p', name: 'mortal' });
await invoke('set_active_bot', { mode: '3p', name: 'mortal3p' });

// User starts proxy.
try {
  await invoke('start_proxy');
} catch (e) {
  console.error('start_proxy failed:', e);  // e is string
}

// Snapshot refresh (after losing event for any reason).
const game = await invoke<GameStateSnapshot | null>('get_game_snapshot');
if (game) renderGame(game);

// Pull mahgen pre-encoded strings (alternative to subscribing).
const view = await invoke<MahgenView | null>('get_mahgen_view');
```

---

## Schemas

### `AppConfig`

```ts
type AppConfig = {
  general: { language: string };
  logging: { dir: string; level: string; all_level: string };
  platform: { kind: 'Majsoul' };
  proxy: { enabled: boolean; addr: string; ca_dir: string };
  bot: { enabled: boolean; active: string; auto_sync: boolean; dir: string };
};
```

Persisted as TOML at the path `update_config` writes back to (location returned alongside config on app launch — exposed via the config-load flow, not a command).

### `BotInfo`

```ts
type BotInfo = {
  name: string;        // subdir name under bot.dir
  dir: string;         // absolute resolved path
  has_pyproject: boolean;
  manifest?: Manifest; // present when the bot ships a manifest.toml
};
```

### `Manifest` / `BotSettings`

```ts
type Manifest = {
  manifest_version: number;        // currently 1
  bot: {
    name: string;
    display?: string;
    description?: string;
    version?: string;
  };
  source?:                          // Phase 3 — install pointer
    | { type: 'github_release'; repo: string; asset_glob?: string };
  settings: Record<string, FieldSpec>;
};

type FieldKind = 'string' | 'bool' | 'int' | 'float' | 'enum';

type FieldSpec = {
  type: FieldKind;
  label: string;
  default: unknown;        // shape matches `type` (string/bool/number/enum-string)
  help?: string;
  secret?: boolean;        // render as password input + redact in logs
  min?: number;            // int / float
  max?: number;            // int / float
  step?: number;           // int / float
  choices?: string[];      // enum
};

// `get_bot_settings` returns:
type BotSettings = {
  manifest: Manifest;                       // schema (form definition)
  values: Record<string, unknown>;          // current values (form state)
};
```

`update_bot_settings` validates each value against the matching `FieldSpec`
(type match, numeric bounds, enum choice). Validation errors come back as
the Tauri command error string. Settings take effect on the **next**
`start_game` event — the running bot keeps its current values. The frontend
should warn on save.

`install_bot_from_github` and `update_bot_from_manifest` push progress
through the `notify` event with sticky id `bot-install-<name>` (info →
info → success). The frontend should subscribe and dismiss the sticky
toast when it sees the matching success entry. `install_bot_from_github`
refuses to overwrite an existing `mjai_bot/<name>/`; offer the user a
"Remove and reinstall" toggle that calls `update_bot_from_manifest` (or
deletes via the file browser) instead.

### `Snapshot`

```ts
type Snapshot = {
  config: AppConfig;
  bot_status: BotStatus;
  proxy_status: ProxyStatus;
  log_dir: string;
};
```

### `AnalysisResult`

Latest output of the analysis engine. Cached by the runner; pushed on `analysis-result` event AND retrievable via `get_analysis`.

```ts
type AnalysisResult = {
  seat: number;                 // 0..3 = our seat
  turn: number;                 // own discards so far
  shanten: number;              // -1=agari, 0=tenpai, 1+ = N away
  state: 'wait13' | 'discard14';
  hand13: Hand13Result | null;  // populated when state='wait13'
  hand14: Hand14Result | null;  // populated when state='discard14'
  opponents: OpponentRisk[];    // up to 3
  mixed_risk: number[];         // 34-vector, deal-in % across all opponents
  best_attack_discard: string | null;   // mjai tile string
  best_defence_discard: string | null;
};

type Hand13Result = {
  shanten: number;
  waits: WaitInfo[];
  waits_total: number;
  next_shanten_waits_count: { [tileIdx: number]: number };
  avg_next_shanten_waits: number;
  mixed_waits_score: number;          // speed-score; sort key
  avg_agari_rate: number;             // %
  is_furiten: boolean;
  furiten_rate: number;               // 1.0 / 0.5 / 0
  improves: ImproveEntry[];           // non-progressing draws that widen waits
  improve_way_count: number;
  avg_improve_waits_count: number;
  dama_point: number;
  riichi_point: number;
  mixed_round_point: number;          // expected per-round delta
  yaku_ids: number[];                 // riichienv yaku ids (see riichienv-core::yaku)
};

type WaitInfo = {
  tile: string;       // mjai (e.g. "5p")
  left: number;       // remaining count in pool
  agari_rate: number | null; // %
};

type ImproveEntry = {
  draw: string;            // mjai tile that widens waits
  widened_waits: WaitInfo[];
  widened_total: number;
};

type Hand14Result = {
  shanten: number;
  maintain: DiscardCandidate[];   // discards keeping current shanten, sorted desc
  backwards: DiscardCandidate[];  // discards walking back one shanten
};

type DiscardCandidate = {
  discard: string;          // mjai tile
  result: Hand13Result;     // post-discard 13-state analysis
};

type OpponentRisk = {
  seat: number;
  tenpai_rate: number;     // %
  risk: number[];          // 34-vector deal-in %
  is_riichi: boolean;
};
```

### `GameStateSnapshot`

Live mirror of the riichienv game state.

```ts
type GameStateSnapshot = {
  bakaze: 'E' | 'S' | 'W' | 'N';
  kyoku: number;             // 1..4
  honba: number;
  kyotaku: number;
  oya: number;               // 0..=2 (3p) or 0..=3 (4p)
  current_player: number;    // 0..=2 (3p) or 0..=3 (4p)
  turn_count: number;
  phase: 'wait_act' | 'wait_response';
  is_done: boolean;
  num_players: number;       // 3 (sanma) or 4 (yonma)
  players: PlayerSnapshot[]; // length matches num_players
  dora_markers: string[];
  our_seat: number | null;   // captured from start_game.id
};

type PlayerSnapshot = {
  seat: number;
  tehai: string[];               // mjai tile strings (unknown opponents may be all "?")
  melds: MeldSnapshot[];
  river: DiscardEntry[];
  score: number;
  riichi_declared: boolean;
  riichi_stage: boolean;         // mid-declaration window
  double_riichi: boolean;
  riichi_declaration_index: number | null;
  kita_tiles: string[];          // 3p only — north tiles set aside via 北抜き; empty in 4p
};

type MeldSnapshot = {
  kind: 'chi' | 'pon' | 'daiminkan' | 'ankan' | 'kakan';
  tiles: string[];
  from_who: number;              // -1 for ankan/kakan; 0..2 (3p) or 0..3 (4p) otherwise
  called_tile: string | null;
};

type DiscardEntry = {
  tile: string;
  tedashi: boolean;              // true = manual cut, false = tsumogiri
  is_riichi: boolean;            // true on the riichi-committing discard
};
```

### `MahgenView`

Pre-encoded mahgen DSL strings. Hand with all logic done backend-side — frontend just plugs into `<mah-gen>`.

```ts
type MahgenView = {
  players: PlayerMahgenView[];   // length matches num_players (3 for sanma, 4 for yonma)
  num_players: number;           // 3 or 4
  dora_indicators: string;       // e.g. "2m" or "2m3p" multi-suit
};

type PlayerMahgenView = {
  seat: number;
  hand: string;                  // self: real tiles, e.g. "123m456p789s11z"
                                 // others: "0z"×N tile-backs, e.g. "0000000000000z"
  melds: string[];               // one mahgen string per meld
  river: string;                 // river-mode mahgen string
};
```

See the [eric200203/mahgen](https://github.com/eric200203/mahgen) project for
the full DSL reference. Quick recap:

- **Meld syntax** — `_` at position 1/2/3 (kamicha/toimen/shimocha) for chi/pon. Daiminkan: pos 1/2/4 (skipping 3 for shimocha). Kakan replaces `_` with `^`/`v0`/`v5`. Ankan: `0z<digit><digit><suit>0z`.
- **River syntax** — `^` = tsumogiri, `_` = riichi declaration, `v` = riichi via tsumogiri, blank = manual cut.

---

## Common UI patterns

### Game board (4-player layout)

```ts
const view = await invoke<MahgenView | null>('get_mahgen_view');
if (!view) return;

// Render each player panel.
view.players.forEach((p) => {
  // <mah-gen> custom element (npm: mahgen)
  const handEl = `<mah-gen data-seq="${p.hand}"></mah-gen>`;
  const meldsEl = p.melds.map(s => `<mah-gen data-seq="${s}"></mah-gen>`).join('');
  const riverEl = `<mah-gen data-seq="${p.river}" data-river-mode></mah-gen>`;
  // assemble panel...
});

// Dora wall.
const doraEl = `<mah-gen data-seq="${view.dora_indicators}"></mah-gen>`;
```

### Live updates

```ts
let view: MahgenView | null = null;

// Initial pull.
view = await invoke('get_mahgen_view');
render(view);

// Listen for analysis events (fires after every mjai event since post-tracker bus chain).
listen<AnalysisResult>('analysis-result', async () => {
  // Re-fetch — analysis-result fires once per state change, so view is current too.
  view = await invoke('get_mahgen_view');
  render(view);
});
```

Alternative: listen to `mjai-event` directly and re-fetch. `analysis-result` is gated by our_seat being set (via `start_game.id`), so for replay/observer modes without that, prefer `mjai-event`.

### Top-3 discard recommendations

```ts
listen<AnalysisResult>('analysis-result', (e) => {
  const r = e.payload;
  if (r.state !== 'discard14' || !r.hand14) return;
  const top3 = r.hand14.maintain.slice(0, 3);
  // Each entry has discard + result.{waits, agari_rate, mixed_round_point, ...}
  setRecommendations(top3.map(c => ({
    tile: c.discard,
    score: c.result.mixed_waits_score,
    ev: c.result.mixed_round_point,
    risk: r.mixed_risk[tileIdx(c.discard)],
  })));
});
```

### Risk overlay

```ts
listen<AnalysisResult>('analysis-result', (e) => {
  const risk = e.payload.mixed_risk;  // 34-vector
  // Map each tile in our hand to its mixed_risk[idx] %
  ourHandTiles.forEach((tile, i) => {
    setTileRisk(i, risk[tileIdx(tile)]);
  });
});

function tileIdx(mjai: string): number {
  // 0..8 = 1m..9m, 9..17 = 1p..9p, 18..26 = 1s..9s, 27..33 = E S W N P F C
  // (5mr → 4, 5pr → 13, 5sr → 22)
  // ...
}
```

### Bot lifecycle UI

```ts
listen<BotStatus>('bot-status', (e) => {
  const s = e.payload;
  if (s.state === 'loading') {
    showSpinner(s.stage === 'syncing_deps' ? 'Installing deps...' : 'Spawning bot...');
  } else if (s.state === 'ready') {
    hideSpinner();
    setActorId(s.actor_id);
  } else if (s.state === 'error') {
    showError(s.error);
  }
});
```

### Notification toast

```ts
const toasts = new Map<string, Toast>();

listen<Notification>('notify', (e) => {
  const n = e.payload;
  if (n.id) {
    // Replace any prior toast with same id.
    toasts.get(n.id)?.dismiss();
    const t = showToast(n.level, n.title, n.body, n.sticky);
    toasts.set(n.id, t);
  } else {
    showToast(n.level, n.title, n.body, n.sticky);
  }
});
```

---

## Frontend bundle dependencies

| Package | Purpose |
|---|---|
| `@tauri-apps/api` | `invoke`, `listen`, `appWindow` |
| `mahgen` | `<mah-gen>` Web Component (custom HTML element) |

Mahgen invocation:
```html
<script src="https://unpkg.com/mahgen/dist/index.umd.js"></script>
<!-- or -->
<script type="module">
  import { Mahgen } from 'mahgen';
  const png = await Mahgen.render('123m456p789s11z', false);
</script>
```

---

## Gotchas

1. **`our_seat` resets every `start_game`** — each new game may put us in a different seat (or none, in observer/replay mode). The tracker always replaces `our_seat` on `start_game`, never inherits the previous game's value. Frontend must:
   - Treat `our_seat: null` as "no perspective yet" — render all-back hands.
   - On every `start_game` event, re-bind any "self" UI to the new `our_seat` (it may have changed since last game).
   - Re-fetch `get_mahgen_view` / `get_analysis` after each `start_game` so the swap takes effect.

2. **`tehai` for opponents** is whatever the bridge fed the engine. In observer/replay mode this may contain real tiles. In live bot mode, opponents typically have `"?"` strings. The `MahgenView.hand` for non-self seats always renders as backs (`"0z"×N`) regardless — the snapshot `tehai` is for completeness only.

3. **Event ordering**:
   - `mjai-event` fires from the bridge directly.
   - `analysis-result` fires from a *different* bus chained through the tracker — slight latency vs `mjai-event` but state is guaranteed current.
   - For UI consistency, prefer subscribing to `analysis-result` for state-dependent renders.

4. **Lagged broadcast**: if the frontend is suspended (window minimized) during a high-rate event burst, the broadcast channel may drop events. Always have a `get_*` snapshot path to recover.

5. **Tile index `0..34`** layout (used by `mixed_risk[]`, `next_shanten_waits_count` keys, etc):
   ```
   0..=8   1m..9m
   9..=17  1p..9p
   18..=26 1s..9s
   27..=33 E S W N P F C
   ```
   Red fives (`5mr`/`5pr`/`5sr`) collapse to indices 4/13/22 in the analysis space.

6. **Errors are `string`**: Tauri command `Err(e)` becomes a JS reject with `e` as the string. Wrap each `invoke` in try/catch.

7. **`update_config` does NOT restart subsystems**: changing `proxy.addr` requires `stop_proxy` + `start_proxy`. Bot config takes effect on the next game.

8. **`set_active_bot` does NOT re-spawn** the running bot. The change applies on the next `start_game`. Display a notification suggesting the user end the current game or restart Mokora.

9. **3p / 4p**: every game-shaped array (`players`, `scores`, `tehais`, `deltas`, `names`) is `num_players`-length. Frontend must read `game.num_players` (default 4) and never index seat 3 unconditionally. `useNumPlayers()` from `stores/gameStore` exposes this; `relativeKind(seat, ourSeat, numPlayers)` and `bakazeFor(seat, oya, numPlayers)` are 3p-aware. 3p has no `toimen` (opposite seat doesn't exist in a triangle).

10. **`kita` event** (3p only): `MjaiEvent` of `type: 'kita'` carries `actor` and optional `pai` (`"N"`). PlayerTile renders a 北×N badge from `PlayerSnapshot.kita_tiles`.

11. **Bot per-mode toggle**: `BotInfo.manifest?.bot.supported_modes` (default `["4p"]`) controls which mode switches are enabled in the Bots route. Use `invoke('set_active_bot', { mode, name })` to set; pass `name: ""` to clear.

---

## Reference files

| Backend file | Frontend-relevant content |
|---|---|
| `src/schema/mjai/mod.rs` | `MjaiEvent` enum |
| `src/schema/ipc.rs` | `Notification`, `BotStatus`, `ProxyStatus`, `BotInfo`, `Snapshot` |
| `src/config/*.rs` | `AppConfig` shape |
| `src/bot/types.rs` | `BotResponse`, `BotMeta` |
| `src/analysis/result.rs` | `AnalysisResult`, `Hand13Result`, `Hand14Result`, `OpponentRisk` |
| `src/game_state/snapshot.rs` | `GameStateSnapshot`, `PlayerSnapshot`, `DiscardEntry`, `MeldSnapshot` |
| `src/game_state/mahgen_view.rs` | `MahgenView`, `PlayerMahgenView` |
| `src/ipc/commands.rs` | All `#[tauri::command]` handlers |
| `src/ipc/mod.rs` | Forwarder wiring |
| `src/event_bus.rs` | Channel definitions |

---

Reference generated: 2026-04-27. Update when adding events / commands / schema fields.
