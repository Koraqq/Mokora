# `src/review/` — Game review (對局覆盤)

Replays a recorded game through a built-in model, from the recorded player's
seat, and keeps for every decision the player had the model's ranking next to
what was played. The 對局覆盤 page reads the result.

It is **analysis only**. A review is read-only: it changes no model's weights,
and nothing that decides a live move reads it. What it says is how far the play
was from that model's own policy ("AI 一致度"), not a win rate, an expected
value or a verdict on right and wrong. Making a model stronger is a job for
training, never for a review.

```
history/games/<id>.mjai.jsonl ─▶ replay ─▶ history/reviews/<id>.<model>.json      (the report)
                                                         └ <id>.<model>.sum.json   (a small summary)
```

## Modules

- **The replay** — one game fed to the engine event by event, the decisions
  read off it and set beside what was played — is a paid feature, in the
  module Mokora downloads once a key is redeemed (`pro_api::method::
  REVIEW_RUN`). `mod.rs` hands it the game, the model's weights and a tick
  it asks between events (cancel, pause, pacing: `job.rs`'s), and makes the
  report of what comes back. `censor` here blanks what a seat other than the
  player's holds or draws, for the self-check.
- **`store.rs`** — the saved reviews. A review is current only while its key
  (version, model, weights fingerprint, seat, game length) equals the key a
  review made now would carry; otherwise it is stale and made again.
- **`job.rs`** — the queue: one game at a time on a thread below normal
  priority, resting between events. Games that already have a current review
  are skipped. It waits while a live game is on (an event on the mjai bus in
  the last 45 s, or the lobby flow in a game) unless told not to.
- **`selfcheck.rs`** — the checks, on the player's real games: no hidden tile
  in any log; a leak plant (every hidden tile replaced by a real one) changes
  nothing the model is shown; two runs agree; the review finds every call and
  ron the logs hold and settles every other window as a pass; and the model
  call's timing alone and beside a running review. Result kept in
  `history/reviews/selfcheck.json`.

## Fixed settings

The same for every game, so two reviews of one game under one model agree:
the model's policy alone (no sampling, no yakuman planner), the game length
from the room as the live bot reads it (`east` / `hanchan`), the top three
rows kept per decision. The reviewer is the strongest built-in model of the
game's player count unless another is picked; Yakuman is never a reviewer.
`REVIEW_VERSION` (in `mod.rs`) is bumped when any of it changes, which makes
every saved review stale.

## What is counted

Statistics are kept per kind of action played (discard, riichi, pon, chi,
daiminkan, ankan, kakan, kita, win, nine terminals, pass). A decision with a
single legal action, and one the model did not rank, count toward neither
agreement nor mean probability. "Low model support" is only the model's
probability for the played action being under a threshold the player sets
(default 5%); it is a filter, not a judgement.

## Tests

`cargo test --lib review` runs the tests that need no module (the cache, the
censor). The rest review through the paid module: set `MOKORA_PRO_DLL` to a
built one and run `cargo test --lib -- --ignored` — among them, what the
model is shown at each decision equals what the live path is shown (the same
`Decision::obs_digest`, one event at a time versus the whole prefix at once),
and the queue's skip-and-cancel. Two of them run on a real history too: set
`REVIEW_HISTORY_DIR` to a copy of a `history/` folder.
