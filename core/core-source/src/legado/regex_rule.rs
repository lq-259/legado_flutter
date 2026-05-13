//! Legado 正则规则
//!
//! 实现三类正则规则：
//! 1. AllInOne: 以 `:` 开头，用于搜索列表/发现列表/详情页预加载/目录列表
//!    格式: `:pattern`，使用 $1, $2 等捕获组
//! 2. OnlyOne: `##regex##replacement###`，只获取第一个匹配并替换
//! 3. 净化: `##regex##replacement` 或 `selector@text##regex##replacement`，
//!    对结果进行循环替换

use regex::Regex;

/// AllInOne 正则规则解析结果
#[derive(Debug, Clone)]
pub struct AllInOneRule {
    /// 正则表达式
    pub regex: Regex,
    /// 捕获组映射 (组名 → 字段名)
    pub groups: Vec<usize>,
}

/// 净化规则对 (pattern, replacement)
#[derive(Debug, Clone)]
pub struct PurificationRule {
    pub pattern: String,
    pub replacement: String,
}

/// 解析 AllInOne 规则 (:pattern)
/// 返回正则表达式和捕获组索引
pub fn parse_all_in_one(rule: &str) -> Result<AllInOneRule, String> {
    let pattern = rule.trim_start_matches(':').trim();
    if pattern.is_empty() {
        return Err("AllInOne rule pattern is empty".into());
    }

    let regex = Regex::new(pattern)
        .map_err(|e| format!("AllInOne regex compile error: {}", e))?;

    // 收集所有具名和编号捕获组
    let mut groups = Vec::new();
    for i in 1..=regex.captures_len() {
        groups.push(i);
    }

    // 如果有命名捕获组，优先使用命名组顺序
    // 对于 Legado 的 AllInOne 规则，通常使用 $1, $2 等

    Ok(AllInOneRule { regex, groups })
}

/// 执行 AllInOne 规则，提取所有匹配
pub fn execute_all_in_one(rule: &AllInOneRule, html: &str) -> Vec<Vec<String>> {
    let mut results = Vec::new();
    for caps in rule.regex.captures_iter(html) {
        let mut row = Vec::new();
        for &group in &rule.groups {
            let val = caps.get(group).map(|m| m.as_str().to_string()).unwrap_or_default();
            row.push(val);
        }
        if !row.is_empty() {
            results.push(row);
        }
    }
    results
}

/// 解析净化规则 (##regex##replacement)
pub fn parse_purification(rule_part: &str) -> Option<PurificationRule> {
    if !rule_part.contains("##") {
        return None;
    }

    // 格式: ##pattern##replacement
    if let Some(rest) = rule_part.strip_prefix("##") {
        if let Some((pattern, replacement)) = rest.split_once("##") {
            return Some(PurificationRule {
                pattern: pattern.to_string(),
                replacement: replacement.to_string(),
            });
        }
        // 只有 ##pattern，没有第二个 ##
        return Some(PurificationRule {
            pattern: rest.to_string(),
            replacement: String::new(),
        });
    }

    None
}

/// 应用净化规则到文本
pub fn apply_purification(text: &str, rule: &PurificationRule) -> String {
    match Regex::new(&rule.pattern) {
        Ok(re) => re.replace_all(text, rule.replacement.as_str()).to_string(),
        Err(_) => text.to_string(),
    }
}

/// 解析 OnlyOne 规则 (##regex##replacement###)
/// 从完整规则字符串中分离出 OnlyOne 部分
pub fn parse_only_one(rule: &str) -> Option<(String, PurificationRule)> {
    if rule.ends_with("###") {
        let inner = &rule[..rule.len() - 3];
        if let Some(rest) = inner.strip_prefix("##") {
            if let Some((pattern, replacement)) = rest.split_once("##") {
                return Some((
                    String::new(),
                    PurificationRule {
                        pattern: pattern.to_string(),
                        replacement: replacement.to_string(),
                    },
                ));
            }
        }
    }
    None
}

/// 判断规则是否为 AllInOne
pub fn is_all_in_one(rule: &str) -> bool {
    rule.trim().starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_in_one() {
        let rule = parse_all_in_one(r#":href="([^"]*)"[^>]*>([^<]*)</a>"#).unwrap();
        let html = r#"<a href="/chapter/1">Chapter 1</a><a href="/chapter/2">Chapter 2</a>"#;
        let results = execute_all_in_one(&rule, html);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0], "/chapter/1");
        assert_eq!(results[0][1], "Chapter 1");
    }

    #[test]
    fn test_apply_purification() {
        let rule = PurificationRule {
            pattern: r"\(本章完\)".to_string(),
            replacement: String::new(),
        };
        let result = apply_purification("这是正文(本章完)", &rule);
        assert_eq!(result, "这是正文");
    }

    #[test]
    fn test_is_all_in_one() {
        assert!(is_all_in_one(":href=\"([^\"]*)\""));
        assert!(!is_all_in_one("div.class@text"));
    }
}
