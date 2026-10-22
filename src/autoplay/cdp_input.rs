//! The input and page reads autoplay makes, on top of [`GamePage`].
//!
//! Centralised so the click + canvas-rect query logic can be unit-mocked
//! and the manager keeps a single dependency on the page it drives.

use crate::autoplay::context::CanvasRect;
use crate::autoplay::page::{Clip, GamePage, MouseEvent, MouseKind};
use anyhow::{anyhow, Context, Result};
use std::time::Duration;

/// A press or release of the left button for one click at `(x, y)`.
fn click_event(kind: MouseKind, x: f64, y: f64) -> MouseEvent {
    MouseEvent {
        kind,
        x,
        y,
        left: true,
        buttons: None,
        click_count: Some(1),
    }
}

/// Dispatch a single mouse click at `(x, y)` (CSS pixels),
/// with a mandatory hover before the press:
///
/// 1. `mouseMoved` to `(x, y)`
/// 2. sleep `hover_delay_ms` (≥100ms — Laya's input system samples hover
///    state before mousedown registers a hit on a tile sprite)
/// 3. ask `proceed`; if it says no, stop here without pressing
/// 4. `mousePressed`
/// 5. sleep `click_hold_ms`
/// 6. `mouseReleased`
///
/// Returns whether the press went out.
///
/// `chromiumoxide::Page::click` collapses 4+6 into back-to-back frames
/// without the hover delay, which Majsoul drops on the floor for hand
/// tiles. Hand-rolling the sequence is required.
///
/// `proceed` sits as close to the press as it can because the hover is the
/// long part of a click, and so the window in which the screen changes under
/// the cursor: a window resize moves the canvas (see `autoplay::resize`), and
/// the result screen's confirm button is where the lobby has the shop.
///
/// `jiggle` nudges the cursor a pixel mid-press and puts it back. It exists
/// for retries: when a press lands on the right control and the action still
/// does not commit, the position is not what is wrong, so the only thing
/// left to change is how the press is made.
pub async fn dispatch_click_shaped_guarded(
    page: &GamePage,
    x: f64,
    y: f64,
    hover_delay_ms: u32,
    click_hold_ms: u32,
    jiggle: bool,
    proceed: impl Fn() -> bool,
) -> Result<bool> {
    page.move_mouse(x, y).await?;
    if hover_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(hover_delay_ms as u64)).await;
    }
    if !proceed() {
        return Ok(false);
    }

    page.mouse(click_event(MouseKind::Pressed, x, y)).await?;

    if jiggle {
        // Split the hold around the nudge so its total stays what the
        // config says. The move is one CSS pixel — enough for the engine
        // to resample the cursor while the button is down, not enough to
        // leave the control.
        let half = u64::from(click_hold_ms) / 2;
        if half > 0 {
            tokio::time::sleep(Duration::from_millis(half)).await;
        }
        page.move_mouse(x + 1.0, y).await.context("jiggle out")?;
        page.move_mouse(x, y).await.context("jiggle back")?;
        let rest = u64::from(click_hold_ms) - half;
        if rest > 0 {
            tokio::time::sleep(Duration::from_millis(rest)).await;
        }
    } else if click_hold_ms > 0 {
        tokio::time::sleep(Duration::from_millis(click_hold_ms as u64)).await;
    }

    page.mouse(click_event(MouseKind::Released, x, y)).await?;

    Ok(true)
}

/// [`dispatch_click_shaped_guarded`] with a plain press (no jiggle).
pub async fn dispatch_click_guarded(
    page: &GamePage,
    x: f64,
    y: f64,
    hover_delay_ms: u32,
    click_hold_ms: u32,
    proceed: impl Fn() -> bool,
) -> Result<bool> {
    dispatch_click_shaped_guarded(page, x, y, hover_delay_ms, click_hold_ms, false, proceed).await
}

/// Spin the mouse wheel over `(x, y)`: `ticks` events of `delta_y` CSS pixels
/// each, `gap_ms` apart. Positive scrolls down.
///
/// Used to move a list to an edge, so the total only has to be more than the
/// list is long — the list clamps, and the edge is the same wherever it
/// started. The cursor is moved there first because the engine routes a wheel
/// event to whatever is under the cursor it last saw.
pub async fn dispatch_wheel(
    page: &GamePage,
    x: f64,
    y: f64,
    delta_y: f64,
    ticks: u32,
    gap_ms: u32,
) -> Result<()> {
    page.move_mouse(x, y).await.context("wheel")?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    for _ in 0..ticks {
        page.wheel(x, y, delta_y).await?;
        tokio::time::sleep(Duration::from_millis(gap_ms as u64)).await;
    }
    Ok(())
}

/// Press at `(x, y_from)`, drag to `(x, y_to)` in `steps` moves, release.
///
/// The alternative to [`dispatch_wheel`] for a client that ignores the wheel.
/// Dragging *up* (`y_to < y_from`) scrolls the list towards its bottom. The
/// move events carry the left button as held, which is what tells the engine
/// a drag is under way rather than a hover.
pub async fn dispatch_drag(
    page: &GamePage,
    x: f64,
    y_from: f64,
    y_to: f64,
    steps: u32,
    step_delay_ms: u32,
) -> Result<()> {
    let steps = steps.max(1);
    page.move_mouse(x, y_from).await.context("drag start")?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    page.mouse(MouseEvent {
        kind: MouseKind::Pressed,
        x,
        y: y_from,
        left: true,
        buttons: Some(1),
        click_count: Some(1),
    })
    .await
    .context("drag press")?;
    for i in 1..=steps {
        let y = y_from + (y_to - y_from) * f64::from(i) / f64::from(steps);
        page.mouse(MouseEvent {
            kind: MouseKind::Moved,
            x,
            y,
            left: true,
            buttons: Some(1),
            click_count: None,
        })
        .await
        .context("drag move")?;
        tokio::time::sleep(Duration::from_millis(step_delay_ms as u64)).await;
    }
    page.mouse(MouseEvent {
        kind: MouseKind::Released,
        x,
        y: y_to,
        left: true,
        buttons: Some(0),
        click_count: Some(1),
    })
    .await
    .context("drag release")?;
    Ok(())
}

/// How many mouse presses the page has seen, from [`install_input_probe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct InputCounts {
    pub down: u64,
}

/// Start counting mouse presses (any button) in the page.
///
/// Autoplay drives the page with the same kind of events a person does, so
/// the page cannot say whose they are — but autoplay knows exactly what it
/// sent. Reading the count right after each of its own actions and again
/// before the next one, any difference is somebody else's hand on the mouse.
/// Only presses count: left, right and middle all raise `mousedown`, and
/// merely moving the mouse does not, so a person resting a hand on it is not
/// taking over. Idempotent: a page that already has the probe keeps its count.
pub async fn install_input_probe(page: &GamePage) -> Result<()> {
    let expr = "(()=>{if(window.__mokoraInput)return true;\
                const c={down:0};window.__mokoraInput=c;\
                const o={capture:true,passive:true};\
                addEventListener('mousedown',()=>{c.down++},o);\
                addEventListener('touchstart',()=>{c.down++},o);\
                return true;})()";
    page.evaluate(expr).await.context("install input probe")?;
    Ok(())
}

/// The probe's current count. `Err` when the page has no probe — it was
/// reloaded since [`install_input_probe`], which also zeroed the count.
pub async fn read_input_probe(page: &GamePage) -> Result<InputCounts> {
    let expr = "(()=>{const c=window.__mokoraInput;\
                return c?{down:c.down}:null;})()";
    let value = page.evaluate(expr).await.context("read input probe")?;
    if value.is_null() {
        return Err(anyhow!("input probe missing (the page was reloaded)"));
    }
    serde_json::from_value(value).context("input probe: deserialise count")
}

/// A decoded screenshot: 8-bit RGB(A), row-major.
struct Raster {
    height: usize,
    channels: usize,
    row_bytes: usize,
    data: Vec<u8>,
}

impl Raster {
    fn decode(png_bytes: &[u8]) -> Result<Self> {
        let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
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
        if info.width == 0 || info.height == 0 {
            return Err(anyhow!("empty screenshot"));
        }
        Ok(Self {
            height: info.height as usize,
            channels,
            row_bytes: info.width as usize * channels,
            data,
        })
    }

    /// The share of pixels in the rows `rows.0..rows.1` (fractions of the
    /// height) for which `pred(r, g, b)` holds.
    fn share(&self, rows: (f64, f64), pred: impl Fn(i32, i32, i32) -> bool) -> f64 {
        let h = self.height as f64;
        let y0 = ((rows.0 * h).floor() as usize).min(self.height - 1);
        let y1 = ((rows.1 * h).ceil() as usize).clamp(y0 + 1, self.height);
        let (mut hit, mut total) = (0usize, 0usize);
        for y in y0..y1 {
            let row = &self.data[y * self.row_bytes..(y + 1) * self.row_bytes];
            for p in row.chunks_exact(self.channels) {
                total += 1;
                if pred(i32::from(p[0]), i32::from(p[1]), i32::from(p[2])) {
                    hit += 1;
                }
            }
        }
        hit as f64 / total as f64
    }
}

/// The confirm button's yellow face.
fn is_button_yellow(r: i32, g: i32, b: i32) -> bool {
    r > 200 && g > 150 && b < 160 && r - b > 70
}

/// The dark blue of the lobby's 段位場 sign.
fn is_sign_navy(r: i32, g: i32, b: i32) -> bool {
    r < 75 && g < 95 && b > 55 && b < 160 && b - r > 28
}

/// The pink-brown of the lobby's 友人場 sign.
fn is_sign_warm(r: i32, g: i32, b: i32) -> bool {
    r > 110 && r - g > 25 && r - b > 5 && r < 235
}

/// The share of pixels in a PNG that are the confirm button's yellow
/// (bright, warm, low blue). Independent of the image's size.
pub fn yellow_fraction(png_bytes: &[u8]) -> Result<f64> {
    Ok(Raster::decode(png_bytes)?.share((0.0, 1.0), is_button_yellow))
}

/// The pale gold of the ranked lists' header labels ("<返回 段位場").
fn is_label_tan(r: i32, g: i32, b: i32) -> bool {
    r > 170 && g > 130 && b < 150 && r - b > 45 && r >= g
}

/// Share of a PNG that is the ranked lists' pale gold header text.
pub fn label_tan_fraction(png_bytes: &[u8]) -> Result<f64> {
    Ok(Raster::decode(png_bytes)?.share((0.0, 1.0), is_label_tan))
}

/// The promotion certificate's parchment: a muted, warm off-white.
fn is_parchment(r: i32, g: i32, b: i32) -> bool {
    r >= 140 && g >= 130 && b >= 105 && (12..=45).contains(&(r - b)) && g - b >= 8
}

/// The certificate's red 關閉 X.
fn is_close_red(r: i32, g: i32, b: i32) -> bool {
    r >= 170 && g <= 90 && b <= 90
}

/// The pale cyan of the bond story's 跳過 button (lettering and frame).
fn is_skip_cyan(r: i32, g: i32, b: i32) -> bool {
    g >= 180 && b >= 180 && g - r >= 25
}

/// Share of a PNG that is the bond story's 跳過 cyan.
pub fn skip_cyan_fraction(png_bytes: &[u8]) -> Result<f64> {
    Ok(Raster::decode(png_bytes)?.share((0.0, 1.0), is_skip_cyan))
}

/// `(parchment share, red share)` of the two certificate screenshots.
pub fn certificate_signature(parchment_png: &[u8], close_png: &[u8]) -> Result<(f64, f64)> {
    Ok((
        Raster::decode(parchment_png)?.share((0.0, 1.0), is_parchment),
        Raster::decode(close_png)?.share((0.0, 1.0), is_close_red),
    ))
}

/// How much of the lobby's two signs a strip screenshot shows:
/// `(navy share of the 段位場 rows, warm share of the 友人場 rows)`.
pub fn lobby_signature(
    png_bytes: &[u8],
    navy_rows: (f64, f64),
    warm_rows: (f64, f64),
) -> Result<(f64, f64)> {
    let raster = Raster::decode(png_bytes)?;
    Ok((
        raster.share(navy_rows, is_sign_navy),
        raster.share(warm_rows, is_sign_warm),
    ))
}

/// A PNG of the part of the canvas in `box_norm` (`(x_min, x_max, y_min,
/// y_max)` on the 16x9 grid), straight from the compositor.
pub async fn screenshot_box(
    page: &GamePage,
    rect: &CanvasRect,
    box_norm: (f64, f64, f64, f64),
) -> Result<Vec<u8>> {
    let (x0, y0) = rect.pixel(box_norm.0, box_norm.2);
    let (x1, y1) = rect.pixel(box_norm.1, box_norm.3);
    page.screenshot(Some(Clip {
        x: x0,
        y: y0,
        width: (x1 - x0).max(1.0),
        height: (y1 - y0).max(1.0),
    }))
    .await
}

/// Is the result screens' confirm button on screen right now?
///
/// The canvas gives nothing to read, and the client's own traffic turned out
/// not to say which screen it is on (its account refresh arrives ~24s after
/// every game whether or not a button has been pressed, and walking through
/// to the lobby sends nothing at all), so this looks: a small screenshot of
/// where the button sits, and whether it is the button's yellow there. The
/// lobby that takes its place is nowhere near it.
pub async fn result_button_visible(
    page: &GamePage,
    rect: &CanvasRect,
    box_norm: (f64, f64, f64, f64),
    min_yellow: f64,
) -> Result<bool> {
    let bytes = screenshot_box(page, rect, box_norm).await?;
    Ok(yellow_fraction(&bytes)? >= min_yellow)
}

/// Read the game canvas's `getBoundingClientRect()` via `Runtime.evaluate`.
///
/// Majsoul renders into the first `<canvas>` element on the page. We grab the
/// first canvas indiscriminately — multi-canvas pages aren't a thing there.
pub async fn evaluate_canvas_rect(page: &GamePage) -> Result<CanvasRect> {
    Ok(evaluate_canvas_layout(page).await?.0)
}

/// The canvas rect plus the viewport it sits in (`innerWidth` ×
/// `innerHeight`, CSS pixels), read in one `Runtime.evaluate` so the two
/// describe the same moment — what tells a canvas the page has laid out
/// for the current window from one it has yet to (see
/// `autoplay::resize::fits_viewport`).
pub async fn evaluate_canvas_layout(page: &GamePage) -> Result<(CanvasRect, (f64, f64))> {
    // IIFE so the page returns a single value, not a Promise.
    // `is_likely_js_function` in chromiumoxide picks the right CDP call
    // based on whether the expression looks like a function — we wrap
    // in `(()=>{...})()` to ensure plain-expression evaluation.
    let expr = "(()=>{const c=document.getElementsByTagName('canvas')[0];\
                if(!c)return null;\
                const r=c.getBoundingClientRect();\
                return {x:r.x,y:r.y,width:r.width,height:r.height,\
                vw:innerWidth,vh:innerHeight};})()";
    let value = page.evaluate(expr).await.context("evaluate canvas rect")?;
    if value.is_null() {
        return Err(anyhow!("canvas rect: page has no <canvas> element"));
    }
    let rect: CanvasRect = serde_json::from_value(value.clone())
        .context("canvas rect: deserialise from page value")?;
    let viewport = (
        value
            .get("vw")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        value
            .get("vh")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
    );
    Ok((rect, viewport))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, width, height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            let data: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
            w.write_image_data(&data).unwrap();
        }
        out
    }

    /// The thresholds are full auto's, in the paid module's map of the lobby:
    /// the tests that pin them to real screens run with it
    /// (`MOKORA_PRO_DLL=<a built mokora_pro library> cargo test -- --ignored`).
    fn layout() -> std::sync::Arc<pro_api::lobby::Layout> {
        assert!(crate::pro::load_for_tests(), "set MOKORA_PRO_DLL");
        crate::pro::lobby_layout().expect("the module answers its layout")
    }

    /// The colours of the real screens: the button's face, and what sits in
    /// its place in the lobby (the pink and white of the gacha cat).
    #[test]
    fn the_buttons_yellow_is_told_from_the_lobby() {
        let button = png_of(8, 4, [245, 215, 100, 255]);
        let lobby = png_of(8, 4, [169, 112, 106, 255]);
        assert!(yellow_fraction(&button).unwrap() > 0.99);
        assert!(yellow_fraction(&lobby).unwrap() < 0.01);
    }

    #[test]
    fn a_non_image_is_an_error_not_a_zero() {
        assert!(yellow_fraction(b"not a png").is_err());
    }

    /// Cut from real screens of the client, at the confirm button's box
    /// (the layout's `result_button_box`): the result screens (standings, rewards
    /// with the blue 再來一場 beside the button) show the button; the 再來一場
    /// dialog dims it; the lobby has the gacha cat there. Pins the threshold
    /// to what the client really draws.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn real_screens_are_told_apart_at_the_threshold() {
        let MIN = layout().result_button_min_yellow;
        let shown = |bytes: &[u8]| yellow_fraction(bytes).unwrap();
        let standings = shown(include_bytes!(
            "../../tests/fixtures/next_game/standings.png"
        ));
        let rewards = shown(include_bytes!(
            "../../tests/fixtures/next_game/rewards_with_again_button.png"
        ));
        let dialog = shown(include_bytes!(
            "../../tests/fixtures/next_game/again_dialog_open.png"
        ));
        let lobby = shown(include_bytes!("../../tests/fixtures/next_game/lobby.png"));
        assert!(standings >= MIN, "standings {standings}");
        assert!(rewards >= MIN, "rewards {rewards}");
        assert!(
            dialog < MIN,
            "the dimmed button must not be pressed: {dialog}"
        );
        assert!(lobby < MIN, "the lobby must not be pressed into: {lobby}");
    }

    /// The "銅幣不足無法入場" dialog's 確認, cut from a real screen at
    /// the layout's `coins_dialog_box`, against the screens it is checked over:
    /// the room list it pops up on and the activities page.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn the_coins_dialog_is_told_from_the_screens_around_it() {
        let MIN = layout().coins_dialog_min_yellow;
        let shown = |bytes: &[u8]| yellow_fraction(bytes).unwrap();
        let open = shown(include_bytes!(
            "../../tests/fixtures/next_game/coins_dialog_open.png"
        ));
        let rooms = shown(include_bytes!(
            "../../tests/fixtures/next_game/coins_dialog_room_list.png"
        ));
        let revive = shown(include_bytes!(
            "../../tests/fixtures/next_game/coins_dialog_revive_page.png"
        ));
        assert!(open >= MIN, "dialog {open}");
        assert!(rooms < MIN, "room list {rooms}");
        assert!(revive < MIN, "activities page {revive}");
    }

    /// The bond story's 跳過 button, cut from a real screen at
    /// the layout's `bond_skip_box`, against the other screens' boxes: none of
    /// them carries its pale cyan.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn the_bond_storys_skip_button_is_told_from_other_screens() {
        let MIN = layout().bond_skip_min_cyan;
        let cyan = |bytes: &[u8]| skip_cyan_fraction(bytes).unwrap();
        let story = cyan(include_bytes!(
            "../../tests/fixtures/next_game/bond_skip_story.png"
        ));
        assert!(story >= MIN, "story {story}");
        for (name, bytes) in [
            (
                "standings",
                &include_bytes!("../../tests/fixtures/next_game/standings.png")[..],
            ),
            (
                "lobby",
                include_bytes!("../../tests/fixtures/next_game/lobby.png"),
            ),
            (
                "certificate",
                include_bytes!("../../tests/fixtures/next_game/cert_close_rank_certificate.png"),
            ),
            (
                "standings top right",
                include_bytes!("../../tests/fixtures/next_game/cert_close_standings.png"),
            ),
            (
                "lobby strip",
                include_bytes!("../../tests/fixtures/next_game/strip_lobby_sakura.png"),
            ),
        ] {
            assert!(cyan(bytes) < MIN, "{name} {}", cyan(bytes));
        }
    }

    /// The opened promotion certificate is told from the rolled-up scroll
    /// before it and the standings after it — boxes cut from real screens at
    /// `RANK_CERT_PARCHMENT_BOX` / `RANK_CERT_CLOSE_BOX`.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn the_rank_certificate_is_told_from_the_screens_around_it() {
        let lay = layout();
        let (RANK_CERT_MIN_PARCHMENT, RANK_CERT_MIN_RED) = (lay.rank_cert_min_parchment, lay.rank_cert_min_red);
        let open = |parchment: &[u8], close: &[u8]| {
            let (p, r) = certificate_signature(parchment, close).unwrap();
            p >= RANK_CERT_MIN_PARCHMENT && r >= RANK_CERT_MIN_RED
        };
        assert!(open(
            include_bytes!("../../tests/fixtures/next_game/cert_parchment_rank_certificate.png"),
            include_bytes!("../../tests/fixtures/next_game/cert_close_rank_certificate.png"),
        ));
        assert!(!open(
            include_bytes!("../../tests/fixtures/next_game/cert_parchment_rank_scroll.png"),
            include_bytes!("../../tests/fixtures/next_game/cert_close_rank_scroll.png"),
        ));
        assert!(!open(
            include_bytes!("../../tests/fixtures/next_game/cert_parchment_standings.png"),
            include_bytes!("../../tests/fixtures/next_game/cert_close_standings.png"),
        ));
    }

    /// The lobby's main screen, on either of two different backgrounds, is
    /// told from a dialog over it, the room list, and every screen that
    /// leads up to it — strips cut from real screens at the map's `lobby_strip`.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn the_lobby_is_told_from_everything_else() {
        let lay = layout();
        let (LOBBY_NAVY_MIN, LOBBY_NAVY_ROWS, LOBBY_WARM_MIN, LOBBY_WARM_ROWS) =
            (lay.lobby_navy_min, lay.lobby_navy_rows, lay.lobby_warm_min, lay.lobby_warm_rows);
        let is_lobby = |bytes: &[u8]| {
            let (navy, warm) = lobby_signature(bytes, LOBBY_NAVY_ROWS, LOBBY_WARM_ROWS).unwrap();
            navy >= LOBBY_NAVY_MIN && warm >= LOBBY_WARM_MIN
        };
        assert!(is_lobby(include_bytes!(
            "../../tests/fixtures/next_game/strip_lobby_sakura.png"
        )));
        assert!(is_lobby(include_bytes!(
            "../../tests/fixtures/next_game/strip_lobby_temple.png"
        )));
        assert!(
            !is_lobby(include_bytes!(
                "../../tests/fixtures/next_game/strip_lobby_with_dialog.png"
            )),
            "a dialog over the lobby blocks every press under it"
        );
        assert!(!is_lobby(include_bytes!(
            "../../tests/fixtures/next_game/strip_standings.png"
        )));
        assert!(
            !is_lobby(include_bytes!(
                "../../tests/fixtures/next_game/strip_again_dialog.png"
            )),
            "the dialog's navy is not the sign's"
        );
        assert!(!is_lobby(include_bytes!(
            "../../tests/fixtures/next_game/strip_room_list.png"
        )));
    }

    /// The ranked lists' "<返回" is told by its pale gold: 0.23 on the room
    /// list, 0.03 or less on the lobby, a dialog over it and the login.
    #[test]
    #[ignore = "needs the paid module (MOKORA_PRO_DLL)"]
    #[allow(non_snake_case)]
    fn the_back_label_shows_only_on_the_ranked_lists() {
        let LIST_BACK_MIN_TAN = layout().list_back_min_tan;
        let tan = |png: &[u8]| label_tan_fraction(png).unwrap();
        assert!(
            tan(include_bytes!(
                "../../tests/fixtures/next_game/back_room_list.png"
            )) >= LIST_BACK_MIN_TAN
        );
        for png in [
            &include_bytes!("../../tests/fixtures/next_game/back_lobby.png")[..],
            &include_bytes!("../../tests/fixtures/next_game/back_settings_dialog.png")[..],
            &include_bytes!("../../tests/fixtures/next_game/back_login.png")[..],
        ] {
            assert!(tan(png) < LIST_BACK_MIN_TAN);
        }
    }
}
