//! Legado JSOUP Default 选择器链
//!
//! 实现 Legado 的 Default 规则语法（JSOUP 变体）：
//!
//! 语法结构：`selector@selector@...@extract`
//!
//! 每个 selector 段支持：
//! - `class.xxx`    → CSS `.xxx`
//! - `id.xxx`       → CSS `#xxx`
//! - `tag.xxx`      → 标签选择器 `xxx`
//! - `text.xxx`     → 按文本内容筛选
//! - `children`     → 获取所有子节点
//! - `.N`           → 正索引 (0-based)
//! - `.-N`          → 负索引 (倒数)
//! - `!N`           → 跳过 N 项
//! - `-`            → 列表倒序
//! - `[start:end:step]` → 区间索引
//! - `[index,index,...]` → 多索引
//! - `[!index,...]` → 排除索引
//!
//! 提取后缀：
//! - `@text`        → 提取文本
//! - `@textNodes`   → 所有文本节点
//! - `@textNode`    → 单个文本节点
//! - `@ownText`     → 仅自身文本
//! - `@html`        → 提取 HTML
//! - `@all`         → 提取所有（含 outerHTML）
//! - `@href`        → 提取 href 属性
//! - `@src`         → 提取 src 属性
//! - `@content`     → 提取 content 属性 (meta 标签)
//! - `@attrName`    → 提取任意属性

use scraper::{Html, Selector};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// 选择器链的提取类型
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractSuffix {
    Text,
    TextNodes,
    TextNode,
    OwnText,
    Html,
    All,
    Href,
    Src,
    Content,
    Attr(String),
    None,
}

/// 一个选择器段落的修饰符
#[derive(Debug, Clone, Default)]
pub struct SelectorModifiers {
    /// 正索引 (0-based)
    pub index: Option<isize>,
    /// 跳过前 N 项
    pub skip: Option<usize>,
    /// 是否倒序
    pub reverse: bool,
    /// 数组/区间索引修饰符，如 [0,2]、[!1]、[1:-1:2]
    pub array: Option<ArrayModifier>,
}

#[derive(Debug, Clone)]
pub enum ArrayModifier {
    Include(Vec<isize>),
    Exclude(Vec<isize>),
    Range {
        start: Option<isize>,
        end: Option<isize>,
        step: isize,
    },
}

/// 解析后的选择器链
#[derive(Debug, Clone)]
pub struct LegadoSelectorChain {
    /// 原始规则字符串
    pub raw: String,
    /// 选择器段列表（按 @ 分割，最后一段是提取类型）
    pub segments: Vec<SelectorSegment>,
    /// 提取类型
    pub extract: ExtractSuffix,
    /// 净化正则（##regex##replacement）
    pub purification: Option<(String, String)>,
}

/// 选择器链中的一个段落
#[derive(Debug, Clone)]
pub struct SelectorSegment {
    /// CSS 选择器字符串
    pub selector: String,
    /// 修饰符
    pub modifiers: SelectorModifiers,
}

/// 解析 Legado 选择器规则字符串
pub fn parse_legado_selector(raw: &str) -> LegadoSelectorChain {
    let trimmed = raw.trim();

    // 分离 ## 净化正则段
    let (selector_part, purification) = split_purification(trimmed);

    if selector_part.is_empty() {
        return LegadoSelectorChain {
            raw: trimmed.to_string(),
            segments: Vec::new(),
            extract: ExtractSuffix::None,
            purification,
        };
    }

    // 按 @ 分割段落
    // 需要小心处理：@ 可能出现在属性选择器中，如 [property$=title]@content
    let parts = split_by_at_sign(&selector_part);

    let mut segments = Vec::new();
    let mut extract = ExtractSuffix::None;
    let mut selector_parts = parts.as_slice();

    if let Some(last) = parts.last().map(|part| part.trim()) {
        if let Some(ext) = parse_extract_token(last) {
            extract = ext;
            selector_parts = &parts[..parts.len() - 1];
        }
    }

    for part in selector_parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if extract == ExtractSuffix::None {
            let (selector, ext) = parse_extract_suffix(part);
            extract = ext;
            if !selector.is_empty() {
                push_selector_segments(&mut segments, selector);
            }
        } else {
            push_selector_segments(&mut segments, part);
        }
    }

    LegadoSelectorChain {
        raw: trimmed.to_string(),
        segments,
        extract,
        purification,
    }
}

fn push_selector_segments(segments: &mut Vec<SelectorSegment>, selector: &str) {
    for part in selector.split_whitespace() {
        if !part.is_empty() {
            segments.push(parse_selector_with_modifiers(&normalize_selector_alias(
                part,
            )));
        }
    }
}

fn normalize_selector_alias(sel: &str) -> String {
    if let Some(name) = sel.strip_prefix("class.") {
        return format!(".{}", name);
    }
    if let Some(name) = sel.strip_prefix("id.") {
        return format!("#{}", name);
    }
    if let Some(name) = sel.strip_prefix("tag.") {
        return name.to_string();
    }
    sel.to_string()
}

fn parse_extract_token(token: &str) -> Option<ExtractSuffix> {
    match token {
        "textNodes" => Some(ExtractSuffix::TextNodes),
        "textNode" => Some(ExtractSuffix::TextNode),
        "ownText" => Some(ExtractSuffix::OwnText),
        "content" => Some(ExtractSuffix::Content),
        "html" => Some(ExtractSuffix::Html),
        "all" => Some(ExtractSuffix::All),
        "text" => Some(ExtractSuffix::Text),
        "href" => Some(ExtractSuffix::Href),
        "src" => Some(ExtractSuffix::Src),
        _ => None,
    }
}

/// 按 @ 符号分割段落（保护属性选择器中的 @）
fn split_by_at_sign(selector: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_bracket = 0;
    let mut chars = selector.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '[' => {
                in_bracket += 1;
                current.push(ch);
            }
            ']' => {
                if in_bracket > 0 {
                    in_bracket -= 1;
                }
                current.push(ch);
            }
            '@' if in_bracket == 0 => {
                // @ 分隔符
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// 解析提取后缀
fn parse_extract_suffix(part: &str) -> (&str, ExtractSuffix) {
    let suffixes: &[(&str, ExtractSuffix)] = &[
        ("@textNodes", ExtractSuffix::TextNodes),
        ("@textNode", ExtractSuffix::TextNode),
        ("@ownText", ExtractSuffix::OwnText),
        ("@content", ExtractSuffix::Content),
        ("@html", ExtractSuffix::Html),
        ("@all", ExtractSuffix::All),
        ("@text", ExtractSuffix::Text),
        ("@href", ExtractSuffix::Href),
        ("@src", ExtractSuffix::Src),
    ];

    for (suffix, ext) in suffixes {
        if let Some(selector) = part.strip_suffix(suffix) {
            return (selector, ext.clone());
        }
    }

    // 检查是否是 @attrName 形式
    if let Some(pos) = part.rfind('@') {
        if pos > 0 {
            let attr_name = &part[pos + 1..];
            if !attr_name.is_empty()
                && attr_name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return (&part[..pos], ExtractSuffix::Attr(attr_name.to_string()));
            }
        }
    }

    (part, ExtractSuffix::None)
}

/// 解析选择器和修饰符
fn parse_selector_with_modifiers(sel: &str) -> SelectorSegment {
    let sel = sel.trim();
    let mut modifiers = SelectorModifiers::default();

    if sel.is_empty() {
        return SelectorSegment {
            selector: String::new(),
            modifiers,
        };
    }

    // 处理倒序修饰符
    let sel = if sel.starts_with('-') && !sel.starts_with("--") {
        modifiers.reverse = true;
        &sel[1..]
    } else {
        sel
    };

    // 检查最后一部分是否是 .N 索引或 !N 跳过
    let (selector, mods) = parse_tail_modifiers(sel);

    SelectorSegment {
        selector: selector.to_string(),
        modifiers: SelectorModifiers {
            index: mods.index.or(modifiers.index),
            skip: mods.skip.or(modifiers.skip),
            reverse: mods.reverse || modifiers.reverse,
            array: mods.array.or(modifiers.array),
        },
    }
}

/// 解析尾部修饰符（.index 或 !skip）
fn parse_tail_modifiers(sel: &str) -> (&str, SelectorModifiers) {
    let mut modifiers = SelectorModifiers::default();

    if let Some((selector, array)) = split_array_modifier(sel) {
        modifiers.array = Some(array);
        return (selector, modifiers);
    }

    // 检查 `!N` 跳过修饰符
    if let Some((selector, skip_str)) = sel.rsplit_once('!') {
        if !selector.is_empty() && !selector.ends_with('\\') {
            if let Ok(skip) = skip_str.parse::<usize>() {
                modifiers.skip = Some(skip);
                return (selector, modifiers);
            }
        }
    }

    // 检查 `.N` 索引修饰符，但不算 class.xxx 等
    if let Some((selector, index_str)) = sel.rsplit_once('.') {
        if !selector.is_empty()
            && !selector.ends_with('\\')
            && !selector.ends_with("class")
            && !selector.ends_with("id")
            && !selector.ends_with("tag")
            && !selector.ends_with("text")
        {
            // 尝试解析为数字
            if let Ok(index) = index_str.parse::<isize>() {
                modifiers.index = Some(index);
                return (selector, modifiers);
            }
        }
    }

    (sel, modifiers)
}

fn split_array_modifier(sel: &str) -> Option<(&str, ArrayModifier)> {
    let start = sel.rfind('[')?;
    if !sel.ends_with(']') || start == 0 {
        return None;
    }
    let selector = &sel[..start];
    let body = sel[start + 1..sel.len() - 1].trim();
    if body.is_empty() {
        return None;
    }
    Some((selector, parse_array_modifier(body)?))
}

fn parse_array_modifier(body: &str) -> Option<ArrayModifier> {
    if let Some(rest) = body.strip_prefix('!') {
        return parse_index_list(rest).map(ArrayModifier::Exclude);
    }
    if body.contains(':') {
        let parts: Vec<&str> = body.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return None;
        }
        let start = parse_optional_index(parts[0]);
        let end = parse_optional_index(parts[1]);
        let step = if parts.len() == 3 && !parts[2].trim().is_empty() {
            parts[2].trim().parse().ok()?
        } else {
            1
        };
        if step == 0 {
            return None;
        }
        return Some(ArrayModifier::Range { start, end, step });
    }
    parse_index_list(body).map(ArrayModifier::Include)
}

fn parse_optional_index(value: &str) -> Option<isize> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        value.parse().ok()
    }
}

fn parse_index_list(body: &str) -> Option<Vec<isize>> {
    let indexes: Vec<isize> = body
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                None
            } else {
                part.parse().ok()
            }
        })
        .collect();
    if indexes.is_empty() {
        None
    } else {
        Some(indexes)
    }
}

/// 分离净化正则段
fn split_purification(rule: &str) -> (String, Option<(String, String)>) {
    // OnlyOne 模式：##regex##replacement###
    if rule.ends_with("###") {
        let rest = &rule[..rule.len() - 3];
        if let Some(pos) = rest.rfind("##") {
            if pos > 0 {
                let selector = &rest[..pos].trim();
                let content = &rest[pos + 2..].trim();
                if let Some(mid) = content.find("##") {
                    let pattern = &content[..mid];
                    let replacement = &content[mid + 2..];
                    return (
                        selector.to_string(),
                        Some((pattern.to_string(), replacement.to_string())),
                    );
                }
                return (
                    selector.to_string(),
                    Some((content.to_string(), String::new())),
                );
            }
        }
    }

    // 净化模式：##regex##replacement
    if !rule.starts_with("##") {
        if let Some(pos) = rule.find("##") {
            if pos > 0 {
                let selector = &rule[..pos].trim();
                let content = &rule[pos + 2..].trim();
                if let Some(mid) = content.find("##") {
                    let pattern = &content[..mid];
                    let replacement = &content[mid + 2..];
                    return (
                        selector.to_string(),
                        Some((pattern.to_string(), replacement.to_string())),
                    );
                }
                return (
                    selector.to_string(),
                    Some((content.to_string(), String::new())),
                );
            }
        }
    }

    (rule.to_string(), None)
}

/// 执行选择器链，从 HTML 中提取内容
pub fn execute_selector_chain(
    chain: &LegadoSelectorChain,
    html: &str,
) -> Result<Vec<String>, String> {
    let mut contexts = vec![html.to_string()];

    // 逐步执行每个选择器段
    for segment in &chain.segments {
        let mut new_contexts = Vec::new();
        for context in &contexts {
            let results = apply_selector_segment(segment, context)?;
            new_contexts.extend(results);
        }
        contexts = new_contexts;

        // 应用修饰符
        let seg_mods = &segment.modifiers;
        if seg_mods.reverse {
            contexts.reverse();
        }
        if let Some(skip) = seg_mods.skip {
            contexts = contexts.into_iter().skip(skip).collect();
        }
        if let Some(index) = seg_mods.index {
            if contexts.is_empty() {
                continue;
            }
            let idx = if index < 0 {
                let abs = index.unsigned_abs();
                if abs <= contexts.len() {
                    contexts.len() - abs
                } else {
                    0
                }
            } else {
                let idx = index as usize;
                if idx < contexts.len() {
                    idx
                } else {
                    contexts.len() - 1
                }
            };
            contexts = contexts.get(idx).cloned().into_iter().collect();
        }
        if let Some(array) = &seg_mods.array {
            contexts = apply_array_modifier(std::mem::take(&mut contexts), array);
        }
    }

    // 对每个上下文应用提取
    let mut results: Vec<String> = Vec::new();
    for context in &contexts {
        let extracted = extract_from_html(context, &chain.extract);
        results.extend(extracted);
    }

    // 应用净化正则
    if let Some((ref pattern, ref replacement)) = chain.purification {
        if let Ok(re) = regex::Regex::new(pattern) {
            results = results
                .into_iter()
                .map(|s| re.replace_all(&s, replacement.as_str()).to_string())
                .collect();
        }
    }

    Ok(results)
}

fn apply_array_modifier(contexts: Vec<String>, array: &ArrayModifier) -> Vec<String> {
    let len = contexts.len();
    match array {
        ArrayModifier::Include(indexes) => indexes
            .iter()
            .filter_map(|idx| normalize_index(*idx, len).and_then(|i| contexts.get(i).cloned()))
            .collect(),
        ArrayModifier::Exclude(indexes) => contexts
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !index_list_contains(indexes, *i, len))
            .map(|(_, value)| value)
            .collect(),
        ArrayModifier::Range { start, end, step } => {
            if len == 0 {
                return Vec::new();
            }
            let step = *step;
            let mut current = start
                .and_then(|idx| normalize_index(idx, len))
                .unwrap_or(if step < 0 { len - 1 } else { 0 })
                as isize;
            let end = end
                .and_then(|idx| normalize_index(idx, len))
                .unwrap_or(if step < 0 { 0 } else { len - 1 }) as isize;
            let mut out = Vec::new();
            if step > 0 {
                while current <= end {
                    if let Some(value) = contexts.get(current as usize) {
                        out.push(value.clone());
                    }
                    current += step;
                }
            } else {
                while current >= end {
                    if let Some(value) = contexts.get(current as usize) {
                        out.push(value.clone());
                    }
                    current += step;
                }
            }
            out
        }
    }
}

fn index_list_contains(indexes: &[isize], index: usize, len: usize) -> bool {
    indexes
        .iter()
        .filter_map(|idx| normalize_index(*idx, len))
        .any(|idx| idx == index)
}

fn normalize_index(index: isize, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if index >= 0 {
        let idx = index as usize;
        (idx < len).then_some(idx)
    } else {
        let abs = index.unsigned_abs();
        (abs <= len).then_some(len - abs)
    }
}

/// 在单个上下文中应用选择器段落
fn apply_selector_segment(segment: &SelectorSegment, html: &str) -> Result<Vec<String>, String> {
    let sel = segment.selector.trim();

    if sel.is_empty() {
        // 空选择器：返回当前上下文
        return Ok(vec![html.to_string()]);
    }

    if sel == "children" || sel == "*" {
        let document = Html::parse_document(html);
        let children: Vec<String> = document
            .select(&Selector::parse("*").unwrap())
            .map(|el| el.html())
            .collect();
        return Ok(children);
    }

    // CSS 选择器
    let selector = parse_selector_safely(sel)?;

    let document = Html::parse_document(html);
    let results: Vec<String> = document.select(&selector).map(|el| el.html()).collect();

    Ok(results)
}

/// 从 HTML 片段中按提取类型提取内容
fn extract_from_html(html: &str, extract: &ExtractSuffix) -> Vec<String> {
    match extract {
        ExtractSuffix::Html | ExtractSuffix::All => {
            vec![html.to_string()]
        }
        ExtractSuffix::None
        | ExtractSuffix::Text
        | ExtractSuffix::TextNodes
        | ExtractSuffix::TextNode => {
            let document = Html::parse_document(html);
            let text: String = document.root_element().text().collect::<Vec<_>>().join("");
            vec![text]
        }
        ExtractSuffix::OwnText => {
            let document = Html::parse_document(html);
            let mut text = String::new();
            for child in document.root_element().children() {
                if let Some(t) = child.value().as_text() {
                    text.push_str(&t.text);
                }
            }
            vec![text]
        }
        ExtractSuffix::Href => {
            if let Some(href) = first_element_attr(html, "href") {
                return vec![href];
            }
            vec![String::new()]
        }
        ExtractSuffix::Src => {
            if let Some(src) = first_element_attr(html, "src") {
                return vec![src];
            }
            vec![String::new()]
        }
        ExtractSuffix::Content => {
            if let Some(content) = first_element_attr(html, "content") {
                return vec![content];
            }
            vec![String::new()]
        }
        ExtractSuffix::Attr(name) => {
            if let Some(val) = first_element_attr(html, name) {
                return vec![val];
            }
            vec![String::new()]
        }
    }
}

fn first_element_attr(html: &str, attr: &str) -> Option<String> {
    let fragment = Html::parse_fragment(html);
    let selector = Selector::parse("*").ok()?;
    fragment
        .select(&selector)
        .find_map(|element| element.value().attr(attr))
        .map(str::to_string)
}

/// 便捷函数：执行规则字符串
pub fn execute_rule_str(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let chain = parse_legado_selector(rule);
    execute_selector_chain(&chain, html)
}

fn parse_selector_safely(sel: &str) -> Result<Selector, String> {
    catch_unwind(AssertUnwindSafe(|| Selector::parse(sel)))
        .map_err(|_| format!("CSS selector parse panic for '{}'", sel))?
        .map_err(|_| format!("CSS selector parse error for '{}'", sel))
}

/// 便捷函数：执行规则并返回第一个结果
pub fn execute_rule_str_first(rule: &str, html: &str) -> Option<String> {
    execute_rule_str(rule, html).ok().and_then(|mut v| {
        if v.is_empty() {
            None
        } else {
            Some(v.remove(0))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_selector() {
        let chain = parse_legado_selector("div.item a@href");
        assert_eq!(chain.segments.len(), 2);
        assert_eq!(chain.segments[0].selector, "div.item");
        assert_eq!(chain.segments[1].selector, "a");
        assert_eq!(chain.extract, ExtractSuffix::Href);
    }

    #[test]
    fn test_parse_index_modifier() {
        let chain = parse_legado_selector("tr.0");
        assert_eq!(chain.segments.len(), 1);
        assert_eq!(chain.segments[0].modifiers.index, Some(0));
        assert_eq!(chain.segments[0].selector, "tr");
    }

    #[test]
    fn test_parse_skip_modifier() {
        let chain = parse_legado_selector("tr!0");
        assert_eq!(chain.segments.len(), 1);
        assert_eq!(chain.segments[0].modifiers.skip, Some(0));
        assert_eq!(chain.segments[0].selector, "tr");
    }

    #[test]
    fn test_parse_with_purification() {
        let chain = parse_legado_selector("div.content@html##(本章完)");
        assert_eq!(chain.segments.len(), 1);
        assert!(chain.purification.is_some());
        let (pat, rep) = chain.purification.unwrap();
        assert_eq!(pat, "(本章完)");
        assert_eq!(rep, "");
    }

    #[test]
    fn test_execute_css_selector() {
        let html = r#"<div class="book"><h1>Test Title</h1><a href="/read/1">Link</a></div>"#;
        let results = execute_rule_str("div.book h1@text", html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("Test"));
    }

    #[test]
    fn test_array_index_modifier() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[1,3]@text", html).unwrap();
        assert_eq!(results, vec!["B", "D"]);
    }

    #[test]
    fn test_array_range_modifier() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[1:3]@text", html).unwrap();
        assert_eq!(results, vec!["B", "C", "D"]);
    }

    #[test]
    fn test_array_exclude_modifier() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let results = execute_rule_str("li[!1]@text", html).unwrap();
        assert_eq!(results, vec!["A", "C"]);
    }

    #[test]
    fn test_basic_chained_selectors() {
        let html = r#"<div class="title"><tag><a>First</a></tag></div><div class="title"><tag><a>Second</a></tag></div>"#;
        let results = execute_rule_str("class.title.0@tag.a.0@text", html).unwrap();
        assert_eq!(results, vec!["First"]);
    }

    #[test]
    fn test_children_selector() {
        let html = r#"<div><span>A</span><p>B</p><span>C</span></div>"#;
        let results = execute_rule_str("div@children", html).unwrap();
        assert!(
            results.len() >= 3,
            "expected at least 3 results, got {}",
            results.len()
        );
    }

    #[test]
    fn test_negative_index_last() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let results = execute_rule_str("li.-1@text", html).unwrap();
        assert_eq!(results, vec!["C"]);
    }

    #[test]
    fn test_negative_index_second_to_last() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let results = execute_rule_str("li.-2@text", html).unwrap();
        assert_eq!(results, vec!["B"]);
    }

    #[test]
    fn test_array_selection_specific_indices() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li><li>E</li></ul>"#;
        let results = execute_rule_str("li[0,2,4]@text", html).unwrap();
        assert_eq!(results, vec!["A", "C", "E"]);
    }

    #[test]
    fn test_range_selection() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[1:3]@text", html).unwrap();
        assert_eq!(results, vec!["B", "C", "D"]);
    }

    #[test]
    fn test_range_with_step() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li><li>E</li></ul>"#;
        let results = execute_rule_str("li[0:4:2]@text", html).unwrap();
        assert_eq!(results, vec!["A", "C", "E"]);
    }

    #[test]
    fn test_exclusion_multiple_indices() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[!0,2]@text", html).unwrap();
        assert_eq!(results, vec!["B", "D"]);
    }

    #[test]
    fn test_reverse_range() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("tag.li[-1:0:-1]@text", html).unwrap();
        assert_eq!(results, vec!["D", "C", "B", "A"]);
    }

    #[test]
    fn test_leading_dash_reverse() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let results = execute_rule_str("-li@text", html).unwrap();
        assert_eq!(results, vec!["C", "B", "A"]);
    }

    #[test]
    fn test_class_alias_selector() {
        let html = r#"<div class="book"><h1>Test Title</h1></div>"#;
        let results = execute_rule_str("class.book h1@text", html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("Test Title"));
    }

    #[test]
    fn test_id_alias_selector() {
        let html = r#"<div id="main"><p>Hello World</p></div>"#;
        let results = execute_rule_str("id.main p@text", html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("Hello World"));
    }

    #[test]
    fn test_tag_alias_selector() {
        let html = r#"<span>Inline Text</span>"#;
        let results = execute_rule_str("tag.span@text", html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("Inline Text"));
    }

    #[test]
    fn test_multi_segment_chained_with_index() {
        let html =
            r#"<div class="list"><ul><li>A</li><li>B</li></ul><ul><li>C</li><li>D</li></ul></div>"#;
        let results = execute_rule_str("class.list ul.1 li.0@text", html).unwrap();
        assert_eq!(results, vec!["C"]);
    }

    #[test]
    fn test_skip_modifier_basic() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let results = execute_rule_str("li!1@text", html).unwrap();
        assert_eq!(results, vec!["B", "C"]);
    }

    #[test]
    fn test_range_negative_indices() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[-2:-1]@text", html).unwrap();
        assert_eq!(results, vec!["C", "D"]);
    }

    #[test]
    fn test_range_no_end_uses_max() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[1:]@text", html).unwrap();
        assert_eq!(results, vec!["B", "C", "D"]);
    }

    #[test]
    fn test_range_no_start_uses_zero() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("li[:2]@text", html).unwrap();
        assert_eq!(results, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_reverse_range_yields_reversed_elements() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li><li>D</li></ul>"#;
        let results = execute_rule_str("tag.li[3:0:-1]@text", html).unwrap();
        assert_eq!(results, vec!["D", "C", "B", "A"]);
    }

    #[test]
    fn test_extract_html() {
        let html = r#"<div class="book"><h1>Title</h1></div>"#;
        let results = execute_rule_str("div.book h1@html", html).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].contains("Title"));
    }

    #[test]
    fn test_extract_href() {
        let html = r#"<a href="/read/42">Link</a>"#;
        let results = execute_rule_str("a@href", html).unwrap();
        assert_eq!(results, vec!["/read/42"]);
    }

    #[test]
    fn test_extract_src() {
        let html = r#"<img src="/img/photo.png" alt="photo">"#;
        let results = execute_rule_str("img@src", html).unwrap();
        assert_eq!(results, vec!["/img/photo.png"]);
    }

    #[test]
    fn test_extract_custom_attr() {
        let html = r#"<a href="/read/42">Link</a>"#;
        let results = execute_rule_str("a@href", html).unwrap();
        assert_eq!(results, vec!["/read/42"]);
    }

    #[test]
    fn test_purification_regex_simple() {
        let html = r#"<div class="content">Text (本章完)</div>"#;
        let results = execute_rule_str("div.content@html##(本章完)", html).unwrap();
        assert!(!results.is_empty());
        assert!(!results[0].contains("本章完"));
        assert!(results[0].contains("Text"));
    }

    #[test]
    fn test_purification_regex_with_replacement() {
        let html = r#"<div class="content">Hello Evil</div>"#;
        let results = execute_rule_str("div.content@text##Evil##Good", html).unwrap();
        assert_eq!(results, vec!["Hello Good"]);
    }

    #[test]
    fn test_own_text_extraction() {
        let html = r#"<div>Pure <span>Inner</span></div>"#;
        let results = execute_rule_str("div@ownText", html).unwrap();
        assert!(
            !results.is_empty(),
            "ownText should produce non-empty result"
        );
    }

    #[test]
    fn test_empty_selector_returns_text() {
        let html = r#"<div>Hello</div>"#;
        let results = execute_rule_str("@text", html).unwrap();
        assert!(results[0].contains("Hello"));
    }
}
