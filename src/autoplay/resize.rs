//! Has the game window changed size since the canvas was measured?
//!
//! Clicks are aimed through a cached `CanvasRect` (one `Runtime.evaluate`
//! per measurement). Resizing the window moves and scales the canvas — the
//! page letterboxes it to 16:9 — so a rect measured before the resize aims
//! every later click, retries included, at where the buttons used to be.
//!
//! The browser says when it happens: CDP's `Page.frameResized` fires on the
//! game tab within a frame of the window changing size, with nothing
//! injected into the page. The capture backend bumps a counter here on each
//! one; the autoplay manager records the count with every measurement and,
//! right before a click, re-measures only if the count has moved since. A
//! click with no resize behind it costs one atomic load.
//!
//! A drag-resize is a burst of events, and the page re-lays the canvas out
//! after each. The time of the latest one says whether the window is still
//! moving: a measurement taken mid-drag is stale a frame later, so the
//! manager waits for [`SETTLE`] of quiet first (bounded by [`SETTLE_MAX`],
//! so a decision is never held hostage by a window being dragged around).
//!
//! Quiet is not the same as laid out. Majsoul's page sizes the canvas from
//! a `resize` listener that defers the work by 100 ms (`setTimeout(
//! checkOrientation, 100)` → `applyLandscapeMode`), longer still when the
//! game is busy drawing, so a canvas measured right after the window stops
//! can still have the old size. The page's own formula says what size it
//! is heading for — the largest 16:9 box that fits the viewport, floored to
//! whole pixels (`calculate16by9Size`) — and [`fits_viewport`] checks a
//! measurement against it: until it matches, measure again.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::autoplay::context::CanvasRect;

/// Quiet time after the latest resize before the canvas counts as laid out.
pub const SETTLE: Duration = Duration::from_millis(100);

/// The longest a click waits for a window that keeps resizing.
pub const SETTLE_MAX: Duration = Duration::from_secs(1);

/// Counts window resizes of the game tab.
#[derive(Debug, Default)]
pub struct ResizeWatch {
    generation: AtomicU64,
    last: Mutex<Option<Instant>>,
}

impl ResizeWatch {
    /// The capture backend saw the game tab resize.
    pub fn note_resized(&self) {
        self.note_resized_at(Instant::now());
    }

    fn note_resized_at(&self, at: Instant) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(at);
        }
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// How many resizes have been seen. A measurement tagged with an older
    /// count was taken before the latest one.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// How much longer to wait before the window has been still for
    /// [`SETTLE`]; `None` once it has (or it never resized).
    pub fn settle_remaining(&self, now: Instant) -> Option<Duration> {
        let last = (*self.last.lock().ok()?)?;
        SETTLE
            .checked_sub(now.saturating_duration_since(last))
            .filter(|d| !d.is_zero())
    }
}

pub type SharedResizeWatch = Arc<ResizeWatch>;

/// How long to wait between measurements while the page catches up.
pub const LAYOUT_POLL: Duration = Duration::from_millis(50);

/// Is `rect` the canvas Majsoul's page lays out for a `vw` × `vh` viewport —
/// the largest 16:9 box that fits, floored to whole pixels? A pixel of
/// slack for the fractional sizes a zoomed page reports.
pub fn fits_viewport(rect: &CanvasRect, (vw, vh): (f64, f64)) -> bool {
    if vw <= 0.0 || vh <= 0.0 {
        return false;
    }
    let (w, h) = if vw / vh > 16.0 / 9.0 {
        (vh * 16.0 / 9.0, vh)
    } else {
        (vw, vw * 9.0 / 16.0)
    };
    (rect.width - w.floor()).abs() <= 1.0 && (rect.height - h.floor()).abs() <= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(width: f64, height: f64) -> CanvasRect {
        CanvasRect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    #[test]
    fn a_laid_out_canvas_fits_its_viewport() {
        // Measured on the live client: a 500 × 1310 window letterboxes the
        // canvas to 500 × 281 (281.25 floored).
        assert!(fits_viewport(&rect(500.0, 281.0), (500.0, 1310.0)));
        // Wider than 16:9: the height decides.
        assert!(fits_viewport(&rect(1600.0, 900.0), (1920.0, 900.0)));
        assert!(fits_viewport(&rect(1920.0, 1080.0), (1920.0, 1080.0)));
    }

    #[test]
    fn a_canvas_from_before_the_resize_does_not() {
        // Window just grew from 1280 × 720; the page has yet to follow.
        assert!(!fits_viewport(&rect(1280.0, 720.0), (1600.0, 900.0)));
        assert!(!fits_viewport(&rect(500.0, 281.0), (0.0, 0.0)));
    }

    #[test]
    fn each_resize_moves_the_generation() {
        let w = ResizeWatch::default();
        assert_eq!(w.generation(), 0);
        w.note_resized();
        w.note_resized();
        assert_eq!(w.generation(), 2);
    }

    #[test]
    fn a_window_that_never_resized_is_settled() {
        let w = ResizeWatch::default();
        assert_eq!(w.settle_remaining(Instant::now()), None);
    }

    #[test]
    fn settles_once_quiet_for_the_settle_time() {
        let w = ResizeWatch::default();
        let t0 = Instant::now();
        w.note_resized_at(t0);
        assert_eq!(w.settle_remaining(t0), Some(SETTLE));
        assert_eq!(
            w.settle_remaining(t0 + Duration::from_millis(40)),
            Some(SETTLE - Duration::from_millis(40))
        );
        assert_eq!(w.settle_remaining(t0 + SETTLE), None);
        // Another resize mid-drag starts the quiet time over.
        w.note_resized_at(t0 + Duration::from_millis(80));
        assert_eq!(
            w.settle_remaining(t0 + SETTLE),
            Some(Duration::from_millis(80))
        );
    }
}
