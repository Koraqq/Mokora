//! Recognizers that read meaning out of captured HTTP exchanges.
//!
//! Capture is generic: [`crate::capture::http`] records every exchange a
//! backend intercepts without understanding any of it. Understanding is
//! this layer's job, and it is strictly additive — a recognizer returns an
//! [`HttpAnnotation`] that rides along with the exchange, and knows
//! nothing about how the exchange was captured.
//!
//! That split is the whole design. Vendor vocabulary (`logstore`,
//! `log_category`, …) lives inside an annotation's `data`, never in
//! [`crate::schema::HttpExchange`]. Adding a recognizer for a new
//! analytics vendor, a new game, or a new protocol therefore touches this
//! directory and nothing else — not the schema, not the capture
//! backends, not the reader, not the UI.
//!
//! Annotations are also how Mokora describes **its own** behaviour: when
//! the proxy declines to intercept something, that decision is recorded
//! as an annotation on the CONNECT (see `mokora_bypass` in the proxy
//! handler). A blind spot that announces itself is not a blind spot.
//!
//! ## Adding a recognizer
//!
//! 1. Add a module here that inspects a [`RequestView`] and returns
//!    `Option<HttpAnnotation>`.
//! 2. Call it from [`annotate_request`].
//! 3. Give it a stable `kind` string — the UI groups and filters on it.
//!
//! Keep recognizers cheap. [`annotate_request`] runs on every intercepted
//! request, the overwhelming majority of which match nothing.

pub mod sls;

use crate::schema::{HttpAnnotation, HttpHeader};

/// What a recognizer gets to look at.
///
/// A struct rather than a parameter list so recognizers that need more
/// context later (a body, a header) do not force every call site to
/// change.
pub struct RequestView<'a> {
    pub method: &'a str,
    /// Absolute URL where the backend could reconstruct one.
    pub url: &'a str,
    pub headers: &'a [HttpHeader],
    /// Request body as text, when it was captured.
    pub body: Option<&'a str>,
}

impl<'a> RequestView<'a> {
    pub fn new(method: &'a str, url: &'a str, headers: &'a [HttpHeader]) -> Self {
        Self {
            method,
            url,
            headers,
            body: None,
        }
    }
}

/// Run every recognizer against a request. Usually returns empty.
pub fn annotate_request(view: &RequestView<'_>) -> Vec<HttpAnnotation> {
    let mut out = Vec::new();
    if let Some(a) = sls::annotate(view) {
        out.push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_traffic_gets_no_annotations() {
        let view = RequestView::new(
            "GET",
            "https://route-5.example.com/api/clientgate/routes?platform=Steam_Win",
            &[],
        );
        assert!(annotate_request(&view).is_empty());
    }

    #[test]
    fn a_recognized_request_is_annotated() {
        let view = RequestView::new(
            "GET",
            "https://example-client.cn-hongkong.log.aliyuncs.com/logstores/client/track?log_category=login_stats",
            &[],
        );
        let annotations = annotate_request(&view);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].kind, "sls_beacon");
    }
}
