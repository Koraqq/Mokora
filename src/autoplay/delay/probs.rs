//! Normalize bot decision metadata into comparable probabilities.
//!
//! The results are exposed to user delay scripts (`ctx.top_prob` /
//! `ctx.second_prob` / `ctx.margin`); the built-in model and the bundled
//! default script ignore them, because measured bot policies can be
//! nearly flat, which turns confidence thresholds into permanent biases.
//! "How sure was the bot" is wanted as `top` / `second` probabilities in
//! `[0, 1]`, but each bot family reports something different:
//!
//! - The built-in native bot emits HUD metadata
//!   `{ show: { items: [{ label, pais, prob, value }] } }` where `prob`
//!   is a raw policy probability (already softmaxed).
//! - Mortal-style external mjai bots emit `{ q_values: [...] }` — **raw
//!   Q values**, not probabilities. They must be softmaxed before a
//!   threshold like "top > 0.995" means anything.
//!
//! Anything unrecognized normalizes to `None` and the delay model simply
//! skips its probability-based rules. Hard-coding thresholds against raw,
//! bot-specific metadata would silently change meaning per bot — that is
//! exactly what this module exists to prevent.

use serde_json::Value;

/// Top-two candidate probabilities, both in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecisionProbs {
    pub top: f64,
    /// `None` when the bot reported a single candidate (forced move).
    pub second: Option<f64>,
}

impl DecisionProbs {
    /// `top - second`; `None` for a single-candidate decision.
    pub fn margin(&self) -> Option<f64> {
        self.second.map(|s| self.top - s)
    }
}

/// Interpret a bot's `meta` field. Returns `None` when no probability
/// information can be extracted.
pub fn normalize_meta(meta: Option<&Value>) -> Option<DecisionProbs> {
    let meta = meta?;

    // Native-bot HUD metadata: show.items[].prob (raw probabilities).
    if let Some(items) = meta.pointer("/show/items").and_then(Value::as_array) {
        let probs: Vec<f64> = items
            .iter()
            .filter_map(|i| i.get("prob").and_then(Value::as_f64))
            .filter(|p| p.is_finite() && *p >= 0.0)
            .collect();
        if let Some(result) = from_probs(probs) {
            return Some(result);
        }
    }

    // Mortal-style raw Q values: softmax first.
    if let Some(q) = meta.get("q_values").and_then(Value::as_array) {
        let vals: Vec<f64> = q
            .iter()
            .filter_map(Value::as_f64)
            .filter(|v| v.is_finite())
            .collect();
        if !vals.is_empty() {
            return from_probs(softmax(&vals));
        }
    }

    None
}

fn from_probs(mut probs: Vec<f64>) -> Option<DecisionProbs> {
    if probs.is_empty() {
        return None;
    }
    probs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    Some(DecisionProbs {
        top: probs[0],
        second: probs.get(1).copied(),
    })
}

/// Numerically stable softmax.
fn softmax(vals: &[f64]) -> Vec<f64> {
    let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = vals.iter().map(|v| (v - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    exps.into_iter().map(|e| e / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_show_meta_uses_raw_probs() {
        let meta = json!({
            "show": {
                "title": "Mokora",
                "items": [
                    { "label": "Discard", "pais": ["5p"], "prob": 0.62, "value": "62%" },
                    { "label": "Discard", "pais": ["1m"], "prob": 0.30, "value": "30%" },
                    { "label": "Riichi", "prob": 0.08, "value": "8%" },
                ],
            },
        });
        let p = normalize_meta(Some(&meta)).unwrap();
        assert!((p.top - 0.62).abs() < 1e-9);
        assert!((p.second.unwrap() - 0.30).abs() < 1e-9);
        assert!((p.margin().unwrap() - 0.32).abs() < 1e-9);
    }

    /// Raw Q values must be softmaxed, not consumed as probabilities:
    /// equal Qs mean a dead-even decision, whatever their magnitude.
    #[test]
    fn q_values_are_softmaxed() {
        let meta = json!({ "q_values": [5.0, 5.0] });
        let p = normalize_meta(Some(&meta)).unwrap();
        assert!((p.top - 0.5).abs() < 1e-9);
        assert!((p.second.unwrap() - 0.5).abs() < 1e-9);
        assert!(p.margin().unwrap().abs() < 1e-9);

        // A dominant Q value maps to a near-1 probability.
        let meta = json!({ "q_values": [10.0, -10.0, -10.0] });
        let p = normalize_meta(Some(&meta)).unwrap();
        assert!(p.top > 0.995, "dominant Q must normalize near 1");
    }

    #[test]
    fn single_candidate_has_no_margin() {
        let meta = json!({ "q_values": [1.25] });
        let p = normalize_meta(Some(&meta)).unwrap();
        assert!((p.top - 1.0).abs() < 1e-9, "softmax of one value is 1");
        assert!(p.second.is_none());
        assert!(p.margin().is_none());
    }

    #[test]
    fn unknown_meta_is_none() {
        assert!(normalize_meta(None).is_none());
        assert!(normalize_meta(Some(&json!({}))).is_none());
        assert!(normalize_meta(Some(&json!({ "confidence": 0.9 }))).is_none());
        // show.items without raw probs (old format) — no fabricated numbers.
        let old = json!({ "show": { "items": [{ "label": "Discard", "value": "62%" }] } });
        assert!(normalize_meta(Some(&old)).is_none());
    }
}
