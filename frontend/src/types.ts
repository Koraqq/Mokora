// Mirrors backend schema. Source: frontend/README.md.

export type MjaiEvent =
  | { type: 'start_game'; names: string[]; kyoku_first?: number; aka_flag?: boolean; id?: number; num_players?: number }
  | { type: 'start_kyoku'; bakaze: string; dora_marker: string; kyoku: number; honba: number; kyotaku: number; oya: number; scores: number[]; tehais: string[][]; num_players?: number }
  | { type: 'tsumo'; actor: number; pai: string }
  | { type: 'dahai'; actor: number; pai: string; tsumogiri: boolean }
  | { type: 'chi'; actor: number; target: number; pai: string; consumed: [string, string] }
  | { type: 'pon'; actor: number; target: number; pai: string; consumed: [string, string] }
  | { type: 'daiminkan'; actor: number; target: number; pai: string; consumed: [string, string, string] }
  | { type: 'kakan'; actor: number; pai: string; consumed: [string, string, string] }
  | { type: 'ankan'; actor: number; consumed: [string, string, string, string] }
  | { type: 'dora'; dora_marker: string }
  | { type: 'reach'; actor: number; pai?: string }
  | { type: 'reach_accepted'; actor: number }
  | { type: 'hora'; actor: number; target: number; deltas?: number[]; ura_markers?: string[] }
  | { type: 'ryukyoku'; deltas?: number[] }
  | { type: 'kita'; actor: number; pai?: string }
  | { type: 'end_kyoku' }
  | { type: 'end_game' }
  | { type: 'none' }

export type BotResponse = MjaiEvent & { meta?: Record<string, unknown> }

/** Bot-driven custom display payload, attached on `meta.show`.
 *  Schema is intentionally generic so a single tile can render top-N
 *  actions, opponent reads, yaku breakdowns, etc. — bots decide the
 *  semantics and formatting. */
export type ShowItem = {
  /** Primary text on the row. */
  label?: string
  /** mjai tile strings; converted to mahgen via `mjaiToMahgen`. */
  pais?: string[]
  /** Raw mahgen DSL string. Wins over `pais` if both are set. */
  tiles?: string
  /** Right-side text (any format — e.g. "85.42%", "+12000"). */
  value?: string
  /** Hex accent color (e.g. "#00ff80") — applied as left bar + faint row tint. */
  color?: string
  /** Small subtitle under `label`. */
  note?: string
}

export type ShowMeta = {
  /** Optional title; falls back to the tile's default title. */
  title?: string
  items: ShowItem[]
}

export type BotStatus =
  | { state: 'idle' }
  | { state: 'loading'; bot: string; stage: 'syncing_deps' | 'spawning' }
  | { state: 'ready'; bot: string; actor_id: number }
  | { state: 'error'; bot: string; error: string }
  | { state: 'stopped'; bot: string }

export type CaptureKind = 'mitm' | 'chromium'

export type CaptureStatus =
  | { state: 'stopped' }
  | { state: 'starting'; kind: CaptureKind; descriptor: string }
  | { state: 'running'; kind: CaptureKind; descriptor: string }
  | { state: 'error'; kind: CaptureKind; descriptor?: string; error: string }

export type Notification = {
  level: 'info' | 'success' | 'warn' | 'error'
  title: string
  body?: string
  sticky: boolean
  id?: string
}

export type CaptureMode = 'embedded' | 'mitm' | 'chromium'

export type ChromiumConfig = {
  executable: string
  user_data_dir: string
  start_url: string
  cft_channel: string
  force_cft: boolean
  extra_args: string[]
}

export type CaptureConfig = {
  mode: CaptureMode
  chromium: ChromiumConfig
}

export type DetectedBrowser = {
  kind: 'chrome' | 'edge' | 'brave' | 'chromium' | 'chrome_for_testing'
  path: string
}

/// Bridge selector (the runtime kind, not the history-record tag — those
/// share names but the schema enum carries extra archive-only variants).
/// Mirrors `src/config/platform.rs::Platform` (`#[derive(Serialize)]` →
/// PascalCase JSON: `"Majsoul"`, `"Tenhou"`).
export type PlatformKind = 'Majsoul'

export type MajsoulAutoplayConfig = {
  pre_click_delay_min_ms: number
  pre_click_delay_max_ms: number
  inter_click_delay_ms: number
  hover_delay_ms: number
  click_hold_ms: number
  /** Wait this long for the client's own input command after a click
   *  before pressing again; 0 disables verification. */
  verify_input_ms: number
  /** Retries when no input command follows a click sequence; 0 = log only. */
  click_retries: number
  /** Reload the game page after this many dead decisions in a row; 0 = off. */
  reload_after_failures: number
  dealer_first_discard_extra_delay_ms: number
}

/** Which ranked room the next game is queued in. Mirrors
 *  `src/config/autoplay.rs::RankRoom` (snake_case). */
export type RankRoom = 'highest' | 'bronze' | 'silver' | 'gold' | 'jade' | 'throne'

/** How the next game's room is chosen. */
export type RoomMode = 'highest_four' | 'highest_three' | 'manual'

/** Which table of the room. `same` repeats the last game's table. */
export type NextGameRule =
  | 'same'
  | 'four_east'
  | 'four_south'
  | 'three_east'
  | 'three_south'

/** The game the next-game flow is about to queue for, as far as it is known
 *  yet. Mirrors `src/schema/ipc.rs::NextGameTarget`. */
export type NextGameTarget = {
  /** Null while it is worked out from a rank not known yet. */
  room: RankRoom | null
  /** The ladder, when the room follows the rank. */
  players: 3 | 4 | null
  /** The table, when set by hand; null = whichever has more players. */
  rule: NextGameRule | null
}

/** Where the next-game flow is between games. Mirrors
 *  `src/schema/ipc.rs::NextGameStatus`. */
export type NextGameStatus =
  | { state: 'idle' }
  | { state: 'resting'; until_ms: number; next: NextGameTarget }
  | { state: 'queueing'; next: NextGameTarget }
  | { state: 'relogging'; route: string | null; ping_ms: number | null }
  /** Stopped for want of copper coins (today's revive coins claimed). */
  | { state: 'blocked'; room: RankRoom; gold: number; need: number }

/** How a list is moved to its edge. */
export type ListScroll = 'wheel' | 'drag'

/** Start the next ranked game by itself (Majsoul). Mirrors
 *  `src/config/autoplay.rs::NextGameConfig`. */
export type NextGameConfig = {
  enabled: boolean
  /** How the room is chosen: the highest room of a ladder (East or South by
   *  player count), or `manual` = `room` and `rule`. */
  room_mode: RoomMode
  room: RankRoom
  rule: NextGameRule
  /** Stop after this many games started this way in a session; 0 = no limit. */
  max_games: number
  /** Start no game by itself once the account has played this many today;
   *  0 = no limit, at most `MAX_GAMES_PER_DAY`. */
  max_games_per_day: number
  /** Minutes to rest in the lobby between games. 0 = no rest. */
  rest_minutes: number
  /** Log out and back in this many times after each game (0-3). */
  relogins_after_game: number
  /** On the login screen, pick the route with the quickest connect. */
  fastest_route: boolean
  /** Stop and leave every screen to the player when a game changes the rank. */
  stop_on_rank_change: boolean
  /** Open the profile window from the lobby to refresh the 個人資料 page. */
  refresh_profile: boolean
  /** Wait before the first press of the result screen's confirm button. */
  advance_delay_min_ms: number
  advance_delay_max_ms: number
  /** Presses of the confirm button before giving up on reaching the lobby. */
  max_confirm_presses: number
  /** How long a lobby stage waits for the client's proof before retrying. */
  step_timeout_ms: number
  scroll: ListScroll
}

/** Pre-click delay model parameters. Mirrors
 *  `src/config/autoplay.rs::DelayModelConfig`; the fine-grained knobs are
 *  config-file-only — the Settings UI exposes the Lua script fields. */
export type DelayMode = 'legacy' | 'lua'

export type DelayModelConfig = {
  /** Which policy is active; exactly one. `legacy` = the old fixed
   *  uniform model, `lua` = the scriptable human-like model backed by
   *  `delay.lua` next to the config file (auto-generated). */
  mode: DelayMode
  /** UI-readiness floor: minimum total thinking time per decision, ms.
   *  Clicks issued before Mahjong Soul renders the UI are lost. */
  min_delay_ms: number
  /** Higher floor for decisions that click an action button (chi/pon/
   *  kan/ron/skip/riichi) — buttons render after the discard animation
   *  plus their own pop-in, later than hand tiles. */
  min_button_delay_ms: number
  distribution: 'uniform' | 'log_normal'
  /** Per-decision-kind log-normal `[mu, sigma]` in ln(seconds); keys like
   *  `dahai_tedashi`, `claim`. Calibrated from ranked-game records. */
  lognormal: Record<string, [number, number]>
  bank_on_long_thought: boolean
  riichi_extra_ms: number
  kan_extra_ms: number
  safety_margin_ms: number
  bank_use_fraction: number
  bank_max_single_ms: number
  no_budget_cap_ms: number
}

export type AutoplayConfig = {
  enabled: boolean
  majsoul: MajsoulAutoplayConfig
  delay: DelayModelConfig
  next_game: NextGameConfig
}

/** The always-on-top suggestion overlay. Mirrors `crate::config::OverlayConfig`. */
export type OverlayConfig = {
  enabled: boolean
  top_n: number
  opacity: number
  always_on_top: boolean
}

/** How GitHub-hosted downloads are routed. Mirrors
 *  `crate::config::GithubMirrorMode` (serde snake_case). */
export type GithubMirrorMode = 'auto' | 'direct' | 'mirror'

/** `[network]` section. Mirrors `crate::config::NetworkConfig`. */
export type NetworkConfig = {
  github_mirror_mode: GithubMirrorMode
  /** gh-proxy-style accelerator prefix (e.g. `https://gh-proxy.com`);
   *  tried before the built-in mirror list. Empty = unset. */
  github_custom_mirror: string
}

/** `[privacy]` section. Mirrors `crate::config::PrivacyConfig`: what Mokora
 *  writes about the game's traffic. The game's own telemetry is still
 *  `proxy.block_telemetry`. */
export type PrivacyConfig = {
  /** Mask login credentials in the protocol logs and the inspector. */
  redact_credentials: boolean
}

/** Bounds enforced by `crate::config::overlay` — mirrored so the UI can't
 *  offer a value the backend would silently clamp. */
export const OVERLAY_TOP_N_MIN = 1
export const OVERLAY_TOP_N_MAX = 5
export const OVERLAY_OPACITY_MIN = 0.3
export const OVERLAY_OPACITY_MAX = 1.0

export type AppConfig = {
  general: {
    first_run_completed: boolean
    /** The window's × keeps Mokora running in the background (tray icon). */
    keep_running_on_close: boolean
    /** Rest the game pages while the window is out of sight and nothing is being played. */
    idle_power_saving: boolean
  }
  logging: { dir: string; level: string; all_level: string }
  platform: { kind: PlatformKind }
  proxy: { enabled: boolean; addr: string; ca_dir: string; block_telemetry: boolean }
  bot: {
    enabled: boolean
    active_4p: string
    active_3p: string
    auto_sync: boolean
    dir: string
  }
  capture: CaptureConfig
  autoplay: AutoplayConfig
  overlay: OverlayConfig
  network: NetworkConfig
  privacy: PrivacyConfig
}

export type FieldKind = 'string' | 'bool' | 'int' | 'float' | 'enum'

export type FieldSpec = {
  type: FieldKind
  label: string
  default: unknown
  help?: string
  secret?: boolean
  min?: number
  max?: number
  step?: number
  choices?: string[]
}

export type Manifest = {
  manifest_version: number
  bot: {
    name: string
    display?: string
    description?: string
    version?: string
    /** Game modes this bot can play. Backend defaults to `["4p"]` when absent. */
    supported_modes: string[]
  }
  source?: { type: 'github_release'; repo: string; asset_glob?: string }
  settings: Record<string, FieldSpec>
}

export type BotInfo = {
  name: string
  dir: string
  has_pyproject: boolean
  /** Bot's Python environment is installed and ready (no slow first-spawn sync). */
  env_ready: boolean
  manifest?: Manifest
}

export type BotSettings = {
  manifest: Manifest
  values: Record<string, unknown>
}

export type Snapshot = {
  config: AppConfig
  bot_status: BotStatus
  capture_status: CaptureStatus
  log_dir: string
}

export type WaitInfo = { tile: string; left: number; agari_rate: number | null }

export type ImproveEntry = { draw: string; widened_waits: WaitInfo[]; widened_total: number }

export type Hand13Result = {
  shanten: number
  waits: WaitInfo[]
  waits_total: number
  next_shanten_waits_count: { [tileIdx: number]: number }
  avg_next_shanten_waits: number
  mixed_waits_score: number
  avg_agari_rate: number
  is_furiten: boolean
  furiten_rate: number
  improves: ImproveEntry[]
  improve_way_count: number
  avg_improve_waits_count: number
  dama_point: number
  riichi_point: number
  mixed_round_point: number
  yaku_ids: number[]
}

export type DiscardCandidate = { discard: string; result: Hand13Result }

export type Hand14Result = {
  shanten: number
  maintain: DiscardCandidate[]
  backwards: DiscardCandidate[]
}

export type OpponentRisk = {
  seat: number
  tenpai_rate: number
  risk: number[]
  is_riichi: boolean
}

export type AnalysisResult = {
  seat: number
  turn: number
  shanten: number
  state: 'wait13' | 'discard14'
  hand13: Hand13Result | null
  hand14: Hand14Result | null
  opponents: OpponentRisk[]
  mixed_risk: number[]
  best_attack_discard: string | null
  best_defence_discard: string | null
}

export type DiscardEntry = {
  tile: string
  tedashi: boolean
  is_riichi: boolean
  /** Claimed by another player (pon/chi/kan); kept for analysis, hidden in the
   * rendered river. */
  called?: boolean
  /** Place in the round's table-wide discard order, when known. */
  seq?: number
}

export type MeldSnapshot = {
  kind: 'chi' | 'pon' | 'daiminkan' | 'ankan' | 'kakan'
  tiles: string[]
  from_who: number
  called_tile: string | null
}

export type PlayerSnapshot = {
  seat: number
  tehai: string[]
  melds: MeldSnapshot[]
  river: DiscardEntry[]
  score: number
  riichi_declared: boolean
  riichi_stage: boolean
  double_riichi: boolean
  riichi_declaration_index: number | null
  /** 3p only: north tiles set aside via kita / nukidora. Empty in 4p. */
  kita_tiles: string[]
}

export type GameStateSnapshot = {
  bakaze: 'E' | 'S' | 'W' | 'N'
  kyoku: number
  honba: number
  kyotaku: number
  oya: number
  current_player: number
  turn_count: number
  phase: 'wait_act' | 'wait_response'
  is_done: boolean
  /** 3 (sanma) or 4 (yonma). */
  num_players: number
  /** Length matches num_players. */
  players: PlayerSnapshot[]
  dora_markers: string[]
  our_seat: number | null
}

export type PlayerMahgenView = {
  seat: number
  hand: string
  melds: string[]
  river: string
}

export type MahgenView = {
  /** Length matches num_players. */
  players: PlayerMahgenView[]
  /** 3 (sanma) or 4 (yonma). */
  num_players: number
  dora_indicators: string
}

/** Mirrors `crate::schema::HoraScoreInfo`. Returned by `compute_bot_hora_score`. */
export type HoraScoreInfo = {
  points: number
  han: number
  fu: number
  yakuman: boolean
  /** mjai tile string of the winning tile. */
  win_tile: string
}

// ---------- Game History ----------
//
// Mirrors `crate::schema::history::*`. Strings carry RFC3339 timestamps;
// the frontend parses them with `new Date(...)` on demand.

export type Platform =
  | 'majsoul'
  | 'tenhou'
  | 'riichi_city'
  | 'mjai'
  | 'unknown'

export type KyokuMode = 'east_only' | 'east_south' | 'other'

/** Per-game stat counters from the recorded player's perspective. */
export type GameStats = {
  round: number
  oya: number

  fuuro: number
  fuuro_num: number
  fuuro_point: number
  fuuro_agari: number
  fuuro_agari_jun: number
  fuuro_agari_point: number
  fuuro_houjuu: number

  agari: number
  agari_as_oya: number
  agari_jun: number
  agari_point_oya: number
  agari_point_ko: number

  houjuu: number
  houjuu_jun: number
  houjuu_to_oya: number
  houjuu_point_to_oya: number
  houjuu_point_to_ko: number

  riichi: number
  riichi_as_oya: number
  riichi_jun: number
  riichi_agari: number
  riichi_agari_point: number
  riichi_agari_jun: number
  riichi_houjuu: number
  riichi_ryukyoku: number
  riichi_point: number
  chasing_riichi: number
  riichi_got_chased: number

  dama_agari: number
  dama_agari_jun: number
  dama_agari_point: number

  ryukyoku: number
  ryukyoku_point: number

  yakuman: number
  nagashi_mangan: number
}

/**
 * Platform-specific match identity captured at `start_game` — which room /
 * rank lobby the game was played in plus the platform's own game (paifu) id.
 * Mirrors `crate::schema::history::MatchInfo`: internally tagged on
 * `platform`, raw platform values, `None` fields omitted from the JSON.
 */
export type MatchInfo =
  | {
      platform: 'majsoul'
      /** Raw `game_uuid` — the replay identifier. */
      game_uuid?: string | null
      /** Ranked matchmode id (1..=28 = Bronze..Throne / Melee, 4p+3p). */
      mode_id?: number | null
      /** Friendly/AI room number. */
      room_id?: number | null
      /** Tournament id. */
      contest_uid?: number | null
      /** The recorded player's account id — whose game this is. Absent on
       *  records from before it was captured. */
      account_id?: number | null
      /** Raw rank level id the player held when the game started (4p rank
       *  on a 4p table, 3p rank on a 3p one), e.g. 10301 = 4p 雀傑1. Absent
       *  on records from before it was captured. */
      level_id?: number | null
      /** Raw `game_config.category`: 1 = friendly room, 2 = matchmaking
       *  (ranked), other values for the 比賽場. */
      category?: number | null
      /** Special-rule switches the table had on (e.g. `wanxiangxiuluo_mode`,
       *  `chuanma`) — an event mode. Omitted when none. */
      rules?: string[]
    }
  | {
      platform: 'tenhou'
      /** Paifu id from `<TAIKYOKU log=…>`. */
      log_id?: string | null
      /** Raw `<GO type=…>` rule/room bitfield (tier in bits 0x20/0x80). */
      go_type?: number | null
      /** Lobby number; 0 = the public ranked lobby. */
      lobby?: number | null
    }
  | {
      platform: 'riichi_city'
      /** Table-instance token from the `cmd_enter_room` wrapper. */
      room_id?: string | null
      /** Matchmaking classification id (wire string). */
      classify_id?: string | null
      /** Rank stage tier of the matchmaking room. */
      stage_type?: number | null
      /** Game mode id (e.g. 1001). */
      game_play?: number | null
    }

export type GameRecord = {
  id: string
  /** RFC3339 timestamp. */
  started_at: string
  /** RFC3339 timestamp. */
  ended_at: string
  platform: Platform
  num_players: 3 | 4
  kyoku_mode: KyokuMode
  names: string[]
  our_seat: number | null
  final_scores: number[]
  final_ranks: number[]
  our_rank: number | null
  /** `final_score - starting_score` (4p:25000, 3p:35000). */
  our_delta: number | null
  stats: GameStats
  /** Absent/null on records from before this field existed. */
  match_info?: MatchInfo | null
  /** The bot active for this game's mode when it was recorded (e.g.
   *  `mokora-3p`). Absent when bots were off, and on records from
   *  before this field existed. */
  bot?: string | null
  /** The rounds' (局) lengths over the game, deal to win or draw, ms.
   *  Absent on records from before they were measured. */
  round_times?: RoundTimes | null
  log_path: string
}

export type RoundTimes = {
  count: number
  total_ms: number
  min_ms: number
  max_ms: number
}

/** One side of `DealInStats`. */
export type DealInTally = {
  deal_ins: number
  /** Most dealt-in first. mjai tiles; red fives as their plain five. */
  tiles: { tile: string; count: number }[]
}

/** Mirrors `crate::schema::history::DealInStats`: which tiles were dealt in
 *  (every seat's ron, tsumo excluded) across the games asked about, split by
 *  who dealt in. */
export type DealInStats = {
  games: number
  /** The recorded player's own deal-ins. */
  mine: DealInTally
  /** Opponents' deal-ins, to anyone — the recorded player included. */
  others: DealInTally
}

/** Mirrors `crate::schema::ipc::LevelScore`. */
export type LevelScore = { id: number; score: number }

/** Mirrors `crate::schema::ipc::HighestHand`: the game's 精選和牌. */
export type HighestHand = {
  tiles: string[]
  win_tile: string
  melds: string[][]
  han: number
}

/** Mirrors `crate::schema::ipc::MajsoulRecentGame`: one game of the game's
 *  own recent-games list. */
export type MajsoulRecentGame = {
  uuid: string
  mode_id: number
  ranked: boolean
  /** Unix seconds. */
  start_time: number
  end_time: number
  rank: number
  final_point: number
}

/** Mirrors `crate::schema::ipc::Achievements`. */
export type Achievements = { gold: number; silver: number; bronze: number }

/** Mirrors `crate::schema::ipc::MajsoulRankStats`: Mahjong Soul's own
 *  statistics of one ladder's ranked games. */
export type MajsoulRankStats = {
  players: 3 | 4
  /** Games finished in each place, first first. */
  placements: number[]
  recent_rounds: number
  recent_ron: number
  recent_tsumo: number
  recent_deal_in: number
  highest_han: number | null
  highest_title: number | null
  average_win: number | null
  /** `[yaku id, count]`, most first. */
  yakuman: [number, number][]
  /** Every yaku won, `[yaku id, count]` by id. */
  yaku: [number, number][]
  nagashi: number
  highest_hand: HighestHand | null
}

/** Mirrors `crate::schema::ipc::AccountProfile` (`get_profile`). */
export type AccountProfile = {
  account_id: number | null
  nickname: string | null
  gold: number | null
  diamond: number | null
  level4: LevelScore | null
  level3: LevelScore | null
  revive_claimed: boolean | null
  stats: MajsoulRankStats[]
  stats_at_ms: number | null
  achievements: Achievements | null
  /** The game's own list of recent games, newest first. */
  recent_games: MajsoulRecentGame[]
  updated_ms: number
}

/** Mirrors `crate::schema::history::AgainstStats`: what opponents did to the
 *  recorded player across the games asked about. */
export type AgainstStats = {
  games: number
  /** Their discards an opponent called pon on. */
  pon: number
  /** Their discards an opponent called chi on (never in sanma). */
  chi: number
  /** Their discards an opponent called an open kan on. */
  kan: number
  /** Opponents' yakuman they paid for, by ron or tsumo. */
  yakuman: number
  /** Yakuman they won themselves (from the logs, so older games count
   *  sanma tsumo yakuman right too). */
  won_yakuman: number
}

export type HistoryFilter = {
  platform?: Platform
  num_players?: 3 | 4
  kyoku_mode?: KyokuMode
  /** RFC3339 timestamp; inclusive. */
  started_after?: string
  /** RFC3339 timestamp; exclusive. */
  started_before?: string
}

export type HistoryEvent =
  | { kind: 'recorded'; record: GameRecord }
  | { kind: 'deleted'; id: string }

// ---------- Logs ----------
//
// Mirrors `crate::schema::ipc::{LogEntry, LogSessionInfo, ReadLogRequest,
// ReadLogResponse}`. The same shape is used both for entries read off
// disk (`read_log_session`) and for live-tailed entries delivered over a
// `tauri::ipc::Channel` (`subscribe_log_events`) — initial-load and live
// arrivals merge into the same UI list without translation.

export type LogLevel = 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'

export type LogEntry = {
  ts_ms: number
  /** One of `LogLevel`, but kept open as `string` because backend may add levels. */
  level: string
  target: string
  file?: string
  line?: number
  message: string
  fields?: Record<string, unknown>
}

export type LogSessionInfo = {
  name: string
  path: string
  size_bytes: number
  mtime_ms: number
  is_active: boolean
}

export type ReadLogRequest = {
  session: string
  offset?: number
  limit?: number
  levels?: string[]
  /** Target prefixes; any-match (OR). */
  targets?: string[]
  /** Case-insensitive substring on `message`. */
  search?: string
}

export type ReadLogResponse = {
  entries: LogEntry[]
  has_more: boolean
  skipped_malformed: number
}

// ---------- Inspector ----------
//
// Mirrors `crate::schema::inspector::*` and `crate::schema::ipc::ReadInspector*`.
// Tagged on `kind` — switch on the discriminant to render kind-specific
// detail panels. Same shape arrives via `subscribe_inspector` (live tail)
// and `read_inspector` (past sessions), so renderers don't fork.

export type FrameDirection = 'up' | 'down'

export type FrameRaw =
  | { format: 'text'; data: string }
  | { format: 'binary'; data: string } // base64

export type ParsedFrame = {
  method: string
  args: unknown
}

export type BotReactionPayload = {
  bot: string
  actor_id: number
  trigger: MjaiEvent
  action: MjaiEvent
  meta?: Record<string, unknown>
  reaction_ms: number
}

/** Which capture backend observed an event. */
export type CaptureSource = 'mitm' | 'chromium'

export type HttpPhase = 'request' | 'response'

export type HttpHeader = {
  name: string
  value: string
}

/** A body we kept, or the reason we did not. */
export type HttpBody = {
  text?: string
  bytes?: number
  /** Absent when the body was captured whole. */
  skipped?: string
}

/**
 * A recognizer's reading of an exchange. Vendor-specific vocabulary lives
 * in `data` — never in the exchange itself — so a new recognizer needs no
 * change here.
 */
export type HttpAnnotation = {
  kind: string
  summary: string
  data: unknown
}

export type InspectorEntry =
  | {
      kind: 'ws_frame'
      ts_ms: number
      direction: FrameDirection
      flow_id: string
      size: number
      raw: FrameRaw
      parsed?: ParsedFrame
      emitted: number
    }
  | {
      kind: 'mjai_event'
      ts_ms: number
      event: MjaiEvent
    }
  | {
      kind: 'bot_reaction'
      ts_ms: number
      // Backend serializes BotReaction with #[serde(flatten)], so its
      // fields land at the top level of the row alongside `kind` and
      // `ts_ms`.
      bot: string
      actor_id: number
      trigger: MjaiEvent
      action: MjaiEvent
      meta?: Record<string, unknown>
      reaction_ms: number
    }
  | {
      kind: 'http'
      ts_ms: number
      source: CaptureSource
      // Backend serializes HttpExchange with #[serde(flatten)], so its
      // fields land at the top level of the row, same as bot_reaction.
      exchange_id?: string
      phase: HttpPhase
      method: string
      url: string
      host: string
      version: string
      status?: number
      headers: HttpHeader[]
      body?: HttpBody
      annotations?: HttpAnnotation[]
    }

export type InspectorKind = InspectorEntry['kind']

export type ReadInspectorRequest = {
  session: string
  offset?: number
  limit?: number
  kinds?: InspectorKind[]
  actor?: number
  search?: string
}

export type ReadInspectorResponse = {
  entries: InspectorEntry[]
  has_more: boolean
  skipped_malformed: number
}

// ---------- Game review ----------
// Mirror `crate::schema::review`: a replay of a recorded game through a
// built-in model, the model's ranking next to what was played.

export type ReviewKind =
  | 'discard'
  | 'riichi'
  | 'pon'
  | 'chi'
  | 'daiminkan'
  | 'ankan'
  | 'kakan'
  | 'kita'
  | 'hora'
  | 'ryukyoku'
  | 'pass'

export type ReviewAction = {
  kind: ReviewKind
  pai?: string
  consumed?: string[]
}

export type ReviewOption = { action: ReviewAction; prob: number }

export type ReviewDecision = {
  idx: number
  bakaze: string
  kyoku: number
  honba: number
  turn: number
  /** A call window (somebody else's discard), not the player's own turn. */
  window: boolean
  actual: ReviewAction
  /** The model's probability for what was played; absent when not ranked. */
  actual_prob?: number
  top: ReviewOption[]
  /** Only one action was legal. */
  forced: boolean
  /** What was played is the model's first choice. */
  matched: boolean
  missed_win: boolean
  /** Fingerprint of what the model was shown. */
  obs: string
}

export type ReviewKindStats = {
  kind: ReviewKind
  scored: number
  matched: number
  prob_sum: number
  forced: number
}

export type ReviewModelInfo = {
  name: string
  num_players: number
  weights_hash: string
}

export type ReviewReport = {
  key: string
  game_id: string
  game_hash: string
  created_ms: number
  model: ReviewModelInfo
  /** The bot that played the game live, when recorded. */
  live_bot?: string
  our_seat: number
  num_players: number
  settings: { version: number; top_k: number; game_length: string; deterministic: boolean }
  decisions: ReviewDecision[]
  kinds: ReviewKindStats[]
  diagnostics: { decision_points: number; no_actual: number; not_ranked: number }
}

export type ReviewSummary = {
  key: string
  model: ReviewModelInfo
  created_ms: number
  decisions: number
  scored: number
  matched: number
}

export type ReviewRow = {
  id: string
  model: string
  reviewed: boolean
  stale: boolean
  summary?: ReviewSummary
}

export type ReviewProgress = {
  state: 'idle' | 'running' | 'paused' | 'cancelled' | 'done' | 'error'
  total: number
  done: number
  current?: string
  message?: string
}

export type LatencyStats = {
  n: number
  mean_ms: number
  p50_ms: number
  p95_ms: number
  max_ms: number
}

export type SelfCheckReport = {
  at_ms: number
  games: number
  decisions: number
  logs_clean: boolean
  logs_changed: number
  leak_games: number
  leak_decisions: number
  leak_mismatches: number
  leak_uncensored_mismatches: number
  deterministic: boolean
  determinism_games: number
  call_windows: number
  passes: number
  calls: number
  calls_in_logs: number
  unresolved: number
  not_ranked: number
  concurrent_events: number
  latency_alone: LatencyStats
  latency_with_review: LatencyStats
  notes: string[]
}
