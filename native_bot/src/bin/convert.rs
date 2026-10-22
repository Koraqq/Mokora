//! Tenhou mjlog → mjai converter (3- and 4-player).
//!
//! `fetch` stores Tenhou's own log format: an XML of `<INIT>`, draw/discard
//! tags (`T`/`D` … one letter per seat), `<N>` calls with packed meld codes,
//! `<REACH>`, `<DORA>`, `<AGARI>` and `<RYUUKYOKU>`. The extractor replays
//! mjai, so this rewrites each game as an mjai JSONL stream, gzip-compressed,
//! one file per game — the format `extract` reads.
//!
//! The player count comes from each log's own rules (`<GO type>`). The output
//! is plain mjai with one entry per real seat — 3-element arrays and `kita`
//! for nukidora in sanma — which `mjai_compat::parse_line` accepts unchanged. Every converted game is
//! replayed through the same engine the extractor uses before it is written,
//! so a game the engine would reject never reaches the dataset.
//!
//! Usage:
//!   convert <mjlog_dir> <mjai_dir>
//!
//! Examples (the layout `train/README.md` uses):
//!   convert dataset/tenhou/p3 dataset/p3
//!   convert dataset/tenhou/p4 dataset/p4

use std::fs::{self, File};
use std::io::{Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rayon::prelude::*;
use serde_json::{json, Value};

use native_bot::replay::replay_game;

/// Seats at a table: the most a log can name.
const MAX_SEATS: usize = 4;

/// mjai name of a Tenhou tile id (0..=135). Ids 16 / 52 / 88 are the red
/// fives when the rules use them.
fn tile(id: u32, aka: bool) -> Result<String> {
    if id > 135 {
        bail!("tile id {id} out of range");
    }
    if aka && matches!(id, 16 | 52 | 88) {
        return Ok(format!("5{}r", ['m', 'p', 's'][(id / 36) as usize]));
    }
    let t = id / 4;
    Ok(match t {
        0..=26 => format!("{}{}", t % 9 + 1, ['m', 'p', 's'][(t / 9) as usize]),
        27..=33 => ["E", "S", "W", "N", "P", "F", "C"][(t - 27) as usize].to_string(),
        _ => unreachable!(),
    })
}

/// One `<NAME a="…" …/>` tag.
struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
}

impl<'a> Tag<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn need(&self, key: &str) -> Result<&'a str> {
        self.get(key)
            .ok_or_else(|| anyhow!("<{}> has no {key}", self.name))
    }

    fn num(&self, key: &str) -> Result<u32> {
        self.need(key)?
            .parse()
            .with_context(|| format!("<{}> {key}", self.name))
    }
}

/// Tenhou's mjlog is one flat sequence of self-closing tags under the root.
fn tags(xml: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    for chunk in xml.split('<').skip(1) {
        let body = chunk.split('>').next().unwrap_or("");
        let body = body.trim_end_matches('/').trim();
        if body.is_empty() || body.starts_with('/') {
            continue;
        }
        let (name, mut rest) = body.split_once(' ').unwrap_or((body, ""));
        let mut attrs = Vec::new();
        while let Some((key, after)) = rest.split_once("=\"") {
            let Some((value, tail)) = after.split_once('"') else {
                break;
            };
            attrs.push((key.trim(), value));
            rest = tail;
        }
        out.push(Tag { name, attrs });
    }
    out
}

fn nums(s: &str) -> Result<Vec<i64>> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| p.trim().parse().with_context(|| format!("number {p:?}")))
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Score changes from a Tenhou `sc` (`score,delta` pairs in hundreds), one
/// per real seat.
fn deltas(sc: &str, seats: usize) -> Result<Vec<i64>> {
    Ok(nums(sc)?
        .chunks(2)
        .take(seats)
        .map(|p| p.get(1).copied().unwrap_or(0) * 100)
        .collect())
}

/// Seat of a draw (`T`/`U`/`V`/`W`) or discard (`D`/`E`/`F`/`G`) tag, and
/// the tile id it carries.
fn draw_or_discard(name: &str) -> Option<(bool, usize, u32)> {
    let mut chars = name.chars();
    let c = chars.next()?;
    let id: u32 = chars.as_str().parse().ok()?;
    match c {
        'T' | 'U' | 'V' | 'W' => Some((true, (c as u8 - b'T') as usize, id)),
        'D' | 'E' | 'F' | 'G' => Some((false, (c as u8 - b'D') as usize, id)),
        _ => None,
    }
}

/// Convert one mjlog to mjai events; also returns the player count.
fn convert(xml: &str) -> Result<(Vec<Value>, usize)> {
    let tags = tags(xml);
    let mut aka = true;
    let mut seats = 0;
    let mut out = Vec::new();
    // Per seat: the tile it just drew, for `tsumogiri`. Cleared by a call.
    let mut drawn: [Option<u32>; MAX_SEATS] = [None; MAX_SEATS];
    // The last discard, which a pon / chi / open kan claims.
    let mut last_discard: Option<(usize, u32)> = None;
    let mut started = false;
    let mut in_kyoku = false;
    // A hand's result arrived; its `end_kyoku` waits for the next `<INIT>`
    // (or the end of the log), because a double ron is two `<AGARI>` tags in
    // a row and both belong before it.
    let mut ended = false;

    for tag in &tags {
        match tag.name {
            "GO" => {
                let ty = tag.num("type")?;
                // Bit 0x10 is sanma; bit 0x02 is "no red fives".
                seats = if ty & 0x10 != 0 { 3 } else { 4 };
                aka = ty & 0x02 == 0;
            }
            "UN" if !started => {
                if tag.get("n0").is_none() {
                    continue; // a reconnect notice, not the roster
                }
                if seats == 0 {
                    bail!("roster before the rules (<GO>)");
                }
                let names: Vec<String> = (0..seats)
                    .map(|i| percent_decode(tag.get(&format!("n{i}")).unwrap_or("")))
                    .collect();
                out.push(json!({ "type": "start_game", "names": names }));
                started = true;
            }
            "INIT" => {
                if std::mem::take(&mut ended) {
                    out.push(json!({ "type": "end_kyoku" }));
                }
                let seed = nums(tag.need("seed")?)?;
                if seed.len() < 6 {
                    bail!("short INIT seed");
                }
                let round = seed[0] as usize;
                let bakaze = ["E", "S", "W", "N"]
                    .get(round / 4)
                    .ok_or_else(|| anyhow!("round {round}"))?;
                let scores: Vec<i64> = nums(tag.need("ten")?)?
                    .into_iter()
                    .take(seats)
                    .map(|s| s * 100)
                    .collect();
                let mut tehais = Vec::new();
                for seat in 0..seats {
                    let hand = nums(tag.need(&format!("hai{seat}"))?)?;
                    if hand.len() != 13 {
                        bail!("seat {seat} dealt {} tiles", hand.len());
                    }
                    tehais.push(
                        hand.iter()
                            .map(|&t| tile(t as u32, aka))
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                out.push(json!({
                    "type": "start_kyoku",
                    "bakaze": bakaze,
                    "kyoku": round % 4 + 1,
                    "honba": seed[1],
                    "kyotaku": seed[2],
                    "oya": tag.num("oya")?,
                    "scores": scores,
                    "dora_marker": tile(seed[5] as u32, aka)?,
                    "tehais": tehais,
                }));
                drawn = [None; MAX_SEATS];
                last_discard = None;
                in_kyoku = true;
            }
            "N" => {
                let who = tag.num("who")? as usize;
                let m = tag.num("m")?;
                drawn[who] = None;
                out.push(meld(who, m, last_discard, aka)?);
            }
            "REACH" => {
                let who = tag.num("who")?;
                match tag.num("step")? {
                    1 => out.push(json!({ "type": "reach", "actor": who })),
                    _ => out.push(json!({ "type": "reach_accepted", "actor": who })),
                }
            }
            "DORA" => {
                out.push(json!({ "type": "dora", "dora_marker": tile(tag.num("hai")?, aka)? }));
            }
            "AGARI" => {
                let who = tag.num("who")?;
                let from = tag.num("fromWho")?;
                let mut ev = json!({
                    "type": "hora",
                    "actor": who,
                    "target": from,
                    "pai": tile(tag.num("machi")?, aka)?,
                    "deltas": deltas(tag.need("sc")?, seats)?,
                });
                if let Some(ura) = tag.get("doraHaiUra") {
                    ev["ura_markers"] = Value::from(
                        nums(ura)?
                            .iter()
                            .map(|&t| tile(t as u32, aka))
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                out.push(ev);
                in_kyoku = false;
                ended = true;
            }
            "RYUUKYOKU" => {
                out.push(json!({ "type": "ryukyoku", "deltas": deltas(tag.need("sc")?, seats)? }));
                in_kyoku = false;
                ended = true;
            }
            name => {
                let Some((is_draw, seat, id)) = draw_or_discard(name) else {
                    continue; // SHUFFLE, TAIKYOKU, BYE, …
                };
                if seat >= seats {
                    bail!("seat {seat} in a {seats}-player game");
                }
                if is_draw {
                    drawn[seat] = Some(id);
                    out.push(json!({ "type": "tsumo", "actor": seat, "pai": tile(id, aka)? }));
                } else {
                    let tsumogiri = drawn[seat] == Some(id);
                    drawn[seat] = None;
                    last_discard = Some((seat, id));
                    out.push(json!({
                        "type": "dahai",
                        "actor": seat,
                        "pai": tile(id, aka)?,
                        "tsumogiri": tsumogiri,
                    }));
                }
            }
        }
    }
    if !started {
        bail!("no player roster");
    }
    if in_kyoku {
        bail!("log ends mid-hand");
    }
    if ended {
        out.push(json!({ "type": "end_kyoku" }));
    }
    out.push(json!({ "type": "end_game" }));
    Ok((out, seats))
}

/// Decode a Tenhou meld code (`<N m="…">`).
fn meld(who: usize, m: u32, last_discard: Option<(usize, u32)>, aka: bool) -> Result<Value> {
    let claimed = || {
        last_discard
            .map(|(from, id)| (from, id))
            .ok_or_else(|| anyhow!("call with no discard to claim"))
    };
    if m & 0x4 != 0 {
        // Chi: three consecutive tiles, one of them claimed.
        let t = (m & 0xFC00) >> 10;
        let called = (t % 3) as usize;
        let t = t / 3;
        let base = (t / 7) * 9 + t % 7;
        let ids = [
            base * 4 + ((m >> 3) & 3),
            (base + 1) * 4 + ((m >> 5) & 3),
            (base + 2) * 4 + ((m >> 7) & 3),
        ];
        let (from, id) = claimed()?;
        if ids[called] != id {
            bail!("chi claims {} but the discard was {id}", ids[called]);
        }
        let consumed: Vec<String> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != called)
            .map(|(_, &t)| tile(t, aka))
            .collect::<Result<_>>()?;
        Ok(
            json!({ "type": "chi", "actor": who, "target": from, "pai": tile(id, aka)?, "consumed": consumed }),
        )
    } else if m & 0x18 != 0 {
        // Pon (0x08) or the tile added to one (kakan, 0x10).
        let t = (m & 0xFE00) >> 9;
        let called = (t % 3) as usize;
        let t = t / 3;
        let unused = (m >> 5) & 3;
        let ids: Vec<u32> = (0..4).filter(|&i| i != unused).map(|i| t * 4 + i).collect();
        if m & 0x08 != 0 {
            let (from, id) = claimed()?;
            if ids[called] != id {
                bail!("pon claims {} but the discard was {id}", ids[called]);
            }
            let consumed: Vec<String> = ids
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != called)
                .map(|(_, &t)| tile(t, aka))
                .collect::<Result<_>>()?;
            Ok(
                json!({ "type": "pon", "actor": who, "target": from, "pai": tile(id, aka)?, "consumed": consumed }),
            )
        } else {
            let consumed: Vec<String> = ids.iter().map(|&t| tile(t, aka)).collect::<Result<_>>()?;
            Ok(
                json!({ "type": "kakan", "actor": who, "pai": tile(t * 4 + unused, aka)?, "consumed": consumed }),
            )
        }
    } else if m & 0x20 != 0 {
        // Nukidora: a North set aside (sanma).
        Ok(json!({ "type": "kita", "actor": who, "pai": "N" }))
    } else {
        // Kan: concealed when nobody else's tile is in it.
        let hai = (m & 0xFF00) >> 8;
        let t = hai / 4;
        let ids: Vec<u32> = (0..4).map(|i| t * 4 + i).collect();
        if m & 3 == 0 {
            let consumed: Vec<String> = ids.iter().map(|&t| tile(t, aka)).collect::<Result<_>>()?;
            Ok(json!({ "type": "ankan", "actor": who, "consumed": consumed }))
        } else {
            let (from, id) = claimed()?;
            if id / 4 != t {
                bail!("open kan of {t} but the discard was {id}");
            }
            let consumed: Vec<String> = ids
                .iter()
                .filter(|&&i| i != id)
                .map(|&t| tile(t, aka))
                .collect::<Result<_>>()?;
            Ok(
                json!({ "type": "daiminkan", "actor": who, "target": from, "pai": tile(id, aka)?, "consumed": consumed }),
            )
        }
    }
}

fn read_gz(path: &Path) -> Result<String> {
    let mut s = String::new();
    GzDecoder::new(File::open(path)?).read_to_string(&mut s)?;
    Ok(s)
}

/// Convert, check the engine accepts it, write `<id>.json.gz`.
fn convert_file(src: &Path, dst: &Path) -> Result<usize> {
    let (events, seats) = convert(&read_gz(src)?)?;
    let mut jsonl = String::new();
    for ev in &events {
        jsonl.push_str(&ev.to_string());
        jsonl.push('\n');
    }
    // The same replay the extractor runs: a game it would choke on is
    // reported here rather than silently thinning the training data later.
    let samples = std::panic::catch_unwind(AssertUnwindSafe(|| {
        replay_game(&jsonl, seats as u8, &mut |_, _, _| {})
    }))
    .map_err(|_| anyhow!("replay panicked"))?;
    if samples == 0 {
        bail!("replay produced no samples");
    }
    let part = dst.with_extension("gz.part");
    {
        let mut gz = GzEncoder::new(File::create(&part)?, Compression::default());
        gz.write_all(jsonl.as_bytes())?;
        gz.finish()?;
    }
    fs::rename(&part, dst)?;
    Ok(samples)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <mjlog_dir> <mjai_dir>", args[0]);
        std::process::exit(2);
    }
    let (src_dir, dst_dir) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    fs::create_dir_all(&dst_dir)?;

    let mut jobs = Vec::new();
    for entry in fs::read_dir(&src_dir).with_context(|| format!("read_dir {src_dir:?}"))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".mjlog.gz") else {
            continue;
        };
        let dst = dst_dir.join(format!("{id}.json.gz"));
        if !dst.exists() {
            jobs.push((path, dst));
        }
    }
    println!("convert: {} logs to convert -> {dst_dir:?}", jobs.len());

    let (done, failed, samples) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    let errors: Vec<String> = jobs
        .par_iter()
        .filter_map(|(src, dst)| {
            let r = convert_file(src, dst);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 2000 == 0 {
                println!("convert: {n}/{}", jobs.len());
            }
            match r {
                Ok(s) => {
                    samples.fetch_add(s, Ordering::Relaxed);
                    None
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    Some(format!("{}\t{e:#}", src.display()))
                }
            }
        })
        .collect();

    let path = dst_dir.join("convert_failed.txt");
    if errors.is_empty() {
        let _ = fs::remove_file(&path);
    } else {
        fs::write(&path, errors.join("\n") + "\n")?;
    }
    println!(
        "convert: done, {} converted ({} samples), {} failed{}",
        jobs.len() - failed.load(Ordering::Relaxed),
        samples.load(Ordering::Relaxed),
        failed.load(Ordering::Relaxed),
        if errors.is_empty() {
            String::new()
        } else {
            format!(" (see {path:?})")
        },
    );
    Ok(())
}
