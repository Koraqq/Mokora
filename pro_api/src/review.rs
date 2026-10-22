//! Game-review types: what a review of one recorded game holds and what the
//! 對局覆盤 page reads. Mirrored by hand in `frontend/src/types.ts`.
//!
//! A review replays a recorded game through a built-in model, from the
//! recorded player's seat, and keeps — for every decision the player had — the
//! model's ranking next to what was actually played. It is a measure of how
//! far the play is from that model's own policy, nothing more: no win rate, no
//! expected value, no judgement of right and wrong.

use serde::{Deserialize, Serialize};

/// The kind of action a decision was settled with. Statistics are kept per
/// kind, since "how often the model agrees on a discard" and "…on a pon" are
/// different questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    /// A plain discard (not a riichi declaration).
    Discard,
    Riichi,
    Pon,
    Chi,
    Daiminkan,
    Ankan,
    Kakan,
    Kita,
    /// Declaring a win, by ron or tsumo.
    Hora,
    /// Nine-terminals abortive draw.
    Ryukyoku,
    /// Declining a call: no pon / chi / kan / ron when one was possible.
    Pass,
}

/// Every kind, in the order the page lists them.
pub const REVIEW_KINDS: [ReviewKind; 11] = [
    ReviewKind::Discard,
    ReviewKind::Riichi,
    ReviewKind::Pon,
    ReviewKind::Chi,
    ReviewKind::Daiminkan,
    ReviewKind::Ankan,
    ReviewKind::Kakan,
    ReviewKind::Kita,
    ReviewKind::Hora,
    ReviewKind::Ryukyoku,
    ReviewKind::Pass,
];

/// One action, normalised so what was played and what the model ranked can be
/// compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAction {
    pub kind: ReviewKind,
    /// The tile discarded, called, or added (kakan). A riichi names the tile
    /// discarded with it when the game recorded one. `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pai: Option<String>,
    /// The tiles taken from the hand for a call or kan, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed: Vec<String>,
}

/// One row of the model's ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewOption {
    pub action: ReviewAction,
    /// The model's probability for it: a softmax over the legal actions.
    pub prob: f32,
}

/// One decision of the recorded player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDecision {
    /// Index of the event in the game's log the decision followed.
    pub idx: u32,
    /// The round (局): `E`/`S`/`W`, 1..=4, and the honba count.
    pub bakaze: String,
    pub kyoku: u8,
    pub honba: u8,
    /// How many discards the player had made in the round, plus one.
    pub turn: u16,
    /// A call window (somebody else's discard) rather than the player's turn.
    pub window: bool,
    /// What was played.
    pub actual: ReviewAction,
    /// The model's probability for it; `None` when the model did not rank it
    /// (see [`ReviewDiagnostics::not_ranked`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_prob: Option<f32>,
    /// The model's three most likely actions, best first.
    pub top: Vec<ReviewOption>,
    /// Only one action was legal.
    pub forced: bool,
    /// What was played is the model's first choice.
    pub matched: bool,
    /// A win was available and not declared.
    pub missed_win: bool,
    /// Fingerprint of what the model was shown (`Decision::obs_digest`), hex.
    pub obs: String,
}

/// Agreement with the model over the decisions of one kind. Forced decisions
/// (a single legal action) and decisions the model did not rank are counted
/// apart: neither says anything about the policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewKindStats {
    pub kind: ReviewKind,
    /// Decisions with a real choice that the model ranked.
    pub scored: u32,
    /// …of which what was played was the model's first choice.
    pub matched: u32,
    /// Sum of the model's probabilities for what was played, over `scored`.
    pub prob_sum: f64,
    /// Decisions of this kind with a single legal action.
    pub forced: u32,
}

/// What could not be read out of the game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewDiagnostics {
    /// Decision points the model was asked at.
    pub decision_points: u32,
    /// …whose played action could not be found in the log (it ended first).
    pub no_actual: u32,
    /// …whose played action was not among the actions the model ranked.
    pub not_ranked: u32,
}

/// The model a game was reviewed with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewModelInfo {
    /// Built-in model name (`mora-3p`, …).
    pub name: String,
    pub num_players: u8,
    /// FNV-1a of the weights, hex: another build with other weights does not
    /// pass for this one.
    pub weights_hash: String,
}

/// The fixed settings a review is made with — the same for every game, so two
/// reviews of the same game under the same model are identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSettings {
    /// Format / logic version; a change invalidates every saved review.
    pub version: u32,
    /// Rows of the ranking kept per decision.
    pub top_k: u8,
    /// `east` or `hanchan`, from the room, as the live bot reads it.
    pub game_length: String,
    /// The model plays by its policy alone: no sampling, no yakuman planner.
    pub deterministic: bool,
}

/// A saved review of one game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewReport {
    /// What makes a saved review current: version, model, weights, seat and
    /// game length. Another key means it has to be made again.
    pub key: String,
    pub game_id: String,
    /// FNV-1a of the game's event log, hex.
    pub game_hash: String,
    /// Ms since the Unix epoch.
    pub created_ms: u64,
    pub model: ReviewModelInfo,
    /// The model that played the game live (`bot.active_*` of its mode), when
    /// recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_bot: Option<String>,
    pub our_seat: u8,
    pub num_players: u8,
    pub settings: ReviewSettings,
    pub decisions: Vec<ReviewDecision>,
    pub kinds: Vec<ReviewKindStats>,
    pub diagnostics: ReviewDiagnostics,
}

/// The small file beside a report, so listing the games does not parse the
/// reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub key: String,
    pub model: ReviewModelInfo,
    pub created_ms: u64,
    pub decisions: u32,
    pub scored: u32,
    pub matched: u32,
}

/// One game on the page: whether it has been reviewed and with what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewRow {
    pub id: String,
    /// The model a review would use for this game now.
    pub model: String,
    pub reviewed: bool,
    /// Reviewed, but under another version / weights: it would be redone.
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReviewSummary>,
}

/// Progress of the review queue, pushed as `review-progress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewProgress {
    /// `idle`, `running`, `paused` (a live game is on), `cancelled`, `done`
    /// or `error`.
    pub state: String,
    pub total: u32,
    pub done: u32,
    /// The game being reviewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ReviewProgress {
    pub fn idle() -> Self {
        Self {
            state: "idle".into(),
            total: 0,
            done: 0,
            current: None,
            message: None,
        }
    }
}

/// Timing of one model call, in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LatencyStats {
    pub n: u32,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// What the self-check found; shown on the review page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelfCheckReport {
    pub at_ms: u64,
    /// Games read from the history (the recorded player's own).
    pub games: u32,
    /// Decisions across them.
    pub decisions: u32,
    /// The censor changed nothing in the recorded logs: they hold only what
    /// the player could see.
    pub logs_clean: bool,
    /// Games whose log was changed by the censor (should be 0).
    pub logs_changed: u32,
    /// Games re-run with every hidden tile planted with a real one: the
    /// observation fed to the model matched at every decision once censored.
    pub leak_games: u32,
    pub leak_decisions: u32,
    /// Decisions whose observation differed after the plant (should be 0).
    pub leak_mismatches: u32,
    /// For information only: how many decisions the plant would have changed
    /// had the review not censored it.
    pub leak_uncensored_mismatches: u32,
    /// Two runs of every game gave the same report.
    pub deterministic: bool,
    pub determinism_games: u32,
    /// Call windows read, how they were settled, and those not found.
    pub call_windows: u32,
    pub passes: u32,
    pub calls: u32,
    /// Calls and rons the player's own logs hold (pon, chi, daiminkan, ron).
    /// Every one must be a window the review found: `calls` equals this.
    pub calls_in_logs: u32,
    pub unresolved: u32,
    pub not_ranked: u32,
    /// Events of games the review beside the timing worked through while the
    /// second run was timed: proof it was really working during it.
    pub concurrent_events: u32,
    /// Model call timing: alone, and while a review runs beside it.
    pub latency_alone: LatencyStats,
    pub latency_with_review: LatencyStats,
    /// Plain-language findings, for the page.
    pub notes: Vec<String>,
}

/// Review one game ([`crate::method::REVIEW_RUN`]); the model's weights are
/// lent as the call's bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewRequest {
    /// The game's events as recorded (Mokora's mjai JSON, one per event).
    pub events: Vec<serde_json::Value>,
    /// The recorded player's seat.
    pub seat: u8,
    pub num_players: u8,
    /// The game's planned length: "east" or "hanchan".
    pub length: String,
    /// Blank what the seat could not see (always, but for the self-check).
    pub censor: bool,
    /// Which weights the bytes are, so the module can keep the model built.
    pub weights_hash: String,
}

/// What a review found ([`crate::method::REVIEW_RUN`]); `None` when Mokora
/// said to stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewRun {
    pub decisions: Vec<ReviewDecision>,
    pub kinds: Vec<ReviewKindStats>,
    pub diagnostics: ReviewDiagnostics,
    /// Call windows the model was asked at, and how each settled.
    pub call_windows: u32,
    pub passes: u32,
    pub calls: u32,
    /// Rows of the ranking kept per decision.
    pub top_k: u8,
}
