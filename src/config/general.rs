use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Set to `true` once the first-run setup wizard has finished.
    /// Existing pre-wizard configs default to `true` via migration so
    /// upgraded users don't see the wizard.
    pub first_run_completed: bool,
    /// The window's × (and a minimise-to-tray) keeps Mokora running in the
    /// background instead of ending it; the tray icon brings the window back,
    /// and only the power button (or the tray's quit) ends it. Off by default:
    /// × ends Mokora.
    pub keep_running_on_close: bool,
    /// While the window is minimised or hidden and no game is being played,
    /// the game pages are put to rest to save power. A game in progress is
    /// never slowed, whatever the window is doing. Off by default.
    pub idle_power_saving: bool,
}
