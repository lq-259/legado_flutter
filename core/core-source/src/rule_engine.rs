//! # 规则引擎模块
//!
//! 负责解析和执行书源规则表达式（CSS/XPath/JSONPath/Regex/JavaScript）。
//! 对应原 Legado 的 AnalyzeRule 模块 (model/analyzeRule/)。

use crate::script_engine::{ScriptContext, ScriptEngine, ScriptResult};
use crate::types::ExtractType;
use jsonpath_lib as jsonpath;
use regex::RegexBuilder;
use scraper::{Html, Node, Selector};
use std::panic::{catch_unwind, AssertUnwindSafe};
use sxd_document::dom::ChildOfElement as SxdChild;
use sxd_document::parser as xpath_parser;
use sxd_xpath::{Context, Factory, Value as XPathValue};
use tracing::debug;

/// 规则类型
/// Legado ## replace rule
#[derive(Debug, Clone)]
pub struct ReplaceRule {
    pattern: String,
    replacement: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum RuleType {
    #[default]
    Css, // CSS 选择器
    XPath,      // XPath 表达式
    JsonPath,   // JSONPath 表达式
    Regex,      // 正则表达式
    JavaScript, // JavaScript 脚本
}

impl std::fmt::Display for RuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleType::Css => write!(f, "CSS"),
            RuleType::XPath => write!(f, "XPath"),
            RuleType::JsonPath => write!(f, "JsonPath"),
            RuleType::Regex => write!(f, "Regex"),
            RuleType::JavaScript => write!(f, "JavaScript"),
        }
    }
}

/// 规则表达式
#[derive(Debug, Clone, Default)]
pub struct RuleExpression {
    pub rule_type: RuleType,
    pub expression: String,
    pub extract_type: ExtractType,
    pub css_index: Option<isize>,
    pub css_skip: Option<usize>,
    pub replace_rules: Vec<ReplaceRule>,
}

impl RuleExpression {
    /// 解析规则表达式字符串
    /// 支持格式：
    /// - CSS: "classname" 或 "div.classname"
    /// - Regex: "/pattern/flags" 或 "regex:pattern"
    /// - XPath: "//div[@class='test']" 或 "@XPath:..."
    /// - JsonPath: "$.data.items" 或 "@Json:..."
    /// - JavaScript: "js:..." 或字段为 jsLib
    pub fn parse(rule_str: &str) -> Option<Self> {
        let (trimmed, replace_rules) = strip_legado_replace_rules(rule_str);
        let trimmed = trimmed.trim();

        // 空规则
        if trimmed.is_empty() {
            return None;
        }

        // 解析提取类型后缀
        let (expr, extract_type) = Self::parse_extract_type(trimmed);
        let (trimmed, css_index, css_skip) = parse_css_modifiers(expr);

        // 判断规则类型
        if trimmed.starts_with('/') && !trimmed.starts_with("//") {
            // Regex enclosure: /pattern/ 或 /pattern/flags，标志只限 igmsxuU
            // XPath absolute path: /bookstore/book 或 /html/body/div[@class='foo']
            let last_slash_idx = trimmed.rfind('/');
            let looks_like_regex_delimiter = last_slash_idx.map_or(false, |idx| {
                idx > 0 && {
                    let after = &trimmed[idx + 1..];
                    after.is_empty()
                        || after
                            .chars()
                            .all(|c| matches!(c, 'i' | 'g' | 'm' | 's' | 'x' | 'u' | 'U'))
                }
            });
            if looks_like_regex_delimiter {
                Some(Self {
                    rule_type: RuleType::Regex,
                    expression: trimmed.to_string(),
                    extract_type,
                    css_index,
                    css_skip,
                    replace_rules,
                })
            } else {
                // 不是 regex delimiter → 作为 absolute XPath 路径
                Some(Self {
                    rule_type: RuleType::XPath,
                    expression: trimmed.to_string(),
                    extract_type,
                    css_index,
                    css_skip,
                    replace_rules,
                })
            }
        } else if let Some(expr) = trimmed.strip_prefix("regex:") {
            Some(Self {
                rule_type: RuleType::Regex,
                expression: expr.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if trimmed.starts_with("//") || trimmed.starts_with("@XPath:") {
            // XPath
            let expr = trimmed.strip_prefix("@XPath:").unwrap_or(trimmed);
            Some(Self {
                rule_type: RuleType::XPath,
                expression: expr.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if trimmed.starts_with("$.")
            || trimmed.starts_with("$[")
            || trimmed.starts_with("@Json:")
        {
            // JsonPath
            let expr = trimmed.strip_prefix("@Json:").unwrap_or(trimmed);
            Some(Self {
                rule_type: RuleType::JsonPath,
                expression: expr.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if trimmed.starts_with("js:") || trimmed.starts_with("@js:") {
            // JavaScript
            Some(Self {
                rule_type: RuleType::JavaScript,
                expression: trimmed.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if let Some(expr) =
            trimmed.strip_prefix("@css:").or_else(|| trimmed.strip_prefix("@CSS:"))
        {
            Some(Self {
                rule_type: RuleType::Css,
                expression: expr.trim().to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if trimmed.contains('{') && trimmed.contains('}') && trimmed.starts_with('$') {
            // 可能是 JSONPath 或包含占位符的字符串
            Some(Self {
                rule_type: RuleType::JsonPath,
                expression: trimmed.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else if looks_like_xpath_function(trimmed) {
            // XPath 函数调用: count(//item), normalize-space(//title) 等
            Some(Self {
                rule_type: RuleType::XPath,
                expression: trimmed.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        } else {
            // 默认作为 CSS 选择器
            Some(Self {
                rule_type: RuleType::Css,
                expression: trimmed.to_string(),
                extract_type,
                css_index,
                css_skip,
                replace_rules,
            })
        }
    }

    /// 解析提取类型后缀（@text, @html, etc.）
    fn parse_extract_type(rule_str: &str) -> (&str, ExtractType) {
        ExtractType::from_rule(rule_str)
    }

    /// 执行规则，返回匹配结果
    pub fn evaluate(&self, content: &str) -> Result<Vec<String>, RuleError> {
        let results = match self.rule_type {
            RuleType::Css => self.evaluate_css(content),
            RuleType::Regex => self.evaluate_regex(content),
            RuleType::XPath => self.evaluate_xpath(content),
            RuleType::JsonPath => self.evaluate_jsonpath(content),
            RuleType::JavaScript => self.evaluate_javascript(content),
        }?;
        Ok(apply_replace_rules_to_results(results, &self.replace_rules))
    }

    /// CSS 选择器执行
    fn evaluate_css(&self, content: &str) -> Result<Vec<String>, RuleError> {
        debug!("执行 CSS 规则: {}", self.expression);

        let document = Html::parse_document(content);

        // 尝试解析为 CSS 选择器
        let selector = parse_selector_safely(&self.expression)?;

        let mut results: Vec<String> = match self.extract_type {
            ExtractType::Text => document
                .select(&selector)
                .map(|element| element.text().collect::<Vec<_>>().join(""))
                .collect(),
            ExtractType::Html => document
                .select(&selector)
                .map(|element| element.inner_html())
                .collect(),
            ExtractType::OwnText => document
                .select(&selector)
                .map(|element| {
                    element
                        .children()
                        .filter_map(|child| {
                            if let Node::Text(t) = child.value() {
                                Some(t.text.to_string())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("")
                })
                .collect(),
            ExtractType::Href => document
                .select(&selector)
                .filter_map(|element| element.value().attr("href"))
                .map(|s| s.to_string())
                .collect(),
            ExtractType::Src => document
                .select(&selector)
                .filter_map(|element| element.value().attr("src"))
                .map(|s| s.to_string())
                .collect(),
            ExtractType::Attr(ref attr) => document
                .select(&selector)
                .filter_map(|element| element.value().attr(attr))
                .map(|s| s.to_string())
                .collect(),
            _ => document
                .select(&selector)
                .map(|element| element.inner_html())
                .collect(),
        };

        if let Some(skip) = self.css_skip {
            results = results.into_iter().skip(skip).collect();
        }
        if let Some(index) = self.css_index {
            let index = if index < 0 {
                results
                    .len()
                    .checked_sub(index.unsigned_abs())
                    .unwrap_or(usize::MAX)
            } else {
                index as usize
            };
            results = results.get(index).cloned().into_iter().collect();
        }

        Ok(results)
    }

    /// 正则表达式执行
    fn evaluate_regex(&self, content: &str) -> Result<Vec<String>, RuleError> {
        debug!("执行 Regex 规则: {}", self.expression);

        // 解析正则（支持 /pattern/flags 格式）
        let (pattern, flags) = self.parse_regex_with_flags(&self.expression);

        let mut builder = RegexBuilder::new(pattern);

        if flags.contains('i') {
            builder.case_insensitive(true);
        }
        if flags.contains('m') {
            builder.multi_line(true);
        }
        if flags.contains('s') {
            builder.dot_matches_new_line(true);
        }
        if flags.contains('x') {
            builder.ignore_whitespace(true);
        }
        if flags.contains('u') {
            // Unicode 是 Rust regex 默认行为，显式设置以保证语义清晰
            builder.unicode(true);
        }
        if flags.contains('U') {
            builder.swap_greed(true);
        }
        // 'g' 标志: Rust captures_iter 已返回所有非重叠匹配，无需显式处理

        let re = builder
            .build()
            .map_err(|e| RuleError::ParseError(format!("正则编译失败: {}", e)))?;

        let results: Vec<String> = re
            .captures_iter(content)
            .map(|cap| {
                // 返回第一个捕获组，如果没有则返回整个匹配
                if cap.len() > 1 {
                    cap[1].to_string()
                } else {
                    cap[0].to_string()
                }
            })
            .collect();

        Ok(results)
    }

    fn evaluate_xpath(&self, content: &str) -> Result<Vec<String>, RuleError> {
        debug!("执行 XPath 规则: {}", self.expression);

        let package = xpath_parser::parse(content)
            .map_err(|e| RuleError::ParseError(format!("HTML 解析失败: {}", e)))?;
        let document = package.as_document();

        let xpath_opt = Factory::new()
            .build(&self.expression)
            .map_err(|e| RuleError::ParseError(format!("XPath 编译失败: {}", e)))?;
        let xpath =
            xpath_opt.ok_or_else(|| RuleError::ParseError("XPath 表达式编译返回空".into()))?;

        let context = Context::new();
        let value = xpath
            .evaluate(&context, document.root())
            .map_err(|e| RuleError::EvaluationError(format!("XPath 执行失败: {}", e)))?;

        match value {
            XPathValue::Nodeset(nodes) => {
                let results: Vec<String> = nodes
                    .iter()
                    .map(|node| match self.extract_type {
                        ExtractType::Html => node.string_value(),
                        ExtractType::Href | ExtractType::Src => {
                            let attr = match self.extract_type {
                                ExtractType::Href => "href",
                                ExtractType::Src => "src",
                                _ => unreachable!(),
                            };
                            node.element()
                                .and_then(|el| el.attribute(attr))
                                .map(|a| a.value().to_string())
                                .unwrap_or_default()
                        }
                        ExtractType::OwnText => node
                            .element()
                            .map(|el| {
                                el.children()
                                    .into_iter()
                                    .filter_map(|child| {
                                        if let SxdChild::Text(t) = child {
                                            Some(t.text().to_string())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("")
                            })
                            .unwrap_or_else(|| node.string_value()),
                        _ => node.string_value(),
                    })
                    .collect();
                Ok(results)
            }
            XPathValue::String(s) => Ok(vec![s]),
            XPathValue::Boolean(b) => Ok(vec![b.to_string()]),
            XPathValue::Number(n) => Ok(vec![n.to_string()]),
        }
    }

    /// JSONPath 执行
    fn evaluate_jsonpath(&self, content: &str) -> Result<Vec<String>, RuleError> {
        debug!("执行 JsonPath 规则: {}", self.expression);

        let json: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| RuleError::ParseError(format!("JSON 解析失败: {}", e)))?;

        let results = jsonpath::select(&json, &self.expression)
            .map_err(|e| RuleError::EvaluationError(format!("JsonPath 执行失败: {}", e)))?;

        let mut out = Vec::new();
        for value in results {
            match value {
                serde_json::Value::Array(items) => {
                    out.extend(items.iter().map(|item| match item {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }));
                }
                serde_json::Value::String(s) => out.push(s.clone()),
                other => out.push(other.to_string()),
            }
        }
        Ok(out)
    }

    /// 解析正则及其标志（/pattern/flags）
    fn parse_regex_with_flags<'a>(&self, input: &'a str) -> (&'a str, String) {
        if let Some(rest) = input.strip_prefix('/') {
            let end_slash = rest.rfind('/');
            if let Some(pos) = end_slash {
                let pattern = &rest[..pos];
                let flags = &rest[pos + 1..];
                return (pattern, flags.to_string());
            }
        }
        (input, String::new())
    }

    fn evaluate_javascript(&self, content: &str) -> Result<Vec<String>, RuleError> {
        debug!(
            "执行 JS 规则: {}...",
            &self.expression.chars().take(60).collect::<String>()
        );

        let script = self
            .expression
            .strip_prefix("js:")
            .or_else(|| self.expression.strip_prefix("@js:"))
            .unwrap_or(&self.expression);

        let engine = ScriptEngine::new();
        let ctx = ScriptContext::new("", content, "");

        match engine.eval(script, Some(&ctx)) {
            Ok(result) => Ok(script_result_to_strings(result)),
            Err(e) => Err(RuleError::EvaluationError(e)),
        }
    }
}

fn parse_selector_safely(selector: &str) -> Result<Selector, RuleError> {
    catch_unwind(AssertUnwindSafe(|| Selector::parse(selector)))
        .map_err(|_| RuleError::ParseError(format!("CSS 解析 panic: {}", selector)))?
        .map_err(|_| RuleError::ParseError(format!("CSS 解析失败: {}", selector)))
}

fn script_result_to_strings(result: ScriptResult) -> Vec<String> {
    match result {
        ScriptResult::String(s) => vec![s],
        ScriptResult::Int(i) => vec![i.to_string()],
        ScriptResult::Float(f) => vec![f.to_string()],
        ScriptResult::Bool(b) => vec![b.to_string()],
        ScriptResult::Array(arr) => arr.into_iter().flat_map(script_result_to_strings).collect(),
        ScriptResult::Map(map) => map
            .into_values()
            .flat_map(script_result_to_strings)
            .collect(),
        ScriptResult::Null => vec![],
    }
}

fn looks_like_xpath_function(s: &str) -> bool {
    if let Some(paren_pos) = s.find('(') {
        let prefix = &s[..paren_pos];
        const XPATH_FUNCTIONS: &[&str] = &[
            "count",
            "normalize-space",
            "string",
            "concat",
            "contains",
            "starts-with",
            "substring",
            "substring-before",
            "substring-after",
            "string-length",
            "translate",
            "not",
            "true",
            "false",
            "number",
            "sum",
            "floor",
            "ceiling",
            "round",
            "name",
            "local-name",
            "namespace-uri",
            "lang",
            "position",
            "last",
        ];
        XPATH_FUNCTIONS.contains(&prefix)
    } else {
        false
    }
}

fn parse_css_modifiers(expr: &str) -> (&str, Option<isize>, Option<usize>) {
    if let Some((selector, index)) = expr.rsplit_once('.') {
        if !selector.is_empty() {
            if let Ok(index) = index.parse::<isize>() {
                return (selector, Some(index), None);
            }
        }
    }
    if let Some((selector, skip)) = expr.rsplit_once('!') {
        if !selector.is_empty() {
            if let Ok(skip) = skip.parse::<usize>() {
                return (selector, None, Some(skip));
            }
        }
    }
    (expr, None, None)
}

fn rfind_at_depth_zero(s: &str, c: char) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices().rev() {
        match ch {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                depth -= 1;
                if depth < 0 {
                    depth = 0;
                }
            }
            _ if ch == c && depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

pub(crate) fn strip_legado_replace_rules(expr: &str) -> (&str, Vec<ReplaceRule>) {
    if let Some(idx) = expr.find("##") {
        let (body, rules_str) = expr.split_at(idx);
        let rules_str = &rules_str[2..];
        let rules = parse_replace_rules(rules_str);
        (body, rules)
    } else {
        (expr, Vec::new())
    }
}

fn parse_replace_rules(rules_str: &str) -> Vec<ReplaceRule> {
    let mut rules = Vec::new();
    for part in rules_str.split('|') {
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = part.find("##") {
            let pattern = &part[..idx];
            let replacement = &part[idx + 2..];
            rules.push(ReplaceRule {
                pattern: pattern.to_string(),
                replacement: replacement.to_string(),
            });
        } else {
            rules.push(ReplaceRule {
                pattern: part.to_string(),
                replacement: String::new(),
            });
        }
    }
    rules
}

pub(crate) fn strip_css_modifiers(expr: &str) -> (&str, Option<isize>, Option<usize>) {
    let mut remaining = expr;
    let mut index: Option<isize> = None;
    let mut skip: Option<usize> = None;

    if let Some(pos) = rfind_at_depth_zero(remaining, '!') {
        let after = &remaining[pos + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
            skip = after.parse().ok();
            remaining = &remaining[..pos];
        }
    }

    if let Some(pos) = rfind_at_depth_zero(remaining, '.') {
        let after = &remaining[pos + 1..];
        let is_index = if let Some(digits) = after.strip_prefix('-') {
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
        } else {
            !after.is_empty() && after.chars().all(|c| c.is_ascii_digit())
        };
        if is_index {
            index = after.parse().ok();
            remaining = &remaining[..pos];
        }
    }

    (remaining, index, skip)
}

pub(crate) fn split_css_alternatives(expr: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut chars = expr.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '[' | '(' => {
                depth += 1;
                current.push(c);
            }
            ']' | ')' => {
                if depth > 0 {
                    depth -= 1;
                }
                current.push(c);
            }
            '|' => {
                if depth == 0 && chars.peek() == Some(&'|') {
                    chars.next();
                    if !current.trim().is_empty() {
                        result.push(current.trim().to_string());
                    }
                    current = String::new();
                } else {
                    current.push(c);
                }
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn apply_replace_rules_to_results(results: Vec<String>, rules: &[ReplaceRule]) -> Vec<String> {
    if rules.is_empty() {
        results
    } else {
        results
            .into_iter()
            .map(|text| apply_replace_rules(&text, rules))
            .collect()
    }
}

fn apply_replace_rules(text: &str, rules: &[ReplaceRule]) -> String {
    let mut result = text.to_string();
    for rule in rules {
        if let Ok(re) = regex::Regex::new(&rule.pattern) {
            if rule.replacement.is_empty() {
                result = re.replace_all(&result, "").to_string();
            } else {
                result = re
                    .replace_all(&result, rule.replacement.as_str())
                    .to_string();
            }
        }
    }
    result
}

/// 规则错误
#[derive(Debug)]
pub enum RuleError {
    ParseError(String),
    EvaluationError(String),
    NotSupported(String),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::ParseError(msg) => write!(f, "解析错误: {}", msg),
            RuleError::EvaluationError(msg) => write!(f, "执行错误: {}", msg),
            RuleError::NotSupported(msg) => write!(f, "不支持: {}", msg),
        }
    }
}

impl std::error::Error for RuleError {}

/// 规则引擎（统一管理所有规则执行）
pub struct RuleEngine {
    // 可扩展配置
}

impl RuleEngine {
    /// 创建新的规则引擎
    pub fn new() -> Self {
        Self {}
    }

    /// 执行单个规则
    pub fn execute_rule(&self, rule_str: &str, content: &str) -> Result<Vec<String>, RuleError> {
        if let Some(rule) = RuleExpression::parse(rule_str) {
            rule.evaluate(content)
        } else {
            Ok(vec![])
        }
    }

    /// 执行规则并返回第一个结果
    pub fn execute_rule_first(&self, rule_str: &str, content: &str) -> Option<String> {
        self.execute_rule(rule_str, content).ok().and_then(|mut v| {
            if v.is_empty() {
                None
            } else {
                Some(v.remove(0))
            }
        })
    }

    /// 批量执行规则（返回所有匹配）
    pub fn execute_rules(&self, rules: &[String], content: &str) -> Vec<String> {
        let mut results = Vec::new();
        for rule in rules {
            if let Ok(mut r) = self.execute_rule(rule, content) {
                results.append(&mut r);
            }
        }
        results
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_rule() {
        let html = r#"<div class="book"><h1>书名</h1><p>作者</p></div>"#;
        let rule = RuleExpression::parse(".book h1").unwrap();
        let results = rule.evaluate(html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("书名"));
    }

    #[test]
    fn test_regex_rule() {
        let text = "书名: 测试书籍, 作者: 张三";
        let rule = RuleExpression::parse(r"/书名:\s*(\w+)/").unwrap();
        let results = rule.evaluate(text).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_rule_type_detection() {
        assert!(matches!(
            RuleExpression::parse(".class"),
            Some(RuleExpression {
                rule_type: RuleType::Css,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("/regex/"),
            Some(RuleExpression {
                rule_type: RuleType::Regex,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("/regex/gi"),
            Some(RuleExpression {
                rule_type: RuleType::Regex,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("$.data"),
            Some(RuleExpression {
                rule_type: RuleType::JsonPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("$[0]"),
            Some(RuleExpression {
                rule_type: RuleType::JsonPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("//xpath"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("/bookstore/book"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("/html/body/div[@class='foo']"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("count(//item)"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("normalize-space(//title)"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
        assert!(matches!(
            RuleExpression::parse("contains(. , 'text')"),
            Some(RuleExpression {
                rule_type: RuleType::XPath,
                ..
            })
        ));
    }

    #[test]
    fn test_regex_dot_matches_newline() {
        let rule = RuleExpression::parse(r"/a.b/s").unwrap();
        assert!(matches!(rule.rule_type, RuleType::Regex));
        let results = rule.evaluate("a\nb").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "a\nb");
    }

    #[test]
    fn test_regex_dot_without_s_flag() {
        let rule = RuleExpression::parse(r"/a.b/").unwrap();
        let results = rule.evaluate("a\nb").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_legado_replace_rules_are_applied() {
        let rule = RuleExpression::parse("p@text##world##reader").unwrap();
        let results = rule.evaluate("<p>hello world</p>").unwrap();
        assert_eq!(results, vec!["hello reader"]);
    }

    #[test]
    fn test_xpath_own_text() {
        let xml = r#"<div>own text<span>child text</span> tail text</div>"#;
        let rule = RuleExpression::parse("//div@ownText").unwrap();
        assert!(matches!(rule.rule_type, RuleType::XPath));
        let results = rule.evaluate(xml).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "own text tail text");
    }
}
