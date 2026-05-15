//! # 工具函数模块
//!
//! 提供书源解析相关的工具函数。

use crate::types::BookSource;
use crate::SearchResult;

/// 构建完整的 URL（处理相对路径）
pub fn build_full_url(base_url: &str, relative_url: &str) -> String {
    if let Ok(base) = url::Url::parse(base_url) {
        if let Ok(full) = base.join(relative_url) {
            return full.to_string();
        }
    }
    relative_url.to_string()
}

/// 替换 URL 中的占位符
pub fn replace_url_placeholders(url_template: &str, keyword: &str) -> String {
    url_template
        .replace("{{keyword}}", keyword)
        .replace("{{encode_keyword}}", &urlencoding::encode(keyword))
}

/// 从规则字符串中提取提取类型
pub fn extract_type_from_rule(rule: &str) -> (String, Option<&str>) {
    let trimmed = rule.trim();

    if let Some(s) = trimmed.strip_suffix("@text") {
        (s.to_string(), Some("text"))
    } else if let Some(s) = trimmed.strip_suffix("@html") {
        (s.to_string(), Some("html"))
    } else if let Some(s) = trimmed.strip_suffix("@ownText") {
        (s.to_string(), Some("ownText"))
    } else if let Some(s) = trimmed.strip_suffix("@href") {
        (s.to_string(), Some("href"))
    } else if let Some(s) = trimmed.strip_suffix("@src") {
        (s.to_string(), Some("src"))
    } else {
        (trimmed.to_string(), None)
    }
}

/// 验证书源规则的完整性
pub fn validate_source_rules(source: &BookSource) -> Vec<String> {
    let mut errors = Vec::new();

    // 检查搜索规则
    if let Some(search_rule) = &source.rule_search {
        if search_rule.book_list.is_none() {
            errors.push("搜索规则缺少 bookList".to_string());
        }
    }

    // 检查详情规则
    if source.rule_book_info.is_some() {
        // 详情规则可以为空，因为有些书源不需要
    }

    // 检查目录规则
    if let Some(toc_rule) = &source.rule_toc {
        if toc_rule.chapter_name.is_none() && toc_rule.chapter_url.is_none() {
            errors.push("目录规则缺少 chapterName 或 chapterUrl".to_string());
        }
    }

    // 检查内容规则
    if let Some(content_rule) = &source.rule_content {
        if content_rule.content.is_none() {
            errors.push("内容规则缺少 content".to_string());
        }
    }

    errors
}

/// 合并多个搜索结果（去重）
pub fn merge_search_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    use std::collections::HashMap;

    let mut seen = HashMap::new();
    let mut merged = Vec::new();

    for result in results {
        let key = format!("{}|{}", result.name, result.author);
        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
            e.insert(true);
            merged.push(result);
        }
    }

    merged
}

/// 清理 HTML 片段（移除多余空白）
pub fn clean_html_fragment(html: &str) -> String {
    // 移除多余的空白和换行
    let re = regex::Regex::new(r"\s+").unwrap();
    re.replace_all(html, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_full_url() {
        assert_eq!(
            build_full_url("https://example.com/books", "/chapter/1"),
            "https://example.com/chapter/1"
        );

        assert_eq!(
            build_full_url("https://example.com/books/", "chapter/1"),
            "https://example.com/books/chapter/1"
        );

        assert_eq!(
            build_full_url("https://example.com", "chapter/1"),
            "https://example.com/chapter/1"
        );
    }

    #[test]
    fn test_replace_placeholders() {
        let url = replace_url_placeholders("/search?q={{keyword}}", "测试");
        assert!(url.contains("测试") || url.contains("%E6%B5%8B%E8%AF%95"));
    }

    #[test]
    fn test_extract_type_from_rule() {
        let (rule, extract_type) = extract_type_from_rule(".title@text");
        assert_eq!(rule, ".title");
        assert_eq!(extract_type, Some("text"));

        let (rule, extract_type) = extract_type_from_rule("//div[@class='test']");
        assert_eq!(rule, "//div[@class='test']");
        assert_eq!(extract_type, None);
    }
}
