//! # 数据模型定义
//!
//! 定义 core-storage 使用的核心数据结构。
//! 对应原 Legado 的 data/entities/ 中的数据实体。

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// 书源结构体（对应原 Legado 的 BookSource 实体）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub source_type: i32, // 0=小说, 1=音频, 2=图片, 3=RSS
    pub group_name: Option<String>,
    pub enabled: bool,
    pub custom_order: i32,
    pub weight: i32,

    // 规则（JSON 格式存储）
    pub rule_search: Option<String>,
    pub rule_book_info: Option<String>,
    pub rule_toc: Option<String>,
    pub rule_content: Option<String>,

    // 其他配置
    pub login_url: Option<String>,
    pub header: Option<String>,
    pub js_lib: Option<String>,
    pub book_url_pattern: Option<String>,
    #[serde(default)]
    pub rule_explore: Option<String>,
    #[serde(default)]
    pub explore_url: Option<String>,
    #[serde(default)]
    pub enabled_explore: bool,
    #[serde(default)]
    pub last_update_time: i64,
    #[serde(default)]
    pub book_source_comment: Option<String>,

    pub created_at: i64,
    pub updated_at: i64,
}

/// 书籍结构体（对应原 Legado 的 Book 实体）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub source_id: String,
    pub source_name: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub chapter_count: i32,
    pub latest_chapter_title: Option<String>,
    pub intro: Option<String>,
    pub kind: Option<String>,
    pub book_url: Option<String>,
    pub toc_url: Option<String>,
    pub last_check_time: Option<i64>,
    pub last_check_count: i32,
    pub total_word_count: i32,
    pub can_update: bool,
    pub order_time: i64,
    pub latest_chapter_time: Option<i64>,
    pub custom_cover_path: Option<String>,
    pub custom_info_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 章节结构体（对应原 Legado 的 Chapter 实体）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub book_id: String,
    pub index_num: i32,
    pub title: String,
    pub url: String,
    pub content: Option<String>,
    pub is_volume: bool,
    pub is_checked: bool,
    pub start: i32,
    pub end: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 阅读进度结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookProgress {
    pub book_id: String,
    pub chapter_index: i32,
    pub paragraph_index: i32,
    pub offset: i32,
    pub read_time: i64, // 累计阅读时长（毫秒）
    pub updated_at: i64,
}

/// 书签结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub book_id: String,
    pub chapter_index: i32,
    pub paragraph_index: i32,
    pub content: Option<String>,
    pub created_at: i64,
}

/// 替换规则结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
    pub scope: i32, // 0=全局, 1=书源, 2=书籍
    pub sort_number: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 下载任务结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub book_id: String,
    pub book_name: String,
    pub cover_url: Option<String>,
    pub total_chapters: i32,
    pub downloaded_chapters: i32,
    pub status: i32, // 0=等待, 1=下载中, 2=暂停, 3=完成, 4=失败
    pub total_size: i64,
    pub downloaded_size: i64,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 下载章节记录结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadChapter {
    pub id: String,
    pub task_id: String,
    pub chapter_id: String,
    pub chapter_index: i32,
    pub chapter_title: String,
    pub status: i32, // 0=等待, 1=下载中, 2=完成, 3=失败
    pub file_path: Option<String>,
    pub file_size: i64,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 生成新的 UUID
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 获取当前时间戳（秒）
pub fn now_timestamp() -> i64 {
    Utc::now().timestamp()
}
