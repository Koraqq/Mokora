use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub dir: PathBuf,
    pub level: String,
    pub all_level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("./logs"),
            level: "info".to_string(),
            all_level: "info".to_string(),
        }
    }
}
