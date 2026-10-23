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

    /// A PNG of `clip` (one image pixel per CSS pixel), or of the whole
    /// visible page.
    pub async fn screenshot(&self, clip: Option<Clip>) -> Result<Vec<u8>> {
        match self {
            GamePage::Cdp(p) => {
                use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
                use chromiumoxide::page::ScreenshotParams;
                // Always the whole page, cut here: asked for a part, Chromium
                // repaints the page for it and the player sees the game flash
                // white on every capture (measured 2026-10-23: every clipped
                // capture flashed, no whole-page one did).
                let params = ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build();
                let full = p.screenshot(params).await.context("CDP screenshot")?;
                let Some(c) = clip else { return Ok(full) };
                let css_width = p
                    .evaluate("window.innerWidth")
                    .await
                    .context("CDP evaluate innerWidth")?
                    .into_value::<f64>()
                    .context("innerWidth")?;
                crop_png(&full, css_width, c)
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

/// The part `clip` (CSS pixels) of the PNG `png` of a page `css_width` CSS
/// pixels wide, as a PNG of one pixel per CSS pixel (nearest pixel).
fn crop_png(png: &[u8], css_width: f64, clip: Clip) -> Result<Vec<u8>> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .context("decode screenshot")?;
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut data)
        .context("decode screenshot frame")?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(anyhow!("unexpected screenshot colour type {other:?}")),
    };
    if info.bit_depth != png::BitDepth::Eight {
        return Err(anyhow!(
            "unexpected screenshot bit depth {:?}",
            info.bit_depth
        ));
    }
    let (src_w, src_h) = (info.width as usize, info.height as usize);
    if src_w == 0 || src_h == 0 || css_width.is_nan() || css_width <= 0.0 {
        return Err(anyhow!("empty screenshot"));
    }
    // Image pixels per CSS pixel (the display's scaling).
    let scale = src_w as f64 / css_width;
    let (w, h) = (
        clip.width.round().max(1.0) as usize,
        clip.height.round().max(1.0) as usize,
    );
    let mut out = Vec::with_capacity(w * h * channels);
    for row in 0..h {
        let sy = (((clip.y + row as f64 + 0.5) * scale) as usize).min(src_h - 1);
        for col in 0..w {
            let sx = (((clip.x + col as f64 + 0.5) * scale) as usize).min(src_w - 1);
            let at = (sy * src_w + sx) * channels;
            out.extend_from_slice(&data[at..at + channels]);
        }
    }
    let mut encoded = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut encoded, w as u32, h as u32);
        enc.set_color(info.color_type);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().context("encode crop")?;
        writer.write_image_data(&out).context("encode crop")?;
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An RGB PNG `w`x`h` whose pixel (x, y) is `(x, y, 7)`.
    fn gradient(w: u32, h: u32) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[x as u8, y as u8, 7]);
            }
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
        out
    }

    fn pixels(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut r = png::Decoder::new(std::io::Cursor::new(png_bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        (info.width, info.height, buf)
    }

    #[test]
    fn a_crop_at_one_image_pixel_per_css_pixel() {
        let clip = Clip {
            x: 10.0,
            y: 20.0,
            width: 3.0,
            height: 2.0,
        };
        let (w, h, px) = pixels(&crop_png(&gradient(100, 50), 100.0, clip).unwrap());
        assert_eq!((w, h), (3, 2));
        assert_eq!(&px[..3], &[10, 20, 7]);
        assert_eq!(&px[px.len() - 3..], &[12, 21, 7]);
    }

    #[test]
    fn a_scaled_display_is_cut_in_css_pixels() {
        // A 1.5x display: the page is 100 CSS pixels wide, the picture 150.
        let clip = Clip {
            x: 10.0,
            y: 20.0,
            width: 4.0,
            height: 2.0,
        };
        let (w, h, px) = pixels(&crop_png(&gradient(150, 75), 100.0, clip).unwrap());
        assert_eq!((w, h), (4, 2));
        // CSS (10.5, 20.5) is image (15.75, 30.75).
        assert_eq!(&px[..3], &[15, 30, 7]);
    }

    #[test]
    fn a_clip_past_the_edge_stays_inside_the_picture() {
        let clip = Clip {
            x: 98.0,
            y: 48.0,
            width: 5.0,
            height: 5.0,
        };
        let (w, h, px) = pixels(&crop_png(&gradient(100, 50), 100.0, clip).unwrap());
        assert_eq!((w, h), (5, 5));
        assert_eq!(&px[px.len() - 3..], &[99, 49, 7]);
    }
}
