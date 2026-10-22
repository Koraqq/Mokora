//! Deleting an account, all of it.
//!
//! What an account leaves on disk, and what goes:
//!
//! - its own folder `accounts/<n>/` ??config, the game browser's profile (the
//!   Mahjong Soul login lives there), logs, UI preferences, the saved profile
//!   and login time ??removed whole, the folder with it;
//! - its games in the shared history (`history/index.jsonl`, the event logs
//!   under `games/`) and the reviews of them (`history/reviews/`): every record
//!   whose Mahjong Soul account is the one this account logged in as. That is
//!   what the bots played, so the statistics, charts and review go with it;
//! - the startup star, if it was on this account.
//!
//! The games may also be kept (`clear_records` off): the account's login and
//! files go, its records stay, and they are its again when that Mahjong Soul
//! account logs in later.
//!
//! The first account (`main`) is the app's base ??its config is what the app
//! starts from, and `accounts/` holds only the others ??so it is emptied, not
//! removed: login, profile, records and logs go, the config and the place in
//! the picker stay ("撣唾? 1", not logged in).
//!
//! Left alone, on purpose: records from before an account's id was recorded
//! (they cannot be told from anyone's), and the records of a Mahjong Soul
//! account another entry of the picker is logged in as too ??they still
//! belong to that entry.
//!
//! The account the window shows cannot be deleted. An account that is open is
//! closed first by the caller (`Hub::close`), so its web views let go of the
//! folder.

use super::{
    numbered_dirs, same_path, set_startup_account, startup_account, AccountInfo, ACCOUNTS_DIR,
    MAIN_ID,
};
use crate::history::HistoryStore;
use crate::review::store::ReviewStore;
use crate::schema::GameRecord;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Removing a folder a game browser has only just let go of may need a few
/// tries.
const REMOVE_TRIES: u32 = 20;
const REMOVE_PAUSE: Duration = Duration::from_millis(500);

/// What was removed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Removed {
    /// Game records (with their logs and reviews) removed.
    pub records: u32,
    /// The records stayed: another entry is logged in as the same account.
    pub records_kept: bool,
    /// Things that could not be removed (still in use, outside the app).
    pub left: Vec<String>,
}

/// The Mahjong Soul account a record was played by.
pub(super) fn record_account(r: &GameRecord) -> Option<u64> {
    crate::history::daily::majsoul_account(r)
}

/// Ids of the records of Mahjong Soul account `account_id`.
pub(super) fn records_of(records: &[GameRecord], account_id: u64) -> Vec<String> {
    records
        .iter()
        .filter(|r| record_account(r) == Some(account_id))
        .map(|r| r.id.clone())
        .collect()
}

/// Why `target` cannot be deleted.
fn refusal(target: &AccountInfo) -> Option<&'static str> {
    if target.current {
        Some("this is the account the window shows; switch to another first")
    } else if target.running {
        Some("this account is open; close it first")
    } else {
        None
    }
}

/// Delete account `id` of `accounts` (as the hub lists them), with its games
/// when `clear_records`. The account must be closed, and must not be the one
/// the window shows.
pub async fn remove(
    history: &HistoryStore,
    accounts: &[AccountInfo],
    id: &str,
    clear_records: bool,
) -> Result<Removed> {
    let target = accounts
        .iter()
        .find(|a| a.id == id)
        .with_context(|| format!("no account {id}"))?
        .clone();
    if let Some(why) = refusal(&target) {
        bail!(why);
    }

    // Where its files are is settled before anything is deleted: refusing the
    // folder after its games had gone would leave the account with no games.
    let folder = account_folder(&target)?;

    let mut removed = Removed::default();

    // Its games, unless kept or another entry is the same Mahjong Soul account.
    if !clear_records {
        removed.records_kept = true;
    } else if let Some(identity) = &target.identity {
        let shared = accounts.iter().any(|a| {
            a.id != target.id
                && a.identity
                    .as_ref()
                    .is_some_and(|i| i.account_id == identity.account_id)
        });
        if shared {
            removed.records_kept = true;
        } else {
            removed.records = remove_games(
                history,
                &ReviewStore::new(history.root()),
                identity.account_id,
            )? as u32;
        }
    }

    // Its files.
    let startup_was_it = startup_account() == target.id;
    match folder {
        None => wipe_main(&target.config_path, &mut removed).await,
        Some(dir) => {
            if let Err(e) = remove_dir(&dir).await {
                removed.left.push(format!("{}: {e:#}", dir.display()));
            }
        }
    }
    if startup_was_it && target.id != MAIN_ID {
        set_startup_account(MAIN_ID)?;
    }
    Ok(removed)
}

/// The folder to remove for `target`: `None` for the first account (which is
/// emptied, not removed), else its own folder under `accounts/`. An error when
/// that is not an account folder.
fn account_folder(target: &AccountInfo) -> Result<Option<PathBuf>> {
    if target.id == MAIN_ID {
        return Ok(None);
    }
    let dir = target
        .config_path
        .parent()
        .with_context(|| format!("account {} has no folder", target.id))?;
    let root = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR));
    let is_numbered = dir.parent().is_some_and(|p| same_path(p, &root))
        && numbered_dirs().iter().any(|(_, d)| same_path(d, dir));
    if !is_numbered {
        bail!("{} is not an account folder", dir.display());
    }
    Ok(Some(dir.to_path_buf()))
}

/// Remove every record of Mahjong Soul account `account_id`, its event log and
/// its reviews. Returns how many records went.
pub(super) fn remove_games(
    history: &HistoryStore,
    reviews: &ReviewStore,
    account_id: u64,
) -> Result<usize> {
    let ids: HashSet<String> = records_of(&history.read_all()?, account_id)
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let n = history.delete_many(&ids)?;
    for id in &ids {
        reviews.delete_game(id);
    }
    Ok(n)
}

/// Empty the first account: its login, profile, saved identity and logs. The
/// config stays, with the place in the picker.
async fn wipe_main(config_path: &Path, removed: &mut Removed) {
    if let Some(dir) = config_path.parent() {
        for name in [
            super::IDENTITY_FILE,
            super::LOGIN_FILE,
            super::PROFILE_FILE,
            super::AVATAR_FILE,
        ] {
            let _ = std::fs::remove_file(dir.join(name));
        }
    }
    let (profile, logs) = config_dirs(config_path);
    for dir in [profile, logs].into_iter().flatten() {
        if !within_app(&dir) {
            removed
                .left
                .push(format!("{} (outside the app folder)", dir.display()));
            continue;
        }
        if dir.exists() {
            if let Err(e) = remove_dir(&dir).await {
                removed.left.push(format!("{}: {e:#}", dir.display()));
            }
        }
    }
}

/// The game browser profile and the log directory a config names.
fn config_dirs(config_path: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(raw) = std::fs::read_to_string(config_path) else {
        return (None, None);
    };
    let Ok(doc) = toml::from_str::<toml::Value>(&raw) else {
        return (None, None);
    };
    let profile = crate::capture::chromium::profile::resolve_profile_dir(
        doc.get("capture")
            .and_then(|c| c.get("chromium"))
            .and_then(|c| c.get("user_data_dir"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .ok();
    let logs = doc
        .get("logging")
        .and_then(|l| l.get("dir"))
        .and_then(|v| v.as_str())
        .map(|d| crate::util::resolve_dir(Path::new(d)));
    (profile, logs)
}

/// Is `dir` inside the folder the app runs from? Only such a folder is wiped
/// on the first account's behalf: a path it was pointed at elsewhere is the
/// user's own.
fn within_app(dir: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(app) = exe.parent() else {
        return false;
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (dir, app) = (canon(dir), canon(app));
    dir.starts_with(&app) && dir != app
}

/// `remove_dir_all`, tried again while a browser lets go of its files.
async fn remove_dir(dir: &Path) -> Result<()> {
    let mut last = None;
    for _ in 0..REMOVE_TRIES {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(REMOVE_PAUSE).await;
    }
    Err(last.map_or_else(|| anyhow::anyhow!("not removed"), Into::into))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::testkit;
    use crate::schema::MatchInfo;

    fn record_of(id: &str, account: Option<u64>) -> GameRecord {
        let dir = tempfile::tempdir().unwrap();
        let (history, game) = testkit::history_with_one_game(dir.path());
        let mut r = history.get(&game).unwrap().unwrap();
        r.id = id.to_string();
        r.match_info = Some(MatchInfo::Majsoul {
            game_uuid: None,
            mode_id: None,
            room_id: None,
            contest_uid: None,
            account_id: account,
            level_id: None,
            category: None,
            rules: vec![],
        });
        r
    }

    /// Only the records of the Mahjong Soul account are picked; one with no
    /// account recorded is nobody's to take.
    #[test]
    fn records_are_picked_by_their_mahjong_soul_account() {
        let all = [
            record_of("a", Some(7)),
            record_of("b", Some(8)),
            record_of("c", None),
            record_of("d", Some(7)),
        ];
        assert_eq!(records_of(&all, 7), vec!["a", "d"]);
        assert!(records_of(&all, 9).is_empty());
    }

    /// The window's own account, and an open one, cannot be deleted.
    #[test]
    fn the_window_account_and_an_open_one_are_refused() {
        let base = |current, running| AccountInfo {
            id: "2".into(),
            config_path: PathBuf::from("accounts/2/config.toml"),
            identity: None,
            running,
            online: false,
            browser: false,
            current,
            startup: false,
            duplicate: false,
            last_login: None,
            avatar: None,
            in_game: false,
            character: None,
            locked: false,
        };
        assert!(refusal(&base(true, false)).is_some());
        assert!(refusal(&base(false, true)).is_some());
        assert!(refusal(&base(false, false)).is_none());
    }

    /// Games, their logs and their reviews go; another account's stay.
    #[test]
    fn an_accounts_games_and_reviews_are_removed_and_others_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (history, id) = testkit::history_with_one_game(dir.path());
        let mut mine = history.get(&id).unwrap().unwrap();
        let events = history.get_events(&id).unwrap().unwrap();
        let reviews = ReviewStore::new(dir.path());
        for (new_id, account) in [("MINE1", 7u64), ("OTHER", 8u64)] {
            mine.id = new_id.into();
            mine.match_info = record_of(new_id, Some(account)).match_info;
            history.append(&mine, &events).unwrap();
            reviews.save(&crate::review::testkit::report_of(&mine)).unwrap();
        }
        let removed = remove_games(&history, &reviews, 7).unwrap();
        assert_eq!(removed, 1);
        let left: Vec<String> = history
            .read_all()
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert!(left.contains(&"OTHER".to_string()) && !left.contains(&"MINE1".to_string()));
        assert!(
            history.get_events("MINE1").unwrap().is_none(),
            "its log is gone"
        );
        assert!(history.get_events("OTHER").unwrap().is_some());
        assert!(reviews.summary("MINE1", "mora-4p").is_none());
        assert!(reviews.summary("OTHER", "mora-4p").is_some());
        assert_eq!(
            remove_games(&history, &reviews, 7).unwrap(),
            0,
            "nothing left"
        );
    }

    /// A folder that is not an account's own is refused before any game goes.
    #[tokio::test]
    async fn a_foreign_folder_is_refused_before_the_games_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (history, id) = testkit::history_with_one_game(dir.path());
        let mut mine = history.get(&id).unwrap().unwrap();
        let events = history.get_events(&id).unwrap().unwrap();
        mine.id = "MINE1".into();
        mine.match_info = record_of("MINE1", Some(7)).match_info;
        history.append(&mine, &events).unwrap();

        let stray = dir.path().join("not-an-account").join("config.toml");
        let target = AccountInfo {
            id: "9".into(),
            config_path: stray,
            identity: Some(crate::accounts::Identity {
                account_id: 7,
                nickname: "n".into(),
            }),
            running: false,
            online: false,
            browser: false,
            current: false,
            startup: false,
            duplicate: false,
            last_login: None,
            avatar: None,
            in_game: false,
            character: None,
            locked: false,
        };
        assert!(remove(&history, &[target], "9", true).await.is_err());
        let left: Vec<String> = history
            .read_all()
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert!(left.contains(&"MINE1".to_string()), "its games stay");
    }
}
