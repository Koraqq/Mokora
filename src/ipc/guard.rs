//! Who may call Mokora's commands: its own pages only.
//!
//! Tauri gives every web view its IPC, the games' too. A game is a web view of
//! the Mahjong Soul site, so without this its scripts (the site's own, and
//! whatever the site loads) could call any command, and between them the
//! commands run programs, write files and open links. Tauri checks its own and
//! the plugins' commands against the capabilities, which grant local pages
//! only, but an app's commands only when the app ships a permission manifest
//! for them. So the handler is wrapped instead: a call is served only when it
//! comes from a page Mokora serves itself, and never from a game's web view.

use std::collections::HashSet;
use std::sync::Mutex;
use tauri::ipc::Invoke;
use tauri::{Runtime, Url};
use tracing::warn;

/// Is `url` a page Mokora serves itself: the bundled interface
/// (`tauri://localhost` on Linux, `http(s)://tauri.localhost` on Windows), or
/// the dev server (`build.devUrl`) under `tauri dev`.
pub fn is_own_url(url: &Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("tauri", Some("localhost")) => true,
        ("http" | "https", Some("tauri.localhost")) => true,
        ("http", Some("localhost" | "127.0.0.1")) => tauri::is_dev() && url.port() == Some(1420),
        _ => false,
    }
}

/// `handler`, answering only Mokora's own pages; any other caller is refused.
pub fn own_pages_only<R, F>(handler: F) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static
where
    R: Runtime,
    F: Fn(Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke: Invoke<R>| {
        let webview = invoke.message.webview_ref();
        let game = webview.label().starts_with(crate::hub::views::GAME_PREFIX);
        let own = !game && webview.url().is_ok_and(|u| is_own_url(&u));
        if own {
            return handler(invoke);
        }
        note_refused(webview.label(), invoke.message.command());
        invoke.resolver.reject("this command is only for Mokora's own pages");
        true
    }
}

/// Logs a refused command once per web view and command, so a page calling in
/// a loop cannot fill the log.
fn note_refused(label: &str, command: &str) {
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let key = format!("{label} {command}");
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.get_or_insert_with(HashSet::new).insert(key) {
        warn!(webview = label, command, "refused a command from a page that is not Mokora's");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own(s: &str) -> bool {
        is_own_url(&s.parse().unwrap())
    }

    #[test]
    fn only_the_bundled_interface_is_mokora_s_own() {
        assert!(own("http://tauri.localhost/index.html"));
        assert!(own("https://tauri.localhost/#/settings"));
        assert!(own("tauri://localhost/index.html"));
        assert!(!own("https://game.maj-soul.com/1/"));
        assert!(!own("https://tauri.localhost.evil.example/"));
        assert!(!own("http://evil.example/?tauri.localhost"));
        assert!(!own("file:///C:/index.html"));
        assert!(!own("about:blank"));
        // The dev server, only under `tauri dev`.
        assert_eq!(own("http://localhost:1420/"), tauri::is_dev());
        assert!(!own("http://localhost:8080/"));
    }
}
