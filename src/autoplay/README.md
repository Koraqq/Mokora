# `src/autoplay/` — bot decisions → actions in the real client

Translates the bot's chosen mjai action into something the game client
actually does, dispatched over CDP. Only active when
`autoplay.enabled = true` **and** the chromium capture backend is running
(the MITM path has no page handle at all).

One route in: Mahjong Soul renders to a canvas and exposes nothing to script,
so the only way is synthesised mouse input at reconstructed coordinates.
Almost everything intricate in this module — coordinate tables, candidate
row index arithmetic, click verification, retries, page reloads — exists
because of that. What every route shares is the delay model, because a
decision executed the instant the bot answers is the most obvious tell there
is.

Other games plug in through `PlatformAutoplay` (see "Adding a game" below);
`_parked/other-games/` holds a DOM-driven web client (Tenhou) and a
frame-injected native one (Riichi City) as worked examples.

## Module map

| Module | Role |
|---|---|
| `manager.rs` | Long-lived task: subscribes to `BotResponseBus` + `MjaiBus`, owns per-game state (`last_kawa_tile`, riichi flags, reach two-step), executes click plans. |
| `platform.rs` | `PlatformAutoplay` trait + `ActionContext`/`PlanResult`/`Step` (`Click` / `Sleep`). The manager only knows this trait. |
| `majsoul/` | The Majsoul implementation: the table's 16:9 coordinates (`coords.rs`) + plan dispatch for every mjai action type. The lobby's are full auto's (below). |
| `budget.rs` | `TimeBudget` — the server's per-decision-window time grant (`OptionalOperationList.time_fixed/time_add`, ms). Written by the Majsoul bridge, read here. Never locally accounted; always the server's own value. |
| `delay/` | The pre-click "thinking time" model. See below. |
| `context.rs` | `AutoplayContext` — shared slots between the capture backend, the bridge and the manager (`page`, `canvas_rect`, `resize`, `time_budget`, `input_watch`). |
| `cdp_input.rs` | chromiumoxide wrappers: hover → press → hold → release click sequence (optionally with a mid-press cursor jiggle for retries), canvas rect query, and the screenshot reads the next-game flow steers by. |
| `lobby.rs` | Majsoul only. `LobbyWatch` — the client's own lobby uplink (back in the lobby / a room's table list opened / queued for a table) and the rank change reported at game end, as counters + tickets. Written by the Majsoul bridge, read by `next_game`. |
| `next_game.rs` | Majsoul only. The "start the next ranked game" task: dismisses the result screens, opens the ranked lobby, a room and a table, proving each stage from `lobby`. See below. |
| `resize.rs` | Majsoul only. `ResizeWatch` — counts window resizes of the game tab (CDP `Page.frameResized`, bumped by the chromium capture backend). The cached canvas rect is re-measured only when the count moved since it was measured, after the window has been still for 100 ms and the page has re-laid the canvas out (`fits_viewport`: Majsoul sizes it 100 ms after the resize, to the largest 16:9 box that fits). Each press checks the count again after its hover, just before pressing, and re-aims if the window resized under it. An unresized press pays one atomic load. |
| `verify.rs` | Majsoul only. `InputWatch` — counts the client's own uplink input commands (`inputOperation` / `inputChiPengGang`, bumped by the Majsoul bridge). The manager takes a ticket before pressing; if the count never moves, the click was swallowed and the plan is pressed again (bounded by `click_retries`, gated on the decision window still being live). Repeated dead decisions trigger a page reload (`reload_after_failures`), which reconnects into the hand via the bridge's `GameRestore` path. |

## What arrives on the `BotResponseBus`

Every response the manager plans against is one the riichi engine asked
for. `bot::manager` only flushes a batch to the bot where the engine says
our seat can act, so an `MjaiEvent::None` reaching autoplay is a *decline*
— press pass — and never "the bot had nothing to say about someone else's
turn". That distinction is not visible in the reply itself, which is why it
is settled upstream; see `src/bot/README.md` → "Deciding what to ask".

It has to be settled somewhere. One frame can carry several seats' actions,
and when the bot was asked about all of them, the fillers answered first: a
real `hora` arrived to find a pass already pressed into its window.

## Adding a game

1. `config::Platform` gets a variant (`src/config/platform.rs`), and the
   frontend's platform list (`lib/platforms.ts`) an entry.
2. Implement `PlatformAutoplay` for it: `plan(&ActionContext) -> PlanResult`
   turns a bot decision into `Step`s. Canvas games use `Step::Click`; a
   client that is not a canvas needs `Step` kinds of its own, each with an
   executor in `manager.rs`.
3. Add the field and the match arm in `AutoplayManager::handle_bot_response`.
4. A bridge slot the planner needs (hand indices, a decision window) goes in
   `BridgeHooks` and `AutoplayContext`.

The two implementations that were here are kept, with everything they touched
and a patch of the shared-file changes, in `_parked/other-games/`.

## Next game (`next_game.rs`, Majsoul)

Full auto is a paid feature. Its map of the lobby — every press point, the
parts of the screen that tell one screen from another and the shares of
colour they must show (`pro_api::lobby::Layout`) — is in the module Mokora
downloads once a key is redeemed; the flow below asks for it
(`pro::lobby_layout`) and does not start without it. The flow itself is
here.

`autoplay.next_game.enabled` makes Mokora start the next ranked game when one
ends. The screens are a canvas, so nothing can be read off them; the client's
own lobby uplink is the only evidence of where it is, and every stage is
gated on it:

| Stage | Proof the client sends |
|---|---|
| *(nothing)* | Walking through the result screens to the lobby sends **no request at all** (checked in the logs), so "back in the lobby" is read off the screen — see below. |
| the client's account refreshed | `Lobby.fetchAccountInfo` with `account_id: 0` — **not** "back in the lobby": it lands 11-28s (mostly ~24s) after the result in 39 of 39 logged games, whatever screen is up. It is the timer for "enough time has passed", and the signal that the rank is fresh. |
| a room's tables are showing | `Lobby.fetchCurrentMatchInfo` — its `mode_list` is the room's tables **in the order they are drawn** |
| queued | `Lobby.startUnifiedMatch` — `match_sid` is `"1:<mode_id>"` |

The flow, in `after_game`: wait for the reward notify (rank change) → stop if
the rank changed → wait out the settlement animation → press the result
screen's confirm for as long as a screenshot of its spot shows the button's
yellow (`cdp_input::result_button_visible`; 0.73-0.87 of the box on result
screens, 0.07-0.11 on the lobby) → once it has been seen and then gone three
looks running *and* the lobby's main screen is showing (a strip screenshot down the
right end of the three signs: navy 0.30-0.35 / warm 0.74-0.77 on the lobby on two
backgrounds, never above 0.2 warm anywhere else — a dialog over the lobby
included, which is right, since nothing under a dialog can be pressed). One
result screen has no button: the room chest's "獲得獎勵", shown when the game
filled the chest (`NotifyGameFinishRewardV2.match_chest` graded with rewards —
every 6-8 games in the logs). It closes on a tap anywhere, so when the reward
notify said the chest opened and the screen shows neither the button nor the
lobby for three looks, the map's `chest_dismiss` (the character art on the left,
which in the lobby is the lobby character) is tapped, up to three times. A
promotion (with `stop_on_rank_change` off) adds two more buttonless screens: a
rolled-up 認定書 scroll that opens on a tap anywhere — the same spot, with
three more taps allowed — and the opened certificate (雀力認定), which closes
only on its red 關閉 X (the map's `rank_cert_close`). The X is pressed only
while two screenshots say the certificate is up: parchment right of its badge
and the X's red (`cdp_input::certificate_signature`; 0.84 / 0.42 on the
certificate, 0.00 / 0.00 on the standings and the rolled-up scroll), at most
twice. A game that raises the bond (羈絆) with the player's character —
whichever character it is: the reward notify's `main_character` comes back
with less `exp` than it `add`ed, the experience having restarted at the new
level — adds the character's story, dialogue with a 跳過 button at the top
right. 跳過 (the map's `bond_skip`) is pressed while a screenshot of it shows
its pale cyan (`cdp_input::skip_cyan_fraction`; 0.15 on the story, about 0
on every other screen — the button is the client's, the same over every
character's scene), at most three times; after it the result screens carry
on as usual (the confirm button) → `queue`,
which checks the lobby is showing again before its first press (so the start
button refuses to run from any other screen). Before the room it checks the copper coins (`gold`, from the login and every
`NotifyAccountUpdate`) against the room's floor (`coin_floor`: Silver
2500 / 3500 East / South, Gold 5000 / 7000, Jade and Throne 10000 /
14000; a rank-picked room against its East, and South only if the coins
reach it). Short, it claims today's revive coins the way a player does —
活動 (the map's `lobby_activity`), the 復活幣 tab, 領取, a tap to close the
獲得獎勵 banner, the return arrow — with the claim's answer
(`gainReviveCoin`) as the proof; still short, or claimed already today
(`fetchReviveCoinInfo`), it stops with `NextGameStatus::Blocked`, which
the frontend shows in the middle of the screen. Should the game still turn
a room or table down with its "銅幣不足無法入場" dialog (`refused`: its
確認's yellow), 確認 is pressed, the coins are
claimed and the room tried once more. `queue` presses
段位場, scrolls the room list to the edge the room is on, presses the room,
waits for the table list, scrolls the table list, presses the table, waits for
the queue.

Things that were decided on purpose:

- **The confirm button is where the lobby has the shop and the gacha**
  (the map's `result_confirm`), so it is guarded twice — before the press is
  dispatched and again after the hover, immediately before the mouse goes
  down (`cdp_input::dispatch_click_guarded`). It is never pressed "until it
  stops mattering".
- **Lists are moved to an edge, not by an amount.** Wheel events far longer
  than the list (or a drag, `next_game.scroll`) leave it clamped at the top or
  bottom, which is the same layout wherever it started. Rooms 銅/銀/金 are
  pressed at the top, 玉/王座 at the bottom; tables 1-3 at the top, table 4
  (三人南) at the bottom.
- **No table-id lookup.** "Same as last game" is the position of the last
  game's `mode_id` in the `mode_list` the room reported. The same list is the
  room check: if it does not contain the last table, this is not the room that
  was played in (or it is locked) and the run stops. With no previous ranked
  game the configured rule picks the row and nothing is compared.
- **The canvas is measured on every press**, not cached like the table
  clicks (which re-measure only after a resize — see `resize.rs`): a resized
  window moves every target, and this flow presses few enough times that the
  query is free.
- **One press, one answer, no retry.** A room or table the player's rank may
  not enter puts a dialog over the lobby ("您目前的段位無法進入這個房間", or
  "段位不符" on the row) and the client sends **nothing** — so a press with no
  proof within `step_timeout_ms` ends the run instead of pressing again under
  a dialog. (A lost press ends it too; that is the price of not guessing.)
- **A person's click ends it.** The page gets a probe counting `mousedown`
  (left, right and middle all raise it; moving the mouse does not —
  `cdp_input::install_input_probe`). The flow reads the count right after each
  of its own presses and scrolls and again before the next one, and while it
  waits; any difference is somebody else's click. It watches only from the
  moment the lobby is up until the table is pressed — clicking through the
  result screens yourself is fine, and so is anything after the queue starts.
- **`room_mode`** picks the room itself unless it is `manual`: `highest_four`
  / `highest_three` queue on that ladder, in the highest room that ladder's own
  level may enter (`level.id` for 4-player, `level3.id` for 3-player — both
  arrive in the login response, and each game's reward updates the one it was
  played on), and take East or South by whichever has more players — the
  `playing_count` the server answers the room request with — so the queue
  fills and games keep coming. `manual` uses `room` and `rule`, with the room
  check against the last game's table.
- **The profile refresh** (`refresh_profile`, on by default): after each game, and when the 個人資料 page is opened, the player's own profile window is opened from the lobby and closed again (the game sends its statistics only then). Off, nothing is pressed.
- **A rank change stops it** (`stop_on_rank_change`, on by default), from
  `NotifyGameFinishRewardV2.level_change.origin.id != final.id`, before any
  press. Off, a promotion's screens are walked through as above (the X only
  while the certificate is seen) and the next room is picked from the new
  rank.
- **Friend-room and AI games do nothing**: no ranked `mode_id` on the last
  `StartGame` means there is no table to repeat.
- Every stop other than "switched off / a game is running" sends a toast.

The Settings page's "start auto play" button wakes the same `queue` from the
lobby (`AutoplayContext::next_game_start`, a `Notify`, so a press while it is
busy is one pending run, not a queue) and resets the `max_games` count; the
games after it follow through the normal game-ended path. It refuses while a
game is on a table.

Whether the client honours CDP wheel events at all is untested —
`scroll = "drag"` is the fallback. The test button exists to check it, and
the map's points with it.

## The delay model (`delay/`)

`delay::decide` computes a target **total** thinking time for the
current decision window — not a sleep length. The caller subtracts what
the window has already consumed before emitting the `Step::Sleep`:
`majsoul::push_pre_delay` deducts network, proxy and bot inference time
from the server's own budget, plus the upcoming click sequence's duration.
A platform with neither a per-window budget nor anything to click has
nothing to deduct, so the target *is* the sleep.

Layers, in order:

1. **Policy** — selected by `autoplay.delay.mode`. `lua` (default):
   `delay.lua` next to the config file, generated from
   `assets/delay_default.lua` (embedded as `script::DEFAULT_SCRIPT`) on
   first use and hot-reloaded. An existing `delay.lua` that is an
   unmodified copy of an *older* bundled default is replaced by the
   current one on start; user-edited scripts are never touched. This
   works by verbatim comparison against every previously shipped
   default, embedded from `assets/delay_default_superseded/` — **when
   changing `assets/delay_default.lua`, copy the version being replaced
   into that directory and add it to `script::SUPERSEDED_DEFAULT_SCRIPTS`**,
   or existing installs will keep the old behaviour. The built-in model
   (per-decision-kind
   log-normal + decision-type bonuses, `delay/mod.rs`) is the fallback
   when the script fails. `legacy`: the historical uniform draw — the
   distribution is forced to Uniform and the script is not consulted.
   The default log-normal parameters are **calibrated against real
   ranked-game records** (think times measured on the server-side
   action clock of decoded game records); change them only with new
   measurements. Both default policies deliberately **ignore the bot's
   probability distribution**: measured bot policies can be nearly flat
   (top ~0.11 on most decisions), which turned every confidence-based
   rule into a permanent bias instead of a signal. Normalized probs
   (`delay/probs.rs`: native-bot `show.items[].prob` raw, Mortal-style
   `q_values` softmaxed — never feed raw Q values to a probability
   threshold) are still exposed to user Lua scripts as
   `ctx.top_prob`/`ctx.second_prob`/`ctx.margin` for custom rules tuned
   to a specific bot.
2. **Budget caps** (`budget_cap`) — `soft = time_fixed - safety_margin -
   click_overhead`; `hard = soft + bounded bank spend`, bank only when
   the policy returned `allow_bank`. Unconditional: overrunning the
   window means the client auto-discards.
3. **Functional floors** (`functional_floor`) — `min_delay_ms` (Mahjong
   Soul must render buttons/tiles before a click can land; too-early
   clicks are silently lost) and the dealing-animation wait. Applied
   after the caps; a script cannot go below them.

The policy's output is the target **server-observed total** for the
window. `majsoul::push_pre_delay` converts it to a sleep by deducting
both the time already elapsed (network + bot inference, from the budget
snapshot) and the upcoming click sequence's own duration (hover + hold +
candidate clicks), then re-applies the functional floor so the first
click still lands after the UI is ready.

### Adding a new delay input

1. Add the field to `DelayInput` (`delay/mod.rs`) and derive it in
   `majsoul::push_pre_delay` (or the manager, if it needs async state).
2. Consume it in `decide`. Default parameter values must be backed by
   calibration data (record measurements); a knob without data defaults
   to off/no-op.
3. Expose it to Lua in `script::DelayScript::build_ctx` and document it
   in `assets/delay_default.lua` + the README's ctx table.
4. Config knobs go in `config/autoplay.rs::DelayModelConfig` (serde
   defaults!), mirrored in `frontend/src/types.ts` and, if user-facing,
   `Settings.tsx`.

### Lua host invariants (`delay/script.rs`)

- Restricted stdlib: math/string/table, plus the base library scrubbed
  of `load`/`loadstring`/`dofile`/`loadfile` (bytecode and filesystem
  escapes), `pcall`/`xpcall` (they could swallow the runaway-guard
  abort) and `string.dump`. No io/os/debug.
- Hard allocation ceiling (`set_memory_limit`) — an allocation bomb is
  a catchable Lua error, not a process abort.
- Instruction budget + wall-clock deadline via VM hook, active for the
  chunk's top level at load time and for every call; if the hook cannot
  be installed the script is not run at all.
- Every failure falls back to the built-in model and logs **once** per
  distinct error. A missing script file is a normal state, not an error.
- Hot reload by mtime+size; a broken file is not recompiled until it
  changes.
- The script decides *when*, never *what*: it gets no coordinates, no
  action choice, and its output is clamped by caps and floors.

## Time budget flow

```
Majsoul server frame (ActionPrototype with operation for our seat)
  └─ MajsoulBridge::update_time_budget          (bridge/majsoul/mod.rs)
       └─ AutoplayContext.time_budget           (std RwLock slot)
            └─ manager snapshot (elapsed_ms precomputed)
                 └─ ActionContext.budget → delay model caps
```

The bridge clears the slot when an action carries no operation (window
closed) and at game boundaries. `GameRestore` replays park the value and
commit only the final still-open window, backdating `opened_at` by
`passed_waiting_time`. The MITM proxy passes no slot — `budget` is
simply `None` there and the static `no_budget_cap_ms` applies.

## Testing notes

- Delay-model tests are seeded (`StdRng`) — no flaky distributions.
- Use synthetic payloads for bridge tests, never pasted real captures
  with account ids.
- `assets/delay_default.lua` is compiled and exercised by
  `script::tests::bundled_default_script_works`; keep it in sync with
  the ctx table.
