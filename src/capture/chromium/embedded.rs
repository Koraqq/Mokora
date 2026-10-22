//! Capture for the game shown inside Mokora's own window.
//!
//! The window's browser is WebView2 — Chromium under another name — so the
//! same DevTools connection that reads a launched Chromium reads it too. The
//! only difference is who starts the browser: here it is Mokora's own web
//! view, made to listen for DevTools on a loopback port of its own (given to
//! the account's web views when they are made; see `hub::views`).
//!
//! The web views of an account share that browser, so the pages seen include
//! the account's own interface; [`super::cdp::run`] leaves those alone.

use super::ShutdownToken;
use super::{launch, make_bridges, CaptureBackend, CaptureCtx, CaptureDescriptor, CaptureKind};
use crate::capture::chromium::cdp;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tracing::{info, warn};

/// The game shown in the app's own window, read through DevTools `port`.
pub struct EmbeddedBackend {
    port: u16,
}

impl EmbeddedBackend {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

#[async_trait]
impl CaptureBackend for EmbeddedBackend {
    async fn run(self: Box<Self>, ctx: CaptureCtx, shutdown: ShutdownToken) -> Result<()> {
        let endpoint = launch::wait_for_devtools_endpoint(Path::new("no-profile"), Some(self.port))
            .await
            .context("reading the embedded browser's DevTools endpoint")?;
        info!("embedded browser DevTools endpoint: {endpoint}");

        let bridges = make_bridges(&ctx);
        let cdp_run = cdp::run(
            &endpoint,
            bridges,
            ctx.mjai_bus.clone(),
            ctx.session.inspector(),
            ctx.autoplay.clone(),
            ctx.http.clone(),
            ctx.block_telemetry,
            true,
        );
        tokio::select! {
            _ = shutdown.wait() => {
                info!("embedded capture: shutdown requested");
                Ok(())
            }
            r = cdp_run => {
                if let Err(e) = &r {
                    warn!("embedded capture: CDP loop error: {e:#}");
                }
                r
            }
        }
    }

    fn descriptor(&self) -> CaptureDescriptor {
        CaptureDescriptor {
            kind: CaptureKind::Chromium,
            label: "built-in".to_string(),
        }
    }
}
