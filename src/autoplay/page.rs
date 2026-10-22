//! The game page autoplay drives, whichever browser shows it.
//!
//! Autoplay needs a handful of things from the page the game runs in: mouse
//! events, a script's value, a picture of part of the screen, a reload. A
//! browser read over the DevTools protocol (a launched Chromium, or the
//! WebView2 of Mokora's own window on Windows) does them through CDP; the
//! WebKitGTK view of Mokora's own window on Linux has no DevTools protocol
//! and does them through WebKit and GTK ([`crate::capture::webkit`]). Both
//! sit behind [`GamePage`], so autoplay's timing, aiming and checks are one
//! piece of code.

use anyhow::{anyhow, Context, Result};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType, MouseButton,
};
use chromiumoxide::layout::Point;
use chromiumoxide::page::Page;

/// One mouse event, as autoplay means it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseEvent {
    pub kind: MouseKind,
    /// CSS pixels from the page's top-left.
    pub x: f64,
    pub y: f64,
    /// The left button is the one the event is about (a press or release of
    /// it, or a move with it held).
    pub left: bool,
    /// Buttons held after the event, as the DOM's `buttons` (1 = left).
    /// `None` leaves it to the browser.
    pub buttons: Option<i64>,
    /// `Some(1)` for a single click's press and release.
    pub click_count: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Moved,
    Pressed,
    Released,
}

/// A part of the page, CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The page the game runs in. Cheap to clone: a handle.
#[derive(Clone)]
pub enum GamePage {
    /// Over the DevTools protocol.
    Cdp(Page),
    /// The WebKitGTK view of Mokora's own window (Linux).
    #[cfg(target_os = "linux")]
    WebKit(crate::capture::webkit::WebKitPage),
}

impl GamePage {
    /// Which tab or view this is, stable for its life.
    pub fn id(&self) -> String {
        match self {
            GamePage::Cdp(p) => p.target_id().inner().clone(),
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.id().to_string(),
        }
    }

    /// The page's current address.
    pub async fn url(&self) -> Option<String> {
        match self {
            GamePage::Cdp(p) => p.url().await.ok().flatten(),
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.url().await,
        }
    }

    /// Move the pointer to `(x, y)` with no button held.
    pub async fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        match self {
            GamePage::Cdp(p) => {
                p.move_mouse(Point::new(x, y))
                    .await
                    .context("CDP move_mouse")?;
                Ok(())
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => {
                w.mouse(MouseEvent {
                    kind: MouseKind::Moved,
                    x,
                    y,
                    left: false,
                    buttons: Some(0),
                    click_count: None,
                })
                .await
            }
        }
    }

    /// Send one mouse event.
    pub async fn mouse(&self, ev: MouseEvent) -> Result<()> {
        match self {
            GamePage::Cdp(p) => {
                let kind = match ev.kind {
                    MouseKind::Moved => DispatchMouseEventType::MouseMoved,
                    MouseKind::Pressed => DispatchMouseEventType::MousePressed,
                    MouseKind::Released => DispatchMouseEventType::MouseReleased,
                };
                let mut b = DispatchMouseEventParams::builder()
                    .r#type(kind)
                    .x(ev.x)
                    .y(ev.y);
                if ev.left {
                    b = b.button(MouseButton::Left);
                }
                if let Some(buttons) = ev.buttons {
                    b = b.buttons(buttons);
                }
                if let Some(n) = ev.click_count {
                    b = b.click_count(n);
                }
                let params = b.build().map_err(|e| anyhow!("build mouse event: {e}"))?;
                p.execute(params)
                    .await
                    .with_context(|| format!("CDP mouse {:?}", ev.kind))?;
                Ok(())
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.mouse(ev).await,
        }
    }

    /// One turn of the wheel over `(x, y)`, `delta_y` CSS pixels (positive
    /// scrolls down).
    pub async fn wheel(&self, x: f64, y: f64, delta_y: f64) -> Result<()> {
        match self {
            GamePage::Cdp(p) => {
                let wheel = DispatchMouseEventParams::builder()
                    .r#type(DispatchMouseEventType::MouseWheel)
                    .x(x)
                    .y(y)
                    .delta_x(0.0)
                    .delta_y(delta_y)
                    .build()
                    .map_err(|e| anyhow!("build mouseWheel: {e}"))?;
                p.execute(wheel).await.context("CDP mouseWheel")?;
                Ok(())
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.wheel(x, y, delta_y).await,
        }
    }

    /// The value of the script `expr` (an expression), as JSON; `Null` for
    /// `null` or `undefined`.
    pub async fn evaluate(&self, expr: &str) -> Result<serde_json::Value> {
        match self {
            GamePage::Cdp(p) => {
                let result = p.evaluate(expr).await.context("CDP evaluate")?;
                Ok(result.value().cloned().unwrap_or(serde_json::Value::Null))
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.evaluate(expr).await,
        }
    }

    /// A PNG of `clip`, or of the whole visible page, one image pixel per CSS
    /// pixel.
    pub async fn screenshot(&self, clip: Option<Clip>) -> Result<Vec<u8>> {
        match self {
            GamePage::Cdp(p) => {
                use chromiumoxide::cdp::browser_protocol::page::{
                    CaptureScreenshotFormat, Viewport,
                };
                use chromiumoxide::page::ScreenshotParams;
                let mut params = ScreenshotParams::builder().format(CaptureScreenshotFormat::Png);
                if let Some(c) = clip {
                    params = params.clip(Viewport {
                        x: c.x,
                        y: c.y,
                        width: c.width,
                        height: c.height,
                        scale: 1.0,
                    });
                }
                p.screenshot(params.build()).await.context("CDP screenshot")
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.screenshot(clip).await,
        }
    }

    /// Reload the page and wait until the new one has loaded.
    pub async fn reload(&self) -> Result<()> {
        match self {
            GamePage::Cdp(p) => {
                p.reload().await.context("CDP reload")?;
                Ok(())
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.reload().await,
        }
    }

    /// Start a reload and return at once.
    pub async fn start_reload(&self) -> Result<()> {
        match self {
            GamePage::Cdp(p) => {
                use chromiumoxide::cdp::browser_protocol::page::ReloadParams;
                p.execute(ReloadParams::default())
                    .await
                    .context("CDP reload")?;
                Ok(())
            }
            #[cfg(target_os = "linux")]
            GamePage::WebKit(w) => w.start_reload().await,
        }
    }
}
