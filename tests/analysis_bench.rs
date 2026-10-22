//! Hand-rolled timing benchmarks for the analysis engine.
//!
//! Run with:
//!     cargo test --release --test analysis_bench -- --ignored --nocapture
//!
//! These are gated `#[ignore]` because they are not correctness tests — they
//! exist solely to estimate the cost of adding the `Improves` layer to
//! `analyze_13` (currently deferred).

use std::time::Instant;

use mokora::analysis::hand::{Counts34, PlayerInfo34Builder};
use mokora::analysis::improves::analyze_13;
use mokora::analysis::search::analyze_14;
use mokora::analysis::shanten;
use mokora::analysis::tile::{Tile34, TILE_COUNT};
use mokora::analysis::waits;

fn one_shanten_13() -> mokora::analysis::PlayerInfo34 {
    PlayerInfo34Builder::new()
        .add_many(&[
            "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m", "1p", "2p", "N", "P",
        ])
        .build()
}

fn one_shanten_14() -> mokora::analysis::PlayerInfo34 {
    PlayerInfo34Builder::new()
        .add_many(&[
            "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m", "1p", "2p", "N", "P", "P",
        ])
        .build()
}

fn report(name: &str, iters: u64, total_ns: u128) {
    let per = total_ns as f64 / iters as f64;
    let micro = per / 1000.0;
    println!("  {name:50}  iters={iters:>8}  per_call={per:>10.0} ns  ({micro:>7.2} µs)");
}

#[test]
#[ignore]
fn bench_shanten_baseline() {
    let info = one_shanten_13();
    let counts: Counts34 = info.hand;
    let len_div3 = info.tehai_len_div3();

    // Warmup
    for _ in 0..1000 {
        let _ = shanten::shanten_from_counts(&counts, len_div3);
    }

    let iters = 1_000_000u64;
    let t0 = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(shanten::shanten_from_counts(&counts, len_div3));
    }
    let dt = t0.elapsed().as_nanos();
    report("shanten_from_counts (1-shanten 13t)", iters, dt);
}

#[test]
#[ignore]
fn bench_waits_enumeration() {
    let info = one_shanten_13();
    let iters = 100_000u64;
    let t0 = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(waits::waits(&info));
    }
    let dt = t0.elapsed().as_nanos();
    report("waits::waits (1-shanten)", iters, dt);
}

#[test]
#[ignore]
fn bench_analyze_13() {
    let info = one_shanten_13();
    let iters = 10_000u64;
    let t0 = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(analyze_13(&info));
    }
    let dt = t0.elapsed().as_nanos();
    report("analyze_13 (current, no improves)", iters, dt);
}

#[test]
#[ignore]
fn bench_analyze_14() {
    let info = one_shanten_14();
    let iters = 1_000u64;
    let t0 = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(analyze_14(&info));
    }
    let dt = t0.elapsed().as_nanos();
    report("analyze_14 (14-state full discard search)", iters, dt);
}

/// Simulates the cost of adding an Improves layer to `analyze_13`. For every
/// non-progressing tile (i.e. tile not in current waits), iterate every
/// possible discard from the resulting 14-state and recompute shanten + waits.
/// Counts the work but discards the result.
fn improves_scan(info: &mokora::analysis::PlayerInfo34) -> u32 {
    let cur_shanten = shanten::shanten(info);
    let len_div3 = info.tehai_len_div3();
    let cur_waits = waits::waits(info);
    let left = info.compute_left_tiles();

    let mut probe = info.hand;
    let mut count = 0u32;

    for drawn in 0..TILE_COUNT {
        // Skip progressing draws (those already in waits) and tiles with no copies left.
        if cur_waits.map.contains_key(&(drawn as u8)) {
            continue;
        }
        if left[drawn] == 0 {
            continue;
        }
        if probe[drawn] >= 4 {
            continue;
        }
        probe[drawn] += 1;
        // For each tile in hand, simulate discarding it and recompute shanten/waits.
        for d in 0..TILE_COUNT {
            if probe[d] == 0 {
                continue;
            }
            probe[d] -= 1;
            let new_shanten = shanten::shanten_from_counts(&probe, len_div3);
            if new_shanten == cur_shanten {
                let new_waits = waits::waits_for_counts(&probe, len_div3, &left, cur_shanten - 1);
                if new_waits.total_left() > cur_waits.total_left() {
                    count += 1;
                }
            }
            probe[d] += 1;
        }
        probe[drawn] -= 1;
    }
    count
}

#[test]
#[ignore]
fn bench_improves_scan() {
    let info = one_shanten_13();
    let iters = 1_000u64;
    let t0 = Instant::now();
    let mut sink: u64 = 0;
    for _ in 0..iters {
        sink = sink.wrapping_add(improves_scan(&info) as u64);
    }
    let dt = t0.elapsed().as_nanos();
    println!("  (sink={sink})");
    report("improves_scan (added cost per analyze_13)", iters, dt);
}

#[test]
#[ignore]
fn bench_summary_estimate() {
    println!();
    println!("=== Summary ===");
    println!("Run individual benches for the underlying numbers; this just");
    println!("aggregates one trial of each into a delta estimate.");

    let info13 = one_shanten_13();
    let info14 = one_shanten_14();

    // Minimal hot timings (small iteration counts to limit total bench time).
    let warmup = 10_000;

    let n_shanten = 1_000_000u64;
    for _ in 0..warmup {
        std::hint::black_box(shanten::shanten_from_counts(
            &info13.hand,
            info13.tehai_len_div3(),
        ));
    }
    let t = Instant::now();
    for _ in 0..n_shanten {
        std::hint::black_box(shanten::shanten_from_counts(
            &info13.hand,
            info13.tehai_len_div3(),
        ));
    }
    let shanten_ns = t.elapsed().as_nanos() as f64 / n_shanten as f64;

    let n13 = 5_000u64;
    let t = Instant::now();
    for _ in 0..n13 {
        std::hint::black_box(analyze_13(&info13));
    }
    let analyze13_ns = t.elapsed().as_nanos() as f64 / n13 as f64;

    let n14 = 500u64;
    let t = Instant::now();
    for _ in 0..n14 {
        std::hint::black_box(analyze_14(&info14));
    }
    let analyze14_ns = t.elapsed().as_nanos() as f64 / n14 as f64;

    let n_imp = 500u64;
    let t = Instant::now();
    let mut s = 0u64;
    for _ in 0..n_imp {
        s = s.wrapping_add(improves_scan(&info13) as u64);
    }
    let improves_ns = t.elapsed().as_nanos() as f64 / n_imp as f64;
    std::hint::black_box(s);

    println!();
    println!(
        "  shanten primitive          : {:>10.0} ns  ({:.3} µs)",
        shanten_ns,
        shanten_ns / 1000.0
    );
    println!(
        "  analyze_13 (current)       : {:>10.0} ns  ({:.3} µs)",
        analyze13_ns,
        analyze13_ns / 1000.0
    );
    println!(
        "  improves_scan (delta cost) : {:>10.0} ns  ({:.3} µs)",
        improves_ns,
        improves_ns / 1000.0
    );
    println!(
        "  analyze_14 (current)       : {:>10.0} ns  ({:.3} µs)",
        analyze14_ns,
        analyze14_ns / 1000.0
    );
    println!();
    println!("  Projected with improves    :");
    let proj_13 = analyze13_ns + improves_ns;
    println!(
        "    analyze_13 (+improves)   : {:>10.0} ns  ({:.3} µs)",
        proj_13,
        proj_13 / 1000.0
    );
    // analyze_14 calls analyze_13 ~14 times (once per discard candidate).
    // Add improves_ns × 14 to analyze_14 as a coarse upper bound.
    let proj_14 = analyze14_ns + improves_ns * 14.0;
    println!(
        "    analyze_14 (+improves)   : {:>10.0} ns  ({:.3} µs)",
        proj_14,
        proj_14 / 1000.0
    );
    println!();
    println!(
        "  Headroom against 50 ms IPC budget per tsumo: {:.0}× margin",
        50_000_000.0 / proj_14
    );
    println!();

    // Avoid unused on debug builds.
    let _ = Tile34::from_mjai("1m");
}
