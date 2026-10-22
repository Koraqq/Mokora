//! Offline training-data extractor.
//!
//! Reads mjai `.json.gz` game logs, replays each through riichienv-core, and
//! writes behavior-cloning samples `(obs, action, mask)` as compact binary
//! shards for the Python trainer. Observations are stored as `u8`
//! (value in `[0, 4]` scaled to `0..=255`, `obs_scale = 4.0`).
//!
//! Usage:
//!   extract <dataset_dir> <num_players> <out_dir> [max_games] [max_samples] [--features v1|v2|v3] [--danger]
//!
//! `--features` picks the observation planes (`native_bot::obs::Features`,
//! default `v1`). `--danger` also writes each sample's danger labels
//! (`native_bot::danger`: the tiles each opponent would win on) as `N.dgr`,
//! one byte per tile per opponent, for the model's danger head. Progress (games done, samples, ETA) is reported every two
//! seconds on one line.
//!
//! Example (light/fast, 4p):
//!   extract dataset/p4 4 out/p4 20000 4000000

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rayon::prelude::*;

use native_bot::action_codec::action_space;
use native_bot::danger::label_len;
use native_bot::obs::{Features, GameLength};
use native_bot::replay::replay_game_labeled;
use native_bot::tiles::tile_dim;

const OBS_SCALE: f32 = 4.0;

#[inline]
fn quantize(v: f32) -> u8 {
    ((v.clamp(0.0, OBS_SCALE) / OBS_SCALE) * 255.0).round() as u8
}

fn collect_gz(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {dir:?}"))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_gz(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("gz") {
            out.push(path);
        }
    }
    Ok(())
}

fn read_gz(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut raw = Vec::new();
    f.read_to_end(&mut raw)?;
    let mut gz = GzDecoder::new(&raw[..]);
    let mut s = String::new();
    gz.read_to_string(&mut s)?;
    Ok(s)
}

/// One progress line, rewritten in place: games done, samples, ETA.
fn report(games_done: &AtomicUsize, total: &AtomicUsize, n_files: usize, started: Instant) {
    let games = games_done.load(Ordering::Relaxed).min(n_files);
    let samples = total.load(Ordering::Relaxed);
    let elapsed = started.elapsed().as_secs_f64();
    let left = if games > 0 {
        elapsed / games as f64 * (n_files - games) as f64
    } else {
        0.0
    };
    print!(
        "\rextract: {games}/{n_files} games ({:.1}%)  {samples} samples  elapsed {}  left {}   ",
        games as f64 * 100.0 / n_files.max(1) as f64,
        hms(elapsed),
        hms(left),
    );
    let _ = std::io::stdout().flush();
}

fn hms(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    let mut features = Features::V1;
    if let Some(i) = args.iter().position(|a| a == "--features") {
        let name = args.get(i + 1).context("--features needs v1, v2 or v3")?;
        features = Features::parse(name).context("--features: expected v1, v2 or v3")?;
        args.drain(i..i + 2);
    }
    let danger = match args.iter().position(|a| a == "--danger") {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    if args.len() < 4 {
        eprintln!(
            "usage: {} <dataset_dir> <num_players> <out_dir> [max_games] [max_samples] [--features v1|v2|v3] [--danger]",
            args[0]
        );
        std::process::exit(2);
    }
    let dataset_dir = PathBuf::from(&args[1]);
    let num_players: u8 = args[2].parse().context("num_players")?;
    let out_dir = PathBuf::from(&args[3]);
    let max_games: usize = args
        .get(4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let max_samples: usize = args
        .get(5)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    fs::create_dir_all(&out_dir)?;

    let mut files = Vec::new();
    collect_gz(&dataset_dir, &mut files)?;
    files.sort();
    if files.len() > max_games {
        files.truncate(max_games);
    }
    let n_files = files.len();
    println!(
        "extract: {n_files} games, {num_players}p, features {features:?}{}, C={} T={} A={}, cap {max_samples} samples -> {out_dir:?}",
        if danger { " + danger labels" } else { "" },
        features.channels(num_players),
        tile_dim(num_players),
        action_space(num_players),
    );

    let c = features.channels(num_players);
    let t = tile_dim(num_players);
    let a = action_space(num_players);
    let obs_len = c * t;
    let dgr_len = if danger { label_len(num_players) } else { 0 };

    // One shard per rayon chunk; keep shards ~balanced.
    let n_threads = rayon::current_num_threads().max(1);
    let n_shards = (n_threads * 4).min(n_files.max(1));
    let chunk_size = n_files.div_ceil(n_shards.max(1));
    let chunks: Vec<&[PathBuf]> = files.chunks(chunk_size.max(1)).collect();

    let total = AtomicUsize::new(0);
    let games_done = AtomicUsize::new(0);

    let shard = |ci: usize, chunk: &[PathBuf]| -> Result<usize> {
        let create = |ext: &str| -> Result<BufWriter<File>> {
            let path = out_dir.join(format!("{ci}.{ext}"));
            let f = File::create(&path).with_context(|| format!("create {path:?}"))?;
            Ok(BufWriter::new(f))
        };
        let mut obs_w = create("obs")?;
        let mut act_w = create("act")?;
        let mut msk_w = create("msk")?;
        let mut dgr_w = if danger { Some(create("dgr")?) } else { None };
        let mut local = 0usize;

        for path in chunk {
            if total.load(Ordering::Relaxed) >= max_samples {
                break;
            }
            games_done.fetch_add(1, Ordering::Relaxed);
            let content = match read_gz(path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Buffer the game's samples; only commit if replay didn't panic,
            // so a pathological log never writes a torn record.
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let mut g_obs: Vec<u8> = Vec::new();
                let mut g_act: Vec<u8> = Vec::new();
                let mut g_msk: Vec<u8> = Vec::new();
                let mut g_dgr: Vec<u8> = Vec::new();
                let mut g_n = 0usize;
                {
                    let mut emit = |obs: &[f32], act: u16, mask: &[u8], dgr: &[u8]| {
                        g_obs.extend(obs.iter().map(|&v| quantize(v)));
                        g_act.extend_from_slice(&act.to_le_bytes());
                        g_msk.extend_from_slice(mask);
                        g_dgr.extend_from_slice(dgr);
                        g_n += 1;
                    };
                    // Tenhou's room tag in the file name says east-only or
                    // hanchan; a log without one is taken for a hanchan.
                    let length = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .and_then(GameLength::of_tenhou_log)
                        .unwrap_or_default();
                    // Labels are only worked out when they are written.
                    if danger {
                        replay_game_labeled(&content, num_players, features, length, &mut emit);
                    } else {
                        native_bot::replay::replay_game_with(
                            &content,
                            num_players,
                            features,
                            length,
                            &mut |obs, act, mask| emit(obs, act, mask, &[]),
                        );
                    }
                }
                (g_obs, g_act, g_msk, g_dgr, g_n)
            }));

            if let Ok((g_obs, g_act, g_msk, g_dgr, g_n)) = outcome {
                // Sanity: lengths must match declared geometry.
                debug_assert_eq!(g_obs.len(), g_n * obs_len);
                debug_assert_eq!(g_msk.len(), g_n * a);
                debug_assert_eq!(g_dgr.len(), g_n * dgr_len);
                // A write failure (full disk) would silently truncate a shard and
                // desync it from the sample counts in meta.json — fail loudly.
                obs_w.write_all(&g_obs).context("write obs shard")?;
                act_w.write_all(&g_act).context("write act shard")?;
                msk_w.write_all(&g_msk).context("write mask shard")?;
                if let Some(w) = dgr_w.as_mut() {
                    w.write_all(&g_dgr).context("write danger shard")?;
                }
                local += g_n;
                total.fetch_add(g_n, Ordering::Relaxed);
            }
        }

        obs_w.flush().context("flush obs shard")?;
        act_w.flush().context("flush act shard")?;
        msk_w.flush().context("flush mask shard")?;
        if let Some(w) = dgr_w.as_mut() {
            w.flush().context("flush danger shard")?;
        }
        Ok(local)
    };

    // Each game is guarded by `catch_unwind`; silence the default panic printer
    // for the duration of the parallel replay so rare pathological logs don't
    // spam stderr (they're skipped). Scoped: the hook is restored immediately
    // after, and every non-replay failure inside is a `Result`, not a panic — so
    // a read-only out_dir reports an error instead of exiting silently.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let done = AtomicBool::new(false);
    let started = Instant::now();
    let shard_counts: Result<Vec<usize>> = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));
                report(&games_done, &total, n_files, started);
            }
        });
        let counts = chunks
            .par_iter()
            .enumerate()
            .map(|(ci, chunk)| shard(ci, chunk))
            .collect();
        done.store(true, Ordering::Relaxed);
        counts
    });
    std::panic::set_hook(prev_hook);
    report(&games_done, &total, n_files, started);
    println!();
    let shard_counts = shard_counts?;

    let total_samples: usize = shard_counts.iter().sum();
    let meta = serde_json::json!({
        "num_players": num_players,
        "features": format!("{features:?}").to_lowercase(),
        "channels": c,
        "tile_dim": t,
        "action_space": a,
        "obs_len": obs_len,
        "obs_scale": OBS_SCALE,
        "num_shards": chunks.len(),
        "shard_samples": shard_counts,
        "total_samples": total_samples,
        // Bytes of danger labels per sample (0: none written).
        "danger_len": dgr_len,
    });
    fs::write(out_dir.join("meta.json"), serde_json::to_vec_pretty(&meta)?)?;

    println!(
        "extract: wrote {total_samples} samples across {} shards",
        chunks.len()
    );
    Ok(())
}
