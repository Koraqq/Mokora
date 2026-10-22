//! Corrects the certificate report a Mahjong Soul standalone client
//! sends about its own gateway connections.
//!
//! ## What the client sends
//!
//! Once the lobby socket is up, the client posts a `certificate_info`
//! beacon listing every TLS certificate it was served, one entry per
//! gateway URL it tried. Measured from a clean capture, byte for byte:
//!
//! ```text
//! [{"issuer":"CN=RapidSSL TLS RSA CA G1, OU=www.digicert.com, O=DigiCert Inc, C=US",
//!   "version":3,"oid_value":"1.2.840.113549.1.1.1","thumbprint":"AB33…",
//!   "serial_number":"07FE…","ip":["198.18.0.46:443"],"oid_friendly_name":"RSA",
//!   "url":"wss:\/\/route-6.maj-soul.com\/gateway",
//!   "not_before":"5\/26\/2026 8:00:00 AM","not_after":"12\/11\/2026 7:59:59 AM",
//!   "subject":"CN=*.maj-soul.com"}]
//! ```
//!
//! It is sent on **every login**, not only when something looks wrong.
//!
//! ## Why it needs correcting
//!
//! With Mokora in the path the client is describing Mokora's certificate,
//! and measured against that capture, six independent fields give it
//! away: the issuer names the tool outright, the subject is the exact
//! hostname where a genuine certificate is a wildcard, the key is ECC
//! where the real one is RSA, the serial is half the length, `not_before`
//! is about a minute before the beacon, and the validity window is a flat
//! 365 days. Renaming the CA would fix exactly one of the six.
//!
//! ## Why this edits bytes instead of re-serializing
//!
//! The first version parsed the report, replaced the fields and
//! re-serialized. Every value came out right and it was still wrong,
//! because a JSON library does not write JSON the way this client does:
//!
//! - **Key order.** The client emits `issuer, version, oid_value,
//!   thumbprint, serial_number, ip, oid_friendly_name, url, not_before,
//!   not_after, subject`. `serde_json::Value` is a `BTreeMap`, so
//!   re-serializing sorts them alphabetically.
//! - **Escaping.** The client escapes forward slashes — `wss:\/\/…`, and
//!   `5\/26\/2026` inside the dates. `serde_json` does not.
//! - **Percent-encoding.** The client encodes the query the way .NET's
//!   `Uri.EscapeUriString` does, leaving `, / : = [ ] ( ) *` literal. A
//!   stricter encoder rewrites bytes in *every* parameter, not just the
//!   one being corrected.
//!
//! Each of those is a fresh fingerprint, shared by every Mokora user, in
//! exchange for removing the old one. So instead this splices replacement
//! values into the bytes the client wrote: key order, escaping, spacing
//! and any field we have never seen all survive untouched, because they
//! are never re-encoded.
//!
//! ## What it changes
//!
//! Nine values per entry — `issuer`, `version`, `oid_value`,
//! `oid_friendly_name`, `thumbprint`, `serial_number`, `not_before`,
//! `not_after`, `subject` — with the fields Mokora observed on the
//! upstream leg. `url` and `ip` are the client's own account of *where*
//! it connected and are not ours to change, and every other query
//! parameter is left byte-identical.
//!
//! The values are genuine; the claim that the client saw them is not.
//! That is the point and the cost, and it is why this is opt-out.
//!
//! ## When it declines
//!
//! - An entry whose host we have no observed certificate for is left
//!   **unchanged** and counted, so the caller can warn. Dropping it would
//!   change the array length, which tracks how many gateways the client
//!   probed.
//! - An entry missing any of the nine fields is left alone entirely
//!   rather than half-corrected — a mix of genuine and Mokora values in
//!   one entry is worse than either.
//! - Content that is not a JSON array, or a query with no `content`
//!   parameter, is passed through untouched.

use crate::inspector::annotate::sls::SlsBeacon;
use crate::proxy::certstore::{CertStore, ObservedCert};

/// The one `log_category` this rewriter applies to.
pub const CATEGORY: &str = "certificate_info";

/// Fields replaced from the observed certificate. `url` and `ip` are
/// deliberately absent — see the module docs.
const REPLACED_FIELDS: &[&str] = &[
    "issuer",
    "version",
    "oid_value",
    "oid_friendly_name",
    "thumbprint",
    "serial_number",
    "not_before",
    "not_after",
    "subject",
];

/// Outcome of a rewrite attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    /// The replacement query string, ready to put back on the URI.
    pub query: String,
    /// Entries whose certificate fields were replaced.
    pub corrected: usize,
    /// Entries left as the client wrote them. These still describe Mokora.
    pub uncorrected: usize,
}

/// Correct the certificate report carried in `query`, or `None` if there
/// is nothing to do.
///
/// `beacon` is the decoded view of the same query — used to check the
/// category and to read the `content` value — while `query` is the raw
/// bytes that everything except `content` is copied from verbatim.
pub fn rewrite(query: &str, beacon: &SlsBeacon, store: &CertStore) -> Option<Rewritten> {
    if beacon.log_category.as_deref() != Some(CATEGORY) {
        return None;
    }
    let content = beacon.param("content")?;
    let (new_content, corrected, uncorrected) = correct_content(content, store)?;
    let query = splice_param(query, "content", &escape_uri_component(&new_content))?;
    Some(Rewritten {
        query,
        corrected,
        uncorrected,
    })
}

/// Replace the certificate fields in each array element we can.
///
/// Returns `None` when nothing could be corrected, so the caller forwards
/// the original bytes rather than an identical copy of them.
fn correct_content(content: &str, store: &CertStore) -> Option<(String, usize, usize)> {
    let elements = array_elements(content)?;
    let mut out = String::with_capacity(content.len());
    let mut corrected = 0usize;
    let mut uncorrected = 0usize;
    let mut cursor = 0usize;

    for (start, end) in elements {
        // Everything between elements — brackets, commas, whitespace —
        // is copied across untouched.
        out.push_str(&content[cursor..start]);
        let element = &content[start..end];
        match correct_element(element, store) {
            Some(fixed) => {
                corrected += 1;
                out.push_str(&fixed);
            }
            None => {
                uncorrected += 1;
                out.push_str(element);
            }
        }
        cursor = end;
    }
    out.push_str(&content[cursor..]);

    if corrected == 0 {
        return None;
    }
    Some((out, corrected, uncorrected))
}

/// Splice replacement values into one entry, or `None` to leave it be.
fn correct_element(element: &str, store: &CertStore) -> Option<String> {
    let url_span = member_value_span(element, "url")?;
    let host = host_of(&json_unescape(strip_quotes(
        &element[url_span.0..url_span.1],
    )?))?;
    let cert = store.get(&host)?;

    // Locate every field first: an entry missing one is left alone rather
    // than half-corrected.
    let mut edits: Vec<((usize, usize), String)> = Vec::with_capacity(REPLACED_FIELDS.len());
    for field in REPLACED_FIELDS {
        let span = member_value_span(element, field)?;
        edits.push((span, replacement_for(field, &cert)?));
    }

    // Apply from the end so earlier offsets stay valid.
    edits.sort_by_key(|((start, _), _)| std::cmp::Reverse(*start));
    let mut out = element.to_string();
    for ((start, end), value) in edits {
        out.replace_range(start..end, &value);
    }
    Some(out)
}

/// The JSON token that replaces a field's value.
fn replacement_for(field: &str, cert: &ObservedCert) -> Option<String> {
    Some(match field {
        // The only number among them.
        "version" => cert.version.to_string(),
        "issuer" => json_string(&cert.issuer),
        "subject" => json_string(&cert.subject),
        "oid_value" => json_string(&cert.oid_value),
        "oid_friendly_name" => json_string(&cert.oid_friendly_name),
        "thumbprint" => json_string(&cert.thumbprint),
        "serial_number" => json_string(&cert.serial_number),
        "not_before" => json_string(&cert.not_before),
        "not_after" => json_string(&cert.not_after),
        _ => return None,
    })
}

/// Byte spans of the top-level elements of a JSON array.
fn array_elements(content: &str) -> Option<Vec<(usize, usize)>> {
    let bytes = content.as_bytes();
    let open = bytes.iter().position(|b| !b.is_ascii_whitespace())?;
    if bytes[open] != b'[' {
        return None;
    }
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = string_end(content, i)?;
                if depth == 0 && start.is_none() {
                    start = Some(i);
                }
                i = end;
                continue;
            }
            b'[' | b'{' => {
                if depth == 0 && start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            b']' | b'}' => {
                if depth == 0 {
                    // Closing bracket of the array itself.
                    if let Some(s) = start.take() {
                        spans.push((s, i));
                    }
                    return Some(spans);
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if let Some(s) = start.take() {
                    spans.push((s, i));
                }
            }
            b if b.is_ascii_whitespace() => {}
            _ => {
                if depth == 0 && start.is_none() {
                    start = Some(i);
                }
            }
        }
        i += 1;
    }
    // Unterminated array: refuse rather than guess.
    None
}

/// Byte span of the value of top-level member `key` inside a JSON object.
fn member_value_span(object: &str, key: &str) -> Option<(usize, usize)> {
    let bytes = object.as_bytes();
    let open = bytes.iter().position(|b| !b.is_ascii_whitespace())?;
    if bytes[open] != b'{' {
        return None;
    }
    let mut depth = 0usize;
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = string_end(object, i)?;
                if depth == 0 {
                    // A string at this depth is a key if a colon follows.
                    let mut j = end;
                    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j] == b':' {
                        let is_match = strip_quotes(&object[i..end])
                            .map(|raw| json_unescape(raw) == key)
                            .unwrap_or(false);
                        j += 1;
                        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        let value_end = value_end(object, j)?;
                        if is_match {
                            return Some((j, value_end));
                        }
                        i = value_end;
                        continue;
                    }
                }
                i = end;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Index just past the JSON value starting at `from`.
fn value_end(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    match *bytes.get(from)? {
        b'"' => string_end(s, from),
        open @ (b'{' | b'[') => {
            let close = if open == b'{' { b'}' } else { b']' };
            let mut depth = 0usize;
            let mut i = from;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = string_end(s, i)?;
                        continue;
                    }
                    b if b == open => depth += 1,
                    b if b == close => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        // Number, or a bare literal like `true` / `null`.
        _ => {
            let mut i = from;
            while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']') {
                i += 1;
            }
            while i > from && bytes[i - 1].is_ascii_whitespace() {
                i -= 1;
            }
            (i > from).then_some(i)
        }
    }
}

/// Index just past the closing quote of the JSON string starting at `from`.
fn string_end(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if *bytes.get(from)? != b'"' {
        return None;
    }
    let mut i = from + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn strip_quotes(token: &str) -> Option<&str> {
    token
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
}

/// Minimal JSON string unescaping — enough for the keys and URLs here.
fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => out.push('\u{fffd}'),
                }
            }
            // `\/`, `\"`, `\\` and anything else: the character itself.
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

/// A quoted JSON string escaped the way the client escapes them —
/// **including the forward slash**, which is legal JSON that most
/// libraries decline to emit and which shows up in every `url`,
/// `not_before` and `not_after` value.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Percent-encode the way .NET's `Uri.EscapeUriString` does, which is
/// what the client's query is encoded with: unreserved and most reserved
/// characters stay literal, spaces become `%20` rather than `+`.
///
/// `&`, `+` and `#` are escaped even though .NET would leave them, because
/// a literal one inside a parameter value would corrupt the query. That
/// can only differ from the client on a value the client would itself
/// have corrupted.
fn escape_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
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

/// Replace one parameter's raw value in a query string, leaving every
/// other byte alone.
fn splice_param(query: &str, name: &str, encoded_value: &str) -> Option<String> {
    let mut cursor = 0usize;
    for part in query.split('&') {
        let start = cursor;
        cursor += part.len() + 1; // consume the '&' too
        let Some((key, _)) = part.split_once('=') else {
            continue;
        };
        if key != name {
            continue;
        }
        let value_start = start + key.len() + 1;
        let value_end = start + part.len();
        let mut out = String::with_capacity(query.len() + encoded_value.len());
        out.push_str(&query[..value_start]);
        out.push_str(encoded_value);
        out.push_str(&query[value_end..]);
        return Some(out);
    }
    None
}

/// Host of a `wss://` / `https://` URL, lowercased.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let authority = rest.split('/').next()?;
    let host = authority.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::annotate::sls;
    use hudsucker::rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
        SerialNumber,
    };
    use time::{Duration, OffsetDateTime};

    /// One entry exactly as a clean capture shows it: the client's key
    /// order, its `\/` escaping, and no whitespace.
    fn genuine_entry(host: &str) -> String {
        format!(
            r#"{{"issuer":"O=Mokora, CN=Mokora Proxy CA","version":3,"oid_value":"1.2.840.10045.2.1","thumbprint":"DEADBEEF","serial_number":"0123456789ABCDEF","ip":["198.18.0.46:443"],"oid_friendly_name":"ECC","url":"wss:\/\/{host}\/gateway","not_before":"7\/31\/2026 10:38:07 PM","not_after":"7\/31\/2027 10:38:07 PM","subject":"CN={host}"}}"#
        )
    }

    fn beacon_for(content: &str) -> (String, SlsBeacon) {
        let query = format!(
            "APIVersion=0.6.0&log_category=certificate_info&device_model={}&content={}&client_type=app",
            escape_uri_component("System Product Name (ASUS)"),
            escape_uri_component(content)
        );
        let beacon = sls::parse_url(&format!("https://h/logstores/client/track?{query}"))
            .expect("fixture must parse");
        (query, beacon)
    }

    /// A gateway certificate shaped like a real one: a wildcard subject
    /// issued by a separate CA, so issuer and subject differ.
    fn genuine_cert_for(store: &CertStore, host: &str) {
        let ca_key = KeyPair::generate().unwrap();
        let mut ca_params = CertificateParams::default();
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CountryName, "US");
        ca_dn.push(DnType::OrganizationName, "Example Inc");
        ca_dn.push(DnType::CommonName, "Example TLS CA");
        ca_params.distinguished_name = ca_dn;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let issuer = Issuer::new(ca_params, ca_key);

        let leaf_key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "*.example.com");
        params.distinguished_name = dn;
        params.serial_number = Some(SerialNumber::from(0x07FEC9E77B8C0D52u64));
        params.not_before = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        params.not_after = params.not_before + Duration::days(200);
        let der = params.signed_by(&leaf_key, &issuer).unwrap().der().to_vec();
        store.record(host, &der);
    }

    fn decoded_content(query: &str) -> String {
        sls::parse_url(&format!("https://h/logstores/client/track?{query}"))
            .unwrap()
            .param("content")
            .unwrap()
            .to_string()
    }

    #[test]
    fn every_giveaway_field_is_replaced() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));

        let out = rewrite(&query, &beacon, &store).expect("should rewrite");
        assert_eq!((out.corrected, out.uncorrected), (1, 0));

        let content = decoded_content(&out.query);
        assert!(content.contains(r#""issuer":"CN=Example TLS CA, O=Example Inc, C=US""#));
        assert!(content.contains(r#""subject":"CN=*.example.com""#));
        assert!(content.contains(r#""serial_number":"07FEC9E77B8C0D52""#));
        assert!(
            !content.contains("Mokora"),
            "no trace may survive: {content}"
        );
        assert!(!content.contains("DEADBEEF"));
    }

    /// The regression this rewrite exists for. Re-serializing produced
    /// correct values in alphabetical order — a fresh fingerprint shared
    /// by every user, traded for the old one.
    #[test]
    fn the_clients_key_order_survives() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));
        let content = decoded_content(&rewrite(&query, &beacon, &store).unwrap().query);

        let order: Vec<&str> = [
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
        ]
        .into_iter()
        .collect();
        let mut cursor = 0usize;
        for key in order {
            let needle = format!("\"{key}\":");
            let at = content[cursor..]
                .find(&needle)
                .unwrap_or_else(|| panic!("{key} missing or out of order in {content}"));
            cursor += at + needle.len();
        }
    }

    /// The other half of the same regression: the client escapes forward
    /// slashes, in URLs *and* in the dates. A JSON library does not.
    #[test]
    fn forward_slashes_stay_escaped_the_way_the_client_writes_them() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));
        let content = decoded_content(&rewrite(&query, &beacon, &store).unwrap().query);

        // Untouched field: exactly as the client wrote it.
        assert!(content.contains(r#""url":"wss:\/\/route-6.maj-soul.com\/gateway""#));
        // Replaced field: written in the same style.
        assert!(
            content.contains(r#""not_before":"11\/"#) || content.contains(r#""not_before":"1\/"#),
            "dates must keep the escaped slashes: {content}"
        );
        assert!(
            !content.contains("://"),
            "unescaped slash leaked: {content}"
        );
    }

    /// Only `content` may change. A stricter percent-encoder rewrote
    /// bytes in every other parameter, which is its own tell.
    #[test]
    fn other_parameters_are_byte_identical() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));
        let out = rewrite(&query, &beacon, &store).unwrap();

        let before: Vec<&str> = query
            .split('&')
            .filter(|p| !p.starts_with("content="))
            .collect();
        let after: Vec<&str> = out
            .query
            .split('&')
            .filter(|p| !p.starts_with("content="))
            .collect();
        assert_eq!(before, after);
        // Including the parentheses and spaces .NET leaves alone.
        assert!(out
            .query
            .contains("device_model=System%20Product%20Name%20(ASUS)"));
    }

    #[test]
    fn url_and_ip_are_not_ours_to_change() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));
        let content = decoded_content(&rewrite(&query, &beacon, &store).unwrap().query);
        assert!(content.contains(r#""ip":["198.18.0.46:443"]"#));
        assert!(content.contains("route-6.maj-soul.com"));
    }

    /// A gateway we never reached keeps its entry, and the count lets the
    /// caller warn. Dropping it would change the array length.
    #[test]
    fn unknown_hosts_are_left_alone_and_counted() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(&format!(
            "[{},{}]",
            genuine_entry("route-6.maj-soul.com"),
            genuine_entry("route-5.maj-soul.com")
        ));

        let out = rewrite(&query, &beacon, &store).expect("should correct what it can");
        assert_eq!((out.corrected, out.uncorrected), (1, 1));
        let content = decoded_content(&out.query);
        assert!(
            content.contains("Mokora Proxy CA"),
            "the second entry stands"
        );
        assert_eq!(
            content.matches(r#""issuer":"#).count(),
            2,
            "length unchanged"
        );
    }

    #[test]
    fn nothing_observed_means_no_rewrite_at_all() {
        let store = CertStore::default();
        let (query, beacon) = beacon_for(&format!("[{}]", genuine_entry("route-6.maj-soul.com")));
        assert!(rewrite(&query, &beacon, &store).is_none());
    }

    #[test]
    fn other_categories_are_not_touched() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let query = "log_category=login_stats&content=%7B%22use_time%22%3A2%7D";
        let beacon = sls::parse_url(&format!("https://h/logstores/client/track?{query}")).unwrap();
        assert!(rewrite(query, &beacon, &store).is_none());
    }

    /// An entry missing a field is left whole rather than half-corrected.
    #[test]
    fn an_entry_missing_a_field_is_declined() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let (query, beacon) = beacon_for(
            r#"[{"issuer":"O=Mokora, CN=Mokora Proxy CA","url":"wss:\/\/route-6.maj-soul.com\/gateway"}]"#,
        );
        assert!(rewrite(&query, &beacon, &store).is_none());
    }

    #[test]
    fn malformed_content_is_passed_through() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        for content in ["not-json", r#"{"a":1}"#, "[", "[{"] {
            let (query, beacon) = beacon_for(content);
            assert!(rewrite(&query, &beacon, &store).is_none(), "{content}");
        }
    }

    /// An unknown extra field must survive: the client may add one, and
    /// dropping it would be as visible as changing one.
    #[test]
    fn fields_we_do_not_know_about_are_preserved() {
        let store = CertStore::default();
        genuine_cert_for(&store, "route-6.maj-soul.com");
        let entry = genuine_entry("route-6.maj-soul.com").replace(
            r#""version":3"#,
            r#""version":3,"future_field":{"nested":[1,2]}"#,
        );
        let (query, beacon) = beacon_for(&format!("[{entry}]"));
        let content = decoded_content(&rewrite(&query, &beacon, &store).unwrap().query);
        assert!(
            content.contains(r#""future_field":{"nested":[1,2]}"#),
            "{content}"
        );
    }

    #[test]
    fn member_spans_are_found_regardless_of_position() {
        let obj = r#"{"a":"1","b":[1,{"a":"nested"}],"c":3}"#;
        let (s, e) = member_value_span(obj, "a").unwrap();
        assert_eq!(&obj[s..e], r#""1""#);
        let (s, e) = member_value_span(obj, "b").unwrap();
        assert_eq!(&obj[s..e], r#"[1,{"a":"nested"}]"#);
        let (s, e) = member_value_span(obj, "c").unwrap();
        assert_eq!(&obj[s..e], "3");
        assert!(
            member_value_span(obj, "nested").is_none(),
            "keys are top-level only"
        );
    }

    /// A `,` or `}` inside a string must not end an element or a value.
    #[test]
    fn punctuation_inside_strings_does_not_confuse_the_scanner() {
        let content = r#"[{"issuer":"O=A, Inc}","url":"wss:\/\/h\/g"},{"issuer":"B","url":"x"}]"#;
        let spans = array_elements(content).unwrap();
        assert_eq!(spans.len(), 2);
        assert!(content[spans[0].0..spans[0].1].contains("O=A, Inc}"));
    }

    #[test]
    fn hosts_are_extracted_from_gateway_urls() {
        assert_eq!(
            host_of("wss://route-6.maj-soul.com/gateway").as_deref(),
            Some("route-6.maj-soul.com")
        );
        assert_eq!(
            host_of("wss://route-3.maj-soul.com:8443/gateway").as_deref(),
            Some("route-3.maj-soul.com")
        );
        assert_eq!(host_of("not a url"), None);
    }

    #[test]
    fn splice_replaces_only_the_named_parameter() {
        let q = "a=1&content=OLD&b=2";
        assert_eq!(
            splice_param(q, "content", "NEW").unwrap(),
            "a=1&content=NEW&b=2"
        );
        // Last and first positions.
        assert_eq!(
            splice_param("content=OLD&b=2", "content", "N").unwrap(),
            "content=N&b=2"
        );
        assert_eq!(
            splice_param("a=1&content=OLD", "content", "N").unwrap(),
            "a=1&content=N"
        );
        // A parameter whose *value* contains the name must not match.
        assert_eq!(
            splice_param("a=content=1&content=OLD", "content", "N").unwrap(),
            "a=content=1&content=N"
        );
        assert!(splice_param("a=1&b=2", "content", "N").is_none());
    }

    #[test]
    fn encoding_matches_the_clients_style() {
        // Spaces are %20, and .NET leaves these literal.
        assert_eq!(escape_uri_component("a b"), "a%20b");
        assert_eq!(escape_uri_component("(x),[y]:z/w=1"), "(x),[y]:z/w=1");
        // Braces, quotes and backslashes are escaped, as the client does.
        assert_eq!(escape_uri_component(r#"{"a"}"#), "%7B%22a%22%7D");
        assert_eq!(escape_uri_component("a\\b"), "a%5Cb");
    }
}
