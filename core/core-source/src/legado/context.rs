//! Legado 执行上下文
//!
//! 定义规则执行时的上下文变量，对应 Legado 中的：
//! - baseUrl: 当前页面 URL
//! - result: 上一步规则执行结果
//! - src: 当前页面源码
//! - title: 当前标题
//! - key: 搜索关键词
//! - page: 页码
//! - source: 书源信息
//! - book: 书籍信息
//! - chapter: 章节信息
//! - cookie: Cookie 操作
//! - cache: 缓存操作

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::value::LegadoValue;

/// 规则执行上下文
#[derive(Debug, Clone)]
pub struct RuleContext {
    /// 当前页面 URL (baseUrl)
    pub base_url: String,
    /// 当前页面源码 (src)
    pub src: String,
    /// 上一步结果 (result)
    pub result: Vec<LegadoValue>,
    /// 当前标题
    pub title: String,
    /// 搜索关键词
    pub key: String,
    /// 页码
    pub page: i32,
    /// 自定义变量
    pub variables: HashMap<String, LegadoValue>,
    /// Shared mutable variables across cloned field contexts.
    pub shared_variables: Arc<Mutex<HashMap<String, LegadoValue>>>,
}

impl Default for RuleContext {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            src: String::new(),
            result: Vec::new(),
            title: String::new(),
            key: String::new(),
            page: 1,
            variables: HashMap::new(),
            shared_variables: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl RuleContext {
    pub fn new(base_url: &str, src: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            src: src.to_string(),
            ..Default::default()
        }
    }

    /// 设置搜索结果上下文
    pub fn for_search(base_url: &str, keyword: &str, page: i32) -> Self {
        Self {
            base_url: base_url.to_string(),
            key: keyword.to_string(),
            page,
            ..Default::default()
        }
    }

    /// 设置详情页上下文
    pub fn for_book_info(base_url: &str, html: &str) -> Self {
        let mut book = HashMap::new();
        book.insert("variable".into(), LegadoValue::Map(HashMap::new()));
        let mut variables = HashMap::new();
        variables.insert("book".into(), LegadoValue::Map(book));
        Self {
            base_url: base_url.to_string(),
            src: html.to_string(),
            variables,
            ..Default::default()
        }
    }

    /// 设置目录页上下文
    pub fn for_toc(base_url: &str, html: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            src: html.to_string(),
            ..Default::default()
        }
    }

    /// 设置正文页上下文
    pub fn for_content(base_url: &str, html: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            src: html.to_string(),
            ..Default::default()
        }
    }

    /// 获取变量值（按 Legado 变量名）
    pub fn get_variable(&self, name: &str) -> LegadoValue {
        match name {
            "baseUrl" | "base_url" => LegadoValue::String(self.base_url.clone()),
            "src" => LegadoValue::String(self.src.clone()),
            "result" => {
                if self.result.len() == 1 {
                    self.result[0].clone()
                } else {
                    LegadoValue::Array(self.result.clone())
                }
            }
            "title" => LegadoValue::String(self.title.clone()),
            "key" | "keyword" => LegadoValue::String(self.key.clone()),
            "page" => LegadoValue::Int(self.page as i64),
            _ => self
                .variables
                .get(name)
                .cloned()
                .or_else(|| {
                    self.shared_variables
                        .lock()
                        .ok()
                        .and_then(|vars| vars.get(name).cloned())
                })
                .unwrap_or(LegadoValue::Null),
        }
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: LegadoValue) {
        let name = name.into();
        match name.as_str() {
            "baseUrl" | "base_url" => self.base_url = value.as_string_lossy(),
            "src" => self.src = value.as_string_lossy(),
            "result" => {
                self.result = match value.clone() {
                    LegadoValue::Array(values) => values,
                    other => vec![other],
                };
            }
            "title" => self.title = value.as_string_lossy(),
            "key" | "keyword" => self.key = value.as_string_lossy(),
            "page" => {
                if let LegadoValue::Int(page) = &value {
                    self.page = *page as i32;
                }
            }
            _ => {}
        }
        self.variables.insert(name.clone(), value.clone());
        if let Ok(mut vars) = self.shared_variables.lock() {
            vars.insert(name, value);
        }
    }

    pub fn all_variables(&self) -> HashMap<String, LegadoValue> {
        let mut vars = self
            .shared_variables
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        vars.extend(self.variables.clone());
        vars
    }
}

/// 搜索上下文
#[derive(Debug, Clone)]
pub struct SearchContext {
    pub source_name: String,
    pub source_url: String,
    pub search_url: String,
    pub keyword: String,
    pub page: i32,
    pub headers: Vec<(String, String)>,
    pub charset: Option<String>,
}

/// 目录上下文
#[derive(Debug, Clone)]
pub struct TocContext {
    pub source_name: String,
    pub source_url: String,
    pub book_url: String,
    pub book_name: String,
    pub headers: Vec<(String, String)>,
}

/// 正文上下文
#[derive(Debug, Clone)]
pub struct ContentContext {
    pub source_name: String,
    pub source_url: String,
    pub chapter_url: String,
    pub chapter_title: String,
    pub book_name: String,
    pub headers: Vec<(String, String)>,
}

/// 传统 LegadoContext（向后兼容）
#[derive(Debug, Clone)]
pub struct LegadoContext {
    pub base_url: String,
    pub src: String,
    pub result: String,
    pub content: String,
    pub url: String,
    pub headers: String,
    pub source_name: Option<String>,
}
