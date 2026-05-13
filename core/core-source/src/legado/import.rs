//! Legado 书源导入与字段规范化
//!
//! 负责将真实 Legado JSON 格式转换为内部存储格式。
//! 对应 Legado 导出格式中的字段名转换和规则规范化。
//!
//! Legado 原始字段 → 内部存储字段映射：
//!   bookSourceUrl          → url
//!   bookSourceName         → name
//!   bookSourceGroup        → group_name
//!   bookSourceType         → source_type
//!   bookSourceComment      → (丢弃)
//!   bookUrlPattern         → (暂存为 raw JSON)
//!   customOrder            → custom_order
//!   enabled                → enabled
//!   enabledCookieJar       → (暂存为 raw JSON)
//!   enabledExplore         → (暂存为 raw JSON)
//!   exploreUrl             → explore_url
//!   header                 → header (JSON string)
//!   lastUpdateTime         → (丢弃)
//!   loginUrl               → login_url
//!   jsLib                  → js_lib
//!   respondTime            → (丢弃)
//!   searchUrl              → rule_search.search_url
//!   weight                 → weight
//!
//! 规则子字段规范化（camelCase → snake_case）：
//!   bookList               → book_list
//!   bookUrl                → book_url
//!   coverUrl               → cover_url
//!   lastChapter            → last_chapter
//!   wordCount              → word_count
//!   chapterList            → chapter_list
//!   chapterName            → chapter_name
//!   chapterUrl             → chapter_url
//!   nextContentUrl         → next_content_url
//!   nextTocUrl             → next_toc_url
//!   isVip                  → is_vip
//!   updateTime             → update_time
//!   canReName              → can_rename
//!   tocUrl                 → toc_url
//!   bookInfoInit           → book_info_init
//!   sourceRegex            → source_regex
//!   webJs                  → web_js
//!   searchUrl              → search_url
//!
//! 选择器语法规范化（JSOUP Default → CSS）：
//!   class.xxx              → .xxx
//!   id.xxx                 → #xxx
//!   tag.xxx                → xxx
//!   @tag.xxx               → xxx
//!   children               → * (保留特殊处理)

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use chrono::Utc;

/// Legado 书源 JSON 导出格式
#[derive(Debug, Clone, Deserialize)]
pub struct LegadoBookSource {
    // ── 基本字段 ──
    #[serde(rename = "bookSourceUrl")]
    pub url: String,

    #[serde(rename = "bookSourceName")]
    pub name: String,

    #[serde(rename = "bookSourceGroup", default, deserialize_with = "string_from_any")]
    pub group_name: String,

    #[serde(rename = "bookSourceType", default)]
    pub source_type: i32,

    #[serde(rename = "bookSourceComment", default, deserialize_with = "string_from_any")]
    pub comment: String,

    #[serde(rename = "bookUrlPattern", default, deserialize_with = "string_from_any")]
    pub book_url_pattern: String,

    // ── 启停与权重 ──
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(rename = "enabledCookieJar", default)]
    pub enabled_cookie_jar: bool,

    #[serde(rename = "enabledExplore", default)]
    pub enabled_explore: bool,

    #[serde(default)]
    pub weight: i32,

    #[serde(rename = "customOrder", default)]
    pub custom_order: i32,

    // ── 请求配置 ──
    #[serde(default, deserialize_with = "option_string_from_any")]
    pub header: Option<String>,

    #[serde(rename = "loginUrl", default, deserialize_with = "option_string_from_any")]
    pub login_url: Option<String>,

    #[serde(rename = "jsLib", default, deserialize_with = "option_string_from_any")]
    pub js_lib: Option<String>,

    // ── 搜索与发现 ──
    #[serde(rename = "searchUrl", default, deserialize_with = "option_string_from_any")]
    pub search_url: Option<String>,

    #[serde(rename = "exploreUrl", default, deserialize_with = "option_string_from_any")]
    pub explore_url: Option<String>,

    // ── 规则对象 ──
    #[serde(rename = "ruleSearch", default)]
    pub rule_search: Option<JsonValue>,

    #[serde(rename = "ruleExplore", default)]
    pub rule_explore: Option<JsonValue>,

    #[serde(rename = "ruleBookInfo", default)]
    pub rule_book_info: Option<JsonValue>,

    #[serde(rename = "ruleToc", default)]
    pub rule_toc: Option<JsonValue>,

    #[serde(rename = "ruleContent", default)]
    pub rule_content: Option<JsonValue>,

    // ── 元数据（可忽略） ──
    #[serde(rename = "lastUpdateTime", default)]
    pub last_update_time: JsonValue,

    #[serde(rename = "respondTime", default)]
    pub respond_time: JsonValue,
}

fn default_true() -> bool {
    true
}

fn string_from_any<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<JsonValue>::deserialize(deserializer)?;
    Ok(value_to_string(value).unwrap_or_default())
}

fn option_string_from_any<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<JsonValue>::deserialize(deserializer)?;
    Ok(value_to_string(value).filter(|s| !s.is_empty()))
}

fn value_to_string(value: Option<JsonValue>) -> Option<String> {
    match value? {
        JsonValue::Null => None,
        JsonValue::String(s) => Some(s),
        other => Some(other.to_string()),
    }
}

/// 导入 Legado 书源，返回内部 BookSource 的字段集合。
/// 注意：这里返回的是一个中间表示，调用者负责生成 ID 并插入数据库。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub source_type: i32,
    pub group_name: Option<String>,
    pub enabled: bool,
    pub custom_order: i32,
    pub weight: i32,
    pub rule_search: Option<String>,
    pub rule_book_info: Option<String>,
    pub rule_toc: Option<String>,
    pub rule_content: Option<String>,
    pub rule_explore: Option<String>,
    pub login_url: Option<String>,
    pub header: Option<String>,
    pub js_lib: Option<String>,
    pub search_url: Option<String>,
    pub explore_url: Option<String>,
    pub enabled_cookie_jar: bool,
    pub enabled_explore: bool,
    pub book_url_pattern: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 将 Legado 书源 JSON 字符串转换为 ImportedSource 列表
pub fn import_legado_source(json: &str) -> Result<Vec<ImportedSource>, String> {
    let legado_sources: Vec<LegadoBookSource> =
        serde_json::from_str(json).map_err(|e| format!("Legado JSON 解析失败: {}", e))?;

    legado_sources
        .iter()
        .map(legado_to_imported)
        .collect::<Result<Vec<_>, _>>()
}

fn legado_to_imported(source: &LegadoBookSource) -> Result<ImportedSource, String> {
    let now = Utc::now().timestamp();
    let rule_search = merge_search_url(source.rule_search.clone(), source.search_url.as_deref());

    Ok(ImportedSource {
        id: Uuid::new_v4().to_string(),
        name: source.name.clone(),
        url: source.url.clone(),
        source_type: source.source_type,
        group_name: if source.group_name.is_empty() {
            None
        } else {
            Some(source.group_name.clone())
        },
        enabled: source.enabled,
        custom_order: source.custom_order,
        weight: source.weight,
        rule_search: rule_search.map(|v| normalize_rule_values(v).to_string()),
        rule_book_info: source
            .rule_book_info
            .clone()
            .map(|v| normalize_rule_values(v).to_string()),
        rule_toc: source
            .rule_toc
            .clone()
            .map(|v| normalize_rule_values(v).to_string()),
        rule_content: source
            .rule_content
            .clone()
            .map(|v| normalize_rule_values(v).to_string()),
        rule_explore: source
            .rule_explore
            .clone()
            .map(|v| normalize_rule_values(v).to_string()),
        login_url: source.login_url.clone(),
        header: source.header.clone(),
        js_lib: source.js_lib.clone(),
        search_url: source.search_url.clone(),
        explore_url: source.explore_url.clone(),
        enabled_cookie_jar: source.enabled_cookie_jar,
        enabled_explore: source.enabled_explore,
        book_url_pattern: if source.book_url_pattern.is_empty() {
            None
        } else {
            Some(source.book_url_pattern.clone())
        },
        created_at: now,
        updated_at: now,
    })
}

/// 合并顶层 searchUrl 到 rule_search JSON 对象。
/// 如果 rule_search 中已有 search_url 字段，则不覆盖。
fn merge_search_url(
    rule_search: Option<JsonValue>,
    search_url: Option<&str>,
) -> Option<JsonValue> {
    let mut value = rule_search.unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()));
    if let (Some(url), Some(obj)) = (search_url, value.as_object_mut()) {
        if !obj.contains_key("search_url") {
            obj.insert(
                "search_url".to_string(),
                JsonValue::String(clean_legado_url(url)),
            );
        }
    }
    Some(value)
}

/// 规范化规则对象：
/// 1. 字段名 camelCase → snake_case
/// 2. 规则值 Legado Default 选择器 → CSS 兼容格式
fn normalize_rule_values(mut value: JsonValue) -> JsonValue {
    let Some(obj) = value.as_object_mut() else {
        return value;
    };

    // 第一步：字段名规范化
    normalize_rule_keys(obj);

    // 第二步：规则值规范化（仅对字符串值）
    for val in obj.values_mut() {
        if let Some(rule_str) = val.as_str() {
            let normalized = normalize_legado_rule(rule_str);
            *val = JsonValue::String(normalized);
        }
    }

    value
}

/// 规范化规则 JSON 的字段名（camelCase → snake_case）
fn normalize_rule_keys(obj: &mut serde_json::Map<String, JsonValue>) {
    let key_mappings: &[(&str, &str)] = &[
        ("bookList", "book_list"),
        ("bookUrl", "book_url"),
        ("coverUrl", "cover_url"),
        ("lastChapter", "last_chapter"),
        ("wordCount", "word_count"),
        ("chapterList", "chapter_list"),
        ("chapterName", "chapter_name"),
        ("chapterUrl", "chapter_url"),
        ("nextContentUrl", "next_content_url"),
        ("nextTocUrl", "next_toc_url"),
        ("isVip", "is_vip"),
        ("updateTime", "update_time"),
        ("canReName", "can_rename"),
        ("tocUrl", "toc_url"),
        ("bookInfoInit", "book_info_init"),
        ("sourceRegex", "source_regex"),
        ("replaceRegex", "replace_regex"),
        ("webJs", "web_js"),
        ("downloadUrls", "download_urls"),
        ("searchUrl", "search_url"),
        ("checkKeyWord", "check_keyword"),
    ];

    for (from, to) in key_mappings {
        if !obj.contains_key(*to) {
            if let Some(v) = obj.remove(*from) {
                obj.insert(to.to_string(), v);
            }
        }
    }
}

/// 规范化单条 Legado 规则字符串
///
/// 支持的转换：
/// - `class.xxx`     → `.xxx`
/// - `id.xxx`        → `#xxx`
/// - `tag.xxx`       → `xxx`
/// - `@tag.xxx`      → `xxx`
/// - 保留 `@js:` / `@css:` / `@XPath:` / `@json:` 前缀
/// - 保留 `@text` / `@html` / `@href` / `@src` / `@content` 等后缀
/// - 保留 `!` 跳过和 `.` 索引
/// - 保留 `##` 正则替换段
/// - 保留 `||` / `&&` / `%%` 组合符及其后续规则
/// - 处理多行规则（以换行符后跟着 `@js:` 开头的第二段规则）
pub fn normalize_legado_rule(rule: &str) -> String {
    let trimmed = rule.trim();

    // 保留 JS 规则原样
    if trimmed.starts_with("@js:") || trimmed.starts_with("js:") {
        return rule.to_string();
    }

    // 处理多行规则：分割换行符后面的独立规则块
    if let Some(nl_pos) = trimmed.find('\n') {
        let first_line = &trimmed[..nl_pos].trim();
        let rest = &trimmed[nl_pos + 1..].trim();
        if !rest.is_empty() && (rest.starts_with("@js:") || rest.starts_with("js:")) {
            let normalized_first = normalize_single_rule(first_line);
            return format!("{}\n{}", normalized_first, rest);
        }
    }

    normalize_single_rule(trimmed)
}

/// 对单行规则字符串做规范化
fn normalize_single_rule(rule: &str) -> String {
    // 先分离后缀（.0, @text 等）和 ## 替换段
    // 对规则主体做规范化
    let (expr, suffix) = split_rule_with_suffix_and_replace(rule);

    // 对组合规则 `||` / `%%` 分段处理
    let parts = split_rule_alternatives(expr);
    let normalized_parts: Vec<String> = parts
        .iter()
        .map(|p| normalize_selector_segment(p.trim()))
        .collect();

    let mut result = normalized_parts.join("||");

    if !suffix.is_empty() {
        result.push_str(suffix);
    }

    result
}

/// 分割规则中的 `||` 组合符（保护 `&&` 和 `%%`）
fn split_rule_alternatives(expr: &str) -> Vec<String> {
    // 简化版：按 || 分割（对于简单规则足够）
    // 更完善的版本需要检测引号和括号
    if expr.contains("||") {
        expr.split("||").map(|s| s.to_string()).collect()
    } else if expr.contains("%%") {
        expr.split("%%").map(|s| s.to_string()).collect()
    } else {
        vec![expr.to_string()]
    }
}

/// 分离规则中的后缀（@text 等）和 ## 替换段
/// 返回 (选择器主体, 后缀+替换段)
fn split_rule_with_suffix_and_replace(rule: &str) -> (&str, &str) {
    let suffixes = [
        "@textNodes",
        "@textNode",
        "@ownText",
        "@content",
        "@html",
        "@text",
        "@href",
        "@src",
        "@all",
    ];

    // 先找 ## 的位置（## 替换段应该在最外层，在选择器提取之后）
    if let Some(hash_pos) = rule.find("##") {
        let (before_hash, hash_part) = rule.split_at(hash_pos);
        // 对 before_hash 部分找后缀
        for suffix in &suffixes {
            if let Some(expr) = before_hash.strip_suffix(suffix) {
                let remaining = format!("{}{}", suffix, hash_part);
                return (expr, &rule[rule.len() - remaining.len()..]);
            }
        }
        return (before_hash, hash_part);
    }

    // 没有 ## 替换段，只找后缀
    for suffix in &suffixes {
        if let Some(expr) = rule.strip_suffix(suffix) {
            return (expr, &rule[expr.len()..]);
        }
    }

    (rule, "")
}

/// 规范化选择器片段：
///   class.xxx → .xxx
///   id.xxx → #xxx
///   @tag.xxx → xxx
///   tag.xxx → xxx (但保留 @ 链)
fn normalize_selector_segment(segment: &str) -> String {
    let segment = segment.trim();
    if segment.is_empty() || segment.starts_with('@') || segment.starts_with("//") {
        return segment.to_string();
    }

    // 对 @ 链的每一段进行处理
    // 规则格式：selector@selector@selector...@extract
    // 例如：class.odd.0@tag.a.0@text
    let parts: Vec<&str> = segment.split('@').collect();
    let mut normalized_parts = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // 最后一段可能是提取类型，也可能是 HTMl属性名
            // 如果匹配已知提取类型，保留
            match *part {
                "text" | "textNodes" | "textNode" | "ownText" | "html" | "all" | "href"
                | "src" | "content" => {
                    // 把前面的规范化部分和提取后缀拼接
                    let selector = normalized_parts.join("@");
                    return format!("{}@{}", selector, part);
                }
                _ => {
                    // 可能是属性名（如 title, data-id）或 CSS 选择器中的 @tag
                    let normalized = normalize_simple_selector(part);
                    normalized_parts.push(normalized);
                }
            }
        } else {
            let normalized = normalize_simple_selector(part);
            normalized_parts.push(normalized);
        }
    }

    normalized_parts.join("@")
}

/// 规范化简单选择器（不包含 @ 链）
fn normalize_simple_selector(sel: &str) -> String {
    let sel = sel.trim();
    if sel.is_empty() {
        return sel.to_string();
    }

    // class.xxx → .xxx
    if let Some(class_name) = sel.strip_prefix("class.") {
        return format!(".{}", class_name);
    }
    // id.xxx → #xxx
    if let Some(id_name) = sel.strip_prefix("id.") {
        return format!("#{}", id_name);
    }
    // tag.xxx → xxx
    if let Some(tag_name) = sel.strip_prefix("tag.") {
        return tag_name.to_string();
    }
    // text.xxx → 保留（这是文本内容选择器）
    if sel.starts_with("text.") {
        return sel.to_string();
    }

    sel.to_string()
}

/// 清理 Legado URL 中的额外参数。
/// 例如：`/search.php?q={{key}}, {"webView": true}`
/// 清理后为 `/search.php?q={{key}}`
fn clean_legado_url(url: &str) -> String {
    let trimmed = url.trim();

    // 检查是否有 URL 选项（逗号 + 空格 + { 或纯逗号 + {）
    if let Some((path, options)) = trimmed.rsplit_once(',') {
        let options_trimmed = options.trim_start();
        if options_trimmed.starts_with('{') && !options_trimmed.starts_with("{{") {
            return path.trim().to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXDZS: &str = include_str!("../../../../sy/axdzs.json");
    const SY_AXDZS: &str = AXDZS;
    const SY_SDG: &str = include_str!("../../../../sy/sdg.json");
    const SY_22BIQU: &str = include_str!("../../../../sy/22biqu - grok.json");
    const SY_COLLECTION: &str = include_str!("../../../../sy/1778070297.json");

    #[test]
    fn test_import_real_axdzs_source() {
        let imported = import_legado_source(AXDZS).unwrap();
        assert_eq!(imported.len(), 1);

        let source = &imported[0];
        assert_eq!(source.name, "爱下电子书");
        assert_eq!(source.url, "https://ixdzs8.com");
        assert_eq!(source.search_url.as_deref(), Some("/bsearch?q={{key}}&page={{page}}"));
        assert!(source.enabled_cookie_jar);
        assert!(source.enabled_explore);
    }

    #[test]
    fn test_axdzs_rule_search_normalization() {
        let imported = import_legado_source(AXDZS).unwrap();
        let rule_search: serde_json::Value = serde_json::from_str(
            imported[0].rule_search.as_deref().unwrap(),
        )
        .unwrap();

        assert_eq!(rule_search["book_list"], ".u-list@li");
        assert_eq!(rule_search["author"], ".bauthor@a@text");
        assert_eq!(rule_search["book_url"], "a@href");
        assert_eq!(rule_search["name"], "a@title");
        assert_eq!(rule_search["search_url"], "/bsearch?q={{key}}&page={{page}}");

        let cover = rule_search["cover_url"].as_str().unwrap();
        assert!(cover.starts_with("img@src\n@js:"), "cover rule should preserve multiline @js, got {cover}");
    }

    #[test]
    fn test_axdzs_js_rules_preserved() {
        let imported = import_legado_source(AXDZS).unwrap();
        let rule_content: serde_json::Value = serde_json::from_str(
            imported[0].rule_content.as_deref().unwrap(),
        )
        .unwrap();
        let rule_toc: serde_json::Value = serde_json::from_str(
            imported[0].rule_toc.as_deref().unwrap(),
        )
        .unwrap();

        assert!(rule_content["content"].as_str().unwrap().starts_with("@js:"));
        assert!(rule_content["content"].as_str().unwrap().contains("java.ajax"));
        assert!(rule_toc["chapter_list"].as_str().unwrap().starts_with("@js:"));
        assert!(rule_toc["chapter_list"].as_str().unwrap().contains("java.post"));
    }

    #[test]
    fn test_clean_legado_url_does_not_strip_conditional_page_template() {
        assert_eq!(
            clean_legado_url("/list-<,{{page}}>.html"),
            "/list-<,{{page}}>.html",
        );
    }

    #[test]
    fn test_import_all_sy_real_sources() {
        for (name, json) in [
            ("sy/axdzs.json", SY_AXDZS),
            ("sy/sdg.json", SY_SDG),
            ("sy/22biqu - grok.json", SY_22BIQU),
            ("sy/1778070297.json", SY_COLLECTION),
        ] {
            let imported = import_legado_source(json).unwrap_or_else(|err| panic!("{name} import failed: {err}"));
            assert!(!imported.is_empty(), "{name} should import at least one source");
            assert!(imported.iter().all(|s| !s.name.is_empty()), "{name} contains empty source name");
            assert!(imported.iter().all(|s| !s.url.is_empty()), "{name} contains empty source url");
        }
    }

    #[test]
    fn test_import_sy_collection_contains_multiple_sources() {
        let imported = import_legado_source(SY_COLLECTION).unwrap();
        assert_eq!(imported.len(), 100, "collection should contain 100 sources");
        assert!(imported.iter().any(|s| s.name.contains("第一版主")));
    }

    #[test]
    fn test_sdg_rules_keep_inline_js_and_default_selectors() {
        let imported = import_legado_source(SY_SDG).unwrap();
        let book_info: serde_json::Value = serde_json::from_str(
            imported[0].rule_book_info.as_deref().unwrap(),
        )
        .unwrap();
        assert_eq!(book_info["author"], ".itemtxt@p.1@a@text");
        assert!(book_info["download_urls"].as_str().unwrap().contains("<js>"));
        assert!(book_info["download_urls"].as_str().unwrap().contains("java.ajax"));
    }

    #[test]
    fn test_22biqu_rules_keep_css_suffix_and_replace_regex() {
        let imported = import_legado_source(SY_22BIQU).unwrap();
        let book_info: serde_json::Value = serde_json::from_str(
            imported[0].rule_book_info.as_deref().unwrap(),
        )
        .unwrap();
        let content: serde_json::Value = serde_json::from_str(
            imported[0].rule_content.as_deref().unwrap(),
        )
        .unwrap();
        assert_eq!(book_info["author"], "meta[property$=author]@content");
        assert_eq!(content["content"], "#chaptercontent@html");
        assert!(content.get("replaceRegex").is_some() || content.get("replace_regex").is_some());
    }

    #[test]
    fn test_import_preserves_book_info_fields() {
        let json = r#"[{
            "bookSourceUrl": "https://example.com",
            "bookSourceName": "Test",
            "ruleBookInfo": {
                "name": "tag.h1@text",
                "bookInfoInit": "@js:return {a:'x'}",
                "tocUrl": "a.read@href",
                "canReName": "1"
            }
        }]"#;
        let imported = import_legado_source(json).unwrap();
        let book_info: serde_json::Value =
            serde_json::from_str(imported[0].rule_book_info.as_deref().unwrap()).unwrap();
        assert_eq!(book_info["book_info_init"], "@js:return {a:'x'}");
        assert_eq!(book_info["toc_url"], "a.read@href");
        assert_eq!(book_info["can_rename"], "1");
    }

    #[test]
    fn test_import_preserves_toc_fields() {
        let json = r#"[{
            "bookSourceUrl": "https://example.com",
            "bookSourceName": "Test",
            "ruleToc": {
                "chapterList": "ul.chapters@li",
                "nextTocUrl": "a.next@href",
                "isVip": "span.vip@text",
                "updateTime": "span.time@text"
            }
        }]"#;
        let imported = import_legado_source(json).unwrap();
        let toc: serde_json::Value =
            serde_json::from_str(imported[0].rule_toc.as_deref().unwrap()).unwrap();
        assert_eq!(toc["next_toc_url"], "a.next@href");
        assert_eq!(toc["is_vip"], "span.vip@text");
        assert_eq!(toc["update_time"], "span.time@text");
    }

    #[test]
    fn test_import_preserves_content_fields() {
        let json = r#"[{
            "bookSourceUrl": "https://example.com",
            "bookSourceName": "Test",
            "ruleContent": {
                "content": "div.text@html",
                "webJs": "getDecode()",
                "sourceRegex": ".*\\.mp4"
            }
        }]"#;
        let imported = import_legado_source(json).unwrap();
        let content: serde_json::Value =
            serde_json::from_str(imported[0].rule_content.as_deref().unwrap()).unwrap();
        assert_eq!(content["web_js"], "getDecode()");
        assert_eq!(content["source_regex"], ".*\\.mp4");
    }

    #[test]
    fn test_import_preserves_search_intro_and_word_count() {
        let json = r#"[{
            "bookSourceUrl": "https://example.com",
            "bookSourceName": "Test",
            "searchUrl": "/search?q={{key}}",
            "ruleSearch": {
                "bookList": ".item",
                "intro": "p.desc@text",
                "wordCount": "span.wc@text"
            }
        }]"#;
        let imported = import_legado_source(json).unwrap();
        let search: serde_json::Value =
            serde_json::from_str(imported[0].rule_search.as_deref().unwrap()).unwrap();
        assert_eq!(search["intro"], "p.desc@text");
        assert_eq!(search["word_count"], "span.wc@text");
    }
}
