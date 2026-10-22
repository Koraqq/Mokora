//! End-to-end: the certificate report a Mahjong Soul client sends must
//! describe the **origin's** certificate, not Mokora's.
//!
//! The unit tests in `rewrite::majsoul_cert` prove the substitution given
//! a populated store. This proves the two halves meet: that the TLS
//! verifier on the upstream leg actually records what an origin serves,
//! and that a beacon travelling through the proxy comes out the other
//! side carrying those values. Nothing is stubbed but the origin itself.
//!
//! Its own binary because `Session::init` installs a process-global
//! tracing subscriber — see `tests/common/mod.rs`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair, SanType,
    SerialNumber,
};
use hudsucker::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use hudsucker::rustls::ServerConfig;
use mokora::config::{HttpCaptureConfig, Platform, ProxyConfig};
use mokora::logger::Session;
use mokora::proxy::start_proxy;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Notify};
use tokio_rustls::TlsAcceptor;

const TIMEOUT: Duration = Duration::from_secs(15);

/// The origin's own CA and leaf — the values that must end up in the
/// beacon. Deliberately unlike anything Mokora would mint: a distinct
/// issuer DN, a wildcard subject, and a serial of its own.
const ORIGIN_CA_CN: &str = "Example Root TLS CA";
const ORIGIN_SUBJECT: &str = "*.example.test";

const ORIGIN_SERIAL: u64 = 0x07FEC9E77B8C0D52;

/// A `certificate_info` beacon as an intercepted client would send it:
/// describing Mokora's CA, because that is what the client was served.
///
/// Byte-for-byte in the client's own style — its key order, its escaped
/// forward slashes, no whitespace — because the point of this test is
/// that everything we do not deliberately change comes out untouched.
fn intercepted_beacon_query(host: &str) -> String {
    let content = format!(
        r#"[{{"issuer":"O=Mokora, CN=Mokora Proxy CA","version":3,"oid_value":"1.2.840.10045.2.1","thumbprint":"DEADBEEF","serial_number":"0123456789ABCDEF","ip":["198.18.0.46:443"],"oid_friendly_name":"ECC","url":"wss:\/\/{host}\/gateway","not_before":"7\/31\/2026 10:38:07 PM","not_after":"7\/31\/2027 10:38:07 PM","subject":"CN={host}"}}]"#
    );
    format!(
        "APIVersion=0.6.0&log_category=certificate_info&account_id=10000001&device_model={}&content={}&client_type=app",
        percent_encode("System Product Name (ASUS)"),
        percent_encode(&content)
    )
}

/// The client encodes its query the way .NET's `Uri.EscapeUriString`
/// does: `, / : = [ ] ( ) *` stay literal, spaces become `%20`.
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let literal = ch.is_ascii_alphanumeric()
            || matches!(
                ch,
                '-' | '_'
                    | '.'
                    | '!'
                    | '~'
                    | '*'
                    | '\''
                    | '('
                    | ')'
                    | ';'
                    | '/'
                    | '?'
                    | ':'
                    | '@'
                    | '='
                    | '$'
                    | ','
                    | '['
                    | ']'
            );
        if literal {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// A TLS origin serving a certificate chain of its own, so the proxy's
/// verifier has something genuine to record. Answers any request with a
/// zero-length 200, exactly as the real beacon endpoint does.
async fn tls_origin(host: &str) -> u16 {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::default();
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CountryName, "US");
    ca_dn.push(DnType::OrganizationName, "Example Inc");
    ca_dn.push(DnType::CommonName, ORIGIN_CA_CN);
    ca_params.distinguished_name = ca_dn;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, ORIGIN_SUBJECT);
    params.distinguished_name = dn;
    params.serial_number = Some(SerialNumber::from(ORIGIN_SERIAL));
    params
        .subject_alt_names
        .push(SanType::DnsName(host.try_into().unwrap()));
    let leaf = params.signed_by(&leaf_key, &issuer).unwrap();

    let certs = vec![CertificateDer::from(leaf.der().to_vec())];
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("origin server config");
    let acceptor = TlsAcceptor::from(Arc::new(cfg));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind origin");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(sock).await else {
                    return;
                };
                let mut buf = [0u8; 4096];
                while let Ok(n) = tls.read(&mut buf).await {
                    if n == 0 {
                        return;
                    }
                    if buf[..n].windows(4).any(|w| w == b"\r\n\r\n")
                        && tls
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                            .await
                            .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    port
}

async fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_until_listening(port: u16) {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("proxy never bound 127.0.0.1:{port}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_report_that_leaves_describes_the_origin_not_mokora() {
    let tmp = TempDir::new().expect("tempdir");
    let session =
        Arc::new(Session::init(&tmp.path().join("logs"), "info", "info", &[]).expect("session"));
    let inspector = session.dir().join("inspector.jsonl");

    // Must be a name that actually resolves: the proxy dials the origin
    // by name, and if resolution fails there is no handshake, no observed
    // certificate, and nothing to substitute. `localhost` also keeps SNI
    // a `DnsName`, which is the production path.
    let host = "localhost";
    let origin_port = tls_origin(host).await;
    let proxy_port = free_port().await;

    let (stop, stop_rx) = oneshot::channel::<()>();
    let proxy = tokio::spawn(start_proxy(
        ProxyConfig {
            enabled: true,
            addr: format!("127.0.0.1:{proxy_port}"),
            ca_dir: tmp.path().join("ca"),
            mitm_domains: ProxyConfig::default().mitm_domains,
            rewrite_certificate_report: true,
            // This test exercises the rewrite path, which only runs when the
            // beacon is actually forwarded — so blocking must be off here.
            block_telemetry: false,
        },
        HttpCaptureConfig::default(),
        Platform::Majsoul,
        session,
        None,
        None,
        Arc::new(Notify::new()),
        async move {
            stop_rx.await.unwrap_or_default();
        },
    ));
    wait_until_listening(proxy_port).await;

    // Two requests over one proxy connection. The first is an ordinary
    // HTTPS call whose only job is to make the proxy dial the origin, so
    // its certificate lands in the store — the same thing the game's
    // gateway probes do before it reports on them.
    let mut stream = TcpStream::connect(("127.0.0.1", proxy_port))
        .await
        .expect("reach proxy");
    let warmup = format!(
        "GET https://{host}:{origin_port}/api/clientgate/routes HTTP/1.1\r\nHost: {host}\r\n\r\n"
    );
    stream.write_all(warmup.as_bytes()).await.unwrap();
    let mut buf = [0u8; 1024];
    let n = tokio::time::timeout(TIMEOUT, stream.read(&mut buf))
        .await
        .expect("proxy answered the warm-up")
        .expect("read");
    // Load-bearing: if the proxy could not reach the origin it answers
    // 502, no certificate is ever observed, and the real assertion below
    // would fail for a reason that has nothing to do with the rewrite.
    let warmup_status = String::from_utf8_lossy(&buf[..n]).to_string();
    assert!(
        warmup_status.starts_with("HTTP/1.1 200"),
        "the proxy must reach the origin, else nothing is observed: {warmup_status}"
    );

    // Now the beacon itself.
    let beacon = format!(
        "GET https://{host}:{origin_port}/logstores/client/track?{} HTTP/1.1\r\n\
         Host: {host}\r\nConnection: close\r\n\r\n",
        intercepted_beacon_query(host)
    );
    stream.write_all(beacon.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(TIMEOUT, stream.read_to_end(&mut out))
        .await
        .expect("proxy answered the beacon")
        .expect("read");

    let _ = stop.send(());
    let _ = tokio::time::timeout(TIMEOUT, proxy).await;

    // The inspector records what *left* the machine, so it is the right
    // place to read the outcome from.
    let body = std::fs::read_to_string(&inspector).unwrap_or_default();
    let row = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("valid JSON"))
        .find(|e| e["kind"] == "http" && e["url"].as_str().is_some_and(|u| u.contains("/track?")))
        .expect("the beacon must be on the timeline");

    let entry = &row["annotations"][0]["data"]["content"][0];
    assert!(!entry.is_null(), "beacon should decode: {row}");

    // The whole point: nothing about Mokora may survive.
    assert_eq!(
        entry["issuer"], "CN=Example Root TLS CA, O=Example Inc, C=US",
        "issuer must be the origin's CA"
    );
    assert_eq!(entry["subject"], format!("CN={ORIGIN_SUBJECT}"));
    assert_eq!(entry["serial_number"], "07FEC9E77B8C0D52");
    assert_ne!(entry["thumbprint"], "DEADBEEF");
    assert_ne!(entry["not_before"], "7/31/2026 10:38:07 PM");
    assert!(
        !row.to_string().contains("Mokora Proxy CA"),
        "the intercepting CA must not reach the wire: {row}"
    );

    // What the client said about *where* it connected is not ours to
    // change, and neither is the rest of the beacon.
    assert_eq!(entry["url"], format!("wss://{host}/gateway"));
    assert_eq!(entry["ip"][0], "198.18.0.46:443");
    let params = row["annotations"][0]["data"]["params"]
        .as_array()
        .expect("params");
    let names: Vec<&str> = params
        .iter()
        .map(|p| p["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        names,
        vec![
            "APIVersion",
            "log_category",
            "account_id",
            "device_model",
            "content",
            "client_type"
        ],
        "parameter order must survive the rewrite"
    );
    assert_eq!(
        params
            .iter()
            .find(|p| p["name"] == "account_id")
            .map(|p| p["value"].clone()),
        Some(serde_json::json!("10000001"))
    );

    // Everything we did not deliberately change must be byte-identical to
    // what the client wrote. A first version of this rewriter produced
    // perfect values in alphabetical order with unescaped slashes, which
    // simply traded one fingerprint for another.
    let sent = row["url"].as_str().expect("url");
    let sent_query = sent.split_once('?').expect("query").1;
    let content = sent_query
        .split('&')
        .find_map(|p| p.strip_prefix("content="))
        .expect("content parameter");
    let content = percent_decode(content);

    let mut cursor = 0usize;
    for key in [
        "issuer",
        "version",
        "oid_value",
        "thumbprint",
        "serial_number",
        "ip",
        "oid_friendly_name",
        "url",
        "not_before",
        "not_after",
        "subject",
    ] {
        let needle = format!("\"{key}\":");
        let at = content[cursor..]
            .find(&needle)
            .unwrap_or_else(|| panic!("{key} missing or out of order in {content}"));
        cursor += at + needle.len();
    }
    assert!(
        content.contains(r#""url":"wss:\/\/localhost\/gateway""#),
        "the client's escaping must survive: {content}"
    );
    assert!(
        !content.contains("://"),
        "an unescaped slash would be a new fingerprint: {content}"
    );
    // .NET leaves parentheses and uses %20; a stricter encoder would have
    // rewritten this parameter even though it has nothing to do with us.
    assert!(
        sent_query.contains("device_model=System%20Product%20Name%20(ASUS)"),
        "other parameters must be untouched: {sent_query}"
    );
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
