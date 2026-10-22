//! AI bot integration.
//!
//! Wires `MjaiEvent`s from the platform bridge to AI bots speaking the
//! [mjai JSONL protocol](https://gimite.net/pukiwiki/index.php?Mjai%20%E9%BA%BB%E9%9B%80AI%E5%AF%BE%E6%88%A6%E3%82%B5%E3%83%BC%E3%83%90).
//! Each bot runs in its own subprocess (Python, by default) so AGPL-licensed
//! bots like Mortal stay legally separated from Mokora's binary.
//!
//! See `README.md` in this directory for the contributor-facing how-to.

pub mod install;
pub mod manager;
pub mod manifest;
pub mod native;
pub mod registry;
pub mod runner;
pub mod runtime;
pub mod supervisor;
pub mod sync_guard;
pub mod types;

pub use install::{install_from_github_release, GithubInstallSpec};
pub use manager::BotManager;
pub use manifest::{BotSource, FieldKind, FieldSpec, Manifest, ManifestBot};
pub use registry::{BotEntry, BotRegistry};
pub use runner::{BotRunner, SubprocessBot};
pub use runtime::{PythonRuntime, RuntimeMode};
pub use supervisor::run_bot_manager;
pub use sync_guard::SyncGuard;
pub use types::BotResponse;
