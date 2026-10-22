# Bridge Module

Platform-specific protocol bridges between game wire protocols and the
[mjai JSONL protocol](https://gimite.net/pukiwiki/index.php?Mjai%20%E9%BA%BB%E9%9B%80AI%E5%AF%BE%E6%88%A6%E3%82%B5%E3%83%BC%E3%83%90)
consumed by AI bots.

## Trait

```rust
pub trait Bridge: Send {
    fn parse(&mut self, content: &[u8]) -> Vec<MjaiEvent>;
    fn build(&mut self, command: &MjaiEvent) -> Option<Vec<u8>>;
}
```

- `parse` — raw inbound WS binary frame → zero or more `MjaiEvent`s.
- `build` — outbound mjai command (also `MjaiEvent`) → optional raw WS binary frame (autoplay).

## mjai types

`MjaiEvent` lives in [`crate::schema::mjai`](../schema/mjai/mod.rs) — it's used
across the project (bridge output, AI bots, frontend HUD) and isn't owned by
this module. See `src/schema/README.md`.

One bridge instance per independent game session. For Majsoul that means one
per WebSocket flow, since each flow has its own request id sequence and game
state.

## Selecting a bridge

`bridge::for_platform(platform, flow_log, session, hooks)` returns a
`Box<dyn Bridge>` for the configured [`Platform`](../config/platform.rs). The
proxy handler builds a fresh bridge inside `handle_websocket` so per-flow state
is isolated.

## `BridgeHooks`

Shared slots the autoplay layer hands to a bridge. Every field is optional and
platform-specific, and only the chromium capture path wires them at all — the
MITM path has no `Page` handle, so nothing consumes them. They exist because
autoplay needs facts only the protocol parser sees:

| Field | Platform | Carries |
|---|---|---|
| `time_budget` | Majsoul | The server's per-decision-window time grant (`OptionalOperationList.time_fixed/time_add`). |
| `input_watch` | Majsoul | A counter of the client's own uplink input commands, so a click can be told from one the UI swallowed. |

Bundled into one struct so adding a platform's slot doesn't grow the argument
list of every constructor between here and the capture backend.

## Adding a new platform

1. Add a variant to `config::Platform` (`src/config/platform.rs`).
2. Create `src/bridge/<name>/mod.rs` with a struct that implements `Bridge`.
3. Re-export it from `src/bridge/mod.rs` and add the match arm in
   `for_platform`.
4. If the platform will support autoplay and its planner needs something only
   the parser sees, add a field to `BridgeHooks` rather than a new parameter.
5. The history tag (`schema::Platform`) and its `MatchInfo` variant already
   exist for Tenhou and Riichi City; a new game adds its own.

Two finished bridges for other games — Tenhou (plain JSON over WS) and Riichi
City (JSON inside a 15-byte binary header) — are parked in
`_parked/other-games/`, with their READMEs.

## Existing bridges

- `majsoul/` — Majsoul (lq.* protobuf over WS). `parser.rs` decodes raw
  WS frames into `ParsedMessage { msg_type, msg_id, method_name, payload }`;
  `mod.rs` runs the state machine that turns them into mjai events. See
  `majsoul/parser.rs` module docs for the 5-layer wire format. Protocol
  layout follows the published Majsoul `liqi.proto` schema; no third-party
  source code is copied.
