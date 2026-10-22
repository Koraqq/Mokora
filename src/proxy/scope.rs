//! Which hosts the MITM proxy may decrypt.
//!
//! One list (`[proxy] mitm_domains`) drives two things that must agree:
//!
//! - the CA certificate's NameConstraints, so the CA a user trusts cannot
//!   vouch for any other site even if its key leaks (see `ca.rs`);
//! - `should_intercept`, so a host outside the list is raw-tunneled with
//!   its genuine certificate instead of getting a leaf the client would
//!   reject (see `handler.rs`).
//!
//! An entry covers the domain itself and every subdomain, the way an X.509
//! dNSName constraint does: `maj-soul.com` covers `game.maj-soul.com`.
//!
//! IP literals are outside the list's reach on purpose. The Steam client
//! dials its WebSocket gateway by raw IP on alt ports, so the CA must keep
//! signing IP leaves; game IPs change, so they cannot be pinned in the CA.

use crate::config::DEFAULT_MITM_DOMAINS;
use std::net::IpAddr;
use tracing::warn;

/// The normalized domain list: lowercase, no wildcard or dots at the ends,
/// sorted, no duplicates. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MitmScope {
    domains: Vec<String>,
}

impl MitmScope {
    /// Normalize the configured list. Entries that are not plain DNS names
    /// are dropped with a warning. An empty result falls back to the
    /// defaults: a CA with no dNSName constraint at all would be
    /// unconstrained, the opposite of what an empty list asks for.
    pub fn from_config(entries: &[String]) -> Self {
        let mut domains: Vec<String> = entries
            .iter()
            .filter_map(|raw| {
                let d = normalize(raw);
                if d.is_none() {
                    warn!("ignoring [proxy] mitm_domains entry {raw:?}: not a plain domain name");
                }
                d
            })
            .collect();
        if domains.is_empty() {
            warn!("[proxy] mitm_domains has no usable entry; using the built-in game domains");
            domains = DEFAULT_MITM_DOMAINS
                .iter()
                .filter_map(|d| normalize(d))
                .collect();
        }
        domains.sort();
        domains.dedup();
        Self { domains }
    }

    pub fn domains(&self) -> &[String] {
        &self.domains
    }

    /// Whether `host` (as `Uri::host` gives it) may be decrypted. IP
    /// literals always may — see the module doc.
    pub fn covers(&self, host: &str) -> bool {
        let unbracketed = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        if unbracketed.parse::<IpAddr>().is_ok() {
            return true;
        }
        let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
        self.domains.iter().any(|d| {
            host == *d
                || host
                    .strip_suffix(d.as_str())
                    .is_some_and(|rest| rest.ends_with('.'))
        })
    }
}

fn normalize(raw: &str) -> Option<String> {
    let d = raw.trim().to_ascii_lowercase();
    let d = d.strip_prefix("*.").unwrap_or(&d);
    let d = d.trim_matches('.');
    super::ca::is_plain_dns_name(d).then(|| d.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(entries: &[&str]) -> MitmScope {
        MitmScope::from_config(&entries.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn covers_domain_and_subdomains_only() {
        let s = scope(&["maj-soul.com"]);
        assert!(s.covers("maj-soul.com"));
        assert!(s.covers("route-6.maj-soul.com"));
        assert!(s.covers("GAME.Maj-Soul.com."));
        assert!(!s.covers("evilmaj-soul.com"), "suffix match must stop at a label");
        assert!(!s.covers("maj-soul.com.evil.example"));
        assert!(!s.covers("google.com"));
    }

    #[test]
    fn ip_literals_are_always_covered() {
        let s = scope(&["maj-soul.com"]);
        assert!(s.covers("13.112.183.79"));
        assert!(s.covers("[2001:db8::1]"));
    }

    #[test]
    fn entries_are_normalized_and_bad_ones_dropped() {
        let s = scope(&[" *.Maj-Soul.com ", ".mahjongsoul.com.", "maj-soul.com", "例子.com", ""]);
        assert_eq!(s.domains(), ["mahjongsoul.com", "maj-soul.com"]);
    }

    #[test]
    fn empty_list_falls_back_to_defaults() {
        let s = scope(&[]);
        assert_eq!(s.domains().len(), DEFAULT_MITM_DOMAINS.len());
        assert!(s.covers("game.maj-soul.com"));
    }
}
