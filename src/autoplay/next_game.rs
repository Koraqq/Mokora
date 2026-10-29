//! Start the next ranked game by itself (Mahjong Soul).
//!
//! When a game ends the client shows a run of screens — final standings,
//! mission progress, a rank change — each dismissed with the same button at
//! the bottom right, and then drops back to the lobby, where the player opens
//! the ranked lobby, a room and a table. This module does that for the player.
//!
//! Where to press, and what part of the screen must show what, is full
//! auto's map of the lobby: a paid feature, from the module Mokora downloads
//! once a key is redeemed (`pro_api::lobby::Layout`, [`crate::pro::
//! lobby_layout`]). Without it a run does not start.
//!
//! The screen is a canvas, so none of it can be *read*. What can be read is
//! the client's own uplink, and the whole design is built on it (see
//! [`crate::autoplay::lobby`]): every stage is something the client proves by
//! sending a specific request, and a stage that cannot be proven ends the run
//! — never clicked through on hope.
//!
//! ```text
//! EndGame ─► (rank changed? stop) ─► wait for the animation ─► press confirm
//!            for as long as the button is seen (a screenshot says so)
//!   ─► log out and back in `relogins_after_game` times (settings ─►
//!       登出帳號 ─► 確認 ─► the fastest route ─► 進入遊戲; a page reload if
//!       the lobby is not back within a minute)
//!   ─► rest in the lobby (`rest_minutes`, if set — counted from the lobby
//!       coming back, the log-ins included, a page reload not)
//!   ─► 段位場 ─► scroll ─► room ─► [fetchCurrentMatchInfo: the room's tables]
//!   ─► scroll ─► table ─► [startUnifiedMatch: queued]
//! ```
//!
//! The confirm button's position is the lobby's shop and gacha, so it is
//! pressed only while a screenshot of its spot shows the button's yellow. (The
//! client's own traffic cannot say which screen it is on: its account refresh
//! lands ~24s after every game, pressed through or not.)
//!
//! Stopping is the default for anything unexpected: a rank change (the rooms
//! on offer may have changed under it), a press the client does not answer
//! (a room or table locked at the player's rank puts up a dialog and sends
//! nothing), a person's click anywhere on the game, a screen that never gives way.
//! A person is told, and the screens are left exactly as they are.
//!
//! The Settings page's "start auto play" button runs the room-and-table half
//! of the sequence on demand, from the lobby, without waiting for a game to
//! end; every game after that follows by itself. It is also how the
//! coordinates and the scroll method are checked. Its "log in again" button
//! logs out and back in the same way, on its own.

use crate::autoplay::cdp_input::{
    certificate_signature, dispatch_click_guarded, dispatch_drag, dispatch_wheel,
    evaluate_canvas_rect, install_input_probe, label_tan_fraction, lobby_signature,
    read_input_probe, result_button_visible, screenshot_box, skip_cyan_fraction, InputCounts,
};
use crate::autoplay::context::AutoplayContext;
use crate::autoplay::lobby::{mode_of_sid, MatchOutcome, RankChange};
use pro_api::lobby::Layout;
use crate::config::{
    AppConfig, ListScroll, MajsoulAutoplayConfig, NextGameConfig, NextGameRule, Platform, RankRoom,
    RoomMode,
};
use crate::event_bus::{MjaiBus, NotifyBus};
use crate::schema::history::MatchInfo;
use crate::schema::{GameEndReason, MjaiEvent, NextGameStatus, NextGameTarget, Notification};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast::error::RecvError, RwLock};
use tracing::{debug, info, warn};

/// How often a stage looks for the client's proof while waiting for it.
const POLL: Duration = Duration::from_millis(100);

/// A stage waiting on the client looks for a person's click every this many
/// polls. It costs a round trip to the page, so not on every one.
const HUMAN_CHECK_EVERY: u32 = 3;

/// How long the reward notify (which carries the rank change) is waited for
/// after the result. It follows the result by a fraction of a second.
const RANK_WAIT_MS: u32 = 4000;

/// How long the result screens may take to clear before the run gives up.
const DISMISS_TIMEOUT: Duration = Duration::from_secs(90);

/// Looks in a row, a second apart, that must find the confirm button gone
/// before the result screens count as dismissed: one look can fall in the
/// gap between two screens.
const ABSENT_CHECKS: u32 = 3;

/// Taps allowed on the room chest's reward screen before giving up on it.
/// One closes it; the rest cover a tap that landed during its opening
/// animation.
const MAX_CHEST_TAPS: u32 = 3;

/// How long the profile window may take to answer the badge press with the
/// statistics, and its pop-in before the ✕.
const PROFILE_WAIT_MS: u64 = 5000;
/// How long a ladder switch in it may take to bring that ladder's games.
const LADDER_WAIT_MS: u64 = 3000;
const PROFILE_SETTLE_MS: (u32, u32) = (1000, 1500);

/// The activities page sliding in after 活動, and a tab's content after its
/// press.
const ACTIVITY_OPEN_MS: (u32, u32) = (1800, 2500);
const TAB_SETTLE_MS: (u32, u32) = (1000, 1500);
/// How long the claim's answer may take after 領取.
const CLAIM_WAIT_MS: u64 = 4000;
/// The 獲得獎勵 banner playing in, before the tap that closes it.
const REWARD_SETTLE_MS: (u32, u32) = (1500, 2200);

/// Presses allowed on the bond story's 跳過. One ends it; the rest cover a
/// press that landed while the story was still fading in.
const MAX_SKIP_PRESSES: u32 = 3;

/// Presses allowed on the promotion certificate's 關閉 X. One closes it; the
/// second covers a press that landed while it was still unrolling.
const MAX_CERT_CLOSES: u32 = 2;

/// The settle after the client reports it is in the lobby, before the first
/// press there: its buttons are not live the instant the screen changes.
const LOBBY_SETTLE_MS: (u32, u32) = (1500, 2500);

/// The wait after pressing the ranked lobby, for the list to slide in.
const LIST_OPEN_MS: (u32, u32) = (1200, 1800);

/// The wait after a list reaches its edge, for the scroll's momentum to die.
const SCROLL_SETTLE_MS: u64 = 900;

/// While resting between games, how often the settings are read again, so
/// switching the feature off ends a long rest promptly.
const REST_CHECK: Duration = Duration::from_secs(5);
/// How long a reloaded game page may take to show its lobby again: the
/// client's assets, the gateway, the login.
const RELOAD_WAIT: Duration = Duration::from_secs(90);
/// The old page keeps answering for a moment after the reload is sent.
const RELOAD_SETTLE: Duration = Duration::from_secs(3);
/// How long 進入遊戲 may take to bring the lobby back before the page is
/// reloaded instead.
const RELOGIN_WAIT: Duration = Duration::from_secs(60);
/// A dialog's pop-in after the press that opens it.
const DIALOG_SETTLE: Duration = Duration::from_millis(1200);
/// How long a dialog's button may take to show.
const DIALOG_WAIT: Duration = Duration::from_secs(4);
/// How long the login screen may take after the log-out is confirmed.
const LOGIN_SCREEN_WAIT: Duration = Duration::from_secs(30);
/// Between screenshots while waiting for a screen.
const SCREEN_POLL: Duration = Duration::from_millis(400);
/// "<返回" presses at most to get from a ranked list to the lobby (a
/// room's tables → the rooms → the lobby, with one to spare).
const MAX_LIST_BACKS: u32 = 3;

/// Between presses of the result screen's confirm button.
const CONFIRM_GAP_MS: (u32, u32) = (2000, 3500);

/// One wheel event's travel, CSS px. Far larger than any of these lists, and
/// sent several times: the list clamps at its edge, so only "more than
/// enough" matters.
const WHEEL_DELTA: f64 = 600.0;
const WHEEL_TICKS: u32 = 6;
const WHEEL_GAP_MS: u32 = 60;

/// How long a queue may go without the server finding a game before the run
/// stops waiting on it. A 3-player South table can be slow to fill.
const MATCH_WAIT: Duration = Duration::from_secs(180);

/// Which end of a list a stage reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
}

/// Why a run ended without queuing a game.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    /// Switched off while the run was under way.
    Disabled,
    /// A game is on a table — pressing anything here would play a tile.
    InGame,
    NotMajsoul,
    NoPage,
    LimitReached(u32),
    /// The account has played its daily limit of games today.
    DailyLimitReached(u32),
    RankChanged(RankChange),
    NeverReachedLobby,
    /// The lobby's main screen is not showing, so nothing there can be pressed.
    NotInLobby,
    /// Logged out and back in, then reloaded, and still no lobby.
    ReloadNoLobby,
    /// A person pressed a mouse button in the lobby, between its arrival and
    /// the table being pressed.
    HumanInput,
    /// The room press drew no answer: the client refused it (a room above the
    /// player's rank puts up a dialog and sends nothing) or the press was lost.
    RoomNotOpened,
    RoomMismatch {
        want: u32,
        got: Vec<u32>,
    },
    NoTableToRepeat,
    /// Choosing a room by rank has no level for that ladder to go on.
    RankUnknown,
    TableNotListed {
        index: usize,
        listed: usize,
    },
    /// The table press drew no answer: same two causes as `RoomNotOpened`
    /// (a table the rank does not allow reads "段位不符" and refuses).
    TableNotQueued,
    /// The queue was cancelled from the client — by the player, since nothing
    /// here sends a cancel.
    MatchCancelled,
    /// The server timed the queue out without finding a game.
    MatchTimedOut,
    WrongTable {
        want: u32,
        got: String,
    },
    Input(String),
    /// `gold` copper coins are short of the `need` of `room`'s table, and
    /// today's revive coins are claimed or did not cover it.
    NoCoins {
        room: RankRoom,
        gold: u64,
        need: u64,
    },
    /// The game refused a room or table with its "銅幣不足" dialog (pressed
    /// away already): coins to get before trying again. Handled inside the
    /// run (see `queue`), never reported as such.
    CoinsRefused {
        room: RankRoom,
        need: u64,
    },
}

impl Stop {
    /// Reasons that are just the run being moot, not something to tell the
    /// player about.
    fn quiet(&self) -> bool {
        matches!(self, Stop::Disabled | Stop::InGame | Stop::NotMajsoul)
    }

    fn message(&self) -> String {
        match self {
            Stop::Disabled => "Switched off.".into(),
            Stop::InGame => "A game is in progress.".into(),
            Stop::NotMajsoul => "Only Mahjong Soul is supported.".into(),
            Stop::NoPage => {
                "The game page is not open. Start capture in Chromium mode and open the game."
                    .into()
            }
            Stop::LimitReached(n) => format!("Reached the limit of {n} games."),
            Stop::DailyLimitReached(n) => format!(
                "Today's limit of {n} games is reached: full auto starts no more games today. You can still start one yourself."
            ),
            Stop::RankChanged(c) => format!(
                "Your rank changed (level {} to {}). Check the rank screens, then start the next game yourself.",
                c.origin, c.last
            ),
            Stop::NeverReachedLobby => {
                "The client never got back to the lobby. Something is blocking the result screens."
                    .into()
            }
            Stop::NotInLobby => {
                "The lobby's main screen is not showing. Close any dialog or sub-screen, then start again from the lobby."
                    .into()
            }
            Stop::ReloadNoLobby => {
                "Logging out and back in did not bring the lobby back, nor did reloading the game — a notice over it, or a login asking for the account. Deal with it, then start again from the lobby."
                    .into()
            }
            Stop::HumanInput => {
                "A mouse click was detected, so Mokora stepped aside and left the screen to you."
                    .into()
            }
            Stop::RoomNotOpened => {
                "The room did not respond. Your rank probably cannot enter it (the game shows a dialog and sends nothing), or the press was lost."
                    .into()
            }
            Stop::RoomMismatch { want, got } => format!(
                "The room opened does not list the table of your last game (mode {want}; room offers {got:?}). Play one game in that room by hand first, or pick the room you have been playing."
            ),
            Stop::NoTableToRepeat => {
                "No ranked game has been played yet, so there is no table to repeat. Pick a table in Settings, or play one game by hand."
                    .into()
            }
            Stop::RankUnknown => {
                "Your rank on that ladder is not known yet, so the room cannot be chosen from it. It arrives when the game page logs in; reload the game page once, or pick a room by hand in Settings."
                    .into()
            }
            Stop::TableNotListed { index, listed } => format!(
                "The room lists {listed} tables, so table {} is not there.",
                index + 1
            ),
            Stop::TableNotQueued => {
                "The table did not respond. Your rank probably cannot enter it (shown as 段位不符), or the press was lost."
                    .into()
            }
            Stop::MatchCancelled => {
                "The queue was cancelled, so no game was started. Mokora will carry on after the next game you play."
                    .into()
            }
            Stop::MatchTimedOut => {
                "The server found no game for the queue and timed it out. Queue again from the lobby, or press Start auto play."
                    .into()
            }
            Stop::WrongTable { want, got } => format!(
                "Queued for {got:?} but wanted mode {want}. The table list may have scrolled differently than expected."
            ),
            Stop::Input(e) => format!("Could not press the screen: {e}"),
            Stop::NoCoins { room, gold, need } => format!(
                "Not enough copper coins for {room:?}: {gold} held, {need} needed, and today's revive coins are already claimed. Pick another room in Settings."
            ),
            Stop::CoinsRefused { room, need } => format!(
                "The game refused {room:?} for want of copper coins ({need} needed)."
            ),
        }
    }
}

/// Where in the room list the room to queue in sits.
fn room_target(lay: &Layout, room: RankRoom) -> (Edge, (f64, f64)) {
    match room {
        RankRoom::Bronze => (Edge::Top, lay.room_top[0]),
        RankRoom::Silver => (Edge::Top, lay.room_top[1]),
        RankRoom::Gold => (Edge::Top, lay.room_top[2]),
        RankRoom::Jade => (Edge::Bottom, lay.room_bottom[1]),
        // Resolved to a real room before anything is pressed (`manual_room`);
        // the highest one is the safe reading should it ever get here.
        RankRoom::Throne | RankRoom::Highest => (Edge::Bottom, lay.room_bottom[2]),
    }
}

/// Where the `index`th table (0..=3, in the order the client draws them)
/// sits: the first three at the top of the list, the fourth below the fold.
fn table_target(lay: &Layout, index: usize) -> Option<(Edge, (f64, f64))> {
    match index {
        0..=2 => Some((Edge::Top, lay.table_top[index])),
        3 => Some((Edge::Bottom, lay.table_bottom_last)),
        _ => None,
    }
}

/// The highest room a level may enter. Level ids read `<ladder><tier><n>`
/// (`10103`, `20201`): tier 1 is 初心, 2 雀士, 3 雀傑, 4 雀豪, 5 雀聖, 6 魂天,
/// and each room takes two tiers — 銅 初心-雀士, 銀 雀士-雀傑, 金 雀傑-雀豪,
/// 玉 雀豪-雀聖, 王座 雀聖 and up — so the highest is the room whose upper
/// tier the level is on.
fn room_for_level(id: u32) -> RankRoom {
    match (id / 100) % 100 {
        0 | 1 => RankRoom::Bronze,
        2 => RankRoom::Silver,
        3 => RankRoom::Gold,
        4 => RankRoom::Jade,
        _ => RankRoom::Throne,
    }
}

/// Which ladder to queue on: the room's table list draws the two 4-player
/// tables first and the two 3-player ones after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ladder {
    Four,
    Three,
}

impl Ladder {
    /// The two rows of the table list that belong to this ladder: East, South.
    fn rows(self) -> [usize; 2] {
        match self {
            Ladder::Four => [0, 1],
            Ladder::Three => [2, 3],
        }
    }
}

/// What is left of a `rest` that started `elapsed` ago, `paused` of which
/// (a page reload) does not count.
fn rest_left(rest: Duration, elapsed: Duration, paused: Duration) -> Duration {
    rest.saturating_sub(elapsed.saturating_sub(paused))
}

/// The ladder a room mode picks its room from; `None` for `Manual`.
fn ladder_of(mode: RoomMode) -> Option<Ladder> {
    match mode {
        RoomMode::HighestFour => Some(Ladder::Four),
        RoomMode::HighestThree => Some(Ladder::Three),
        RoomMode::Manual => None,
    }
}

/// The ladder of a ranked game, from the level id it was played at
/// (`10301` is on the 4-player ladder, `20202` on the 3-player one).
fn ladder_of_level(level: u32) -> Option<Ladder> {
    match level / 10000 {
        1 => Some(Ladder::Four),
        2 => Some(Ladder::Three),
        _ => None,
    }
}

/// The ladder a set table rule queues on; `Same` takes the last game's.
fn rule_ladder(rule: NextGameRule, last: Option<Ladder>) -> Option<Ladder> {
    match rule {
        NextGameRule::FourEast | NextGameRule::FourSouth => Some(Ladder::Four),
        NextGameRule::ThreeEast | NextGameRule::ThreeSouth => Some(Ladder::Three),
        NextGameRule::Same => last,
    }
}

/// The table rule to press for a room that may change with the rank: `Same`
/// is the last game's ladder and East or South, since the last table's id
/// belongs to the room it was played in.
fn rule_for_changing_room(
    rule: NextGameRule,
    last_mode: Option<u32>,
    last: Option<Ladder>,
) -> Result<NextGameRule, Stop> {
    if rule != NextGameRule::Same {
        return Ok(rule);
    }
    let (mode, ladder) = last_mode.zip(last).ok_or(Stop::NoTableToRepeat)?;
    Ok(match (ladder, mode_is_south(mode)) {
        (Ladder::Four, false) => NextGameRule::FourEast,
        (Ladder::Four, true) => NextGameRule::FourSouth,
        (Ladder::Three, false) => NextGameRule::ThreeEast,
        (Ladder::Three, true) => NextGameRule::ThreeSouth,
    })
}

/// The room of a hand-set queue: the configured one, or for `Highest` the
/// highest the rank of the rule's ladder allows.
fn manual_room(
    ng: &NextGameConfig,
    levels: (Option<u32>, Option<u32>),
    last: Option<Ladder>,
) -> Result<RankRoom, Stop> {
    if ng.room != RankRoom::Highest {
        return Ok(ng.room);
    }
    let ladder = rule_ladder(ng.rule, last).ok_or(Stop::NoTableToRepeat)?;
    highest_room(ladder, levels)
}

/// What the next game will be, as far as it is known before the lobby is
/// opened: for the rank-picked modes the ladder and its highest room (once
/// the rank is known; the table follows the player counts), for `Manual`
/// the configured room and table. Shown while resting and queuing.
fn target_of(
    ng: &NextGameConfig,
    levels: (Option<u32>, Option<u32>),
    last: Option<Ladder>,
) -> NextGameTarget {
    match ladder_of(ng.room_mode) {
        Some(ladder) => NextGameTarget {
            room: highest_room(ladder, levels).ok(),
            players: Some(match ladder {
                Ladder::Four => 4,
                Ladder::Three => 3,
            }),
            rule: None,
        },
        None => NextGameTarget {
            room: manual_room(ng, levels, last).ok(),
            players: None,
            rule: Some(ng.rule),
        },
    }
}

/// The highest room the given ladder's level allows.
fn highest_room(ladder: Ladder, levels: (Option<u32>, Option<u32>)) -> Result<RankRoom, Stop> {
    let level = match ladder {
        Ladder::Four => levels.0,
        Ladder::Three => levels.1,
    };
    level.map(room_for_level).ok_or(Stop::RankUnknown)
}

/// East or South: whichever of the ladder's two tables has more players on
/// it right now, so the queue fills and the games keep coming. Ties, and a
/// room that reported no counts, take East.
fn pick_row(ladder: Ladder, listed: &[u32], counts: &[(u32, u32)]) -> Option<usize> {
    let players = |row: usize| {
        let mode = listed.get(row)?;
        Some(
            counts
                .iter()
                .find(|(id, _)| id == mode)
                .map_or(0, |(_, n)| *n),
        )
    };
    let [east, south] = ladder.rows();
    let east_n = players(east)?;
    match players(south) {
        Some(south_n) if south_n > east_n => Some(south),
        _ => Some(east),
    }
}

/// The copper coins a ranked table asks the player to hold to enter it (the
/// fee taken is a fifth of it). From the game's own table: Bronze is free;
/// East / South take 2500 / 3500 in Silver, 5000 / 7000 in Gold and
/// 10000 / 14000 in Jade and Throne — the same for either ladder.
fn coin_floor(room: RankRoom, south: bool) -> u64 {
    let (east, south_floor) = match room {
        RankRoom::Bronze => (0, 0),
        RankRoom::Silver => (2500, 3500),
        RankRoom::Gold => (5000, 7000),
        RankRoom::Jade | RankRoom::Throne | RankRoom::Highest => (10000, 14000),
    };
    if south {
        south_floor
    } else {
        east
    }
}

/// A row of a room's table list is a South table (the list reads 4p East,
/// 4p South, 3p East, 3p South).
fn row_is_south(row: usize) -> bool {
    row % 2 == 1
}

/// Whether a ranked table's mode id is a South (hanchan) one.
fn mode_is_south(mode: u32) -> bool {
    matches!(mode, 3 | 6 | 9 | 12 | 16 | 18 | 20 | 22 | 24 | 26)
}

/// The coins needed before a room is even opened: for a room picked by rank
/// its cheaper (East) table, for a set rule that rule's table.
fn coins_to_open(
    room: RankRoom,
    ladder: Option<Ladder>,
    rule: NextGameRule,
    last_mode: Option<u32>,
) -> u64 {
    let south = match (ladder, rule) {
        (Some(_), _) => false,
        (None, NextGameRule::Same) => last_mode.is_some_and(mode_is_south),
        (None, NextGameRule::FourSouth | NextGameRule::ThreeSouth) => true,
        (None, _) => false,
    };
    coin_floor(room, south)
}

/// Which row of the room's table list to press, for a fixed rule.
///
/// `Same` finds the last game's table in the list the room itself reported —
/// no table-id lookup is kept anywhere, the client's own order is the
/// authority. A fixed rule is the row's position in the same order.
fn table_index(rule: NextGameRule, last_mode: Option<u32>, listed: &[u32]) -> Result<usize, Stop> {
    let index = match rule {
        NextGameRule::Same => {
            let mode = last_mode.ok_or(Stop::NoTableToRepeat)?;
            listed
                .iter()
                .position(|m| *m == mode)
                .ok_or_else(|| Stop::RoomMismatch {
                    want: mode,
                    got: listed.to_vec(),
                })?
        }
        NextGameRule::FourEast => 0,
        NextGameRule::FourSouth => 1,
        NextGameRule::ThreeEast => 2,
        NextGameRule::ThreeSouth => 3,
    };
    if index >= listed.len() {
        return Err(Stop::TableNotListed {
            index,
            listed: listed.len(),
        });
    }
    Ok(index)
}

/// Uniform in `[min, max]`; tolerant of the two being given the wrong way
/// round.
fn between(min: u32, max: u32) -> u64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    u64::from(lo) + rand::random::<u64>() % (u64::from(hi - lo) + 1)
}

async fn nap(range: (u32, u32)) {
    tokio::time::sleep(Duration::from_millis(between(range.0, range.1))).await;
}

struct NextGame {
    cfg: Arc<RwLock<AppConfig>>,
    ctx: Arc<AutoplayContext>,
    notify: NotifyBus,
    /// Mode id of the last ranked game started, if it was a ranked one.
    last_mode: Option<u32>,
    /// The ladder that game was on (from the level it was played at).
    last_ladder: Option<Ladder>,
    /// Games queued by this run of the app.
    queued: u32,
    /// Where a picture of the screen is left when a run stops (next to the
    /// config file), so a stop can be diagnosed from what was actually showing.
    config_dir: PathBuf,
    /// The page's mouse-press count as of the last thing *this* flow did,
    /// while a run is under way. Any difference from the page's count now is
    /// a person. `None` outside a run.
    baseline: Mutex<Option<InputCounts>>,
}

impl NextGame {
    /// The settings in force right now, or why there is nothing to do.
    async fn settings(&self) -> Result<(NextGameConfig, MajsoulAutoplayConfig), Stop> {
        let cfg = self.cfg.read().await;
        if cfg.platform.kind != Platform::Majsoul {
            return Err(Stop::NotMajsoul);
        }
        // Full auto is a paid feature: without it the switch is as good as off.
        if !cfg.autoplay.enabled
            || !cfg.autoplay.next_game.enabled
            || !crate::pro::available()
        {
            return Err(Stop::Disabled);
        }
        Ok((cfg.autoplay.next_game.clone(), cfg.autoplay.majsoul.clone()))
    }

    /// The lobby's map, from the paid module; without it the run is as good
    /// as switched off.
    fn lay(&self) -> Result<Arc<Layout>, Stop> {
        crate::pro::lobby_layout().ok_or(Stop::Disabled)
    }

    async fn page(&self) -> Result<crate::autoplay::page::GamePage, Stop> {
        self.ctx.page.read().await.clone().ok_or(Stop::NoPage)
    }

    fn baseline(&self) -> Option<InputCounts> {
        *self.baseline.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_baseline(&self, counts: Option<InputCounts>) {
        *self.baseline.lock().unwrap_or_else(|e| e.into_inner()) = counts;
    }

    /// Start watching for a person's mouse presses from now.
    async fn arm(&self) -> Result<(), Stop> {
        let page = self.page().await?;
        install_input_probe(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let counts = read_input_probe(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        self.set_baseline(Some(counts));
        Ok(())
    }

    /// Has anyone but this flow pressed a mouse button since the last thing
    /// it did? A no-op while not armed.
    async fn check_human(&self) -> Result<(), Stop> {
        let Some(base) = self.baseline() else {
            return Ok(());
        };
        let page = self.page().await?;
        let now = read_input_probe(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        if now != base {
            debug!("next-game: mouse presses moved {base:?} -> {now:?}");
            return Err(Stop::HumanInput);
        }
        Ok(())
    }

    /// Take this flow's own input into the baseline, so it is not mistaken
    /// for a person's.
    async fn rebase(&self) {
        if self.baseline().is_none() {
            return;
        }
        if let Ok(page) = self.page().await {
            if let Ok(counts) = read_input_probe(&page).await {
                self.set_baseline(Some(counts));
            }
        }
    }

    /// Poll `probe` until it yields a value or `timeout_ms` passes, looking
    /// for a person's click as it waits.
    async fn wait_proof<T>(
        &self,
        timeout_ms: u64,
        mut probe: impl FnMut() -> Option<T>,
    ) -> Result<Option<T>, Stop> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut polls = 0u32;
        loop {
            if let Some(v) = probe() {
                return Ok(Some(v));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(POLL).await;
            polls += 1;
            if polls.is_multiple_of(HUMAN_CHECK_EVERY) {
                self.check_human().await?;
            }
        }
    }

    /// Press a 16:9-normalised point. The canvas is measured again every time
    /// — a window resized since the last press moves every target — and
    /// `guard` is asked again after the hover, right before the press.
    /// Returns whether the press went out.
    async fn press(&self, at: (f64, f64), guard: impl Fn() -> bool) -> Result<bool, Stop> {
        let (_, ap) = self.settings().await?;
        self.press_with(&ap, at, guard).await
    }

    /// [`Self::press`] with the click settings in hand — for the log-in-again
    /// button, which works with the next-game flow switched off.
    async fn press_with(
        &self,
        ap: &MajsoulAutoplayConfig,
        at: (f64, f64),
        guard: impl Fn() -> bool,
    ) -> Result<bool, Stop> {
        self.check_human().await?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let (px, py) = rect.pixel(at.0, at.1);
        if !rect.contains(px, py) {
            return Err(Stop::Input(format!(
                "({px:.0},{py:.0}) is outside the canvas {rect:?}"
            )));
        }
        // The engine drops a press that has not been hovered first; the
        // configured value is never allowed under that floor.
        let hover = ap.hover_delay_ms.max(100);
        let sent = dispatch_click_guarded(&page, px, py, hover, ap.click_hold_ms, guard)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        self.rebase().await;
        info!("next-game: pressed ({:.1}, {:.1}) sent={sent}", at.0, at.1);
        Ok(sent)
    }

    /// Move the room list or table list to one of its edges.
    async fn scroll(&self, how: ListScroll, edge: Edge) -> Result<(), Stop> {
        let lay = self.lay()?;
        self.check_human().await?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let (x, y) = rect.pixel(lay.list_anchor.0, lay.list_anchor.1);
        let result = match how {
            ListScroll::Wheel => {
                let delta = match edge {
                    Edge::Top => -WHEEL_DELTA,
                    Edge::Bottom => WHEEL_DELTA,
                };
                dispatch_wheel(&page, x, y, delta, WHEEL_TICKS, WHEEL_GAP_MS).await
            }
            ListScroll::Drag => {
                // Dragging the content up shows the bottom of the list.
                let (_, y_low) = rect.pixel(lay.list_anchor.0, 6.9);
                let (_, y_high) = rect.pixel(lay.list_anchor.0, 3.7);
                let (from, to) = match edge {
                    Edge::Bottom => (y_low, y_high),
                    Edge::Top => (y_high, y_low),
                };
                let mut out = Ok(());
                for _ in 0..3 {
                    out = dispatch_drag(&page, x, from, to, 12, 25).await;
                    if out.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(350)).await;
                }
                out
            }
        };
        result.map_err(|e| Stop::Input(format!("{e:#}")))?;
        self.rebase().await;
        tokio::time::sleep(Duration::from_millis(SCROLL_SETTLE_MS)).await;
        Ok(())
    }

    /// A game just ended: dismiss the result screens, then queue the next.
    /// Nobody is watched for until the lobby is up (see `queue_stages`), so
    /// clicking through the result screens yourself is fine.
    async fn after_game(&mut self) -> Result<String, Stop> {
        let result = self.after_game_stages().await;
        self.set_baseline(None);
        result
    }

    async fn after_game_stages(&mut self) -> Result<String, Stop> {
        let (ng, _) = self.settings().await?;
        let lobby = self.ctx.lobby.clone();

        // The rank change arrives just behind the result. A promotion or a
        // demotion ends the run before a single press: the rank screens are
        // the ones nobody should click through blind.
        if let Some(change) = self
            .wait_proof(u64::from(RANK_WAIT_MS), || lobby.rank())
            .await?
        {
            if ng.stop_on_rank_change && change.changed() {
                return Err(Stop::RankChanged(change));
            }
        }
        if ng.max_games > 0 && self.queued >= ng.max_games {
            return Err(Stop::LimitReached(ng.max_games));
        }

        // Let the last hand's settlement play out. Cut short only by a game
        // having started.
        let delay = between(ng.advance_delay_min_ms, ng.advance_delay_max_ms);
        if self
            .wait_proof(delay, || lobby.in_game().then_some(()))
            .await?
            .is_some()
        {
            return Err(Stop::InGame);
        }

        self.dismiss_result_screens(&ng).await?;
        info!("next-game: result screens dismissed, back in the lobby");
        // The rest counts from here, the lobby being back. Logging in again
        // is part of it; a page reload is not — the clock stops until the
        // lobby is back.
        let rest_from = Instant::now();
        let mut paused = Duration::ZERO;
        // A fresh session every game, if so set.
        let relogins = ng.relogins_after_game.min(crate::config::MAX_RELOGINS);
        for n in 1..=relogins {
            let (_, ap) = self.settings().await?;
            paused += self.relogin(&ap, ng.fastest_route).await?;
            info!("next-game: logged in again ({n}/{relogins})");
        }
        // From here — the lobby is up — a click of the player's own ends the
        // run, until the table is pressed.
        self.arm().await?;
        // The game's statistics of the player, for the 個人資料 page: its
        // profile window, opened and closed again from the lobby.
        if ng.refresh_profile {
            let (_, ap) = self.settings().await?;
            match self.refresh_profile_stats(&ap).await {
                Ok(_) => {}
                Err(stop @ (Stop::InGame | Stop::HumanInput | Stop::Disabled)) => return Err(stop),
                Err(stop) => warn!("next-game: profile not refreshed: {}", stop.message()),
            }
        }
        let rest = Duration::from_secs(u64::from(ng.rest_minutes) * 60);
        self.rest(&ng, rest_left(rest, rest_from.elapsed(), paused))
            .await?;
        nap(LOBBY_SETTLE_MS).await;
        // The rank this game ended on is already in hand — the reward notify
        // that carries the level change arrived with the result, and the
        // client's own account refresh agrees with it — so the room is chosen
        // from the latest rank.
        self.queue_stages(&ng).await
    }

    /// Log out and back in: settings → 登出帳號 → 確認, then on the login
    /// screen (the account still filled in) the fastest route when
    /// `fastest_route`, and 進入遊戲 — every step checked on a screenshot
    /// before the next. Should the lobby not be back within a minute, the
    /// page is reloaded and 進入遊戲 pressed again. Returns how long that
    /// reload took (zero without one), which the rest does not count.
    async fn relogin(
        &self,
        ap: &MajsoulAutoplayConfig,
        fastest_route: bool,
    ) -> Result<Duration, Stop> {
        let page = self.page().await?;
        let mut route: Option<(usize, String, Duration)> = None;
        if fastest_route {
            let url = page.url().await.unwrap_or_default();
            match crate::autoplay::route::gateways(&url).await {
                Ok(gateways) => match crate::autoplay::route::pick_among(&gateways).await {
                    Ok((id, took)) => {
                        let row = gateways
                            .iter()
                            .position(|g| g.id == id)
                            .unwrap_or(usize::MAX);
                        // Remembered too, so a reload falls back on it.
                        let set = format!(
                            "localStorage.setItem('routeID', {})",
                            serde_json::Value::String(id.clone())
                        );
                        if let Err(e) = page.evaluate(&set).await {
                            warn!("next-game: could not store the route: {e:#}");
                        }
                        route = Some((row, id, took));
                    }
                    Err(e) => warn!("next-game: no fastest route ({e:#}); keeping the current one"),
                },
                Err(e) => warn!("next-game: no route list ({e:#}); keeping the current one"),
            }
        }
        self.announce(NextGameStatus::Relogging {
            route: route.as_ref().map(|(_, id, _)| id.clone()),
            ping_ms: route.as_ref().map(|(_, _, t)| t.as_millis() as u32),
        });
        match self
            .log_out_and_in(ap, route.as_ref().map(|(row, _, _)| *row))
            .await
        {
            Ok(()) => Ok(Duration::ZERO),
            Err(stop @ (Stop::InGame | Stop::Disabled | Stop::NoPage)) => Err(stop),
            Err(stop) => {
                warn!(
                    "next-game: logging back in did not reach the lobby ({}); reloading the page",
                    stop.message()
                );
                let reload_from = Instant::now();
                self.reload_and_enter(ap).await?;
                Ok(reload_from.elapsed())
            }
        }
    }

    /// The presses of [`Self::relogin`]; `route_row` is the route list's row
    /// to pick, if any.
    async fn log_out_and_in(
        &self,
        ap: &MajsoulAutoplayConfig,
        route_row: Option<usize>,
    ) -> Result<(), Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        let not_in_game = || !lobby.in_game();
        if lobby.in_game() {
            return Err(Stop::InGame);
        }
        self.press_with(ap, lay.settings_gear, not_in_game)
            .await?;
        tokio::time::sleep(DIALOG_SETTLE).await;
        self.press_with(ap, lay.settings_log_out, not_in_game)
            .await?;
        if !self
            .yellow_within(lay.log_out_confirm_box, DIALOG_WAIT)
            .await
        {
            return Err(Stop::Input("the log-out confirmation did not open".into()));
        }
        self.press_with(ap, lay.log_out_confirm, not_in_game)
            .await?;
        if !self
            .yellow_within(lay.login_enter_box, LOGIN_SCREEN_WAIT)
            .await
        {
            return Err(Stop::Input("the login screen did not come up".into()));
        }
        info!("next-game: logged out, on the login screen");

        if let Some(row) = route_row.filter(|r| *r < lay.login_route_rows) {
            self.press_with(ap, lay.login_switch_route, not_in_game)
                .await?;
            tokio::time::sleep(DIALOG_SETTLE).await;
            // The list dims 進入遊戲; it is back once a row closed the list.
            if self
                .yellow_within(lay.login_enter_box, Duration::ZERO)
                .await
            {
                warn!("next-game: the route list did not open; logging in on the current route");
            } else {
                let at = (
                    lay.login_route_first.0,
                    lay.login_route_first.1 + lay.login_route_step * row as f64,
                );
                self.press_with(ap, at, not_in_game).await?;
                if !self
                    .yellow_within(lay.login_enter_box, DIALOG_WAIT)
                    .await
                {
                    return Err(Stop::Input("the route list did not close".into()));
                }
            }
        }

        self.press_sign_in(ap, not_in_game).await?;
        let lobby_back = self.lobby_within(RELOGIN_WAIT).await;
        self.ctx.lobby.disarm_assist();
        lobby_back
    }

    /// Press what signs the account in on the login screen: 進入遊戲, or for
    /// a Google or Facebook account the provider's icon, where it was pressed
    /// on an earlier login. The provider's own window that opens is answered
    /// by the page poll while the flow is armed (see `autoplay::login`).
    async fn press_sign_in(
        &self,
        ap: &MajsoulAutoplayConfig,
        guard: impl Fn() -> bool,
    ) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let setup = self.ctx.lobby.login_setup();
        match setup.and_then(|s| s.provider_press().map(|at| (s.method, at))) {
            Some((method, at)) => {
                info!("next-game: signing in through {method:?}");
                self.ctx.lobby.arm_assist(method);
                self.press_with(ap, at, guard).await
            }
            None => self.press_with(ap, lay.login_enter, guard).await,
        }
    }

    /// Reload the game page and log in from whatever comes up: the lobby
    /// (the session survived) or the login screen (press 進入遊戲). As
    /// `AutoplayManager::reload_page`, the cached canvas rect is dropped.
    async fn reload_and_enter(&self, ap: &MajsoulAutoplayConfig) -> Result<(), Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        info!("next-game: reloading the game page");
        page.start_reload()
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        *self.ctx.canvas_rect.write().await = None;
        tokio::time::sleep(RELOAD_SETTLE).await;

        let lobby = self.ctx.lobby.clone();
        let deadline = Instant::now() + RELOAD_WAIT;
        let mut entered = false;
        loop {
            if lobby.in_game() {
                return Err(Stop::InGame);
            }
            // Screenshots fail while the page loads; only what they show counts.
            if matches!(self.lobby_visible().await, Ok(true)) {
                return Ok(());
            }
            if !entered
                && self
                    .yellow_within(lay.login_enter_box, Duration::ZERO)
                    .await
            {
                self.press_sign_in(ap, || !lobby.in_game()).await?;
                entered = true;
            }
            if Instant::now() >= deadline {
                return Err(Stop::ReloadNoLobby);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Does `box_norm` show a button's yellow within `wait` (looked at once
    /// for a zero wait)? A screenshot that fails counts as no.
    async fn yellow_within(&self, box_norm: (f64, f64, f64, f64), wait: Duration) -> bool {
        let Ok(lay) = self.lay() else {
            return false;
        };
        let deadline = Instant::now() + wait;
        loop {
            if let Ok(page) = self.page().await {
                if let Ok(rect) = evaluate_canvas_rect(&page).await {
                    if let Ok(true) =
                        result_button_visible(&page, &rect, box_norm, lay.login_min_yellow)
                            .await
                    {
                        return true;
                    }
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(SCREEN_POLL).await;
        }
    }

    /// Wait (up to `wait`) for the lobby's main screen, through screenshots
    /// that may fail while a screen loads.
    async fn lobby_within(&self, wait: Duration) -> Result<(), Stop> {
        let deadline = Instant::now() + wait;
        loop {
            if self.ctx.lobby.in_game() {
                return Err(Stop::InGame);
            }
            if matches!(self.lobby_visible().await, Ok(true)) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Stop::Input("the lobby did not come back".into()));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Is a ranked list (the rooms, or a room's tables) showing? Told by its
    /// header's "<返回" (the layout's `list_back_box`).
    async fn ranked_list_visible(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let bytes = screenshot_box(&page, &rect, lay.list_back_box)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let tan = label_tan_fraction(&bytes).map_err(|e| Stop::Input(format!("{e:#}")))?;
        debug!("next-game: list header tan={tan:.2}");
        Ok(tan >= lay.list_back_min_tan)
    }

    /// For the two buttons: bring the lobby's main screen up from wherever
    /// the player left it. In a game, nothing. From a ranked list — the
    /// rooms, or a room's tables — "<返回" is pressed until the main screen
    /// shows, each press checked on a screenshot; the run then starts from
    /// the top, so the room and table are always the ones it picks. Any
    /// other screen (result screens, a dialog) is left alone.
    async fn to_lobby_main(&self, ap: &MajsoulAutoplayConfig) -> Result<(), Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        for _ in 0..=MAX_LIST_BACKS {
            if lobby.in_game() {
                return Err(Stop::InGame);
            }
            if self.lobby_visible().await? {
                return Ok(());
            }
            if !self.ranked_list_visible().await? {
                return Err(Stop::NotInLobby);
            }
            info!("next-game: on a ranked list; going back to the lobby");
            self.press_with(ap, lay.list_back, || !lobby.in_game())
                .await?;
            tokio::time::sleep(DIALOG_SETTLE).await;
        }
        Err(Stop::NotInLobby)
    }

    /// The Settings page's "log in again" button: log out and back in now
    /// (on the fastest route, if so set), whether or not the next-game flow
    /// is on.
    async fn on_relogin(&mut self) {
        let (ap, fastest_route) = {
            let cfg = self.cfg.read().await;
            if cfg.platform.kind != Platform::Majsoul {
                return;
            }
            (
                cfg.autoplay.majsoul.clone(),
                cfg.autoplay.next_game.fastest_route,
            )
        };
        if self.ctx.lobby.in_game() {
            let _ = self.notify.send(
                Notification::warn("Not logged out")
                    .body("A game is in progress. Log in again from the lobby."),
            );
            return;
        }
        if let Err(stop) = self.to_lobby_main(&ap).await {
            self.tell(&stop).await;
            return;
        }
        match self.relogin(&ap, fastest_route).await {
            Ok(_) => {
                let _ = self
                    .notify
                    .send(Notification::success("Logged in again").body("Back in the lobby."));
            }
            Err(stop) => self.tell(&stop).await,
        }
        self.announce(NextGameStatus::Idle);
    }

    /// Tell the frontend where the flow is (see `NextGameStatus`).
    fn announce(&self, status: NextGameStatus) {
        self.ctx.next_game_status.send_replace(status);
    }

    /// Rest the remaining `rest` in the lobby (see [`rest_left`]).
    ///
    /// The configured rest between games, spent in the lobby (`rest_minutes`;
    /// `0` skips it). Watched like any other wait: a person's click or a game
    /// that starts ends the run, and so does the feature being switched off.
    async fn rest(&self, ng: &NextGameConfig, rest: Duration) -> Result<(), Stop> {
        if rest.is_zero() {
            return Ok(());
        }
        info!(
            "next-game: resting {}s before the next game",
            rest.as_secs()
        );
        let lobby = self.ctx.lobby.clone();
        let deadline = Instant::now() + rest;
        self.announce(NextGameStatus::Resting {
            until_ms: chrono::Utc::now().timestamp_millis() + rest.as_millis() as i64,
            next: target_of(ng, lobby.levels(), self.last_ladder),
        });
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            let slice = left.min(REST_CHECK).as_millis() as u64;
            if self
                .wait_proof(slice, || lobby.in_game().then_some(()))
                .await?
                .is_some()
            {
                return Err(Stop::InGame);
            }
            self.settings().await?;
        }
        Ok(())
    }

    /// Is the result screens' confirm button showing right now?
    async fn result_button_visible(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        result_button_visible(
            &page,
            &rect,
            lay.result_button_box,
            lay.result_button_min_yellow,
        )
        .await
        .map_err(|e| Stop::Input(format!("{e:#}")))
    }

    /// Is a promotion's certificate open right now? Both of its signs —
    /// the parchment and the red 關閉 X — have to be there.
    async fn rank_certificate_open(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let parchment = screenshot_box(&page, &rect, lay.rank_cert_parchment_box)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let close = screenshot_box(&page, &rect, lay.rank_cert_close_box)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let (paper, red) =
            certificate_signature(&parchment, &close).map_err(|e| Stop::Input(format!("{e:#}")))?;
        debug!("next-game: certificate signs parchment={paper:.2} red={red:.2}");
        Ok(paper >= lay.rank_cert_min_parchment && red >= lay.rank_cert_min_red)
    }

    /// Fetch the game's statistics of the player the way a player does: from
    /// the lobby's main screen, press their badge, wait for the profile
    /// window's answer (`fetchAccountStatisticInfo` — the bridge keeps it for
    /// the 個人資料 page), press its two ladder switches for both ladders'
    /// recent games, and close it with its ✕. The ✕ is pressed only
    /// once the window has answered; either way the main screen has to be
    /// back. Returns whether the statistics came.
    async fn refresh_profile_stats(&self, ap: &MajsoulAutoplayConfig) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        if lobby.in_game() {
            return Err(Stop::InGame);
        }
        if !self.lobby_visible().await? {
            return Err(Stop::NotInLobby);
        }
        let ticket = lobby.ticket();
        self.press_with(ap, lay.lobby_avatar, || !lobby.in_game())
            .await?;
        let got = self
            .wait_proof(PROFILE_WAIT_MS, || lobby.stats_since(ticket))
            .await?
            .is_some();
        if got {
            nap(PROFILE_SETTLE_MS).await;
            // The window opens on the 4-player ladder and has sent only its
            // recent games; the 3-player ones come with its switch.
            let before = lobby.ticket();
            self.press_with(ap, lay.profile_3p_switch, || !lobby.in_game())
                .await?;
            self.wait_proof(LADDER_WAIT_MS, || lobby.recent_since(before))
                .await?;
            nap(TAB_SETTLE_MS).await;
            self.press_with(ap, lay.profile_close, || !lobby.in_game())
                .await?;
            tokio::time::sleep(DIALOG_SETTLE).await;
            info!("next-game: profile statistics refreshed");
        } else {
            warn!("next-game: the badge press drew no profile; statistics not refreshed");
        }
        self.wait_for_lobby(4000).await?;
        Ok(got)
    }

    /// The 個人資料 page opened: refresh the statistics if the lobby's main
    /// screen is up. Quiet — the page shows what it has either way.
    async fn on_profile_refresh(&mut self) {
        let ap = {
            let cfg = self.cfg.read().await;
            if cfg.platform.kind != Platform::Majsoul || !cfg.autoplay.next_game.refresh_profile {
                return;
            }
            cfg.autoplay.majsoul.clone()
        };
        if self.ctx.lobby.in_game() {
            return;
        }
        if let Err(stop) = self.refresh_profile_stats(&ap).await {
            debug!("next-game: profile not refreshed: {}", stop.message());
        }
        // Asks that came in while the window was being opened are the same
        // ask: the profile is fresh now, so they do not open it again.
        while futures_util::FutureExt::now_or_never(self.ctx.profile_refresh.notified()).is_some() {
        }
    }

    /// Is the "銅幣不足無法入場" dialog up right now? Its 確認's yellow, in the
    /// middle of the screen.
    async fn coins_dialog_visible(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        result_button_visible(
            &page,
            &rect,
            lay.coins_dialog_box,
            lay.coins_dialog_min_yellow,
        )
        .await
        .map_err(|e| Stop::Input(format!("{e:#}")))
    }

    /// A room or table press drew no answer: if it was the "銅幣不足" dialog,
    /// press it away and say so (the run gets coins and tries again);
    /// otherwise `otherwise`.
    async fn refused(&self, room: RankRoom, need: u64, otherwise: Stop) -> Stop {
        let Ok(lay) = self.lay() else {
            return Stop::Disabled;
        };
        match self.coins_dialog_visible().await {
            Ok(true) => {
                let lobby = self.ctx.lobby.clone();
                info!("next-game: {room:?} refused for want of coins; pressing the dialog away");
                if let Err(stop) = self
                    .press(lay.coins_dialog_confirm, || !lobby.in_game())
                    .await
                {
                    return stop;
                }
                tokio::time::sleep(DIALOG_SETTLE).await;
                Stop::CoinsRefused { room, need }
            }
            Ok(false) => otherwise,
            Err(stop) => stop,
        }
    }

    /// Hold the `need` coins of `room`'s table before pressing anything for
    /// it: short, claim today's revive coins (if still there) and look
    /// again. Coins not known yet are left to the game, whose dialog the run
    /// answers the same way (see `refused`).
    async fn ensure_coins(&self, room: RankRoom, need: u64) -> Result<(), Stop> {
        let lobby = self.ctx.lobby.clone();
        let Some(gold) = lobby.gold() else {
            return Ok(());
        };
        if gold >= need {
            return Ok(());
        }
        info!("next-game: {gold} copper coins, {room:?} needs {need}");
        if lobby.revive_claimed() == Some(true) {
            return Err(Stop::NoCoins { room, gold, need });
        }
        let gold = self.claim_revive_coins().await?.unwrap_or(gold);
        if gold >= need {
            Ok(())
        } else {
            Err(Stop::NoCoins { room, gold, need })
        }
    }

    /// Claim today's revive coins (復活幣) the way a player does, from the
    /// lobby or a ranked list: 活動, the 復活幣 tab, 領取, a tap to close the
    /// 獲得獎勵 banner, and the return arrow. The claim's own answer
    /// (`gainReviveCoin`) is the proof; the coins after it, the result. A
    /// 領取 that is not answered (claimed already, or the rank's revive
    /// coins not due at this many coins) leaves the page closed again and
    /// the day marked claimed.
    async fn claim_revive_coins(&self) -> Result<Option<u64>, Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        let not_in_game = || !lobby.in_game();
        info!("next-game: claiming today's revive coins");
        let ticket = lobby.ticket();
        self.press(lay.lobby_activity, not_in_game).await?;
        nap(ACTIVITY_OPEN_MS).await;
        self.press(lay.activity_revive_tab, not_in_game).await?;
        nap(TAB_SETTLE_MS).await;
        self.press(lay.revive_claim, not_in_game).await?;
        let claimed = self
            .wait_proof(CLAIM_WAIT_MS, || lobby.revive_claimed_since(ticket))
            .await?
            .is_some();
        let gold = if claimed {
            let gold = self.wait_proof(3000, || lobby.gold_since(ticket)).await?;
            info!("next-game: revive coins claimed; copper coins now {gold:?}");
            nap(REWARD_SETTLE_MS).await;
            self.press(lay.reward_dismiss, not_in_game).await?;
            nap(TAB_SETTLE_MS).await;
            gold.or_else(|| lobby.gold())
        } else {
            warn!("next-game: 領取 drew no answer — today's revive coins are not available");
            lobby.note_revive(true);
            None
        };
        self.press(lay.activity_close, not_in_game).await?;
        nap(LIST_OPEN_MS).await;
        Ok(gold)
    }

    /// Is a bond level-up's character story showing right now? Its 跳過
    /// button's pale cyan is where it sits.
    async fn bond_story_open(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let bytes = screenshot_box(&page, &rect, lay.bond_skip_box)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let cyan = skip_cyan_fraction(&bytes).map_err(|e| Stop::Input(format!("{e:#}")))?;
        debug!("next-game: bond story sign cyan={cyan:.2}");
        Ok(cyan >= lay.bond_skip_min_cyan)
    }

    /// Is the lobby's main screen showing right now? Its three signs are
    /// there and nothing — no dialog, no sub-screen — is over them.
    async fn lobby_visible(&self) -> Result<bool, Stop> {
        let lay = self.lay()?;
        let page = self.page().await?;
        let rect = evaluate_canvas_rect(&page)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let bytes = screenshot_box(&page, &rect, lay.lobby_strip)
            .await
            .map_err(|e| Stop::Input(format!("{e:#}")))?;
        let (navy, warm) =
            lobby_signature(&bytes, lay.lobby_navy_rows, lay.lobby_warm_rows)
                .map_err(|e| Stop::Input(format!("{e:#}")))?;
        debug!("next-game: lobby signs navy={navy:.2} warm={warm:.2}");
        Ok(navy >= lay.lobby_navy_min && warm >= lay.lobby_warm_min)
    }

    /// Wait (up to `timeout_ms`) for the lobby's main screen to be showing.
    async fn wait_for_lobby(&self, timeout_ms: u64) -> Result<(), Stop> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if self.lobby_visible().await? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Stop::NotInLobby);
            }
            self.wait_proof(500, || None::<()>).await?;
        }
    }

    /// Press the confirm button for as long as it is showing, until the
    /// result screens (standings, missions, rank) are gone and the lobby's
    /// main screen is showing. The buttonless screens among them — the
    /// room chest's reward, a promotion's rolled-up scroll and opened
    /// certificate, and a bond level-up's character story — are tapped,
    /// closed or skipped on the way (see the loop).
    ///
    /// The screens are looked at, not inferred from the client's traffic: its
    /// account refresh — which this once took for "back in the lobby" —
    /// arrives about 24s after *every* game (39 of 39 in the logs, whatever
    /// had or had not been pressed), and walking through to the lobby sends
    /// nothing at all. The button is only ever pressed while it is seen, so
    /// nothing is pressed into the lobby that follows, and it must have been
    /// seen at least once before its absence means anything — the result
    /// screen is not up yet during the last hand's animation. Gone is not
    /// enough either (a dialog or a reward animation also has no button), so
    /// the lobby itself has to be seen.
    async fn dismiss_result_screens(&self, ng: &NextGameConfig) -> Result<(), Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        let max_presses = ng.max_confirm_presses.max(1);
        let deadline = Instant::now() + DISMISS_TIMEOUT;
        let (mut presses, mut seen, mut absent) = (0u32, false, 0u32);
        let (mut taps, mut cert_closes, mut skips) = (0u32, 0u32, 0u32);
        // Reached only with the rank-change stop off (or no change at all).
        let promoted = lobby.rank().is_some_and(|c| c.promoted());
        if lobby.bond_raised() {
            info!("next-game: the bond went up a level, expecting the character's story");
        }
        loop {
            if lobby.in_game() {
                return Err(Stop::InGame);
            }
            if Instant::now() >= deadline {
                return Err(Stop::NeverReachedLobby);
            }
            if self.result_button_visible().await? {
                seen = true;
                absent = 0;
                if presses >= max_presses {
                    return Err(Stop::NeverReachedLobby);
                }
                presses += 1;
                info!("next-game: confirm button showing, press {presses} of {max_presses}");
                self.press(lay.result_confirm, || !lobby.in_game())
                    .await?;
                self.wait_proof(between(CONFIRM_GAP_MS.0, CONFIRM_GAP_MS.1), || None::<()>)
                    .await?;
            } else {
                if seen {
                    absent += 1;
                }
                // Gone for long enough after being seen — or never seen at
                // all because the player already clicked through to the lobby
                // — and the lobby has to be what is showing. The lobby's signs
                // exist nowhere else, so seeing them is enough on their own.
                if (!seen || absent >= ABSENT_CHECKS) && self.lobby_visible().await? {
                    return Ok(());
                }
                // A bond level-up's character story: its 跳過 ends it. Pressed
                // only while the button is seen, whenever it comes up — the
                // story may come before the standings' button or after it.
                if skips < MAX_SKIP_PRESSES && self.bond_story_open().await? {
                    skips += 1;
                    info!(
                        "next-game: bond story showing, skipping it ({skips} of {MAX_SKIP_PRESSES})"
                    );
                    self.press(lay.bond_skip, || !lobby.in_game()).await?;
                    absent = 0;
                    self.wait_proof(between(CONFIRM_GAP_MS.0, CONFIRM_GAP_MS.1), || None::<()>)
                        .await?;
                    continue;
                }
                // A promotion's certificate, opened: it closes only on its
                // X, and is pressed only while it is seen — the X's spot is
                // nothing in particular on any other screen, but a press is
                // made for what is on screen, not for what should be.
                if seen
                    && absent >= ABSENT_CHECKS
                    && cert_closes < MAX_CERT_CLOSES
                    && self.rank_certificate_open().await?
                {
                    cert_closes += 1;
                    info!(
                        "next-game: rank certificate open, closing it ({cert_closes} of {MAX_CERT_CLOSES})"
                    );
                    self.press(lay.rank_cert_close, || !lobby.in_game())
                        .await?;
                    absent = 0;
                    self.wait_proof(between(CONFIRM_GAP_MS.0, CONFIRM_GAP_MS.1), || None::<()>)
                        .await?;
                    continue;
                }
                // Neither the button nor the lobby: if this game filled the
                // room's chest, it is the chest's reward screen, which only
                // closes on a tap; if it promoted the player, it is the
                // rolled-up certificate, which opens on one (the same spot
                // is clear on both). Tapped only after the result screens
                // have started (the button was seen) and stayed buttonless
                // for a few looks, so a screen merely changing over is left
                // alone — and only as many taps as those screens can use.
                let tap_screens = u32::from(lobby.chest_opened()) + u32::from(promoted);
                if seen && absent >= ABSENT_CHECKS && taps < MAX_CHEST_TAPS * tap_screens {
                    taps += 1;
                    info!(
                        "next-game: chest reward / rank scroll screen, tap {taps} of {}",
                        MAX_CHEST_TAPS * tap_screens
                    );
                    self.press(lay.chest_dismiss, || !lobby.in_game())
                        .await?;
                    absent = 0;
                    self.wait_proof(between(CONFIRM_GAP_MS.0, CONFIRM_GAP_MS.1), || None::<()>)
                        .await?;
                    continue;
                }
                self.wait_proof(1000, || None::<()>).await?;
            }
        }
    }

    /// From the lobby's main screen to a queued game, watching for a
    /// person's click from the start (the start button's entry point; a run
    /// after a game starts watching once the lobby is up). Returns the
    /// `match_sid` queued for.
    async fn queue(&mut self, ng: &NextGameConfig) -> Result<String, Stop> {
        self.arm().await?;
        let mut result = self.queue_stages(ng).await;
        // The game turned the room or table down for want of coins (its
        // dialog is pressed away already): get today's revive coins — 活動
        // is on the ranked lists too — and try once more from the lobby.
        if let Err(Stop::CoinsRefused { room, need }) = result {
            result = self.after_coins_refused(ng, room, need).await;
        }
        self.set_baseline(None);
        result
    }

    async fn after_coins_refused(
        &mut self,
        ng: &NextGameConfig,
        room: RankRoom,
        need: u64,
    ) -> Result<String, Stop> {
        let lobby = self.ctx.lobby.clone();
        let gold = lobby.gold().unwrap_or(0);
        if lobby.revive_claimed() == Some(true) {
            return Err(Stop::NoCoins { room, gold, need });
        }
        let Some(gold) = self.claim_revive_coins().await? else {
            return Err(Stop::NoCoins { room, gold, need });
        };
        let (_, ap) = self.settings().await?;
        self.to_lobby_main(&ap).await?;
        match self.queue_stages(ng).await {
            Err(Stop::CoinsRefused { room, need }) => Err(Stop::NoCoins { room, gold, need }),
            other => other,
        }
    }

    async fn queue_stages(&mut self, ng: &NextGameConfig) -> Result<String, Stop> {
        let lay = self.lay()?;
        let lobby = self.ctx.lobby.clone();
        // Every game this flow starts goes through here, so this is where
        // the daily limit holds.
        self.check_daily_limit(ng)?;
        self.announce(NextGameStatus::Queueing {
            next: target_of(ng, lobby.levels(), self.last_ladder),
        });
        let ticket = lobby.ticket();
        let timeout = u64::from(ng.step_timeout_ms.max(1000));
        let not_in_game = || !lobby.in_game();

        // Everything below is pressed at lobby coordinates, so the lobby has
        // to be what is on screen — after a game it has just been confirmed,
        // from the start button it is whatever the player left up.
        self.wait_for_lobby(3000).await?;

        // Which room, and (unless set by hand) which ladder to take a table
        // from. Each ladder has its own level, so the highest room is worked
        // out from that ladder's alone.
        let ladder = ladder_of(ng.room_mode);
        let room = match ladder {
            Some(ladder) => {
                let room = highest_room(ladder, lobby.levels())?;
                info!("next-game: highest {ladder:?} room: {room:?}");
                room
            }
            None => {
                let room = manual_room(ng, lobby.levels(), self.last_ladder)?;
                if ng.room == RankRoom::Highest {
                    info!("next-game: highest room for the rule: {room:?}");
                }
                room
            }
        };
        // A room that follows the rank may not be the one the last game was
        // in, so the table is named by its East / South and ladder alone.
        let rule = if ng.room == RankRoom::Highest {
            rule_for_changing_room(ng.rule, self.last_mode, self.last_ladder)?
        } else {
            ng.rule
        };

        // Coins: every room above Bronze asks a floor of copper coins of
        // its tables. Short of the room's, today's revive coins first.
        let need = coins_to_open(room, ladder, rule, self.last_mode);
        self.ensure_coins(room, need).await?;

        // The room. One press, one answer: the client replies to a room it
        // will open and says nothing to one it refuses (a dialog puts itself
        // over the lobby), so no answer ends the run rather than pressing on
        // under a dialog.
        if lobby.in_game() {
            return Err(Stop::InGame);
        }
        let (edge, at) = room_target(&lay, room);
        self.press(lay.lobby_ranked, not_in_game).await?;
        nap(LIST_OPEN_MS).await;
        self.scroll(ng.scroll, edge).await?;
        self.press(at, not_in_game).await?;
        let Some(listed) = self
            .wait_proof(timeout, || lobby.room_since(ticket))
            .await?
        else {
            return Err(self.refused(room, need, Stop::RoomNotOpened).await);
        };
        info!("next-game: room opened, tables {listed:?}");

        let index = match ladder {
            // Automatic: the room is whichever one the rank allows, so there
            // is nothing to compare it with. East or South is whichever has
            // more players — the server's answer to the room request, which
            // follows it by a moment.
            Some(ladder) => {
                let counts = self
                    .wait_proof(2000, || lobby.counts_since(ticket))
                    .await?
                    .unwrap_or_default();
                let mut row = pick_row(ladder, &listed, &counts).ok_or(Stop::TableNotListed {
                    index: ladder.rows()[0],
                    listed: listed.len(),
                })?;
                // South busier but its floor out of reach: East, whose floor
                // the coins were checked against.
                if row_is_south(row) && lobby.gold().is_some_and(|g| g < coin_floor(room, true)) {
                    info!("next-game: not enough coins for South; East instead");
                    row = ladder.rows()[0];
                }
                info!("next-game: table row {} (players {counts:?})", row + 1);
                row
            }
            None => {
                // The room has to be the one the last game was played in. Its
                // own list says so: the last table must be in it. (With no
                // last game there is nothing to compare, and the configured
                // rule picks.)
                if let Some(mode) = self.last_mode.filter(|_| ng.room != RankRoom::Highest) {
                    if !listed.contains(&mode) {
                        return Err(Stop::RoomMismatch {
                            want: mode,
                            got: listed,
                        });
                    }
                }
                table_index(rule, self.last_mode, &listed)?
            }
        };
        let (edge, at) = table_target(&lay, index).ok_or(Stop::TableNotListed {
            index,
            listed: listed.len(),
        })?;
        let want = listed[index];

        // The table: pressed once, and answered with a queue or not at all.
        if lobby.in_game() {
            return Err(Stop::InGame);
        }
        self.scroll(ng.scroll, edge).await?;
        self.press(at, not_in_game).await?;
        // The table is pressed: nothing the player does from here is in the
        // way of anything, so the watch ends.
        self.set_baseline(None);
        let Some(sid) = self
            .wait_proof(timeout, || lobby.match_since(ticket))
            .await?
        else {
            let need = coin_floor(room, mode_is_south(want));
            return Err(self.refused(room, need, Stop::TableNotQueued).await);
        };
        if mode_of_sid(&sid) != Some(want) {
            return Err(Stop::WrongTable { want, got: sid });
        }
        self.queued += 1;
        info!("next-game: queued {sid} ({} this session)", self.queued);

        // Queued is not a game. Follow the queue to the end: the server
        // finding a table is the success, a cancel or a timeout is not.
        let queued_at = lobby.ticket();
        match self
            .wait_proof(MATCH_WAIT.as_millis() as u64, || {
                lobby.match_outcome_since(queued_at)
            })
            .await?
        {
            Some(MatchOutcome::Found) => {
                info!("next-game: match found for {sid}");
                Ok(sid)
            }
            Some(MatchOutcome::Cancelled) => Err(Stop::MatchCancelled),
            Some(MatchOutcome::TimedOut) => Err(Stop::MatchTimedOut),
            None => {
                warn!("next-game: still queued for {sid} after {MATCH_WAIT:?}; no longer waiting");
                Ok(sid)
            }
        }
    }

    /// Leave a picture of what the screen was showing when a run stopped, so
    /// "the room did not respond" can be told apart from "something was in
    /// the way" after the fact. Best effort.
    async fn snapshot(&self) -> Option<PathBuf> {
        let page = self.page().await.ok()?;
        let path = self.config_dir.join("next-game-stop.png");
        let saved = match page.screenshot(None).await {
            Ok(png) => std::fs::write(&path, png).map_err(anyhow::Error::from),
            Err(e) => Err(e),
        };
        match saved {
            Ok(()) => Some(path),
            Err(e) => {
                debug!("next-game: screenshot failed: {e:#}");
                None
            }
        }
    }

    /// Refuse to start a game once the logged-in account has played its
    /// daily limit today. Without a known account (or a readable history)
    /// there is nothing to count, and the flow carries on.
    fn check_daily_limit(&self, ng: &NextGameConfig) -> Result<(), Stop> {
        let limit = ng.max_games_per_day.min(crate::config::MAX_GAMES_PER_DAY);
        if limit == 0 {
            return Ok(());
        }
        let (_, profile) = self.ctx.lobby.profile();
        let (Some(account), Some(history)) = (profile.account_id, self.ctx.history.as_ref()) else {
            return Ok(());
        };
        match crate::history::daily::games_today(history, account, chrono::Utc::now()) {
            Ok(played) if played >= limit => {
                info!("next-game: {played} games today, the daily limit is {limit}");
                Err(Stop::DailyLimitReached(limit))
            }
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("next-game: could not count today's games: {e:#}");
                Ok(())
            }
        }
    }

    async fn tell(&self, stop: &Stop) {
        if stop.quiet() {
            debug!("next-game: not running: {}", stop.message());
            return;
        }
        // The picture is for telling what went wrong; a reached daily limit
        // is not something going wrong.
        let shot = if matches!(stop, Stop::DailyLimitReached(_)) {
            None
        } else {
            self.snapshot().await
        };
        warn!(
            "next-game: stopped: {} (screenshot: {shot:?})",
            stop.message()
        );
        let mut body = stop.message();
        if let Some(path) = &shot {
            body.push_str(&format!(" Screen at that moment: {}", path.display()));
        }
        let _ = self
            .notify
            .send(Notification::warn("Next game stopped").body(body));
    }

    async fn on_game_end(&mut self) {
        // A friend-room or AI table has no ranked table to repeat.
        if self.last_mode.is_none() {
            debug!("next-game: the last game was not a ranked one; nothing to repeat");
            return;
        }
        if let Err(stop) = self.after_game().await {
            self.tell(&stop).await;
            if self.blocked(&stop) {
                return;
            }
        }
        self.announce(NextGameStatus::Idle);
    }

    /// A run that stopped for want of coins stays on screen as such — in the
    /// middle of it — until the next run: announced instead of `Idle`.
    fn blocked(&self, stop: &Stop) -> bool {
        let Stop::NoCoins { room, gold, need } = stop else {
            return false;
        };
        self.announce(NextGameStatus::Blocked {
            room: *room,
            gold: *gold,
            need: *need,
        });
        true
    }

    /// The Settings page's "start auto play" button: queue a game from the
    /// lobby now. The games after it follow by themselves, and the count
    /// `max_games` limits starts over again from here.
    async fn on_start(&mut self) {
        let result = async {
            let (ng, ap) = self.settings().await?;
            if self.ctx.lobby.in_game() {
                return Err(Stop::InGame);
            }
            // Wherever the player left it: back to the lobby's main screen.
            self.to_lobby_main(&ap).await?;
            self.queued = 0;
            self.queue(&ng).await
        }
        .await;
        match result {
            Ok(sid) => {
                let _ = self
                    .notify
                    .send(Notification::success("Match found").body(format!(
                        "Started {sid}. The next games follow on their own."
                    )));
            }
            Err(Stop::InGame) => {
                let _ = self.notify.send(
                    Notification::warn("Not started")
                        .body("A game is in progress. Start from the lobby."),
                );
            }
            Err(Stop::Disabled) => {
                let _ = self.notify.send(
                    Notification::warn("Not started")
                        .body("Turn on autoplay and the next-game switch, and save, first."),
                );
            }
            Err(stop) => {
                self.tell(&stop).await;
                if self.blocked(&stop) {
                    return;
                }
            }
        }
        self.announce(NextGameStatus::Idle);
    }
}

/// The mode id of the ranked table a game is being played at, if it is one.
fn ranked_mode(meta: &Option<crate::schema::GameMeta>) -> Option<u32> {
    match meta.as_ref()?.match_info.as_ref()? {
        MatchInfo::Majsoul { mode_id, .. } => *mode_id,
        _ => None,
    }
}

fn ranked_ladder(meta: &Option<crate::schema::GameMeta>) -> Option<Ladder> {
    match meta.as_ref()?.match_info.as_ref()? {
        MatchInfo::Majsoul { level_id, .. } => level_id.and_then(ladder_of_level),
        _ => None,
    }
}

/// Long-lived task: watches for a game ending (and for the test button) and
/// runs the next-game sequence. Its own subscription to the mjai bus, so the
/// minute a run takes never holds the autoplay manager's plan loop up.
pub async fn run_next_game(
    cfg: Arc<RwLock<AppConfig>>,
    ctx: Arc<AutoplayContext>,
    bus: MjaiBus,
    notify: NotifyBus,
    config_dir: PathBuf,
) {
    let mut rx = bus.subscribe();
    let start = ctx.next_game_start.clone();
    let relogin = ctx.next_game_relogin.clone();
    let profile = ctx.profile_refresh.clone();
    let mut flow = NextGame {
        cfg,
        ctx,
        notify,
        last_mode: None,
        last_ladder: None,
        queued: 0,
        config_dir,
        baseline: Mutex::new(None),
    };
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(MjaiEvent::StartGame { game_meta, .. }) => {
                    flow.last_mode = ranked_mode(&game_meta);
                    flow.last_ladder = ranked_ladder(&game_meta);
                    flow.announce(NextGameStatus::Idle);
                }
                Ok(MjaiEvent::EndGame { reason: GameEndReason::Confirmed, .. }) => {
                    flow.on_game_end().await;
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            _ = start.notified() => flow.on_start().await,
            _ = relogin.notified() => flow.on_relogin().await,
            _ = profile.notified() => flow.on_profile_refresh().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Logging in again counts towards the rest, a page reload does not, and
    /// a rest used up by the log-ins leaves nothing.
    /// The coins each table asks, and which the run checks before opening a
    /// room: a rank-picked room its East, a set rule that table.
    #[test]
    fn coin_floors_follow_the_room_and_table() {
        assert_eq!(coin_floor(RankRoom::Bronze, true), 0);
        assert_eq!(coin_floor(RankRoom::Silver, false), 2500);
        assert_eq!(coin_floor(RankRoom::Silver, true), 3500);
        assert_eq!(coin_floor(RankRoom::Gold, true), 7000);
        assert_eq!(coin_floor(RankRoom::Throne, false), 10000);
        assert!(row_is_south(1) && row_is_south(3) && !row_is_south(2));
        assert!(mode_is_south(20) && !mode_is_south(19) && mode_is_south(6));
        // As logged: 2426 coins, 雀士 on the 3-player ladder → Silver East.
        assert_eq!(
            coins_to_open(
                RankRoom::Silver,
                Some(Ladder::Three),
                NextGameRule::Same,
                None
            ),
            2500
        );
        assert_eq!(
            coins_to_open(RankRoom::Silver, None, NextGameRule::ThreeSouth, None),
            3500
        );
        assert_eq!(
            coins_to_open(RankRoom::Gold, None, NextGameRule::Same, Some(22)),
            7000
        );
        assert_eq!(
            coins_to_open(RankRoom::Gold, None, NextGameRule::Same, Some(21)),
            5000
        );
    }

    #[test]
    fn the_rest_counts_the_relogins_but_not_a_reload() {
        let s = Duration::from_secs;
        assert_eq!(rest_left(s(180), s(40), Duration::ZERO), s(140));
        assert_eq!(rest_left(s(180), s(100), s(60)), s(140));
        assert_eq!(rest_left(s(180), s(400), s(10)), Duration::ZERO);
        assert_eq!(
            rest_left(Duration::ZERO, s(40), Duration::ZERO),
            Duration::ZERO
        );
    }

    /// Rank-picked modes name the ladder and its room (once the rank is
    /// known) and leave the table to the player counts; `Manual` names both.
    /// "Highest rank" as the room of a hand-set queue: the ladder comes from
    /// the table rule (or, for "same", from the last game), and the table
    /// from that game's East / South.
    #[test]
    fn a_manual_room_can_follow_the_rank() {
        let levels = (Some(10301), Some(20401));
        let mut ng = NextGameConfig {
            room_mode: RoomMode::Manual,
            room: RankRoom::Highest,
            rule: NextGameRule::FourSouth,
            ..NextGameConfig::default()
        };
        assert_eq!(manual_room(&ng, levels, None), Ok(RankRoom::Gold));
        ng.rule = NextGameRule::ThreeEast;
        assert_eq!(manual_room(&ng, levels, None), Ok(RankRoom::Jade));
        ng.rule = NextGameRule::Same;
        assert_eq!(manual_room(&ng, levels, Some(Ladder::Four)), Ok(RankRoom::Gold));
        assert_eq!(manual_room(&ng, levels, None), Err(Stop::NoTableToRepeat));
        assert_eq!(manual_room(&ng, (None, None), Some(Ladder::Four)), Err(Stop::RankUnknown));
        ng.room = RankRoom::Silver;
        assert_eq!(manual_room(&ng, (None, None), None), Ok(RankRoom::Silver));

        assert_eq!(ladder_of_level(10301), Some(Ladder::Four));
        assert_eq!(ladder_of_level(20202), Some(Ladder::Three));
        assert_eq!(ladder_of_level(0), None);
        // "Same" keeps the last game's ladder and East / South, whichever
        // room that game was in.
        assert_eq!(
            rule_for_changing_room(NextGameRule::Same, Some(12), Some(Ladder::Four)),
            Ok(NextGameRule::FourSouth)
        );
        assert_eq!(
            rule_for_changing_room(NextGameRule::Same, Some(19), Some(Ladder::Three)),
            Ok(NextGameRule::ThreeEast)
        );
        assert_eq!(
            rule_for_changing_room(NextGameRule::Same, None, None),
            Err(Stop::NoTableToRepeat)
        );
        assert_eq!(
            rule_for_changing_room(NextGameRule::ThreeSouth, None, None),
            Ok(NextGameRule::ThreeSouth)
        );
    }

    #[test]
    fn the_next_target_follows_the_room_mode() {
        let mut ng = NextGameConfig {
            room_mode: RoomMode::HighestThree,
            ..NextGameConfig::default()
        };
        let three = target_of(&ng, (None, None), None);
        assert_eq!(
            (three.room, three.players, three.rule),
            (None, Some(3), None)
        );
        ng.room_mode = RoomMode::Manual;
        ng.room = RankRoom::Jade;
        ng.rule = NextGameRule::ThreeSouth;
        let manual = target_of(&ng, (None, None), None);
        assert_eq!(
            (manual.room, manual.players, manual.rule),
            (Some(RankRoom::Jade), None, Some(NextGameRule::ThreeSouth))
        );
    }

    /// "Same" follows the client's own table order, whatever the ids are.
    #[test]
    fn same_table_is_found_by_position_in_the_rooms_list() {
        let listed = [5, 6, 19, 20];
        assert_eq!(table_index(NextGameRule::Same, Some(5), &listed), Ok(0));
        assert_eq!(table_index(NextGameRule::Same, Some(19), &listed), Ok(2));
        assert_eq!(table_index(NextGameRule::Same, Some(20), &listed), Ok(3));
    }

    /// A room that does not list the last game's table is not the room it
    /// was played in (or not one the player can use): that is a stop, not a
    /// guess at the nearest row.
    #[test]
    fn a_room_without_the_last_table_is_a_mismatch() {
        let listed = [7, 8, 21, 22];
        assert_eq!(
            table_index(NextGameRule::Same, Some(19), &listed),
            Err(Stop::RoomMismatch {
                want: 19,
                got: vec![7, 8, 21, 22]
            })
        );
    }

    #[test]
    fn same_with_no_history_has_nothing_to_repeat() {
        assert_eq!(
            table_index(NextGameRule::Same, None, &[5, 6, 19, 20]),
            Err(Stop::NoTableToRepeat)
        );
    }

    /// A fixed rule is a row position, and a room that lists fewer rows than
    /// that does not have it.
    #[test]
    fn a_fixed_rule_needs_its_row_to_exist() {
        assert_eq!(
            table_index(NextGameRule::ThreeSouth, None, &[5, 6, 19, 20]),
            Ok(3)
        );
        assert_eq!(
            table_index(NextGameRule::ThreeSouth, None, &[5, 6, 19]),
            Err(Stop::TableNotListed {
                index: 3,
                listed: 3
            })
        );
    }

    /// The first three tables are read at the top of the list and the fourth
    /// below the fold; the rooms likewise split at gold/jade.
    /// The module's map (`MOKORA_PRO_DLL=<a built mokora_pro library> cargo
    /// test -- --ignored`).
    fn layout() -> Arc<Layout> {
        assert!(crate::pro::load_for_tests(), "set MOKORA_PRO_DLL");
        crate::pro::lobby_layout().expect("the module answers its layout")
    }

    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn tables_and_rooms_pick_the_right_edge() {
        let lay = layout();
        assert_eq!(table_target(&lay, 0).unwrap().0, Edge::Top);
        assert_eq!(table_target(&lay, 2).unwrap().0, Edge::Top);
        assert_eq!(table_target(&lay, 3).unwrap().0, Edge::Bottom);
        assert!(table_target(&lay, 4).is_none());
        assert_eq!(room_target(&lay, RankRoom::Gold).0, Edge::Top);
        assert_eq!(room_target(&lay, RankRoom::Jade).0, Edge::Bottom);
        assert_eq!(room_target(&lay, RankRoom::Throne).0, Edge::Bottom);
    }

    /// Every target sits inside the 16:9 grid and in the list column, and
    /// the rows of one list do not overlap (they are ~1.4 units apart).
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    fn targets_are_inside_the_grid_and_ordered() {
        let lay = layout();
        for (_, (x, y)) in [
            room_target(&lay, RankRoom::Bronze),
            room_target(&lay, RankRoom::Silver),
            room_target(&lay, RankRoom::Gold),
            room_target(&lay, RankRoom::Jade),
            room_target(&lay, RankRoom::Throne),
        ] {
            assert!((0.0..16.0).contains(&x) && (0.0..9.0).contains(&y));
        }
        let ys: Vec<f64> = (0..3).map(|i| table_target(&lay, i).unwrap().1 .1).collect();
        assert!(ys.windows(2).all(|w| w[1] - w[0] > 1.0));
        // The confirm button must not be a point the lobby's room list uses.
        assert!(lay.result_confirm.0 > lay.list_anchor.0 + 2.0);
    }

    /// Each room takes two tiers, and the highest a level may enter is the
    /// room whose upper tier it is on: 雀士 (tier 2) reaches 銀, which is the
    /// room a 3-player 雀士 plays in.
    #[test]
    fn the_highest_room_follows_the_level_tier() {
        assert_eq!(room_for_level(10101), RankRoom::Bronze);
        assert_eq!(room_for_level(10103), RankRoom::Bronze);
        assert_eq!(room_for_level(20201), RankRoom::Silver);
        assert_eq!(room_for_level(10301), RankRoom::Gold);
        assert_eq!(room_for_level(10401), RankRoom::Jade);
        assert_eq!(room_for_level(20501), RankRoom::Throne);
        assert_eq!(room_for_level(10601), RankRoom::Throne);
    }

    /// Each ladder has its own level and so its own highest room: a player
    /// who is 初心 on the 4-player ladder and 雀士 on the 3-player one (the
    /// case this was built for) gets 銅 for the one and 銀 for the other.
    #[test]
    fn the_highest_room_is_worked_out_per_ladder() {
        let levels = (Some(10103), Some(20202));
        assert_eq!(highest_room(Ladder::Four, levels), Ok(RankRoom::Bronze));
        assert_eq!(highest_room(Ladder::Three, levels), Ok(RankRoom::Silver));
    }

    #[test]
    fn a_ladder_with_no_level_has_no_room() {
        assert_eq!(
            highest_room(Ladder::Four, (None, Some(20202))),
            Err(Stop::RankUnknown)
        );
        assert_eq!(
            highest_room(Ladder::Three, (Some(10103), None)),
            Err(Stop::RankUnknown)
        );
    }

    /// East or South follows the player counts the room reported, from the
    /// real room this was written against: East is usually busier, and the
    /// ladder is never crossed.
    #[test]
    fn the_busier_of_east_and_south_is_picked() {
        let listed = [5, 6, 19, 20];
        let counts = [(5, 1859), (6, 601), (19, 2677), (20, 900)];
        assert_eq!(pick_row(Ladder::Four, &listed, &counts), Some(0));
        assert_eq!(pick_row(Ladder::Three, &listed, &counts), Some(2));
        // South busier on the 3-player ladder.
        let counts = [(5, 100), (6, 50), (19, 10), (20, 400)];
        assert_eq!(pick_row(Ladder::Three, &listed, &counts), Some(3));
        assert_eq!(pick_row(Ladder::Four, &listed, &counts), Some(0));
    }

    /// A tie, or a room that reported no counts at all, takes East; a table
    /// the room does not list is not an option.
    #[test]
    fn ties_and_missing_counts_take_east() {
        let listed = [5, 6, 19, 20];
        assert_eq!(pick_row(Ladder::Four, &listed, &[(5, 7), (6, 7)]), Some(0));
        assert_eq!(pick_row(Ladder::Three, &listed, &[]), Some(2));
        assert_eq!(pick_row(Ladder::Three, &[5, 6], &[]), None);
        // Only East of the ladder listed: East by default.
        assert_eq!(pick_row(Ladder::Three, &[5, 6, 19], &[(19, 0)]), Some(2));
    }

    #[test]
    fn between_stays_in_range_whichever_way_round() {
        for _ in 0..500 {
            assert!((3..=9).contains(&between(3, 9)));
            assert!((3..=9).contains(&between(9, 3)));
        }
        assert_eq!(between(7, 7), 7);
    }

    #[test]
    fn only_moot_stops_are_quiet() {
        assert!(Stop::InGame.quiet());
        assert!(Stop::Disabled.quiet());
        assert!(!Stop::NeverReachedLobby.quiet());
        assert!(!Stop::HumanInput.quiet(), "a person is always told");
        assert!(
            !Stop::RoomNotOpened.quiet(),
            "a refused room is always told"
        );
        assert!(!Stop::RankChanged(RankChange { origin: 1, last: 2 }).quiet());
    }

    #[test]
    fn only_a_ranked_start_gives_a_table_to_repeat() {
        use crate::schema::GameMeta;
        let ranked = Some(GameMeta {
            match_info: Some(MatchInfo::Majsoul {
                game_uuid: None,
                mode_id: Some(19),
                room_id: None,
                contest_uid: None,
                account_id: None,
                level_id: None,
                category: None,
                rules: vec![],
            }),
            ..GameMeta::default()
        });
        assert_eq!(ranked_mode(&ranked), Some(19));
        let friend = Some(GameMeta {
            match_info: Some(MatchInfo::Majsoul {
                game_uuid: None,
                mode_id: None,
                room_id: Some(33123),
                contest_uid: None,
                account_id: None,
                level_id: None,
                category: None,
                rules: vec![],
            }),
            ..GameMeta::default()
        });
        assert_eq!(ranked_mode(&friend), None);
        assert_eq!(ranked_mode(&None), None);
    }
}
