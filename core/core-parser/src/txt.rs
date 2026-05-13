//! # TXT 文件解析模块
//!
//! 负责解析纯文本格式的书籍文件，支持自动编码检测。
//! 对应原 Legado 的 TXT 解析功能 (modules/book/TxtChapterRule.kt)。

use std::fs;
use std::path::Path;
use encoding_rs::{UTF_8, GB18030, Encoding};
use tracing::{debug, info, warn};
use regex::{Regex, RegexBuilder};
use crate::Chapter;
use crate::cleaner::apply_replace_rules;

/// TXT 解析器配置
#[derive(Debug, Clone)]
pub struct TxtParserConfig {
    /// 章节标题匹配正则（用于自动分章）
    pub chapter_regex: Option<String>,
    /// 是否移除空行
    pub remove_empty_lines: bool,
    /// 内容清洗规则
    pub clean_rules: Vec<String>,
    /// 内容替换规则 (模式, 替换文本)
    pub replace_rules: Vec<(String, String)>,
}

impl Default for TxtParserConfig {
    fn default() -> Self {
        // 注意：regex crate 不支持 (?i) 内联标志，使用 (?i:...) 语法
        let chapter_regex = r"^第?[一二三四五六七八九十百千万\d]+[章回节卷集].*$|^Chapter\s+\d+.*$";
        Self {
            chapter_regex: Some(chapter_regex.to_string()),
            remove_empty_lines: true,
            clean_rules: vec![
                r"<[^>]+>".to_string(),  // 移除 HTML 标签
                r"&nbsp;|&lt;|&gt;|&quot;|&amp;".to_string(),  // 转义字符
            ],
            replace_rules: vec![],
        }
    }
}

/// TXT 文件解析器
pub struct TxtParser {
    config: TxtParserConfig,
}

impl TxtParser {
    /// 创建新的 TXT 解析器
    pub fn new(config: TxtParserConfig) -> Self {
        Self { config }
    }

    /// 解析 TXT 文件
    /// 返回章节列表
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<Chapter>, String> {
        let path = path.as_ref();
        info!("开始解析 TXT 文件: {:?}", path);
        
        // 读取文件内容并检测编码
        let (content, encoding) = self.read_file_with_encoding(path)?;
        debug!("检测到编码: {}", encoding);
        
        // 解析章节
        self.parse_content(&content)
    }

    /// 读取文件并自动检测编码
    fn read_file_with_encoding(&self, path: &Path) -> Result<(String, String), String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("读取文件失败: {}", e))?;
        
        // 使用 core-net 的编码检测函数
        let (text, encoding) = detect_encoding_fallback(&bytes);
        
        Ok((text, encoding.name().to_string()))
    }

    /// 解析文本内容，分割成章节
    fn parse_content(&self, content: &str) -> Result<Vec<Chapter>, String> {
        let mut chapters: Vec<Chapter> = Vec::new();
        
        // 如果有章节正则，按章节分割
        if let Some(regex_str) = &self.config.chapter_regex {
            match RegexBuilder::new(regex_str).multi_line(true).case_insensitive(true).build() {
            Ok(re) => {
                    let mut last_end = 0;
                    let mut chapter_index = 0;
                    let mut current_title: Option<String> = None;

                    for mat in re.find_iter(content) {
                        if mat.start() > last_end {
                            let chapter_content = &content[last_end..mat.start()].trim();
                            if !chapter_content.is_empty() {
                                let title = current_title.take().unwrap_or_else(|| "正文".to_string());
                                chapters.push(Chapter {
                                    title,
                                    content: self.clean_content(chapter_content),
                                    index: chapter_index,
                                    href: None,
                                });
                                chapter_index += 1;
                            }
                        }
                        current_title = Some(mat.as_str().trim().to_string());
                        last_end = mat.end();
                    }

                    // 处理最后一章
                    if last_end < content.len() {
                        let remaining = content[last_end..].trim();
                        if !remaining.is_empty() {
                            let title = current_title.take().unwrap_or_else(|| {
                                if chapter_index == 0 { "正文".to_string() } else { "最后一章".to_string() }
                            });
                            chapters.push(Chapter {
                                title,
                                content: self.clean_content(remaining),
                                index: chapter_index,
                                href: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    warn!("章节正则编译失败: {}, 将整个文件作为一章", e);
                    chapters.push(Chapter {
                        title: "正文".to_string(),
                        content: self.clean_content(content),
                        index: 0,
                        href: None,
                    });
                }
            }
        } else {
            // 无章节正则，整个文件作为一章
            chapters.push(Chapter {
                title: "正文".to_string(),
                content: self.clean_content(content),
                index: 0,
                href: None,
            });
        }
        
        info!("TXT 解析完成，共 {} 章", chapters.len());
        Ok(chapters)
    }

    /// 清洗内容（移除多余空白、HTML 标签等）
    fn clean_content(&self, content: &str) -> String {
        let mut text = content.to_string();
        
        // 应用清洗规则
        for rule in &self.config.clean_rules {
            if let Ok(re) = Regex::new(rule) {
                text = re.replace_all(&text, "").to_string();
            }
        }

        // 应用替换规则
        if !self.config.replace_rules.is_empty() {
            text = apply_replace_rules(&text, &self.config.replace_rules);
        }

        // 移除多余空行
        if self.config.remove_empty_lines {
            let re = Regex::new(r"\n\s*\n\s*\n").unwrap();
            text = re.replace_all(&text, "\n\n").to_string();
        }
        
        text.trim().to_string()
    }
}

/// 便捷函数：快速解析 TXT 文件
pub fn parse_txt_file<P: AsRef<Path>>(path: P) -> Result<Vec<Chapter>, String> {
    let parser = TxtParser::new(TxtParserConfig::default());
    parser.parse_file(path)
}

/// 备用编码检测函数（如果 core-net 不可用）
fn detect_encoding_fallback(bytes: &[u8]) -> (String, &'static Encoding) {
    // 1. BOM 检测
    if bytes.len() >= 3 && bytes[0..3] == [0xEF, 0xBB, 0xBF] {
        let (text, _, _) = UTF_8.decode(&bytes[3..]);
        return (text.into_owned(), UTF_8);
    }
    
    // 2. UTF-8 有效性检查（简化），非 UTF-8 时默认中文编码
    let mut utf8_valid = true;

    for window in bytes.windows(2) {
        if window[0] & 0x80 != 0
            && window[0] & 0xE0 == 0xC0
                && window.len() > 1 && window[1] & 0xC0 != 0x80 {
                    utf8_valid = false;
                }
    }
    
    let encoding = if utf8_valid {
        UTF_8
    } else {
        GB18030 // 非 UTF-8 时默认中文编码
    };
    
    let (text, _, _) = encoding.decode(bytes);
    (text.into_owned(), encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_txt_utf8_chapter_parsing() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_txt_utf8.txt");
        let content = "前言\n这是前言内容。\n第一章 开始\n这是第一章的内容。\n第二章 继续\n这是第二章的内容。\n";
        std::fs::write(&path, content).unwrap();
        let parser = TxtParser::new(TxtParserConfig::default());
        let chapters = parser.parse_file(&path).unwrap();
        assert!(chapters.len() >= 3, "got {} chapters", chapters.len());
        assert_eq!(chapters[0].title, "正文");
        assert_eq!(chapters[1].title, "第一章 开始");
        assert!(chapters[1].content.contains("第一章的内容"));
        assert_eq!(chapters[2].title, "第二章 继续");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_txt_gb18030_parsing() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_txt_gb18030.txt");
        let text = "第一章 测试\n这是第一章的内容。\n第二章 继续\n这是第二章的内容。\n";
        let (encoded, _, _) = encoding_rs::GB18030.encode(text);
        std::fs::write(&path, encoded).unwrap();
        let parser = TxtParser::new(TxtParserConfig::default());
        let chapters = parser.parse_file(&path).unwrap();
        assert_eq!(chapters.len(), 2);
        assert!(chapters[0].title.contains("第一章"));
        assert!(chapters[1].title.contains("第二章"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_txt_no_chapter_titles() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_txt_nochap.txt");
        std::fs::write(&path, "Plain text without any chapter markers.\nJust some content.\n").unwrap();
        let parser = TxtParser::new(TxtParserConfig::default());
        let chapters = parser.parse_file(&path).unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "正文");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_txt_replace_rules_applied() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_txt_replace.txt");
        std::fs::write(&path, "第一章 测试\n内容包含广告词请支持正版。\n").unwrap();
        let config = TxtParserConfig {
            replace_rules: vec![("请支持正版".to_string(), "[已移除]".to_string())],
            ..Default::default()
        };
        let parser = TxtParser::new(config);
        let chapters = parser.parse_file(&path).unwrap();
        assert_eq!(chapters.len(), 1);
        assert!(!chapters[0].content.contains("请支持正版"));
        assert!(chapters[0].content.contains("[已移除]"));
        std::fs::remove_file(&path).ok();
    }
}
