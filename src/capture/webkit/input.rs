//! Mouse and wheel input for the WebKitGTK game view, as GTK events.
//!
//! Each event is built as a GDK event on the view's own window and handed to
//! GTK's dispatcher, which delivers it to the view as if the windowing system
//! had: WebKit turns it into the DOM events a hand on the mouse makes
//! (trusted, with the usual `buttons` and hover). The real pointer is never
//! moved, so the person at the machine keeps theirs, and a game parked off
//! the window's edge is reached all the same.
//!
//! Main thread only (GTK).

use crate::autoplay::page::{MouseEvent, MouseKind};
use anyhow::{anyhow, Result};
use glib::translate::ToGlibPtr;
use gtk::prelude::*;

/// CSS pixels one GDK smooth-scroll step becomes in WebKitGTK's DOM `wheel`
/// event, at the least: a `delta_y` of pixels is sent as
/// `delta_y / WHEEL_PIXELS_PER_STEP` steps. Measured on WebKitGTK 2.50, the
/// page saw 1.0× to 1.5× the pixels asked for (linear within a run, the
/// factor varying between runs) — never fewer. Autoplay turns the wheel to
/// push a list to its edge, where more than asked is harmless and fewer
/// would stop short.
pub const WHEEL_PIXELS_PER_STEP: f64 = 62.0;

/// The view's window and the pointer device events claim to come from.
struct Target {
    window: gtk::gdk::Window,
    pointer: Option<gtk::gdk::Device>,
}

fn target(view: &webkit2gtk::WebView) -> Result<Target> {
    if !view.has_window() {
        return Err(anyhow!("the game view has no window of its own"));
    }
    let window = view
        .window()
        .ok_or_else(|| anyhow!("the game view is not realized"))?;
    let pointer = window.display().default_seat().and_then(|s| s.pointer());
    Ok(Target { window, pointer })
}

/// Milliseconds on GTK's clock, as event times are.
fn now_ms() -> u32 {
    (glib::monotonic_time() / 1000) as u32
}

/// Fill the fields every pointer event shares, hand the event to GTK, free it.
///
/// # Safety
/// `event` was made by `gdk_event_new` with a type whose struct starts with
/// the shared fields `fill` writes, and is not used after this call.
unsafe fn dispatch(
    t: &Target,
    event: *mut gdk_sys::GdkEvent,
    fill: impl FnOnce(*mut gdk_sys::GdkEvent, *mut gdk_sys::GdkWindow, u32, f64, f64),
    x: f64,
    y: f64,
) {
    let (rx, ry) = t.window.root_coords(x.round() as i32, y.round() as i32);
    // The event owns this reference; `gdk_event_free` drops it.
    let window: *mut gdk_sys::GdkWindow = t.window.to_glib_full();
    fill(event, window, now_ms(), f64::from(rx), f64::from(ry));
    if let Some(device) = &t.pointer {
        gdk_sys::gdk_event_set_device(event, device.to_glib_none().0);
    }
    gtk_sys::gtk_main_do_event(event);
    gdk_sys::gdk_event_free(event);
}

/// Deliver `ev` to `view`.
pub fn mouse(view: &webkit2gtk::WebView, ev: MouseEvent) -> Result<()> {
    let t = target(view)?;
    let held = ev.buttons.unwrap_or(0) & 1 == 1;
    unsafe {
        match ev.kind {
            MouseKind::Moved => {
                let e = gdk_sys::gdk_event_new(gdk_sys::GDK_MOTION_NOTIFY);
                dispatch(
                    &t,
                    e,
                    |e, window, time, x_root, y_root| {
                        let m = e.cast::<gdk_sys::GdkEventMotion>();
                        (*m).window = window;
                        (*m).send_event = 0;
                        (*m).time = time;
                        (*m).x = ev.x;
                        (*m).y = ev.y;
                        (*m).state = if held { gdk_sys::GDK_BUTTON1_MASK } else { 0 };
                        (*m).is_hint = 0;
                        (*m).x_root = x_root;
                        (*m).y_root = y_root;
                    },
                    ev.x,
                    ev.y,
                );
            }
            MouseKind::Pressed | MouseKind::Released => {
                let pressed = ev.kind == MouseKind::Pressed;
                let kind = if pressed {
                    gdk_sys::GDK_BUTTON_PRESS
                } else {
                    gdk_sys::GDK_BUTTON_RELEASE
                };
                let e = gdk_sys::gdk_event_new(kind);
                dispatch(
                    &t,
                    e,
                    |e, window, time, x_root, y_root| {
                        let b = e.cast::<gdk_sys::GdkEventButton>();
                        (*b).window = window;
                        (*b).send_event = 0;
                        (*b).time = time;
                        (*b).x = ev.x;
                        (*b).y = ev.y;
                        // The modifier state is the one before the event: a
                        // release happens with the button still down.
                        (*b).state = if pressed {
                            0
                        } else {
                            gdk_sys::GDK_BUTTON1_MASK
                        };
                        (*b).button = 1;
                        (*b).x_root = x_root;
                        (*b).y_root = y_root;
                    },
                    ev.x,
                    ev.y,
                );
            }
        }
    }
    Ok(())
}

/// One wheel turn of `delta_y` CSS pixels over `(x, y)` (positive scrolls
/// down), as a smooth-scroll event.
pub fn wheel(view: &webkit2gtk::WebView, x: f64, y: f64, delta_y: f64) -> Result<()> {
    let t = target(view)?;
    unsafe {
        let e = gdk_sys::gdk_event_new(gdk_sys::GDK_SCROLL);
        dispatch(
            &t,
            e,
            |e, window, time, x_root, y_root| {
                let s = e.cast::<gdk_sys::GdkEventScroll>();
                (*s).window = window;
                (*s).send_event = 0;
                (*s).time = time;
                (*s).x = x;
                (*s).y = y;
                (*s).state = 0;
                (*s).direction = gdk_sys::GDK_SCROLL_SMOOTH;
                (*s).x_root = x_root;
                (*s).y_root = y_root;
                (*s).delta_x = 0.0;
                (*s).delta_y = delta_y / WHEEL_PIXELS_PER_STEP;
                (*s).is_stop = 0;
            },
            x,
            y,
        );
    }
    Ok(())
}
