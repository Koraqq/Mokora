// What was run when the review was built — a fixed record, not live numbers
// (the 驗證 button above runs the same checks again on the games as they are
// now). Debug build, as `start-Mokora.bat` runs it.

export const REVIEW_DEV_RESULTS = {
  date: '2026-10-11',
  version: 1,
  tests: {
    // `cargo test --lib`, `native_bot` and `vitest run`.
    backend: 1017,
    nativeBot: 75,
    frontend: 150,
    review: [
      'what the model is shown at each decision equals what the live path is shown',
      'the censor blanks only what the player could not see',
      'a pass is rebuilt from the log: no call by the player, a rival\'s call, a riichi acceptance, a double ron',
      'the played action is found among the ranked ones, red fives merged only in calls',
      'statistics per kind, forced decisions left out',
      'a review is repeatable, refuses a model of the wrong player count, stops on request',
      'a reviewed game is not reviewed again; a stale one is; a cancelled queue saves nothing',
      'the self-check passes on a clean game; the plant fills only hidden seats',
    ],
  },
  realGames: {
    games: 30,
    decisions: 2002,
    callWindows: 289,
    passes: 212,
    calls: 77,
    callsInLogs: 77,
    leakGames: 6,
    leakDecisions: 397,
    leakMismatches: 0,
    queueGames: 8,
    queueSeconds: 8.8,
  },
  latency: {
    aloneP50: 12.2,
    aloneP95: 17.6,
    aloneMax: 24.6,
    withP50: 11.1,
    withP95: 15.9,
    withMax: 22.0,
    events: 613,
  },
}
