//! Capture and input for the game in Mokora's own window on Linux.
//!
//! On Windows the window's browser is WebView2, Chromium under another name,
//! so the game is read and driven over the DevTools protocol like a launched
//! browser (`capture::chromium::embedded`). On Linux the window's browser is
//! WebKitGTK, which has no DevTools protocol; this module does the same jobs
//! with what WebKit and GTK offer:
//!
//! - **Reading the game.** [`HOOK`] runs in the page before its own scripts
//!   and queues every WebSocket frame in the page; [`WebKitBackend`] takes
//!   the queue every [`DRAIN_EVERY`] and feeds the frames through the very
//!   path CDP frames take (`cdp::route_frame`), so parsing, recording and
//!   everything after are shared. (Not a script message handler: wry's IPC
//!   handler takes every script message of the view as a Tauri command.)
//!   [`HOOK`] also decompresses the game build's `.gz` files where libsoup
//!   leaves them compressed (see the file).
//! - **Driving it.** [`WebKitPage`] is autoplay's page here
//!   (`autoplay::page::GamePage::WebKit`): mouse and wheel events are
//!   delivered to the view as GTK events, so the page sees ordinary trusted
//!   input; scripts run through `evaluate_javascript`; screenshots come from
//!   WebKit's own snapshot.
//! - **Telemetry.** The game's analytics beacons are refused by a WebKit
//!   content filter ([`telemetry`]), as the CDP leg answers them in place.
//!
//! Everything WebKit and GTK is touched on the main thread: [`WebKitPage`]
//! hands each job over with `Webview::with_webview` and waits for the answer.

mod input;
#[cfg(debug_assertions)]
mod selftest;
mod telemetry;

use crate::autoplay::page::{Clip, GamePage, MouseEvent};
use crate::capture::chromium::cdp::{self, FlowKey};
use crate::capture::{CaptureBackend, CaptureCtx, CaptureDescriptor, CaptureKind, ShutdownToken};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use gtk::prelude::*;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

/// The script Mokora runs in the game page (see the file).
pub const HOOK: &str = include_str!("hook.js");

/// How often the capture takes what [`HOOK`] has queued. The game's own
/// decisions are seconds apart; this is the most a frame waits.
const DRAIN_EVERY: Duration = Duration::from_millis(25);

/// Takes and empties [`HOOK`]'s queue: an array of JSON strings, or `null`
/// before the hook has run.
const DRAIN: &str = "(()=>{const q=window.__mokoraWsQueue;return q?q.splice(0):null})()";

/// How long a job on the main thread may take before it counts as lost.
const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(15);

/// A message from [`HOOK`].
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "k")]
enum PageMessage {
    #[serde(rename = "open")]
    Open { id: String, url: String },
    #[serde(rename = "f")]
    Frame {
        id: String,
        up: bool,
        op: i64,
        d: String,
    },
    #[serde(rename = "close")]
    Close { id: String },
}

/// What the game view tells the capture, besides its page's queue.
#[derive(Debug)]
enum TapEvent {
    /// The view changed size (the game canvas moves with it).
    Resized,
}

/// An account's game view, as the capture and autoplay see it: made by the
/// hub with the view ([`attach`]), read by [`WebKitBackend`] while it runs.
#[derive(Default)]
pub struct WebKitTap {
    /// Where the view's messages go while a capture listens.
    sink: Mutex<Option<mpsc::UnboundedSender<TapEvent>>>,
    page: OnceLock<WebKitPage>,
    /// WebSocket frames the capture has routed, for diagnostics.
    pub frames_seen: std::sync::atomic::AtomicU64,
}

impl WebKitTap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn send(&self, ev: TapEvent) {
        if let Ok(sink) = self.sink.lock() {
            if let Some(tx) = sink.as_ref() {
                let _ = tx.send(ev);
            }
        }
    }

    /// Start taking the view's messages (only the latest listener gets them).
    fn listen(&self) -> mpsc::UnboundedReceiver<TapEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut sink) = self.sink.lock() {
            *sink = Some(tx);
        }
        rx
    }

    fn stop(&self) {
        if let Ok(mut sink) = self.sink.lock() {
            *sink = None;
        }
    }

    /// The game view, once it is made.
    pub fn page(&self) -> Option<WebKitPage> {
        self.page.get().cloned()
    }
}

/// Wire the account's freshly made game view to `tap`. The view must have
/// been built with [`HOOK`] as an initialization script. Runs on the main
/// thread.
pub fn attach(view: &tauri::Webview, tap: &Arc<WebKitTap>) -> tauri::Result<()> {
    let _ = tap.page.set(WebKitPage {
        view: Arc::new(view.clone()),
        id: Arc::from(view.label()),
    });
    #[cfg(debug_assertions)]
    let tap_for_selftest = tap.clone();
    let tap = tap.clone();
    view.with_webview(move |w| {
        let wv = w.inner();
        #[cfg(debug_assertions)]
        selftest::console_to_stdout(&wv);
        // Only a change of size moves the canvas; parking the view off the
        // window (a change of position) does not.
        let last = std::cell::Cell::new((0, 0));
        wv.connect_size_allocate(move |_, a| {
            let size = (a.width(), a.height());
            if last.replace(size) != size {
                tap.send(TapEvent::Resized);
            }
        });
    })?;
    #[cfg(debug_assertions)]
    selftest::maybe_start(&tap_for_selftest);
    Ok(())
}

/// The game view as autoplay drives it. Cheap to clone (and as small as the
/// CDP page it stands beside in `GamePage`).
#[derive(Clone)]
pub struct WebKitPage {
    view: Arc<tauri::Webview>,
    id: Arc<str>,
}

impl WebKitPage {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Run `job` on the main thread with the view, and wait for what it sends
    /// back (it may answer later, from a WebKit callback).
    async fn on_main<T: Send + 'static>(
        &self,
        what: &str,
        job: impl FnOnce(webkit2gtk::WebView, tokio::sync::oneshot::Sender<Result<T>>) + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.view
            .with_webview(move |w| job(w.inner(), tx))
            .map_err(|e| anyhow!("{what}: {e}"))?;
        tokio::time::timeout(MAIN_THREAD_TIMEOUT, rx)
            .await
            .map_err(|_| {
                anyhow!(
                    "{what}: no answer within {}s",
                    MAIN_THREAD_TIMEOUT.as_secs()
                )
            })?
            .map_err(|_| anyhow!("{what}: the view went away"))?
    }

    pub async fn url(&self) -> Option<String> {
        self.view.url().ok().map(|u| u.to_string())
    }

    pub async fn mouse(&self, ev: MouseEvent) -> Result<()> {
        self.on_main("mouse event", move |wv, tx| {
            let _ = tx.send(input::mouse(&wv, ev));
        })
        .await
    }

    pub async fn wheel(&self, x: f64, y: f64, delta_y: f64) -> Result<()> {
        self.on_main("wheel event", move |wv, tx| {
            let _ = tx.send(input::wheel(&wv, x, y, delta_y));
        })
        .await
    }

    pub async fn evaluate(&self, expr: &str) -> Result<serde_json::Value> {
        let expr = expr.to_string();
        let json = self
            .on_main("evaluate", move |wv, tx| {
                wv.evaluate_javascript(&expr, None, None, None::<&gio::Cancellable>, move |r| {
                    use javascriptcore::ValueExt;
                    let out = r
                        .map(|v| {
                            if v.is_undefined() || v.is_null() {
                                None
                            } else {
                                v.to_json(0).map(|s| s.to_string())
                            }
                        })
                        .map_err(|e| anyhow!("evaluate: {e}"));
                    let _ = tx.send(out);
                });
            })
            .await?;
        match json {
            None => Ok(serde_json::Value::Null),
            Some(s) => serde_json::from_str(&s).context("evaluate: the value is not JSON"),
        }
    }

    pub async fn screenshot(&self, clip: Option<Clip>) -> Result<Vec<u8>> {
        self.on_main("screenshot", move |wv, tx| {
            wv.snapshot(
                SnapshotRegion::Visible,
                SnapshotOptions::NONE,
                None::<&gio::Cancellable>,
                move |r| {
                    let out = r
                        .map_err(|e| anyhow!("snapshot: {e}"))
                        .and_then(|surface| png_of(&surface, clip));
                    let _ = tx.send(out);
                },
            );
        })
        .await
    }

    /// Reload, then wait until the new page has loaded (as CDP's reload does).
    pub async fn reload(&self) -> Result<()> {
        self.start_reload().await?;
        // The old document can still answer for a moment: wait for it to go
        // before waiting for the new one to be complete.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let state = self.evaluate("document.readyState").await.ok();
            if state.as_ref().and_then(|v| v.as_str()) == Some("complete") {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!(
                    "reload: the page did not finish loading within 30s"
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn start_reload(&self) -> Result<()> {
        self.on_main("reload", |wv, tx| {
            wv.reload();
            let _ = tx.send(Ok(()));
        })
        .await
    }
}

/// `surface` (a snapshot of the visible page) cut to `clip` and encoded as
/// PNG, one image pixel per CSS pixel. A snapshot of a high-density screen
/// carries its device scale, and painting it at 1:1 scales it down.
fn png_of(surface: &cairo::Surface, clip: Option<Clip>) -> Result<Vec<u8>> {
    let (sx, sy) = surface.device_scale();
    let full = cairo::ImageSurface::try_from(surface.clone())
        .map_err(|_| anyhow!("snapshot: not an image"))?;
    let (page_w, page_h) = (
        f64::from(full.width()) / sx.max(f64::MIN_POSITIVE),
        f64::from(full.height()) / sy.max(f64::MIN_POSITIVE),
    );
    let c = clip.unwrap_or(Clip {
        x: 0.0,
        y: 0.0,
        width: page_w,
        height: page_h,
    });
    let (w, h) = (
        c.width.round().max(1.0) as i32,
        c.height.round().max(1.0) as i32,
    );
    let mut out = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
        .map_err(|e| anyhow!("snapshot: {e}"))?;
    {
        let cr = cairo::Context::new(&out).map_err(|e| anyhow!("snapshot: {e}"))?;
        cr.set_source_surface(surface, -c.x, -c.y)
            .map_err(|e| anyhow!("snapshot: {e}"))?;
        cr.paint().map_err(|e| anyhow!("snapshot: {e}"))?;
    }
    out.flush();
    let stride = out.stride() as usize;
    let data = out.data().map_err(|e| anyhow!("snapshot: {e}"))?;
    Ok(encode_rgba_png(&data, w as usize, h as usize, stride))
}

/// Cairo's native-endian, premultiplied ARGB32 rows as an RGBA PNG.
fn encode_rgba_png(argb: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in argb.chunks(stride).take(height) {
        for px in row[..width * 4].as_chunks::<4>().0 {
            let v = u32::from_ne_bytes(*px);
            let a = (v >> 24) as u8;
            let un = |c: u32| -> u8 {
                let c = (c & 0xff) as u8;
                if a == 0 {
                    0
                } else {
                    ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[un(v >> 16), un(v >> 8), un(v), a]);
        }
    }
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, width as u32, height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let _ = w.write_image_data(&rgba);
        }
    }
    png
}

/// The capture for the game in Mokora's own window on Linux.
pub struct WebKitBackend {
    tap: Arc<WebKitTap>,
}

impl WebKitBackend {
    pub fn new(tap: Arc<WebKitTap>) -> Self {
        Self { tap }
    }
}

/// The document a socket id belongs to ([`HOOK`] numbers sockets per
/// document as `ws<n>-<doc>`).
fn document_of(id: &str) -> &str {
    id.split_once('-').map_or("", |(_, doc)| doc)
}

#[async_trait]
impl CaptureBackend for WebKitBackend {
    async fn run(self: Box<Self>, ctx: CaptureCtx, shutdown: ShutdownToken) -> Result<()> {
        let page = self
            .tap
            .page()
            .context("the game view is not made yet (embedded mode needs it)")?;
        if let Err(e) = telemetry::apply(&page.view, ctx.block_telemetry) {
            warn!("webkit capture: telemetry filter unavailable: {e:#}");
        }
        let bridges = crate::capture::chromium::make_bridges(&ctx);
        let inspector = ctx.session.inspector();
        let mut rx = self.tap.listen();
        info!("webkit capture: reading the game view {}", page.id());
        let mut drain = tokio::time::interval(DRAIN_EVERY);
        drain.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Sockets still open, by id; the document they belong to.
        let mut open: HashSet<String> = HashSet::new();
        let mut document = String::new();
        let key = |id: &str| FlowKey {
            target: page.id().to_string(),
            request: id.to_string(),
        };
        loop {
            let messages = tokio::select! {
                _ = shutdown.wait() => break,
                ev = rx.recv() => match ev {
                    Some(TapEvent::Resized) => {
                        if let Some(autoplay) = &ctx.autoplay {
                            let ours = autoplay
                                .page
                                .read()
                                .await
                                .as_ref()
                                .is_some_and(|p| p.id() == page.id());
                            if ours {
                                autoplay.resize.note_resized();
                            }
                        }
                        continue;
                    }
                    None => break,
                },
                _ = drain.tick() => match page.evaluate(DRAIN).await {
                    // Between documents, or a page busy for a moment: the
                    // next tick takes what is there.
                    Err(_) => continue,
                    Ok(v) => match serde_json::from_value::<Option<Vec<String>>>(v) {
                        Ok(Some(m)) if !m.is_empty() => m,
                        _ => continue,
                    },
                },
            };
            for text in messages {
                let message = match serde_json::from_str::<PageMessage>(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        debug!("webkit capture: unreadable page message: {e}");
                        continue;
                    }
                };
                match message {
                    PageMessage::Open { id, url } => {
                        // A new document (a reload) left the old one's sockets
                        // without a close: their flows go with it.
                        if document_of(&id) != document {
                            for old in open.drain() {
                                let k = key(&old);
                                let bridge = bridges.acquire(k.clone(), "ws", "ws frame");
                                bridges.release(&k, bridge);
                            }
                            document = document_of(&id).to_string();
                        }
                        let _ = bridges.acquire(
                            key(&id),
                            &crate::capture::flow::slugify(&url),
                            &format!("ws {url}"),
                        );
                        open.insert(id.clone());
                        debug!("ws created: {url} ({id})");
                        if let Some(autoplay) = &ctx.autoplay {
                            if cdp::is_autoplay_target_url(&url) {
                                let mut guard = autoplay.page.write().await;
                                let same = guard.as_ref().is_some_and(|p| p.id() == page.id());
                                if !same {
                                    *guard = Some(GamePage::WebKit(page.clone()));
                                    drop(guard);
                                    *autoplay.canvas_rect.write().await = None;
                                    info!(
                                        "autoplay: page handle bound to the game view via WS {url}"
                                    );
                                }
                            }
                        }
                    }
                    PageMessage::Frame { id, up, op, d } => {
                        self.tap
                            .frames_seen
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        cdp::route_frame(up, op, &d, key(&id), &bridges, &inspector, &ctx.mjai_bus);
                    }
                    PageMessage::Close { id } => {
                        if open.remove(&id) {
                            let k = key(&id);
                            let bridge = bridges.acquire(k.clone(), "ws", "ws frame");
                            bridges.release(&k, bridge);
                        }
                        debug!("ws closed: {id}");
                    }
                }
            }
        }
        self.tap.stop();
        info!("webkit capture: stopped");
        Ok(())
    }

    fn descriptor(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            kind: CaptureKind::Chromium,
            label: "built-in".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hook's messages, as it writes them.
    #[test]
    fn page_messages_parse() {
        let open: PageMessage =
            serde_json::from_str(r#"{"k":"open","id":"a1b2-ws1","url":"wss://x.maj-soul.com/gw"}"#)
                .unwrap();
        assert_eq!(
            open,
            PageMessage::Open {
                id: "a1b2-ws1".into(),
                url: "wss://x.maj-soul.com/gw".into()
            }
        );
        let frame: PageMessage =
            serde_json::from_str(r#"{"k":"f","id":"a1b2-ws1","up":true,"op":2,"d":"AAE="}"#)
                .unwrap();
        assert_eq!(
            frame,
            PageMessage::Frame {
                id: "a1b2-ws1".into(),
                up: true,
                op: 2,
                d: "AAE=".into()
            }
        );
        let close: PageMessage = serde_json::from_str(r#"{"k":"close","id":"a1b2-ws1"}"#).unwrap();
        assert_eq!(
            close,
            PageMessage::Close {
                id: "a1b2-ws1".into()
            }
        );
    }

    #[test]
    fn sockets_are_grouped_by_document() {
        assert_eq!(document_of("ws1-k3x9q"), "k3x9q");
        assert_eq!(document_of("ws12-k3x9q"), "k3x9q");
        assert_eq!(document_of("ws1"), "");
    }

    /// Premultiplied ARGB comes out as straight RGBA.
    #[test]
    fn argb_rows_become_rgba() {
        // One half-transparent red pixel (premultiplied: r = 0x80), then a
        // stride's padding.
        let px = 0x8080_0000u32.to_ne_bytes();
        let mut row = px.to_vec();
        row.extend_from_slice(&[0; 4]);
        let png_bytes = encode_rgba_png(&row, 1, 1, 8);
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..4], &[255, 0, 0, 0x80]);
    }
}
