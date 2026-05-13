//! # 内容清洗模块
//!
//! 提供通用的内容清洗功能（移除广告、格式化文本等）。
//! 对应原 Legado 的内容清洗规则 (modules/book/BookContent.kt)

use regex::Regex;

/// 内容清洗器配置
#[derive(Debug, Clone)]
pub struct CleanerConfig {
    /// 要移除的正则规则列表
    pub remove_rules: Vec<String>,
    /// 要替换的规则列表 (查找, 替换)
    pub replace_rules: Vec<(String, String)>,
    /// 是否移除 HTML 标签
    pub remove_html_tags: bool,
    /// 是否解码 HTML 实体
    pub decode_html_entities: bool,
    /// 是否移除空行
    pub remove_empty_lines: bool,
    /// 是否合并多余空格
    pub collapse_whitespace: bool,
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            remove_rules: vec![
                // 常见广告文本
                r"本书首发于.*?请记住：.*?$".to_string(),
                r"请支持正版.*?$".to_string(),
                r"天才一秒记住.*?$".to_string(),
                r"最新章节请关注.*?$".to_string(),
                // 注释和脚本
                r"<!--.*?-->".to_string(),
                r"<script[^>]*>.*?</script>".to_string(),
                r"<style[^>]*>.*?</style>".to_string(),
            ],
            replace_rules: vec![
                // HTML 实体
                ("&nbsp;".to_string(), " ".to_string()),
                ("&lt;".to_string(), "<".to_string()),
                ("&gt;".to_string(), ">".to_string()),
                ("&quot;".to_string(), "\"".to_string()),
                ("&amp;".to_string(), "&".to_string()),
                ("&mdash;".to_string(), "——".to_string()),
                ("&hellip;".to_string(), "…".to_string()),
            ],
            remove_html_tags: true,
            decode_html_entities: true,
            remove_empty_lines: true,
            collapse_whitespace: true,
        }
    }
}

/// 内容清洗器
pub struct ContentCleaner {
    config: CleanerConfig,
    compiled_remove: Vec<Regex>,
}

impl ContentCleaner {
    /// 创建新的内容清洗器
    pub fn new(config: CleanerConfig) -> Result<Self, String> {
        // 预编译移除规则
        let mut compiled_remove = Vec::new();
        for rule in &config.remove_rules {
            match Regex::new(rule) {
                Ok(re) => compiled_remove.push(re),
                Err(e) => return Err(format!("编译规则失败 '{}': {}", rule, e)),
            }
        }
        
        Ok(Self {
            config,
            compiled_remove,
        })
    }

    /// 清洗内容
    pub fn clean(&self, content: &str) -> String {
        let mut text = content.to_string();
        
        // 1. 移除 HTML 标签
        if self.config.remove_html_tags {
            let re = Regex::new(r"<[^>]+>").unwrap();
            text = re.replace_all(&text, "").to_string();
        }
        
        // 2. 解码 HTML 实体
        if self.config.decode_html_entities {
            text = self.decode_html_entities(&text);
        }
        
        // 3. 应用移除规则
        for re in &self.compiled_remove {
            text = re.replace_all(&text, "").to_string();
        }
        
        // 4. 应用替换规则
        for (find, replace) in &self.config.replace_rules {
            text = text.replace(find, replace);
        }
        
        // 5. 移除空行
        if self.config.remove_empty_lines {
            let re = Regex::new(r"\n\s*\n\s*\n").unwrap();
            text = re.replace_all(&text, "\n\n").to_string();
        }
        
        // 6. 合并多余空格
        if self.config.collapse_whitespace {
            let re = Regex::new(r"[ \t]+").unwrap();
            text = re.replace_all(&text, " ").to_string();
        }
        
        text.trim().to_string()
    }

    /// 解码 HTML 实体
    fn decode_html_entities(&self, text: &str) -> String {
        let mut result = text.to_string();
        
        // 常见 HTML 实体
        let entities = [
            ("&nbsp;", " "),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&amp;", "&"),
            ("&mdash;", "——"),
            ("&ndash;", "–"),
            ("&hellip;", "…"),
            ("&lsquo;", "'"),
            ("&rsquo;", "'"),
            ("&ldquo;", "\""),
            ("&rdquo;", "\""),
        ];
        
        for (entity, replacement) in &entities {
            result = result.replace(entity, replacement);
        }
        
        result
    }

    /// 批量清洗章节内容
    pub fn clean_chapters(&self, chapters: Vec<super::types::Chapter>) -> Vec<super::types::Chapter> {
        chapters
            .into_iter()
            .map(|mut chapter| {
                chapter.content = self.clean(&chapter.content);
                chapter
            })
            .collect()
    }
}

impl Default for ContentCleaner {
    fn default() -> Self {
        Self::new(CleanerConfig::default()).expect("创建默认清洗器失败")
    }
}

/// 便捷函数：快速清洗内容
pub fn clean_text(content: &str) -> String {
    let cleaner = ContentCleaner::default();
    cleaner.clean(content)
}

/// 根据书源配置的替换规则清洗内容
/// 对应原 Legado 的 replaceRule 功能
pub fn apply_replace_rules(content: &str, rules: &[(String, String)]) -> String {
    let mut text = content.to_string();
    
    for (pattern, replacement) in rules {
        // 尝试作为正则替换
        if let Ok(re) = Regex::new(pattern) {
            text = re.replace_all(&text, replacement).to_string();
        } else {
            // 否则作为普通文本替换
            text = text.replace(pattern, replacement);
        }
    }
    
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html() {
        let cleaner = ContentCleaner::default();
        let html = "<p>Hello <b>World</b></p><!-- comment -->";
        let cleaned = cleaner.clean(html);
        assert_eq!(cleaned, "Hello World");
    }

    #[test]
    fn test_decode_entities() {
        let cleaner = ContentCleaner::default();
        let text = "Hello&nbsp;World&gt;";
        let cleaned = cleaner.clean(text);
        assert!(cleaned.contains("Hello World>"));
    }

    #[test]
    fn test_apply_replace_rules_regex() {
        let content = "Hello World 123";
        let rules = vec![(r"\d+".to_string(), "XXX".to_string())];
        let result = apply_replace_rules(content, &rules);
        assert_eq!(result, "Hello World XXX");
    }

    #[test]
    fn test_apply_replace_rules_plain_text() {
        let content = "Hello World";
        let rules = vec![("World".to_string(), "Rust".to_string())];
        let result = apply_replace_rules(content, &rules);
        assert_eq!(result, "Hello Rust");
    }

    #[test]
    fn test_apply_replace_rules_empty() {
        let content = "Hello World";
        let result = apply_replace_rules(content, &[]);
        assert_eq!(result, "Hello World");
    }
}
