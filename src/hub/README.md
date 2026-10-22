# `src/hub/` — The accounts open in this Mokora

One process, one window, any number of Mahjong Soul accounts.

```
Hub ── open accounts ── Account "main" ── AppState, web views ui-main + game-main
 │                  └── Account "2"    ── AppState, web views ui-2    + game-2
 └── the shown account's id
```

- **`Account`** = an `AppState` (game tracker, analysis runner, history recorder,
  bot and autoplay managers, capture supervisor, buses) plus two web views of the
  window. Everything that used to start with the app starts per account, in
  `account.rs`.
- **`Shared`** (in `ipc/state.rs`) is what the accounts use the one of: the log
  session, history store, Python runtime, updater and review queue.
- **`views.rs`** makes the web views. Every account has an *interface*
  (`ui-<id>`, the React app, filling the window) and a *game* (`game-<id>`, laid
  over the interface wherever the interface keeps a placeholder — the interface
  reports the rectangle with `place_game`). Only the shown account's interface is
  visible. The other accounts' games are **parked off the window's edge, not
  hidden**: a hidden web view stops the game's loop and a match would stall.
- **Storage and DevTools.** Each account's views share its storage (its login)
  and its own DevTools port (`--remote-debugging-port` in the views' browser
  arguments). Views that share a storage folder must share their arguments, so
  the overlay window has a storage folder of its own. Never set
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`: it overrides every per-view setting.
- **Commands** take `Acct` (`arg.rs`) and get the state of the account whose
  interface called them; a caller that belongs to no account (the overlay) gets
  the shown account's.
- **Events** go to the account's own interface, and to the overlay while it is
  the shown one (`ipc::Route`). `accounts-changed` goes to every interface and
  makes the tabs refresh.
- **Views are made on the main thread** (`Hub::on_main`): from a Tokio worker
  the call deadlocks the event loop on Windows.

## Lifecycle

`open` registers the account *before* its views exist (they call commands the
moment they load), makes the views, then starts the runtime. `activate` opens if
needed and shows it. `close` stops its capture, aborts its tasks and removes the
views; the last open account cannot be closed. `quit_all` stops every capture and
ends the process (the window's ×, the power button, Ctrl+C).

## Adding something per account

Put it in `AppState` (built in `AppState::new`), start its task in
`account::start` and push the handle, so `close` aborts it.
