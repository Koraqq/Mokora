use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
};

/// Append-only text log writer, one file per "flow" (e.g. one Majsoul
/// WebSocket connection). Lines are written atomically under a mutex so
/// concurrent direction tasks don't interleave bytes mid-line.
pub struct FlowLogger {
    label: String,
    file: Mutex<File>,
}

impl FlowLogger {
    /// Open `<session_dir>/<subdir>/<file_name>`, creating `subdir` if
    /// needed. Caller supplies the full filename (extension included), so
    /// the same `FlowLogger` works for `.log`, `.mjai.jsonl`, etc.
    /// `label` is purely for diagnostics (used in failure logs).
    pub fn new(
        session_dir: &Path,
        subdir: &str,
        file_name: &str,
        label: impl Into<String>,
    ) -> Result<Self> {
        let dir = session_dir.join(subdir);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create flow log dir {}", dir.display()))?;
        let path = dir.join(file_name);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open flow log {}", path.display()))?;
        Ok(Self {
            label: label.into(),
            file: Mutex::new(file),
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Append `line` followed by `\n`. No formatting beyond the newline.
    pub fn writeln(&self, line: &str) {
        let mut file = self.file.lock().expect("flow log mutex poisoned");
        if let Err(e) = file
            .write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
        {
            tracing::warn!("flow log '{}' write failed: {e}", self.label);
        }
    }
}
