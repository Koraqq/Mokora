//! End-to-end numeric parity: the candle loader must reproduce the PyTorch
//! (folded) reference logits for a deterministic input. The reference JSON is
//! produced by `train/parity_check.py` next to the shipped weights.
#![cfg(feature = "infer")]

use std::fs;
use std::path::Path;

use native_bot::model::Model;

fn floats(v: &serde_json::Value) -> Vec<f32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect()
}

fn argmax(v: &[f32]) -> usize {
    v.iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0
}

fn check(model_file: &str, parity_file: &str, num_players: u8) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("weights");
    let bytes = fs::read(dir.join(model_file)).expect("weights present");
    let model = Model::from_safetensors(bytes, num_players).expect("load model");

    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join(parity_file)).expect("parity json")).unwrap();
    let input = floats(&json["input"]);
    let reference = floats(&json["logits"]);

    let got = model.forward_logits(&input).expect("forward");
    assert_eq!(got.len(), reference.len(), "logit length mismatch");

    let max_diff = got
        .iter()
        .zip(&reference)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert_eq!(
        argmax(&got),
        json["argmax"].as_u64().unwrap() as usize,
        "candle argmax differs from PyTorch"
    );
    // An absolute floor, plus a share of the logits' own size: the big 4-player
    // models' logits reach ~1e5, where float32 rounding alone is ~0.05 (a
    // relative error of 5e-7), far beyond the floor.
    let scale = reference.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let tolerance = 2e-3 + 1e-6 * scale;
    assert!(
        max_diff < tolerance,
        "candle vs PyTorch logit diff too large: {max_diff} (tolerance {tolerance})"
    );
    eprintln!("{model_file}: candle/PyTorch max|diff| = {max_diff:.2e}, argmax matches");
}

#[test]
fn parity_4p() {
    check("base4p.safetensors", "parity_4p.json", 4);
}

#[test]
fn parity_3p() {
    check("base3p.safetensors", "parity_3p.json", 3);
}

/// Mora reads the V2 planes (66 channels): the loader picks that up from the
/// weights and must still match PyTorch.
#[test]
fn parity_mora3p() {
    check("mora3p.safetensors", "parity_mora3p.json", 3);
}

/// Mora 4p: the V2 planes for four players (75 channels), and a danger head.
#[test]
fn parity_mora4p() {
    check("mora4p.safetensors", "parity_mora4p.json", 4);
}
