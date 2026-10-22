//! Backend ↔ frontend IPC payload types.
//!
//! These types travel two directions:
//!
//! - **Backend → frontend**: emitted as Tauri events by `crate::ipc`. Names
//!   match the Tauri event names (kebab-case): `notify`, `bot-status`,
//!   `proxy-status`.
//! - **Frontend → backend**: returned from `#[tauri::command]` handlers in
//!   `crate::ipc::commands`.
//!
//! All types are `Serialize + Deserialize` so they round-trip through Tauri's
//! JSON bridge and through the in-process broadcast buses in
//! `crate::event_bus`. Variants are tagged so the wire shape stays stable
//! when new states are added — frontends can match on `state` / `level` /
//! `stage` without positional surprises.

use serde::{Deserialize, Serialize};

// ---------- Notification ----------

/// Severity for a frontend toast / notification box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLevel {
    Info,
    Success,
    Warn,
    Error,
}

/// One frontend-facing notification. Any subsystem may push these onto
/// `event_bus::NotifyBus`; `crate::ipc` forwards them to all webviews as
/// the `notify` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub level: NotifyLevel,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// `true` → frontend keeps the toast until the user dismisses it.
    /// Used for long-running blocking states (e.g. "installing bot deps").
    #[serde(default)]
    pub sticky: bool,
    /// Stable key. Frontends should replace any prior toast with the same
    /// `id`, so progress updates don't pile up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl Notification {
    pub fn info(title: impl Into<String>) -> Self {
        Self::new(NotifyLevel::Info, title)
    }
    pub fn success(title: impl Into<String>) -> Self {
        Self::new(NotifyLevel::Success, title)
    }
    pub fn warn(title: impl Into<String>) -> Self {
        Self::new(NotifyLevel::Warn, title)
    }
    pub fn error(title: impl Into<String>) -> Self {
        Self::new(NotifyLevel::Error, title)
    }

    fn new(level: NotifyLevel, title: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            body: None,
            sticky: false,
            id: None,
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn sticky(mut self) -> Self {
        self.sticky = true;
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

// ---------- BotStatus ----------

/// Sub-stage during the slow first-spawn path.
///
/// `SyncingDeps` is the long one — `uv sync` downloads every wheel listed
/// in the bot's `pyproject.toml` into `<bot_dir>/.mokora/venv` on first use,
/// which can take tens of seconds. `Spawning` is the short tail (process
/// creation + waiting for the first react round-trip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadStage {
    SyncingDeps,
    Spawning,
}

/// Lifecycle state of the active bot subprocess.
///
/// Tagged with `state` so the frontend can match on a string discriminant
/// (`{"state":"loading","bot":"mortal","stage":"syncing_deps"}`) without
/// having to know Rust's untagged-enum trick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BotStatus {
    /// No active bot — waiting for `start_game` (or bot disabled).
    Idle,
    /// First-time install / spawn in progress. UI should show a spinner.
    Loading { bot: String, stage: LoadStage },
    /// Subprocess up and reacting.
    Ready { bot: String, actor_id: u8 },
    /// Spawn or react path failed; runner torn down.
    Error { bot: String, error: String },
    /// Game ended (`EndGame`) — runner dropped, awaiting next start.
    Stopped { bot: String },
}

// ---------- CaptureStatus ----------

/// Discriminant of the active capture transport. Mirrors
/// [`crate::capture::CaptureKind`] for IPC payloads — re-exported here so
/// `schema::*` is a self-contained surface for frontend type generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Mitm,
    Chromium,
}

/// Lifecycle of the active capture backend.
///
/// `descriptor` carries a human-readable label (proxy listen addr for
/// MITM, executable path for Chromium) — surface in UI, do not parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CaptureStatus {
    Stopped,
    Starting {
        kind: CaptureKind,
        descriptor: String,
    },
    Running {
        kind: CaptureKind,
        descriptor: String,
    },
    Error {
        kind: CaptureKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        descriptor: Option<String>,
        error: String,
    },
}

// ---------- NextGameStatus ----------

/// Where the next-game flow (`autoplay::next_game`) is between ranked games,
/// shown on the Game page's bot-action tile. `Idle` the rest of the time: in
/// a game, or with the flow off, finished or stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NextGameStatus {
    #[default]
    Idle,
    /// Resting in the lobby (`rest_minutes`) until `until_ms` (Unix ms), then
    /// queuing for `next`.
    Resting { until_ms: i64, next: NextGameTarget },
    /// Pressing through the lobby for `next` and waiting for the match.
    Queueing { next: NextGameTarget },
    /// Logging out and back in — on gateway `route` (`ping_ms` to connect)
    /// when the fastest one was picked — and waiting for the lobby.
    Relogging {
        route: Option<String>,
        ping_ms: Option<u32>,
    },
    /// Stopped: `gold` copper coins are short of the `need` of `room`'s
    /// table, and today's revive coins are claimed (or did not cover it).
    /// Stays until the next run, so the frontend can say so in the middle
    /// of the screen.
    Blocked {
        room: crate::config::RankRoom,
        gold: u64,
        need: u64,
    },
}

/// The game the flow is about to queue for, as far as it is known yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextGameTarget {
    /// `None` when it is worked out from a rank not known yet.
    pub room: Option<crate::config::RankRoom>,
    /// The ladder, when the room follows the rank (`RoomMode::Highest*`).
    pub players: Option<u8>,
    /// The table, when set by hand; `None` = whichever has more players.
    pub rule: Option<crate::config::NextGameRule>,
}

// ---------- AccountProfile ----------

/// A level and the points within it (`account.level` / `level3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelScore {
    pub id: u32,
    pub score: u32,
}

/// The biggest hand a player has won (the game's 精選和牌), in mjai tiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HighestHand {
    /// The concealed tiles, as the game lists them.
    pub tiles: Vec<String>,
    /// The tile it was won on.
    pub win_tile: String,
    /// Each call's tiles.
    pub melds: Vec<Vec<String>>,
    pub han: u32,
}

/// One game of the player's, as the game itself keeps it (its recent games
/// list, `fetchAccountInfoExtra`) — every game, played through Mokora or not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MajsoulRecentGame {
    /// The game's uuid, as the history records it (`MatchInfo::game_uuid`).
    pub uuid: String,
    /// The ranked table's mode id (`sub_tag`); `tag` 1 is ranked.
    pub mode_id: u32,
    pub ranked: bool,
    /// Unix seconds.
    pub start_time: i64,
    pub end_time: i64,
    pub rank: u32,
    pub final_point: i64,
}

/// Achievements by rarity: the game's gold, silver and bronze counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Achievements {
    pub gold: u32,
    pub silver: u32,
    pub bronze: u32,
}

/// What Mahjong Soul's own statistics say about one ladder's ranked games
/// (`fetchAccountStatisticInfo`, sent when the player opens their own
/// profile in the game).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MajsoulRankStats {
    /// 3 or 4.
    pub players: u8,
    /// Games finished in each place, first place first.
    pub placements: Vec<u32>,
    /// Of the last `recent_rounds` rounds: won by ron, by tsumo, dealt in.
    pub recent_rounds: u32,
    pub recent_ron: u32,
    pub recent_tsumo: u32,
    pub recent_deal_in: u32,
    /// Han of the biggest hand ever won (13+ = yakuman), and its title id.
    pub highest_han: Option<u32>,
    pub highest_title: Option<u32>,
    /// Average points of the last 20 wins.
    pub average_win: Option<u32>,
    /// Yakuman won in ranked games, by Mahjong Soul's yaku id (35 tenhou …
    /// 50 daisuushii), most first.
    pub yakuman: Vec<(u32, u32)>,
    /// Every yaku won in ranked games, `(yaku id, wins)` by id — the game's
    /// own list (1 menzen tsumo … 34 kita dora, 35 … 50 yakuman).
    pub yaku: Vec<(u32, u32)>,
    /// Nagashi mangan.
    pub nagashi: u32,
    /// The biggest hand won (精選和牌).
    pub highest_hand: Option<HighestHand>,
}

/// The logged-in player as the game's own traffic describes them: coins,
/// ranks, today's revive coins and the game's statistics. Kept per account
/// in `profile.json` beside its config, so it shows with Mokora offline too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AccountProfile {
    pub account_id: Option<u64>,
    pub nickname: Option<String>,
    /// The id of the player's main character (雀士), from `fetchCharacterInfo`.
    /// The base character, whatever skin is worn; `frontend/public/characters/`
    /// has its icon by this id.
    pub main_character: Option<u32>,
    /// Copper coins (銅幣) — what a ranked table asks to enter.
    pub gold: Option<u64>,
    /// Jade (玉).
    pub diamond: Option<u64>,
    pub level4: Option<LevelScore>,
    pub level3: Option<LevelScore>,
    /// Today's revive coins (復活幣): `Some(true)` claimed, `Some(false)`
    /// still to claim, `None` not known for today.
    pub revive_claimed: Option<bool>,
    /// The game's own statistics, per ladder, when last seen.
    pub stats: Vec<MajsoulRankStats>,
    pub stats_at_ms: Option<i64>,
    pub achievements: Option<Achievements>,
    /// The game's own list of the player's recent games, newest first.
    pub recent_games: Vec<MajsoulRecentGame>,
    /// When any of it last changed (Unix ms).
    pub updated_ms: i64,
}

// ---------- HoraScoreInfo ----------

/// Bot's hora-decision score breakdown, computed on demand by
/// `commands::compute_bot_hora_score`. Returned as `Option<Self>` —
/// `None` when no game is in progress, the actor's hand isn't a valid
/// agari shape, or the winning tile can't be inferred from the live state.
///
/// `points` is the actor's *received* total: for ron, the discarder's pay;
/// for tsumo, the sum across all paying seats. Honba/riichi-stick payouts
/// are NOT folded in (those depend on platform settlement).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoraScoreInfo {
    pub points: u32,
    pub han: u32,
    pub fu: u32,
    pub yakuman: bool,
    /// mjai tile string (e.g. `"5mr"`, `"C"`) — the tile the actor wins on.
    /// For ron this is the most recent discard; for tsumo the most recent draw.
    pub win_tile: String,
}

// ---------- Snapshot / BotInfo ----------
//
// Returned by `commands::get_status` and `commands::list_bots`. These
// types pull in `AppConfig` (one direction only — schema doesn't cycle
// back into config). Keep these stable; the React side keys off field
// names.

use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::PathBuf;

/// One discovered bot, IPC-shaped (separate from `bot::BotEntry` so the
/// wire contract can evolve independently of the registry internals).
///
/// `manifest` carries the bot's settings schema (rendered as a form on
/// the frontend). Bots without a `manifest.toml` have it set to `None`
/// and the UI hides the settings panel for them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotInfo {
    pub name: String,
    pub dir: String,
    pub has_pyproject: bool,
    /// `true` when the bot's Python environment is already installed (no
    /// `pyproject.toml`, or its venv + sync stamp are up to date). The UI
    /// blocks activating a bot whose env isn't ready so a game never picks
    /// a bot that would need a slow first-spawn `uv sync`.
    #[serde(default)]
    pub env_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<crate::bot::manifest::Manifest>,
}

/// Returned by the `get_bot_settings` command. `values` is the result of
/// merging defaults from `manifest` with whatever lives in the bot's
/// `settings.toml`. Frontend should treat `manifest.settings` as the form
/// schema and `values` as the form's current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotSettings {
    pub manifest: crate::bot::manifest::Manifest,
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
}

/// One-shot dump of everything the UI needs on startup. Cheap to clone
/// (config + two enums + a path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub config: AppConfig,
    pub bot_status: BotStatus,
    pub capture_status: CaptureStatus,
    pub log_dir: PathBuf,
}

// ---------- Logs ----------
//
// One canonical shape for an emitted tracing event: written to disk as a
// line of `all.jsonl` AND broadcast to the frontend log viewer over a
// `tauri::ipc::Channel`. Keeping disk and wire shapes identical means the
// initial-load reader and the live-tail subscriber feed the same UI rows
// without a translation step.

/// One log event. `ts_ms` is millisecond Unix time (local clock at emit).
/// `fields` carries the structured fields recorded via `tracing` (the
/// canonical `message` is hoisted to its own slot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts_ms: i64,
    pub level: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, serde_json::Value>,
}

/// Metadata for one log session directory under `<log_root>/`. Sorted
/// newest-first by `name` (timestamp). `is_active` marks the session
/// owned by the running process — the UI uses this to enable live tail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSessionInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub mtime_ms: i64,
    pub is_active: bool,
}

/// Filtered, paginated read against a single session's `all.jsonl`.
/// `limit` is capped server-side at 2000 to bound payload size.
/// `levels`/`targets`/`search` are AND-ed; within each, multiple entries
/// are OR-ed (level in set; any target prefix matches; substring on
/// `message` is case-insensitive).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadLogRequest {
    pub session: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadLogResponse {
    pub entries: Vec<LogEntry>,
    pub has_more: bool,
    pub skipped_malformed: u32,
}

// ---------- Inspector ----------
//
// Mirrors the on-disk `<session>/inspector.jsonl` shape for past-session
// reads. Same `kind` filtering / pagination model as `ReadLogRequest`.

/// Filtered, paginated read against a session's `inspector.jsonl`.
/// `kinds` accepts `"ws_frame"`, `"mjai_event"`, `"bot_reaction"`.
/// `actor` filters records that reference an actor (mjai events with an
/// `actor` field; bot reactions by `actor_id`); ws frames are
/// always included regardless of actor unless `kinds` excludes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadInspectorRequest {
    pub session: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<u8>,
    /// Case-insensitive substring on the entry's text representation
    /// (frame raw text, mjai event JSON, bot action JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadInspectorResponse {
    pub entries: Vec<crate::schema::InspectorEntry>,
    pub has_more: bool,
    pub skipped_malformed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_minimal_round_trips() {
        let n = Notification::info("hi");
        let s = serde_json::to_string(&n).unwrap();
        // No body / id; sticky=false is emitted (no skip_if).
        assert_eq!(s, r#"{"level":"info","title":"hi","sticky":false}"#);
        let back: Notification = serde_json::from_str(&s).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn notification_full_round_trips() {
        let n = Notification::error("boom")
            .body("stack trace here")
            .sticky()
            .id("bot-load-fail");
        let s = serde_json::to_string(&n).unwrap();
        let back: Notification = serde_json::from_str(&s).unwrap();
        assert_eq!(back, n);
        assert!(s.contains(r#""level":"error""#));
        assert!(s.contains(r#""sticky":true"#));
        assert!(s.contains(r#""id":"bot-load-fail""#));
    }

    #[test]
    fn bot_status_loading_tagged_correctly() {
        let s = BotStatus::Loading {
            bot: "mortal".into(),
            stage: LoadStage::SyncingDeps,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(
            j,
            r#"{"state":"loading","bot":"mortal","stage":"syncing_deps"}"#
        );
        let back: BotStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn bot_status_ready_round_trips() {
        let s = BotStatus::Ready {
            bot: "example".into(),
            actor_id: 2,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"state":"ready","bot":"example","actor_id":2}"#);
        let back: BotStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn bot_status_idle_has_no_extra_fields() {
        let s = BotStatus::Idle;
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"state":"idle"}"#);
    }

    #[test]
    fn capture_status_running_round_trips() {
        let s = CaptureStatus::Running {
            kind: CaptureKind::Mitm,
            descriptor: "127.0.0.1:23410".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: CaptureStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
        assert!(j.contains(r#""state":"running""#));
        assert!(j.contains(r#""kind":"mitm""#));
        assert!(j.contains(r#""descriptor":"127.0.0.1:23410""#));
    }

    #[test]
    fn capture_status_chromium_running() {
        let s = CaptureStatus::Running {
            kind: CaptureKind::Chromium,
            descriptor: "/usr/bin/google-chrome".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains(r#""kind":"chromium""#));
        let back: CaptureStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn capture_status_error_omits_null_descriptor() {
        let s = CaptureStatus::Error {
            kind: CaptureKind::Chromium,
            descriptor: None,
            error: "spawn failed".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains(r#""state":"error""#));
        assert!(j.contains(r#""kind":"chromium""#));
        // skip_serializing_if = Option::is_none → no descriptor key
        assert!(!j.contains("descriptor"), "got: {j}");
    }

    #[test]
    fn capture_status_stopped_minimal() {
        let s = CaptureStatus::Stopped;
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"state":"stopped"}"#);
    }

    #[test]
    fn log_entry_round_trips_minimal() {
        // Minimal entry with no file/line/fields — empty fields map and
        // missing optional file/line should not appear in the JSON.
        let e = LogEntry {
            ts_ms: 1_700_000_000_000,
            level: "INFO".into(),
            target: "mokora::lib".into(),
            file: None,
            line: None,
            message: "hello".into(),
            fields: HashMap::new(),
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(!j.contains("file"), "got: {j}");
        assert!(!j.contains("line"), "got: {j}");
        assert!(!j.contains("fields"), "got: {j}");
        let back: LogEntry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn log_entry_round_trips_full() {
        let mut fields = HashMap::new();
        fields.insert("seat".into(), serde_json::Value::Number(2.into()));
        fields.insert("kind".into(), serde_json::Value::String("ron".into()));
        let e = LogEntry {
            ts_ms: 1_700_000_000_000,
            level: "WARN".into(),
            target: "mokora::proxy::handler".into(),
            file: Some("src/proxy/handler.rs".into()),
            line: Some(42),
            message: "stalled".into(),
            fields,
        };
        let j = serde_json::to_string(&e).unwrap();
        let back: LogEntry = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }
}
