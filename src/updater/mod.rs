//! Updates: is a newer release out, has the user asked not to be reminded,
//! and installing it.
//!
//! - [`check`] compares the running version with the latest GitHub release
//!   (directly or via accelerator mirrors, per the `[network]` config).
//! - [`reminders`] remembers "don't remind me again" until a newer version
//!   is installed.
//! - [`install`] downloads the release's package for this system, checks it
//!   against the published digest and starts installing it; Mokora then
//!   closes. When it cannot (untrusted metadata, no package), the prompt
//!   sends the user to the release page instead.
//!
//! The Tauri commands `check_for_update`, `install_update` and
//! `mute_update_reminders` live in `src/ipc/commands.rs`.

pub mod check;
pub mod install;
pub mod reminders;

pub use check::{check_for_update, UpdateInfo};
