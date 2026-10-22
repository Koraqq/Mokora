use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mahjong Soul's own domains (CN, JP / EN game servers and the EN web
/// client), plus the Aliyun log host whose certificate-report beacon is
/// rewritten when telemetry is forwarded (`block_telemetry = false`).
pub const DEFAULT_MITM_DOMAINS: &[&str] = &[
    "maj-soul.com",
    "mahjongsoul.com",
    "mahjongsoul.game.yo-star.com",
    "log.aliyuncs.com",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub addr: String,
    pub ca_dir: PathBuf,
    /// The only domains (and their subdomains) the MITM proxy decrypts;
    /// every other host is tunneled untouched. The CA certificate is
    /// constrained to exactly this list, so it cannot vouch for any other
    /// site. Changing the list makes Mokora replace the CA on next start,
    /// and the new `mokora-ca.cer` has to be trusted again.
    /// See `src/proxy/scope.rs`.
    pub mitm_domains: Vec<String>,
    /// Correct the certificate report a Mahjong Soul standalone client
    /// sends about its gateway connections, substituting the certificates
    /// Mokora observed upstream for the ones it served itself.
    ///
    /// On by default because leaving it off means the client reports
    /// Mokora's CA by name on every login. Turn it off to capture what the
    /// client *would* have said — which is the only way to check the
    /// correction is still complete after a client update.
    ///
    /// MITM-mode only: the chromium backend has nothing to rewrite, and a
    /// browser cannot report peer certificates in the first place.
    /// See `src/proxy/rewrite/majsoul_cert.rs`.
    pub rewrite_certificate_report: bool,
    /// Drop the game client's Aliyun SLS web-tracking (telemetry) beacons
    /// instead of forwarding them upstream.
    ///
    /// On by default. The client fires these fire-and-forget analytics
    /// beacons at `*.log.aliyuncs.com/logstores/<store>/track`, reporting on
    /// itself — login stats, game status, device/account identifiers. Many
    /// ad blockers already block that host, so a missing beacon is
    /// indistinguishable from an ad-blocked one and is not evidence of a
    /// third-party tool; dropping them keeps that data from leaving the
    /// machine at all.
    ///
    /// Takes precedence over `rewrite_certificate_report`: when this is on
    /// the Mahjong Soul certificate report is dropped along with every other
    /// beacon, so there is nothing left to rewrite (and nothing left to leak
    /// Mokora's CA before the genuine upstream certificate has been
    /// observed). Turn this off to forward the beacons — the certificate
    /// report then still gets corrected.
    ///
    /// Both capture modes: the MITM proxy refuses the beacons (see
    /// `src/proxy/handler.rs`), and the chromium backend pauses them with
    /// `Fetch` and answers in the server's place (see
    /// `src/capture/chromium/cdp.rs`). Kept under `[proxy]` because existing
    /// config files already hold the user's choice here; the Settings page
    /// shows it under Privacy & uploads with the rest of `[privacy]`.
    pub block_telemetry: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            addr: "127.0.0.1:23410".to_string(),
            ca_dir: PathBuf::from("./ca"),
            mitm_domains: DEFAULT_MITM_DOMAINS.iter().map(|d| d.to_string()).collect(),
            rewrite_certificate_report: true,
            block_telemetry: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The correction is on by default: with it off, a standalone client
    /// names Mokora's CA to the operator on every single login.
    #[test]
    fn certificate_report_is_corrected_by_default() {
        assert!(ProxyConfig::default().rewrite_certificate_report);
    }

    /// Telemetry blocking is on by default — the whole point is that it
    /// protects without the user having to know the beacons exist.
    #[test]
    fn telemetry_is_blocked_by_default() {
        assert!(ProxyConfig::default().block_telemetry);
    }

    /// A `config.toml` written before either field existed must still load,
    /// and must pick up the defaults rather than silently disabling them.
    #[test]
    fn older_configs_gain_the_correction() {
        let cfg: ProxyConfig =
            toml::from_str("enabled = true\naddr = \"127.0.0.1:23410\"\nca_dir = \"./ca\"")
                .expect("older config must still parse");
        assert!(cfg.rewrite_certificate_report);
        assert!(cfg.block_telemetry);
        assert_eq!(cfg.mitm_domains, ProxyConfig::default().mitm_domains);
    }
}
