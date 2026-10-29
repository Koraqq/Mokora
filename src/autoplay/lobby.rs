//! Where is the client? Answered from its own lobby uplink.
//!
//! Starting the next game means clicking through screens that are drawn on a
//! canvas, so there is nothing to read off the page. What the client *does* do
//! is talk to the server as it moves between them, and the traffic is
//! consistent enough to steer by. Across every game in a session's logs, the
//! first thing the client sends after the final standings are dismissed is
//! `Lobby.fetchAccountInfo` (about 15-25s after the result arrives, when a
//! person is the one clicking); opening a room's table list sends
//! `Lobby.fetchCurrentMatchInfo` with the room's `mode_list`, in the order the
//! tables are drawn; and picking a table sends `Lobby.startUnifiedMatch` with
//! `match_sid = "1:<mode_id>"`.
//!
//! Each of those is a proof that one stage of the sequence worked, and the
//! first is the one that says to *stop pressing* — the result screen's confirm
//! button sits where the lobby has the shop and the gacha.
//!
//! The Majsoul bridge is the writer (its `parse()` is synchronous, hence a
//! `std::sync::Mutex`), the next-game flow is the reader. Like
//! [`crate::autoplay::verify`] it is counters and tickets rather than
//! timestamps: the question is always "has this happened *since I started
//! waiting*", which a count answers without any clock comparison.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::login::{LoginMethod, LoginSetup};

use crate::schema::{
    AccountProfile, Achievements, LevelScore, MajsoulRankStats, MajsoulRecentGame,
};

/// How a queue for a table ended, as far as the client's traffic says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOutcome {
    /// The server found a game (`NotifyMatchGameStart`).
    Found,
    /// The queue was cancelled from the client (`cancelUnifiedMatch`) — by
    /// the player, as nothing here ever sends one.
    Cancelled,
    /// The server gave up finding a game (`NotifyMatchTimeout`).
    TimedOut,
}

/// A rank change reported at the end of a game
/// (`NotifyGameFinishRewardV2.level_change`). The ids are the client's own
/// level ids (`10103`, `20201`, …); the score within a level is ignored,
/// because moving inside one is not a change anybody has to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankChange {
    pub origin: u32,
    pub last: u32,
}

impl RankChange {
    pub fn changed(&self) -> bool {
        self.origin != self.last
    }

    /// A step up the ladder. Level ids grow with the rank within a ladder
    /// (`10103` 初心3 → `10201` 雀士1), and a change never crosses ladders.
    pub fn promoted(&self) -> bool {
        self.last > self.origin
    }
}

#[derive(Debug, Default)]
struct Inner {
    in_game: bool,
    room_opens: u64,
    room_modes: Vec<u32>,
    /// `(mode_id, playing_count)` from the answer to the last room request.
    room_counts: Vec<(u32, u32)>,
    room_counts_seen: u64,
    match_starts: u64,
    match_sid: Option<String>,
    matches_found: u64,
    matches_cancelled: u64,
    matches_timed_out: u64,
    /// Set by the reward notify that follows a game's result, cleared when
    /// the next result arrives — so a value read here belongs to the game
    /// that just ended and never to an earlier one.
    rank: Option<RankChange>,
    /// The game that just ended filled the room's chest (`match_chest` with
    /// `is_graded` and rewards), so its result screens include the chest's
    /// "獲得獎勵" screen — which has no button and closes on a tap anywhere.
    /// Same lifetime as `rank`.
    chest_opened: bool,
    /// The game that just ended raised the bond with the player's character
    /// a level, so its result screens include the character's story (see
    /// the map's `bond_skip`). Same lifetime as `rank`.
    bond_raised: bool,
    /// The player's current level ids, `(4-player, 3-player)`. From the
    /// client's own account info on every arrival in the lobby, and from the
    /// level a game's reward notify ends on.
    levels: (Option<u32>, Option<u32>),
    /// Who is logged in: `(account_id, nickname)`, from the same account
    /// info. Tells one account's Mokora from another's (see
    /// `crate::accounts`).
    account: Option<(u64, String)>,
    /// Copper coins and jade, from the account info and every
    /// `NotifyAccountUpdate` that moves them.
    gold: Option<u64>,
    diamond: Option<u64>,
    /// `(4-player, 3-player)` level and points, for the profile.
    level_scores: (Option<LevelScore>, Option<LevelScore>),
    /// Today's revive coins: claimed or not, and the day (local, days since
    /// the epoch) that was learned on — they come back every day.
    revive: Option<(bool, i64)>,
    revive_claims: u64,
    gold_updates: u64,
    /// The game's own statistics, and when they arrived (Unix ms).
    stats_updates: u64,
    stats: Vec<MajsoulRankStats>,
    stats_at_ms: Option<i64>,
    achievements: Option<Achievements>,
    /// The game's own lists of recent games, one per `(players, kind)` —
    /// kind 1 ranked, 2 friend, 3 event — each newest first. The game sends
    /// only the list its profile window is showing, so each is kept apart.
    recent_games: std::collections::BTreeMap<(u8, u32), Vec<MajsoulRecentGame>>,
    recent_updates: u64,
    /// Bumped by everything the profile shows, so a reader can tell it moved.
    profile_changes: u64,
    /// The player's main character's id.
    main_character: Option<u32>,
    /// How this account signs in (see `autoplay::login`), and a counter that
    /// moves with it so the saved copy can follow.
    login: Option<LoginSetup>,
    login_changes: u64,
    /// The next-game flow is logging the account back in through a provider:
    /// the provider's window that opens is answered, not just noted.
    assist: Option<(LoginMethod, Instant)>,
}

/// Today, as days since the epoch in local time: the revive coins reset
/// with the day.
fn today() -> i64 {
    use chrono::{Datelike, Local};
    i64::from(Local::now().date_naive().num_days_from_ce())
}

/// Mahjong Soul's yaku ids of the yakuman (35 tenhou … 50 daisuushii).
pub const YAKUMAN_IDS: std::ops::RangeInclusive<u32> = 35..=50;

/// Level ids at or above this are the 3-player ladder (`20201`), below it the
/// 4-player one (`10103`).
const THREE_PLAYER_LEVELS: u32 = 20000;

/// The lobby traffic seen so far.
#[derive(Debug, Default)]
pub struct LobbyWatch {
    inner: Mutex<Inner>,
}

/// A snapshot of the counters, taken before a stage starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LobbyTicket {
    room_opens: u64,
    room_counts_seen: u64,
    match_starts: u64,
    matches_found: u64,
    matches_cancelled: u64,
    matches_timed_out: u64,
    revive_claims: u64,
    gold_updates: u64,
    stats_updates: u64,
    recent_updates: u64,
}

impl LobbyWatch {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock only means a writer panicked mid-update of plain
        // counters; the data is still the best answer there is.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How this account signs in, as far as it is known.
    pub fn login_setup(&self) -> Option<LoginSetup> {
        self.lock().login.clone()
    }

    /// A login showed how the account signs in.
    pub fn note_login_setup(&self, setup: LoginSetup) {
        let mut g = self.lock();
        if g.login.as_ref() != Some(&setup) {
            g.login = Some(setup);
            g.login_changes += 1;
        }
    }

    /// What was saved earlier: known from the start, nothing to save again.
    pub fn restore_login_setup(&self, setup: LoginSetup) {
        let mut g = self.lock();
        if g.login.is_none() {
            g.login = Some(setup);
        }
    }

    /// Moves whenever [`Self::note_login_setup`] changed something.
    pub fn login_changes(&self) -> u64 {
        self.lock().login_changes
    }

    /// The flow is about to press a provider's icon: answer its window.
    pub fn arm_assist(&self, method: LoginMethod) {
        self.lock().assist = Some((method, Instant::now()));
    }

    /// The provider the flow is waiting on, while it still is (a minute).
    pub fn assist_armed(&self) -> Option<LoginMethod> {
        self.lock()
            .assist
            .filter(|(_, since)| since.elapsed() < Duration::from_secs(60))
            .map(|(method, _)| method)
    }

    pub fn disarm_assist(&self) {
        self.lock().assist = None;
    }

    /// A game's `authGame` came back: the client is at a table.
    pub fn note_game_started(&self) {
        self.lock().in_game = true;
    }

    /// The game's result (or its termination) arrived. Forgets the previous
    /// game's rank change so the next reward notify is unambiguous.
    pub fn note_game_ended(&self) {
        let mut g = self.lock();
        g.in_game = false;
        g.rank = None;
        g.chest_opened = false;
        g.bond_raised = false;
    }

    /// The game's reward notify says the room's chest filled and paid out.
    pub fn note_chest_opened(&self) {
        self.lock().chest_opened = true;
    }

    /// Did the game that just ended open the room's chest?
    pub fn chest_opened(&self) -> bool {
        self.lock().chest_opened
    }

    /// The game's reward notify says the bond with the character went up a
    /// level.
    pub fn note_bond_raised(&self) {
        self.lock().bond_raised = true;
    }

    /// Did the game that just ended raise the bond a level?
    pub fn bond_raised(&self) -> bool {
        self.lock().bond_raised
    }

    /// The client asked for its own account info.
    pub fn note_lobby_entered(&self) {
        let mut g = self.lock();
        // Nobody asks for their own account info from a table, so this also
        // corrects a game whose result was missed (a dropped connection) and
        // would otherwise leave the client looking "in a game" for good.
        g.in_game = false;
    }

    /// The client opened a room's table list.
    pub fn note_room_opened(&self, modes: Vec<u32>) {
        let mut g = self.lock();
        g.room_opens += 1;
        g.room_modes = modes;
    }

    /// The server answered a room request with how many are playing each of
    /// its tables.
    pub fn note_room_counts(&self, counts: Vec<(u32, u32)>) {
        let mut g = self.lock();
        g.room_counts_seen += 1;
        g.room_counts = counts;
    }

    /// The client queued for a table.
    pub fn note_match_started(&self, sid: Option<String>) {
        let mut g = self.lock();
        g.match_starts += 1;
        g.match_sid = sid;
    }

    /// The server found a game for the queue.
    pub fn note_match_found(&self) {
        self.lock().matches_found += 1;
    }

    /// The client cancelled its queue.
    pub fn note_match_cancelled(&self) {
        self.lock().matches_cancelled += 1;
    }

    /// The server timed the queue out.
    pub fn note_match_timed_out(&self) {
        self.lock().matches_timed_out += 1;
    }

    /// The game's reward notify carried a level change.
    pub fn note_rank(&self, change: RankChange) {
        let mut g = self.lock();
        g.rank = Some(change);
        if change.last >= THREE_PLAYER_LEVELS {
            g.levels.1 = Some(change.last);
        } else {
            g.levels.0 = Some(change.last);
        }
    }

    /// The player's own levels, from their account info.
    pub fn note_levels(&self, four: Option<u32>, three: Option<u32>) {
        let mut g = self.lock();
        if four.is_some() {
            g.levels.0 = four;
        }
        if three.is_some() {
            g.levels.1 = three;
        }
    }

    /// `(4-player, 3-player)` level ids, as far as they are known.
    pub fn levels(&self) -> (Option<u32>, Option<u32>) {
        self.lock().levels
    }

    /// The logged-in account, from its account info.
    pub fn note_account(&self, account_id: u64, nickname: String) {
        self.lock().account = Some((account_id, nickname));
    }

    /// `(account_id, nickname)` of the logged-in account, once known.
    pub fn account(&self) -> Option<(u64, String)> {
        self.lock().account.clone()
    }

    /// The player's main character, from `fetchCharacterInfo` (or after they
    /// change it).
    pub fn note_main_character(&self, id: u32) {
        let mut g = self.lock();
        if g.main_character != Some(id) {
            g.main_character = Some(id);
            g.profile_changes += 1;
        }
    }

    /// Coins and jade from the account info (either may be missing).
    pub fn note_wallet(&self, gold: Option<u64>, diamond: Option<u64>) {
        let mut g = self.lock();
        if gold.is_some() && gold != g.gold {
            g.gold = gold;
            g.gold_updates += 1;
            g.profile_changes += 1;
        }
        if diamond.is_some() && diamond != g.diamond {
            g.diamond = diamond;
            g.profile_changes += 1;
        }
    }

    /// Copper coins held, once known.
    pub fn gold(&self) -> Option<u64> {
        self.lock().gold
    }

    /// The levels with their points, for the profile.
    pub fn note_level_scores(&self, four: Option<LevelScore>, three: Option<LevelScore>) {
        let mut g = self.lock();
        let next = (four.or(g.level_scores.0), three.or(g.level_scores.1));
        if next != g.level_scores {
            g.level_scores = next;
            g.profile_changes += 1;
        }
    }

    /// `fetchReviveCoinInfo` answered: today's revive coins claimed or not.
    pub fn note_revive(&self, claimed: bool) {
        let mut g = self.lock();
        g.revive = Some((claimed, today()));
        g.profile_changes += 1;
    }

    /// `gainReviveCoin` succeeded: today's are claimed.
    pub fn note_revive_claimed(&self) {
        let mut g = self.lock();
        g.revive = Some((true, today()));
        g.revive_claims += 1;
        g.profile_changes += 1;
    }

    /// Today's revive coins: `Some(true)` claimed, `Some(false)` not yet,
    /// `None` when not known for today (never asked, or asked yesterday).
    pub fn revive_claimed(&self) -> Option<bool> {
        self.lock()
            .revive
            .and_then(|(claimed, day)| (day == today()).then_some(claimed))
    }

    /// The revive coins were claimed since `ticket`.
    pub fn revive_claimed_since(&self, ticket: LobbyTicket) -> Option<()> {
        (self.lock().revive_claims != ticket.revive_claims).then_some(())
    }

    /// The coin count moved since `ticket`: the new count.
    pub fn gold_since(&self, ticket: LobbyTicket) -> Option<u64> {
        let g = self.lock();
        (g.gold_updates != ticket.gold_updates)
            .then_some(g.gold)
            .flatten()
    }

    /// The player's own statistics arrived since `ticket`.
    pub fn stats_since(&self, ticket: LobbyTicket) -> Option<()> {
        (self.lock().stats_updates != ticket.stats_updates).then_some(())
    }

    /// The game's own statistics of the player, just fetched.
    pub fn note_stats(&self, stats: Vec<MajsoulRankStats>) {
        let mut g = self.lock();
        g.stats = stats;
        g.stats_updates += 1;
        g.stats_at_ms = Some(chrono::Utc::now().timestamp_millis());
        g.profile_changes += 1;
    }

    /// Achievements by rarity, from the account info.
    pub fn note_achievements(&self, achievements: Achievements) {
        let mut g = self.lock();
        if g.achievements != Some(achievements) {
            g.achievements = Some(achievements);
            g.profile_changes += 1;
        }
    }

    /// The game's own list of the player's recent `players`-player games
    /// of one kind (1 ranked, 2 friend, 3 event).
    pub fn note_recent_games(&self, players: u8, kind: u32, games: Vec<MajsoulRecentGame>) {
        let mut g = self.lock();
        g.recent_games.insert((players, kind), games);
        g.recent_updates += 1;
        g.profile_changes += 1;
    }

    /// A recent-games list arrived since `ticket`.
    pub fn recent_since(&self, ticket: LobbyTicket) -> Option<()> {
        (self.lock().recent_updates != ticket.recent_updates).then_some(())
    }

    /// Everything the profile shows, and a counter that moves with it.
    pub fn profile(&self) -> (u64, AccountProfile) {
        let g = self.lock();
        let revive_claimed = g
            .revive
            .and_then(|(claimed, day)| (day == today()).then_some(claimed));
        (
            g.profile_changes,
            AccountProfile {
                account_id: g.account.as_ref().map(|a| a.0),
                nickname: g.account.as_ref().map(|a| a.1.clone()),
                main_character: g.main_character,
                gold: g.gold,
                diamond: g.diamond,
                level4: g.level_scores.0,
                level3: g.level_scores.1,
                revive_claimed,
                stats: g.stats.clone(),
                stats_at_ms: g.stats_at_ms,
                achievements: g.achievements,
                recent_games: merge_recent(g.recent_games.values().flatten().cloned(), []),
                updated_ms: chrono::Utc::now().timestamp_millis(),
            },
        )
    }

    pub fn ticket(&self) -> LobbyTicket {
        let g = self.lock();
        LobbyTicket {
            room_opens: g.room_opens,
            room_counts_seen: g.room_counts_seen,
            match_starts: g.match_starts,
            matches_found: g.matches_found,
            matches_cancelled: g.matches_cancelled,
            matches_timed_out: g.matches_timed_out,
            revive_claims: g.revive_claims,
            gold_updates: g.gold_updates,
            stats_updates: g.stats_updates,
            recent_updates: g.recent_updates,
        }
    }

    pub fn in_game(&self) -> bool {
        self.lock().in_game
    }

    /// The `mode_list` of the room opened since `ticket`, if one was.
    pub fn room_since(&self, ticket: LobbyTicket) -> Option<Vec<u32>> {
        let g = self.lock();
        (g.room_opens != ticket.room_opens).then(|| g.room_modes.clone())
    }

    /// `(mode_id, playing_count)` for the tables of the room, if the server
    /// has answered since `ticket`.
    pub fn counts_since(&self, ticket: LobbyTicket) -> Option<Vec<(u32, u32)>> {
        let g = self.lock();
        (g.room_counts_seen != ticket.room_counts_seen).then(|| g.room_counts.clone())
    }

    /// The `match_sid` queued for since `ticket`, if the client queued.
    pub fn match_since(&self, ticket: LobbyTicket) -> Option<String> {
        let g = self.lock();
        (g.match_starts != ticket.match_starts).then(|| g.match_sid.clone().unwrap_or_default())
    }

    /// How the queue ended, if it has since `ticket`. A found game wins over
    /// a cancel that raced it: the game is what the player is in.
    pub fn match_outcome_since(&self, ticket: LobbyTicket) -> Option<MatchOutcome> {
        let g = self.lock();
        if g.matches_found != ticket.matches_found {
            Some(MatchOutcome::Found)
        } else if g.matches_cancelled != ticket.matches_cancelled {
            Some(MatchOutcome::Cancelled)
        } else if g.matches_timed_out != ticket.matches_timed_out {
            Some(MatchOutcome::TimedOut)
        } else {
            None
        }
    }

    /// The rank change of the game that just ended, once its reward arrived.
    pub fn rank(&self) -> Option<RankChange> {
        self.lock().rank
    }
}

/// Recent games from several lists as one, newest first: a game in more than
/// one (the same uuid) once, the first list's copy kept.
pub fn merge_recent(
    first: impl IntoIterator<Item = MajsoulRecentGame>,
    then: impl IntoIterator<Item = MajsoulRecentGame>,
) -> Vec<MajsoulRecentGame> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<MajsoulRecentGame> = first
        .into_iter()
        .chain(then)
        .filter(|g| seen.insert(g.uuid.clone()))
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.end_time));
    out
}

/// Shared between the Majsoul bridge (writes) and the next-game flow (reads).
pub type SharedLobbyWatch = Arc<LobbyWatch>;

/// The mode id inside a `match_sid` of the form `"<kind>:<mode_id>"`.
pub fn mode_of_sid(sid: &str) -> Option<u32> {
    sid.rsplit(':').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coins move with the account info and each update; a claim of the
    /// revive coins is seen once, and remembered for the day.
    #[test]
    fn coins_and_revive_coins_are_tracked() {
        let w = LobbyWatch::default();
        assert_eq!((w.gold(), w.revive_claimed()), (None, None));
        w.note_wallet(Some(2426), Some(0));
        let t = w.ticket();
        assert_eq!(w.gold_since(t), None);
        w.note_revive(false);
        assert_eq!(w.revive_claimed(), Some(false));
        w.note_revive_claimed();
        w.note_wallet(Some(6926), None);
        assert_eq!(w.revive_claimed_since(t), Some(()));
        assert_eq!(w.gold_since(t), Some(6926));
        assert_eq!(w.revive_claimed(), Some(true));
        let (_, p) = w.profile();
        assert_eq!(
            (p.gold, p.diamond, p.revive_claimed),
            (Some(6926), Some(0), Some(true))
        );
    }

    /// Each ladder's list is kept: the 4-player one arriving does not drop
    /// the 3-player one, and the profile shows both, newest first.
    #[test]
    fn recent_games_of_each_ladder_are_kept() {
        let game = |uuid: &str, end: i64| MajsoulRecentGame {
            uuid: uuid.into(),
            end_time: end,
            ..Default::default()
        };
        let w = LobbyWatch::default();
        let t = w.ticket();
        w.note_recent_games(3, 1, vec![game("a", 3), game("b", 1)]);
        assert_eq!(w.recent_since(t), Some(()));
        w.note_recent_games(4, 1, vec![game("c", 2)]);
        let uuids: Vec<String> = w
            .profile()
            .1
            .recent_games
            .into_iter()
            .map(|g| g.uuid)
            .collect();
        assert_eq!(uuids, ["a", "c", "b"]);
    }

    /// The room's table order is what picks which row to press, so the list
    /// has to come back exactly as the client sent it.
    #[test]
    fn room_list_keeps_the_clients_order() {
        let w = LobbyWatch::default();
        let t = w.ticket();
        assert_eq!(w.room_since(t), None);
        w.note_room_opened(vec![5, 6, 19, 20]);
        assert_eq!(w.room_since(t), Some(vec![5, 6, 19, 20]));
        // The client re-polls an open room every few seconds; a fresh
        // ticket must not see the old one.
        let t = w.ticket();
        assert_eq!(w.room_since(t), None);
    }

    #[test]
    fn table_counts_arrive_after_the_room_request() {
        let w = LobbyWatch::default();
        let t = w.ticket();
        assert_eq!(w.counts_since(t), None);
        w.note_room_counts(vec![(5, 1859), (6, 601)]);
        assert_eq!(w.counts_since(t), Some(vec![(5, 1859), (6, 601)]));
    }

    /// A queue ends one of three ways, and only what happens *after* the
    /// ticket counts — a cancel from an earlier queue must not end this one.
    #[test]
    fn a_queue_ends_found_cancelled_or_timed_out() {
        let w = LobbyWatch::default();
        w.note_match_cancelled();
        let t = w.ticket();
        assert_eq!(
            w.match_outcome_since(t),
            None,
            "an older cancel is not ours"
        );
        w.note_match_timed_out();
        assert_eq!(w.match_outcome_since(t), Some(MatchOutcome::TimedOut));
        w.note_match_cancelled();
        assert_eq!(w.match_outcome_since(t), Some(MatchOutcome::Cancelled));
        w.note_match_found();
        assert_eq!(w.match_outcome_since(t), Some(MatchOutcome::Found));
    }

    #[test]
    fn match_start_carries_the_sid() {
        let w = LobbyWatch::default();
        let t = w.ticket();
        assert_eq!(w.match_since(t), None);
        w.note_match_started(Some("1:19".into()));
        assert_eq!(w.match_since(t).as_deref(), Some("1:19"));
    }

    /// A rank change belongs to one game: the next result wipes it, so a
    /// stale promotion can never stop (or fail to stop) a later game.
    #[test]
    fn a_rank_change_is_forgotten_when_the_next_game_ends() {
        let w = LobbyWatch::default();
        w.note_game_started();
        assert!(w.in_game());
        w.note_game_ended();
        assert!(!w.in_game());
        w.note_rank(RankChange {
            origin: 20201,
            last: 20301,
        });
        assert!(w.rank().unwrap().changed());
        assert!(w.rank().unwrap().promoted());
        assert!(!RankChange {
            origin: 10201,
            last: 10103
        }
        .promoted());
        assert!(!RankChange {
            origin: 10201,
            last: 10201
        }
        .promoted());
        w.note_game_started();
        w.note_game_ended();
        assert_eq!(w.rank(), None);
    }

    /// Like a rank change, an opened chest belongs to one game.
    #[test]
    fn an_opened_chest_is_forgotten_when_the_next_game_ends() {
        let w = LobbyWatch::default();
        w.note_game_ended();
        w.note_chest_opened();
        assert!(w.chest_opened());
        w.note_game_started();
        w.note_game_ended();
        assert!(!w.chest_opened());
    }

    /// So does a bond level-up.
    #[test]
    fn a_bond_level_up_is_forgotten_when_the_next_game_ends() {
        let w = LobbyWatch::default();
        w.note_game_ended();
        w.note_bond_raised();
        assert!(w.bond_raised());
        w.note_game_started();
        w.note_game_ended();
        assert!(!w.bond_raised());
    }

    /// A game whose result never arrived must not read as running forever.
    #[test]
    fn arriving_in_the_lobby_means_no_game_is_running() {
        let w = LobbyWatch::default();
        w.note_game_started();
        assert!(w.in_game());
        w.note_lobby_entered();
        assert!(!w.in_game());
    }

    /// Each ladder keeps its own level, and a game's reward moves only the
    /// ladder it was played on.
    /// How the account signs in is learned from its logins, kept across runs
    /// without being saved again, and the flow's wait for a provider's window
    /// is armed and disarmed.
    #[test]
    fn how_the_account_signs_in_is_learned_and_restored() {
        let w = LobbyWatch::default();
        assert_eq!(w.login_setup(), None);
        let google = LoginSetup {
            method: LoginMethod::Google,
            press: Some((6.0, 7.5)),
            account: None,
        };
        // What was saved earlier is known, but is not a change to save.
        w.restore_login_setup(google.clone());
        assert_eq!(w.login_setup(), Some(google.clone()));
        assert_eq!(w.login_changes(), 0);
        // The same login again changes nothing; a different one does.
        w.note_login_setup(google);
        assert_eq!(w.login_changes(), 0);
        w.note_login_setup(LoginSetup::password());
        assert_eq!(w.login_changes(), 1);
        assert_eq!(w.login_setup(), Some(LoginSetup::password()));
        // Restoring never overrides what a login showed.
        w.restore_login_setup(LoginSetup {
            method: LoginMethod::Facebook,
            press: None,
            account: None,
        });
        assert_eq!(w.login_setup(), Some(LoginSetup::password()));

        assert_eq!(w.assist_armed(), None);
        w.arm_assist(LoginMethod::Facebook);
        assert_eq!(w.assist_armed(), Some(LoginMethod::Facebook));
        w.disarm_assist();
        assert_eq!(w.assist_armed(), None);
    }

    #[test]
    fn levels_are_kept_per_ladder() {
        let w = LobbyWatch::default();
        assert_eq!(w.levels(), (None, None));
        w.note_levels(Some(10103), Some(20201));
        assert_eq!(w.levels(), (Some(10103), Some(20201)));
        w.note_rank(RankChange {
            origin: 20201,
            last: 20301,
        });
        assert_eq!(w.levels(), (Some(10103), Some(20301)));
        w.note_rank(RankChange {
            origin: 10103,
            last: 10201,
        });
        assert_eq!(w.levels(), (Some(10201), Some(20301)));
    }

    /// Gaining score inside a level is not a rank change.
    #[test]
    fn same_level_is_not_a_change() {
        let c = RankChange {
            origin: 20201,
            last: 20201,
        };
        assert!(!c.changed());
    }

    #[test]
    fn sid_parses_to_a_mode_id() {
        assert_eq!(mode_of_sid("1:19"), Some(19));
        assert_eq!(mode_of_sid("19"), Some(19));
        assert_eq!(mode_of_sid("1:x"), None);
        assert_eq!(mode_of_sid(""), None);
    }
}
