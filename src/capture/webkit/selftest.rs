//! A self-test of the WebKitGTK game view, for development (debug builds
//! only): with `MOKORA_WEBKIT_SELFTEST=<dir>` set, once the view is made it
//! drives the page through autoplay's own [`GamePage`] calls and writes what
//! came of them to `<dir>/result.json` (plus two screenshots), so the input,
//! script, screenshot and capture paths can be checked against a test page
//! that logs what it receives — on a machine with no game account.

use super::WebKitTap;
use crate::autoplay::cdp_input::{
    dispatch_click_guarded, dispatch_drag, dispatch_wheel, evaluate_canvas_layout,
    install_input_probe, read_input_probe,
};
use crate::autoplay::page::{Clip, GamePage};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// The page's console on stdout, with `MOKORA_WEBKIT_CONSOLE` set. Called
/// on the main thread as the view is made, so it covers the first load.
pub fn console_to_stdout(view: &webkit2gtk::WebView) {
    use webkit2gtk::{SettingsExt, WebViewExt};
    if std::env::var_os("MOKORA_WEBKIT_CONSOLE").is_some() {
        if let Some(settings) = view.settings() {
            settings.set_enable_write_console_messages_to_stdout(true);
        }
    }
}

/// Start the self-test if the environment asks for it.
pub fn maybe_start(tap: &Arc<WebKitTap>) {
    let Some(dir) = std::env::var_os("MOKORA_WEBKIT_SELFTEST").map(PathBuf::from) else {
        return;
    };
    let tap = tap.clone();
    tauri::async_runtime::spawn(async move {
        let _ = std::fs::create_dir_all(&dir);
        let result = run(&tap, &dir).await;
        let _ = std::fs::write(
            dir.join("result.json"),
            serde_json::to_vec_pretty(&result).unwrap_or_default(),
        );
        tracing::info!("webkit selftest: done, {}", dir.display());
    });
}

async fn run(tap: &Arc<WebKitTap>, dir: &std::path::Path) -> serde_json::Value {
    // The page loads, its socket opens and echoes (a real game page takes
    // longer: `MOKORA_WEBKIT_SELFTEST_WAIT` seconds).
    let wait = std::env::var("MOKORA_WEBKIT_SELFTEST_WAIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let Some(page) = tap.page() else {
        return json!({ "error": "no view" });
    };
    tokio::time::sleep(Duration::from_secs(wait)).await;

    // `MOKORA_WEBKIT_SELFTEST_SAMPLE=<seconds>`: only sample the test page's
    // frame and timer counts every two seconds, to see what keeps running
    // while the window is minimised or hidden from outside.
    if let Some(secs) = std::env::var("MOKORA_WEBKIT_SELFTEST_SAMPLE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        let page = GamePage::WebKit(tap.page().expect("checked above"));
        let started = std::time::Instant::now();
        let mut samples = Vec::new();
        while started.elapsed().as_secs() < secs {
            let v = page
                .evaluate("[window.__log.raf, window.__log.timer, document.visibilityState]")
                .await
                .unwrap_or_default();
            samples.push(json!({ "t": started.elapsed().as_secs(), "v": v }));
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        return json!({ "samples": samples });
    }

    // `MOKORA_WEBKIT_SELFTEST_CLICK=x,y`: only click there (page pixels) the
    // way autoplay does, and keep a picture of before and after.
    if let Some((x, y)) = std::env::var("MOKORA_WEBKIT_SELFTEST_CLICK")
        .ok()
        .and_then(|s| {
            s.split_once(',')
                .map(|(a, b)| (a.trim().parse(), b.trim().parse()))
        })
        .and_then(|(a, b)| Some((a.ok()?, b.ok()?)))
    {
        let page = GamePage::WebKit(tap.page().expect("checked above"));
        if let Ok(png) = page.screenshot(None).await {
            let _ = std::fs::write(dir.join("before_click.png"), png);
        }
        let click = dispatch_click_guarded(&page, x, y, 200, 100, || true)
            .await
            .map_err(|e| format!("{e:#}"));
        tokio::time::sleep(Duration::from_secs(3)).await;
        if let Ok(png) = page.screenshot(None).await {
            let _ = std::fs::write(dir.join("after_click.png"), png);
        }
        return json!({ "click": click, "at": [x, y] });
    }
    let page = GamePage::WebKit(page);
    let step = |r: anyhow::Result<()>| r.err().map(|e| format!("{e:#}"));

    let sum = page
        .evaluate("(()=>1+1)()")
        .await
        .map_err(|e| format!("{e:#}"));
    let document = page
        .evaluate(
            "({state: document.readyState, canvases: document.getElementsByTagName('canvas').length, \
              visibility: document.visibilityState, size: [innerWidth, innerHeight], \
              webgl: !!document.createElement('canvas').getContext('webgl'), \
              webgl2: !!document.createElement('canvas').getContext('webgl2'), \
              title: document.title, laya: typeof Laya, game_mgr: typeof GameMgr, \
              resources: performance.getEntriesByType('resource').map(e => e.name.slice(0, 90) + ' ' + Math.round(e.duration)), \
              ws_hooked: !!window.__mokoraWs, ws_class: WebSocket.name, \
              sw: 'serviceWorker' in navigator, \
              sw_controller: !!(navigator.serviceWorker && navigator.serviceWorker.controller), \
              secure: window.isSecureContext})",
        )
        .await
        .map_err(|e| format!("{e:#}"));
    let undefined = page
        .evaluate("undefined")
        .await
        .map_err(|e| format!("{e:#}"));
    let layout = evaluate_canvas_layout(&page)
        .await
        .map(|(r, vp)| json!({ "rect": r, "viewport": vp }))
        .map_err(|e| format!("{e:#}"));
    let probe = step(install_input_probe(&page).await);
    let click = dispatch_click_guarded(&page, 100.0, 120.0, 150, 80, || true)
        .await
        .map_err(|e| format!("{e:#}"));
    let wheel = step(dispatch_wheel(&page, 200.0, 220.0, 100.0, 2, 50).await);
    // One turn of each size, to check the page gets the pixels asked for.
    for delta in [40.0, 300.0, -120.0] {
        let _ = page.wheel(210.0, 230.0, delta).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let drag = step(dispatch_drag(&page, 300.0, 300.0, 200.0, 5, 20).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let presses = read_input_probe(&page)
        .await
        .map(|c| c.down)
        .map_err(|e| format!("{e:#}"));
    let log = page
        .evaluate("window.__log")
        .await
        .map_err(|e| format!("{e:#}"));

    let full = page.screenshot(None).await;
    let left = page
        .screenshot(Some(Clip {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 30.0,
        }))
        .await;
    let mut shots = serde_json::Map::new();
    for (name, shot) in [("full", full), ("left", left)] {
        let v = match shot {
            Ok(png) => {
                let _ = std::fs::write(dir.join(format!("{name}.png")), &png);
                json!({ "bytes": png.len() })
            }
            Err(e) => json!({ "error": format!("{e:#}") }),
        };
        shots.insert(name.to_string(), v);
    }

    // A reload: waited for, and the new document is hooked and captured too.
    let frames_before = tap.frames_seen.load(Ordering::Relaxed);
    let reload = step(page.reload().await);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after_reload = json!({
        "error": reload,
        "new_frames": tap.frames_seen.load(Ordering::Relaxed) - frames_before,
        "page_ws": page.evaluate("window.__log.ws").await.map_err(|e| format!("{e:#}")),
    });

    json!({
        "after_reload": after_reload,
        "document": document,
        "url": page.url().await,
        "evaluate_sum": sum,
        "evaluate_undefined": undefined,
        "canvas_layout": layout,
        "probe_install_error": probe,
        "click": click,
        "wheel_error": wheel,
        "drag_error": drag,
        "presses_seen_by_probe": presses,
        "frames_captured": tap.frames_seen.load(Ordering::Relaxed),
        "page_log": log,
        "screenshots": shots,
    })
}
