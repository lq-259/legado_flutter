//! # Cookie 管理模块
//!
//! 提供 Cookie 的持久化存储和管理功能。
//! 对应原 Legado 的 Cookie 管理逻辑，使用 cookie_store 库实现。
//!
//! ## 持久化策略
//! - `save_persistent_cookies` / `load_persistent_cookies` 仅保存/加载持久化 Cookie
//!   （即带有 Max-Age 或 Expires 属性的 Cookie），不处理会话级 Cookie。
//! - cookie_store 的 serde_json 序列化默认会跳过到期和会话 Cookie。
//! - clear_domain 通过内部 raw_cookies 跟踪实现，同时支持会话级和持久化 Cookie；
//!   旧格式加载（raw_cookies 为空）时回退到 JSON 过滤持久化 Cookie。

use std::path::Path;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use cookie_store::{CookieStore, Cookie as StoreCookie};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info};

/// Cookie 条目（用于跟踪原始 cookie 数据，包括会话级 Cookie）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CookieEntry {
    raw_cookie: String,
    url: String,
}

/// Cookie 管理器内部状态
struct CookieManagerInner {
    store: CookieStore,
    raw_cookies: Vec<CookieEntry>,
}

/// Cookie 管理器
/// 封装 cookie_store 提供持久化功能，使用 Arc<Mutex<>> 支持跨线程共享
#[derive(Clone)]
pub struct CookieManager {
    inner: Arc<Mutex<CookieManagerInner>>,
}

impl CookieManager {
    /// 创建新的 Cookie 管理器
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            inner: Arc::new(Mutex::new(CookieManagerInner {
                store: CookieStore::default(),
                raw_cookies: Vec::new(),
            })),
        })
    }

    /// 从文件加载持久化 Cookie
    /// 仅加载持久化 Cookie（有 Max-Age 或 Expires），不加载会话 Cookie
    pub fn load_persistent_cookies<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        debug!("从文件加载 Cookie: {:?}", path);

        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // 尝试加载新格式 {"store": ..., "raw_cookies": [...]}
        // 向后兼容旧格式（直接是 CookieStore 的 JSON 数组）
        let (store, raw_cookies) = match serde_json::from_str::<Value>(&contents) {
            Ok(value) if value.get("store").is_some() => {
                let store: CookieStore = serde_json::from_value(value["store"].clone())?;
                let raw: Vec<CookieEntry> =
                    serde_json::from_value(value["raw_cookies"].clone()).unwrap_or_default();
                (store, raw)
            }
            _ => {
                let store: CookieStore = serde_json::from_str(&contents)?;
                (store, Vec::new())
            }
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(CookieManagerInner { store, raw_cookies })),
        })
    }

    /// 保存持久化 Cookie 到文件（原子写入：先写临时文件再 rename）
    /// 仅保存持久化 Cookie（有 Max-Age 或 Expires），不保存会话 Cookie
    pub fn save_persistent_cookies<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = path.as_ref();
        debug!("保存 Cookie 到文件: {:?}", path);

        let inner = self.inner.lock().unwrap();
        // 只保存持久化 raw_cookies
        let persistent_raw: Vec<&CookieEntry> = inner
            .raw_cookies
            .iter()
            .filter(|e| is_persistent_cookie(&e.raw_cookie))
            .collect();

        let data = serde_json::json!({
            "store": &inner.store,
            "raw_cookies": persistent_raw,
        });
        let json = serde_json::to_string_pretty(&data)?;

        let tmp_path = path.with_extension("tmp");
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
        }
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// 添加 Cookie（对应原 Legado 的 addCookie 方法）
    /// 去重键基于已解析 cookie 的 name/domain_flag/path，与 CookieStore 语义一致
    pub fn add_cookie(
        &self,
        cookie_str: &str,
        url: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = Url::parse(url)?;
        let parsed = StoreCookie::parse(cookie_str, &url)?;

        // 从已解析 cookie 提取去重键（与 CookieStore 的 insert 内部键一致）
        let dedup_name = parsed.name().to_string();
        let dedup_domain_flag = match parsed.domain() {
            Some(d) => Some(format!("domain:{}", d)),
            None => url.host_str().map(|h| format!("host:{}", h)),
        };
        let dedup_path = parsed
            .path()
            .map(|s| s.to_string())
            .unwrap_or_else(|| cookie_default_path_from_url(&url));

        let cookie = parsed.into_owned();

        let mut inner = self.inner.lock().unwrap();
        inner.store.insert(cookie, &url)?;

        inner.raw_cookies.retain(|existing| {
            if let Ok(existing_url) = Url::parse(&existing.url) {
                if let Ok(existing_parsed) =
                    StoreCookie::parse(&existing.raw_cookie, &existing_url)
                {
                    let en = existing_parsed.name().to_string();
                    let ed = match existing_parsed.domain() {
                        Some(d) => Some(format!("domain:{}", d)),
                        None => existing_url.host_str().map(|h| format!("host:{}", h)),
                    };
                    let ep = existing_parsed
                        .path()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| cookie_default_path_from_url(&existing_url));
                    return !(en == dedup_name && ed == dedup_domain_flag && ep == dedup_path);
                }
            }
            true
        });

        inner.raw_cookies.push(CookieEntry {
            raw_cookie: cookie_str.to_string(),
            url: url.to_string(),
        });
        debug!("添加 Cookie: {}",
            cookie_str.split(';').next().unwrap_or(cookie_str)
                .split_once('=').map(|(n,_)| format!("{}=***", n))
                .unwrap_or_else(|| cookie_str.to_string()));

        Ok(())
    }

    /// 获取指定 URL 的所有 Cookie（对应原 Legado 的 getCookie 方法）
    pub fn get_cookies(&self, url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let url = Url::parse(url)?;
        let inner = self.inner.lock().unwrap();
        let cookies: Vec<String> = inner
            .store
            .get_request_values(&url)
            .map(|(name, value)| format!("{}={}", name, value))
            .collect();

        Ok(cookies.join("; "))
    }

    /// 清除所有 Cookie
    pub fn clear_all(&self) {
        info!("清除所有 Cookie");
        let mut inner = self.inner.lock().unwrap();
        inner.store = CookieStore::default();
        inner.raw_cookies.clear();
    }

    /// 清除指定域名的 Cookie
    /// 基于 Cookie 自身 Domain 属性语义匹配（支持子域匹配，避免简单 ends_with 误删）。
    /// 旧格式加载（raw_cookies 为空）时回退到 JSON 过滤持久化 Cookie。
    pub fn clear_domain(&self, domain: &str) {
        debug!("清除域名 Cookie: {}", domain);
        let mut inner = self.inner.lock().unwrap();

        if inner.raw_cookies.is_empty() {
            // 旧格式加载回退：对持久化 Cookie 做 JSON 过滤
            let value = serde_json::to_value(&inner.store).unwrap_or_default();
            if let Value::Array(cookies) = value {
                let filtered: Vec<Value> = cookies
                    .into_iter()
                    .filter(|c| !cookie_json_domain_matches(c, domain))
                    .collect();
                inner.store =
                    serde_json::from_value(Value::Array(filtered)).unwrap_or_default();
            }
            return;
        }

        let (keep, _remove): (Vec<CookieEntry>, Vec<CookieEntry>) = inner
            .raw_cookies
            .iter()
            .cloned()
            .partition(|entry| {
                let eff_domain = cookie_effective_domain(entry);
                if eff_domain.is_empty() {
                    return true;
                }
                !cookie_domain_matches(&eff_domain, domain)
            });

        // 用保留的条目重建 CookieStore
        inner.store = CookieStore::default();
        for entry in &keep {
            if let Ok(url) = Url::parse(&entry.url) {
                if let Ok(cookie) = StoreCookie::parse(&entry.raw_cookie, &url) {
                    let _ = inner.store.insert(cookie.into_owned(), &url);
                }
            }
        }

        inner.raw_cookies = keep;
    }
}

impl Default for CookieManager {
    fn default() -> Self {
        Self::new().expect("创建 CookieManager 失败")
    }
}

/// 从 raw cookie 字符串提取 Domain 属性值
fn extract_domain_attr(raw_cookie: &str) -> Option<String> {
    let lower = raw_cookie.to_lowercase();
    for part in lower.split(';') {
        let part = part.trim();
        if let Some(domain) = part.strip_prefix("domain=") {
            let d = domain.trim();
            if !d.is_empty() {
                return Some(d.to_string());
            }
        }
    }
    None
}

/// 从 raw cookie 字符串提取 Path 属性值（大小写敏感，默认 "/"）
/// 注意：此函数仅用于外部参考，实际去重已改为从 StoreCookie::parse 提取 Path，
/// 避免大小写/默认值偏差。保留此函数供将来 clear_domain 或其他路径使用。
#[allow(dead_code)]
fn cookie_path(raw_cookie: &str) -> String {
    for part in raw_cookie.split(';') {
        let trimmed = part.trim();
        if trimmed.to_lowercase().starts_with("path=") {
            let p = trimmed[5..].trim();
            if !p.is_empty() {
                return p.to_string();
            }
        }
    }
    "/".to_string()
}

/// 从 URL path 推导默认 Cookie Path（用于无显式 Path 属性的 Cookie）
fn cookie_default_path_from_url(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() { return "/".to_string(); }
    match path.rfind('/') {
        Some(pos) if pos > 0 => path[..=pos].to_string(),
        _ => "/".to_string(),
    }
}

/// 获取 Cookie 条目的有效域名（Domain 属性存在时使用属性值，否则使用 URL host）
fn cookie_effective_domain(entry: &CookieEntry) -> String {
    extract_domain_attr(&entry.raw_cookie).unwrap_or_else(|| {
        Url::parse(&entry.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_default()
    })
}

/// Cookie 域名语义匹配：eff_domain == target 或 eff_domain 是 target 的子域
/// 避免简单 ends_with 误删 badexample.com 等后缀匹配域
fn cookie_domain_matches(effective_domain: &str, target_domain: &str) -> bool {
    effective_domain == target_domain
        || effective_domain.ends_with(&format!(".{}", target_domain))
}

/// CookieStore JSON 中单个 Cookie 的 domain 是否匹配目标（用于旧格式回退）
fn cookie_json_domain_matches(cookie: &Value, domain: &str) -> bool {
    let domain_str = cookie
        .get("domain")
        .and_then(|d| d.get("Domain").or_else(|| d.get("HostOnly")))
        .and_then(|v| v.as_str());
    match domain_str {
        Some(d) if !d.is_empty() => cookie_domain_matches(d, domain),
        _ => false,
    }
}

/// 判断是否为持久化 Cookie（包含 Max-Age 或 Expires 属性）
fn is_persistent_cookie(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    lower.contains("max-age") || lower.contains("expires")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs;

    #[test]
    fn test_create_and_default() {
        let manager = CookieManager::default();
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(cookies, "");
    }

    #[test]
    fn test_add_and_get_cookie() {
        let manager = CookieManager::default();
        manager.add_cookie("session=abc123", "https://example.com").unwrap();
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("session=abc123"));
    }

    #[test]
    fn test_get_cookies_no_match() {
        let manager = CookieManager::default();
        manager.add_cookie("token=xyz", "https://example.com").unwrap();
        let cookies = manager.get_cookies("https://other.com").unwrap();
        assert_eq!(cookies, "");
    }

    #[test]
    fn test_clear_all() {
        let manager = CookieManager::default();
        manager.add_cookie("a=1", "https://example.com").unwrap();
        manager.add_cookie("b=2", "https://example.com").unwrap();
        manager.clear_all();
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(cookies, "");
    }

    #[test]
    fn test_save_and_load() {
        let manager = CookieManager::default();
        manager.add_cookie("loaded=true; Max-Age=3600", "https://example.com").unwrap();

        let path = temp_dir().join("test_cookies.json");
        manager.save_persistent_cookies(&path).unwrap();

        let loaded = CookieManager::load_persistent_cookies(&path).unwrap();
        let cookies = loaded.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("loaded=true"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_save_and_load_backward_compat() {
        let manager = CookieManager::default();
        manager.add_cookie("oldformat=true; Max-Age=3600", "https://example.com").unwrap();

        let path = temp_dir().join("test_cookies_old.json");
        // Write old format (just CookieStore JSON)
        {
            let inner = manager.inner.lock().unwrap();
            let json = serde_json::to_string_pretty(&inner.store).unwrap();
            fs::write(&path, json).unwrap();
        }

        let loaded = CookieManager::load_persistent_cookies(&path).unwrap();
        let cookies = loaded.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("oldformat=true"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_add_cookie_invalid_url() {
        let manager = CookieManager::default();
        let result = manager.add_cookie("a=1", "not-a-valid-url://");
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_domain() {
        let manager = CookieManager::default();
        manager.add_cookie("a=1; Domain=example.com", "https://example.com").unwrap();
        manager.add_cookie("b=2; Domain=other.com", "https://other.com").unwrap();
        manager.clear_domain("example.com");
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(cookies, "");
        let other_cookies = manager.get_cookies("https://other.com").unwrap();
        assert!(other_cookies.contains("b=2"));
    }

    #[test]
    fn test_clear_domain_session_cookies() {
        let manager = CookieManager::default();
        manager.add_cookie("s1=val1", "https://example.com").unwrap();
        manager.add_cookie("s2=val2", "https://other.com").unwrap();
        manager.clear_domain("example.com");
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(cookies, "");
        let other_cookies = manager.get_cookies("https://other.com").unwrap();
        assert!(other_cookies.contains("s2=val2"));
    }

    #[test]
    fn test_clear_domain_no_false_match() {
        let manager = CookieManager::default();
        // badexample.com 不应被清除（避免 ends_with 误删）
        manager.add_cookie("x=1; Domain=badexample.com", "https://badexample.com").unwrap();
        manager.add_cookie("y=2; Domain=example.com", "https://example.com").unwrap();
        manager.clear_domain("example.com");
        let bad_cookies = manager.get_cookies("https://badexample.com").unwrap();
        assert!(bad_cookies.contains("x=1"), "badexample.com cookies should survive");
        let good_cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(good_cookies, "", "example.com cookies should be cleared");
    }

    #[test]
    fn test_clear_domain_subdomain() {
        let manager = CookieManager::default();
        manager.add_cookie("sub=1; Domain=sub.example.com", "https://sub.example.com").unwrap();
        manager.add_cookie("main=2; Domain=example.com", "https://example.com").unwrap();
        // 清除 example.com 应同时清除 sub.example.com（取决于 Domain 属性语义）
        // sub.example.com 的 Domain 是 sub.example.com，ends_with(".example.com") → true → 应被清除
        manager.clear_domain("example.com");
        let sub_cookies = manager.get_cookies("https://sub.example.com").unwrap();
        assert!(sub_cookies.is_empty(), "sub.example.com should be cleared as subdomain");
        let main_cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(main_cookies, "");
    }

    #[test]
    fn test_clear_domain_host_only() {
        let manager = CookieManager::default();
        // 不带 Domain 属性的 host-only cookie
        manager.add_cookie("hostonly=1", "https://example.com").unwrap();
        manager.add_cookie("other=2", "https://other.com").unwrap();
        manager.clear_domain("example.com");
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert_eq!(cookies, "");
        let other_cookies = manager.get_cookies("https://other.com").unwrap();
        assert!(other_cookies.contains("other=2"));
    }

    #[test]
    fn test_old_format_clear_domain() {
        // 模拟旧格式加载后 clear_domain：raw_cookies 为空，store 有持久化 cookie
        let manager = CookieManager::default();
        manager.add_cookie("keep=1; Max-Age=3600", "https://keep.com").unwrap();
        manager.add_cookie("del=2; Max-Age=3600", "https://example.com").unwrap();

        // 写入旧格式（仅 CookieStore JSON）
        let path = temp_dir().join("test_old_clear.json");
        {
            let inner = manager.inner.lock().unwrap();
            let json = serde_json::to_string_pretty(&inner.store).unwrap();
            fs::write(&path, json).unwrap();
        }

        // 加载旧格式
        let loaded = CookieManager::load_persistent_cookies(&path).unwrap();
        // raw_cookies 应为空
        {
            let inner = loaded.inner.lock().unwrap();
            assert!(inner.raw_cookies.is_empty());
        }

        // 清除 example.com 域名
        loaded.clear_domain("example.com");
        let del_cookies = loaded.get_cookies("https://example.com").unwrap();
        assert_eq!(del_cookies, "");
        let keep_cookies = loaded.get_cookies("https://keep.com").unwrap();
        assert!(keep_cookies.contains("keep=1"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_cookie_clone_and_share() {
        let m1 = CookieManager::default();
        m1.add_cookie("shared=true; Max-Age=3600", "https://example.com").unwrap();
        let m2 = m1.clone();
        let cookies = m2.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("shared=true"));
        m2.clear_all();
        assert_eq!(m1.get_cookies("https://example.com").unwrap(), "");
    }

    #[test]
    fn test_add_cookie_overwrite() {
        let manager = CookieManager::default();
        manager.add_cookie("overwrite=old; Max-Age=3600", "https://example.com").unwrap();
        manager.add_cookie("overwrite=new; Max-Age=3600", "https://example.com").unwrap();

        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("overwrite=new"));
        assert!(!cookies.contains("overwrite=old"));
    }

    #[test]
    fn test_same_name_diff_path_coexist() {
        let manager = CookieManager::default();
        manager.add_cookie("a=1; Path=/api", "https://example.com").unwrap();
        manager.add_cookie("a=2; Path=/other", "https://example.com").unwrap();

        // cookie_store 尊重 Path 匹配: /api 路径不返回 Path=/other 的 cookie
        let api_cookies = manager.get_cookies("https://example.com/api").unwrap();
        assert!(api_cookies.contains("a=1"));
        assert!(!api_cookies.contains("a=2"), "Path=/other should not match /api");

        // 两个不同 Path 的 cookie 都应在 raw_cookies 中
        let inner = manager.inner.lock().unwrap();
        let a_entries: Vec<_> = inner.raw_cookies.iter()
            .filter(|e| e.raw_cookie.starts_with("a="))
            .collect();
        assert_eq!(a_entries.len(), 2, "two cookies with different paths should coexist");
    }

    #[test]
    fn test_domain_helpers() {
        assert_eq!(extract_domain_attr("a=1; Domain=example.com"), Some("example.com".to_string()));
        assert_eq!(extract_domain_attr("a=1; Domain=EXAMPLE.COM"), Some("example.com".to_string()));
        assert_eq!(extract_domain_attr("a=1"), None);
        assert_eq!(extract_domain_attr("a=1; Path=/; Secure"), None);

        assert!(cookie_domain_matches("example.com", "example.com"));
        assert!(cookie_domain_matches("sub.example.com", "example.com"));
        assert!(!cookie_domain_matches("badexample.com", "example.com"));
        assert!(!cookie_domain_matches("notexample.com", "example.com"));
    }

    #[test]
    fn test_host_only_vs_domain_cookie_coexist() {
        let manager = CookieManager::default();
        // host-only cookie on example.com
        manager.add_cookie("sid=1", "https://example.com").unwrap();
        // Domain=example.com cookie (explicit domain, applies to subdomains too)
        manager.add_cookie("sid=2; Domain=example.com", "https://example.com").unwrap();

        // Both must exist in raw_cookies (tagged key distinguishes host-only vs Domain)
        let inner = manager.inner.lock().unwrap();
        let sid_entries: Vec<_> = inner.raw_cookies.iter()
            .filter(|e| e.raw_cookie.starts_with("sid="))
            .collect();
        assert_eq!(sid_entries.len(), 2, "host-only and Domain cookie should coexist");
        drop(inner);

        // Host-only on different host should also coexist
        manager.add_cookie("sid=3", "https://other.com").unwrap();
        let inner2 = manager.inner.lock().unwrap();
        let all_sid: Vec<_> = inner2.raw_cookies.iter()
            .filter(|e| e.raw_cookie.starts_with("sid="))
            .collect();
        assert_eq!(all_sid.len(), 3, "host-only on different host should not collide");
        drop(inner2);

        // clear_domain should not cause cross-contamination
        manager.clear_domain("example.com");
        let cookies = manager.get_cookies("https://example.com").unwrap();
        assert!(cookies.is_empty(), "example.com cookies should be cleared");
        let other_cookies = manager.get_cookies("https://other.com").unwrap();
        assert!(other_cookies.contains("sid=3"), "other.com host-only should survive");
    }

    #[test]
    fn test_session_cookie_not_persisted() {
        let manager = CookieManager::default();
        // session cookie (no Max-Age/Expires)
        manager.add_cookie("session_only=val", "https://example.com").unwrap();
        // persistent cookie
        manager.add_cookie("persistent=val; Max-Age=3600", "https://example.com").unwrap();

        let path = temp_dir().join("test_session_persist.json");
        manager.save_persistent_cookies(&path).unwrap();

        let loaded = CookieManager::load_persistent_cookies(&path).unwrap();
        // persistent cookie must be restored
        let cookies = loaded.get_cookies("https://example.com").unwrap();
        assert!(cookies.contains("persistent=val"), "persistent cookie must be restored");
        // session cookie must NOT be restored
        assert!(!cookies.contains("session_only=val"), "session cookie must not persist");

        fs::remove_file(&path).ok();
    }
}
