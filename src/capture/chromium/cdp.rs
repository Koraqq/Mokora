//! Per-page CDP subscription that routes WebSocket frames into the
//! platform bridge.
//!
//! Why per-page (not browser-level): chromiumoxide 0.9.1 does not deliver
//! page-scoped events to `Browser::event_listener` even with
//! `Target.setAutoAttach { flatten: true }`. The events arrive on the
//! browser connection but stay tagged with the originating page session;
//! `Browser::event_listener` only surfaces browser-level events. The
//! canonical pattern (see `chromiumoxide-0.9.1/examples/interception.rs`)
//! is to grab a `Page` and call `page.event_listener::<E>()` on it.
//!
//! Subscription lifecycle:
//! - Poll `browser.pages()` every ~1s.
//! - On a new `target_id`: enable Network domain on that page, subscribe
//!   to the four WS events, spawn a routing task.
//! - On a `target_id` disappearing from the snapshot (tab closed):
//!   `JoinHandle::abort` the routing task and drop our entry.
//!
//! Service-worker WebSockets are not subscribed in v1 — Majsoul uses
//! page-scoped WS today. If real-world testing shows otherwise, expand
//! the polling to include `browser.targets()` and filter on type.

use crate::autoplay::AutoplayContext;
use crate::bridge::Direction;
use crate::capture::flow::{slugify, FlowBridges};
use crate::config::HttpCaptureConfig;
use crate::event_bus::MjaiBus;
use crate::inspector::annotate::{self, RequestView};
use crate::inspector::InspectorWriter;
use crate::schema::{
    CaptureSource, FrameDirection, FrameRaw, HttpBody, HttpExchange, HttpHeader, HttpPhase,
    InspectorEntry,
};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use chromiumoxide::page::Page;
use chromiumoxide::{
    cdp::browser_protocol::network::{
        EnableParams as NetworkEnableParams, EventRequestWillBeSent, EventResponseReceived,
        EventWebSocketClosed, EventWebSocketCreated, EventWebSocketFrameReceived,
        EventWebSocketFrameSent, Headers, ResourceType,
    },
    Browser,
};
use chrono::Local;
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const PAGE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Per-flow key for `FlowBridges`. `target` is the page that owns the
/// WebSocket; `request` is the CDP request id that the page assigned to
/// `new WebSocket(...)`. The pair is unique across the browser session
/// even when two tabs both open a connection to the same Majsoul host.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub target: String,
    pub request: String,
}

fn decode_payload(b64: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .ok()
}

/// Outcome of decoding a `Network.webSocketFrame*` payload into raw bytes
/// for the bridge.
///
/// CDP's `WebSocketFrame.payloadData` is shaped by the WS opcode (RFC 6455):
///
/// - opcode `1` (text): the field is a **plain UTF-8 string** (text-protocol
///   games, e.g. JSON frames). We pass the bytes straight through; the bridge
///   re-parses them.
/// - opcode `2` (binary): the field is a **base64-encoded string**.
///   Majsoul uses this (length-prefixed protobuf).
/// - everything else (`0` continuation, `8` close, `9` ping, `10` pong):
///   carries no game data — drop.
///
/// Splitting this out so the dispatch is unit-testable: prior to this
/// fix the inline branches only handled opcode 2, which silently dropped
/// every text frame on the chromium backend.
#[derive(Debug, PartialEq, Eq)]
enum FrameDecode {
    Bytes(Vec<u8>),
    Skip,
    BadBase64,
}

fn decode_frame_payload(opcode: i64, payload_data: &str) -> FrameDecode {
    match opcode {
        1 => FrameDecode::Bytes(payload_data.as_bytes().to_vec()),
        2 => match decode_payload(payload_data) {
            Some(b) => FrameDecode::Bytes(b),
            None => FrameDecode::BadBase64,
        },
        _ => FrameDecode::Skip,
    }
}

/// Compute the symmetric difference between the previous and current
/// page snapshots. Returns `(adds, removes)` — target ids to subscribe
/// and target ids whose subscription tasks should be reaped. Pure so
/// the diff logic is unit-testable independent of the CDP loop.
pub fn diff_pages(prev: &HashSet<String>, current: &HashSet<String>) -> (Vec<String>, Vec<String>) {
    let adds: Vec<_> = current.difference(prev).cloned().collect();
    let removes: Vec<_> = prev.difference(current).cloned().collect();
    (adds, removes)
}

/// Decide whether the autoplay page handle — currently owned by tab
/// `owner` (its `TargetId`, if any) — must be cleared when the page-poll
/// loop reaps the `removed` tabs this tick.
///
/// The handle tracks the browser **tab**, not any single WebSocket, so it
/// is cleared only when its owning tab disappears from the snapshot. This
/// is the crux of the "autoplay silently stops mid-game" fix: Majsoul
/// opens and closes many short-lived Route-probe / lobby-reconnect sockets
/// to `*.maj-soul.com` while a game runs on a separate `game-gateway`
/// socket, and those socket closures must **not** drop the handle. Pure so
/// the decision is unit-testable without a live `Page`.
pub fn page_handle_cleared_by_removal(owner: Option<&str>, removed: &[String]) -> bool {
    matches!(owner, Some(o) if removed.iter().any(|r| r == o))
}

/// Hosts whose WebSocket creation hands the page handle to autoplay.
/// `maj-soul.com` covers en/cn/jp portals; `mahjongsoul.com` is the
/// Yostar mirror. A new game's hosts go here.
const AUTOPLAY_HOST_HINTS: &[&str] = &["maj-soul.com", "mahjongsoul.com"];

/// Is `url` one of Mokora's own interface pages (served as `tauri.localhost`
/// or `tauri://`, or the dev server)?
fn is_own_page(url: Option<&str>) -> bool {
    url.is_some_and(|u| {
        u.contains("tauri.localhost") || u.starts_with("tauri://") || u.contains("localhost:1420")
    })
}

pub(crate) fn is_autoplay_target_url(ws_url: &str) -> bool {
    AUTOPLAY_HOST_HINTS.iter().any(|h| ws_url.contains(h))
}

/// Run the CDP loop until the browser disconnects or an unrecoverable
/// error occurs. Frames flow through `bridges` into `mjai_bus`, and each
/// frame is also recorded into `inspector` for the Logs → Inspector tab.
///
/// `autoplay` is `Some` only on the chromium backend when the autoplay
/// feature is wired (`AppState.autoplay_context`). On Majsoul WS open
/// we publish the page handle into it; autoplay reads it back to dispatch
/// `Input.dispatchMouseEvent`. Passing `None` makes the loop bridge-only.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    endpoint: &str,
    bridges: Arc<FlowBridges<FlowKey>>,
    mjai_bus: MjaiBus,
    inspector: InspectorWriter,
    autoplay: Option<Arc<AutoplayContext>>,
    http_cfg: HttpCaptureConfig,
    block_telemetry: bool,
    skip_own_pages: bool,
) -> Result<()> {
    info!("CDP connecting to {endpoint}");
    let (browser_owned, mut handler) = Browser::connect(endpoint)
        .await
        .with_context(|| format!("CDP connect to {endpoint}"))?;
    // `Browser` is not `Clone`; share via Arc for the page-poll task.
    let browser = Arc::new(browser_owned);

    // Pump the chromiumoxide handler — required so its internal
    // request/response oneshots resolve. The handler also surfaces
    // `WS Invalid message` warnings when Chrome sends events
    // chromiumoxide doesn't have a typed binding for; those are
    // non-fatal noise and the stream keeps running.
    let pump = tokio::spawn(async move {
        while let Some(ev) = handler.next().await {
            if let Err(e) = ev {
                debug!("chromiumoxide handler event error: {e:?}");
            }
        }
    });

    // Per-page subscription registry. Key: TargetId stringified.
    let mut subscribed: HashMap<String, JoinHandle<()>> = HashMap::new();

    let poll_loop = async {
        loop {
            let pages = match browser.pages().await {
                Ok(p) => p,
                Err(e) => {
                    debug!("browser.pages() error: {e:?}");
                    tokio::time::sleep(PAGE_POLL_INTERVAL).await;
                    continue;
                }
            };
            let current: HashSet<String> = pages
                .iter()
                .map(|p| p.target_id().inner().clone())
                .collect();
            let prev: HashSet<String> = subscribed.keys().cloned().collect();
            let (adds, removes) = diff_pages(&prev, &current);

            // Reap closed tabs first so we don't leak resources during
            // long sessions where users open + close many tabs.
            for id in &removes {
                if let Some(h) = subscribed.remove(id) {
                    h.abort();
                    debug!("CDP: dropped subscription for closed target {id}");
                }
            }

            // The autoplay page handle tracks the browser *tab*, not any
            // single WebSocket, so it is cleared here — when its owning tab
            // is actually gone — rather than on `webSocketClosed`. Majsoul
            // opens and closes many short-lived Route-probe / lobby-reconnect
            // sockets to *.maj-soul.com while a game runs on a separate
            // game-gateway socket; clearing the handle on those closes was
            // silently stopping autoplay mid-game.
            if let (Some(ctx), false) = (&autoplay, removes.is_empty()) {
                // Hold the write lock across the check + clear so a
                // concurrent rebind from another tab's task can't slip
                // between reading the owner and nulling the handle.
                let mut guard = ctx.page.write().await;
                let owner = guard.as_ref().map(|p| p.id());
                if page_handle_cleared_by_removal(owner.as_deref(), &removes) {
                    *guard = None;
                    drop(guard);
                    *ctx.canvas_rect.write().await = None;
                    info!("autoplay: page handle cleared — owning Majsoul tab closed");
                }
            }

            // Subscribe new tabs.
            for page in pages {
                let id = page.target_id().inner().clone();
                if !adds.contains(&id) {
                    continue;
                }
                // In Mokora's own window the browser is shared with its
                // interface, whose pages have nothing to capture.
                if skip_own_pages && is_own_page(page.url().await.ok().flatten().as_deref()) {
                    continue;
                }
                match attach_page(
                    page.clone(),
                    id.clone(),
                    bridges.clone(),
                    mjai_bus.clone(),
                    inspector.clone(),
                    autoplay.clone(),
                    http_cfg.clone(),
                    block_telemetry,
                )
                .await
                {
                    Ok(handle) => {
                        info!("CDP: attached to page target {id}");
                        subscribed.insert(id, handle);
                    }
                    Err(e) => {
                        warn!("CDP: failed to attach to target {id}: {e:#}");
                    }
                }
            }

            tokio::time::sleep(PAGE_POLL_INTERVAL).await;
        }
        // unreachable, but type-check the future as `()` for select arm
        #[allow(unreachable_code)]
        ()
    };

    tokio::select! {
        _ = pump => info!("CDP handler pump exited"),
        _ = poll_loop => info!("CDP page poll exited"),
    }
    // Abort any still-live page subscriptions before tearing down.
    for (_id, h) in subscribed {
        h.abort();
    }
    drop(browser);
    Err(anyhow!("CDP loop terminated"))
}

/// Requests to the Aliyun SLS web-tracking service, the analytics vendor the
/// game clients report through. Loose on purpose (`*` also spans `/`); the
/// host is checked exactly once a request is paused — see
/// [`is_telemetry_host`].
const TELEMETRY_URL_PATTERN: &str = "*.log.aliyuncs.com/*";

/// With `block_telemetry`, ask the browser to hand us the game's analytics
/// beacons before they are sent: paused at the *request* stage, so they never
/// leave the machine. (A game whose client script must be rewritten on its way
/// in adds a *response*-stage pattern here — see `_parked/other-games/`.)
async fn enable_interception(page: &Page, block_telemetry: bool) -> Result<()> {
    use chromiumoxide::cdp::browser_protocol::fetch::{EnableParams, RequestPattern, RequestStage};
    let mut patterns = Vec::new();
    if block_telemetry {
        patterns.push(RequestPattern {
            url_pattern: Some(TELEMETRY_URL_PATTERN.to_string()),
            resource_type: None,
            request_stage: Some(RequestStage::Request),
        });
    }
    if patterns.is_empty() {
        return Ok(());
    }
    let params = EnableParams {
        patterns: Some(patterns),
        handle_auth_requests: None,
    };
    page.execute(params).await.context("Fetch.enable")?;
    Ok(())
}

/// The SLS service's hosts — `<project>.<region>.log.aliyuncs.com` serves
/// nothing but beacons. The same rule as the MITM leg's CONNECT refusal.
fn is_telemetry_host(host: &str) -> bool {
    host == "log.aliyuncs.com" || host.ends_with(".log.aliyuncs.com")
}

/// Answer a beacon paused at the request stage the way the real endpoint
/// does — `200`, empty body — without it ever being sent, which is also what
/// an ad blocker's gap looks like to the client. Anything the loose pattern
/// caught that is not a beacon is let through unchanged.
async fn answer_paused_request(
    page: &Page,
    request_id: chromiumoxide::cdp::browser_protocol::fetch::RequestId,
    url: &str,
) {
    use chromiumoxide::cdp::browser_protocol::fetch::{
        ContinueRequestParams, FulfillRequestParams, HeaderEntry,
    };
    if is_telemetry_host(&host_of(url)) {
        let fulfilled = FulfillRequestParams::builder()
            .request_id(request_id.clone())
            .response_code(200)
            // Beacons are fire-and-forget XHRs from the game's origin; say
            // the empty answer may be read so none surfaces as a CORS error.
            .response_header(HeaderEntry::new("Access-Control-Allow-Origin", "*"))
            .body(String::new())
            .build();
        match fulfilled {
            Ok(p) => match page.execute(p).await {
                Ok(_) => {
                    info!(target: "mokora::capture::http", "blocked telemetry beacon: {url}");
                    return;
                }
                Err(e) => warn!("CDP: could not answer telemetry beacon {url}: {e:#}"),
            },
            Err(e) => warn!("CDP: could not build telemetry answer: {e}"),
        }
    }
    if let Ok(p) = ContinueRequestParams::builder()
        .request_id(request_id)
        .build()
    {
        let _ = page.execute(p).await;
    }
}

/// Enable Network on the page, subscribe to the four WS events, and
/// spawn a routing task. Returns the task handle so the poll loop can
/// abort it when the tab closes.
#[allow(clippy::too_many_arguments)]
async fn attach_page(
    page: Page,
    target_id: String,
    bridges: Arc<FlowBridges<FlowKey>>,
    mjai_bus: MjaiBus,
    inspector: InspectorWriter,
    autoplay: Option<Arc<AutoplayContext>>,
    http_cfg: HttpCaptureConfig,
    block_telemetry: bool,
) -> Result<JoinHandle<()>> {
    page.execute(NetworkEnableParams::default())
        .await
        .context("Network.enable")?;
    // The pause listener has to exist before `Fetch.enable` arms the
    // interceptor: a request paused with no listener yet is a request nobody
    // ever continues, and the page hangs on it. The stream buffers
    // (unbounded) until the routing task below starts polling, so
    // subscribing early costs nothing.
    let mut on_paused = page
        .event_listener::<chromiumoxide::cdp::browser_protocol::fetch::EventRequestPaused>()
        .await
        .context("subscribe requestPaused")?;
    // Hold back the game's analytics beacons. Scoped to that one service so
    // nothing else on the page is paused; a failure to enable is logged and
    // the session continues without the block.
    if let Err(e) = enable_interception(&page, block_telemetry).await {
        warn!("CDP: request interception unavailable on target {target_id}: {e:#}");
    }
    let mut on_created = page
        .event_listener::<EventWebSocketCreated>()
        .await
        .context("subscribe webSocketCreated")?;
    let mut on_recv = page
        .event_listener::<EventWebSocketFrameReceived>()
        .await
        .context("subscribe webSocketFrameReceived")?;
    let mut on_sent = page
        .event_listener::<EventWebSocketFrameSent>()
        .await
        .context("subscribe webSocketFrameSent")?;
    let mut on_closed = page
        .event_listener::<EventWebSocketClosed>()
        .await
        .context("subscribe webSocketClosed")?;
    // Every HTTP request the page makes, so we can pick the game's
    // analytics beacons out of it. `Network.enable` above already turns
    // this event on; the filtering is ours, in the select arm.
    let mut on_request = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .context("subscribe requestWillBeSent")?;
    let mut on_response = page
        .event_listener::<EventResponseReceived>()
        .await
        .context("subscribe responseReceived")?;
    // The window changing size moves the game canvas; autoplay re-measures
    // it before its next click (see `autoplay::resize`). A browser-side
    // event, so nothing is added to the page.
    let mut on_resized = page
        .event_listener::<chromiumoxide::cdp::browser_protocol::page::EventFrameResized>()
        .await
        .context("subscribe frameResized")?;

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(ev) = on_paused.next() => {
                    let url = ev.request.url.clone();
                    // Paused at the request stage, which only the telemetry
                    // pattern asks for.
                    answer_paused_request(&page, ev.request_id.clone(), &url).await;
                }
                Some(ev) = on_created.next() => {
                    let key = FlowKey {
                        target: target_id.clone(),
                        request: ev.request_id.inner().clone(),
                    };
                    let label = format!("ws {}", ev.url);
                    let slug = slugify(&ev.url);
                    let _ = bridges.acquire(key, &slug, &label);
                    debug!("ws created: {} (target {target_id} request {})", ev.url, ev.request_id.inner());

                    // If this is the platform's WS (Majsoul), capture
                    // the owning page so autoplay can dispatch input
                    // into it. The handle is bound to the *tab* and lives
                    // until the tab closes (see the poll loop); a new WS on
                    // the same tab just refreshes it. Multi-tab user:
                    // most-recent wins, per the plan.
                    if let Some(ctx) = &autoplay {
                        if is_autoplay_target_url(&ev.url) {
                            let mut guard = ctx.page.write().await;
                            let prev_target =
                                guard.as_ref().map(|p| p.id());
                            let same_tab = prev_target.as_deref() == Some(target_id.as_str());
                            *guard = Some(crate::autoplay::page::GamePage::Cdp(page.clone()));
                            drop(guard);
                            if same_tab {
                                // Majsoul re-opens sockets (Route probes,
                                // lobby reconnects) constantly on the same
                                // tab; refreshing the handle is a no-op and
                                // must not spam warnings.
                                debug!(
                                    "autoplay: page handle refreshed on new WS for target {target_id} ({})",
                                    ev.url
                                );
                            } else {
                                if let Some(prev) = &prev_target {
                                    warn!(
                                        "autoplay: page handle moving from tab {prev} to {target_id}"
                                    );
                                    // The cached canvas rect belonged to the
                                    // old tab; a different tab may have
                                    // different geometry, so drop it and let
                                    // the manager re-query against the new page.
                                    *ctx.canvas_rect.write().await = None;
                                }
                                info!(
                                    "autoplay: page handle bound to target {target_id} via WS {}",
                                    ev.url
                                );
                            }
                        }
                    }
                }
                Some(ev) = on_recv.next() => {
                    for frame in ready_frames(WsFrame::Down(ev), &mut on_sent, &mut on_recv) {
                        route_ws_frame(&frame, &target_id, &bridges, &inspector, &mjai_bus);
                    }
                }
                Some(ev) = on_sent.next() => {
                    for frame in ready_frames(WsFrame::Up(ev), &mut on_sent, &mut on_recv) {
                        route_ws_frame(&frame, &target_id, &bridges, &inspector, &mjai_bus);
                    }
                }
                Some(ev) = on_closed.next() => {
                    let key = FlowKey {
                        target: target_id.clone(),
                        request: ev.request_id.inner().clone(),
                    };
                    debug!("ws closed: target={target_id} request={}", ev.request_id.inner());
                    // Synthetic empty bridge ref so we can call release.
                    // FlowBridges::release reaps the entry when no other
                    // direction's task is holding a clone.
                    let bridge = bridges.acquire(key.clone(), "ws", "ws frame");
                    bridges.release(&key, bridge);

                    // NB: we deliberately do NOT touch the autoplay page
                    // handle here. Majsoul closes short-lived Route-probe /
                    // lobby-reconnect sockets to *.maj-soul.com throughout a
                    // game while the real game-gateway socket stays open;
                    // dropping the handle on those closes silently stopped
                    // autoplay mid-game. The handle is tied to the tab and
                    // cleared by the poll loop when the tab itself closes.
                }
                Some(_) = on_resized.next() => {
                    // Only the tab autoplay clicks into has a canvas worth
                    // re-measuring.
                    if let Some(ctx) = &autoplay {
                        let owns = ctx
                            .page
                            .read()
                            .await
                            .as_ref()
                            .is_some_and(|p| p.id() == target_id);
                        if owns {
                            ctx.resize.note_resized();
                            debug!("autoplay: game tab {target_id} resized");
                        }
                    }
                }
                Some(ev) = on_request.next() => {
                    // Fires for every subresource the page loads — the
                    // asymmetry with the MITM leg, where a whole session
                    // is a couple of dozen requests. Filter first, and
                    // keep the recognizers' work off the hot path.
                    if is_static_asset(ev.r#type.as_ref()) && !http_cfg.static_assets {
                        continue;
                    }
                    let headers = headers_of(&ev.request.headers);
                    let annotations = annotate::annotate_request(&RequestView::new(
                        &ev.request.method,
                        &ev.request.url,
                        &headers,
                    ));
                    if let Some(a) = annotations.first() {
                        info!(
                            target: "mokora::capture::http",
                            "recognized {} {}", a.kind, a.summary,
                        );
                    }
                    if !http_cfg.record_all && annotations.is_empty() {
                        continue;
                    }
                    inspector.record(InspectorEntry::Http {
                        ts_ms: Local::now().timestamp_millis(),
                        source: CaptureSource::Chromium,
                        exchange: HttpExchange {
                            // CDP hands out a real request id, so pairing
                            // here is exact — unlike the MITM leg.
                            exchange_id: Some(ev.request_id.inner().clone()),
                            phase: HttpPhase::Request,
                            method: ev.request.method.clone(),
                            url: ev.request.url.clone(),
                            host: host_of(&ev.request.url),
                            version: String::new(),
                            status: None,
                            headers,
                            body: None,
                            annotations,
                        },
                    });
                }
                Some(ev) = on_response.next() => {
                    if !http_cfg.record_all {
                        continue;
                    }
                    if is_static_asset(Some(&ev.r#type)) && !http_cfg.static_assets {
                        continue;
                    }
                    inspector.record(InspectorEntry::Http {
                        ts_ms: Local::now().timestamp_millis(),
                        source: CaptureSource::Chromium,
                        exchange: HttpExchange {
                            exchange_id: Some(ev.request_id.inner().clone()),
                            phase: HttpPhase::Response,
                            method: String::new(),
                            url: ev.response.url.clone(),
                            host: host_of(&ev.response.url),
                            version: String::new(),
                            status: Some(ev.response.status as u16),
                            headers: headers_of(&ev.response.headers),
                            // Reading a body here costs a separate
                            // `Network.getResponseBody` round-trip per
                            // request, which the MITM leg does not need.
                            // Say so rather than look like there was none.
                            body: Some(HttpBody {
                                text: None,
                                bytes: None,
                                skipped: Some(
                                    "not captured on the chromium backend".to_string(),
                                ),
                            }),
                            annotations: Vec::new(),
                        },
                    });
                }
                else => break,
            }
        }
    });
    Ok(handle)
}

/// Subresource types that say nothing about the client and would bury
/// everything else. A WebGL game pulls thousands; the MITM leg never sees
/// them at all because the game fetches them outside the proxied path.
fn is_static_asset(kind: Option<&ResourceType>) -> bool {
    matches!(
        kind,
        Some(
            ResourceType::Image
                | ResourceType::Font
                | ResourceType::Media
                | ResourceType::Stylesheet
        )
    )
}

/// CDP delivers headers as a JSON object, which has no wire order to
/// preserve — unlike the MITM leg, where order is a real fingerprint.
/// Sorted so two captures of the same request compare equal.
fn headers_of(headers: &Headers) -> Vec<HttpHeader> {
    let Some(map) = headers.inner().as_object() else {
        return Vec::new();
    };
    let mut out: Vec<HttpHeader> = map
        .iter()
        .map(|(name, value)| HttpHeader {
            name: name.clone(),
            value: crate::bridge::redact::header_value(
                name,
                match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
            ),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Host of an absolute URL, or empty when it has none (`data:`, `blob:`).
fn host_of(url: &str) -> String {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|u| u.host().map(str::to_string))
        .unwrap_or_default()
}

/// Build a stable flow id for the inspector. Uses just the request id
/// (truncated, since CDP request ids are opaque hashes ~10 chars) for
/// brevity — the timeline already implicitly groups by flow because
/// frames from one connection arrive interleaved.
fn format_flow_id(key: &FlowKey) -> String {
    let req = &key.request;
    let trim = if req.len() > 10 { &req[..10] } else { req };
    format!("ws:{trim}")
}

/// Record one inspector `WsFrame` entry for a parsed frame. For text
/// frames (`opcode == 1`) `payload_data` is the original UTF-8 string —
/// we use it verbatim so the JSONL stays human-readable. For binary
/// frames (`opcode == 2`) we re-emit the original base64 (`payload_data`)
/// rather than re-encoding `payload`, which is identical content but
/// avoids a copy.
/// One WebSocket frame event, either way.
///
/// chromiumoxide hands sent and received frames out on two separate streams,
/// and the routing task polls them through `select!`, which picks a ready
/// branch at random. Taken one at a time, a request and its quick answer can
/// therefore come out answer first — the real logs showed answers dropped as
/// "no pending request". Every event carries the browser's own monotonic
/// timestamp, so [`in_wire_order`] puts a batch back in the order the browser
/// saw it.
enum WsFrame {
    Up(Arc<EventWebSocketFrameSent>),
    Down(Arc<EventWebSocketFrameReceived>),
}

impl WsFrame {
    fn timestamp(&self) -> f64 {
        match self {
            WsFrame::Up(ev) => *ev.timestamp.inner(),
            WsFrame::Down(ev) => *ev.timestamp.inner(),
        }
    }

    fn request_id(&self) -> &str {
        match self {
            WsFrame::Up(ev) => ev.request_id.inner(),
            WsFrame::Down(ev) => ev.request_id.inner(),
        }
    }

    fn response(&self) -> &chromiumoxide::cdp::browser_protocol::network::WebSocketFrame {
        match self {
            WsFrame::Up(ev) => &ev.response,
            WsFrame::Down(ev) => &ev.response,
        }
    }
}

/// `first` plus every frame already waiting on either stream, in the order
/// the browser saw them.
///
/// Draining both streams before parsing is what makes the order recoverable:
/// the browser emits a request's frame before its answer's, so by the time
/// the answer is ready to be read, the request is too (or within the
/// instant it takes the handler to file it — which the parser's hold for
/// early answers covers).
fn ready_frames<S, R>(first: WsFrame, sent: &mut S, received: &mut R) -> Vec<WsFrame>
where
    S: futures_util::Stream<Item = Arc<EventWebSocketFrameSent>> + Unpin,
    R: futures_util::Stream<Item = Arc<EventWebSocketFrameReceived>> + Unpin,
{
    use futures_util::FutureExt;
    let mut frames = vec![first];
    while let Some(Some(ev)) = sent.next().now_or_never() {
        frames.push(WsFrame::Up(ev));
    }
    while let Some(Some(ev)) = received.next().now_or_never() {
        frames.push(WsFrame::Down(ev));
    }
    in_wire_order(frames)
}

/// Sort by the browser's timestamp. The sort is stable, so frames with the
/// same timestamp keep the order their own stream gave them.
fn in_wire_order(mut frames: Vec<WsFrame>) -> Vec<WsFrame> {
    frames.sort_by(|a, b| a.timestamp().total_cmp(&b.timestamp()));
    frames
}

/// Parse one frame through its flow's bridge, record it, and publish what
/// came of it.
fn route_ws_frame(
    frame: &WsFrame,
    target_id: &str,
    bridges: &FlowBridges<FlowKey>,
    inspector: &InspectorWriter,
    mjai_bus: &MjaiBus,
) {
    let response = frame.response();
    let key = FlowKey {
        target: target_id.to_string(),
        request: frame.request_id().to_string(),
    };
    route_frame(
        matches!(frame, WsFrame::Up(_)),
        response.opcode as i64,
        &response.payload_data,
        key,
        bridges,
        inspector,
        mjai_bus,
    );
}

/// [`route_ws_frame`] for a frame however it was read: `up` from the page to
/// the server, `opcode` and `payload_data` as CDP gives them (text as is,
/// binary in base64), on the flow `key`. The WebKit capture on Linux reads
/// frames in the page and hands them in here, so both capture the same way.
pub(crate) fn route_frame(
    up: bool,
    opcode: i64,
    payload_data: &str,
    key: FlowKey,
    bridges: &FlowBridges<FlowKey>,
    inspector: &InspectorWriter,
    mjai_bus: &MjaiBus,
) {
    let (direction, frame_direction) = if up {
        (Direction::Up, FrameDirection::Up)
    } else {
        (Direction::Down, FrameDirection::Down)
    };
    let payload = match decode_frame_payload(opcode, payload_data) {
        FrameDecode::Bytes(b) => b,
        FrameDecode::BadBase64 => {
            warn!("base64 decode failed for {} WS frame", direction.as_str());
            return;
        }
        FrameDecode::Skip => return,
    };
    let flow_id = format_flow_id(&key);
    let bridge = bridges.acquire(key, "ws", "ws frame");
    let result = {
        let mut b = bridge.lock().expect("bridge mutex poisoned");
        b.parse(direction, &payload)
    };
    record_frame(
        inspector,
        frame_direction,
        flow_id,
        opcode,
        &payload,
        payload_data,
        &result,
    );
    for e in result.events {
        let _ = mjai_bus.send(e);
    }
}

fn record_frame(
    inspector: &InspectorWriter,
    direction: FrameDirection,
    flow_id: String,
    opcode: i64,
    payload: &[u8],
    payload_data: &str,
    result: &crate::bridge::ParseResult,
) {
    // The decoded view had credentials masked; the raw bytes still hold them.
    let raw = if result.redacted {
        FrameRaw::Text(crate::bridge::redact::RAW_PLACEHOLDER.to_string())
    } else if opcode == 1 {
        FrameRaw::Text(payload_data.to_string())
    } else {
        FrameRaw::Binary(payload_data.to_string())
    };
    inspector.record(InspectorEntry::WsFrame {
        ts_ms: Local::now().timestamp_millis(),
        direction,
        flow_id,
        size: payload.len(),
        raw,
        parsed: result.parsed.clone(),
        emitted: result.events.len(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame event as Chromium sends it, `ts` its monotonic timestamp.
    fn ws_event<T: serde::de::DeserializeOwned>(ts: f64, payload: &str) -> Arc<T> {
        Arc::new(
            serde_json::from_value(serde_json::json!({
                "requestId": "1",
                "timestamp": ts,
                "response": { "opcode": 2, "mask": false, "payloadData": payload },
            }))
            .unwrap(),
        )
    }

    fn order(frames: &[WsFrame]) -> Vec<(char, String)> {
        frames
            .iter()
            .map(|f| {
                let dir = if matches!(f, WsFrame::Up(_)) { 'U' } else { 'D' };
                (dir, f.response().payload_data.clone())
            })
            .collect()
    }

    /// The real case: the answer is the event in hand while its request is
    /// still waiting on the other stream. The batch must come out request
    /// first, then the answer, then whatever followed.
    #[test]
    fn ready_frames_restore_the_browsers_order() {
        let mut sent = futures_util::stream::iter(vec![ws_event(1.0, "req")]);
        let mut received = futures_util::stream::iter(vec![ws_event(3.0, "later")]);
        let first = WsFrame::Down(ws_event(2.0, "answer"));
        let frames = ready_frames(first, &mut sent, &mut received);
        assert_eq!(
            order(&frames),
            [
                ('U', "req".to_string()),
                ('D', "answer".to_string()),
                ('D', "later".to_string())
            ]
        );
    }

    /// Equal timestamps keep the order their own stream gave them.
    #[test]
    fn equal_timestamps_keep_stream_order() {
        let frames = in_wire_order(vec![
            WsFrame::Up(ws_event(5.0, "a")),
            WsFrame::Up(ws_event(5.0, "b")),
            WsFrame::Down(ws_event(4.0, "c")),
        ]);
        let got: Vec<String> = order(&frames).into_iter().map(|(_, p)| p).collect();
        assert_eq!(got, ["c", "a", "b"]);
    }

    /// The loose interception pattern also catches look-alikes; only the
    /// SLS service's own hosts are answered in place of the server.
    #[test]
    fn only_sls_hosts_count_as_telemetry() {
        let host = |u: &str| is_telemetry_host(&host_of(u));
        assert!(host(
            "https://hwmaj-client.ap-northeast-1.log.aliyuncs.com/logstores/client/track?x=1"
        ));
        assert!(!host("https://evil.example.com/?next=.log.aliyuncs.com/x"));
        assert!(!host("https://log.aliyuncs.com.evil.example.com/x"));
        assert!(!host("https://game.maj-soul.com/1/"));
    }

    #[test]
    fn decode_payload_ok() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"hello");
        assert_eq!(decode_payload(&b64), Some(b"hello".to_vec()));
    }

    #[test]
    fn decode_payload_bad() {
        assert_eq!(decode_payload("not-base64-!@#"), None);
    }

    #[test]
    fn diff_adds_and_removes() {
        let prev: HashSet<String> = ["a", "b", "c"].into_iter().map(String::from).collect();
        let current: HashSet<String> = ["b", "c", "d"].into_iter().map(String::from).collect();
        let (adds, removes) = diff_pages(&prev, &current);
        let mut adds = adds;
        let mut removes = removes;
        adds.sort();
        removes.sort();
        assert_eq!(adds, vec!["d"]);
        assert_eq!(removes, vec!["a"]);
    }

    #[test]
    fn diff_empty_when_unchanged() {
        let s: HashSet<String> = ["x", "y"].into_iter().map(String::from).collect();
        let (adds, removes) = diff_pages(&s, &s);
        assert!(adds.is_empty());
        assert!(removes.is_empty());
    }

    #[test]
    fn diff_initial_subscribe() {
        let prev: HashSet<String> = HashSet::new();
        let current: HashSet<String> = ["a", "b"].into_iter().map(String::from).collect();
        let (adds, removes) = diff_pages(&prev, &current);
        let mut adds = adds;
        adds.sort();
        assert_eq!(adds, vec!["a", "b"]);
        assert!(removes.is_empty());
    }

    /// Regression: a Majsoul Route-probe / lobby-reconnect socket closing
    /// must NOT clear the autoplay page handle — that was making autoplay
    /// silently stop mid-game. The handle is tied to the browser tab, so
    /// only the owning tab's removal from the page snapshot clears it.
    #[test]
    fn page_handle_cleared_only_when_owning_tab_closes() {
        let owner = Some("TAB_A");
        // A *different* tab closing (or a WS closing, which never reaches
        // this predicate at all) leaves our handle intact.
        assert!(!page_handle_cleared_by_removal(owner, &["TAB_B".into()]));
        // Nothing reaped this tick — keep the handle.
        assert!(!page_handle_cleared_by_removal(owner, &[]));
        // The owning tab itself disappearing is the only trigger.
        assert!(page_handle_cleared_by_removal(
            owner,
            &["TAB_B".into(), "TAB_A".into()]
        ));
        // No handle bound → nothing to clear regardless of what closed.
        assert!(!page_handle_cleared_by_removal(None, &["TAB_A".into()]));
    }

    /// Regression: prior code dropped every non-binary frame, which
    /// silently broke Tenhou capture (Tenhou uses opcode 1 / text).
    #[test]
    fn text_frame_passes_through_as_utf8_bytes() {
        let payload = r#"{"tag":"INIT","seed":"1,0,0,2,5,134"}"#;
        assert_eq!(
            decode_frame_payload(1, payload),
            FrameDecode::Bytes(payload.as_bytes().to_vec())
        );
    }

    #[test]
    fn text_heartbeat_passes_through() {
        // Tenhou's `<Z/>` heartbeat is a 4-byte text frame.
        assert_eq!(
            decode_frame_payload(1, "<Z/>"),
            FrameDecode::Bytes(b"<Z/>".to_vec())
        );
    }

    #[test]
    fn binary_frame_base64_decodes() {
        let raw = b"\x00\x01\x02hello";
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        assert_eq!(
            decode_frame_payload(2, &b64),
            FrameDecode::Bytes(raw.to_vec())
        );
    }

    #[test]
    fn binary_frame_bad_base64_signals_decode_error() {
        // Distinguished from `Skip` so the inline branch can WARN —
        // legit malformed CDP from Chrome shouldn't be confused with
        // an intentionally-ignored control frame.
        assert_eq!(
            decode_frame_payload(2, "not base64!@#"),
            FrameDecode::BadBase64
        );
    }

    #[test]
    fn control_and_continuation_frames_are_skipped() {
        for opcode in [0i64, 8, 9, 10] {
            assert_eq!(
                decode_frame_payload(opcode, "irrelevant"),
                FrameDecode::Skip,
                "opcode {opcode} should skip"
            );
        }
    }
}
