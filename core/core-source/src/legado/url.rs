//! Legado URL 模板与选项解析
//!
//! 支持 Legado 的 URL 格式：
//! - 基础 URL：/search?q={{key}}
//! - 带选项 URL：/search?q={{key}}, {"method": "POST", "body": "...", "charset": "gbk"}
//! - GET/POST 请求
//! - charset 指定
//! - webView 标记
//! - 模板变量：{{key}}, {{keyword}}, {{page}}, {{encodeKey}}, {{encode_keyword}}
//! - JS 表达式：{{java.base64Encode(key)}}
//! - 相对 URL 处理和完整 URL 构建
//! - <,{{page}}> 语法：第一页无页码

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::js_runtime::{DefaultJsRuntime, JsRuntime};
use super::value::LegadoValue;

/// Legado 解析后的 URL 结构
#[derive(Debug, Clone)]
pub struct LegadoUrl {
    /// URL 路径部分
    pub path: String,
    /// URL 选项
    pub options: UrlOption,
    /// 是否为相对 URL
    pub is_relative: bool,
}

/// Legado URL 选项
///
/// 对应 Legado 的 UrlOption data class：
/// ```kotlin
/// data class UrlOption(
///     val method: String?,
///     val charset: String?,
///     val webView: Any?,
///     val headers: Any?,
///     val body: Any?,
///     val type: String?,
///     val js: String?,
///     val retry: Int = 0
/// )
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UrlOption {
    /// HTTP 方法（GET/POST）
    #[serde(default)]
    pub method: Option<String>,
    /// 字符集（utf-8/gbk/gb2312/gb18030）
    #[serde(default)]
    pub charset: Option<String>,
    /// 是否使用 WebView 加载
    #[serde(default)]
    pub web_view: bool,
    /// 请求头（JSON 字符串或 Map）
    #[serde(default)]
    pub headers: Option<serde_json::Value>,
    /// POST 请求体
    #[serde(default)]
    pub body: Option<String>,
    /// JS 脚本（加载 URL 前执行）
    #[serde(default)]
    pub js: Option<String>,
    /// 重试次数
    #[serde(default)]
    pub retry: i32,
    /// 内容类型
    #[serde(rename = "type", default)]
    pub content_type: Option<String>,
}

/// 解析 Legado URL 字符串
///
/// 输入格式：`/search?q={{key}}` 或 `/search?q={{key}}, {"method": "POST", "charset": "gbk"}`
///
/// 使用从右到左扫描找到末尾的有效 JSON 选项块，避免 URL 中逗号导致的误分割。
pub fn parse_legado_url(url_str: &str) -> LegadoUrl {
    let trimmed = url_str.trim();
    let (path, options_json) = extract_json_option(trimmed);

    let options = options_json
        .and_then(|j| serde_json::from_str::<UrlOption>(&j).ok())
        .unwrap_or_default();

    let is_relative = !path.starts_with("http://") && !path.starts_with("https://");

    LegadoUrl {
        path: path.to_string(),
        options,
        is_relative,
    }
}

/// 从右向左扫描字符串，找到末尾有效 JSON `{...}` 块作为选项。
///
/// 从最右边的 `{` 开始尝试，找到匹配的 `}` 后尝试 JSON 解析。
/// 如果该块不在字符串末尾（忽略空白），则尝试前一个 `{`。
/// 如果没有任何有效 JSON 块在末尾，整个字符串作为 URL 路径。
fn extract_json_option(s: &str) -> (&str, Option<String>) {
    let bytes = s.as_bytes();
    let mut brace_positions: Vec<usize> = Vec::new();

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            brace_positions.push(i);
        }
    }

    for &start in brace_positions.iter().rev() {
        let mut depth: i32 = 0;
        let mut end: Option<usize> = None;

        for (j, &b) in bytes[start..].iter().enumerate() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + j + 1);
                        break;
                    }
                }
                _ => {}
            }
        }

        if let Some(end_pos) = end {
            let candidate = std::str::from_utf8(&bytes[start..end_pos]).unwrap_or("");
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                let after_json = s[end_pos..].trim();
                if after_json.is_empty() {
                    let prefix = s[..start].trim_end_matches(',').trim();
                    return (prefix, Some(candidate.to_string()));
                }
            }
        }
    }

    (s, None)
}

/// 解析 URL 模板，替换占位符
///
/// 支持的占位符：
/// - `{{key}}` / `{{keyword}}` → 搜索关键词
/// - `{{encodeKey}}` / `{{encode_keyword}}` → URL 编码后的关键词
/// - `{{page}}` → 页码
/// - `{{(page-1)*20}}` → 页面计算表达式
/// - `<,{{page}}>` → 第一页忽略页码
///
/// 返回最终的 URL 路径和 option（如果有 POST body 中的占位符也应在此替换）
pub fn resolve_url_template(
    legado_url: &LegadoUrl,
    keyword: &str,
    page: i32,
    base_url: &str,
) -> String {
    let mut path = legado_url.path.clone();

    // 处理 <,{{page}}> 语法：第一页去除前面的部分
    path = resolve_conditional_page(&path, page);
    path = resolve_template_expressions(&path, keyword, page);

    // 处理 <,xxx> 语法中剩余的占位符
    path = resolve_conditional_placeholder(&path, page);

    // 如果是相对 URL，拼接 base_url
    if legado_url.is_relative {
        build_full_url(base_url, &path)
    } else {
        path
    }
}

/// 构建 POST body，替换其中的占位符
pub fn resolve_post_body(body: &str, keyword: &str, page: i32) -> String {
    resolve_template_expressions(body, keyword, page)
}

fn resolve_template_expressions(input: &str, keyword: &str, page: i32) -> String {
    let re = regex::Regex::new(r"\{\{([\s\S]*?)\}\}").unwrap();
    let vars = build_template_vars(keyword, page);
    let runtime = DefaultJsRuntime::new();
    let mut result = String::with_capacity(input.len());
    let mut last = 0;

    for caps in re.captures_iter(input) {
        let Some(full_match) = caps.get(0) else { continue };
        result.push_str(&input[last..full_match.start()]);
        let expr = caps.get(1).map(|m| m.as_str()).unwrap_or_default().trim();
        let replacement = runtime
            .eval(expr, &vars)
            .map(|value| value.as_string_lossy())
            .unwrap_or_default();
        result.push_str(&replacement);
        last = full_match.end();
    }

    result.push_str(&input[last..]);
    result
}

fn build_template_vars(keyword: &str, page: i32) -> HashMap<String, LegadoValue> {
    let mut vars = HashMap::new();
    vars.insert("key".into(), LegadoValue::String(keyword.to_string()));
    vars.insert("keyword".into(), LegadoValue::String(keyword.to_string()));
    vars.insert("page".into(), LegadoValue::Int(page as i64));
    vars.insert("encodeKey".into(), LegadoValue::String(urlencoding::encode(keyword).to_string()));
    vars.insert("encode_keyword".into(), LegadoValue::String(urlencoding::encode(keyword).to_string()));
    vars
}

/// 处理 `<,{{page}}>` 语法
///
/// 语法 `prefix,{{page}}`:
/// - 如果 page == 1，去除 `prefix,` 部分，返回空字符串
/// - 如果 page > 1，保留 `,{{page}}`，去除 `<` 和 `>`
fn resolve_conditional_page(url: &str, page: i32) -> String {
    if let Some(start) = url.find('<') {
        if let Some(end) = url[start..].find('>') {
            let inner = &url[start + 1..start + end];
            if let Some((prefix, rest)) = inner.split_once(',') {
                let _prefix = prefix.trim();
                let rest = rest.trim();
                if page == 1 {
                    // 第一页：去除 prefix
                    format!("{}{}", &url[..start], &url[start + end + 1..])
                } else {
                    // 非第一页：保留 rest 部分（去掉 < >）
                    let replaced = format!("{}{}{}",
                        &url[..start],
                        rest,
                        &url[start + end + 1..]
                    );
                    replaced
                }
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}

/// 处理 `<,xxx>` 中非页码部分
fn resolve_conditional_placeholder(url: &str, _page: i32) -> String {
    // 清理由 `<,{{page}}>` 处理后的残余占位符
    url.trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

/// 构建完整 URL（处理相对路径）
pub fn build_full_url(base_url: &str, relative_url: &str) -> String {
    if let Ok(base) = url::Url::parse(base_url) {
        if let Ok(full) = base.join(relative_url) {
            return full.to_string();
        }
    }
    // fallback: 直接拼接
    let base = base_url.trim_end_matches('/');
    let relative = relative_url.trim_start_matches('/');
    format!("{}/{}", base, relative)
}

/// 从 URL option 中获取 charset
pub fn get_charset_from_option(option: &UrlOption) -> Option<&str> {
    option.charset.as_deref()
}

/// 从响应头中推测 charset
pub fn guess_charset_from_response(headers: &HashMap<String, String>, body_bytes: &[u8]) -> String {
    // 1. 检查 Content-Type header
    if let Some(content_type) = headers.get("content-type") {
        if let Some(charset) = content_type
            .split(';')
            .find(|s| s.trim().to_lowercase().starts_with("charset"))
        {
            let charset = charset.split('=').nth(1).map(|s| s.trim()).unwrap_or("utf-8");
            return match charset.to_lowercase().as_str() {
                "gbk" | "gb2312" | "gb18030" => charset.to_string(),
                _ => "utf-8".to_string(),
            };
        }
    }

    // 2. 检查 HTML meta charset
    let html = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(1024)]);
    let html_lower = html.to_lowercase();
    if let Some(pos) = html_lower.find("charset=") {
        let rest = &html_lower[pos + 8..];
        let charset = rest
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .next()
            .unwrap_or("utf-8");
        return match charset {
            "gbk" | "gb2312" | "gb18030" => charset.to_string(),
            _ => "utf-8".to_string(),
        };
    }

    "utf-8".to_string()
}

pub(crate) fn decode_response_bytes(bytes: &[u8], charset: &str) -> (String, bool) {
    let encoding = match charset.to_lowercase().as_str() {
        "gbk" | "gb2312" | "gb18030" => encoding_rs::Encoding::for_label(b"gbk").unwrap_or(encoding_rs::UTF_8),
        "big5" => encoding_rs::Encoding::for_label(b"big5").unwrap_or(encoding_rs::UTF_8),
        "shift_jis" | "shift-jis" => encoding_rs::Encoding::for_label(b"shift_jis").unwrap_or(encoding_rs::UTF_8),
        "euc-kr" => encoding_rs::Encoding::for_label(b"euc-kr").unwrap_or(encoding_rs::UTF_8),
        _ => encoding_rs::UTF_8,
    };
    let (decoded, _, had_errors) = encoding.decode(bytes);
    (decoded.into_owned(), had_errors)
}

/// 解析请求头，从 JSON 字符串或 Map 中提取
pub fn parse_headers(headers_value: &Option<serde_json::Value>) -> Vec<(String, String)> {
    let Some(headers) = headers_value else {
        return Vec::new();
    };

    match headers {
        serde_json::Value::Object(map) => map
            .iter()
            .filter_map(|(k, v)| {
                // 跳过 Legado 特有的 proxy 头
                if k == "proxy" {
                    return None;
                }
                v.as_str()
                    .map(|s| (k.clone(), s.to_string()))
                    .or_else(|| Some((k.clone(), v.to_string())))
            })
            .collect(),
        serde_json::Value::String(s) => {
            // 尝试解析为 JSON
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(s) {
                map.into_iter().collect()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Parse proxy string from Legado headers JSON.
///
/// Supports formats:
/// - `socks5://host:port`
/// - `http://host:port`
/// - `socks5://host:port@username@password`
pub fn parse_proxy(headers: &Option<serde_json::Value>) -> Option<String> {
    let headers = headers.as_ref()?;
    match headers {
        serde_json::Value::Object(map) => {
            map.get("proxy")?.as_str().map(|s| s.to_string())
        }
        serde_json::Value::String(s) => {
            let map: HashMap<String, String> = serde_json::from_str(s).ok()?;
            map.get("proxy").cloned()
        }
        _ => None,
    }
}

/// Resolve non-search {{rule}} templates in a string value.
///
/// In Legado, non-search URLs and field values can contain `{{...}}` blocks
/// that mix rule expressions (CSS, XPath, JSONPath, Default, JS) with literal
/// text. Unlike search URL templates where `{{}}` is always JavaScript, this
/// function detects the rule type inside each block and executes accordingly.
///
/// Rule detection inside `{{...}}`:
/// - `@@tag.a@href` → Default selector rule (strips `@@` prefix)
/// - `@css:a@href`   → CSS selector rule
/// - `@json:$.id`    → JSONPath rule
/// - `@xpath://a`    → XPath rule
/// - `@js:expr`      → JavaScript rule
/// - `js:expr`       → JavaScript (explicit)
/// - `//a/@href`     → XPath (pattern)
/// - `$.id`, `$[0]`  → JSONPath (pattern)
/// - anything else   → JavaScript expression (with rule context vars)
///
/// If input does not contain `{{`, returns the original string unchanged.
pub fn resolve_rule_template(
    input: &str,
    html: &str,
    context: &super::context::RuleContext,
) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }

    let re = regex::Regex::new(r"\{\{([\s\S]*?)\}\}").unwrap();
    let mut result = String::with_capacity(input.len());
    let mut last = 0;

    for caps in re.captures_iter(input) {
        let Some(full_match) = caps.get(0) else { continue };
        result.push_str(&input[last..full_match.start()]);

        let inner = caps.get(1).map(|m| m.as_str()).unwrap_or_default().trim();
        let replacement = if inner.is_empty() {
            String::new()
        } else {
            resolve_single_template_rule(inner, html, context)
        };
        result.push_str(&replacement);
        last = full_match.end();
    }

    result.push_str(&input[last..]);
    result
}

/// Execute the inner content of a single `{{...}}` template block.
fn resolve_single_template_rule(
    inner: &str,
    html: &str,
    context: &super::context::RuleContext,
) -> String {
    use super::rule::execute_legado_rule;
    use super::js_runtime::{DefaultJsRuntime, JsRuntime, build_runtime_vars};

    let inner = inner.trim();

    // @@ prefix: Default selector rule — strip both @, execute as CSS selector
    if inner.starts_with("@@") {
        let rule = &inner[2..];
        return execute_legado_rule(rule, html, context)
            .ok()
            .and_then(|v| v.into_iter().next())
            .unwrap_or_default();
    }

    // @-prefixed rules and known patterns go through execute_legado_rule
    if inner.starts_with('@')
        || inner.starts_with("//")
        || inner.starts_with("$.")
        || inner.starts_with("$[")
    {
        return execute_legado_rule(inner, html, context)
            .ok()
            .and_then(|v| v.into_iter().next())
            .unwrap_or_default();
    }

    // Explicit JS
    if inner.starts_with("js:") {
        let script = &inner[3..];
        let vars = build_runtime_vars(context, html);
        let runtime = DefaultJsRuntime::new();
        return runtime
            .eval(script, &vars)
            .map(|v| v.as_string_lossy())
            .unwrap_or_default();
    }

    // Default: JavaScript expression
    let vars = build_runtime_vars(context, html);
    let runtime = DefaultJsRuntime::new();
    runtime
        .eval(inner, &vars)
        .map(|v| v.as_string_lossy())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_url() {
        let url = parse_legado_url("/search?q={{key}}");
        assert_eq!(url.path, "/search?q={{key}}");
        assert!(url.is_relative);
        assert_eq!(url.options.method, None);
    }

    #[test]
    fn test_parse_url_with_options() {
        let url = parse_legado_url(
            "/search?q={{key}}, {\"method\": \"POST\", \"charset\": \"gbk\", \"body\": \"key={{key}}&page={{page}}\"}",
        );
        assert_eq!(url.path, "/search?q={{key}}");
        assert_eq!(url.options.method.as_deref(), Some("POST"));
        assert_eq!(url.options.charset.as_deref(), Some("gbk"));
        assert_eq!(url.options.body.as_deref(), Some("key={{key}}&page={{page}}"));
    }

    #[test]
    fn test_resolve_url_keyword() {
        let url = parse_legado_url("/search?q={{key}}");
        let resolved = resolve_url_template(&url, "我的", 1, "https://example.com");
        assert!(resolved.contains("https://example.com/search?q=%E6%88%91%E7%9A%84"));
    }

    #[test]
    fn test_resolve_url_page() {
        let url = parse_legado_url("/search?q=test&page={{page}}");
        let resolved = resolve_url_template(&url, "test", 3, "https://example.com");
        assert!(resolved.contains("page=3"));
    }

    #[test]
    fn test_conditional_page_first() {
        let url = parse_legado_url("/list-<,{{page}}>.html");
        let resolved = resolve_url_template(&url, "", 1, "https://example.com");
        assert_eq!(resolved, "https://example.com/list-.html");
    }

    #[test]
    fn test_conditional_page_second() {
        let url = parse_legado_url("/list-<,{{page}}>.html");
        let resolved = resolve_url_template(&url, "", 2, "https://example.com");
        assert_eq!(resolved, "https://example.com/list-2.html");
    }

    #[test]
    fn test_page_expression() {
        let url = parse_legado_url("/list?start={{(page-1)*20}}&limit=20");
        let resolved = resolve_url_template(&url, "", 3, "https://example.com");
        assert!(resolved.contains("start=40"));
    }

    #[test]
    fn test_js_base64_expression() {
        let url = parse_legado_url("/search?q={{java.base64Encode(key)}}");
        let resolved = resolve_url_template(&url, "test", 1, "https://example.com");
        assert_eq!(resolved, "https://example.com/search?q=dGVzdA==");
    }

    #[test]
    fn test_js_ternary_expression() {
        let url = parse_legado_url("/list{{page - 1 == 0 ? '' : page}}.html");
        let first = resolve_url_template(&url, "", 1, "https://example.com");
        let second = resolve_url_template(&url, "", 2, "https://example.com");
        assert_eq!(first, "https://example.com/list.html");
        assert_eq!(second, "https://example.com/list2.html");
    }

    #[test]
    fn test_post_body_js_expression() {
        let body = resolve_post_body("key={{java.md5Encode(key)}}&page={{(page-1)*20}}", "123", 2);
        assert_eq!(body, "key=202cb962ac59075b964b07152d234b70&page=20");
    }

    #[test]
    fn test_resolve_rule_template_css() {
        let html = r#"<html><a href="/book/1">Link</a></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let template = "{{@css:a@href}}";
        let result = resolve_rule_template(template, html, &ctx);
        assert_eq!(result, "/book/1");
    }

    #[test]
    fn test_resolve_rule_template_css_text() {
        let html = r#"<html><a href="/book/2">Link</a></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let template = "{{@css:a}}";
        let result = resolve_rule_template(template, html, &ctx);
        assert_eq!(result, "Link");
    }

    #[test]
    fn test_resolve_rule_template_jsonpath() {
        let json = r#"{"id": "book-123", "name": "Test"}"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", json);
        let template = "{{$.id}}";
        let result = resolve_rule_template(template, json, &ctx);
        assert_eq!(result, "book-123");
    }

    #[test]
    fn test_resolve_rule_template_default() {
        let html = r#"<html><a href="/book/3">Link</a></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let template = "{{@@a}}";
        let result = resolve_rule_template(template, html, &ctx);
        assert_eq!(result, "Link");
    }

    #[test]
    fn test_resolve_rule_template_js_fallback() {
        let html = r#"<html></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let template = "{{1 + 2}}";
        let result = resolve_rule_template(template, html, &ctx);
        assert_eq!(result, "3");
    }

    #[test]
    fn test_resolve_rule_template_no_template() {
        let html = r#"<html></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let input = "plain text without template";
        let result = resolve_rule_template(input, html, &ctx);
        assert_eq!(result, input);
    }

    #[test]
    fn test_resolve_rule_template_mixed() {
        let html = r#"<html><a href="/ch/1">Chapter 1</a></html>"#;
        let ctx = crate::legado::context::RuleContext::for_book_info("https://example.com", html);
        let template = "/toc/{{@css:a@href}}?page=1";
        let result = resolve_rule_template(template, html, &ctx);
        assert_eq!(result, "/toc//ch/1?page=1");
    }
}
