//! Legado 统一 HTTP 客户端
//!
//! 支持：
//! - Cookie 持久化（共享 cookie jar）
//! - Charset 自动检测与解码（UTF-8, GBK, GB2312, GB18030）
//! - 自定义请求头
//! - GET/POST 请求
//! - 重试机制
//! - 异步和同步接口

use reqwest::cookie::Jar;
use reqwest::Client as ReqwestClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

use super::url::{LegadoUrl, parse_headers, parse_proxy, get_charset_from_option, guess_charset_from_response};

/// Legado HTTP 客户端
///
/// 封装 reqwest，提供 charset 解码和 cookie 管理。
/// 共享 cookie jar，使后续请求能携带之前设置的 cookie。
#[derive(Clone)]
pub struct LegadoHttpClient {
    client: ReqwestClient,
    cookie_jar: Arc<Jar>,
}

impl LegadoHttpClient {
    pub fn new() -> Self {
        let cookie_jar = Arc::new(Jar::default());
        let client = ReqwestClient::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .cookie_provider(cookie_jar.clone())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, cookie_jar }
    }

    pub fn cookie_jar(&self) -> Arc<Jar> {
        self.cookie_jar.clone()
    }

    /// 发起 HTTP GET 请求
    ///
    /// # Arguments
    /// * `url` - 完整 URL
    /// * `headers` - 额外请求头
    /// * `charset` - 指定字符集（None 时自动检测）
    pub async fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
        charset: Option<&str>,
    ) -> Result<String, String> {
        self.request("GET", url, None, headers, charset, 0, None).await
    }

    /// 发起 HTTP POST 请求
    ///
    /// # Arguments
    /// * `url` - 完整 URL
    /// * `body` - POST body
    /// * `headers` - 额外请求头
    /// * `charset` - 指定字符集
    pub async fn post(
        &self,
        url: &str,
        body: &str,
        headers: &[(String, String)],
        charset: Option<&str>,
    ) -> Result<String, String> {
        self.request("POST", url, Some(body), headers, charset, 0, None).await
    }

    /// 通用请求方法（带重试循环）
    async fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
        charset: Option<&str>,
        retry: i32,
        proxy: Option<&str>,
    ) -> Result<String, String> {
        let max_retries = if retry > 0 { retry.min(3) } else { 1 };
        let mut last_error = String::new();

        let has_content_type = headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        for attempt in 0..max_retries {
            let client = if let Some(proxy_url) = proxy {
                let proxy = reqwest::Proxy::all(proxy_url)
                    .map_err(|e| format!("Invalid proxy URL: {}", e))?;
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(15))
                    .cookie_provider(self.cookie_jar.clone())
                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                    .proxy(proxy)
                    .build()
                    .map_err(|e| format!("Failed to create proxy client: {}", e))?
            } else {
                self.client.clone()
            };

            let mut req = match method {
                "POST" => client.post(url),
                _ => client.get(url),
            };

            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }

            if let Some(body_str) = body {
                if !has_content_type {
                    let ct = if body_str.trim_start().starts_with('{')
                        || body_str.trim_start().starts_with('[')
                    {
                        "application/json"
                    } else {
                        "application/x-www-form-urlencoded"
                    };
                    req = req.header("Content-Type", ct);
                }
                req = req.body(body_str.to_string());
            }

            match req.send().await {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        last_error = format!(
                            "HTTP {}: {}",
                            status.as_u16(),
                            status.canonical_reason().unwrap_or("Unknown")
                        );
                        if attempt + 1 < max_retries {
                            warn!(
                                "Request failed (attempt {}): {}, retrying...",
                                attempt + 1,
                                last_error
                            );
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            continue;
                        }
                        return Err(last_error);
                    }

                    let headers_map: HashMap<String, String> = response
                        .headers()
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.as_str().to_lowercase(),
                                v.to_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect();

                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|e| format!("Failed to read response body: {}", e))?;

                    let encoding_name = charset
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| guess_charset_from_response(&headers_map, &bytes).to_string());

                    return decode_bytes(&bytes, &encoding_name);
                }
                Err(e) => {
                    last_error = format!("HTTP request failed: {}", e);
                    if attempt + 1 < max_retries {
                        warn!(
                            "Request failed (attempt {}): {}, retrying...",
                            attempt + 1,
                            last_error
                        );
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }

        Err(last_error)
    }

    /// 使用 LegadoUrl 发起请求
    ///
    /// 自动处理 URL 选项中的 method, charset, headers, body, retry。
    pub async fn request_with_legado_url(
        &self,
        full_url: &str,
        legado_url: &LegadoUrl,
        keyword: &str,
        page: i32,
    ) -> Result<String, String> {
        self.request_with_legado_url_and_headers(full_url, legado_url, keyword, page, &[]).await
    }

    /// 使用 LegadoUrl 和额外书源请求头发起请求。
    pub async fn request_with_legado_url_and_headers(
        &self,
        full_url: &str,
        legado_url: &LegadoUrl,
        keyword: &str,
        page: i32,
        extra_headers: &[(String, String)],
    ) -> Result<String, String> {
        if legado_url.options.web_view {
            return Err("WEBVIEW_REQUIRED: URL requires platform WebView loading".into());
        }
        let method = legado_url.options.method.as_deref().unwrap_or("GET");
        let charset = get_charset_from_option(&legado_url.options);
        let mut headers = parse_headers(&legado_url.options.headers);
        headers.extend_from_slice(extra_headers);
        let body = legado_url.options.body.as_deref().map(|b| super::url::resolve_post_body(b, keyword, page));
        let retry = legado_url.options.retry;
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

        self.request_with_headers(method, &request_url, body.as_deref(), &all_headers, charset, retry, proxy.as_deref()).await
    }

    async fn request_with_headers(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &[(String, String)],
        charset: Option<&str>,
        retry: i32,
        proxy: Option<&str>,
    ) -> Result<String, String> {
        self.request(method, url, body, headers, charset, retry, proxy).await
    }
}

impl Default for LegadoHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 解码字节数组为字符串
pub(crate) fn decode_bytes(bytes: &[u8], charset: &str) -> Result<String, String> {
    let (text, had_errors) = super::url::decode_response_bytes(bytes, charset);
    if had_errors {
        warn!("Charset decode had errors for encoding {}", charset);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_with_url_option_js_updates_url_and_headers() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/changed")
                .header("X-Test", "ok");
            then.status(200).body("changed-ok");
        });

        let legado_url = LegadoUrl {
            path: server.url("/original"),
            is_relative: false,
            options: super::super::url::UrlOption {
                js: Some(format!(
                    "java.url = '{}'; java.headerMap.put('X-Test', 'ok')",
                    server.url("/changed")
                )),
                ..Default::default()
            },
        };
        let client = LegadoHttpClient::new();
        let result = client
            .request_with_legado_url(&server.url("/original"), &legado_url, "", 1)
            .await
            .unwrap();

        mock.assert();
        assert_eq!(result, "changed-ok");
    }
}
