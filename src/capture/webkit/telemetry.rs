//! The game's analytics beacons, refused in the WebKitGTK game view.
//!
//! The CDP leg answers them in place and the MITM leg refuses their
//! connection; here a WebKit content filter (WebKit's content-blocker rules,
//! Safari's format) blocks every request to the SLS service's hosts before it
//! leaves the view. The Rust bindings leave content filters out, so this goes
//! through the C API.

use anyhow::{anyhow, Context, Result};
use glib::translate::ToGlibPtr;
use std::ffi::{CStr, CString};
use tracing::{info, warn};
use webkit2gtk::WebViewExt;

/// The filter's name in its store.
const FILTER_ID: &str = "mokora-telemetry";

/// Any request to `log.aliyuncs.com` or one of its subdomains (the same hosts
/// `cdp::is_telemetry_host` answers).
const RULES: &str = r#"[
  {"trigger": {"url-filter": "^https?://log\\.aliyuncs\\.com[:/]"}, "action": {"type": "block"}},
  {"trigger": {"url-filter": "^https?://[^/]*\\.log\\.aliyuncs\\.com[:/]"}, "action": {"type": "block"}}
]"#;

/// Block the beacons in `view` when `block`, or stop blocking them. Takes
/// effect for the requests the page makes from now on.
pub fn apply(view: &tauri::Webview, block: bool) -> Result<()> {
    let dir = dirs::cache_dir()
        .map(|d| d.join("mokora").join("content-filters"))
        .ok_or_else(|| anyhow!("no cache directory for the compiled filter"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let dir = CString::new(dir.to_string_lossy().as_bytes()).context("filter directory")?;
    view.with_webview(move |w| {
        let wv = w.inner();
        let Some(ucm) = wv.user_content_manager() else {
            return;
        };
        let ucm: *mut webkit2gtk_sys::WebKitUserContentManager = ucm.to_glib_none().0;
        // SAFETY: on the main thread, with a live content manager; the
        // manager is kept by a reference of our own until the save answers.
        unsafe {
            webkit2gtk_sys::webkit_user_content_manager_remove_all_filters(ucm);
            if !block {
                return;
            }
            let store = webkit2gtk_sys::webkit_user_content_filter_store_new(dir.as_ptr());
            let id = CString::new(FILTER_ID).expect("no NUL in the filter id");
            let rules = glib_sys::g_bytes_new(RULES.as_ptr().cast(), RULES.len());
            gobject_sys::g_object_ref(ucm.cast());
            webkit2gtk_sys::webkit_user_content_filter_store_save(
                store,
                id.as_ptr(),
                rules,
                std::ptr::null_mut(),
                Some(saved),
                ucm.cast(),
            );
            glib_sys::g_bytes_unref(rules);
            // The save keeps the store until it answers.
            gobject_sys::g_object_unref(store.cast());
        }
    })
    .map_err(|e| anyhow!("{e}"))
}

/// The compiled filter is ready: put it on the content manager `data`.
unsafe extern "C" fn saved(
    store: *mut gobject_sys::GObject,
    result: *mut gio_sys::GAsyncResult,
    data: glib_sys::gpointer,
) {
    let ucm = data.cast::<webkit2gtk_sys::WebKitUserContentManager>();
    let mut err: *mut glib_sys::GError = std::ptr::null_mut();
    let filter = webkit2gtk_sys::webkit_user_content_filter_store_save_finish(
        store.cast(),
        result,
        &mut err,
    );
    if filter.is_null() {
        let why = if err.is_null() {
            "unknown error".to_string()
        } else {
            let why = CStr::from_ptr((*err).message)
                .to_string_lossy()
                .into_owned();
            glib_sys::g_error_free(err);
            why
        };
        warn!("webkit capture: telemetry filter not compiled: {why}");
    } else {
        webkit2gtk_sys::webkit_user_content_manager_add_filter(ucm, filter);
        webkit2gtk_sys::webkit_user_content_filter_unref(filter);
        info!("webkit capture: telemetry beacons blocked");
    }
    gobject_sys::g_object_unref(ucm.cast());
}
