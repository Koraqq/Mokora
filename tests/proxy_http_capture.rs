//! The MITM proxy must record the HTTP traffic it intercepts, and must
//! forward it unaltered while doing so.
//!
//! Mokora recorded WebSocket frames and discarded everything else, so the
//! game's own HTTP was invisible: route topology, version endpoints, and
//! the analytics beacons through which the client reports on itself. This
//! drives a real proxy and a real upstream over real sockets and asserts
//! what comes back out of `<session>/inspector.jsonl`.

mod common;

use common::{get_through_proxy, Harness, BEACON_QUERY, UPSTREAM_BODY};
use mokora::config::HttpCaptureConfig;

/// The default policy: recognized exchanges are kept, everything else is
/// not. This is what a user who never touches the config gets, so it is
/// the behaviour worth pinning down.
#[tokio::test(flavor = "multi_thread")]
async fn default_policy_records_only_recognized_exchanges() {
    let h = Harness::start(HttpCaptureConfig::default()).await;

    let beacon = h.url(&format!("/logstores/client/track?{BEACON_QUERY}"));
    let res = get_through_proxy(h.proxy_port, &beacon, &h.host()).await;
    assert!(res.contains("200 OK"), "beacon should be forwarded: {res}");
    // The body must reach the client untouched even though we read it.
    assert!(res.contains(UPSTREAM_BODY), "forwarded body was altered");

    let routes = h.url("/api/clientgate/routes?platform=Steam_Win");
    get_through_proxy(h.proxy_port, &routes, &h.host()).await;

    let entries = h.finish().await;
    let http: Vec<&serde_json::Value> = entries.iter().filter(|e| e["kind"] == "http").collect();
    assert_eq!(
        http.len(),
        1,
        "only the recognized beacon should be recorded: {http:#?}"
    );

    let row = http[0];
    assert_eq!(row["source"], "mitm");
    assert_eq!(row["phase"], "request");
    assert_eq!(row["method"], "GET");
    assert_eq!(row["status"], serde_json::Value::Null);

    // Vendor vocabulary lives in the annotation, never in the exchange.
    let ann = &row["annotations"][0];
    assert_eq!(ann["kind"], "sls_beacon");
    assert_eq!(ann["summary"], "client/certificate_info");
    assert_eq!(ann["data"]["logstore"], "client");
    assert_eq!(ann["data"]["log_category"], "certificate_info");
    assert_eq!(
        ann["data"]["content"][0]["issuer"],
        "CN=Example Test CA, O=Example"
    );
    assert!(
        row.get("logstore").is_none(),
        "the exchange itself must stay vendor-neutral: {row}"
    );

    // Identifiers are kept verbatim — seeing exactly what was sent is the
    // point, and redaction would re-create the blind spot.
    let params = ann["data"]["params"].as_array().expect("params");
    let names: Vec<&str> = params
        .iter()
        .map(|p| p["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        vec![
            "APIVersion",
            "level",
            "log_category",
            "account_id",
            "device_id",
            "content"
        ],
        "parameter order is part of the beacon's identity"
    );

    // A GET with no framing headers has no body; saying it was skipped
    // would invent one.
    assert!(
        row.get("body").is_none(),
        "GET should record no body: {row}"
    );
}
