//! # 书源 DAO (Data Access Object)
//!
//! 提供书源相关的数据库操作。
//! 对应原 Legado 的 BookSource 实体操作 (data/entities/BookSource.kt)

use super::models::BookSource;
use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::Deserialize;
use tracing::{debug, info};
use uuid::Uuid;

/// 书源 DAO
pub struct SourceDao<'a> {
    conn: &'a mut Connection,
}

impl<'a> SourceDao<'a> {
    /// еИЫеїЇжЦ∞зЪД SourceDao
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// 插入或更新书源，返回实际写入的 ID（URL 去重时可能与 source.id 不同）
    pub fn upsert(&self, source: &BookSource) -> SqlResult<String> {
        debug!("插入/更新书源: {} ({})", source.name, source.url);

        // Handle URL uniqueness: if a different source already has this URL,
        // merge into that record to preserve book->source foreign keys.
        let effective_id: String = match self.conn.query_row(
            "SELECT id FROM book_sources WHERE url = ? AND id != ?",
            params![source.url, source.id],
            |row| row.get(0),
        ) {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => source.id.clone(),
            Err(e) => return Err(e),
        };

        self.conn.execute(
            "INSERT INTO book_sources (
                id, name, url, source_type, group_name, enabled, custom_order, weight,
                rule_search, rule_book_info, rule_toc, rule_content,
                login_url, header, js_lib, book_url_pattern,
                rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                url = excluded.url,
                source_type = excluded.source_type,
                group_name = excluded.group_name,
                enabled = excluded.enabled,
                custom_order = excluded.custom_order,
                weight = excluded.weight,
                rule_search = excluded.rule_search,
                rule_book_info = excluded.rule_book_info,
                rule_toc = excluded.rule_toc,
                rule_content = excluded.rule_content,
                login_url = excluded.login_url,
                header = excluded.header,
                js_lib = excluded.js_lib,
                book_url_pattern = excluded.book_url_pattern,
                rule_explore = excluded.rule_explore,
                explore_url = excluded.explore_url,
                enabled_explore = excluded.enabled_explore,
                last_update_time = excluded.last_update_time,
                book_source_comment = excluded.book_source_comment,
                updated_at = excluded.updated_at",
            params![
                effective_id,
                source.name,
                source.url,
                source.source_type,
                source.group_name,
                source.enabled as i32,
                source.custom_order,
                source.weight,
                source.rule_search,
                source.rule_book_info,
                source.rule_toc,
                source.rule_content,
                source.login_url,
                source.header,
                source.js_lib,
                source.book_url_pattern,
                source.rule_explore,
                source.explore_url,
                source.enabled_explore as i32,
                source.last_update_time,
                source.book_source_comment,
                source.created_at,
                source.updated_at,
            ],
        )?;

        Ok(effective_id)
    }

    /// 根据 ID 获取书源
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<BookSource>> {
        let mut stmt = self.conn.prepare(
             "SELECT id, name, url, source_type, group_name, enabled, custom_order, weight,
                     rule_search, rule_book_info, rule_toc, rule_content,
                     login_url, header, js_lib, book_url_pattern,
                     rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                     created_at, updated_at
              FROM book_sources WHERE id = ?"
        )?;

        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(book_source_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 获取所有启用的书源
    pub fn get_enabled(&self) -> SqlResult<Vec<BookSource>> {
        let mut stmt = self.conn.prepare(
             "SELECT id, name, url, source_type, group_name, enabled, custom_order, weight,
                     rule_search, rule_book_info, rule_toc, rule_content,
                     login_url, header, js_lib, book_url_pattern,
                     rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                     created_at, updated_at
              FROM book_sources WHERE enabled = 1 ORDER BY custom_order ASC, weight DESC"
        )?;

        let rows = stmt.query_map([], book_source_from_row)?;
        rows.collect()
    }

    /// 获取所有书源
    pub fn get_all(&self) -> SqlResult<Vec<BookSource>> {
        let mut stmt = self.conn.prepare(
             "SELECT id, name, url, source_type, group_name, enabled, custom_order, weight,
                     rule_search, rule_book_info, rule_toc, rule_content,
                     login_url, header, js_lib, book_url_pattern,
                     rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                     created_at, updated_at
              FROM book_sources ORDER BY custom_order ASC, weight DESC"
        )?;

        let rows = stmt.query_map([], book_source_from_row)?;
        rows.collect()
    }

    /// 根据 URL 搜索书源
    pub fn get_by_url(&self, url: &str) -> SqlResult<Option<BookSource>> {
        let mut stmt = self.conn.prepare(
             "SELECT id, name, url, source_type, group_name, enabled, custom_order, weight,
                     rule_search, rule_book_info, rule_toc, rule_content,
                     login_url, header, js_lib, book_url_pattern,
                     rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                     created_at, updated_at
              FROM book_sources WHERE url = ?"
        )?;

        let mut rows = stmt.query(params![url])?;

        if let Some(row) = rows.next()? {
            Ok(Some(book_source_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 删除书源
    pub fn delete(&self, id: &str) -> SqlResult<()> {
        info!("删除书源: {}", id);
        self.conn
            .execute("DELETE FROM book_sources WHERE id = ?", params![id])?;
        Ok(())
    }

    /// 批量删除书源
    pub fn delete_batch(&self, ids: &[String]) -> SqlResult<()> {
        info!("批量删除 {} 个书源", ids.len());
        for id in ids {
            self.conn
                .execute("DELETE FROM book_sources WHERE id = ?", params![id])?;
        }
        Ok(())
    }

    /// 启用/禁用书源
    pub fn set_enabled(&self, id: &str, enabled: bool) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE book_sources SET enabled = ?, updated_at = ? WHERE id = ?",
            params![enabled as i32, Utc::now().timestamp(), id],
        )?;
        Ok(())
    }

    /// 更新书源排序权重
    pub fn update_order(&self, id: &str, custom_order: i32) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE book_sources SET custom_order = ?, updated_at = ? WHERE id = ?",
            params![custom_order, Utc::now().timestamp(), id],
        )?;
        Ok(())
    }

    /// 批量导入书源
    pub fn batch_insert(&mut self, sources: &[BookSource]) -> SqlResult<()> {
        info!("批量导入 {} 个书源", sources.len());

        let tx = self.conn.transaction()?;

        for source in sources {
            // Handle URL uniqueness: if a different source already has this URL,
            // merge into that record to preserve book->source foreign keys.
            let effective_id: String = match tx.query_row(
                "SELECT id FROM book_sources WHERE url = ? AND id != ?",
                params![source.url, source.id],
                |row| row.get(0),
            ) {
                Ok(id) => id,
                Err(rusqlite::Error::QueryReturnedNoRows) => source.id.clone(),
                Err(e) => return Err(e),
            };

            tx.execute(
                "INSERT INTO book_sources (
                    id, name, url, source_type, group_name, enabled, custom_order, weight,
                    rule_search, rule_book_info, rule_toc, rule_content,
                    login_url, header, js_lib, book_url_pattern,
                    rule_explore, explore_url, enabled_explore, last_update_time, book_source_comment,
                    created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    url = excluded.url,
                    source_type = excluded.source_type,
                    group_name = excluded.group_name,
                    enabled = excluded.enabled,
                    custom_order = excluded.custom_order,
                    weight = excluded.weight,
                    rule_search = excluded.rule_search,
                    rule_book_info = excluded.rule_book_info,
                    rule_toc = excluded.rule_toc,
                    rule_content = excluded.rule_content,
                    login_url = excluded.login_url,
                    header = excluded.header,
                    js_lib = excluded.js_lib,
                    book_url_pattern = excluded.book_url_pattern,
                    rule_explore = excluded.rule_explore,
                    explore_url = excluded.explore_url,
                    enabled_explore = excluded.enabled_explore,
                    last_update_time = excluded.last_update_time,
                    book_source_comment = excluded.book_source_comment,
                    updated_at = excluded.updated_at",
                params![
                    effective_id,
                    source.name,
                    source.url,
                    source.source_type,
                    source.group_name,
                    source.enabled as i32,
                    source.custom_order,
                    source.weight,
                    source.rule_search,
                    source.rule_book_info,
                    source.rule_toc,
                    source.rule_content,
                    source.login_url,
                    source.header,
                    source.js_lib,
                    source.book_url_pattern,
                    source.rule_explore,
                    source.explore_url,
                    source.enabled_explore as i32,
                    source.last_update_time,
                    source.book_source_comment,
                    source.created_at,
                    source.updated_at,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// 从 JSON 字符串导入书源
    /// 支持两种格式:
    /// 1. 内部存储格式 (Vec<BookSource>)
    /// 2. Legado 书源导出格式 (字段名 camelCase, 规则为嵌套对象)
    pub fn import_from_json(&mut self, json: &str) -> Result<usize, String> {
        // Try internal storage format first
        if let Ok(sources) = serde_json::from_str::<Vec<BookSource>>(json) {
            let count = sources.len();
            self.batch_insert(&sources)
                .map_err(|e| format!("批量插入书源失败: {}", e))?;
            return Ok(count);
        }

        // Try real-world Legado export format
        let legado_sources: Vec<LegadoBookSource> =
            serde_json::from_str(json).map_err(|e| format!("解析Legado书源JSON失败: {}", e))?;

        let mut sources = Vec::with_capacity(legado_sources.len());
        for s in &legado_sources {
            sources.push(legado_to_storage(s)?);
        }

        let count = sources.len();
        self.batch_insert(&sources)
            .map_err(|e| format!("批量插入书源失败: {}", e))?;
        Ok(count)
    }

    /// 创建新书源（便捷函数）
    pub fn create(&self, name: &str, url: &str) -> SqlResult<BookSource> {
        let now = Utc::now().timestamp();
        let source = BookSource {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            url: url.to_string(),
            source_type: 0,
            group_name: None,
            enabled: true,
            custom_order: 0,
            weight: 0,
            rule_search: None,
            rule_book_info: None,
            rule_toc: None,
            rule_content: None,
            login_url: None,
            header: None,
            js_lib: None,
            book_url_pattern: None,
            rule_explore: None,
            explore_url: None,
            enabled_explore: true,
            last_update_time: 0,
            book_source_comment: None,
            created_at: now,
            updated_at: now,
        };

        let effective_id = self.upsert(&source)?;
        if effective_id == source.id {
            Ok(source)
        } else {
            self.get_by_id(&effective_id)
                .map(|opt| opt.expect("source not found after dedup upsert"))
        }
    }
}

/// 从数据库行转换到 BookSource 结构体
fn book_source_from_row(row: &rusqlite::Row) -> SqlResult<BookSource> {
    Ok(BookSource {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        source_type: row.get(3)?,
        group_name: row.get(4)?,
        enabled: row.get::<_, i32>(5)? != 0,
        custom_order: row.get(6)?,
        weight: row.get(7)?,
        rule_search: row.get(8)?,
        rule_book_info: row.get(9)?,
        rule_toc: row.get(10)?,
        rule_content: row.get(11)?,
        login_url: row.get(12)?,
        header: row.get(13)?,
        js_lib: row.get(14)?,
        book_url_pattern: row.get(15)?,
        rule_explore: row.get(16)?,
        explore_url: row.get(17)?,
        enabled_explore: row.get::<_, i32>(18)? != 0,
        last_update_time: row.get(19)?,
        book_source_comment: row.get(20)?,
        created_at: row.get(21)?,
        updated_at: row.get(22)?,
    })
}

/// Real-world Legado book source JSON export format
#[derive(Debug, Clone, Deserialize)]
struct LegadoBookSource {
    #[serde(rename = "bookSourceUrl")]
    url: String,
    #[serde(rename = "bookSourceName")]
    name: String,
    #[serde(rename = "bookSourceGroup", default)]
    group_name: String,
    #[serde(rename = "bookSourceType", default)]
    source_type: i32,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    weight: i32,
    #[serde(rename = "customOrder", default)]
    custom_order: i32,
    #[serde(default, deserialize_with = "deser_flexible_header")]
    header: Option<String>,
    #[serde(rename = "loginUrl", default)]
    login_url: Option<String>,
    #[serde(rename = "jsLib", default)]
    js_lib: Option<String>,
    #[serde(rename = "searchUrl", default)]
    search_url: Option<String>,
    #[serde(rename = "ruleSearch", default)]
    rule_search: Option<serde_json::Value>,
    #[serde(rename = "ruleBookInfo", default)]
    rule_book_info: Option<serde_json::Value>,
    #[serde(rename = "ruleToc", default)]
    rule_toc: Option<serde_json::Value>,
    #[serde(rename = "ruleContent", default)]
    rule_content: Option<serde_json::Value>,
    #[serde(rename = "ruleExplore", default)]
    rule_explore: Option<serde_json::Value>,
    #[serde(rename = "exploreUrl", default)]
    explore_url: Option<String>,
    #[serde(rename = "bookUrlPattern", default)]
    book_url_pattern: Option<String>,
    #[serde(rename = "enabledExplore", default = "default_true")]
    enabled_explore: bool,
    #[serde(
        rename = "lastUpdateTime",
        default,
        deserialize_with = "deser_flexible_i64"
    )]
    last_update_time: i64,
    #[serde(rename = "bookSourceComment", default)]
    book_source_comment: Option<String>,
}

fn deser_flexible_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = i64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or string containing a number")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            Ok(v as i64)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
            Ok(v as i64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.parse::<i64>()
                .map_err(|_| de::Error::custom("invalid number string"))
        }
    }
    deserializer.deserialize_any(Visitor)
}

fn deser_flexible_header<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Option<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string, object, array, or null")
        }
        fn visit_none<E: de::Error>(self) -> Result<Option<String>, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Option<String>, E> {
            Ok(None)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<String>, E> {
            let s = v.to_string();
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s))
            }
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Option<String>, E> {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            let mut s = String::new();
            while let Some((k, v)) = map.next_entry::<String, serde_json::Value>()? {
                use std::fmt::Write;
                if !s.is_empty() {
                    s.push_str(", ");
                }
                let _ = write!(s, "\"{}\":{}", k, v);
            }
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s))
            }
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut s = String::new();
            while let Some(v) = seq.next_element::<serde_json::Value>()? {
                if !s.is_empty() {
                    s.push_str(", ");
                }
                use std::fmt::Write;
                let _ = write!(s, "{}", v);
            }
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s))
            }
        }
    }
    deserializer.deserialize_any(Visitor)
}

fn default_true() -> bool {
    true
}

fn legado_to_storage(source: &LegadoBookSource) -> Result<BookSource, String> {
    let now = Utc::now().timestamp();
    let rule_search = normalize_rule_value(merge_search_url(
        source.rule_search.clone(),
        source.search_url.as_deref(),
    ));
    Ok(BookSource {
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
        rule_search,
        rule_book_info: normalize_rule_value(source.rule_book_info.clone()),
        rule_toc: normalize_rule_value(source.rule_toc.clone()),
        rule_content: normalize_rule_value(source.rule_content.clone()),
        login_url: source.login_url.clone(),
        header: source.header.clone(),
        js_lib: source.js_lib.clone(),
        rule_explore: normalize_rule_value(source.rule_explore.clone()),
        explore_url: source.explore_url.clone(),
        book_url_pattern: source.book_url_pattern.clone(),
        enabled_explore: source.enabled_explore,
        last_update_time: source.last_update_time,
        book_source_comment: source.book_source_comment.clone(),
        created_at: now,
        updated_at: now,
    })
}

fn normalize_rule_value(value: Option<serde_json::Value>) -> Option<String> {
    match value {
        None => None,
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(ref arr)) if arr.is_empty() => None,
        Some(serde_json::Value::Object(ref obj)) if obj.is_empty() => None,
        Some(v) => Some(normalize_rule_keys(v).to_string()),
    }
}

fn normalize_rule_keys(mut value: serde_json::Value) -> serde_json::Value {
    let Some(obj) = value.as_object_mut() else {
        return value;
    };

    for (from, to) in [
        ("bookList", "book_list"),
        ("bookUrl", "book_url"),
        ("coverUrl", "cover_url"),
        ("lastChapter", "last_chapter"),
        ("chapterList", "chapter_list"),
        ("chapterName", "chapter_name"),
        ("chapterUrl", "chapter_url"),
        ("nextContentUrl", "next_content_url"),
        ("searchUrl", "search_url"),
        ("wordCount", "word_count"),
        ("bookInfoInit", "book_info_init"),
        ("tocUrl", "toc_url"),
        ("canReName", "can_rename"),
        ("nextTocUrl", "next_toc_url"),
        ("updateTime", "update_time"),
        ("webJs", "web_js"),
        ("sourceRegex", "source_regex"),
        ("isVip", "is_vip"),
    ] {
        if !obj.contains_key(to) {
            if let Some(v) = obj.remove(from) {
                obj.insert(to.to_string(), v);
            }
        }
    }
    for value in obj.values_mut() {
        if let Some(rule) = value.as_str() {
            *value = serde_json::Value::String(normalize_legado_rule(rule));
        }
    }
    value
}

fn normalize_legado_rule(rule: &str) -> String {
    if rule.trim_start().starts_with("@js:") || rule.trim_start().starts_with("js:") {
        return rule.to_string();
    }

    let (expr, suffix) = split_rule_suffix(rule);
    let normalized = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() && !suffix.is_empty() {
        return suffix.to_string();
    }
    format!("{}{}", normalized, suffix)
}

fn split_rule_suffix(rule: &str) -> (&str, &str) {
    for suffix in [
        "@textNodes",
        "@textNode",
        "@ownText",
        "@content",
        "@text",
        "@html",
        "@href",
        "@src",
    ] {
        if let Some(expr) = rule.strip_suffix(suffix) {
            return (expr, suffix);
        }
    }
    (rule, "")
}

fn merge_search_url(
    rule_search: Option<serde_json::Value>,
    search_url: Option<&str>,
) -> Option<serde_json::Value> {
    let mut value = rule_search.unwrap_or_else(|| serde_json::json!({}));
    if let (Some(url), Some(obj)) = (search_url, value.as_object_mut()) {
        obj.entry("search_url".to_string())
            .or_insert_with(|| serde_json::Value::String(clean_legado_url(url)));
    }
    Some(value)
}

fn clean_legado_url(url: &str) -> String {
    url.trim().to_string()
}

impl<'a> SourceDao<'a> {
    pub fn export_legado_json(&self) -> SqlResult<String> {
        let sources = self.get_all()?;
        let legado_sources: Vec<serde_json::Value> = sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "bookSourceUrl": s.url,
                    "bookSourceName": s.name,
                    "bookSourceGroup": s.group_name.as_deref().unwrap_or(""),
                    "bookSourceType": s.source_type,
                    "bookSourceComment": s.book_source_comment.as_deref().unwrap_or(""),
                    "bookUrlPattern": s.book_url_pattern.as_deref().unwrap_or(""),
                    "customOrder": s.custom_order,
                    "enabled": s.enabled,
                    "enabledExplore": s.enabled_explore,
                    "weight": s.weight,
                    "lastUpdateTime": s.last_update_time,
                    "header": s.header.as_deref().unwrap_or(""),
                    "loginUrl": s.login_url.as_deref().unwrap_or(""),
                    "jsLib": s.js_lib.as_deref().unwrap_or(""),
                    "searchUrl": extract_search_url(&s.rule_search),
                    "exploreUrl": s.explore_url.as_deref().unwrap_or(""),
                    "ruleSearch": parse_rule_json(&s.rule_search),
                    "ruleBookInfo": parse_rule_json(&s.rule_book_info),
                    "ruleToc": parse_rule_json(&s.rule_toc),
                    "ruleContent": parse_rule_json(&s.rule_content),
                    "ruleExplore": parse_rule_json(&s.rule_explore),
                })
            })
            .collect();
        serde_json::to_string_pretty(&legado_sources)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }
}

fn extract_search_url(rule_json: &Option<String>) -> String {
    if let Some(json_str) = rule_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            return val
                .get("search_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
    }
    String::new()
}

fn denormalize_rule_keys(mut value: serde_json::Value) -> serde_json::Value {
    let Some(obj) = value.as_object_mut() else {
        return value;
    };
    let renames: Vec<(&str, &str)> = vec![
        ("book_list", "bookList"),
        ("book_url", "bookUrl"),
        ("cover_url", "coverUrl"),
        ("last_chapter", "lastChapter"),
        ("chapter_list", "chapterList"),
        ("chapter_name", "chapterName"),
        ("chapter_url", "chapterUrl"),
        ("next_content_url", "nextContentUrl"),
        ("search_url", "searchUrl"),
        ("word_count", "wordCount"),
        ("book_info_init", "bookInfoInit"),
        ("toc_url", "tocUrl"),
        ("can_rename", "canReName"),
        ("next_toc_url", "nextTocUrl"),
        ("update_time", "updateTime"),
        ("web_js", "webJs"),
        ("source_regex", "sourceRegex"),
        ("is_vip", "isVip"),
    ];
    for (from, to) in renames {
        if let Some(v) = obj.remove(from) {
            obj.insert(to.to_string(), v);
        }
    }
    value
}

fn parse_rule_json(rule_json: &Option<String>) -> serde_json::Value {
    if let Some(json_str) = rule_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            return denormalize_rule_keys(val);
        }
    }
    serde_json::Value::Null
}
