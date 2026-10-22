//! Which account a command is for.
//!
//! Every open account has its own interface (`ui-<id>`), so the web view that
//! calls a command says whose it is: [`Acct`] is that account's state. A caller
//! that belongs to no account (the overlay window) gets the shown account's.

use super::Hub;
use crate::ipc::AppState;
use std::ops::Deref;
use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{Manager, Runtime};

/// The state of the account whose interface called the command.
pub struct Acct(AppState);

impl Acct {
    pub fn inner(&self) -> &AppState {
        &self.0
    }
}

impl Deref for Acct {
    type Target = AppState;

    fn deref(&self) -> &AppState {
        &self.0
    }
}

impl<'de, R: Runtime> CommandArg<'de, R> for Acct {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let webview = command.message.webview();
        let hub = webview
            .app_handle()
            .try_state::<Hub>()
            .ok_or_else(|| InvokeError::from("the accounts are not set up yet"))?;
        hub.for_label(webview.label())
            .or_else(|| hub.active())
            .map(|account| Acct(account.state.clone()))
            .ok_or_else(|| InvokeError::from("no account is open"))
    }
}
