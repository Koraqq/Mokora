# `src/accounts/` — What is kept for each Mahjong Soul account

Several accounts play at once in one Mokora (`src/hub/` runs them). This module
is the part that needs no running app: where an account's files are, who it is
logged in as, its saved profile and picture, and making and deleting accounts.

```
configs/config.toml          first account ("main": whatever load_config(None) picks)
configs/account.json         who it is logged in as
accounts/2/config.toml       second account — its own bot, autoplay, privacy…
accounts/2/webview/          …its own Mahjong Soul login and UI preferences
accounts/2/logs/             …its own logs
accounts/2/account.json
history/                     shared by every account
```

A new account's config is a copy of the account it was added from, with
`logging.dir` (and, for the external-browser mode, the browser profile) pointed
under its directory. The first account keeps the app's default web-view storage,
so a login made before accounts existed is kept; the others keep theirs in
`accounts/<n>/webview/`.

## Files beside each config

| File | What | Written by |
|---|---|---|
| `account.json` | `{account_id, nickname}` of the login | the account's watcher, when the bridge sees the login (`LobbyWatch::account`) |
| `login.json` | `{at_ms}`: when its game last came online | the watcher; the picker lists the most recent logins first |
| `profile.json` | coins, ranks, revive coins, the game's statistics (`AccountProfile`) | the watcher, whenever the traffic moved it; the 個人資料 page reads it with the account closed too |
| `avatar.png` | the picture the player chose | `set_account_avatar` |

`accounts/startup.json` (`{account}`) is the account opened when Mokora starts;
`accounts/layout.json` a game-page layout shared with every account;
`accounts/ui.json` the interface language last chosen, so an account opened
later starts in it.

## Deleting an account

`remove.rs`: the account's folder, its games in the shared history and their
reviews (every record whose Mahjong Soul account is the one it logged in as,
unless another entry is logged in as the same one), and the startup star if it
was on it. The first account is emptied, not removed. The account must be
closed (the command closes it first) and must not be the one the window shows.

## Tests

The file helpers are unit-tested here (`cargo test accounts`); the running side
is `src/hub/`.
