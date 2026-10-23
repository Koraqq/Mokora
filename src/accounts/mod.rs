//! The accounts Mokora knows, and what is kept on disk for each.
//!
//! One Mokora window plays any number of Mahjong Soul accounts at once. Each
//! open account has its own runtime (game tracker, bot, autoplay, capture) and
//! its own web views — the interface and the game — whose storage holds that
//! account's login; `crate::hub` runs them. This module is the part that does
//! not need a running app: where an account's files are, who it is logged in
//! as, its saved profile, its picture, and making and removing accounts.
//!
//! ```text
//! configs/config.toml          the first account (whatever `load_config(None)` picks)
//! configs/account.json         who it logged in as
//! accounts/2/config.toml       a second account: its own bot, autoplay, browser…
//! accounts/2/logs/
//! accounts/2/webview/          …its own Mahjong Soul login and UI preferences
//! accounts/2/account.json
//! history/                     shared: every account's games, told apart by
//!                              `MatchInfo::Majsoul.account_id`
//! ```
//!
//! Small files beside each config:
//!
//! - `account.json` — `{account_id, nickname}` of the login, written when the
//!   bridge sees it (`LobbyWatch::account`). Names the account in the tabs and
//!   the window title.
//! - `login.json` — `{at_ms}`: when that account's game last came online, so
//!   the picker lists the most recent logins first.
//! - `profile.json` — the player's coins, ranks, today's revive coins and the
//!   game's own statistics (`AccountProfile`), as last seen; the 個人資料 page
//!   reads it with that account offline too.
//! - `avatar.png` — the picture the player chose for the account.
//! - `accounts/startup.json` — `{account}`: the account opened when Mokora
//!   starts. Without it, the first account.

mod remove;

pub use remove::{remove as remove_account, Removed};

use crate::config::AppConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Picker id of the account `load_config(None)` resolves to.
pub const MAIN_ID: &str = "main";
/// Where the other accounts' directories live (`accounts/<n>/`).
pub(crate) const ACCOUNTS_DIR: &str = "./accounts";
const IDENTITY_FILE: &str = "account.json";
const AVATAR_FILE: &str = "avatar.png";
const LOGIN_FILE: &str = "login.json";
const PROFILE_FILE: &str = "profile.json";
/// The startup account's file, in the accounts directory.
const STARTUP_FILE: &str = "startup.json";
/// The interface language last chosen, in the accounts directory: a newly
/// opened account (its own storage starts empty) begins in it.
const UI_FILE: &str = "ui.json";
/// The order of the account chips on the play screen. Kept apart from
/// [`UI_FILE`] so changing the language and dragging a chip from two account
/// views at once cannot overwrite one another.
const ORDER_FILE: &str = "order.json";
/// The game page's layout one account shared with all of them, in the
/// accounts directory (see [`share_layout`]).
const LAYOUT_FILE: &str = "layout.json";
/// The webview's storage directory of an `accounts/<n>/` account.
const WEBVIEW_DIR: &str = "webview";
/// How often the logged-in account is checked for a change.
pub const IDENTITY_POLL: Duration = Duration::from_secs(2);

/// Who an account's browser profile is logged in as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub account_id: u64,
    pub nickname: String,
}

/// One entry of the sidebar's account picker.
#[derive(Debug, Clone, Serialize)]
pub struct AccountInfo {
    /// `"main"` or the account's directory name under `accounts/`.
    pub id: String,
    pub config_path: PathBuf,
    /// `None` until that account's browser has logged in once.
    pub identity: Option<Identity>,
    /// This account is open (its game and runtime are running).
    pub running: bool,
    /// …and its game is connected.
    pub online: bool,
    /// …and its game is open.
    pub browser: bool,
    /// The account of the web view asking (the one the window shows).
    pub current: bool,
    /// The account the window shows when Mokora starts.
    pub startup: bool,
    /// Another account is logged in as the same Mahjong Soul account — the
    /// server keeps only one session, so one of them keeps getting kicked.
    pub duplicate: bool,
    /// When its game last came online (ms since the Unix epoch); `None` if
    /// it never has since this was recorded.
    pub last_login: Option<u64>,
    /// The picture the player chose for this account, as a `data:` URL.
    pub avatar: Option<String>,
    /// A game is being played on it right now (it is in the lobby otherwise).
    pub in_game: bool,
    /// Its player's main character (the id of the icon in
    /// `frontend/public/characters/`), as last seen in the game's traffic.
    pub character: Option<u32>,
    /// It cannot be opened under the license now (see [`may_open`]).
    pub locked: bool,
}

pub(crate) fn mark_duplicates(accounts: &mut [AccountInfo]) {
    let ids: Vec<Option<u64>> = accounts
        .iter()
        .map(|a| a.identity.as_ref().map(|i| i.account_id))
        .collect();
    for (i, a) in accounts.iter_mut().enumerate() {
        a.duplicate = ids[i].is_some()
            && ids
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && *other == ids[i]);
    }
}

/// `accounts/<n>/` directories holding a config, by number.
fn numbered_dirs() -> Vec<(u32, PathBuf)> {
    let root = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR));
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut dirs: Vec<(u32, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.parse::<u32>().ok()?;
            let dir = e.path();
            dir.join("config.toml").is_file().then_some((n, dir))
        })
        .collect();
    dirs.sort_by_key(|(n, _)| *n);
    dirs
}

/// Make a new account: `accounts/<n>/config.toml`, a copy of `template` (the
/// current account's settings, so the bot and autoplay choices carry over)
/// with its own browser profile and log directory. Returns its config path.
pub fn create(template: &AppConfig) -> Result<PathBuf> {
    let next = numbered_dirs().last().map_or(2, |(n, _)| n + 1);
    let rel = format!("{ACCOUNTS_DIR}/{next}");
    let dir = crate::util::resolve_dir(Path::new(&rel));
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let cfg = config_for(template, &rel, &profile_dir_of(&dir));
    let path = dir.join("config.toml");
    let body = toml::to_string_pretty(&cfg).context("serialising the new account's config")?;
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// `template` with the per-account directories pointed under the account's
/// own: logs relative to the app (`rel`), the browser profile at `profile` —
/// absolute, because `capture::chromium::profile::resolve_profile_dir`
/// accepts nothing else (see [`heal_profile_dir`] for when the folder moves).
fn config_for(template: &AppConfig, rel: &str, profile: &str) -> AppConfig {
    let mut cfg = template.clone();
    cfg.capture.chromium.user_data_dir = profile.to_string();
    cfg.logging.dir = PathBuf::from(format!("{rel}/logs"));
    cfg
}

/// The browser profile directory of the account living in `dir`.
fn profile_dir_of(dir: &Path) -> String {
    dir.join("chrome-profile").to_string_lossy().into_owned()
}

/// Point an `accounts/<n>/` config's browser profile at that directory as it
/// is *now*. The path is absolute, so a moved app folder (a fresh download
/// with `accounts/` copied over) would otherwise open a profile at the old
/// place — or none. Only the one key is touched; everything else in the file
/// is kept as it is.
fn heal_profile_dir(config_path: &Path) -> Result<()> {
    heal_profile_dir_in(
        config_path,
        &crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)),
    )
}

fn heal_profile_dir_in(config_path: &Path, accounts_root: &Path) -> Result<()> {
    let Some(dir) = config_path.parent() else {
        return Ok(());
    };
    if !dir
        .parent()
        .is_some_and(|parent| same_path(parent, accounts_root))
    {
        return Ok(()); // the first account keeps the app's default profile
    }
    let raw = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let mut doc: toml_edit::DocumentMut = raw.parse().context("parsing the account's config")?;
    let want = profile_dir_of(dir);
    // Read with `get`: indexing a missing table panics, and a config written
    // by hand may have no `[capture]` at all (the write below creates it).
    let have = doc
        .get("capture")
        .and_then(|c| c.get("chromium"))
        .and_then(|c| c.get("user_data_dir"))
        .and_then(|v| v.as_str());
    if have == Some(want.as_str()) {
        return Ok(());
    }
    doc["capture"]["chromium"]["user_data_dir"] = toml_edit::value(want);
    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("writing {}", config_path.display()))
}

/// The config of account `id`: the first account's, or `accounts/<id>/config.toml`.
pub fn config_path_of(id: &str) -> Option<PathBuf> {
    if id == MAIN_ID {
        return Some(crate::config::default_config_path());
    }
    let path = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR))
        .join(id)
        .join("config.toml");
    path.is_file().then_some(path)
}

/// How many accounts a license without the paid feature may have and open:
/// the first ones of [`known`]. A paid license has no limit; accounts made
/// while paid are kept when it ends, the ones past the limit just do not open.
pub const FREE_ACCOUNTS: usize = 2;

fn within_free(known: &[(String, PathBuf)], id: &str) -> bool {
    known.iter().take(FREE_ACCOUNTS).any(|(k, _)| k == id)
}

/// Whether account `id` may be opened under the license now.
pub fn may_open(id: &str) -> bool {
    crate::license::allows(crate::license::PRO) || within_free(&known(), id)
}

/// Whether an account may be added under the license now.
pub fn may_add() -> bool {
    crate::license::allows(crate::license::PRO) || known().len() < FREE_ACCOUNTS
}

/// Every account that exists: its id and config, the first account first and
/// the added ones by number.
pub fn known() -> Vec<(String, PathBuf)> {
    let mut out = vec![(MAIN_ID.to_string(), crate::config::default_config_path())];
    out.extend(
        numbered_dirs()
            .into_iter()
            .map(|(n, dir)| (n.to_string(), dir.join("config.toml"))),
    );
    out
}

/// The config Mokora opens on: the startup account's, or `None` for the first
/// account's (`load_config(None)`). The account's browser profile is pointed
/// at its folder first.
pub fn startup_config() -> Option<PathBuf> {
    let path = startup_config_in(
        &crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)),
        &startup_account(),
    )?;
    if let Err(e) = heal_profile_dir(&path) {
        // Before the logger: the console is all there is.
        eprintln!(
            "accounts: could not point the startup account's browser profile at its folder: {e:#}"
        );
    }
    Some(path)
}

/// Point an account's config at its folder as it is now (see
/// [`heal_profile_dir`]); done when the account is opened.
pub fn prepare(config_path: &Path) -> Result<()> {
    heal_profile_dir(config_path)
}

fn startup_config_in(accounts_root: &Path, id: &str) -> Option<PathBuf> {
    if id == MAIN_ID {
        return None;
    }
    let path = accounts_root.join(id).join("config.toml");
    path.is_file().then_some(path)
}

/// The startup account's id ([`MAIN_ID`] unless one was picked).
pub fn startup_account() -> String {
    startup_account_in(&crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)))
}

fn startup_account_in(accounts_root: &Path) -> String {
    #[derive(Deserialize)]
    struct Startup {
        account: String,
    }
    std::fs::read(accounts_root.join(STARTUP_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Startup>(&b).ok())
        .map_or_else(|| MAIN_ID.to_string(), |s| s.account)
}

/// Make account `id` the one the window shows when Mokora starts.
pub fn set_startup_account(id: &str) -> Result<()> {
    set_startup_account_in(&crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)), id)
}

fn set_startup_account_in(accounts_root: &Path, id: &str) -> Result<()> {
    std::fs::create_dir_all(accounts_root)
        .with_context(|| format!("creating {}", accounts_root.display()))?;
    let path = accounts_root.join(STARTUP_FILE);
    let body = serde_json::to_vec_pretty(&serde_json::json!({ "account": id }))?;
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))
}

/// Languages the interface has.
const LANGUAGES: [&str; 4] = ["en", "zh-TW", "zh-CN", "ja"];

/// The interface language last chosen in any account.
pub fn ui_language() -> Option<String> {
    #[derive(Deserialize)]
    struct Ui {
        lang: String,
    }
    let bytes =
        std::fs::read(crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)).join(UI_FILE)).ok()?;
    let ui: Ui = serde_json::from_slice(&bytes).ok()?;
    LANGUAGES.contains(&ui.lang.as_str()).then_some(ui.lang)
}

/// Remember `lang` for the accounts opened from now on.
pub fn set_ui_language(lang: &str) -> Result<()> {
    anyhow::ensure!(LANGUAGES.contains(&lang), "unknown language {lang}");
    let root = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR));
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    let path = root.join(UI_FILE);
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({ "lang": lang }))?,
    )
    .with_context(|| format!("writing {}", path.display()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountOrder {
    accounts: Vec<String>,
}

/// Put account information in the order the player chose. IDs absent from an
/// older order file stay at the end in their natural order, so adding an
/// account never hides or displaces an existing one unpredictably.
pub fn apply_order(accounts: &mut [AccountInfo]) {
    apply_order_in(&crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)), accounts);
}

fn apply_order_in(accounts_root: &Path, accounts: &mut [AccountInfo]) {
    let Ok(bytes) = std::fs::read(accounts_root.join(ORDER_FILE)) else {
        return;
    };
    let Ok(saved) = serde_json::from_slice::<AccountOrder>(&bytes) else {
        return;
    };
    let positions: std::collections::HashMap<&str, usize> = saved
        .accounts
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    accounts.sort_by_key(|account| {
        positions
            .get(account.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
}

/// Remember the account-chip order. A stale interface may mention an account
/// just deleted or omit one just added: unknown IDs are ignored and missing
/// known IDs are appended, rather than rejecting the whole drag.
pub fn set_order(ids: &[String]) -> Result<()> {
    set_order_in(
        &crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)),
        ids,
        &known().into_iter().map(|(id, _)| id).collect::<Vec<_>>(),
    )
}

fn set_order_in(accounts_root: &Path, ids: &[String], known: &[String]) -> Result<()> {
    let known_set: std::collections::HashSet<&str> = known.iter().map(String::as_str).collect();
    let mut seen = std::collections::HashSet::new();
    let mut accounts: Vec<String> = ids
        .iter()
        .filter(|id| known_set.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    accounts.extend(known.iter().filter(|id| seen.insert(id.as_str())).cloned());

    std::fs::create_dir_all(accounts_root)
        .with_context(|| format!("creating {}", accounts_root.display()))?;
    let path = accounts_root.join(ORDER_FILE);
    crate::util::write_atomic(
        &path,
        serde_json::to_vec_pretty(&AccountOrder { accounts })?,
    )
    .with_context(|| format!("writing {}", path.display()))
}

/// A game page layout shared with every account: when it was shared (ms since
/// the epoch, so each Mokora applies it once) and the layout itself, which
/// only the frontend reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedLayout {
    pub stamp_ms: u64,
    pub layout: serde_json::Value,
}

/// Share `layout` with every account; returns its stamp.
pub fn share_layout(layout: serde_json::Value) -> Result<u64> {
    share_layout_in(&crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)), layout)
}

fn share_layout_in(accounts_root: &Path, layout: serde_json::Value) -> Result<u64> {
    std::fs::create_dir_all(accounts_root)
        .with_context(|| format!("creating {}", accounts_root.display()))?;
    let stamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let path = accounts_root.join(LAYOUT_FILE);
    // Aside and renamed over, so another account never reads it half-written.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec(&SharedLayout { stamp_ms, layout })?,
    )
    .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(stamp_ms)
}

/// The layout last shared with every account, if one was.
pub fn shared_layout() -> Option<SharedLayout> {
    shared_layout_in(&crate::util::resolve_dir(Path::new(ACCOUNTS_DIR)))
}

fn shared_layout_in(accounts_root: &Path) -> Option<SharedLayout> {
    let bytes = std::fs::read(accounts_root.join(LAYOUT_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Where `config_path`'s account keeps its webview storage — its UI
/// preferences (`localStorage`): `accounts/<n>/webview/`. `None` for the
/// first account, which keeps the app's default, so nothing it had is lost.
pub fn webview_dir(config_path: &Path) -> Option<PathBuf> {
    let dir = config_path.parent()?;
    let root = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR));
    dir.parent()
        .is_some_and(|parent| same_path(parent, &root))
        .then(|| dir.join(WEBVIEW_DIR))
}

/// The picture the player chose for `config_path`'s account, as a `data:` URL.
pub fn read_avatar(config_path: &Path) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(sibling(config_path, AVATAR_FILE)).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Keep `png` as `config_path`'s avatar. The interface sends a small square;
/// anything that is not a PNG, or is large, is refused.
pub fn set_avatar(config_path: &Path, png: &[u8]) -> Result<()> {
    const MAX: usize = 512 * 1024;
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png.starts_with(SIGNATURE) {
        anyhow::bail!("the avatar is not a PNG image");
    }
    if png.len() > MAX {
        anyhow::bail!("the avatar is larger than {} KB", MAX / 1024);
    }
    std::fs::write(sibling(config_path, AVATAR_FILE), png).context("save the avatar")
}

/// Go back to the initial letter.
pub fn clear_avatar(config_path: &Path) -> Result<()> {
    match std::fs::remove_file(sibling(config_path, AVATAR_FILE)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("remove the avatar"),
    }
}

/// Who `config_path`'s account logged in as, if it has.
pub fn read_identity(config_path: &Path) -> Option<Identity> {
    let bytes = std::fs::read(sibling(config_path, IDENTITY_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Store who `config_path`'s account is logged in as. Returns whether that
/// changed.
pub fn note_identity(config_path: &Path, identity: &Identity) -> Result<bool> {
    if read_identity(config_path).as_ref() == Some(identity) {
        return Ok(false);
    }
    let path = sibling(config_path, IDENTITY_FILE);
    std::fs::write(&path, serde_json::to_vec_pretty(identity)?)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

#[derive(Debug, Serialize, Deserialize)]
struct Login {
    at_ms: u64,
}

/// Note that `config_path`'s game has just come online.
pub fn note_login(config_path: &Path) -> Result<()> {
    let at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let path = sibling(config_path, LOGIN_FILE);
    std::fs::write(&path, serde_json::to_vec(&Login { at_ms })?)
        .with_context(|| format!("writing {}", path.display()))
}

/// When `config_path`'s game last came online, if it has been recorded.
pub fn read_login(config_path: &Path) -> Option<u64> {
    let bytes = std::fs::read(sibling(config_path, LOGIN_FILE)).ok()?;
    serde_json::from_slice::<Login>(&bytes)
        .ok()
        .map(|l| l.at_ms)
}

/// The profile `config_path`'s account last showed (see `profile.json`).
pub fn read_profile(config_path: &Path) -> Option<crate::schema::AccountProfile> {
    let bytes = std::fs::read(sibling(config_path, PROFILE_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn write_profile(
    config_path: &Path,
    profile: &crate::schema::AccountProfile,
) -> Result<()> {
    let path = sibling(config_path, PROFILE_FILE);
    std::fs::write(&path, serde_json::to_vec_pretty(profile)?)
        .with_context(|| format!("writing {}", path.display()))
}

/// What this session has seen over what was saved: a session that has not
/// seen something yet (the statistics come only when the player opens their
/// profile in the game) keeps the saved value. A saved profile of another
/// login is not carried over.
pub fn merge_profile(
    saved: Option<crate::schema::AccountProfile>,
    live: crate::schema::AccountProfile,
) -> crate::schema::AccountProfile {
    let Some(saved) = saved else { return live };
    if live.account_id.is_some()
        && saved.account_id.is_some()
        && live.account_id != saved.account_id
    {
        return live;
    }
    let (stats, stats_at_ms) = if live.stats.is_empty() {
        (saved.stats, saved.stats_at_ms)
    } else {
        (live.stats, live.stats_at_ms)
    };
    crate::schema::AccountProfile {
        account_id: live.account_id.or(saved.account_id),
        nickname: live.nickname.or(saved.nickname),
        main_character: live.main_character.or(saved.main_character),
        gold: live.gold.or(saved.gold),
        diamond: live.diamond.or(saved.diamond),
        level4: live.level4.or(saved.level4),
        level3: live.level3.or(saved.level3),
        // Only today's answer means anything.
        revive_claimed: live.revive_claimed,
        stats,
        stats_at_ms,
        achievements: live.achievements.or(saved.achievements),
        // Each list the game sent this session over the saved ones: a ladder
        // not shown since is kept.
        recent_games: crate::autoplay::lobby::merge_recent(live.recent_games, saved.recent_games),
        updated_ms: if live.account_id.is_some() {
            live.updated_ms
        } else {
            saved.updated_ms
        },
    }
}

fn sibling(config_path: &Path, name: &str) -> PathBuf {
    config_path
        .parent()
        .map_or_else(|| PathBuf::from(name), |dir| dir.join(name))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_avatar_is_kept_per_account_and_only_when_it_is_a_small_png() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        assert_eq!(read_avatar(&config), None);

        let png: Vec<u8> = [&b"\x89PNG\r\n\x1a\n"[..], b"rest"].concat();
        set_avatar(&config, &png).unwrap();
        assert!(read_avatar(&config)
            .unwrap()
            .starts_with("data:image/png;base64,"));

        assert!(set_avatar(&config, b"GIF89a").is_err(), "not a PNG");
        let mut big = png.clone();
        big.resize(600 * 1024, 0);
        assert!(set_avatar(&config, &big).is_err(), "too large");

        clear_avatar(&config).unwrap();
        assert_eq!(read_avatar(&config), None);
        clear_avatar(&config).unwrap();
    }

    use super::*;

    fn info(id: &str, account: Option<u64>) -> AccountInfo {
        AccountInfo {
            id: id.to_string(),
            config_path: PathBuf::from(format!("{id}/config.toml")),
            identity: account.map(|account_id| Identity {
                account_id,
                nickname: format!("n{account_id}"),
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
        }
    }

    #[test]
    fn without_the_paid_feature_only_the_first_two_accounts_open() {
        let known: Vec<(String, PathBuf)> = ["main", "2", "3", "5"]
            .iter()
            .map(|id| (id.to_string(), PathBuf::new()))
            .collect();
        assert!(within_free(&known, "main"));
        assert!(within_free(&known, "2"));
        assert!(!within_free(&known, "3"));
        assert!(!within_free(&known, "5"));
        assert!(!within_free(&known, "9"));
    }

    #[test]
    fn a_second_account_gets_its_own_profile_and_logs() {
        let cfg = config_for(
            &AppConfig::default(),
            "./accounts/2",
            "C:/mokora/accounts/2/chrome-profile",
        );
        assert_eq!(
            cfg.capture.chromium.user_data_dir,
            "C:/mokora/accounts/2/chrome-profile"
        );
        assert_eq!(cfg.logging.dir, PathBuf::from("./accounts/2/logs"));
        // Everything else is the template's.
        assert_eq!(cfg.bot.active_3p, AppConfig::default().bot.active_3p);
    }

    /// Only `accounts/<n>/` accounts get a webview directory of their own.
    #[test]
    fn only_added_accounts_get_their_own_webview_storage() {
        let root = crate::util::resolve_dir(Path::new(ACCOUNTS_DIR));
        let config = root.join("7").join("config.toml");
        assert_eq!(webview_dir(&config), Some(root.join("7").join(WEBVIEW_DIR)));
        let elsewhere = tempfile::tempdir().unwrap();
        assert_eq!(webview_dir(&elsewhere.path().join("config.toml")), None);
    }

    /// A moved app folder (or a config from before profiles were absolute)
    /// is pointed back at the account's own profile; the first account and
    /// every other key are left alone.
    #[test]
    fn an_accounts_profile_follows_its_folder() {
        let root = tempfile::tempdir().unwrap();
        let accounts = root.path().join("accounts");
        let dir = accounts.join("2");
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            "# kept
[capture.chromium]
user_data_dir = \"./accounts/2/chrome-profile\"
start_url = \"x\"
",
        )
        .unwrap();
        heal_profile_dir_in(&config, &accounts).unwrap();
        let healed = std::fs::read_to_string(&config).unwrap();
        let doc: toml_edit::DocumentMut = healed.parse().unwrap();
        assert_eq!(
            doc["capture"]["chromium"]["user_data_dir"].as_str(),
            Some(profile_dir_of(&dir).as_str())
        );
        assert_eq!(doc["capture"]["chromium"]["start_url"].as_str(), Some("x"));
        assert!(healed.contains("# kept"));

        // Not under `accounts/`: untouched.
        let main = root.path().join("configs");
        std::fs::create_dir_all(&main).unwrap();
        let main_config = main.join("config.toml");
        std::fs::write(
            &main_config,
            "[capture.chromium]
user_data_dir = \"\"
",
        )
        .unwrap();
        heal_profile_dir_in(&main_config, &accounts).unwrap();
        assert!(std::fs::read_to_string(&main_config)
            .unwrap()
            .contains("user_data_dir = \"\""));
        // A config without `[capture]` (written by hand) gets one, not a panic.
        let bare = accounts.join("3");
        std::fs::create_dir_all(&bare).unwrap();
        let bare_config = bare.join("config.toml");
        std::fs::write(
            &bare_config,
            "[general]
first_run_completed = true
",
        )
        .unwrap();
        heal_profile_dir_in(&bare_config, &accounts).unwrap();
        let doc: toml_edit::DocumentMut = std::fs::read_to_string(&bare_config)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            doc["capture"]["chromium"]["user_data_dir"].as_str(),
            Some(profile_dir_of(&bare).as_str())
        );
    }

    #[test]
    fn a_shared_layout_reads_back_with_its_stamp() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(shared_layout_in(dir.path()), None);
        let layout = serde_json::json!({ "layouts3p": { "lg": [] } });
        let stamp = share_layout_in(dir.path(), layout.clone()).unwrap();
        let back = shared_layout_in(dir.path()).unwrap();
        assert_eq!((back.stamp_ms, back.layout), (stamp, layout));
        assert!(!dir.path().join("layout.json.tmp").exists());
    }

    #[test]
    fn the_same_login_twice_is_flagged_on_both() {
        let mut accounts = vec![
            info("main", Some(7)),
            info("2", Some(7)),
            info("3", Some(9)),
            info("4", None),
        ];
        mark_duplicates(&mut accounts);
        let flags: Vec<bool> = accounts.iter().map(|a| a.duplicate).collect();
        assert_eq!(flags, [true, true, false, false]);
    }

    #[test]
    fn the_saved_chip_order_is_shared_and_new_accounts_follow_it() {
        let dir = tempfile::tempdir().unwrap();
        let known = vec!["main".to_string(), "2".to_string(), "3".to_string()];
        set_order_in(
            dir.path(),
            &["3".to_string(), "main".to_string(), "missing".to_string()],
            &known,
        )
        .unwrap();

        let mut accounts = vec![info("main", None), info("2", None), info("3", None)];
        apply_order_in(dir.path(), &mut accounts);
        assert_eq!(
            accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["3", "main", "2"]
        );
        assert!(!dir.path().join("order.json.tmp").exists());
    }

    #[test]
    fn a_bad_or_old_chip_order_never_loses_an_account() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ORDER_FILE), b"not json").unwrap();
        let mut accounts = vec![info("main", None), info("2", None)];
        apply_order_in(dir.path(), &mut accounts);
        assert_eq!(
            accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["main", "2"]
        );

        let known = vec!["main".to_string(), "2".to_string()];
        set_order_in(
            dir.path(),
            &["2".to_string(), "2".to_string(), "gone".to_string()],
            &known,
        )
        .unwrap();
        apply_order_in(dir.path(), &mut accounts);
        assert_eq!(
            accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["2", "main"]
        );
    }

    #[test]
    fn identity_and_login_files_live_beside_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let id = Identity {
            account_id: 12315690,
            nickname: "K_o".to_string(),
        };
        assert_eq!(read_identity(&config), None);
        assert!(note_identity(&config, &id).unwrap());
        assert!(
            !note_identity(&config, &id).unwrap(),
            "unchanged is not rewritten"
        );
        assert_eq!(read_identity(&config), Some(id));

        assert_eq!(read_login(&config), None);
        note_login(&config).unwrap();
        assert!(read_login(&config).is_some_and(|at| at > 0));
    }

    /// Mokora opens on the starred account's config, the first account's when
    /// that is the star (or the starred one is gone).
    #[test]
    fn mokora_opens_on_the_startup_accounts_config() {
        let root = tempfile::tempdir().unwrap();
        let three = root.path().join("3");
        std::fs::create_dir_all(&three).unwrap();
        std::fs::write(three.join("config.toml"), "").unwrap();
        assert_eq!(
            startup_config_in(root.path(), "3"),
            Some(three.join("config.toml"))
        );
        assert_eq!(startup_config_in(root.path(), MAIN_ID), None);
        assert_eq!(
            startup_config_in(root.path(), "7"),
            None,
            "a starred account since removed"
        );
    }

    /// A session keeps what it has not seen from the saved profile, and a
    /// saved profile of another login is dropped.
    #[test]
    fn a_profile_keeps_what_the_session_has_not_seen() {
        use crate::schema::{AccountProfile, MajsoulRankStats};
        let saved = AccountProfile {
            account_id: Some(1),
            gold: Some(100),
            revive_claimed: Some(true),
            stats: vec![MajsoulRankStats {
                players: 3,
                ..Default::default()
            }],
            stats_at_ms: Some(5),
            ..Default::default()
        };
        let live = AccountProfile {
            account_id: Some(1),
            gold: Some(6926),
            ..Default::default()
        };
        let merged = merge_profile(Some(saved.clone()), live);
        assert_eq!(merged.gold, Some(6926));
        assert_eq!(merged.stats.len(), 1, "statistics kept");
        assert_eq!(merged.revive_claimed, None, "only today's answer counts");
        let other = AccountProfile {
            account_id: Some(2),
            ..Default::default()
        };
        assert!(merge_profile(Some(saved), other).stats.is_empty());
    }

    #[test]
    fn the_startup_account_defaults_to_the_first() {
        let root = tempfile::tempdir().unwrap();
        let accounts = root.path().join("accounts");
        assert_eq!(startup_account_in(&accounts), MAIN_ID);
        set_startup_account_in(&accounts, "2").unwrap();
        assert_eq!(startup_account_in(&accounts), "2");
        set_startup_account_in(&accounts, MAIN_ID).unwrap();
        assert_eq!(startup_account_in(&accounts), MAIN_ID);
    }
}
