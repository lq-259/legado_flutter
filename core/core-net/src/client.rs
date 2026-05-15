//! # HttpClient - HTTP 客户端封装
//!
//! 基于 reqwest 封装的异步 HTTP 客户端，提供：
//! - 自动 Cookie 管理
//! - 代理支持
//! - 重试机制（指数退避）
//! - 超时控制
//! - 并发请求限制

use crate::cookie::CookieManager;
use crate::proxy::redact_proxy_credentials;
use crate::ProxyConfig;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, SET_COOKIE, USER_AGENT,
};
use reqwest::{Client, RequestBuilder, Response};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// HTTP 客户端配置
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub max_concurrent: usize,
    pub max_retries: usize,
    pub max_response_bytes: u64,
    pub user_agent: String,
    pub proxy: Option<ProxyConfig>,
    pub cookie_persistence_path: Option<String>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            connect_timeout_secs: 10,
            max_concurrent: 10,
            max_retries: 3,
            max_response_bytes: 10 * 1024 * 1024,
            user_agent: "Legado-Flutter/0.1.0".to_string(),
            proxy: None,
            cookie_persistence_path: None,
        }
    }
}

/// 封装的 HTTP 客户端
pub struct HttpClient {
    client: Client,
    semaphore: Arc<Semaphore>,
    config: HttpClientConfig,
    cookie_manager: CookieManager,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端
    /// 对应原 Legado 的 help/http/ 网络请求模块
    pub fn new(config: HttpClientConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
        );

        let mut builder = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(config.connect_timeout_secs));

        if let Some(proxy_config) = &config.proxy {
            use reqwest::Proxy;
            let proxy_url = proxy_config.to_url();
            debug!("使用代理: {}", redact_proxy_credentials(&proxy_url));
            builder = builder.proxy(Proxy::all(&proxy_url)?);
        }

        let client = builder.build()?;

        let cookie_manager = match &config.cookie_persistence_path {
            Some(path) => match CookieManager::load_persistent_cookies(path) {
                Ok(manager) => {
                    info!("从 {} 加载了持久化 Cookie", path);
                    manager
                }
                Err(e) => {
                    warn!("加载 Cookie 文件 {} 失败: {}，使用空 Cookie 存储", path, e);
                    CookieManager::new()?
                }
            },
            None => CookieManager::new()?,
        };

        Ok(Self {
            client,
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            cookie_manager,
        })
    }

    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// 发起 GET 请求（带重试，自动管理 Cookie）
    pub async fn get(&self, url: &str) -> Result<Response, reqwest::Error> {
        self.request_with_retry(url, || self.client.get(url)).await
    }

    /// 发起 POST 请求（带重试，自动管理 Cookie）
    pub async fn post(&self, url: &str, body: String) -> Result<Response, reqwest::Error> {
        self.request_with_retry(url, move || self.client.post(url).body(body.clone()))
            .await
    }

    /// 注入 Cookie 到请求头
    fn inject_cookies(&self, url: &str, builder: RequestBuilder) -> RequestBuilder {
        match self.cookie_manager.get_cookies(url) {
            Ok(cookies) if !cookies.is_empty() => {
                debug!("为 {} 注入 Cookie ({} bytes)", url, cookies.len());
                builder.header(COOKIE, cookies)
            }
            _ => builder,
        }
    }

    /// 从响应中提取 Set-Cookie 并存入管理器
    fn extract_cookies(&self, url: &str, response: &Response) {
        for header in response.headers().get_all(SET_COOKIE) {
            if let Ok(cookie_str) = header.to_str() {
                if let Err(e) = self.cookie_manager.add_cookie(cookie_str, url) {
                    let cookie_name = cookie_str.split('=').next().unwrap_or("<unknown>");
                    warn!("解析 Set-Cookie 失败: {} - {}", cookie_name, e);
                }
            }
        }
    }

    /// 保存 Cookie 到持久化文件
    pub fn save_cookies(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.config.cookie_persistence_path {
            Some(path) => self.cookie_manager.save_persistent_cookies(path),
            None => Err("未配置 Cookie 持久化路径".into()),
        }
    }

    /// 获取 Cookie 管理器引用（用于外部操作如 clear_domain）
    pub fn cookie_manager(&self) -> &CookieManager {
        &self.cookie_manager
    }

    /// 带重试的请求执行（指数退避），自动管理 Cookie 注入和提取
    async fn request_with_retry<F>(
        &self,
        url: &str,
        request_fn: F,
    ) -> Result<Response, reqwest::Error>
    where
        F: Fn() -> RequestBuilder + Clone,
    {
        let mut retries = 0;
        let max_retries = self.config.max_retries;

        loop {
            // NOTE: The Semaphore is never closed during HttpClient's lifetime,
            // so this .expect() only panics on a programming error.
            let permit = self
                .semaphore
                .acquire()
                .await
                .expect("Semaphore closed unexpectedly");

            debug!("发起请求，重试次数: {}/{}", retries, max_retries);
            let builder = self.inject_cookies(url, request_fn());
            let response = builder.send().await;

            match response {
                Ok(resp) => {
                    self.extract_cookies(url, &resp);
                    if resp.status().is_success() {
                        drop(permit);
                        return Ok(resp);
                    } else if resp.status().is_server_error() && retries < max_retries {
                        retries += 1;
                        let backoff_ms = retry_backoff_ms(retries as u32);
                        warn!("服务器错误 {}，将在 {}ms 后重试", resp.status(), backoff_ms);
                        drop(permit);
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        continue;
                    } else {
                        drop(permit);
                        return Ok(resp);
                    }
                }
                Err(e) => {
                    if retries < max_retries && (e.is_timeout() || e.is_connect()) {
                        retries += 1;
                        let backoff_ms = retry_backoff_ms(retries as u32);
                        warn!("请求失败: {}，将在 {}ms 后重试", e, backoff_ms);
                        drop(permit);
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        continue;
                    } else {
                        drop(permit);
                        return Err(e);
                    }
                }
            }
        }
    }

    /// 获取文本内容（自动处理编码）
    pub async fn get_text(&self, url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let mut response = self.get(url).await?;

        let encoding = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_charset);

        let max = self.config.max_response_bytes as usize;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("读取响应失败: {}", e))?
        {
            if bytes.len() + chunk.len() > max {
                return Err(format!("响应体超过上限 {} 字节", max).into());
            }
            bytes.extend_from_slice(&chunk);
        }

        let text = if let Some(enc) = encoding {
            match enc.to_lowercase().as_str() {
                "gbk" | "gb2312" | "gb18030" => {
                    let (text, _, _) = encoding_rs::GB18030.decode(&bytes);
                    text.into_owned()
                }
                "big5" => {
                    let (text, _, _) = encoding_rs::BIG5.decode(&bytes);
                    text.into_owned()
                }
                _ => String::from_utf8_lossy(&bytes).to_string(),
            }
        } else {
            let (encoding, bom_len) =
                encoding_rs::Encoding::for_bom(&bytes).unwrap_or((encoding_rs::UTF_8, 0));
            let (text, _, _) = encoding.decode(&bytes[bom_len..]);
            text.into_owned()
        };

        Ok(text)
    }

    /// 获取 JSON 内容
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let response = self.get(url).await?;
        let json = response.json::<T>().await?;
        Ok(json)
    }
}

fn retry_backoff_ms(retries: u32) -> u64 {
    100u64
        .saturating_mul(2u64.saturating_pow(retries))
        .min(30_000)
}

/// 便捷函数：创建默认 HttpClient
pub fn create_client() -> Result<HttpClient, Box<dyn std::error::Error>> {
    HttpClient::new(HttpClientConfig::default())
}

/// 从 Content-Type 头解析 charset
fn parse_charset(content_type: &str) -> Option<String> {
    content_type.split(';').map(str::trim).find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key.eq_ignore_ascii_case("charset") {
            Some(value.trim_matches('"').to_ascii_lowercase())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs;

    #[test]
    fn test_http_client_cookie_lifecycle() {
        let path = temp_dir().join("test_http_lifecycle.json");
        let path_str = path.to_string_lossy().to_string();

        // 创建 client1，添加 cookie 并持久化
        let config = HttpClientConfig {
            cookie_persistence_path: Some(path_str.clone()),
            ..Default::default()
        };
        let client1 = HttpClient::new(config.clone()).unwrap();

        client1
            .cookie_manager()
            .add_cookie("lifecycle=test; Max-Age=3600", "https://example.com")
            .unwrap();
        client1
            .cookie_manager()
            .add_cookie("session=abc", "https://example.com")
            .unwrap();
        client1.save_cookies().unwrap();

        // 创建 client2，加载持久化 cookie，验证生命周期
        let client2 = HttpClient::new(config).unwrap();
        let cookies = client2
            .cookie_manager()
            .get_cookies("https://example.com")
            .unwrap();
        assert!(cookies.contains("lifecycle=test"));

        // 验证持久化文件可以再次写入
        client2.save_cookies().unwrap();

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_http_client_no_persist_path() {
        let client = HttpClient::new(HttpClientConfig::default()).unwrap();
        client
            .cookie_manager()
            .add_cookie("x=1", "https://example.com")
            .unwrap();
        let result = client.save_cookies();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_http_client_set_cookie_from_server() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200)
                .header("Set-Cookie", "srv_cookie=val1; Max-Age=3600; Path=/")
                .body("OK");
        });

        let client = HttpClient::new(HttpClientConfig::default()).unwrap();
        let resp = client.get(&server.url("/test")).await.unwrap();
        assert_eq!(resp.status(), 200);

        let cookies = client
            .cookie_manager()
            .get_cookies(&server.url("/test"))
            .unwrap();
        assert!(
            cookies.contains("srv_cookie=val1"),
            "expected srv_cookie, got: {}",
            cookies
        );

        mock.assert();
    }

    #[tokio::test]
    async fn test_http_client_cookie_injection_on_next_request() {
        use httpmock::prelude::*;

        let server = MockServer::start();

        let auth_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/auth")
                .header("Cookie", "session=abc123");
            then.status(200).body("authenticated");
        });

        let login_mock = server.mock(|when, then| {
            when.method(GET).path("/login");
            then.status(200)
                .header("Set-Cookie", "session=abc123; Path=/")
                .body("logged in");
        });

        let client = HttpClient::new(HttpClientConfig::default()).unwrap();

        client.get(&server.url("/login")).await.unwrap();
        login_mock.assert();

        client.get(&server.url("/auth")).await.unwrap();
        auth_mock.assert();
    }

    #[tokio::test]
    async fn test_http_client_persistence_across_instances() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let path = temp_dir().join("test_http_persist.json");
        let path_str = path.to_string_lossy().to_string();

        let mock = server.mock(|when, then| {
            when.method(GET).path("/set");
            then.status(200)
                .header("Set-Cookie", "persist_cookie=xyz; Max-Age=3600; Path=/")
                .body("OK");
        });

        let config = HttpClientConfig {
            cookie_persistence_path: Some(path_str.clone()),
            ..Default::default()
        };
        let client1 = HttpClient::new(config.clone()).unwrap();
        client1.get(&server.url("/set")).await.unwrap();
        client1.save_cookies().unwrap();

        let client2 = HttpClient::new(config).unwrap();
        let cookies = client2
            .cookie_manager()
            .get_cookies(&server.url("/set"))
            .unwrap();
        assert!(cookies.contains("persist_cookie=xyz"));

        mock.assert();
        fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_http_client_post_set_cookie() {
        use httpmock::prelude::*;

        let server = MockServer::start();

        let mock = server.mock(|when, then| {
            when.method(POST).path("/submit").body("data=1");
            then.status(200)
                .header("Set-Cookie", "post_cookie=val1; Max-Age=3600; Path=/")
                .body("saved");
        });

        let client = HttpClient::new(HttpClientConfig::default()).unwrap();
        let resp = client
            .post(&server.url("/submit"), "data=1".to_string())
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let cookies = client
            .cookie_manager()
            .get_cookies(&server.url("/submit"))
            .unwrap();
        assert!(
            cookies.contains("post_cookie=val1"),
            "expected post_cookie, got: {}",
            cookies
        );

        mock.assert();
    }
}
