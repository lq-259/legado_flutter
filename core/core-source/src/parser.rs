//! # 书源解析器模块
//!
//! 整合规则引擎和脚本引擎，提供完整的书源解析功能。
//! 对应原 Legado 的 WebBook 模块 (model/webBook/)。

use crate::rule_engine::RuleEngine;
use crate::script_engine::ScriptEngine;
use crate::types::{BookSource, TocRule};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::{info, warn};
use regex::Regex;
use std::collections::{HashSet, VecDeque};

/// 搜索结果（对应原 Legado 的 SearchBook）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub author: String,
    pub cover_url: Option<String>,
    pub intro: Option<String>,
    pub book_url: String,
    pub source_id: String,
    pub source_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploreEntry {
    pub title: String,
    pub url: String,
}

/// 书籍详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDetail {
    pub id: String,
    pub name: String,
    pub author: String,
    pub cover_url: Option<String>,
    pub intro: Option<String>,
    pub kind: Option<String>,
    pub word_count: Option<String>,
    pub book_url: String,
    pub source_id: String,
    pub chapters_url: Option<String>,
}

/// 章节信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub index: i32,
    #[serde(default)]
    pub is_vip: Option<bool>,
}

/// 章节内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterContent {
    pub chapter_id: String,
    pub content: String,
    pub next_chapter_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_request: Option<PlatformRequest>,
}

/// Request that must be handled by the host platform (Android/WebView layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlatformRequest {
    WebViewContent {
        url: String,
        content_rule: Option<String>,
        web_js: Option<String>,
        source_regex: Option<String>,
        headers: std::collections::HashMap<String, String>,
        user_agent: Option<String>,
    },
}

/// 书源解析器
pub struct BookSourceParser {
    rule_engine: RuleEngine,
    script_engine: ScriptEngine,
    http_client: crate::legado::LegadoHttpClient,
}

impl Default for BookSourceParser {
    fn default() -> Self {
        Self::new()
    }
}

impl BookSourceParser {
    /// 创建新的书源解析器
    pub fn new() -> Self {
        Self {
            rule_engine: RuleEngine::new(),
            script_engine: ScriptEngine::new(),
            http_client: crate::legado::LegadoHttpClient::new(),
        }
    }

    /// Execute a rule string, preferring the legado selector chain for Legado-style rules.
    fn run_rule(&self, rule: &str, html: &str, context: &crate::legado::RuleContext) -> Result<Vec<String>, String> {
        match crate::legado::execute_legado_rule(rule, html, context) {
            Ok(results) if !results.is_empty() => return Ok(results),
            Ok(results) if !can_fallback_to_legacy_rule_engine(rule) => return Ok(results),
            _ => {}
        }
        self.rule_engine.execute_rule(rule, html).map_err(|e| e.to_string())
    }

    /// Execute a rule string and return the first result.
    fn run_rule_first(&self, rule: &str, html: &str, context: &crate::legado::RuleContext) -> Option<String> {
        self.run_rule(rule, html, context).ok()?.into_iter().next()
    }

    async fn fetch_url(&self, source: &BookSource, url: &str, keyword: &str, page: i32) -> Result<String, String> {
        let legado_url = crate::legado::url::parse_legado_url(url);
        let full_url = resolve_source_url(source, &legado_url, keyword, page);
        let headers = parse_source_headers(source.header.as_deref());
        self.http_client
            .request_with_legado_url_and_headers(&full_url, &legado_url, keyword, page, &headers)
            .await
    }

    /// 搜索书籍
    /// 对应原 Legado 的 searchBook 流程
    pub async fn search(&self, source: &BookSource, keyword: &str) -> Vec<SearchResult> {
        info!("搜索书籍: {} (书源: {})", keyword, source.name);
        
        // 1. 构建搜索 URL
        let search_url = match &source.rule_search {
            Some(search_rule) => {
                match search_rule.search_url.as_ref() {
                    Some(url) => url.clone(),
                    None => {
                        warn!("书源 {} 未配置搜索 URL", source.name);
                        return vec![];
                    }
                }
            }
            None => {
                warn!("书源 {} 未配置搜索规则", source.name);
                return vec![];
            }
        };
        
        // 2. 发起 HTTP 请求
        let request_url = resolve_source_url(source, &crate::legado::url::parse_legado_url(&search_url), keyword, 1);
        let html = match self.fetch_url(source, &search_url, keyword, 1).await {
            Ok(text) => text,
            Err(e) => {
                warn!("搜索请求失败: {}", e);
                return vec![];
            }
        };
        let request_context = crate::legado::RuleContext::for_search(&search_url, keyword, 1);
        let request_context = rule_context_with_source_headers(
            rule_context_with_src(request_context, &html),
            source,
        );
        
        // 3. 使用规则解析搜索结果
        let rules = source.rule_search.as_ref().unwrap();
        
        // 4. 解析结果
        let mut results = Vec::new();
        
        let contexts = rules.book_list.as_ref()
            .and_then(|r| self.run_rule(r, &html, &request_context).ok())
            .filter(|items| !items.is_empty())
            .unwrap_or_else(|| vec![html.clone()]);

        let names = extract_from_contexts(self, rules.name.as_deref(), &contexts, &request_context);
        let authors = extract_from_contexts(self, rules.author.as_deref(), &contexts, &request_context);
        let covers = extract_from_contexts(self, rules.cover_url.as_deref(), &contexts, &request_context);
        let book_urls = extract_from_contexts(self, rules.book_url.as_deref(), &contexts, &request_context);
        let intros = extract_from_contexts(self, rules.intro.as_deref(), &contexts, &request_context);

        let max_len = names.len()
            .max(authors.len())
            .max(covers.len())
            .max(book_urls.len())
            .max(intros.len());
        
        for i in 0..max_len {
            results.push(SearchResult {
                id: uuid::Uuid::new_v4().to_string(),
                name: names.get(i).cloned().unwrap_or_default(),
                author: authors.get(i).cloned().unwrap_or_default(),
                cover_url: covers.get(i).cloned()
                    .map(|u| crate::utils::build_full_url(&request_url, &u)),
                intro: None,
                book_url: book_urls.get(i).cloned()
                    .map(|u| crate::utils::build_full_url(&request_url, &u))
                    .unwrap_or_else(|| request_url.clone()),
                source_id: source.id.clone(),
                source_name: source.name.clone(),
            });
        }
        
        info!("搜索完成，找到 {} 个结果", results.len());
        results
    }

    /// 探索/发现书籍
    /// 对应原 Legado 的 explore 流程
    pub async fn explore(&self, source: &BookSource, explore_url: &str, page: i32) -> Vec<SearchResult> {
        info!("探索: {} page={} (书源: {})", explore_url, page, source.name);

        let legado_url = crate::legado::url::parse_legado_url(explore_url);
        let full_url = crate::legado::url::resolve_url_template(&legado_url, "", page, &source.url);

        let html = match self.fetch_url(source, &full_url, "", page).await {
            Ok(text) => text,
            Err(e) => {
                warn!("探索请求失败: {}", e);
                return vec![];
            }
        };

        // Try JSON array format first: [{"title": "...", "url": "..."}]
        if let Ok(json_array) = serde_json::from_str::<Vec<JsonValue>>(&html) {
            let results: Vec<SearchResult> = json_array.iter().filter_map(|item| {
                let title = item.get("title").or_else(|| item.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let url = item.get("url").or_else(|| item.get("bookUrl"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if title.is_empty() || url.is_empty() {
                    return None;
                }
                Some(SearchResult {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: title.to_string(),
                    author: item.get("author").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    cover_url: item.get("cover").or_else(|| item.get("coverUrl"))
                        .and_then(|v| v.as_str())
                        .map(|u| crate::utils::build_full_url(&full_url, u)),
                    intro: item.get("intro").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    book_url: crate::utils::build_full_url(&full_url, url),
                    source_id: source.id.clone(),
                    source_name: source.name.clone(),
                })
            }).collect();
            info!("探索完成 (JSON)，找到 {} 个结果", results.len());
            return results;
        }

        // Try title::url text format
        if html.contains("::") {
            let results: Vec<SearchResult> = html.lines()
                .filter(|line| line.contains("::"))
                .filter_map(|line| {
                    let (title, url) = line.split_once("::")?;
                    let title = title.trim();
                    let url = url.trim();
                    if title.is_empty() || url.is_empty() {
                        return None;
                    }
                    Some(SearchResult {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: title.to_string(),
                        author: String::new(),
                        cover_url: None,
                        intro: None,
                        book_url: crate::utils::build_full_url(&full_url, url),
                        source_id: source.id.clone(),
                        source_name: source.name.clone(),
                    })
                })
                .collect();
            info!("探索完成 (文本)，找到 {} 个结果", results.len());
            return results;
        }

        // Use rule_explore (like search rules) to parse HTML
        if let Some(ref explore_rule) = source.rule_explore {
            let results = self.parse_explore_with_rule(&html, explore_rule, source, &full_url);
            info!("探索完成 (规则)，找到 {} 个结果", results.len());
            return results;
        }

        vec![]
    }

    fn parse_explore_with_rule(&self, html: &str, explore_rule: &crate::types::SearchRule, source: &BookSource, base_url: &str) -> Vec<SearchResult> {
        let context = crate::legado::RuleContext::for_search(base_url, "", 1);
        let request_context = rule_context_with_source_headers(
            rule_context_with_src(context, html),
            source,
        );

        let contexts = explore_rule.book_list.as_ref()
            .and_then(|r| self.run_rule(r, html, &request_context).ok())
            .filter(|items| !items.is_empty())
            .unwrap_or_else(|| vec![html.to_string()]);

        let names = extract_from_contexts(self, explore_rule.name.as_deref(), &contexts, &request_context);
        let authors = extract_from_contexts(self, explore_rule.author.as_deref(), &contexts, &request_context);
        let covers = extract_from_contexts(self, explore_rule.cover_url.as_deref(), &contexts, &request_context);
        let book_urls = extract_from_contexts(self, explore_rule.book_url.as_deref(), &contexts, &request_context);

        let max_len = names.len().max(authors.len()).max(covers.len()).max(book_urls.len());
        (0..max_len).map(|i| {
            SearchResult {
                id: uuid::Uuid::new_v4().to_string(),
                name: names.get(i).cloned().unwrap_or_default(),
                author: authors.get(i).cloned().unwrap_or_default(),
                cover_url: covers.get(i).cloned().map(|u| crate::utils::build_full_url(base_url, &u)),
                intro: None,
                book_url: book_urls.get(i).cloned()
                    .map(|u| crate::utils::build_full_url(base_url, &u))
                    .unwrap_or_else(|| base_url.to_string()),
                source_id: source.id.clone(),
                source_name: source.name.clone(),
            }
        }).collect()
    }

    pub fn get_explore_entries(source: &BookSource) -> Vec<ExploreEntry> {
        let mut entries = Vec::new();
        if let Some(ref explore_url) = source.explore_url {
            for entry in explore_url.split("&&") {
                let entry = entry.trim();
                if let Some((title, url)) = entry.split_once("::") {
                    entries.push(ExploreEntry {
                        title: title.trim().to_string(),
                        url: url.trim().to_string(),
                    });
                }
            }
        }
        entries
    }

    /// 获取书籍详情
    /// 对应原 Legado 的 getBookInfo 流程
    pub async fn get_book_info(&self, source: &BookSource, book_url: &str) -> Option<BookDetail> {
        let book_url = crate::utils::build_full_url(&source.url, book_url);
        info!("获取书籍详情: {} (书源: {})", book_url, source.name);
        
        // 1. 请求书籍页面
        let html = match self.fetch_url(source, &book_url, "", 1).await {
            Ok(text) => text,
            Err(e) => {
                warn!("请求书籍页面失败: {}", e);
                return None;
            }
        };
        let context = rule_context_with_source_headers(
            crate::legado::RuleContext::for_book_info(&book_url, &html),
            source,
        );
        
        // 2. 使用规则解析
        let rules = source.rule_book_info.as_ref()?;

        // Phase 2a: book_info_init - execute init rule and use JSON result if available
        let (working_content, init_context, is_init_json) = execute_book_info_init(
            self,
            rules.book_info_init.as_deref(),
            &html,
            &context,
        )
        .await;

        let extract_field = |rule: Option<&String>| -> Option<String> {
            let rule_str = rule?;
            if rule_str.contains("{{") {
                let resolved = crate::legado::url::resolve_rule_template(rule_str, &working_content, &init_context);
                if resolved.is_empty() { None } else { Some(resolved) }
            } else {
                let effective_rule = if is_init_json && is_simple_field_name(rule_str) {
                    format!("$.{}", rule_str)
                } else {
                    rule_str.clone()
                };
                self.run_rule_first(&effective_rule, &working_content, &init_context)
            }
        };

        let detail_name = extract_field(rules.name.as_ref());
        let detail_author = extract_field(rules.author.as_ref());

        let can_rename_name = rules.can_rename.as_ref()
            .and_then(|rule| self.run_rule_first(rule, &working_content, &init_context))
            .map(|v| !v.is_empty() && v != "false" && v != "0");
        let can_rename_author = rules.can_rename.as_ref()
            .and_then(|rule| self.run_rule_first(rule, &working_content, &init_context))
            .map(|v| !v.is_empty() && v != "false" && v != "0");

        let name = match (can_rename_name, detail_name.as_ref()) {
            (Some(true), Some(dn)) if !dn.is_empty() => dn.clone(),
            (Some(false), _) => String::new(),
            _ => detail_name.clone().unwrap_or_default(),
        };

        let author = match (can_rename_author, detail_author.as_ref()) {
            (Some(true), Some(da)) if !da.is_empty() => Some(da.clone()),
            (Some(false), _) => None,
            _ => detail_author.clone(),
        };

        let intro = extract_field(rules.intro.as_ref());

        let cover_url = extract_field(rules.cover_url.as_ref());

        let kind = extract_field(rules.kind.as_ref());

        let word_count = extract_field(rules.word_count.as_ref());

        // Phase 2b: toc_url - parse directory page URL
        let chapters_url = match rules.toc_url.as_deref() {
            Some(toc_rule) if !toc_rule.trim().is_empty() => {
                let resolved = if toc_rule.contains("{{") {
                    crate::legado::url::resolve_rule_template(toc_rule, &working_content, &init_context)
                } else {
                    let effective_toc_rule = if is_init_json && is_simple_field_name(toc_rule) {
                        format!("$.{}", toc_rule)
                    } else {
                        toc_rule.to_string()
                    };
                    self.run_rule_first(&effective_toc_rule, &working_content, &init_context)
                        .unwrap_or_default()
                };
                if resolved.is_empty() {
                    book_url.clone()
                } else {
                    crate::utils::build_full_url(&book_url, &resolved)
                }
            }
            _ => book_url.clone(),
        };

        Some(BookDetail {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            author: author.unwrap_or_default(),
            cover_url: cover_url.map(|u| crate::utils::build_full_url(&book_url, &u)),
            intro,
            kind,
            word_count,
            book_url: book_url.clone(),
            source_id: source.id.clone(),
            chapters_url: Some(chapters_url),
        })
    }

    /// 获取章节列表 (supports multi-page catalogs via nextTocUrl)
    pub async fn get_chapters(&self, source: &BookSource, book_url: &str) -> Vec<ChapterInfo> {
        let rules = match &source.rule_toc {
            Some(r) => r,
            None => {
                warn!("书源 {} 未配置章节列表规则", source.name);
                return vec![];
            }
        };

        let chapter_list_reverse = rules.chapter_list.as_deref()
            .map_or(false, |r| r.trim_start().starts_with('-'));
        let modified_rules: std::borrow::Cow<TocRule>;
        let effective_rules = if chapter_list_reverse {
            let mut m = rules.clone();
            m.chapter_list = m.chapter_list.map(|s| {
                s.trim_start().trim_start_matches('-').trim().to_string()
            });
            modified_rules = std::borrow::Cow::Owned(m);
            &*modified_rules
        } else {
            rules
        };

        let mut all_chapters: Vec<ChapterInfo> = Vec::new();
        let current_url = crate::utils::build_full_url(&source.url, book_url);
        let mut seen_urls: HashSet<String> = HashSet::new();
        let mut chapter_offset: i32 = 0;
        const MAX_TOC_PAGES: usize = 50;

        info!("开始获取章节列表: {} (书源: {})", current_url, source.name);

        let mut url_queue: VecDeque<String> = VecDeque::new();
        url_queue.push_back(current_url);

        while let Some(url) = url_queue.pop_front() {
            if seen_urls.contains(&url) {
                warn!("检测到目录页 URL 循环: {}", url);
                continue;
            }
            if seen_urls.len() >= MAX_TOC_PAGES {
                warn!("目录页数量达到上限: {}", MAX_TOC_PAGES);
                break;
            }
            seen_urls.insert(url.clone());

            let first_page = seen_urls.len() == 1;
            let html = match self.fetch_url(source, &url, "", 1).await {
                Ok(text) => text,
                Err(e) => {
                    if first_page {
                        warn!("请求章节列表失败 (首页): {}", e);
                        return vec![];
                    }
                    warn!("请求章节列表失败: {}", e);
                    continue;
                }
            };
            let context = rule_context_with_source_headers(
                crate::legado::RuleContext::for_toc(&url, &html),
                source,
            );

            let (chapter_names, chapter_urls, chapter_vips) = self
                .parse_chapters_from_page(source, effective_rules, &html, &context, &url)
                .await;

            let max_len = chapter_names.len().max(chapter_urls.len()).max(chapter_vips.len());
            for i in 0..max_len {
                let title = chapter_names.get(i).cloned()
                    .unwrap_or_else(|| format!("第 {} 章", chapter_offset + i as i32 + 1));
                let chapter_url_val = chapter_urls.get(i)
                    .cloned()
                    .map(|u| crate::utils::build_full_url(&url, &u))
                    .unwrap_or_default();
                let is_vip = chapter_vips.get(i).copied().flatten();
                all_chapters.push(ChapterInfo {
                    id: uuid::Uuid::new_v4().to_string(),
                    title,
                    url: chapter_url_val,
                    index: chapter_offset + i as i32,
                    is_vip,
                });
            }
            chapter_offset += max_len as i32;

            let next_urls: Vec<String> = match rules.next_toc_url.as_deref() {
                Some(next_rule) if !next_rule.trim().is_empty() => {
                    if next_rule.contains("{{") {
                        let resolved = crate::legado::url::resolve_rule_template(next_rule, &html, &context);
                        if resolved.is_empty() { Vec::new() } else { vec![resolved] }
                    } else {
                        self.run_rule(next_rule, &html, &context).unwrap_or_default()
                    }
                }
                _ => Vec::new(),
            };
            for next in next_urls {
                if !next.trim().is_empty() {
                    let full_url = crate::utils::build_full_url(&url, &next);
                    if !full_url.is_empty() && !seen_urls.contains(&full_url) {
                        url_queue.push_back(full_url);
                    }
                }
            }
        }

        if chapter_list_reverse {
            all_chapters.reverse();
            for (i, ch) in all_chapters.iter_mut().enumerate() {
                ch.index = i as i32;
            }
        }

        info!("章节列表获取完成，共 {} 章 ({} 页)", all_chapters.len(), seen_urls.len());
        all_chapters
    }

    /// Parse chapters from a single catalog page
    async fn parse_chapters_from_page(
        &self,
        source: &BookSource,
        rules: &TocRule,
        html: &str,
        context: &crate::legado::RuleContext,
        book_url: &str,
    ) -> (Vec<String>, Vec<String>, Vec<Option<bool>>) {
        let Some(chapter_list_rule) = rules.chapter_list.as_deref() else {
            return (Vec::new(), Vec::new(), Vec::new());
        };

        if chapter_list_rule.trim_start().starts_with("@js:") {
            match execute_chapter_list_js_rule_blocking(
                chapter_list_rule,
                html,
                context,
                self.http_client.cookie_jar(),
            ).await {
                Some(items) => {
                    let names = extract_json_field_from_contexts(rules.chapter_name.as_deref(), &items);
                    let urls = extract_json_field_from_contexts(rules.chapter_url.as_deref(), &items);
                    let vips = rules.is_vip.as_deref()
                        .map(|rule| {
                            items.iter().map(|item| {
                                item.get(rule).and_then(js_is_vip_to_bool)
                            }).collect()
                        })
                        .unwrap_or_else(|| vec![None; items.len()]);
                    return (names, urls, vips);
                }
                None => match self.execute_legado_chapter_list_script(source, book_url, chapter_list_rule).await {
                    Some(items) => {
                        let names = extract_json_field_from_contexts(rules.chapter_name.as_deref(), &items);
                        let urls = extract_json_field_from_contexts(rules.chapter_url.as_deref(), &items);
                        let vips = rules.is_vip.as_deref()
                            .map(|rule| {
                                items.iter().map(|item| {
                                    item.get(rule).and_then(js_is_vip_to_bool)
                                }).collect()
                            })
                            .unwrap_or_else(|| vec![None; items.len()]);
                        return (names, urls, vips);
                    }
                    None => return (Vec::new(), Vec::new(), Vec::new()),
                },
            }
        }

        let item_contexts = match self.run_rule(chapter_list_rule, html, context) {
            Ok(items) if !items.is_empty() => items,
            _ => vec![html.to_string()],
        };
        let names = extract_from_contexts(self, rules.chapter_name.as_deref(), &item_contexts, context);
        let urls = extract_from_contexts(self, rules.chapter_url.as_deref(), &item_contexts, context);
        let vips = rules.is_vip.as_deref()
            .map(|rule| {
                item_contexts.iter().map(|item| {
                    let mut ctx = context.clone();
                    ctx.result = vec![crate::legado::LegadoValue::String(item.clone())];
                    self.run_rule_first(rule, item, &ctx)
                        .map(|v| !v.is_empty() && v != "false" && v != "0")
                }).collect()
            })
            .unwrap_or_else(|| vec![None; item_contexts.len()]);
        (names, urls, vips)
    }

    /// 获取章节内容
    pub async fn get_chapter_content(&self, source: &BookSource, chapter_url: &str) -> Option<ChapterContent> {
        const MAX_CONTENT_PAGES: usize = 50;

        let initial_url = crate::utils::build_full_url(&source.url, chapter_url);
        info!("获取章节内容: {} (书源: {})", initial_url, source.name);

        let current_url = initial_url.clone();
        let mut all_content = String::new();
        let mut seen_urls = HashSet::new();
        let mut final_next_chapter_url = None;

        let mut url_queue: VecDeque<String> = VecDeque::new();
        url_queue.push_back(current_url);

        while let Some(url) = url_queue.pop_front() {
            if !seen_urls.insert(url.clone()) {
                warn!("检测到重复内容页 URL, 跳过: {}", url);
                continue;
            }
            if seen_urls.len() > MAX_CONTENT_PAGES {
                warn!("内容页数量达到上限: {}", MAX_CONTENT_PAGES);
                break;
            }

            let first_page = seen_urls.len() == 1;
            let html = match self.fetch_url(source, &url, "", 1).await {
                Ok(text) => text,
                Err(e) => {
                    if first_page {
                        if e.starts_with("WEBVIEW_REQUIRED") {
                            let (web_js, source_regex, content_rule) = source.rule_content.as_ref()
                                .map(|r| (r.web_js.clone(), r.source_regex.clone(), r.content.clone()))
                                .unwrap_or((None, None, None));
                            return Some(ChapterContent {
                                chapter_id: uuid::Uuid::new_v4().to_string(),
                                content: String::new(),
                                next_chapter_url: None,
                                platform_request: Some(PlatformRequest::WebViewContent {
                                    url: url.clone(),
                                    content_rule,
                                    web_js,
                                    source_regex,
                                    headers: parse_source_headers(source.header.as_deref()).into_iter().collect(),
                                    user_agent: source_user_agent(source.header.as_deref()),
                                }),
                            });
                        }
                        warn!("请求章节内容失败: {}", e);
                        return None;
                    }
                    warn!("请求后续内容页失败: {}", e);
                    continue;
                }
            };

            let context = rule_context_with_source_headers(
                crate::legado::RuleContext::for_content(&url, &html),
                source,
            );
            let mut context = context;
            context.set_variable("chapter", crate::legado::LegadoValue::Map(chapter_context_map(&url, chapter_url)));

            if let Some(rule) = &source.rule_content {
                if rule.web_js.as_deref().is_some_and(|s| !s.trim().is_empty())
                    || rule.source_regex.as_deref().is_some_and(|s| !s.trim().is_empty())
                {
                    warn!("正文规则需要平台 WebView/sourceRegex 支持: {}", url);
                    return Some(ChapterContent {
                        chapter_id: uuid::Uuid::new_v4().to_string(),
                        content: String::new(),
                        next_chapter_url: None,
                        platform_request: Some(PlatformRequest::WebViewContent {
                            url: url.clone(),
                            content_rule: rule.content.clone(),
                            web_js: rule.web_js.clone(),
                            source_regex: rule.source_regex.clone(),
                            headers: parse_source_headers(source.header.as_deref()).into_iter().collect(),
                            user_agent: source_user_agent(source.header.as_deref()),
                        }),
                    });
                }
            }

            let (page_content, next_urls) = match &source.rule_content {
                Some(rule) => {
                    let content_str = rule.content.as_deref().unwrap_or("");
                    let parsed = if content_str.contains("{{") {
                        crate::legado::url::resolve_rule_template(content_str, &html, &context)
                    } else if content_str.trim_start().starts_with("@js:") {
                        if let Some(content) = self.run_rule_first_blocking(content_str, &html, &context).await.filter(|s| !s.is_empty()) {
                            content
                        } else {
                            self.execute_legado_content_script(source, &url, content_str, &html).await
                                .unwrap_or_default()
                        }
                    } else {
                        self.run_rule_first(content_str, &html, &context)
                            .unwrap_or_default()
                    };
                    let nexts: Vec<String> = rule.next_content_url.as_deref()
                        .map(|r| {
                            if r.contains("{{") {
                                let resolved = crate::legado::url::resolve_rule_template(r, &html, &context);
                                if resolved.is_empty() { Vec::new() } else { vec![resolved] }
                            } else {
                                self.run_rule(r, &html, &context).unwrap_or_default()
                            }
                        })
                        .unwrap_or_default();
                    (parsed, nexts)
                }
                None => {
                    warn!("书源 {} 未配置内容规则", source.name);
                    (String::new(), Vec::new())
                }
            };

            let page_content = resolve_image_src_headers(&page_content, &url);
            if !first_page && !page_content.is_empty() {
                all_content.push('\n');
            }
            all_content.push_str(&page_content);

            if let Some(first_next) = next_urls.first() {
                let full_next = crate::utils::build_full_url(&url, first_next);
                final_next_chapter_url = Some(full_next);
            }
            for next in next_urls {
                if !next.is_empty() {
                    let full_url = crate::utils::build_full_url(&url, &next);
                    if !full_url.is_empty() && !seen_urls.contains(&full_url) {
                        url_queue.push_back(full_url);
                    }
                }
            }
        }

        if all_content.is_empty() {
            return None;
        }

        let mut content = if let Some(ref js_lib) = source.js_lib {
            let ctx = crate::script_engine::ScriptContext::new(
                "",
                &all_content,
                chapter_url,
            );
            match self.script_engine.eval(js_lib, Some(&ctx)) {
                Ok(result) => result.as_string().unwrap_or(all_content),
                Err(_) => all_content,
            }
        } else {
            all_content
        };

        Some(ChapterContent {
            chapter_id: uuid::Uuid::new_v4().to_string(),
            content,
            next_chapter_url: final_next_chapter_url,
            platform_request: None,
        })
    }

    async fn execute_legado_chapter_list_script(
        &self,
        source: &BookSource,
        book_url: &str,
        script: &str,
    ) -> Option<Vec<JsonValue>> {
        if !script.contains("/novel/clist/") || !script.contains("java.post") {
            warn!("暂不支持的目录 JS 规则: {}", script.chars().take(80).collect::<String>());
            return None;
        }

        let bid = regex::Regex::new(r"/read/(\d+)/")
            .ok()?
            .captures(book_url)?
            .get(1)?
            .as_str()
            .to_string();
        let url = crate::utils::build_full_url(&source.url, "/novel/clist/");
        let body = format!("bid={}", bid);
        let text = self.http_client
            .post(
                &url,
                &body,
                &[("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string())],
                None,
            )
            .await
            .ok()?;
        let json: JsonValue = serde_json::from_str(&text).ok()?;
        let mut items = json.get("data")?.as_array()?.clone();
        let mut volume_name = String::new();

        for idx in (0..items.len()).rev() {
            let is_volume = items[idx]
                .get("ctype")
                .and_then(|v| v.as_str())
                .map(|v| v == "1")
                .unwrap_or(false);
            if is_volume {
                volume_name = items[idx]
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                items.remove(idx);
                continue;
            }

            let page = items[idx]
                .get("ordernum")
                .and_then(json_scalar_to_string)
                .unwrap_or_default();
            if let Some(obj) = items[idx].as_object_mut() {
                obj.insert(
                    "url".to_string(),
                    JsonValue::String(crate::utils::build_full_url(
                        &source.url,
                        &format!("/read/{}/p{}.html", bid, page),
                    )),
                );
                obj.insert("n".to_string(), JsonValue::String(volume_name.clone()));
            }
        }

        Some(items)
    }

    async fn execute_legado_content_script(
        &self,
        _source: &BookSource,
        chapter_url: &str,
        script: &str,
        html: &str,
    ) -> Option<String> {
        if !script.contains("challenge") || !script.contains("java.ajax") {
            warn!("暂不支持的正文 JS 规则: {}", script.chars().take(80).collect::<String>());
            return None;
        }

        let token = regex::Regex::new(r#"token\s*=\s*"([^"]+)""#)
            .ok()?
            .captures(html)?
            .get(1)?
            .as_str()
            .to_string();
        let url = build_challenge_url(chapter_url, &token)?;
        let text = self.http_client.get(&url, &[], None).await.ok()?;
        let section = regex::Regex::new(r"(?is)<section>\s*((?:<p>.*?</p>\s*)+).*?</section>")
            .ok()?
            .captures(&text)?
            .get(1)?
            .as_str()
            .to_string();
        Some(
            section
                .replace("<p>", "\n")
                .replace("</p>", "")
                .trim()
                .to_string(),
        )
    }

    async fn run_rule_first_blocking(
        &self,
        rule: &str,
        html: &str,
        context: &crate::legado::RuleContext,
    ) -> Option<String> {
        let rule = rule.to_string();
        let html = html.to_string();
        let context = context.clone();
        let cookie_jar = self.http_client.cookie_jar();
        let default_headers = context_default_headers(&context);
        tokio::task::spawn_blocking(move || {
            crate::legado::execute_legado_rule_with_http_state(
                &rule,
                &html,
                &context,
                cookie_jar,
                default_headers,
            )
                .ok()
                .and_then(|values| values.into_iter().next())
        })
        .await
        .ok()
        .flatten()
    }
}

fn build_challenge_url(chapter_url: &str, token: &str) -> Option<String> {
    let mut url = url::Url::parse(chapter_url).ok()?;
    url.set_query(Some(&format!("challenge={}", urlencoding::encode(token))));
    Some(url.to_string())
}

fn can_fallback_to_legacy_rule_engine(rule: &str) -> bool {
    let trimmed = rule.trim_start();
    !(trimmed.starts_with("@js:")
        || trimmed.starts_with("js:")
        || trimmed.contains("<js>")
        || trimmed.starts_with("@put:")
        || trimmed.starts_with("@get:")
        || trimmed.starts_with("@get."))
}

fn resolve_source_url(
    source: &BookSource,
    legado_url: &crate::legado::url::LegadoUrl,
    keyword: &str,
    page: i32,
) -> String {
    crate::legado::url::resolve_url_template(legado_url, keyword, page, &source.url)
}

fn rule_context_with_src(mut context: crate::legado::RuleContext, html: &str) -> crate::legado::RuleContext {
    context.src = html.to_string();
    context
}

fn rule_context_with_source_headers(
    mut context: crate::legado::RuleContext,
    source: &BookSource,
) -> crate::legado::RuleContext {
    let headers = parse_source_headers(source.header.as_deref());
    if !headers.is_empty() {
        let map = headers
            .into_iter()
            .map(|(key, value)| (key, crate::legado::LegadoValue::String(value)))
            .collect();
        context.variables.insert("__source_header".into(), crate::legado::LegadoValue::Map(map));
    }
    context
}

async fn execute_book_info_init(
    parser: &BookSourceParser,
    init_rule: Option<&str>,
    html: &str,
    context: &crate::legado::RuleContext,
) -> (String, crate::legado::RuleContext, bool) {
    let Some(init_rule) = init_rule.filter(|r| !r.trim().is_empty()) else {
        return (html.to_string(), context.clone(), false);
    };

    let rule = init_rule.to_string();
    let html_owned = html.to_string();
    let context_clone = context.clone();
    let cookie_jar = parser.http_client.cookie_jar();
    let default_headers = context_default_headers(&context_clone);

    let init_values = match tokio::task::spawn_blocking(move || {
        crate::legado::execute_legado_rule_values_with_http_state(
            &rule, &html_owned, &context_clone, cookie_jar, default_headers,
        )
    }).await {
        Ok(Ok(values)) => values,
        _ => return (html.to_string(), context.clone(), false),
    };

    if init_values.is_empty() {
        return (html.to_string(), context.clone(), false);
    }

    if init_values.len() == 1 {
        if let crate::legado::LegadoValue::Map(_) = &init_values[0] {
            let json_str = init_values[0].to_json_value().to_string();
            let init_context = crate::legado::RuleContext::for_book_info(
                &context.base_url,
                &json_str,
            );
            return (json_str, init_context, true);
        }
    }

    let init_result = init_values[0].as_string_lossy();
    if init_result.trim().is_empty() {
        return (html.to_string(), context.clone(), false);
    }

    if let Ok(init_json) = serde_json::from_str::<JsonValue>(&init_result) {
        if init_json.is_object() {
            let init_context = crate::legado::RuleContext::for_book_info(
                &context.base_url,
                &init_result,
            );
            return (init_result, init_context, true);
        }
    }

    (init_result, context.clone(), false)
}

fn is_simple_field_name(rule: &str) -> bool {
    let trimmed = rule.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('@')
        && !trimmed.starts_with("//")
        && !trimmed.starts_with("$.")
        && !trimmed.starts_with("$[")
        && !trimmed.starts_with('/')
        && !trimmed.starts_with(':')
        && !trimmed.starts_with("js:")
        && !trimmed.starts_with("regex:")
        && !trimmed.contains('@')
        && !trimmed.contains("class.")
        && !trimmed.contains("id.")
        && !trimmed.contains("tag.")
}

fn parse_source_headers(header: Option<&str>) -> Vec<(String, String)> {
    let Some(header) = header.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };

    if let Ok(value) = serde_json::from_str::<JsonValue>(header) {
        return crate::legado::url::parse_headers(&Some(value));
    }

    header
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            if key.is_empty() {
                None
            } else {
                Some((key.to_string(), value.trim().to_string()))
            }
        })
        .collect()
}

fn source_user_agent(header: Option<&str>) -> Option<String> {
    parse_source_headers(header)
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("user-agent"))
        .map(|(_, value)| value)
}

fn execute_chapter_list_js_rule(
    rule: &str,
    html: &str,
    context: &crate::legado::RuleContext,
    cookie_jar: std::sync::Arc<reqwest::cookie::Jar>,
) -> Option<Vec<JsonValue>> {
    let values = crate::legado::execute_legado_rule_values_with_http_state(
        rule,
        html,
        context,
        cookie_jar,
        context_default_headers(context),
    ).ok()?;
    let items: Vec<JsonValue> = values
        .into_iter()
        .map(|value| value.to_json_value())
        .filter(|value| !value.is_null())
        .collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn context_default_headers(context: &crate::legado::RuleContext) -> Vec<(String, String)> {
    context
        .variables
        .get("__source_header")
        .and_then(|value| value.as_map())
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.as_string_lossy()))
                .collect()
        })
        .unwrap_or_default()
}

fn chapter_context_map(current_url: &str, original_url: &str) -> std::collections::HashMap<String, crate::legado::LegadoValue> {
    let mut map = std::collections::HashMap::new();
    map.insert("url".into(), crate::legado::LegadoValue::String(current_url.to_string()));
    map.insert("baseUrl".into(), crate::legado::LegadoValue::String(current_url.to_string()));
    map.insert("bookUrl".into(), crate::legado::LegadoValue::String(original_url.to_string()));
    map.insert("title".into(), crate::legado::LegadoValue::String(String::new()));
    map.insert("index".into(), crate::legado::LegadoValue::Int(0));
    map.insert("resourceUrl".into(), crate::legado::LegadoValue::String(String::new()));
    map.insert("tag".into(), crate::legado::LegadoValue::String(String::new()));
    map.insert("start".into(), crate::legado::LegadoValue::Int(0));
    map.insert("end".into(), crate::legado::LegadoValue::Int(0));
    map.insert("variable".into(), crate::legado::LegadoValue::Map(std::collections::HashMap::new()));
    map.insert("isVip".into(), crate::legado::LegadoValue::Bool(false));
    map.insert("is_vip".into(), crate::legado::LegadoValue::Bool(false));
    map
}

fn resolve_image_src_headers(content: &str, base_url: &str) -> String {
    let img_re = regex::Regex::new(r#"<img\s+[^>]*src="([^"]*)"[^>]*>"#).unwrap();
    img_re.replace_all(content, |caps: &regex::Captures| -> String {
        let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        let src = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if let Some(comma_idx) = src.find(",{") {
            let url = &src[..comma_idx];
            let resolved = if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("data:") {
                crate::utils::build_full_url(base_url, url)
            } else {
                url.to_string()
            };
            full_match.replace(src, &resolved)
        } else {
            let resolved = if !src.starts_with("http://") && !src.starts_with("https://") && !src.starts_with("data:") {
                crate::utils::build_full_url(base_url, src)
            } else {
                src.to_string()
            };
            full_match.replace(src, &resolved)
        }
    }).to_string()
}

async fn execute_chapter_list_js_rule_blocking(
    rule: &str,
    html: &str,
    context: &crate::legado::RuleContext,
    cookie_jar: std::sync::Arc<reqwest::cookie::Jar>,
) -> Option<Vec<JsonValue>> {
    let rule = rule.to_string();
    let html = html.to_string();
    let context = context.clone();
    tokio::task::spawn_blocking(move || execute_chapter_list_js_rule(&rule, &html, &context, cookie_jar))
        .await
        .ok()
        .flatten()
}

fn extract_from_contexts(
    parser: &BookSourceParser,
    rule: Option<&str>,
    contexts: &[String],
    base_context: &crate::legado::RuleContext,
) -> Vec<String> {
    let Some(rule) = rule else {
        return Vec::new();
    };
    contexts
        .iter()
        .filter_map(|item| {
            let mut context = base_context.clone();
            context.result = vec![crate::legado::LegadoValue::String(item.clone())];
            if rule.contains("{{") {
                let resolved = crate::legado::url::resolve_rule_template(rule, item, &context);
                if resolved.is_empty() { None } else { Some(resolved) }
            } else {
                parser.run_rule_first(rule, item, &context)
            }
        })
        .collect()
}

fn extract_json_field_from_contexts(rule: Option<&str>, contexts: &[JsonValue]) -> Vec<String> {
    let Some(rule) = rule else {
        return Vec::new();
    };
    contexts
        .iter()
        .filter_map(|item| item.get(rule).and_then(json_scalar_to_string))
        .collect()
}

pub fn source_matches_url(source: &BookSource, url: &str) -> bool {
    let Some(ref pattern) = source.book_url_pattern else {
        return true;
    };
    if pattern.trim().is_empty() {
        return true;
    }
    Regex::new(pattern).is_ok_and(|re| re.is_match(url))
}

fn json_scalar_to_string(value: &JsonValue) -> Option<String> {
    if let Some(s) = value.as_str() {
        Some(s.to_string())
    } else if value.is_number() || value.is_boolean() {
        Some(value.to_string())
    } else {
        None
    }
}

fn js_is_vip_to_bool(value: &JsonValue) -> Option<bool> {
    if let Some(s) = value.as_str() {
        Some(!s.is_empty() && s != "false" && s != "0")
    } else if let Some(b) = value.as_bool() {
        Some(b)
    } else if let Some(n) = value.as_i64() {
        Some(n != 0)
    } else if let Some(n) = value.as_f64() {
        Some(n != 0.0)
    } else {
        None
    }
}

/// 便捷函数：快速搜索（使用默认解析器）
pub async fn search_book(source: &BookSource, keyword: &str) -> Vec<SearchResult> {
    let parser = BookSourceParser::new();
    parser.search(source, keyword).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BookSource, SearchRule, BookInfoRule, ContentRule};

    #[tokio::test]
    async fn test_search_books() {
        let source = BookSource {
            id: "test".into(),
            name: "Test".into(),
            url: "https://example.com".into(),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };
        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_no_search_url() {
        let source = BookSource {
            id: "test2".into(),
            name: "Test2".into(),
            url: "https://example.com".into(),
            rule_search: Some(SearchRule {
                search_url: None,
                book_list: Some(".book".into()),
                name: Some(".title".into()),
                author: Some(".author".into()),
                book_url: Some("a@href".into()),
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
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
        };
        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_chapter_content_with_next_url() {
        let content = ChapterContent {
            chapter_id: "ch1".into(),
            content: "test content".into(),
            next_chapter_url: Some("https://next.example.com/ch2".into()),
            platform_request: None,
        };
        assert_eq!(content.content, "test content");
        assert_eq!(content.next_chapter_url, Some("https://next.example.com/ch2".into()));
    }

    #[tokio::test]
    async fn test_get_chapter_content_with_mock_server() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let html = r#"<html><body><div class="content">Chapter text here</div><a class="next" href="/ch2.html">Next</a></body></html>"#;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/ch1.html");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(html);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("div.content@text".into()),
                next_content_url: Some("a.next@href".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let result = parser.get_chapter_content(&source, "/ch1.html").await;
        assert!(result.is_some(), "expected chapter content, got None");
        let chapter = result.unwrap();
        assert!(chapter.content.contains("Chapter text here"));
        assert!(chapter.next_chapter_url.is_some());
        assert_eq!(
            chapter.next_chapter_url.unwrap(),
            server.url("/ch2.html"),
            "next_chapter_url must be the fully normalized URL"
        );

        mock.assert();
    }

    #[tokio::test]
    async fn test_search_with_mock_server() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let html = r#"<html><body>
            <div class="book-item">
                <span class="book-title">Test Book</span>
                <span class="book-author">Test Author</span>
                <a href="/book/123">Read</a>
                <img class="book-cover" src="/covers/123.jpg" />
            </div>
            <div class="book-item">
                <span class="book-title">Second Book</span>
                <span class="book-author">Second Author</span>
                <a href="/book/456">Read</a>
                <img class="book-cover" src="/covers/456.jpg" />
            </div>
        </body></html>"#;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/search");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(html);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_search: Some(SearchRule {
                search_url: Some("/search?keyword={{keyword}}".into()),
                book_list: Some(".book-item".into()),
                name: Some(".book-title".into()),
                author: Some(".book-author".into()),
                book_url: Some("a@href".into()),
                cover_url: Some(".book-cover@src".into()),
                kind: None,
                last_chapter: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;

        assert_eq!(results.len(), 2, "expected 2 results, got {}", results.len());
        assert_eq!(results[0].name, "Test Book");
        assert_eq!(results[0].author, "Test Author");
        assert_eq!(results[0].book_url, server.url("/book/123"));
        assert_eq!(results[0].cover_url, Some(server.url("/covers/123.jpg")));

        assert_eq!(results[1].name, "Second Book");
        assert_eq!(results[1].author, "Second Author");
        assert_eq!(results[1].book_url, server.url("/book/456"));
        assert_eq!(results[1].cover_url, Some(server.url("/covers/456.jpg")));

        mock.assert();
    }

    #[tokio::test]
    async fn test_search_field_url_template_is_resolved_without_selector_parse() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/search");
            then.status(200)
                .header("Content-Type", "application/json; charset=utf-8")
                .body(r#"{"data":[{"id":42,"name":"Template Book","thumb":"/covers/42.jpg"}]}"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_search: Some(SearchRule {
                search_url: Some("/api/search?q={{keyword}}".into()),
                book_list: Some("$.data[*]".into()),
                name: Some("$.name".into()),
                author: None,
                book_url: Some("https://example.test/book/{{$.id}}".into()),
                cover_url: Some("https://img.example.test{{$.thumb}}".into()),
                kind: None,
                last_chapter: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Template Book");
        assert_eq!(results[0].book_url, "https://example.test/book/42");
        assert_eq!(results[0].cover_url, Some("https://img.example.test/covers/42.jpg".into()));
        mock.assert();
    }

    #[tokio::test]
    async fn test_search_jsonpath_array_context_is_expanded() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/search");
            then.status(200)
                .header("Content-Type", "application/json; charset=utf-8")
                .body(r#"{"data":{"items":[{"name":"One","id":1},{"name":"Two","id":2}]}}"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_search: Some(SearchRule {
                search_url: Some("/api/search?q={{keyword}}".into()),
                book_list: Some("$.data.items".into()),
                name: Some("$.name".into()),
                author: None,
                book_url: Some("https://example.test/book/{{$.id}}".into()),
                cover_url: None,
                kind: None,
                last_chapter: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "One");
        assert_eq!(results[0].book_url, "https://example.test/book/1");
        assert_eq!(results[1].name, "Two");
        assert_eq!(results[1].book_url, "https://example.test/book/2");
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_book_info_with_kind_and_word_count() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let html = r#"<html><body>
            <h1 class="book-name">Test Novel</h1>
            <span class="book-author">Test Author</span>
            <span class="book-kind">都市</span>
            <span class="book-word-count">100万字</span>
        </body></html>"#;

        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/789");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(html);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_book_info: Some(BookInfoRule {
                name: Some(".book-name@text".into()),
                author: Some(".book-author@text".into()),
                kind: Some(".book-kind@text".into()),
                word_count: Some(".book-word-count@text".into()),
                intro: None,
                cover_url: None,
                last_chapter: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let result = parser.get_book_info(&source, "/book/789").await;
        assert!(result.is_some(), "expected book detail, got None");
        let detail = result.unwrap();
        assert_eq!(detail.name, "Test Novel");
        assert_eq!(detail.author, "Test Author");
        assert_eq!(detail.kind.as_deref(), Some("都市"));
        assert_eq!(detail.word_count.as_deref(), Some("100万字"));

        mock.assert();
    }

    #[tokio::test]
    async fn test_search_uses_legado_url_option_js_and_source_header() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let html = r#"<html><body>
            <div class="book-item"><span class="book-title">Changed Book</span><a href="/book/changed">Read</a></div>
        </body></html>"#;

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/changed")
                .header("X-Source", "source-ok")
                .header("X-Option", "option-ok");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(html);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_search: Some(SearchRule {
                search_url: Some(format!(
                    "/original, {{\"js\": \"java.url = '{}'; java.headerMap.put('X-Option', 'option-ok')\"}}",
                    server.url("/changed")
                )),
                book_list: Some(".book-item".into()),
                name: Some(".book-title@text".into()),
                author: None,
                book_url: Some("a@href".into()),
                cover_url: None,
                kind: None,
                last_chapter: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: Some(r#"{"X-Source":"source-ok"}"#.into()),
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.search(&source, "test").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Changed Book");
        assert_eq!(results[0].book_url, server.url("/book/changed"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_chapters_uses_generic_js_rule_result() {
        use httpmock::prelude::*;
        use crate::types::TocRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/1");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body("<html></html>");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_toc: Some(TocRule {
                chapter_list: Some(r#"@js:
                    var chapters = [
                        {"title":"Chapter A","url":"/a.html"},
                        {"title":"Chapter B","url":"/b.html"}
                    ];
                    chapters;
                "#.into()),
                chapter_name: Some("title".into()),
                chapter_url: Some("url".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let chapters = parser.get_chapters(&source, "/book/1").await;

        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Chapter A");
        assert_eq!(chapters[0].url, server.url("/a.html"));
        assert_eq!(chapters[1].title, "Chapter B");
        assert_eq!(chapters[1].url, server.url("/b.html"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_content_uses_generic_js_rule() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/chapter/1");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body("<html><body><p>Raw</p></body></html>");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("@js:\nvar text = 'Generic JS Content';\ntext;".into()),
                next_content_url: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let content = parser.get_chapter_content(&source, "/chapter/1").await.unwrap();

        assert_eq!(content.content, "Generic JS Content");
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_chapters_generic_js_rule_can_use_java_post() {
        use httpmock::prelude::*;
        use crate::types::TocRule;

        let server = MockServer::start();
        let page_mock = server.mock(|when, then| {
            when.method(GET).path("/read/123/");
            then.status(200).body("<html></html>");
        });
        let api_mock = server.mock(|when, then| {
            when.method(POST).path("/novel/clist/").body("bid=123");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"data":[{"title":"Remote A","url":"/ra.html"},{"title":"Remote B","url":"/rb.html"}]}"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_toc: Some(TocRule {
                chapter_list: Some(r#"@js:
                    var bid = baseUrl.match(/read\/(\d+)/)[1];
                    var resp = java.post(source.getKey() + "/novel/clist/", "bid=" + bid, {});
                    JSON.parse(resp.body()).data;
                "#.into()),
                chapter_name: Some("title".into()),
                chapter_url: Some("url".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let chapters = parser.get_chapters(&source, "/read/123/").await;

        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Remote A");
        assert_eq!(chapters[0].url, server.url("/ra.html"));
        assert_eq!(chapters[1].title, "Remote B");
        assert_eq!(chapters[1].url, server.url("/rb.html"));
        page_mock.assert();
        api_mock.assert();
    }

    #[tokio::test]
    async fn test_get_content_generic_js_rule_can_use_java_ajax() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let page_mock = server.mock(|when, then| {
            when.method(GET).path("/chapter/2");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body("<html><script>var token = \"abc\";</script></html>");
        });
        let ajax_mock = server.mock(|when, then| {
            when.method(GET).path("/ajax").query_param("challenge", "abc");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body("<section><p>Ajax Content</p></section>");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some(r#"@js:
                    var token = src.match(/token\s*=\s*"([^"]+)"/)[1];
                    var text = java.ajax(source.getKey() + "/ajax?challenge=" + encodeURIComponent(token));
                    text.match(/<p>(.*?)<\/p>/)[1];
                "#.into()),
                next_content_url: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let content = parser.get_chapter_content(&source, "/chapter/2").await.unwrap();

        assert_eq!(content.content, "Ajax Content");
        page_mock.assert();
        ajax_mock.assert();
    }

    #[tokio::test]
    async fn test_generic_js_rule_shares_parser_cookie_jar() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let page_mock = server.mock(|when, then| {
            when.method(GET).path("/chapter/cookie");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Set-Cookie", "sid=parser-cookie; Path=/")
                .body("<html></html>");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("@js: java.getCookie(baseUrl, 'sid');".into()),
                next_content_url: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let content = parser.get_chapter_content(&source, "/chapter/cookie").await.unwrap();

        assert_eq!(content.content, "parser-cookie");
        page_mock.assert();
    }

    #[tokio::test]
    async fn test_generic_js_ajax_inherits_source_header() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let page_mock = server.mock(|when, then| {
            when.method(GET).path("/chapter/header");
            then.status(200).body("<html></html>");
        });
        let ajax_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/needs-header")
                .header("X-Source", "source-ok");
            then.status(200).body("header-ok");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("@js: java.ajax(source.getKey() + '/needs-header');".into()),
                next_content_url: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: Some(r#"{"X-Source":"source-ok"}"#.into()),
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let content = parser.get_chapter_content(&source, "/chapter/header").await.unwrap();

        assert_eq!(content.content, "header-ok");
        page_mock.assert();
        ajax_mock.assert();
    }

    #[tokio::test]
    async fn test_generic_js_explicit_header_overrides_source_header() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let page_mock = server.mock(|when, then| {
            when.method(GET).path("/chapter/override");
            then.status(200).body("<html></html>");
        });
        let ajax_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/override-header")
                .header("X-Source", "explicit-ok");
            then.status(200).body("override-ok");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("@js: java.get(source.getKey() + '/override-header', {'X-Source':'explicit-ok'}).body();".into()),
                next_content_url: None,
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: Some(r#"{"X-Source":"source-default"}"#.into()),
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let content = parser.get_chapter_content(&source, "/chapter/override").await.unwrap();

        assert_eq!(content.content, "override-ok");
        page_mock.assert();
        ajax_mock.assert();
    }

    #[tokio::test]
    async fn test_book_info_init_js_returns_object() {
        use httpmock::prelude::*;
        use crate::types::BookInfoRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/init");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body("<html></html>");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_book_info: Some(BookInfoRule {
                book_info_init: Some("@js:\nreturn {a:'Init Name',b:'Init Author',h:'/toc/list.html'}".into()),
                name: Some("a".into()),
                author: Some("b".into()),
                toc_url: Some("h".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let detail = parser.get_book_info(&source, "/book/init").await.unwrap();

        assert_eq!(detail.name, "Init Name");
        assert_eq!(detail.author, "Init Author");
        assert_eq!(detail.chapters_url, Some(server.url("/toc/list.html")));
        mock.assert();
    }

    #[tokio::test]
    async fn test_book_info_toc_url_selector() {
        use httpmock::prelude::*;
        use crate::types::BookInfoRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/toc");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><a class="read" href="/read/list">Read</a></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_book_info: Some(BookInfoRule {
                name: Some("tag.h1@text".into()),
                author: Some("tag.h2@text".into()),
                toc_url: Some("a.read@href".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let detail = parser.get_book_info(&source, "/book/toc").await.unwrap();

        assert_eq!(detail.chapters_url, Some(server.url("/read/list")));
        mock.assert();
    }

    #[tokio::test]
    async fn test_book_info_init_all_in_one_regex() {
        use httpmock::prelude::*;
        use crate::types::BookInfoRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/regex");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><meta property="name" content="Regex Name"><meta property="author" content="Regex Author"></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_book_info: Some(BookInfoRule {
                book_info_init: Some(r#"@js:
    return {a:'Regex Name',b:'Regex Author'}
"#.into()),
                name: Some("a".into()),
                author: Some("b".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let detail = parser.get_book_info(&source, "/book/regex").await.unwrap();

        assert_eq!(detail.name, "Regex Name");
        assert_eq!(detail.author, "Regex Author");
        mock.assert();
    }

    #[tokio::test]
    async fn test_get_chapters_multi_page_via_next_toc_url() {
        use httpmock::prelude::*;
        use crate::types::TocRule;

        let server = MockServer::start();
        let page1_mock = server.mock(|when, then| {
            when.method(GET).path("/toc/page1");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><ul class="chapters"><li><a href="/ch1">Ch1</a></li><li><a href="/ch2">Ch2</a></li></ul><a class="next" href="/toc/page2">Next</a></html>"#);
        });
        let page2_mock = server.mock(|when, then| {
            when.method(GET).path("/toc/page2");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><ul class="chapters"><li><a href="/ch3">Ch3</a></li><li><a href="/ch4">Ch4</a></li></ul></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_toc: Some(TocRule {
                chapter_list: Some("ul.chapters@li".into()),
                chapter_name: Some("a@text".into()),
                chapter_url: Some("a@href".into()),
                next_toc_url: Some("a.next@href".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let chapters = parser.get_chapters(&source, "/toc/page1").await;

        assert_eq!(chapters.len(), 4, "expected 4 chapters across 2 pages");
        assert_eq!(chapters[0].title, "Ch1");
        assert_eq!(chapters[0].url, server.url("/ch1"));
        assert_eq!(chapters[1].title, "Ch2");
        assert_eq!(chapters[1].url, server.url("/ch2"));
        assert_eq!(chapters[2].title, "Ch3");
        assert_eq!(chapters[2].url, server.url("/ch3"));
        assert_eq!(chapters[3].title, "Ch4");
        assert_eq!(chapters[3].url, server.url("/ch4"));
        page1_mock.assert();
        page2_mock.assert();
    }

    #[tokio::test]
    async fn test_content_pagination_multi_page() {
        use httpmock::prelude::*;
        use crate::types::ContentRule;

        let server = MockServer::start();
        let page1_mock = server.mock(|when, then| {
            when.method(GET).path("/ch/page1");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><div class="content">Page 1 content</div><a class="next-page" href="/ch/page2">Next</a></html>"#);
        });
        let page2_mock = server.mock(|when, then| {
            when.method(GET).path("/ch/page2");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><div class="content">Page 2 content</div></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_content: Some(ContentRule {
                content: Some("div.content@text".into()),
                next_content_url: Some("a.next-page@href".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let result = parser.get_chapter_content(&source, "/ch/page1").await;
        assert!(result.is_some(), "expected chapter content, got None");
        let chapter = result.unwrap();
        assert!(chapter.content.contains("Page 1 content"), "should contain page 1");
        assert!(chapter.content.contains("Page 2 content"), "should contain page 2");
        assert!(chapter.content.contains("\n"), "pages should be separated by newline");
        assert!(chapter.next_chapter_url.is_none(), "no next chapter when pagination ends");
        page1_mock.assert();
        page2_mock.assert();
    }

    #[tokio::test]
    async fn test_book_info_toc_url_template_resolution() {
        use httpmock::prelude::*;
        use crate::types::BookInfoRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/book/template");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><a class="toc-link" href="/read/list">目录</a><div class="title">Test Book</div><span class="author">Author Name</span></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            rule_book_info: Some(BookInfoRule {
                name: Some("@css:.title@text".into()),
                author: Some("@css:.author@text".into()),
                toc_url: Some("{{@css:a.toc-link@href}}".into()),
                ..Default::default()
            }),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let detail = parser.get_book_info(&source, "/book/template").await.unwrap();

        assert_eq!(detail.name, "Test Book");
        assert_eq!(detail.author, "Author Name");
        assert_eq!(detail.chapters_url, Some(server.url("/read/list")));
        mock.assert();
    }

    #[tokio::test]
    async fn test_explore_json_array_format() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/explore/json");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"[{"title":"Book 1","url":"/book/1","author":"A1"},{"title":"Book 2","url":"/book/2","cover":"/img/2.jpg"}]"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.explore(&source, "/explore/json", 1).await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Book 1");
        assert_eq!(results[0].book_url, server.url("/book/1"));
        assert_eq!(results[0].author, "A1");
        assert_eq!(results[1].name, "Book 2");
        assert_eq!(results[1].book_url, server.url("/book/2"));
        assert_eq!(results[1].cover_url, Some(server.url("/img/2.jpg")));
        mock.assert();
    }

    #[tokio::test]
    async fn test_explore_title_url_text_format() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/explore/text");
            then.status(200)
                .header("Content-Type", "text/plain")
                .body("Category A::/cat/a\nCategory B::/cat/b");
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: None,
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.explore(&source, "/explore/text", 1).await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Category A");
        assert_eq!(results[0].book_url, server.url("/cat/a"));
        assert_eq!(results[1].name, "Category B");
        assert_eq!(results[1].book_url, server.url("/cat/b"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_explore_with_rule_explore() {
        use httpmock::prelude::*;
        use crate::types::SearchRule;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/explore/rule");
            then.status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(r#"<html><ul><li class="item"><a href="/book/r1">Rule Book 1</a></li><li class="item"><a href="/book/r2">Rule Book 2</a></li></ul></html>"#);
        });

        let source = BookSource {
            id: "test".into(),
            name: "Test Source".into(),
            url: server.base_url(),
            source_type: 0,
            enabled: true,
            group_name: None,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            explore_url: None,
            rule_explore: Some(SearchRule {
                book_list: Some("li.item".into()),
                name: Some("a@text".into()),
                book_url: Some("a@href".into()),
                ..Default::default()
            }),
            book_url_pattern: None,
            enabled_explore: false,
            last_update_time: 0,
            book_source_comment: None,
            created_at: 0,
            updated_at: 0,
        };

        let parser = BookSourceParser::new();
        let results = parser.explore(&source, "/explore/rule", 1).await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Rule Book 1");
        assert_eq!(results[0].book_url, server.url("/book/r1"));
        assert_eq!(results[1].name, "Rule Book 2");
        assert_eq!(results[1].book_url, server.url("/book/r2"));
        mock.assert();
    }
}
