use std::time::Duration;

use crate::model::ErrorInfo;
use crate::settings::{ProxyMode, Settings};

pub const USER_AGENT: &str = concat!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Velora/", env!("CARGO_PKG_VERSION"));

/// Build the HTTP client used for all downloads.
///
/// * TLS certificates are always verified by the platform verifier (SChannel on Windows). There is no
///   option to turn this off.
/// * HTTP/1.1 only: parallel ranges should travel over separate TCP connections. HTTP/2 would
///   multiplex them onto one connection and remove the benefit.
/// * Responses are never transparently decompressed, so byte offsets always refer to the stored entity.
pub fn build_client(settings: &Settings) -> Result<reqwest::Client, ErrorInfo> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(settings.connect_timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(10))
        .http1_only()
        .pool_max_idle_per_host(crate::settings::MAX_CONNECTIONS as usize)
        .pool_idle_timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(30))
        .tcp_nodelay(true);

    match settings.proxy.mode {
        ProxyMode::System => {}
        ProxyMode::None => builder = builder.no_proxy(),
        ProxyMode::Manual => {
            let url = settings.proxy.url.trim();
            if url.is_empty() {
                return Err(ErrorInfo::new("proxy", "Manual proxy is selected but no proxy address is set."));
            }
            let proxy = reqwest::Proxy::all(url)
                .map_err(|_| ErrorInfo::new("proxy", "The proxy address is not valid. Use http://host:port or socks5://host:port."))?;
            builder = builder.proxy(proxy);
        }
    }

    builder.build().map_err(|e| ErrorInfo::new("network", format!("Couldn't initialise networking: {e}")))
}
