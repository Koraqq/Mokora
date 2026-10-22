//! The saved reviews, under `<history>/reviews/`: a full report and a small
//! summary per game and model. A review is only current while its key matches
//! what a review made now would carry; otherwise it is as good as absent and
//! is made again.

use crate::schema::review::{ReviewReport, ReviewRow, ReviewSummary};
use crate::schema::GameRecord;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const DIR: &str = "reviews";

pub struct ReviewStore {
    dir: PathBuf,
}

impl ReviewStore {
    /// The store under the history root `history_root`.
    pub fn new(history_root: &Path) -> Self {
        Self {
            dir: history_root.join(DIR),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn report_path(&self, id: &str, model: &str) -> PathBuf {
        self.dir.join(format!("{id}.{model}.json"))
    }

    fn summary_path(&self, id: &str, model: &str) -> PathBuf {
        self.dir.join(format!("{id}.{model}.sum.json"))
    }

    /// The small summary of a saved review, if there is one.
    pub fn summary(&self, id: &str, model: &str) -> Option<ReviewSummary> {
        let bytes = std::fs::read(self.summary_path(id, model)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// The saved report, if there is one.
    pub fn report(&self, id: &str, model: &str) -> Option<ReviewReport> {
        let bytes = std::fs::read(self.report_path(id, model)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Save a report and its summary. The report is written first, so a
    /// summary never points at a report that is not there.
    pub fn save(&self, report: &ReviewReport) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        write_atomic(
            &self.report_path(&report.game_id, &report.model.name),
            &serde_json::to_vec(report)?,
        )?;
        write_atomic(
            &self.summary_path(&report.game_id, &report.model.name),
            &serde_json::to_vec(&summary_of(report))?,
        )
    }

    /// Remove every saved review of game `id`, whatever the model.
    pub fn delete_game(&self, id: &str) {
        let prefix = format!("{id}.");
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    /// The page's row for `record`, reviewed (or not) with `model`.
    pub fn row(&self, record: &GameRecord, model: &str) -> ReviewRow {
        let summary = self.summary(&record.id, model);
        let current = super::current_key(record, model);
        let stale = match (&summary, &current) {
            (Some(s), Some(k)) => &s.key != k,
            (Some(_), None) => true,
            _ => false,
        };
        ReviewRow {
            id: record.id.clone(),
            model: model.to_string(),
            reviewed: summary.is_some() && !stale,
            stale,
            summary: summary.filter(|_| !stale),
        }
    }

    /// Whether `record` already has a current review under `model`.
    pub fn is_current(&self, record: &GameRecord, model: &str) -> bool {
        self.row(record, model).reviewed
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::util::write_atomic(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// The summary of `report`.
pub fn summary_of(report: &ReviewReport) -> ReviewSummary {
    ReviewSummary {
        key: report.key.clone(),
        model: report.model.clone(),
        created_ms: report.created_ms,
        decisions: report.decisions.len() as u32,
        scored: report.kinds.iter().map(|k| k.scored).sum(),
        matched: report.kinds.iter().map(|k| k.matched).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::review::{ReviewDiagnostics, ReviewModelInfo, ReviewSettings};

    fn report(key: &str) -> ReviewReport {
        ReviewReport {
            key: key.to_string(),
            game_id: "G1".into(),
            game_hash: "h".into(),
            created_ms: 1,
            model: ReviewModelInfo {
                name: "mora-3p".into(),
                num_players: 3,
                weights_hash: "w".into(),
            },
            live_bot: None,
            our_seat: 0,
            num_players: 3,
            settings: ReviewSettings {
                version: 1,
                top_k: 3,
                game_length: "hanchan".into(),
                deterministic: true,
            },
            decisions: vec![],
            kinds: vec![],
            diagnostics: ReviewDiagnostics::default(),
        }
    }

    /// A saved review comes back as it went in, with its summary beside it.
    #[test]
    fn a_saved_review_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new(dir.path());
        assert!(store.summary("G1", "mora-3p").is_none());
        let r = report("k");
        store.save(&r).unwrap();
        assert_eq!(store.report("G1", "mora-3p"), Some(r.clone()));
        assert_eq!(store.summary("G1", "mora-3p").unwrap().key, "k");
        // Another model's review of the same game is a different file.
        assert!(store.summary("G1", "mokora-3p").is_none());
    }
}
