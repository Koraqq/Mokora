# Example Bot — Rule-based Shanten Optimizer

A first-party rule-based bot that ships with Mokora v3 to validate the
end-to-end pipeline (proxy → bridge → bus → bot manager → bot).

## How Mokora invokes this bot

- Working dir: this folder.
- Argv: `python bot.py <player_id>` (mjai.app convention).
- Env: `MOKORA_PLAYER_ID=<player_id>` (kept for parity).
- Stdin: one JSON array of `MjaiEvent`s per line.
- Stdout: one JSON object (mjai action) per line; `{"type":"none"}`
  when no action is owed.
- Stderr: free-form logs; Mokora pumps each line into `tracing` with
  `bot=example`.

`start_game.id` is treated as authoritative for seat assignment — argv
and env are just early hints that get overridden when the protocol kicks
in.
