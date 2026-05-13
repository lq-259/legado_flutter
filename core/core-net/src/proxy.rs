//! Proxy management module
//! Supports HTTP/HTTPS/SOCKS5 proxy configuration.

use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Proxy type
#[derive(Debug, Clone, PartialEq)]
pub enum ProxyType {
    Http,
    Https,
    Socks5,
}

impl std::fmt::Display for ProxyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyType::Http => write!(f, "http"),
            ProxyType::Https => write!(f, "https"),
            ProxyType::Socks5 => write!(f, "socks5"),
        }
    }
}

/// Proxy configuration
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    /// Create new proxy config
    pub fn new(proxy_type: ProxyType, host: &str, port: u16) -> Self {
        Self {
            proxy_type,
            host: host.to_string(),
            port,
            username: None,
            password: None,
        }
    }

    /// Set authentication
    pub fn with_auth(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self
    }

    /// Convert to URL string for reqwest
    pub fn to_url(&self) -> String {
        match (&self.username, &self.password) {
            (Some(user), Some(pass)) => {
                let user_encoded = urlencoding::encode(user);
                let pass_encoded = urlencoding::encode(pass);
                format!("{}://{}:{}@{}:{}", self.proxy_type, user_encoded, pass_encoded, self.host, self.port)
            }
            _ => format!("{}://{}:{}", self.proxy_type, self.host, self.port),
        }
    }

    /// Parse from URL like "socks5://127.0.0.1:1080" or "http://user:pass@host:port"
    pub fn from_url(url: &str) -> Option<Self> {
        if let Ok(parsed) = url::Url::parse(url) {
            let proxy_type = match parsed.scheme() {
                "http" => ProxyType::Http,
                "https" => ProxyType::Https,
                "socks5" => ProxyType::Socks5,
                _ => {
                    warn!("Unsupported proxy type: {}", parsed.scheme());
                    return None;
                }
            };

            let host = parsed.host_str().unwrap_or("127.0.0.1").to_string();
            let port = parsed.port().unwrap_or(1080);
            let mut config = ProxyConfig::new(proxy_type, &host, port);

            let user = parsed.username();
            if !user.is_empty() {
                config.username = Some(user.to_string());
            }
            if let Some(pass) = parsed.password() {
                config.password = Some(pass.to_string());
            }

            return Some(config);
        }
        None
    }
}

/// Proxy manager
pub struct ProxyManager {
    proxies: HashMap<String, ProxyConfig>,
    default_proxy: Option<ProxyConfig>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self {
            proxies: HashMap::new(),
            default_proxy: None,
        }
    }

    pub fn set_default_proxy(&mut self, config: ProxyConfig) {
        debug!("Set default proxy: {}", redact_proxy_credentials(&config.to_url()));
        self.default_proxy = Some(config);
    }

    pub fn set_proxy_for_source(&mut self, source_id: &str, config: ProxyConfig) {
        debug!("Set proxy for source {}: {}", source_id, redact_proxy_credentials(&config.to_url()));
        self.proxies.insert(source_id.to_string(), config);
    }

    pub fn get_proxy_for_source(&self, source_id: Option<&str>) -> Option<String> {
        if let Some(id) = source_id {
            if let Some(config) = self.proxies.get(id) {
                debug!("Using dedicated proxy for source {}", id);
                return Some(config.to_url());
            }
        }
        self.default_proxy.as_ref().map(|c| c.to_url())
    }

    pub fn clear_all(&mut self) {
        info!("Clear all proxy configs");
        self.proxies.clear();
        self.default_proxy = None;
    }
}

impl Default for ProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Redact credentials from a proxy URL for safe logging
pub(crate) fn redact_proxy_credentials(url_str: &str) -> String {
    match url::Url::parse(url_str) {
        Ok(mut url) if !url.username().is_empty() => {
            let _ = url.set_username("***");
            let _ = url.set_password(Some("***"));
            url.to_string()
        }
        _ => url_str.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_url_generation() {
        let config = ProxyConfig::new(ProxyType::Socks5, "127.0.0.1", 1080);
        assert_eq!(config.to_url(), "socks5://127.0.0.1:1080");

        let config_with_auth = ProxyConfig::new(ProxyType::Http, "proxy.example.com", 8080)
            .with_auth("user", "pass");
        assert_eq!(config_with_auth.to_url(), "http://user:pass@proxy.example.com:8080");
    }

    #[test]
    fn test_proxy_from_url() {
        let config = ProxyConfig::from_url("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(config.proxy_type, ProxyType::Socks5);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 1080);

        let config_with_auth = ProxyConfig::from_url("http://user:pass@proxy.example.com:8080").unwrap();
        assert_eq!(config_with_auth.username, Some("user".to_string()));
        assert_eq!(config_with_auth.password, Some("pass".to_string()));
    }
}
