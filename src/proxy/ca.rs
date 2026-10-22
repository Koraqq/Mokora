use super::scope::MitmScope;
use anyhow::{anyhow, Context, Result};
use http::uri::Authority;
use hudsucker::{
    certificate_authority::CertificateAuthority,
    rcgen::{
        string::Ia5String, BasicConstraints, CertificateParams, DistinguishedName, DnType,
        GeneralSubtree, IsCa, Issuer, KeyPair, KeyUsagePurpose, NameConstraints, SanType,
    },
    rustls::{
        self,
        crypto::CryptoProvider,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
        ServerConfig,
    },
};
use rand::{rng, Rng};
use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use time::{Duration, OffsetDateTime};
use tracing::{info, warn};

/// Leaf-cert validity window. Mirrors hudsucker's defaults.
const LEAF_TTL_SECS: i64 = 365 * 24 * 60 * 60;
const NOT_BEFORE_OFFSET_SECS: i64 = 60;

const BASENAME: &str = "mokora-ca";
/// The certificate is written in every format an OS / browser store may ask
/// for. All of them are public.
const CERT_PEM_EXTS: &[&str] = &["cer", "crt", "pem"];
const CERT_DER_EXT: &str = "der";
/// The private key: one PEM file, readable by the current user only.
const KEY_PEM_EXT: &str = "key";
/// A DER copy of the key that versions before 0.1.0 wrote next to the PEM
/// one. Nothing reads it, and a second copy of the key is a second thing to
/// leak, so it is deleted on load.
const LEGACY_KEY_DER_EXT: &str = "key.der";

/// The CA ready to sign, and whether the one on disk was just replaced — in
/// which case the user has to trust the new certificate again.
pub struct LoadedCa {
    pub authority: IpAwareAuthority,
    /// Why the CA on disk was replaced. `None` when it was reused, or when
    /// there was none and this is the first.
    pub replaced: Option<String>,
    /// The certificate to trust (`mokora-ca.cer`).
    pub cert_path: PathBuf,
}

/// Load the CA in `dir`, or make one. The CA is constrained to `scope`'s
/// domains (X.509 NameConstraints), so the certificate a user trusts can
/// vouch for those sites and nothing else. A CA on disk without that
/// constraint (made by an older Mokora) or constrained to a different list
/// is replaced, key and all.
pub fn load_or_generate(dir: &Path, scope: &MitmScope) -> Result<LoadedCa> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create CA dir {}", dir.display()))?;

    let cert_pem_path = dir.join(format!("{BASENAME}.cer"));
    let key_pem_path = dir.join(format!("{BASENAME}.{KEY_PEM_EXT}"));
    remove_legacy_key_der(dir)?;

    let mut replaced = None;
    let existing = if cert_pem_path.exists() && key_pem_path.exists() {
        let cert_pem = std::fs::read_to_string(&cert_pem_path).context("Failed to read CA cert")?;
        let key_pem = std::fs::read_to_string(&key_pem_path).context("Failed to read CA key")?;
        match scope_mismatch(&cert_pem, scope) {
            None => Some((cert_pem, key_pem)),
            Some(reason) => {
                warn!(
                    "Replacing the CA at {}: {reason}. Trust the new {} again.",
                    dir.display(),
                    cert_pem_path.display()
                );
                replaced = Some(reason);
                None
            }
        }
    } else {
        None
    };

    let (cert_pem, key_pem) = match existing {
        Some((cert_pem, key_pem)) => {
            info!("Loading CA from {}", dir.display());
            // A key written before keys were restricted gets tightened too.
            crate::util::restrict_to_owner(&key_pem_path)
                .context("Failed to restrict the CA key to the current user")?;
            write_cert_copies(dir, &cert_pem, false)?;
            (cert_pem, key_pem)
        }
        None => {
            info!("Generating new CA at {} for {:?}", dir.display(), scope.domains());
            let (cert_pem, key_pem) = generate_ca(scope)?;
            // Key first: a crash before the certificate lands leaves the old
            // certificate, which fails the scope check and is replaced again.
            crate::util::write_private(&key_pem_path, &key_pem)
                .context("Failed to write CA key")?;
            write_cert_copies(dir, &cert_pem, true)?;
            (cert_pem, key_pem)
        }
    };

    let key_pair = KeyPair::from_pem(&key_pem).context("Failed to parse CA key")?;
    let issuer =
        Issuer::from_ca_cert_pem(&cert_pem, key_pair).context("Failed to parse CA certificate")?;

    Ok(LoadedCa {
        authority: IpAwareAuthority::new(
            issuer,
            rustls::crypto::aws_lc_rs::default_provider(),
            scope.clone(),
        ),
        replaced,
        cert_path: cert_pem_path,
    })
}

fn remove_legacy_key_der(dir: &Path) -> Result<()> {
    let p = dir.join(format!("{BASENAME}.{LEGACY_KEY_DER_EXT}"));
    match std::fs::remove_file(&p) {
        Ok(()) => {
            info!("Removed the unused CA key copy {}", p.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("Failed to remove {}", p.display())),
    }
}

/// Write the certificate as `.cer` / `.crt` / `.pem` and `.der`. With
/// `overwrite` every copy is replaced (a new CA); without it only missing
/// copies are filled in.
fn write_cert_copies(dir: &Path, cert_pem: &str, overwrite: bool) -> Result<()> {
    let der = pem_to_der(cert_pem)?;
    let copies = CERT_PEM_EXTS
        .iter()
        .map(|ext| (*ext, cert_pem.as_bytes()))
        .chain(std::iter::once((CERT_DER_EXT, der.as_slice())));
    for (ext, body) in copies {
        let p = dir.join(format!("{BASENAME}.{ext}"));
        if overwrite || !p.exists() {
            crate::util::write_atomic(&p, body)
                .with_context(|| format!("Failed to write CA cert {}", p.display()))?;
        }
    }
    Ok(())
}

fn pem_to_der(cert_pem: &str) -> Result<Vec<u8>> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes())
        .map_err(|e| anyhow!("CA certificate is not PEM: {e}"))?;
    Ok(pem.contents)
}

/// Why the CA certificate `cert_pem` does not fit `scope`, or `None` if it
/// is constrained to exactly `scope`'s domains.
fn scope_mismatch(cert_pem: &str, scope: &MitmScope) -> Option<String> {
    match permitted_dns_names(cert_pem) {
        Err(e) => Some(format!("the certificate could not be read ({e:#})")),
        Ok(None) => Some("it has no domain restriction (made by an older Mokora)".to_string()),
        Ok(Some(mut names)) => {
            names.sort();
            names.dedup();
            (names != scope.domains())
                .then(|| "its domains differ from [proxy] mitm_domains".to_string())
        }
    }
}

/// The dNSName subtrees the CA certificate is constrained to; `None` when it
/// constrains no DNS names at all.
fn permitted_dns_names(cert_pem: &str) -> Result<Option<Vec<String>>> {
    use x509_parser::extensions::GeneralName;

    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes())
        .map_err(|e| anyhow!("not PEM: {e}"))?;
    let cert = pem.parse_x509().map_err(|e| anyhow!("not X.509: {e}"))?;
    let Some(constraints) = cert.name_constraints().map_err(|e| anyhow!("{e}"))? else {
        return Ok(None);
    };
    let names: Vec<String> = constraints
        .value
        .permitted_subtrees
        .iter()
        .flatten()
        .filter_map(|subtree| match subtree.base {
            GeneralName::DNSName(name) => Some(name.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    Ok((!names.is_empty()).then_some(names))
}

fn generate_ca(scope: &MitmScope) -> Result<(String, String)> {
    let key_pair = KeyPair::generate().context("Failed to generate CA key pair")?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Mokora Proxy CA");
    dn.push(DnType::OrganizationName, "Mokora");
    params.distinguished_name = dn;
    // Path length 0: it signs leaves only, never another CA.
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: scope
            .domains()
            .iter()
            .map(|d| GeneralSubtree::DnsName(d.clone()))
            .collect(),
        excluded_subtrees: Vec::new(),
    });

    let cert = params
        .self_signed(&key_pair)
        .context("Failed to self-sign CA certificate")?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

/// Build the parameters for a per-host leaf certificate.
///
/// The crucial difference from hudsucker's built-in `RcgenAuthority`: when the
/// host is an IP literal we emit an `iPAddress` SAN, not a `DnsName`. A DNS SAN
/// holding an IP string is rejected by RFC-6125-compliant TLS clients, so
/// MITMing a server reached by raw IP (Riichi City's game server connects to
/// `<ip>:443`) would otherwise fail the handshake.
///
/// `host` comes from the CONNECT authority, so an IPv6 literal arrives still
/// wrapped in brackets (`[2001:db8::1]`); those are stripped before parsing.
/// A host that is neither an IP literal nor a plain DNS name (letters, digits,
/// `-`, `_`, `.`) is rejected rather than baked into a certificate.
fn leaf_params(host: &str) -> Result<CertificateParams> {
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let san = match unbracketed.parse::<IpAddr>() {
        Ok(ip) => SanType::IpAddress(ip),
        Err(_) => {
            anyhow::ensure!(is_plain_dns_name(host), "host {host:?} is not a plain DNS name");
            SanType::DnsName(Ia5String::try_from(host).context("host is not a valid IA5 string")?)
        }
    };

    let mut params = CertificateParams::default();
    params.serial_number = Some(rng().random::<u64>().into());

    let not_before = OffsetDateTime::now_utc() - Duration::seconds(NOT_BEFORE_OFFSET_SECS);
    params.not_before = not_before;
    params.not_after = not_before + Duration::seconds(LEAF_TTL_SECS);

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, host);
    params.distinguished_name = dn;
    params.subject_alt_names.push(san);
    Ok(params)
}

/// Letters, digits, `-`, `_` and `.`, at most 253 bytes, no empty labels.
/// `_` is outside strict LDH but real hosts use it, so it is allowed, as is
/// one trailing root dot (`example.com.`).
pub(super) fn is_plain_dns_name(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// Aborts every handshake. Served for a host no leaf certificate can be issued
/// for, so the client sees a refused TLS connection instead of the proxy
/// panicking.
#[derive(Debug)]
struct RefuseHandshake;

impl rustls::server::ResolvesServerCert for RefuseHandshake {
    fn resolve(
        &self,
        _client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        None
    }
}

/// A hudsucker [`CertificateAuthority`] that issues leaf certs with the correct
/// SAN type for both hostnames and IP literals (the latter is why we can't use
/// the built-in `RcgenAuthority`). Generated configs are cached per host for the
/// lifetime of the proxy.
///
/// Only hosts inside the [`MitmScope`] get a leaf. `should_intercept` already
/// tunnels the rest, so this is the backstop: a leaf for an outside host
/// would break the CA's NameConstraints and be rejected by the client anyway.
pub struct IpAwareAuthority {
    issuer: Issuer<'static, KeyPair>,
    private_key: PrivateKeyDer<'static>,
    cache: Mutex<HashMap<String, Arc<ServerConfig>>>,
    provider: Arc<CryptoProvider>,
    scope: MitmScope,
}

impl IpAwareAuthority {
    pub fn new(issuer: Issuer<'static, KeyPair>, provider: CryptoProvider, scope: MitmScope) -> Self {
        let private_key =
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(issuer.key().serialize_der()));
        Self {
            issuer,
            private_key,
            cache: Mutex::new(HashMap::new()),
            provider: Arc::new(provider),
            scope,
        }
    }

    fn gen_cert(&self, host: &str) -> Result<CertificateDer<'static>> {
        anyhow::ensure!(
            self.scope.covers(host),
            "host {host} is outside [proxy] mitm_domains"
        );
        Ok(leaf_params(host)?
            .signed_by(self.issuer.key(), &self.issuer)
            .context("failed to sign leaf certificate")?
            .into())
    }
}

impl CertificateAuthority for IpAwareAuthority {
    async fn gen_server_config(&self, authority: &Authority) -> Arc<ServerConfig> {
        let host = authority.host().to_string();
        if let Some(cfg) = self
            .cache
            .lock()
            .expect("CA cache poisoned")
            .get(&host)
            .cloned()
        {
            return cfg;
        }

        let builder = ServerConfig::builder_with_provider(Arc::clone(&self.provider))
            .with_safe_default_protocol_versions()
            .expect("failed to set protocol versions")
            .with_no_client_auth();
        let leaf = self.gen_cert(&host).and_then(|cert| {
            builder
                .clone()
                .with_single_cert(vec![cert], self.private_key.clone_key())
                .context("failed to build ServerConfig")
        });
        let mut server_cfg = match leaf {
            Ok(cfg) => cfg,
            Err(e) => {
                // Not cached: a refusal costs nothing to rebuild, and a
                // transient signing failure must not stick for the session.
                warn!("refusing TLS for {host}: cannot issue a certificate: {e:#}");
                return Arc::new(builder.with_cert_resolver(Arc::new(RefuseHandshake)));
            }
        };
        // hudsucker is built with the http2 feature, so advertise h2 too.
        server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let server_cfg = Arc::new(server_cfg);

        self.cache
            .lock()
            .expect("CA cache poisoned")
            .insert(host, Arc::clone(&server_cfg));
        server_cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_host_gets_ip_san_not_dns() {
        // Regression: Riichi City's game server is reached by raw IP. A DnsName
        // SAN holding an IP would be rejected by the client, dropping the
        // gameplay WebSocket.
        let p = leaf_params("13.112.183.79").unwrap();
        assert!(
            matches!(p.subject_alt_names.as_slice(), [SanType::IpAddress(_)]),
            "IP host must get an iPAddress SAN, got {:?}",
            p.subject_alt_names
        );
    }

    #[test]
    fn ipv6_host_gets_ip_san() {
        let p = leaf_params("2001:db8::1").unwrap();
        assert!(matches!(
            p.subject_alt_names.as_slice(),
            [SanType::IpAddress(_)]
        ));
    }

    /// `Authority::host()` keeps the brackets around an IPv6 literal; the
    /// leaf must still get an iPAddress SAN, not a DnsName of `[...]`.
    #[test]
    fn bracketed_ipv6_from_authority_gets_ip_san() {
        let authority = Authority::from_static("[2001:db8::1]:443");
        let p = leaf_params(authority.host()).unwrap();
        assert!(
            matches!(p.subject_alt_names.as_slice(), [SanType::IpAddress(_)]),
            "got {:?}",
            p.subject_alt_names
        );
    }

    #[test]
    fn hostname_gets_dns_san() {
        let p = leaf_params("game.maj-soul.com").unwrap();
        assert!(matches!(
            p.subject_alt_names.as_slice(),
            [SanType::DnsName(_)]
        ));
    }

    #[test]
    fn plain_dns_name_rules() {
        for ok in ["game.maj-soul.com", "_dmarc.example.com", "example.com.", "localhost"] {
            assert!(is_plain_dns_name(ok), "{ok} should be accepted");
        }
        for bad in ["", ".", "a..b", "例子.com", "a%2eb.com", "a b.com", "a!b.com", "[::1]"] {
            assert!(!is_plain_dns_name(bad), "{bad:?} should be rejected");
        }
        assert!(!is_plain_dns_name(&"a".repeat(64)));
        assert!(leaf_params("a%2eb.com").is_err());
    }

    fn scope(domains: &[&str]) -> MitmScope {
        MitmScope::from_config(&domains.iter().map(|d| d.to_string()).collect::<Vec<_>>())
    }

    /// A fresh CA in `dir` for `domains`, plus its certificate in DER.
    fn fresh_ca(dir: &Path, domains: &[&str]) -> (IpAwareAuthority, Vec<u8>) {
        let loaded = load_or_generate(dir, &scope(domains)).unwrap();
        let der = std::fs::read(dir.join("mokora-ca.der")).unwrap();
        (loaded.authority, der)
    }

    /// Run a full TLS handshake in memory between a client that trusts only
    /// `ca_der` and `server_cfg`, connecting to `server_name`.
    fn handshake(
        server_cfg: Arc<ServerConfig>,
        ca_der: &[u8],
        server_name: &str,
    ) -> std::result::Result<(), rustls::Error> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(ca_der.to_vec())).unwrap();
        let client_cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(server_name.to_string()).unwrap();
        let mut client = rustls::ClientConnection::new(Arc::new(client_cfg), name).unwrap();
        let mut server = rustls::ServerConnection::new(server_cfg).unwrap();

        for _ in 0..10 {
            let mut wire = Vec::new();
            while client.wants_write() {
                client.write_tls(&mut wire).unwrap();
            }
            let mut rd = wire.as_slice();
            while !rd.is_empty() {
                server.read_tls(&mut rd).unwrap();
                server.process_new_packets()?;
            }
            let mut wire = Vec::new();
            while server.wants_write() {
                server.write_tls(&mut wire).unwrap();
            }
            let mut rd = wire.as_slice();
            while !rd.is_empty() {
                client.read_tls(&mut rd).unwrap();
                client.process_new_packets()?;
            }
            if !client.is_handshaking() && !server.is_handshaking() {
                return Ok(());
            }
        }
        Err(rustls::Error::General("handshake did not finish".into()))
    }

    /// A host no certificate can be issued for, or one outside the scope,
    /// gets a config that refuses the handshake — never a panic, never a
    /// cached entry.
    #[tokio::test]
    async fn unsupported_or_outside_host_is_refused_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let (ca, ca_der) = fresh_ca(dir.path(), &["example.com"]);

        // Both pass `http`'s authority parser, so both can reach the CA.
        for bad in ["a!b.example.com:443", "a..b.example.com:443", "google.com:443"] {
            let bad: Authority = bad.parse().expect("http accepts this authority");
            let refused = ca.gen_server_config(&bad).await;
            assert!(
                handshake(refused, &ca_der, "example.com").is_err(),
                "{bad} must be refused"
            );
        }
        assert!(ca.cache.lock().unwrap().is_empty(), "refusals must not be cached");

        let good = ca.gen_server_config(&Authority::from_static("game.example.com:443")).await;
        handshake(good, &ca_der, "game.example.com").expect("a host in scope must handshake");
        assert_eq!(ca.cache.lock().unwrap().len(), 1);
    }

    /// The NameConstraints are what a real verifier enforces: a leaf the CA
    /// signs for a host outside its domains is rejected by the client, even
    /// with the CA trusted. This is the protection if the key ever leaks.
    #[test]
    fn verifier_rejects_a_leaf_outside_the_constraints() {
        let dir = tempfile::tempdir().unwrap();
        load_or_generate(dir.path(), &scope(&["example.com"])).unwrap();
        let cert_pem = std::fs::read_to_string(dir.path().join("mokora-ca.cer")).unwrap();
        let key_pem = std::fs::read_to_string(dir.path().join("mokora-ca.key")).unwrap();
        let ca_der = std::fs::read(dir.path().join("mokora-ca.der")).unwrap();
        let issuer = Issuer::from_ca_cert_pem(&cert_pem, KeyPair::from_pem(&key_pem).unwrap())
            .unwrap();

        let serve = |host: &str| {
            let leaf = leaf_params(host).unwrap().signed_by(issuer.key(), &issuer).unwrap();
            let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(issuer.key().serialize_der()));
            Arc::new(
                ServerConfig::builder_with_provider(Arc::new(
                    rustls::crypto::aws_lc_rs::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![leaf.into()], key)
                .unwrap(),
            )
        };

        handshake(serve("game.example.com"), &ca_der, "game.example.com")
            .expect("control: a host inside the constraints must verify");
        let err = handshake(serve("google.com"), &ca_der, "google.com")
            .expect_err("a leaf outside the constraints must not verify");
        assert!(format!("{err:?}").contains("NameConstraint"), "unexpected error: {err:?}");
    }

    /// Writes a CA the way versions before 0.1.0 did: no NameConstraints,
    /// and the key also as `.key.der`.
    fn write_legacy_ca(dir: &Path) {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        let cert = params.self_signed(&key).unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("mokora-ca.cer"), cert.pem()).unwrap();
        std::fs::write(dir.join("mokora-ca.key"), key.serialize_pem()).unwrap();
        std::fs::write(dir.join("mokora-ca.key.der"), key.serialize_der()).unwrap();
    }

    #[test]
    fn legacy_unconstrained_ca_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        write_legacy_ca(dir.path());
        let old_cert = std::fs::read_to_string(dir.path().join("mokora-ca.cer")).unwrap();

        let loaded = load_or_generate(dir.path(), &scope(&["maj-soul.com"])).unwrap();
        assert!(loaded.replaced.is_some(), "an unconstrained CA must be replaced");
        let new_cert = std::fs::read_to_string(dir.path().join("mokora-ca.cer")).unwrap();
        assert_ne!(old_cert, new_cert);
        assert_eq!(
            permitted_dns_names(&new_cert).unwrap(),
            Some(vec!["maj-soul.com".to_string()])
        );
        assert!(!dir.path().join("mokora-ca.key.der").exists(), "the key copy must be gone");
        // Every public copy is the new certificate.
        for ext in ["crt", "pem"] {
            let copy = std::fs::read_to_string(dir.path().join(format!("mokora-ca.{ext}"))).unwrap();
            assert_eq!(copy, new_cert, ".{ext} must match");
        }
        let der = std::fs::read(dir.path().join("mokora-ca.der")).unwrap();
        assert_eq!(der, pem_to_der(&new_cert).unwrap());
    }

    #[test]
    fn matching_ca_is_reused_and_a_changed_list_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_generate(dir.path(), &scope(&["maj-soul.com", "mahjongsoul.com"]))
            .unwrap();
        assert!(first.replaced.is_none(), "the first CA is not a replacement");
        let cert = std::fs::read_to_string(&first.cert_path).unwrap();

        // Same list, written differently: reused as is.
        let again = load_or_generate(dir.path(), &scope(&["*.MahjongSoul.com", "maj-soul.com"]))
            .unwrap();
        assert!(again.replaced.is_none());
        assert_eq!(std::fs::read_to_string(&again.cert_path).unwrap(), cert);

        // A domain added: replaced.
        let changed = load_or_generate(
            dir.path(),
            &scope(&["maj-soul.com", "mahjongsoul.com", "example.com"]),
        )
        .unwrap();
        assert!(changed.replaced.is_some());
        assert_ne!(std::fs::read_to_string(&changed.cert_path).unwrap(), cert);
    }

    #[test]
    fn new_ca_signs_leaves_only() {
        let dir = tempfile::tempdir().unwrap();
        load_or_generate(dir.path(), &scope(&["example.com"])).unwrap();
        let der = std::fs::read(dir.path().join("mokora-ca.der")).unwrap();
        let (_, cert) = x509_parser::parse_x509_certificate(&der).unwrap();
        let bc = cert.basic_constraints().unwrap().unwrap().value;
        assert!(bc.ca);
        assert_eq!(bc.path_len_constraint, Some(0));
    }
}
