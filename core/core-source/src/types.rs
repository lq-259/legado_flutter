//! # 类型定义
//!
//! 提供 core-source 模块使用的核心数据结构。
//! 对应原 Legado 的 BookSource/SearchRule/BookInfoRule 等。

use serde::{Deserialize, Serialize};

/// 书源结构体（对应原 Legado 的 BookSource）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSource {
    pub id: String,
    pub name: String,
    pub url: String,

    #[serde(default)]
    pub source_type: i32, // 0=小说, 1=音频, 2=图片, 3=RSS
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub custom_order: i32,
    #[serde(default)]
    pub weight: i32,

    /// 规则（JSON 格式）
    #[serde(default)]
    pub rule_search: Option<SearchRule>,
    #[serde(default)]
    pub rule_book_info: Option<BookInfoRule>,
    #[serde(default)]
    pub rule_toc: Option<TocRule>,
    #[serde(default)]
    pub rule_content: Option<ContentRule>,

    /// 其他配置
    #[serde(default)]
    pub login_url: Option<String>,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub js_lib: Option<String>,
    #[serde(default)]
    pub explore_url: Option<String>,
    #[serde(default)]
    pub rule_explore: Option<SearchRule>,
    #[serde(default)]
    pub book_url_pattern: Option<String>,
    #[serde(default)]
    pub enabled_explore: bool,
    #[serde(default)]
    pub last_update_time: i64,
    #[serde(default)]
    pub book_source_comment: Option<String>,

    #[serde(default = "now_timestamp")]
    pub created_at: i64,
    #[serde(default = "now_timestamp")]
    pub updated_at: i64,
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 搜索规则
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchRule {
    #[serde(default, alias = "searchUrl")]
    pub search_url: Option<String>, // 搜索URL模板（含{{keyword}}占位符）
    #[serde(default, alias = "bookList")]
    pub book_list: Option<String>, // 搜索结果列表的选择器
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default, alias = "bookUrl")]
    pub book_url: Option<String>,
    #[serde(default, alias = "coverUrl")]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, alias = "lastChapter")]
    pub last_chapter: Option<String>,
    #[serde(default)]
    pub intro: Option<String>,
    #[serde(default, alias = "wordCount")]
    pub word_count: Option<String>,
}

/// 书籍详情规则
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookInfoRule {
    #[serde(default, alias = "bookInfoInit")]
    pub book_info_init: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub intro: Option<String>,
    #[serde(default, alias = "coverUrl")]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, alias = "wordCount")]
    pub word_count: Option<String>,
    #[serde(default, alias = "lastChapter")]
    pub last_chapter: Option<String>,
    #[serde(default, alias = "tocUrl")]
    pub toc_url: Option<String>,
    #[serde(default, alias = "canReName")]
    pub can_rename: Option<String>,
}

/// 目录规则
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TocRule {
    #[serde(default, alias = "chapterList")]
    pub chapter_list: Option<String>,
    #[serde(default, alias = "chapterName")]
    pub chapter_name: Option<String>,
    #[serde(default, alias = "chapterUrl")]
    pub chapter_url: Option<String>,
    #[serde(default, alias = "nextTocUrl")]
    pub next_toc_url: Option<String>,
    #[serde(default, alias = "isVip")]
    pub is_vip: Option<String>,
    #[serde(default, alias = "updateTime")]
    pub update_time: Option<String>,
}

/// 内容规则
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentRule {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, alias = "nextContentUrl")]
    pub next_content_url: Option<String>,
    #[serde(default, alias = "webJs")]
    pub web_js: Option<String>,
    #[serde(default, alias = "sourceRegex")]
    pub source_regex: Option<String>,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub name: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub book_url: String,
    pub kind: Option<String>,
}

/// 提取类型后缀
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExtractType {
    Text,         // @text - 提取文本内容
    Html,         // @html - 提取 HTML 结构
    OwnText,      // @ownText - 仅元素自身文本
    Href,         // @href - 提取链接地址
    Src,          // @src - 提取资源地址
    TextNode,     // @textNode - 单个文本节点 / @textNodes - 所有文本节点
    Content,      // @content - 提取 content 属性 (用于 meta 标签等)
    Attr(String), // @attrName - 提取任意属性 (e.g. @title, @data-id)
    #[default]
    None, // 无后缀
}

impl ExtractType {
    /// 从规则字符串中解析提取类型
    pub fn from_rule(rule: &str) -> (&str, Self) {
        if let Some(s) = rule.strip_suffix("@textNodes") {
            (s, Self::TextNode)
        } else if let Some(s) = rule.strip_suffix("@textNode") {
            (s, Self::TextNode)
        } else if let Some(s) = rule.strip_suffix("@ownText") {
            (s, Self::OwnText)
        } else if let Some(s) = rule.strip_suffix("@content") {
            (s, Self::Content)
        } else if let Some(s) = rule.strip_suffix("@text") {
            (s, Self::Text)
        } else if let Some(s) = rule.strip_suffix("@html") {
            (s, Self::Html)
        } else if let Some(s) = rule.strip_suffix("@href") {
            (s, Self::Href)
        } else if let Some(s) = rule.strip_suffix("@src") {
            (s, Self::Src)
        } else if let Some(pos) = rule.rfind('@') {
            let attr_name = &rule[pos + 1..];
            if !attr_name.is_empty()
                && attr_name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                && pos > 0
            {
                (&rule[..pos], Self::Attr(attr_name.to_string()))
            } else {
                (rule, Self::None)
            }
        } else {
            (rule, Self::None)
        }
    }

    /// Returns the attribute name if this is an Attr variant
    pub fn attribute_name(&self) -> Option<&str> {
        match self {
            Self::Attr(name) => Some(name.as_str()),
            _ => None,
        }
    }
}
