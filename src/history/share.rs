//! Sharing one player's recorded games with a friend, by a code.
//!
//! The sender's Mokora takes the player's games, blanks what must not leave
//! the computer (the other players' names, the platform's ids), compresses the
//! lot and seals it with a key made on the spot. The sealed bytes go to the
//! server, which keeps them three days and hands back a short id. The **code**
//! is that id plus the key — the key never reaches the server, so what the
//! server holds is unreadable to it:
//!
//! ```text
//! MK-ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ01-2345-67
//!    └─ id ─────┘ └─ key (128 bits) ──────────────────┘
//! ```
//!
//! The friend's Mokora asks the server for the id, opens the bytes with the
//! key, shows what is in them, and — once confirmed — keeps them under
//! `history/shared/`. From then on they are local: the code may have expired,
//! the data stays. A shared set is **not** the receiver's own history: it is
//! listed apart, counted in none of the receiver's statistics, and cannot be
//! shared again — the share command reads only the receiver's own history
//! store, which a shared set is never put in.
//!
//! Wire format of the sealed bytes: `nonce (12) ‖ ChaCha20-Poly1305(deflate(json))`
//! with the cipher key `SHA-256("mokora-share-v1" ‖ key)`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::schema::{GameRecord, MatchInfo};

/// How long a code works, from the moment it is made (seconds). The server
/// enforces it; the sealed data carries it too, so an old blob is refused even
/// by a server that forgot.
pub const TTL_SECS: i64 = 3 * 24 * 3600;
/// The most games one share holds: the newest of the player's.
pub const MAX_GAMES: usize = 500;
/// What the server is sent at most (bytes): what it keeps in one row (2 MB) once
/// it is base64.
pub const MAX_SEALED: usize = 1024 * 1024;
/// What opening a share may inflate to (bytes): a sealed bomb stops here.
const MAX_OPEN: u64 = 64 * 1024 * 1024;
const FORMAT: u8 = 1;
const DOMAIN: &[u8] = b"mokora-share-v1";

/// What is sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub v: u8,
    /// The player's nickname, as the receiver's player list shows it.
    pub name: String,
    /// A one-way stamp of who this is, so a later share of the same player
    /// replaces the earlier one instead of listing the player twice.
    pub source: String,
    /// When the code stops working (unix seconds).
    pub exp: i64,
    pub made: i64,
    pub records: Vec<GameRecord>,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The stamp [`Payload::source`] carries for a player key such as
/// `majsoul:123456`.
pub fn source_stamp(player_key: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"mokora-share-source");
    h.update(player_key.as_bytes());
    h.finalize()[..10].iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- what leaves

/// One game as it is shared, or `None` for a game that is no one's (observed,
/// no own seat). The other players' names become `#2`…, the platform's ids and
/// the log path are dropped; the result, the room and the statistics stay.
pub fn sanitize(record: &GameRecord) -> Option<GameRecord> {
    let seat = usize::from(record.our_seat?);
    if seat >= record.names.len() {
        return None;
    }
    let mut r = record.clone();
    for (i, name) in r.names.iter_mut().enumerate() {
        if i != seat {
            *name = format!("#{}", i + 1);
        }
    }
    r.log_path = String::new();
    r.match_info = r.match_info.take().map(strip);
    Some(r)
}

fn strip(info: MatchInfo) -> MatchInfo {
    match info {
        MatchInfo::Majsoul {
            mode_id,
            level_id,
            category,
            rules,
            ..
        } => MatchInfo::Majsoul {
            game_uuid: None,
            mode_id,
            room_id: None,
            contest_uid: None,
            account_id: None,
            level_id,
            category,
            rules,
        },
        MatchInfo::Tenhou { go_type, .. } => MatchInfo::Tenhou {
            log_id: None,
            go_type,
            lobby: None,
        },
        other => other,
    }
}

/// The payload for `records` (the player's games, any order): the newest
/// [`MAX_GAMES`] that are someone's, sanitised.
pub fn build(name: &str, source: &str, records: &[GameRecord]) -> Result<Payload> {
    let mut mine: Vec<&GameRecord> = records.iter().collect();
    mine.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let records: Vec<GameRecord> = mine
        .into_iter()
        .filter_map(sanitize)
        .take(MAX_GAMES)
        .collect();
    if records.is_empty() {
        bail!("no_games");
    }
    let name: String = name.trim().chars().take(64).collect();
    if name.is_empty() {
        bail!("no_name");
    }
    let made = now();
    Ok(Payload {
        v: FORMAT,
        name,
        source: source_stamp(source),
        exp: made + TTL_SECS,
        made,
        records,
    })
}

// ---------------------------------------------------------------------- seal

fn cipher(key: &[u8; 16]) -> ChaCha20Poly1305 {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(key);
    let key: [u8; 32] = h.finalize().into();
    ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&key))
}

/// Compress and seal `payload` under a fresh key. Returns the bytes for the
/// server and the key for the code.
pub fn seal(payload: &Payload) -> Result<(Vec<u8>, [u8; 16])> {
    let json = serde_json::to_vec(payload)?;
    let mut z = DeflateEncoder::new(Vec::new(), Compression::default());
    z.write_all(&json)?;
    let packed = z.finish()?;
    let key: [u8; 16] = rand::random();
    let nonce: [u8; 12] = rand::random();
    let sealed = cipher(&key)
        .encrypt(Nonce::from_slice(&nonce), packed.as_slice())
        .map_err(|_| anyhow!("seal failed"))?;
    let mut out = Vec::with_capacity(12 + sealed.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    if out.len() > MAX_SEALED {
        bail!("too_large");
    }
    Ok((out, key))
}

/// Open what [`seal`] made. A wrong key, damaged bytes, a format this build
/// does not know and a share past its time all end here as errors.
pub fn open(bytes: &[u8], key: &[u8; 16]) -> Result<Payload> {
    if bytes.len() < 12 + 16 {
        bail!("bad_data");
    }
    let (nonce, sealed) = bytes.split_at(12);
    let packed = cipher(key)
        .decrypt(Nonce::from_slice(nonce), sealed)
        .map_err(|_| anyhow!("wrong_key"))?;
    let mut json = Vec::new();
    let mut z = DeflateDecoder::new(packed.as_slice()).take(MAX_OPEN + 1);
    z.read_to_end(&mut json).map_err(|_| anyhow!("bad_data"))?;
    if json.len() as u64 > MAX_OPEN {
        bail!("bad_data");
    }
    let payload: Payload = serde_json::from_slice(&json).map_err(|_| anyhow!("bad_data"))?;
    if payload.v != FORMAT {
        bail!("newer_format");
    }
    if payload.exp < now() {
        bail!("expired");
    }
    Ok(clean(payload))
}

/// What a receiver keeps of a payload: only games that are someone's, no more
/// than a share holds, with nothing that leads to a file.
fn clean(mut p: Payload) -> Payload {
    p.records.retain(|r| {
        r.our_seat
            .is_some_and(|s| usize::from(s) < r.names.len() && r.names.len() == usize::from(r.num_players))
    });
    p.records.truncate(MAX_GAMES);
    for r in &mut p.records {
        r.log_path = String::new();
        r.match_info = r.match_info.take().map(strip);
    }
    p.name = p.name.trim().chars().take(64).collect();
    p
}

// ---------------------------------------------------------------------- code

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const ID_LEN: usize = 8;
const KEY_CHARS: usize = 26;

fn b32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn b32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Is `id` one the server could have made: eight characters of the alphabet.
pub fn valid_id(id: &str) -> bool {
    id.len() == ID_LEN && id.bytes().all(|b| ALPHABET.contains(&b))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    pub id: String,
    pub key: [u8; 16],
}

impl Code {
    /// `MK-` and the rest in groups of four.
    pub fn encode(&self) -> String {
        let body = format!("{}{}", self.id, b32_encode(&self.key));
        let groups: Vec<&str> = body
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).expect("ascii"))
            .collect();
        format!("MK-{}", groups.join("-"))
    }

    /// What a person pastes: case, dashes and spaces do not matter, and the
    /// letters the alphabet leaves out are read as the digits they look like.
    pub fn parse(text: &str) -> Option<Code> {
        let clean: String = text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            })
            .collect();
        let body = clean.strip_prefix("MK")?;
        if body.len() != ID_LEN + KEY_CHARS {
            return None;
        }
        let (id, key) = body.split_at(ID_LEN);
        if !valid_id(id) {
            return None;
        }
        let key: [u8; 16] = b32_decode(key)?.get(..16)?.try_into().ok()?;
        Some(Code { id: id.to_string(), key })
    }
}

// --------------------------------------------------------------- made shares

/// A share this Mokora made, kept so its code can be shown again and revoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeShare {
    pub id: String,
    pub code: String,
    pub name: String,
    pub games: usize,
    pub made_at: i64,
    pub expires_at: i64,
    /// What a free account had left to share when this was made (`None` for a
    /// paid one): shown once, with the new code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_left: Option<i64>,
}

fn made_path(root: &Path) -> PathBuf {
    root.join("shares-made.json")
}

/// The shares still running, newest first. Expired ones are dropped from the
/// file as it is read.
pub fn made_list(root: &Path, at: i64) -> Vec<MadeShare> {
    let all: Vec<MadeShare> = std::fs::read(made_path(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut live: Vec<MadeShare> = all.iter().filter(|s| s.expires_at > at).cloned().collect();
    live.sort_by(|a, b| b.made_at.cmp(&a.made_at));
    if live.len() != all.len() {
        let _ = write_made(root, &live);
    }
    live
}

fn write_made(root: &Path, list: &[MadeShare]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    std::fs::write(made_path(root), serde_json::to_vec_pretty(list)?)?;
    Ok(())
}

pub fn made_add(root: &Path, share: MadeShare) -> Result<()> {
    let mut list = made_list(root, 0);
    list.retain(|s| s.id != share.id);
    list.push(share);
    write_made(root, &list)
}

pub fn made_remove(root: &Path, id: &str) -> Result<()> {
    let mut list = made_list(root, 0);
    list.retain(|s| s.id != id);
    write_made(root, &list)
}

// ------------------------------------------------------------------ received

/// What was received and kept: one player's games, as the sender had them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedSet {
    /// The share's id, which also names the file.
    pub id: String,
    pub name: String,
    pub source: String,
    pub imported_at: i64,
    pub records: Vec<GameRecord>,
}

/// What the receiver is shown before it keeps anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub id: String,
    pub name: String,
    pub games: usize,
    pub first: String,
    pub last: String,
    pub expires_at: i64,
    /// The games of the set this one would replace (a share of the same
    /// player that was kept before), if there is one.
    pub replaces: Option<usize>,
}

fn dir(root: &Path) -> PathBuf {
    root.join("shared")
}

pub fn shared_list(root: &Path) -> Vec<SharedSet> {
    let Ok(entries) = std::fs::read_dir(dir(root)) else {
        return Vec::new();
    };
    let mut sets: Vec<SharedSet> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice(&b).ok())
        .collect();
    sets.sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
    sets
}

pub fn preview(root: &Path, id: &str, p: &Payload) -> Preview {
    let times = p.records.iter().map(|r| r.started_at);
    Preview {
        id: id.to_string(),
        name: p.name.clone(),
        games: p.records.len(),
        first: times.clone().min().map(|t| t.to_rfc3339()).unwrap_or_default(),
        last: times.max().map(|t| t.to_rfc3339()).unwrap_or_default(),
        expires_at: p.exp,
        replaces: shared_list(root)
            .iter()
            .find(|s| s.source == p.source)
            .map(|s| s.records.len()),
    }
}

/// Keep `payload` as the set `id`, replacing the earlier set of the same
/// player. Its games get ids of their own so none can be taken for one of the
/// receiver's.
pub fn keep(root: &Path, id: &str, payload: Payload) -> Result<SharedSet> {
    if !valid_id(id) {
        bail!("bad_id");
    }
    for old in shared_list(root) {
        if old.source == payload.source && old.id != id {
            remove(root, &old.id)?;
        }
    }
    let mut records = payload.records;
    for (n, r) in records.iter_mut().enumerate() {
        r.id = format!("shared-{id}-{n:04}");
    }
    let set = SharedSet {
        id: id.to_string(),
        name: payload.name,
        source: payload.source,
        imported_at: now(),
        records,
    };
    std::fs::create_dir_all(dir(root))?;
    std::fs::write(dir(root).join(format!("{id}.json")), serde_json::to_vec(&set)?)
        .context("write shared set")?;
    Ok(set)
}

pub fn remove(root: &Path, id: &str) -> Result<()> {
    if !valid_id(id) {
        bail!("bad_id");
    }
    match std::fs::remove_file(dir(root).join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

// --------------------------------------------------------------- one import

/// The set a lookup found, waiting for the receiver to confirm: there is one
/// at a time, the last looked up.
static PENDING: Mutex<Option<(String, Payload)>> = Mutex::new(None);

pub fn hold(id: &str, payload: Payload) {
    *PENDING.lock().expect("pending lock") = Some((id.to_string(), payload));
}

pub fn take(id: &str) -> Option<Payload> {
    let mut slot = PENDING.lock().expect("pending lock");
    match slot.take() {
        Some((held, p)) if held == id => Some(p),
        other => {
            *slot = other;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{GameStats, KyokuMode, Platform};
    use chrono::{TimeZone, Utc};
    use tempfile::TempDir;

    fn record(id: &str, day: u32, seat: Option<u8>) -> GameRecord {
        GameRecord {
            id: id.to_string(),
            started_at: Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 10, day, 12, 40, 0).unwrap(),
            platform: Platform::Majsoul,
            num_players: 4,
            kyoku_mode: KyokuMode::EastSouth,
            names: vec!["Mine".into(), "Ann".into(), "Bob".into(), "Cy".into()],
            our_seat: seat,
            final_scores: vec![30000, 25000, 25000, 20000],
            final_ranks: vec![1, 2, 3, 4],
            our_rank: seat.map(|_| 1),
            our_delta: seat.map(|_| 5000),
            stats: GameStats::default(),
            match_info: Some(MatchInfo::Majsoul {
                game_uuid: Some("uuid-secret".into()),
                mode_id: Some(12),
                room_id: Some(7),
                contest_uid: None,
                account_id: Some(123_456),
                level_id: Some(10301),
                category: Some(2),
                rules: vec![],
            }),
            bot: Some("mokora".into()),
            round_times: None,
            log_path: format!("games/{id}.mjai.jsonl"),
        }
    }

    #[test]
    fn sanitising_hides_the_others_and_the_platforms_ids() {
        let r = sanitize(&record("a", 1, Some(0))).unwrap();
        assert_eq!(r.names, vec!["Mine", "#2", "#3", "#4"]);
        assert_eq!(r.log_path, "");
        match r.match_info.unwrap() {
            MatchInfo::Majsoul {
                game_uuid,
                account_id,
                room_id,
                mode_id,
                level_id,
                ..
            } => {
                assert!(game_uuid.is_none() && account_id.is_none() && room_id.is_none());
                assert_eq!((mode_id, level_id), (Some(12), Some(10301)));
            }
            _ => panic!("platform changed"),
        }
        // A game seen from no seat is no one's.
        assert!(sanitize(&record("b", 1, None)).is_none());
    }

    #[test]
    fn a_share_holds_the_newest_games_up_to_the_cap() {
        let records: Vec<GameRecord> = (0..MAX_GAMES + 20)
            .map(|i| {
                let mut r = record(&format!("g{i}"), 1, Some(0));
                r.started_at += chrono::Duration::minutes(i as i64);
                r
            })
            .collect();
        let p = build("Mine", "majsoul:1", &records).unwrap();
        assert_eq!(p.records.len(), MAX_GAMES);
        assert_eq!(p.records[0].id, format!("g{}", MAX_GAMES + 19), "newest first");
        assert!(build("Mine", "majsoul:1", &[]).is_err());
        assert!(build("  ", "majsoul:1", &records).is_err());
        assert_eq!(p.exp - p.made, TTL_SECS, "three days");
    }

    #[test]
    fn what_is_sealed_opens_only_with_its_key() {
        let p = build("Mine", "majsoul:1", &[record("a", 1, Some(0))]).unwrap();
        let (bytes, key) = seal(&p).unwrap();
        let back = open(&bytes, &key).unwrap();
        assert_eq!(back.name, "Mine");
        assert_eq!(back.records.len(), 1);
        let mut wrong = key;
        wrong[0] ^= 1;
        assert_eq!(open(&bytes, &wrong).unwrap_err().to_string(), "wrong_key");
        let mut damaged = bytes.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        assert!(open(&damaged, &key).is_err());
        assert_eq!(open(&bytes[..10], &key).unwrap_err().to_string(), "bad_data");
        // Nothing readable is in what the server would hold.
        assert!(!bytes.windows(4).any(|w| w == b"Mine"));
    }

    #[test]
    fn a_share_past_its_time_is_refused() {
        let mut p = build("Mine", "majsoul:1", &[record("a", 1, Some(0))]).unwrap();
        p.exp = now() - 1;
        let (bytes, key) = seal(&p).unwrap();
        assert_eq!(open(&bytes, &key).unwrap_err().to_string(), "expired");
    }

    #[test]
    fn a_receiver_keeps_nothing_that_points_at_a_file() {
        let mut p = build("Mine", "majsoul:1", &[record("a", 1, Some(0))]).unwrap();
        p.records[0].log_path = "../../secret".into();
        if let Some(MatchInfo::Majsoul { account_id, .. }) = p.records[0].match_info.as_mut() {
            *account_id = Some(9);
        }
        let (bytes, key) = seal(&p).unwrap();
        let back = open(&bytes, &key).unwrap();
        assert_eq!(back.records[0].log_path, "");
        match back.records[0].match_info.clone().unwrap() {
            MatchInfo::Majsoul { account_id, .. } => assert!(account_id.is_none()),
            _ => panic!(),
        }
    }

    #[test]
    fn a_code_round_trips_and_forgives_how_it_was_typed() {
        let code = Code { id: "7F3A9K2D".into(), key: rand::random() };
        let text = code.encode();
        assert!(text.starts_with("MK-7F3A-9K2D-"), "{text}");
        assert_eq!(Code::parse(&text), Some(code.clone()));
        assert_eq!(Code::parse(&text.to_lowercase()), Some(code.clone()));
        assert_eq!(Code::parse(&format!("  {}  ", text.replace('-', " "))), Some(code.clone()));
        // `O` and `I` stand for the digits they look like.
        let lookalike = text.replace('0', "O").replace('1', "I");
        assert_eq!(Code::parse(&lookalike), Some(code));
        assert_eq!(Code::parse("MK-1234"), None);
        assert_eq!(Code::parse(&text.replace("MK", "XX")), None);
        assert_eq!(Code::parse(&format!("{text}A")), None);
    }

    #[test]
    fn received_games_are_kept_apart_and_a_later_share_replaces_the_earlier() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let p = build("Friend", "majsoul:5", &[record("a", 1, Some(0)), record("b", 2, Some(0))]).unwrap();
        let pre = preview(root, "AAAAAAAA", &p);
        assert_eq!((pre.games, pre.replaces), (2, None));
        let set = keep(root, "AAAAAAAA", p).unwrap();
        assert_eq!(set.records[0].id, "shared-AAAAAAAA-0000");
        assert_eq!(shared_list(root).len(), 1);

        let again = build("Friend", "majsoul:5", &[record("c", 3, Some(0))]).unwrap();
        assert_eq!(preview(root, "BBBBBBBB", &again).replaces, Some(2));
        keep(root, "BBBBBBBB", again).unwrap();
        let sets = shared_list(root);
        assert_eq!(sets.len(), 1, "the earlier set of the same player went");
        assert_eq!((sets[0].id.as_str(), sets[0].records.len()), ("BBBBBBBB", 1));

        let other = build("Other", "majsoul:6", &[record("d", 4, Some(0))]).unwrap();
        keep(root, "CCCCCCCC", other).unwrap();
        assert_eq!(shared_list(root).len(), 2);
        remove(root, "CCCCCCCC").unwrap();
        remove(root, "CCCCCCCC").unwrap();
        assert_eq!(shared_list(root).len(), 1);
        assert!(remove(root, "../x").is_err());
        assert!(keep(root, "bad", build("X", "k", &[record("e", 5, Some(0))]).unwrap()).is_err());
    }

    #[test]
    fn the_shares_made_are_listed_until_they_run_out() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let share = |id: &str, made_at: i64, expires_at: i64| MadeShare {
            id: id.into(),
            code: format!("MK-{id}"),
            name: "Mine".into(),
            games: 3,
            made_at,
            expires_at,
            free_left: None,
        };
        made_add(root, share("AAAAAAAA", 100, 1_000)).unwrap();
        made_add(root, share("BBBBBBBB", 200, 5_000)).unwrap();
        let ids: Vec<String> = made_list(root, 1_500).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["BBBBBBBB"], "the first has run out");
        made_remove(root, "BBBBBBBB").unwrap();
        assert!(made_list(root, 1_500).is_empty());
    }

    #[test]
    fn the_pending_import_is_the_last_looked_up() {
        let p = build("Friend", "majsoul:5", &[record("a", 1, Some(0))]).unwrap();
        hold("AAAAAAAA", p.clone());
        assert!(take("BBBBBBBB").is_none(), "another id finds nothing");
        assert!(take("AAAAAAAA").is_some());
        assert!(take("AAAAAAAA").is_none(), "taken once");
    }

    /// Against a running server and a signed-in session:
    ///
    /// ```text
    /// MOKORA_LICENSE_SERVER=http://127.0.0.1:8787 MOKORA_E2E_REFRESH=… MOKORA_E2E_MACHINE=… \
    ///   cargo test --lib real_server -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "needs a license server and a signed-in session"]
    async fn a_share_goes_through_a_real_server_and_comes_back() {
        use crate::license::client;
        let refresh = std::env::var("MOKORA_E2E_REFRESH").expect("MOKORA_E2E_REFRESH");
        let machine = std::env::var("MOKORA_E2E_MACHINE").expect("MOKORA_E2E_MACHINE");
        let payload = build(
            "Friend",
            "majsoul:5",
            &[record("a", 1, Some(0)), record("b", 2, Some(0))],
        )
        .unwrap();
        let (bytes, key) = seal(&payload).unwrap();
        let made = client::share_put(&refresh, &machine, &payload.source, &bytes, 2)
            .await
            .unwrap();
        assert!(valid_id(&made.id), "{}", made.id);
        assert!(made.expires_at - now() > TTL_SECS - 60);

        let code = Code::parse(&Code { id: made.id.clone(), key }.encode()).unwrap();
        let fetched = client::share_get(&code.id).await.unwrap();
        assert_eq!(fetched, bytes);
        assert_eq!(open(&fetched, &code.key).unwrap().records.len(), 2);

        client::share_revoke(&refresh, &machine, &made.id).await.unwrap();
        let gone = client::share_get(&made.id).await.unwrap_err();
        assert_eq!(gone.ui_code(), "not_found");
    }
}
