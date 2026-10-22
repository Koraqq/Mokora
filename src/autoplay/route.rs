//! The fastest Mahjong Soul gateway ("線路"), for a reload of the game page.
//!
//! The client keeps the gateway it connects through in
//! `localStorage.routeID` (`"route-4"`…) and opens that one on the next
//! load — checked on a fresh profile: set before a reload, the client opened
//! exactly the gateway named. The gateways themselves are listed in the
//! client's `config.json` (`ip[].gateways[] = {id, url}`), found the way the
//! client finds it: `version.json` → `resversion<v>.json` → the prefixed
//! `config.json`, all beside the game page.
//!
//! "Fastest" is the quickest TCP connect to each gateway's host (the best of
//! a few tries): the round trip the game's own traffic pays. Compared in
//! whole milliseconds, as the login screen shows them; a tie goes to the
//! lower route number.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::net::TcpStream;
use tracing::{debug, info};

/// Connect attempts per gateway; the best one counts.
const TRIES: u32 = 3;
/// A gateway that takes longer than this to answer is out of the running.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Each fetch of the client's config files.
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// One gateway of the client's config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gateway {
    /// What the client stores as `routeID`.
    pub id: String,
    pub host: String,
    pub port: u16,
}

/// The gateways the game page at `page_url` (`https://game.maj-soul.com/1/`)
/// can use, from its client's `config.json`.
pub async fn gateways(page_url: &str) -> Result<Vec<Gateway>> {
    let base = base_of(page_url).ok_or_else(|| anyhow!("not a game page: {page_url}"))?;
    let http = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .context("building the HTTP client")?;
    let get = |path: String| {
        let http = http.clone();
        let url = format!("{base}{path}");
        async move {
            http.get(&url)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .with_context(|| format!("fetching {url}"))?
                .json::<serde_json::Value>()
                .await
                .with_context(|| format!("reading {url}"))
        }
    };
    let version = get("version.json".into()).await?;
    let version = version["version"]
        .as_str()
        .ok_or_else(|| anyhow!("version.json has no version"))?
        .to_string();
    let res = get(format!("resversion{version}.json")).await?;
    let prefix = res["res"]["config.json"]["prefix"]
        .as_str()
        .ok_or_else(|| anyhow!("resversion has no config.json"))?
        .to_string();
    let config = get(format!("{prefix}/config.json")).await?;
    let found = parse_config(&config);
    if found.is_empty() {
        return Err(anyhow!("config.json lists no gateways"));
    }
    Ok(found)
}

/// `https://game.maj-soul.com/1/` from the page's URL (path up to its last
/// `/`, query and fragment dropped).
fn base_of(page_url: &str) -> Option<String> {
    let url = page_url.split(['?', '#']).next()?;
    if !url.starts_with("https://") {
        return None;
    }
    let cut = url.rfind('/')?;
    (cut >= "https://".len()).then(|| url[..=cut].to_string())
}

/// The gateways listed in a client `config.json`.
fn parse_config(config: &serde_json::Value) -> Vec<Gateway> {
    let Some(groups) = config["ip"].as_array() else {
        return Vec::new();
    };
    groups
        .iter()
        .filter_map(|g| g["gateways"].as_array())
        .flatten()
        .filter_map(|g| {
            let id = g["id"].as_str()?.to_string();
            let url = g["url"].as_str()?;
            let (host, port) = host_port(url)?;
            Some(Gateway { id, host, port })
        })
        .collect()
}

/// Host and port of an `https://host[:port]` URL.
fn host_port(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split('/').next()?;
    match authority.rsplit_once(':') {
        Some((host, port)) => Some((host.to_string(), port.parse().ok()?)),
        None => Some((authority.to_string(), 443)),
    }
}

/// Each gateway's best connect time; `None` where it did not answer.
pub async fn measure(gateways: &[Gateway]) -> Vec<(String, Option<Duration>)> {
    let probes = gateways.iter().map(|g| async move {
        let mut best: Option<Duration> = None;
        for _ in 0..TRIES {
            let start = tokio::time::Instant::now();
            let connect = TcpStream::connect((g.host.as_str(), g.port));
            if let Ok(Ok(_stream)) = tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
                let took = start.elapsed();
                best = Some(best.map_or(took, |b| b.min(took)));
            }
        }
        debug!("route: {} ({}:{}) {best:?}", g.id, g.host, g.port);
        (g.id.clone(), best)
    });
    futures_util::future::join_all(probes).await
}

/// The quickest gateway that answered: fewest whole milliseconds, then the
/// lower route number.
pub fn fastest(measured: &[(String, Option<Duration>)]) -> Option<(String, Duration)> {
    measured
        .iter()
        .filter_map(|(id, t)| t.map(|t| (id.clone(), t)))
        .min_by_key(|(id, t)| (t.as_millis(), route_number(id)))
}

/// `4` of `route-4`; unnumbered ids sort last.
fn route_number(id: &str) -> u32 {
    id.rsplit('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX)
}

/// Find, time and pick, among `gateways`.
pub async fn pick_among(gateways: &[Gateway]) -> Result<(String, Duration)> {
    let measured = measure(gateways).await;
    let summary: Vec<String> = measured
        .iter()
        .map(|(id, t)| match t {
            Some(t) => format!("{id} {} ms", t.as_millis()),
            None => format!("{id} no answer"),
        })
        .collect();
    info!("route: {}", summary.join(", "));
    fastest(&measured).ok_or_else(|| anyhow!("no gateway answered"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the live client's `config.json` (trimmed).
    #[test]
    fn gateways_come_from_the_client_config() {
        let config = serde_json::json!({
            "ip": [{
                "name": "player",
                "gateways": [
                    {"id": "route-2", "url": "https://route-2.maj-soul.com"},
                    {"id": "route-3", "url": "https://route-3.maj-soul.com:8443"}
                ],
                "system_email_url": "https://common-202411.maj-soul.com"
            }],
            "goods_sheleve_id": "shelves_001"
        });
        assert_eq!(
            parse_config(&config),
            vec![
                Gateway {
                    id: "route-2".into(),
                    host: "route-2.maj-soul.com".into(),
                    port: 443
                },
                Gateway {
                    id: "route-3".into(),
                    host: "route-3.maj-soul.com".into(),
                    port: 8443
                },
            ]
        );
        assert!(parse_config(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn the_config_sits_beside_the_page() {
        assert_eq!(
            base_of("https://game.maj-soul.com/1/").as_deref(),
            Some("https://game.maj-soul.com/1/")
        );
        assert_eq!(
            base_of("https://game.maj-soul.com/1/index.html?x=1#y").as_deref(),
            Some("https://game.maj-soul.com/1/")
        );
        assert_eq!(base_of("about:blank"), None);
    }

    #[test]
    fn the_fastest_answering_gateway_wins() {
        let ms = Duration::from_millis;
        let measured = vec![
            ("route-2".to_string(), Some(ms(40))),
            ("route-3".to_string(), None),
            ("route-4".to_string(), Some(ms(25))),
        ];
        assert_eq!(fastest(&measured), Some(("route-4".to_string(), ms(25))));
        assert_eq!(fastest(&[("route-3".to_string(), None)]), None);
    }

    /// The same whole milliseconds: the lower route number.
    #[test]
    fn a_tie_goes_to_the_lower_route() {
        let measured = vec![
            ("route-5".to_string(), Some(Duration::from_micros(13_100))),
            ("route-2".to_string(), Some(Duration::from_micros(13_900))),
            ("route-4".to_string(), Some(Duration::from_millis(16))),
        ];
        assert_eq!(fastest(&measured).unwrap().0, "route-2");
    }
}
