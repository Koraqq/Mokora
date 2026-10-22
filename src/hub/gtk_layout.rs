//! Where the web views sit in the window on Linux.
//!
//! Tauri puts a window's child web views on Linux into the window's GTK box,
//! which stacks them one under the other and ignores the bounds they are
//! given (wry places a view only in a `GtkFixed`). The window here needs the
//! interface to fill it and the game to sit over the interface's placeholder,
//! or off the window's edge, as on Windows. So each view is moved, as soon
//! as it is made, into a `GtkLayout` that fills the window, and placed there
//! by this module: interface views follow the layout's size, the game goes
//! where [`place`] puts it.
//!
//! A `GtkLayout` rather than a `GtkFixed`: a fixed container asks for the
//! space its children take, so a full-size interface view would stop the
//! window from shrinking; a layout's size is its own.
//!
//! Each web view has a GDK window of its own, and showing a hidden one raises
//! it over its siblings: an interface shown after its game would cover the
//! game. So the game is raised whenever it is placed, and again whenever an
//! interface is shown ([`raise`]).
//!
//! Main thread only (GTK); every entry point hands its work over through
//! `Webview::with_webview`, which keeps the order calls are made in.

use gtk::prelude::*;
use std::cell::RefCell;

/// Name given to interface views, which fill the layout.
const FILLS: &str = "mokora-fills";

thread_local! {
    /// The one window's layout, made with its first view.
    static LAYOUT: RefCell<Option<gtk::Layout>> = const { RefCell::new(None) };
}

fn layout_of(window: &tauri::Window) -> Option<gtk::Layout> {
    LAYOUT.with(|cell| {
        if let Some(layout) = cell.borrow().as_ref() {
            return Some(layout.clone());
        }
        let vbox = window.default_vbox().ok()?;
        let layout = gtk::Layout::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        vbox.pack_start(&layout, true, true, 0);
        layout.connect_size_allocate(|layout, a| {
            // A size asked for in the middle of an allocation is not acted on;
            // ask once this one is over.
            let (layout, width, height) = (layout.clone(), a.width(), a.height());
            glib::idle_add_local_once(move || {
                for child in layout.children() {
                    if child.widget_name() == FILLS
                        && (child.allocated_width(), child.allocated_height()) != (width, height)
                    {
                        child.set_size_request(width, height);
                    }
                }
            });
        });
        layout.show();
        *cell.borrow_mut() = Some(layout.clone());
        Some(layout)
    })
}

/// Move `view` (just made by `add_child`) into the window's layout at
/// `(x, y)`, `width` × `height` logical pixels; with `fills`, it follows the
/// layout's size from then on.
pub fn adopt(
    window: &tauri::Window,
    view: &tauri::Webview,
    fills: bool,
    rect: super::views::Bounds,
) {
    let window = window.clone();
    let _ = view.with_webview(move |w| {
        let widget: gtk::Widget = w.inner().upcast();
        let Some(layout) = layout_of(&window) else {
            tracing::warn!("views: the window has no box to lay the web views out in");
            return;
        };
        tracing::debug!(
            "views: adopting a view (realized: {}, mapped: {})",
            widget.is_realized(),
            widget.is_mapped()
        );
        if let Some(parent) = widget.parent() {
            if parent != *layout.upcast_ref::<gtk::Widget>() {
                // `widget` holds a reference, so the view survives the move.
                if let Ok(container) = parent.downcast::<gtk::Container>() {
                    container.remove(&widget);
                }
                layout.put(&widget, rect.x as i32, rect.y as i32);
            }
        }
        if fills {
            widget.set_widget_name(FILLS);
            let a = layout.allocation();
            if a.width() > 1 && a.height() > 1 {
                widget.set_size_request(a.width(), a.height());
            } else {
                widget.set_size_request(rect.width as i32, rect.height as i32);
            }
        } else {
            widget.set_size_request(rect.width.max(1.0) as i32, rect.height.max(1.0) as i32);
        }
    });
}

/// Put `view` at `rect` in the layout.
pub fn place(view: &tauri::Webview, rect: super::views::Bounds) {
    let _ = view.with_webview(move |w| {
        let widget: gtk::Widget = w.inner().upcast();
        let Some(layout) = LAYOUT.with(|cell| cell.borrow().clone()) else {
            return;
        };
        layout.move_(&widget, rect.x.round() as i32, rect.y.round() as i32);
        widget.set_size_request(
            rect.width.max(1.0).round() as i32,
            rect.height.max(1.0).round() as i32,
        );
        raise_widget(&widget);
        tracing::debug!(
            "views: game placed at {rect:?} (in layout: {}, visible: {})",
            widget
                .parent()
                .is_some_and(|p| p == *layout.upcast_ref::<gtk::Widget>()),
            widget.is_visible()
        );
    });
}

/// Keep the game pages running while the window is minimised.
///
/// WebKitGTK counts a page in a minimised window as hidden: it stops the
/// page's animation frames and slows its timers, which would stall a match
/// whenever the window is minimised (or sent to the tray, which minimises it
/// on Linux — see `hub::background`). It learns of it from the window's
/// `window-state-event`, acting only when the event says the iconified state
/// changed. This handler is connected before any web view joins the window,
/// so it runs before WebKit's, and takes the "iconified changed" mark off a
/// minimise: the pages stay visible, as Windows keeps them. A restore goes
/// through untouched (WebKit finds the window not minimised and the pages
/// visible, as they were). Resting the games to save power is done in the
/// page instead (`capture::webkit` hook, `views::set_games_visible`).
///
/// Call on the main thread, before the window gets its first web view.
pub fn keep_games_running_when_minimized(window: &tauri::Window) {
    let Ok(gtk_window) = window.gtk_window() else {
        return;
    };
    gtk_window.connect_window_state_event(|_, event| {
        use glib::translate::ToGlibPtr;
        let iconified = gtk::gdk::WindowState::ICONIFIED;
        if event.changed_mask().contains(iconified) && event.new_window_state().contains(iconified)
        {
            let raw: *const gtk::gdk::ffi::GdkEvent = (**event).to_glib_none().0;
            // SAFETY: the event is a window-state event (this signal carries
            // only those), alive for the emission; its mask is plain data.
            unsafe {
                let state = raw as *mut gdk_sys::GdkEventWindowState;
                (*state).changed_mask &= !gdk_sys::GDK_WINDOW_STATE_ICONIFIED;
            }
        }
        glib::Propagation::Proceed
    });
}

/// Put `view` over the other views.
pub fn raise(view: &tauri::Webview) {
    let _ = view.with_webview(|w| raise_widget(&w.inner().upcast()));
}

fn raise_widget(widget: &gtk::Widget) {
    if let Some(window) = widget.window() {
        window.raise();
    }
}

/// Move `view` to `(x, y)` in the layout, keeping its size.
pub fn move_to(view: &tauri::Webview, x: f64, y: f64) {
    let _ = view.with_webview(move |w| {
        let widget: gtk::Widget = w.inner().upcast();
        if let Some(layout) = LAYOUT.with(|cell| cell.borrow().clone()) {
            layout.move_(&widget, x.round() as i32, y.round() as i32);
        }
    });
}
