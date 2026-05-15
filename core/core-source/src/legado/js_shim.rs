//! Legado JS Shim — Java/Source 对象兼容层
//!
//! 提供 Legado JS 规则中常用的全局函数和对象。
//! 实际 HTTP 调用由 parser.rs 中的 Rust fallback 处理，
//! 此模块提供 JS 层面的兼容对象和方法桩。
//!
//! 支持的 Legado JS API：
//!   java.ajax(url)
//!   java.post(url, body, headers)
//!   java.get(url, headers)
//!   source.getKey()
//!   source.key
//!   baseUrl
//!   result

use super::value::LegadoValue;
use std::collections::HashMap;

/// 检测规则是否为 @js: 或 js: 类型
pub fn is_js_rule(rule: &str) -> bool {
    let trimmed = rule.trim();
    trimmed.starts_with("@js:") || trimmed.starts_with("js:") || trimmed.starts_with("@js\n")
}

/// 检测 JS 脚本中是否包含 HTTP 调用（需要 Rust fallback 处理）
pub fn js_requires_http(script: &str) -> bool {
    script.contains("java.ajax")
        || script.contains("java.post")
        || script.contains("java.get")
        || script.contains("java.connect")
}

/// 检测 JS 脚本是否包含 POST 到 /novel/clist/ 的模式 (axdzs.json)
pub fn js_uses_clist_api(script: &str) -> bool {
    script.contains("/novel/clist/") && script.contains("java.post")
}

/// 检测 JS 脚本是否包含 challenge + ajax 模式 (axdzs.json)
pub fn js_uses_challenge(script: &str) -> bool {
    script.contains("challenge") && script.contains("java.ajax")
}

/// 构建 JS 执行所需的变量 Map
///
/// 将上下文中的标准变量转换为 LegadoValue Map，
/// 供 BoaJsRuntime::eval() 使用。
pub fn build_js_vars(
    base_url: &str,
    source_url: &str,
    src: &str,
    result: &str,
    title: &str,
    key: &str,
    page: i32,
) -> HashMap<String, LegadoValue> {
    let mut vars = HashMap::new();

    vars.insert(
        "baseUrl".to_string(),
        LegadoValue::String(base_url.to_string()),
    );
    vars.insert(
        "__source_url__".to_string(),
        LegadoValue::String(source_url.to_string()),
    );
    vars.insert("src".to_string(), LegadoValue::String(src.to_string()));
    vars.insert(
        "result".to_string(),
        LegadoValue::String(result.to_string()),
    );
    vars.insert("title".to_string(), LegadoValue::String(title.to_string()));
    vars.insert("key".to_string(), LegadoValue::String(key.to_string()));
    vars.insert("page".to_string(), LegadoValue::Int(page as i64));

    vars
}

/// 简单的 JS 变量 Map（最小集）
pub fn build_minimal_js_vars(base_url: &str, source_url: &str) -> HashMap<String, LegadoValue> {
    let mut vars = HashMap::new();
    vars.insert(
        "baseUrl".to_string(),
        LegadoValue::String(base_url.to_string()),
    );
    vars.insert(
        "__source_url__".to_string(),
        LegadoValue::String(source_url.to_string()),
    );
    vars
}
