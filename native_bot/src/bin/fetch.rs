//! Tenhou Phoenix-room log downloader.
//!
//! Tenhou publishes the ids of every Phoenix-room (鳳凰卓) game in its daily
//! `scc*.html.gz` listings; the logs themselves are only served one game at a
//! time. This walks the listings Tenhou still serves, keeps the games of one
//! player count (`三鳳` for 3-player, `四鳳` for 4-player), and downloads each
//! log as its raw mjlog XML, gzip-compressed, one file per game. The mjlogs are converted to mjai separately (see
//! `train/README.md`), so this tool only ever talks to Tenhou.
//!
//! Re-runnable: a log already on disk is skipped, so an interrupted run picks
//! up where it stopped, and running it again later fetches only new games.
//! Logs that still fail after retries are listed in `failed.txt`.
//!
//! Usage:
//!   fetch <out_dir> [concurrency] [num_players]
//!
//! Examples (the layout `train/README.md` uses):
//!   fetch dataset/tenhou/p3 10
//!   fetch dataset/tenhou/p4 10 4

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

const RAW: &str = "https://tenhou.net/sc/raw";
const LOG: &str = "https://tenhou.net/0/log/?";
/// Listing entries are tagged with the room: `三鳳` / `四鳳` are the 3- and
/// 4-player Phoenix rooms.
fn phoenix_tag(num_players: u8) -> &'static str {
    if num_players == 3 {
        "三鳳"
    } else {
        "四鳳"
    }
}
/// Per-log attempts before it goes to `failed.txt`.
const ATTEMPTS: u32 = 4;
/// Pause each worker takes between requests, on top of the request itself.
const PAUSE: Duration = Duration::from_millis(100);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("mokora-native_bot-fetch")
        .build()
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let resp = agent
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(64 << 20)
        .read_to_end(&mut body)
        .with_context(|| format!("read {url}"))?;
    Ok(body)
}

/// The `scc` listing files Tenhou still serves: the dated archive (`?old`)
/// plus the hourly files of the last days.
fn listing_files(agent: &ureq::Agent) -> Result<Vec<String>> {
    let mut files = BTreeSet::new();
    for url in [format!("{RAW}/list.cgi?old"), format!("{RAW}/list.cgi")] {
        let text = String::from_utf8_lossy(&get_bytes(agent, &url)?).into_owned();
        // Entries look like `{file:'2026/scc20260101.html.gz',size:…}`.
        for part in text.split("file:'").skip(1) {
            let Some(name) = part.split('\'').next() else {
                continue;
            };
            let base = name.rsplit('/').next().unwrap_or(name);
            if base.starts_with("scc") && base.ends_with(".html.gz") {
                files.insert(name.to_string());
            }
        }
    }
    Ok(files.into_iter().collect())
}

/// Every Phoenix log id in one listing file whose room tag is `tag`.
fn ids_in_listing(agent: &ureq::Agent, file: &str, tag: &str) -> Result<Vec<String>> {
    let gz = get_bytes(agent, &format!("{RAW}/dat/{file}"))?;
    let mut html = String::new();
    GzDecoder::new(&gz[..])
        .read_to_string(&mut html)
        .with_context(|| format!("gunzip {file}"))?;
    // `00:04 | 16 | 三鳳南喰赤－ | <a href="http://tenhou.net/0/?log=<id>">牌譜</a> | …`
    Ok(html
        .lines()
        .filter(|l| l.contains(tag))
        .filter_map(|l| l.split("log=").nth(1))
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect())
}

fn log_path(out: &Path, id: &str) -> PathBuf {
    out.join(format!("{id}.mjlog.gz"))
}

/// Download one log and store it gzip-compressed, via a `.part` file so an
/// interrupted write never looks like a finished log.
fn fetch_log(agent: &ureq::Agent, out: &Path, id: &str) -> Result<()> {
    let xml = get_bytes(agent, &format!("{LOG}{id}"))?;
    if !xml.starts_with(b"<mjloggm") {
        bail!("not an mjlog ({} bytes)", xml.len());
    }
    let dst = log_path(out, id);
    let part = dst.with_extension("gz.part");
    {
        let mut gz = GzEncoder::new(File::create(&part)?, Compression::default());
        gz.write_all(&xml)?;
        gz.finish()?;
    }
    fs::rename(&part, &dst)?;
    Ok(())
}

fn hms(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <out_dir> [concurrency] [num_players]", args[0]);
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[1]);
    let workers: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
        .max(1);
    let num_players: u8 = match args.get(3).map(String::as_str) {
        None | Some("3") => 3,
        Some("4") => 4,
        Some(other) => bail!("num_players must be 3 or 4, got {other}"),
    };
    let tag = phoenix_tag(num_players);
    fs::create_dir_all(&out).with_context(|| format!("create {out:?}"))?;
    let agent = agent();

    // 1. Which games exist.
    let files = listing_files(&agent)?;
    println!("fetch: {} Phoenix listings to scan", files.len());
    let mut ids = BTreeSet::new();
    for (i, file) in files.iter().enumerate() {
        match ids_in_listing(&agent, file, tag) {
            Ok(found) => ids.extend(found),
            Err(e) => eprintln!("fetch: skipping listing {file}: {e:#}"),
        }
        print!(
            "\rfetch: scanned {}/{} listings, {} {num_players}p games",
            i + 1,
            files.len(),
            ids.len()
        );
        std::io::stdout().flush().ok();
        thread::sleep(PAUSE);
    }
    println!();
    let mut list = File::create(out.join("ids.txt"))?;
    for id in &ids {
        writeln!(list, "{id}")?;
    }

    // 2. Which of them are still missing.
    let todo: Vec<String> = ids
        .iter()
        .filter(|id| !log_path(&out, id).exists())
        .cloned()
        .collect();
    let have = ids.len() - todo.len();
    println!(
        "fetch: {} games listed, {have} already downloaded, {} to fetch with {workers} workers -> {out:?}",
        ids.len(),
        todo.len()
    );
    if todo.is_empty() {
        return Ok(());
    }

    // 3. Download them.
    let todo = Arc::new(todo);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(Mutex::new(Vec::<String>::new()));
    let start = Instant::now();
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            let (todo, next, done, failed, out, agent) = (
                todo.clone(),
                next.clone(),
                done.clone(),
                failed.clone(),
                out.clone(),
                agent.clone(),
            );
            thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(id) = todo.get(i) else { break };
                let mut result = Ok(());
                for attempt in 0..ATTEMPTS {
                    result = fetch_log(&agent, &out, id);
                    if result.is_ok() {
                        break;
                    }
                    // Back off harder each time — the server may be asking us to.
                    thread::sleep(Duration::from_secs(2u64 << attempt));
                }
                if let Err(e) = result {
                    failed.lock().unwrap().push(format!("{id}\t{e:#}"));
                }
                done.fetch_add(1, Ordering::Relaxed);
                thread::sleep(PAUSE);
            })
        })
        .collect();

    // Progress, until every worker has finished.
    while !handles.iter().all(|h| h.is_finished()) {
        thread::sleep(Duration::from_secs(2));
        let n = done.load(Ordering::Relaxed);
        let secs = start.elapsed().as_secs_f64().max(1e-3);
        let rate = n as f64 / secs;
        let left = (todo.len() - n) as f64 / rate.max(1e-3);
        print!(
            "\rfetch: {n}/{} ({:.1}%)  failed {}  {rate:.1} games/s  elapsed {}  left ~{}   ",
            todo.len(),
            n as f64 * 100.0 / todo.len() as f64,
            failed.lock().unwrap().len(),
            hms(secs as u64),
            hms(left as u64),
        );
        std::io::stdout().flush().ok();
    }
    for h in handles {
        let _ = h.join();
    }
    println!();

    let failed = failed.lock().unwrap();
    let path = out.join("failed.txt");
    if failed.is_empty() {
        let _ = fs::remove_file(&path);
        println!("fetch: done, {} games downloaded", todo.len());
    } else {
        fs::write(&path, failed.join("\n") + "\n")?;
        println!(
            "fetch: done, {} failed (listed in {path:?}); run again to retry them",
            failed.len()
        );
    }
    Ok(())
}
