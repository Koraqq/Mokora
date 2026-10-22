//! A Mahjong Soul account's games of the day, for the daily game limit
//! (`autoplay.next_game.max_games_per_day`).
//!
//! The day is the game day: it starts at 05:00 UTC+8, when Mahjong Soul
//! resets its dailies, wherever the player is (the frontend's `dailyStats`
//! uses the same moment).
//! A game counts for the account whose id the game itself recorded
//! (`MatchInfo::Majsoul { account_id }`), so every account sharing the
//! history keeps a count of its own.

use crate::history::store::{HistoryStore, NO_LIMIT};
use crate::schema::history::{GameRecord, HistoryFilter, MatchInfo};
use anyhow::Result;
use chrono::{DateTime, Duration, FixedOffset, NaiveTime, TimeZone, Utc};

/// The hour a game day starts at, in [`DAY_ZONE_HOURS`].
pub const DAY_START_HOUR: u32 = 5;
/// The time zone the game day is counted in: UTC+8.
pub const DAY_ZONE_HOURS: i32 = 8;

/// The Mahjong Soul account a record was played by.
pub fn majsoul_account(r: &GameRecord) -> Option<u64> {
    match &r.match_info {
        Some(MatchInfo::Majsoul { account_id, .. }) => *account_id,
        _ => None,
    }
}

/// When the game day `now` falls in began, as UTC: 05:00 UTC+8 that day, or
/// the day before when it is not yet five there.
pub fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    let zone = FixedOffset::east_opt(DAY_ZONE_HOURS * 3600).expect("a valid offset");
    let start = NaiveTime::from_hms_opt(DAY_START_HOUR, 0, 0).expect("a valid hour");
    let there = now.with_timezone(&zone);
    let mut date = there.date_naive();
    if there.time() < start {
        date -= Duration::days(1);
    }
    // A fixed offset has no gaps: the time always exists.
    zone.from_local_datetime(&date.and_time(start))
        .single()
        .map_or(now, |t| t.with_timezone(&Utc))
}

/// How many of `records` account `account_id` started at or after `since`.
pub fn count_since(records: &[GameRecord], account_id: u64, since: DateTime<Utc>) -> u32 {
    records
        .iter()
        .filter(|r| r.started_at >= since && majsoul_account(r) == Some(account_id))
        .count() as u32
}

/// How many games account `account_id` has played in the game day `now` is
/// in.
pub fn games_today(store: &HistoryStore, account_id: u64, now: DateTime<Utc>) -> Result<u32> {
    let since = start_of_day(now);
    let filter = HistoryFilter {
        started_after: Some(since),
        ..Default::default()
    };
    Ok(count_since(
        &store.list(&filter, NO_LIMIT, 0)?,
        account_id,
        since,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(account: Option<u64>, started_at: DateTime<Utc>) -> GameRecord {
        let mut r: GameRecord = serde_json::from_value(serde_json::json!({
            "id": "g",
            "started_at": started_at,
            "ended_at": started_at,
            "platform": "majsoul",
            "num_players": 4,
            "kyoku_mode": "east_only",
            "names": ["a", "b", "c", "d"],
            "our_seat": 0,
            "final_scores": [25000, 25000, 25000, 25000],
            "final_ranks": [1, 2, 3, 4],
            "our_rank": 1,
            "our_delta": 0,
            "stats": crate::schema::history::GameStats::default(),
            "log_path": "games/g.mjai.jsonl",
        }))
        .expect("a minimal record");
        if let Some(id) = account {
            r.match_info = Some(
                serde_json::from_value(serde_json::json!({
                    "platform": "majsoul",
                    "account_id": id,
                }))
                .expect("majsoul match info"),
            );
        }
        r
    }

    /// Only the account's own games since the day began count.
    #[test]
    fn counts_the_accounts_games_of_the_day() {
        let since = Utc::now() - Duration::hours(3);
        let records = vec![
            record(Some(7), since + Duration::minutes(5)),
            record(Some(7), since + Duration::hours(2)),
            record(Some(8), since + Duration::minutes(30)),
            record(Some(7), since - Duration::minutes(1)),
            record(None, since + Duration::minutes(40)),
        ];
        assert_eq!(count_since(&records, 7, since), 2);
        assert_eq!(count_since(&records, 8, since), 1);
        assert_eq!(count_since(&records, 9, since), 0);
    }

    fn utc(day: u32, h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, h, m, 0).unwrap()
    }

    /// The day starts at 05:00 UTC+8 (21:00 UTC the day before); before that
    /// it is still the day before, whatever the computer's own time zone.
    #[test]
    fn the_day_starts_at_five_utc8() {
        let start = utc(21, 21, 0); // 2026-10-22 05:00 UTC+8
        assert_eq!(start_of_day(utc(22, 7, 0)), start); // 15:00 UTC+8
        assert_eq!(start_of_day(utc(21, 21, 0)), start); // 05:00 UTC+8
        assert_eq!(start_of_day(utc(21, 20, 59)), utc(20, 21, 0)); // 04:59 UTC+8
    }
}
