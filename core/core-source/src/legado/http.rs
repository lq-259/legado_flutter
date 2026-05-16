//! Legado 统一 HTTP 客户端 — 基于 ureq 的阻塞式客户端，通过 spawn_blocking 对外提供 async 接口

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::warn;

use super::url::{
    get_charset_from_option, guess_charset_from_response, parse_headers, parse_proxy, LegadoUrl,
};

const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct LegadoHttpClient {
    agent: ureq::Agent,
    proxy_agents: Arc<Mutex<HashMap<String, ureq::Agent>>>,
    pub(crate) cookie_jar: Arc<reqwest::cookie::Jar>,
}

impl LegadoHttpClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(USER_AGENT)
            .https_only(false)
            .build()
            .new_agent();

        Self {
            agent,
            proxy_agents: Arc::new(Mutex::new(HashMap::new())),
            cookie_jar: Arc::new(reqwest::cookie::Jar::default()),
        }
    }

    fn proxy_agent(&self, proxy_url: &str) -> Result<ureq::Agent, String> {
        let mut cache = self.proxy_agents.lock().map_err(|e| format!("proxy lock: {e}"))?;
        if let Some(a) = cache.get(proxy_url) {
            return Ok(a.clone());
        }
        let proxy = ureq::Proxy::new(proxy_url)
            .map_err(|e| format!("Invalid proxy URL: {}", e))?;
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(USER_AGENT)
            .https_only(false)
            .proxy(Some(proxy))
            .build()
            .new_agent();
        cache.insert(proxy_url.to_string(), agent.clone());
        Ok(agent)
    }

    pub fn cookie_jar(&self) -> Arc<reqwest::cookie::Jar> {
        self.cookie_jar.clone()
    }

    pub async fn get(&self, url: &str, headers: &[(String, String)], charset: Option<&str>) -> Result<String, String> {
        self.do_request("GET", url, None, headers, charset, None).await
    }

    pub async fn post(&self, url: &str, body: &str, headers: &[(String, String)], charset: Option<&str>) -> Result<String, String> {
        self.do_request("POST", url, Some(body), headers, charset, None).await
    }

    async fn do_request(
        &self, method: &str, url: &str, body: Option<&str>,
        headers: &[(String, String)], charset: Option<&str>, proxy: Option<&str>,
    ) -> Result<String, String> {
        let agent = if let Some(p) = proxy { self.proxy_agent(p)? } else { self.agent.clone() };
        let method = method.to_string();
        let url = url.to_string();
        let body_str = body.map(|s| s.to_string());
        let headers = headers.to_vec();
        let charset = charset.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            execute_http(&agent, &method, &url, body_str.as_deref(), &headers, charset.as_deref())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    pub async fn request_with_legado_url(&self, full_url: &str, legado_url: &LegadoUrl, keyword: &str, page: i32) -> Result<String, String> {
        self.request_with_legado_url_and_headers(full_url, legado_url, keyword, page, &[]).await
    }

    pub async fn request_with_legado_url_and_headers(
        &self, full_url: &str, legado_url: &LegadoUrl, keyword: &str, page: i32, extra_headers: &[(String, String)],
    ) -> Result<String, String> {
        if legado_url.options.web_view {
            return Err("WEBVIEW_REQUIRED: URL requires platform WebView loading".into());
        }
        let method = legado_url.options.method.as_deref().unwrap_or("GET");
        let charset = get_charset_from_option(&legado_url.options);
        let mut headers = parse_headers(&legado_url.options.headers);
        headers.extend_from_slice(extra_headers);
        let body = legado_url.options.body.as_deref()
            .map(|b| super::url::resolve_post_body(b, keyword, page));
        let proxy = parse_proxy(&legado_url.options.headers);

        let mut request_url = full_url.to_string();
        let mut all_headers = headers.clone();

        if let Some(script) = legado_url.options.js.as_deref().filter(|s| !s.trim().is_empty()) {
            let js_context = super::js_runtime::UrlJsContext::new(&request_url, &all_headers);
            if let Ok(updated) = super::js_runtime::eval_url_option_js(script, &js_context) {
                request_url = updated.url;
                all_headers = updated.headers;
            }
        }

        self.do_request(method, &request_url, body.as_deref(), &all_headers, charset, proxy.as_deref()).await
    }
}

impl Default for LegadoHttpClient {
    fn default() -> Self { Self::new() }
}

fn execute_http(
    agent: &ureq::Agent, method: &str, url: &str,
    body: Option<&str>, headers: &[(String, String)], charset: Option<&str>,
) -> Result<String, String> {
    let response = if method == "POST" {
        let mut r = agent.post(url);
        for (k, v) in headers { r = r.header(k, v); }
        let b = body.unwrap_or("");
        if !r.headers_ref().is_some_and(|h| h.contains_key("content-type")) {
            let ct = if b.trim_start().starts_with('{') || b.trim_start().starts_with('[') { "application/json" } else { "application/x-www-form-urlencoded" };
            r = r.header("Content-Type", ct);
        }
        r.send(b)
    } else {
        let mut r = agent.get(url);
        for (k, v) in headers { r = r.header(k, v); }
        r.call()
    };

    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!("HTTP {}: {}", code, status_text(code)));
        }
        Err(e) => return Err(format!("HTTP request failed: {e}")),
    };

    let headers_map: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(name, val)| (name.as_str().to_lowercase(), val.to_str().unwrap_or("").to_string()))
        .collect();

    let (parts, body) = response.into_parts();
    drop(parts);

    let mut reader = body.into_reader();
    let mut bytes = Vec::new();
    reader.take(MAX_RESPONSE_BYTES as u64).read_to_end(&mut bytes)
        .map_err(|e| format!("read body: {e}"))?;

    let encoding_name = charset.map(|c| c.to_string())
        .unwrap_or_else(|| guess_charset_from_response(&headers_map, &bytes).to_string());

    decode_bytes(&bytes, &encoding_name)
}

fn status_text(code: u16) -> &'static str {
    match code {
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden", 404 => "Not Found",
        405 => "Method Not Allowed", 429 => "Too Many Requests",
        500 => "Internal Server Error", 502 => "Bad Gateway", 503 => "Service Unavailable",
        _ => "",
    }
}

/// 解码字节数组为字符串
pub(crate) fn decode_bytes(bytes: &[u8], charset: &str) -> Result<String, String> {
    let (text, had_errors) = super::url::decode_response_bytes(bytes, charset);
    if had_errors { warn!("Charset decode had errors for encoding {}", charset); }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_with_url_option_js_updates_url_and_headers() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/changed").header("X-Test", "ok");
            then.status(200).body("changed-ok");
        });
        let legado_url = LegadoUrl {
            path: server.url("/original"), is_relative: false,
            options: super::super::url::UrlOption {
                js: Some(format!("java.url = '{}'; java.headerMap.put('X-Test', 'ok')", server.url("/changed"))),
                ..Default::default()
            },
        };
        let client = LegadoHttpClient::new();
        let result = client.request_with_legado_url(&server.url("/original"), &legado_url, "", 1).await.unwrap();
        mock.assert();
        assert_eq!(result, "changed-ok");
    }
}
