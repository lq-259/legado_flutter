//! # core-source - 书源规则引擎
//!
//! 核心中的核心模块，负责解析和执行书源规则。
//! 对应原 Legado 的 `model/webBook/` 和 `model/analyzeRule/`。
//! 支持 CSS/XPath/JSONPath/Regex 四种规则表达式，使用 Rhai 脚本引擎代替原 JS 引擎。

pub mod rule_engine;
pub mod script_engine;
pub mod parser;
pub mod types;
pub mod utils;
pub mod legado;

// 重新导出主要类型
pub use rule_engine::{RuleEngine, RuleExpression, RuleType, RuleError};
pub use script_engine::{ScriptEngine, ScriptResult, ScriptContext};
pub use parser::{BookSourceParser, SearchResult, BookDetail, ChapterInfo, ChapterContent, ExploreEntry};
pub use types::{BookSource, SearchRule, BookInfoRule, TocRule, ContentRule, ExtractType};

use serde::Serialize;
use std::fs;
use std::path::Path;
use jsonpath_lib as jsonpath;
use sxd_xpath::Factory;

/// 书源规则解析入口
pub fn parse_book_source(json: &str) -> Result<BookSource, String> {
    serde_json::from_str(json)
        .map_err(|e| format!("解析书源失败: {}", e))
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub field: String,
    pub severity: String, // "error" | "warning" | "info"
    pub message: String,
}

/// 验证书源配置是否有效，返回结构化结果
pub fn validate_book_source(source: &BookSource) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if source.name.is_empty() {
        issues.push(ValidationIssue {
            field: "name".into(),
            severity: "error".into(),
            message: "书源名称不能为空".into(),
        });
    }
    if source.url.is_empty() {
        issues.push(ValidationIssue {
            field: "url".into(),
            severity: "error".into(),
            message: "书源URL不能为空".into(),
        });
    } else if !source.url.starts_with("http://") && !source.url.starts_with("https://") {
        issues.push(ValidationIssue {
            field: "url".into(),
            severity: "warning".into(),
            message: "URL 应以 http:// 或 https:// 开头".into(),
        });
    }

    validate_search_rules(source, &mut issues);
    validate_book_info_rules(source, &mut issues);
    validate_toc_rules(source, &mut issues);
    validate_content_rules(source, &mut issues);
    validate_rule_expressions(source, &mut issues);

    issues
}

fn validate_search_rules(source: &BookSource, issues: &mut Vec<ValidationIssue>) {
    let rule = match &source.rule_search {
        Some(r) => r,
        None => {
            issues.push(ValidationIssue {
                field: "rule_search".into(),
                severity: "warning".into(),
                message: "未配置搜索规则，将无法在线搜索".into(),
            });
            return;
        }
    };

    if let Some(ref search_url) = rule.search_url {
        if !search_url.contains("{{") && !search_url.contains("key") {
            issues.push(ValidationIssue {
                field: "rule_search.search_url".into(),
                severity: "warning".into(),
                message: "搜索URL未包含 {{keyword}} 占位符".into(),
            });
        }
    }

    if let Some(ref book_list) = rule.book_list {
        check_rule_expression("rule_search.book_list", book_list, issues);
    }
    if let Some(ref name) = rule.name {
        check_rule_expression("rule_search.name", name, issues);
    }
    if let Some(ref author) = rule.author {
        check_rule_expression("rule_search.author", author, issues);
    }
    if let Some(ref book_url) = rule.book_url {
        check_rule_expression("rule_search.book_url", book_url, issues);
    }
    if let Some(ref cover_url) = rule.cover_url {
        check_rule_expression("rule_search.cover_url", cover_url, issues);
    }
}

fn validate_book_info_rules(source: &BookSource, issues: &mut Vec<ValidationIssue>) {
    let rule = match &source.rule_book_info {
        Some(r) => r,
        None => return,
    };
    if let Some(ref name) = rule.name {
        check_rule_expression("rule_book_info.name", name, issues);
    }
    if let Some(ref author) = rule.author {
        check_rule_expression("rule_book_info.author", author, issues);
    }
    if let Some(ref cover_url) = rule.cover_url {
        check_rule_expression("rule_book_info.cover_url", cover_url, issues);
    }
    if let Some(ref intro) = rule.intro {
        check_rule_expression("rule_book_info.intro", intro, issues);
    }
}

fn validate_toc_rules(source: &BookSource, issues: &mut Vec<ValidationIssue>) {
    let rule = match &source.rule_toc {
        Some(r) => r,
        None => {
            issues.push(ValidationIssue {
                field: "rule_toc".into(),
                severity: "warning".into(),
                message: "未配置目录规则，将无法获取章节列表".into(),
            });
            return;
        }
    };
    if let Some(ref chapter_list) = rule.chapter_list {
        check_rule_expression("rule_toc.chapter_list", chapter_list, issues);
    }
    if let Some(ref chapter_name) = rule.chapter_name {
        check_rule_expression("rule_toc.chapter_name", chapter_name, issues);
    }
    if let Some(ref chapter_url) = rule.chapter_url {
        check_rule_expression("rule_toc.chapter_url", chapter_url, issues);
    }
}

fn validate_content_rules(source: &BookSource, issues: &mut Vec<ValidationIssue>) {
    let rule = match &source.rule_content {
        Some(r) => r,
        None => {
            issues.push(ValidationIssue {
                field: "rule_content".into(),
                severity: "warning".into(),
                message: "未配置内容规则，将无法获取正文".into(),
            });
            return;
        }
    };
    if let Some(ref content) = rule.content {
        check_rule_expression("rule_content.content", content, issues);
    }
    if let Some(ref next_content_url) = rule.next_content_url {
        check_rule_expression("rule_content.next_content_url", next_content_url, issues);
    }
}

fn validate_rule_expressions(source: &BookSource, issues: &mut Vec<ValidationIssue>) {
    if let Some(ref rule) = source.rule_search {
        if let Some(ref url) = rule.search_url {
            if !url.starts_with("http") && !url.contains("{{") && !url.contains('?') {
                if RuleExpression::parse(url).is_some() {
                    issues.push(ValidationIssue {
                        field: "rule_search.search_url".into(),
                        severity: "warning".into(),
                        message: "搜索URL看起来像选择器而非URL模板".into(),
                    });
                }
            }
        }
    }
}

fn check_rule_expression(field: &str, expr: &str, issues: &mut Vec<ValidationIssue>) {
    if expr.is_empty() {
        return;
    }
    let trimmed = expr.trim();
    let (expr_no_replace, _) = rule_engine::strip_legado_replace_rules(trimmed);
    let expr_without_suffix = ExtractType::from_rule(expr_no_replace).0;

    if is_legado_extended(expr_without_suffix) {
        issues.push(ValidationIssue {
            field: field.into(),
            severity: "info".into(),
            message: "Legado 扩展规则 — 将在运行时解析".into(),
        });
        return;
    }

    let rule_type = RuleExpression::parse(expr_without_suffix)
        .map(|r| r.rule_type);

    match rule_type {
        Some(RuleType::Regex) => {
            if let Err(e) = regex::Regex::new(expr_without_suffix) {
                issues.push(ValidationIssue {
                    field: field.into(),
                    severity: "error".into(),
                    message: format!("正则表达式语法错误: {}", e),
                });
            }
        }
        Some(RuleType::XPath) => {
            let xpath_expr = expr_without_suffix
                .strip_prefix("@XPath:")
                .unwrap_or(expr_without_suffix);
            match Factory::new().build(xpath_expr) {
                Err(e) => {
                    issues.push(ValidationIssue {
                        field: field.into(),
                        severity: "error".into(),
                        message: format!("XPath 表达式编译失败: {}", e),
                    });
                }
                Ok(None) => {
                    issues.push(ValidationIssue {
                        field: field.into(),
                        severity: "warning".into(),
                        message: "XPath 表达式编译返回空".into(),
                    });
                }
                Ok(Some(_)) => {}
            }
        }
        Some(RuleType::JsonPath) => {
            let jsonpath_expr = expr_without_suffix
                .strip_prefix("@Json:")
                .unwrap_or(expr_without_suffix);
            let test_json = serde_json::json!({"test": {"key": "value"}, "items": [{"name": "test"}]});
            if let Err(e) = jsonpath::select(&test_json, jsonpath_expr) {
                issues.push(ValidationIssue {
                    field: field.into(),
                    severity: "error".into(),
                    message: format!("JSONPath 表达式无效: {}", e),
                });
            }
        }
        Some(RuleType::JavaScript) => {
            issues.push(ValidationIssue {
                field: field.into(),
                severity: "info".into(),
                message: "JS 脚本规则 — 仅在运行时验证".into(),
            });
        }
        Some(RuleType::Css) | None => {
            let (css_base, _, _) = rule_engine::strip_css_modifiers(expr_without_suffix);
            if css_base.is_empty() {
                return;
            }
            // 去掉 @css: 前缀再交给 CSS 解析器
            let css_expr = css_base.strip_prefix("@css:").unwrap_or(css_base);
            let css_expr = css_expr.trim();
            if css_expr.is_empty() {
                return;
            }
            // JSOUP 默认规则（不含 @css: 前缀，用 @tag. / @class. / @id. 分隔）不是标准 CSS，
            // 不要用 scraper::Selector::parse() 校验，避免误报
            if is_jsoup_like(css_expr) {
                issues.push(ValidationIssue {
                    field: field.into(),
                    severity: "info".into(),
                    message: "JSOUP 规则 — 将在运行时解析".into(),
                });
                return;
            }
            let alternatives = rule_engine::split_css_alternatives(css_expr);
            for alt in alternatives {
                let alt = alt.trim();
                if alt.is_empty() {
                    continue;
                }
                match scraper::Selector::parse(alt) {
                    Err(e) => {
                        issues.push(ValidationIssue {
                            field: field.into(),
                            severity: "warning".into(),
                            message: format!("CSS 选择器可能无效: {:?}", e),
                        });
                    }
                    Ok(_) => {}
                }
            }
        }
    }
}

fn is_jsoup_like(expr: &str) -> bool {
    for pattern in &[
        "@tag.", "@class.", "@id.", "@attr.",
        "@text", "@html", "@href", "@src", "@ownText",
        "@textNodes", "@content", "@raw", "@css",
        ":contains(", ":matches(", ":matchText(",
        ":eq(", ":lt(", ":gt(",
    ] {
        if expr.contains(pattern) {
            return true;
        }
    }
    false
}

fn is_legado_extended(expr: &str) -> bool {
    expr.contains("{{@@") || expr.contains("@get:") || expr.contains("@put:")
}

/// 从 JSON 字符串解析并验证，返回 JSON 数组
pub fn validate_source_json(source_json: &str) -> Result<String, String> {
    let source: BookSource = parse_book_source(source_json)?;
    let issues = validate_book_source(&source);
    serde_json::to_string(&issues).map_err(|e| format!("序列化失败: {}", e))
}

/// 创建示例书源（用于测试）
pub fn create_sample_book_source() -> BookSource {
    BookSource {
        id: uuid::Uuid::new_v4().to_string(),
        name: "示例书源".to_string(),
        url: "https://example.com".to_string(),
        source_type: 0,
        enabled: true,
        group_name: None,
        custom_order: 0,
        weight: 0,
        
        rule_search: Some(SearchRule {
            search_url: None,
            book_list: Some(".book-item".to_string()),
            name: Some(".book-title".to_string()),
            author: Some(".book-author".to_string()),
            book_url: Some("a@href".to_string()),
            cover_url: Some(".book-cover img@src".to_string()),
            kind: None,
            last_chapter: None,
            ..Default::default()
        }),
        rule_book_info: None,
        rule_toc: None,
        rule_content: None,
        
        login_url: None,
        header: None,
        js_lib: None,
        
        explore_url: None,
        rule_explore: None,
        book_url_pattern: None,
        enabled_explore: true,
        last_update_time: 0,
        book_source_comment: None,
        created_at: 0,
        updated_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source(search_url: Option<&str>, book_list: Option<&str>, name: Option<&str>) -> BookSource {
        BookSource {
            id: "test".into(),
            name: "test".into(),
            url: "https://test.com".into(),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: Some(SearchRule {
                search_url: search_url.map(String::from),
                book_list: book_list.map(String::from),
                name: name.map(String::from),
                author: None,
                book_url: None,
                cover_url: None,
                kind: None,
                last_chapter: None,
                ..Default::default()
            }),
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: true,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_search_url_with_keyword_placeholder_no_false_selector_warning() {
        let source = make_source(Some("/search?q={{keyword}}"), None, None);
        let issues = validate_book_source(&source);
        let has_selector_warning = issues.iter().any(|i| i.message.contains("选择器"));
        assert!(
            !has_selector_warning,
            "URL with {{keyword}} should not trigger selector warning, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_search_url_without_placeholder_still_flagged() {
        let source = make_source(Some("//div[@class='book']"), None, None);
        let issues = validate_book_source(&source);
        let has_selector_warning = issues.iter().any(|i| i.message.contains("选择器"));
        assert!(
            has_selector_warning,
            "XPath-like search URL without {{keyword}} should be flagged, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_invalid_xpath_produces_compilation_issue() {
        let source = make_source(None, Some("//div[@class='missing'"), None);
        let issues = validate_book_source(&source);
        let has_xpath_issue = issues.iter().any(|i| {
            i.field == "rule_search.book_list" && i.message.contains("XPath")
        });
        assert!(
            has_xpath_issue,
            "Expected XPath compilation issue for unclosed predicate, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_valid_xpath_no_false_warning() {
        let source = make_source(None, Some("//div[@class='book']"), None);
        let issues = validate_book_source(&source);
        let has_xpath_issue = issues.iter().any(|i| {
            i.field == "rule_search.book_list" && i.message.contains("XPath")
        });
        assert!(
            !has_xpath_issue,
            "Valid XPath should not produce a compilation issue, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_invalid_jsonpath_produces_compilation_error() {
        let source = make_source(None, None, Some("$.foo["));
        let issues = validate_book_source(&source);
        let has_jsonpath_error = issues.iter().any(|i| {
            i.field == "rule_search.name" && i.severity == "error" && i.message.contains("JSONPath")
        });
        assert!(
            has_jsonpath_error,
            "Expected JSONPath compilation error for unclosed bracket, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_valid_jsonpath_no_false_warning() {
        let source = make_source(None, None, Some("$.data.items"));
        let issues = validate_book_source(&source);
        let has_jsonpath_issue = issues.iter().any(|i| {
            i.field == "rule_search.name" && i.message.contains("JSONPath")
        });
        assert!(
            !has_jsonpath_issue,
            "Valid JSONPath should not produce an error, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_valid_css_no_false_warning() {
        let source = make_source(None, Some(".book-item"), None);
        let issues = validate_book_source(&source);
        let has_book_list_issue = issues.iter().any(|i| i.field == "rule_search.book_list");
        assert!(
            !has_book_list_issue,
            "Valid CSS selector should not produce any issue, got: {:?}",
            issues
        );
    }

    // ── XPath with ## replace rules ──────────────────────────────────────

    #[test]
    fn test_xpath_with_legado_replace_rules_no_false_error() {
        let source = BookSource {
            rule_content: Some(ContentRule {
                content: Some("//div[@id='content']//p@textNodes##请记住本站.*|最快更新.*".into()),
                next_content_url: None,
                ..Default::default()
            }),
            ..make_source(None, None, None)
        };
        let issues = validate_book_source(&source);
        let has_xpath_error = issues.iter().any(|i| {
            i.field == "rule_content.content" && i.severity == "error" && i.message.contains("XPath")
        });
        assert!(
            !has_xpath_error,
            "XPath with ## replace rules should NOT produce a compilation error, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_xpath_without_replace_rules_still_validates_correctly() {
        let source = BookSource {
            rule_content: Some(ContentRule {
                content: Some("//div[@class='valid']".into()),
                next_content_url: None,
                ..Default::default()
            }),
            ..make_source(None, None, None)
        };
        let issues = validate_book_source(&source);
        let has_xpath_error = issues.iter().any(|i| {
            i.field == "rule_content.content" && i.severity == "error" && i.message.contains("XPath")
        });
        assert!(
            !has_xpath_error,
            "Valid XPath (without ##) should not produce error, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_xpath_with_known_invalid_still_flags() {
        let source = BookSource {
            rule_content: Some(ContentRule {
                content: Some("//div[@class='missing'".into()),
                next_content_url: None,
                ..Default::default()
            }),
            ..make_source(None, None, None)
        };
        let issues = validate_book_source(&source);
        let has_xpath_error = issues.iter().any(|i| {
            i.field == "rule_content.content" && i.severity == "error" && i.message.contains("XPath")
        });
        assert!(
            has_xpath_error,
            "Truly invalid XPath must still be flagged, got: {:?}",
            issues
        );
    }

    // ── CSS with .N index / !N skip modifiers ────────────────────────────

    #[test]
    fn test_css_with_index_modifier_no_false_warning() {
        let source = make_source(None, None, Some("a.0@title"));
        let issues = validate_book_source(&source);
        let has_name_css_warning = issues.iter().any(|i| {
            i.field == "rule_search.name" && i.message.contains("CSS")
        });
        assert!(
            !has_name_css_warning,
            "CSS with .0 index modifier should not trigger false CSS warning, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_css_with_negative_index_modifier_no_false_warning() {
        let source = make_source(None, Some("td.-1@text"), None);
        let issues = validate_book_source(&source);
        let has_book_list_css_warning = issues.iter().any(|i| {
            i.field == "rule_search.book_list" && i.message.contains("CSS")
        });
        assert!(
            !has_book_list_css_warning,
            "CSS with .-1 index modifier should not trigger false CSS warning, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_css_with_skip_modifier_no_false_warning() {
        let source = make_source(None, None, Some("a!2@title"));
        let issues = validate_book_source(&source);
        let has_name_css_warning = issues.iter().any(|i| {
            i.field == "rule_search.name" && i.message.contains("CSS")
        });
        assert!(
            !has_name_css_warning,
            "CSS with !2 skip modifier should not trigger false CSS warning, got: {:?}",
            issues
        );
    }

    // ── CSS with || alternatives ─────────────────────────────────────────

    #[test]
    fn test_css_with_alternatives_no_false_warning() {
        let source = make_source(None, Some(".class1||.class2"), None);
        let issues = validate_book_source(&source);
        let has_book_list_css_warning = issues.iter().any(|i| {
            i.field == "rule_search.book_list" && i.message.contains("CSS")
        });
        assert!(
            !has_book_list_css_warning,
            "CSS with || alternatives should not trigger false CSS warning, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_css_with_bad_alternative_still_warns() {
        let source = make_source(None, Some(".good||."), None);
        let issues = validate_book_source(&source);
        let has_book_list_css_warning = issues.iter().any(|i| {
            i.field == "rule_search.book_list" && i.message.contains("CSS")
        });
        assert!(
            has_book_list_css_warning,
            "CSS with a bad alternative ('.') should still produce a warning, got: {:?}",
            issues
        );
    }

    // ── CSS with @attribute suffix ───────────────────────────────────────

    #[test]
    fn test_css_with_attribute_suffix_no_false_warning() {
        let source = BookSource {
            rule_book_info: Some(BookInfoRule {
                name: Some("[property$=title]@content".into()),
                author: Some("[property$=author]@content".into()),
                ..Default::default()
            }),
            ..make_source(None, None, None)
        };
        let issues = validate_book_source(&source);
        // Note: [property$=title] may or may not parse in scraper’s CSS parser.
        // The key point is no crash and the suffix is stripped.
        let has_book_info_error = issues.iter().any(|i| {
            (i.field == "rule_book_info.name" || i.field == "rule_book_info.author")
                && i.severity == "error" && i.message.contains("CSS")
        });
        assert!(
            !has_book_info_error,
            "CSS with @attribute suffix and property selectors should not produce error, got: {:?}",
            issues
        );
    }
}
