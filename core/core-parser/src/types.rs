//! # 通用类型定义
//!
//! 提供 core-parser 模块使用的通用数据结构。

use serde::{Deserialize, Serialize};

/// 章节信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub content: String,
    pub index: usize,
    pub href: Option<String>,  // EPUB 中的文件路径
}

/// 书籍元数据（EPUB 使用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub identifier: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub cover: Option<String>,
    pub date: Option<String>,
    pub rights: Option<String>,
    pub subjects: Vec<String>,
}

/// EPUB 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubData {
    pub metadata: BookMetadata,
    pub chapters: Vec<Chapter>,
}
