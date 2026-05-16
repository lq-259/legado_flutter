//! Legado 规则执行引擎
//!
//! 统一规则执行入口，支持：
//! - JSOUP Default 选择器链
//! - CSS 选择器（@css: 前缀）
//! - XPath 表达式（@XPath: 或 // 前缀）
//! - JSONPath 表达式（@json: 或 $. 前缀）
//! - 正则表达式（/pattern/ 或 regex: 前缀）
//! - JavaScript 脚本（@js: 或 js: 前缀）
//! - AllInOne 正则（: 前缀）
//! - 规则组合：&&, ||, %%
//!
//! 对应 Legado 的 AnalyzeRule.kt

use super::context::RuleContext;
use super::js_runtime::{self, JsRuntime};
use super::regex_rule;
use super::selector;
use super::value::LegadoValue;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::LazyLock;

use regex::Regex;
use std::sync::Arc;

/// 规则执行结果类型
#[derive(Debug, Clone)]
pub enum RuleResult {
    /// 字符串列表
    Strings(Vec<String>),
    /// 结构化值列表
    Values(Vec<LegadoValue>),
}

impl RuleResult {
    pub fn is_empty(&self) -> bool {
        match self {
            RuleResult::Strings(s) => s.is_empty(),
            RuleResult::Values(v) => v.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            RuleResult::Strings(s) => s.len(),
            RuleResult::Values(v) => v.len(),
        }
    }

    pub fn first_string(&self) -> Option<String> {
        match self {
            RuleResult::Strings(s) => s.first().cloned(),
            RuleResult::Values(v) => v.first().map(|val| val.as_string_lossy()),
        }
    }

    pub fn into_strings(self) -> Vec<String> {
        match self {
            RuleResult::Strings(s) => s,
            RuleResult::Values(v) => v.into_iter().map(|val| val.as_string_lossy()).collect(),
        }
    }
}

/// 执行 Legado 规则字符串，返回字符串列表
///
/// # Arguments
/// * `rule_str` - 规则字符串
/// * `html` - HTML 或 JSON 内容
/// * `context` - 执行上下文（baseUrl, result, src 等）
pub fn execute_legado_rule(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
) -> Result<Vec<String>, String> {
    let rule_str = rule_str.trim();
    if rule_str.is_empty() {
        return Ok(vec![html.to_string()]);
    }

    if contains_combinator(rule_str) {
        return execute_combinator_rule(rule_str, html, context);
    }

    if rule_str.starts_with("@get:") || rule_str.starts_with("@get.") {
        return execute_get_rule(rule_str, context);
    }

    if rule_str.starts_with("@put:") {
        let mut child_context = context.clone();
        return execute_put_rule(rule_str, html, &mut child_context);
    }

    if contains_inline_js(rule_str) {
        return execute_inline_js_rule(rule_str, html, context);
    }

    // 检测规则类型
    if rule_str.starts_with('@') {
        return execute_prefixed_rule(rule_str, html, context);
    }

    // JS 规则
    if rule_str.starts_with("js:") {
        return execute_js_rule(&rule_str[3..], html, context);
    }

    // 正则规则
    if rule_str.starts_with('/') || rule_str.starts_with("regex:") {
        return execute_regex_rule(rule_str, html);
    }

    // XPath
    if rule_str.starts_with("//") {
        return execute_xpath_rule(rule_str, html);
    }

    // JSONPath
    if rule_str.starts_with("$.") || rule_str.starts_with("$[") {
        return execute_jsonpath_rule(rule_str, html);
    }

    // AllInOne 正则
    if rule_str.starts_with(':') {
        return execute_all_in_one_rule(rule_str, html);
    }

    // CSS 选择器（如果以 // 开头的已经走 XPath 了）
    if looks_like_xpath_function(rule_str) {
        return execute_xpath_rule(rule_str, html);
    }

    // Default: JSOUP Default 选择器链
    execute_default_rule(rule_str, html)
}

/// Execute a Legado rule and preserve structured JS/JSON values when possible.
pub fn execute_legado_rule_values(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
) -> Result<Vec<LegadoValue>, String> {
    let rule_str = rule_str.trim();
    if rule_str.starts_with("@js:") || rule_str.starts_with("js:") {
        let script = rule_str
            .strip_prefix("@js:")
            .or_else(|| rule_str.strip_prefix("js:"))
            .unwrap_or(rule_str);
        let vars = js_runtime::build_runtime_vars(context, html);
        let runtime = js_runtime::DefaultJsRuntime::new();
        return match runtime.eval(script, &vars)? {
            LegadoValue::Null => Ok(vec![]),
            LegadoValue::Array(values) => Ok(values),
            other => Ok(vec![other]),
        };
    }

    execute_legado_rule(rule_str, html, context)
        .map(|values| values.into_iter().map(LegadoValue::String).collect())
}

pub fn execute_legado_rule_values_with_cookie_jar(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
    cookie_jar: Arc<reqwest::cookie::Jar>,
) -> Result<Vec<LegadoValue>, String> {
    execute_legado_rule_values_with_http_state(rule_str, html, context, cookie_jar, Vec::new())
}

pub fn execute_legado_rule_values_with_http_state(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
    cookie_jar: Arc<reqwest::cookie::Jar>,
    default_headers: Vec<(String, String)>,
) -> Result<Vec<LegadoValue>, String> {
    let rule_str = rule_str.trim();
    if rule_str.starts_with("@js:") || rule_str.starts_with("js:") {
        let script = rule_str
            .strip_prefix("@js:")
            .or_else(|| rule_str.strip_prefix("js:"))
            .unwrap_or(rule_str);
        let vars = js_runtime::build_runtime_vars(context, html);
        return match js_runtime::eval_default_with_http_state(
            script,
            &vars,
            cookie_jar,
            default_headers,
        )? {
            LegadoValue::Null => Ok(vec![]),
            LegadoValue::Array(values) => Ok(values),
            other => Ok(vec![other]),
        };
    }

    execute_legado_rule(rule_str, html, context)
        .map(|values| values.into_iter().map(LegadoValue::String).collect())
}

pub fn execute_legado_rule_with_cookie_jar(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
    cookie_jar: Arc<reqwest::cookie::Jar>,
) -> Result<Vec<String>, String> {
    execute_legado_rule_with_http_state(rule_str, html, context, cookie_jar, Vec::new())
}

pub fn execute_legado_rule_with_http_state(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
    cookie_jar: Arc<reqwest::cookie::Jar>,
    default_headers: Vec<(String, String)>,
) -> Result<Vec<String>, String> {
    let rule_str = rule_str.trim();
    if rule_str.starts_with("@js:") || rule_str.starts_with("js:") {
        return execute_legado_rule_values_with_http_state(
            rule_str,
            html,
            context,
            cookie_jar,
            default_headers,
        )
        .map(|values| {
            values
                .into_iter()
                .map(|value| value.as_string_lossy())
                .collect()
        });
    }

    execute_legado_rule(rule_str, html, context)
}

/// 执行带前缀的规则（@css:, @XPath:, @json:, @js:）
fn execute_prefixed_rule(
    rule_str: &str,
    html: &str,
    context: &RuleContext,
) -> Result<Vec<String>, String> {
    if let Some(expr) = strip_css_prefix_case_insensitive(rule_str) {
        return execute_css_rule(expr, html);
    }
    if let Some(expr) = rule_str.strip_prefix("@XPath:") {
        return execute_xpath_rule(expr, html);
    }
    if let Some(expr) = rule_str.strip_prefix("@json:") {
        return execute_jsonpath_rule(expr, html);
    }
    if let Some(expr) = rule_str.strip_prefix("@js:") {
        return execute_js_rule(expr, html, context);
    }
    // 其他 @ 前缀：可能是 Default 选择器链
    execute_default_rule(rule_str, html)
}

/// Case‑insensitive @css: / @CSS: prefix
fn strip_css_prefix_case_insensitive(rule_str: &str) -> Option<&str> {
    if rule_str.len() < 5 {
        return None;
    }
    let prefix = &rule_str[..5];
    if prefix.eq_ignore_ascii_case("@css:") {
        Some(&rule_str[5..])
    } else {
        None
    }
}

/// 执行 CSS 规则
fn execute_css_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let selector_str = rule.split("##").next().unwrap_or(rule).trim();

    if selector_str.is_empty() {
        return Ok(vec![]);
    }

    let mut results = Vec::new();

    // 处理 || 组合
    if selector_str.contains("||") {
        for part in selector_str.split("||") {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match execute_single_css(part, html) {
                Ok(mut r) => results.append(&mut r),
                Err(_) => {}
            }
            if !results.is_empty() {
                break;
            }
        }
    } else {
        results = execute_single_css(selector_str, html)?;
    }

    // 应用净化
    if let Some(purification) = regex_rule::parse_purification(rule) {
        results = results
            .into_iter()
            .map(|s| regex_rule::apply_purification(&s, &purification))
            .collect();
    }

    Ok(results)
}

fn execute_single_css(selector_str: &str, html: &str) -> Result<Vec<String>, String> {
    let (selector_str, output) = split_css_output(selector_str);
    // Strip any legacy @css: prefix that may remain from || combined rules
    let selector_str = strip_css_prefix_case_insensitive(selector_str).unwrap_or(selector_str);

    // Pre‑process JSOUP pseudo‑selectors that scraper does not support.
    // We strip them from the CSS string, let scraper parse the clean selector,
    // then apply the JSOUP filters on the result set in Rust.
    let (clean_css, jsoup_filters) = extract_jsoup_pseudos(selector_str);

    let selector = parse_css_selector_safely(&clean_css)?;
    let document = scraper::Html::parse_document(html);
    let mut results: Vec<String> = document
        .select(&selector)
        .map(|el| match output {
            Some("text") | Some("textNodes") => el.text().collect::<String>(),
            Some("ownText") => el.text().collect::<String>(),
            Some("html") | Some("all") | None => el.inner_html(),
            Some(attr) => el.value().attr(attr).unwrap_or_default().to_string(),
        })
        .collect();

    // Apply JSOUP pseudo‑selector filters in order
    for filter in jsoup_filters {
        apply_jsoup_filter(&mut results, &filter);
    }

    Ok(results)
}

/// Extracts JSOUP pseudo‑selector fragments that scraper does not support,
/// returning the cleaned CSS string and the list of filters to apply.
fn extract_jsoup_pseudos(css: &str) -> (String, Vec<JsoupPseudo>) {
    static JSOUP_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#":(contains|eq|lt|gt|matchText|matches)\(([^)]*)\)"#).unwrap()
    });
    let mut clean = String::with_capacity(css.len());
    let mut filters = Vec::new();
    let mut last_end = 0;
    for caps in JSOUP_RE.captures_iter(css) {
        let m = caps.get(0).unwrap();
        clean.push_str(&css[last_end..m.start()]);
        last_end = m.end();
        let kind = caps.get(1).unwrap().as_str();
        let arg = caps.get(2).unwrap().as_str();
        filters.push(match kind {
            "contains" => JsoupPseudo::Contains(arg.to_string()),
            "eq" => JsoupPseudo::Eq(arg.parse().unwrap_or(0)),
            "lt" => JsoupPseudo::Lt(arg.parse().unwrap_or(0)),
            "gt" => JsoupPseudo::Gt(arg.parse().unwrap_or(0)),
            "matchText" => JsoupPseudo::MatchText(arg.to_string()),
            "matches" => JsoupPseudo::Matches(arg.to_string()),
            _ => continue,
        });
    }
    clean.push_str(&css[last_end..]);
    (clean, filters)
}

#[derive(Debug, Clone)]
enum JsoupPseudo {
    Contains(String),
    Eq(usize),
    Lt(usize),
    Gt(usize),
    MatchText(String),
    Matches(String),
}

fn apply_jsoup_filter(results: &mut Vec<String>, filter: &JsoupPseudo) {
    match filter {
        JsoupPseudo::Contains(text) => {
            results.retain(|s| s.contains(text.as_str()));
        }
        JsoupPseudo::Eq(idx) => {
            let val = results.get(*idx).cloned();
            results.clear();
            if let Some(v) = val {
                results.push(v);
            }
        }
        JsoupPseudo::Lt(idx) => {
            if *idx < results.len() {
                results.truncate(*idx);
            }
        }
        JsoupPseudo::Gt(idx) => {
            let skip = (*idx + 1).min(results.len());
            *results = results[skip..].to_vec();
        }
        JsoupPseudo::MatchText(re) | JsoupPseudo::Matches(re) => {
            if let Ok(regex) = Regex::new(re) {
                results.retain(|s| regex.is_match(s));
            }
        }
    }
}

fn parse_css_selector_safely(selector_str: &str) -> Result<scraper::Selector, String> {
    catch_unwind(AssertUnwindSafe(|| scraper::Selector::parse(selector_str)))
        .map_err(|_| format!("CSS parse panic for '{}'", selector_str))?
        .map_err(|_| format!("CSS parse error for '{}'", selector_str))
}

fn split_css_output(rule: &str) -> (&str, Option<&str>) {
    const OUTPUTS: &[&str] = &[
        "text",
        "textNodes",
        "ownText",
        "html",
        "all",
        "href",
        "src",
        "content",
    ];
    if let Some((selector, output)) = rule.rsplit_once('@') {
        if OUTPUTS.contains(&output) || !output.is_empty() {
            return (selector.trim(), Some(output.trim()));
        }
    }
    (rule.trim(), None)
}

/// 执行 XPath 规则
fn execute_xpath_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let expr_str = rule.split("##").next().unwrap_or(rule).trim();
    if expr_str.is_empty() {
        return Ok(vec![]);
    }

    let package =
        sxd_document::parser::parse(html).map_err(|e| format!("HTML parse error: {}", e))?;
    let document = package.as_document();

    let xpath = sxd_xpath::Factory::new()
        .build(expr_str)
        .map_err(|e| format!("XPath compile error: {}", e))?
        .ok_or_else(|| "XPath expression returned None".to_string())?;

    let context = sxd_xpath::Context::new();
    let value = xpath
        .evaluate(&context, document.root())
        .map_err(|e| format!("XPath eval error: {}", e))?;

    let results: Vec<String> = match value {
        sxd_xpath::Value::Nodeset(nodes) => nodes.iter().map(|node| node.string_value()).collect(),
        sxd_xpath::Value::Boolean(b) => vec![b.to_string()],
        sxd_xpath::Value::Number(n) => vec![n.to_string()],
        sxd_xpath::Value::String(s) => vec![s],
    };

    // 应用净化
    if let Some(purification) = regex_rule::parse_purification(rule) {
        return Ok(results
            .into_iter()
            .map(|s| regex_rule::apply_purification(&s, &purification))
            .collect());
    }

    Ok(results)
}

/// 执行 JSONPath 规则
fn execute_jsonpath_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let expr_str = rule.split("##").next().unwrap_or(rule).trim();

    // 尝试解析 HTML 为 JSON
    let json_val: serde_json::Value = serde_json::from_str(html).unwrap_or(serde_json::Value::Null);

    if json_val.is_null() {
        return Ok(vec![]);
    }

    let results =
        jsonpath_lib::select(&json_val, expr_str).map_err(|e| format!("JSONPath error: {}", e))?;

    let mut strings = Vec::new();
    for value in results {
        match value {
            serde_json::Value::Array(items) => {
                strings.extend(items.iter().map(|item| match item {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                }));
            }
            serde_json::Value::String(s) => strings.push(s.clone()),
            other => strings.push(other.to_string()),
        }
    }

    // 应用净化
    if let Some(purification) = regex_rule::parse_purification(rule) {
        return Ok(strings
            .into_iter()
            .map(|s| regex_rule::apply_purification(&s, &purification))
            .collect());
    }

    Ok(strings)
}

/// 执行 JS 规则（委托给 JS runtime）
fn execute_js_rule(rule: &str, html: &str, context: &RuleContext) -> Result<Vec<String>, String> {
    let vars = js_runtime::build_runtime_vars(context, html);
    let runtime = js_runtime::DefaultJsRuntime::new();
    let value = runtime.eval(rule, &vars)?;
    match value {
        LegadoValue::Null => Ok(vec![]),
        LegadoValue::Array(values) => Ok(values.into_iter().map(|v| v.as_string_lossy()).collect()),
        other => Ok(vec![other.as_string_lossy()]),
    }
}

fn execute_get_rule(rule: &str, context: &RuleContext) -> Result<Vec<String>, String> {
    let key = rule
        .strip_prefix("@get:")
        .or_else(|| rule.strip_prefix("@get."))
        .unwrap_or_default()
        .trim();
    let key = key
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(key)
        .trim();
    if key.is_empty() {
        return Ok(vec![]);
    }
    let value = context.get_variable(key);
    match value {
        LegadoValue::Null => Ok(vec![]),
        LegadoValue::Array(values) => Ok(values.into_iter().map(|v| v.as_string_lossy()).collect()),
        other => Ok(vec![other.as_string_lossy()]),
    }
}

fn execute_put_rule(
    rule: &str,
    html: &str,
    context: &mut RuleContext,
) -> Result<Vec<String>, String> {
    let expr = rule.strip_prefix("@put:").unwrap_or_default().trim();
    if expr.is_empty() {
        return Ok(vec![]);
    }

    if let Some((key, value_rule)) = expr.split_once('=') {
        let key = key.trim().trim_matches(|c| c == '"' || c == '\'');
        let value_rule = value_rule.trim().trim_matches(|c| c == '"' || c == '\'');
        let values = execute_legado_rule(value_rule, html, context)?;
        context.set_variable(key.to_string(), values_to_legado_value(values));
        return Ok(vec![]);
    }

    let json_like = expr.trim().trim_start_matches('{').trim_end_matches('}');
    for pair in json_like.split(',') {
        let Some((key, value_rule)) = pair.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches(|c| c == '"' || c == '\'');
        let value_rule = value_rule.trim().trim_matches(|c| c == '"' || c == '\'');
        if key.is_empty() || value_rule.is_empty() {
            continue;
        }
        let values = execute_legado_rule(value_rule, html, context)?;
        context.set_variable(key.to_string(), values_to_legado_value(values));
    }
    Ok(vec![])
}

fn values_to_legado_value(values: Vec<String>) -> LegadoValue {
    if values.len() == 1 {
        LegadoValue::String(values[0].clone())
    } else {
        LegadoValue::Array(values.into_iter().map(LegadoValue::String).collect())
    }
}

fn contains_inline_js(rule: &str) -> bool {
    rule.contains("<js>") && rule.contains("</js>")
}

fn execute_inline_js_rule(
    rule: &str,
    html: &str,
    context: &RuleContext,
) -> Result<Vec<String>, String> {
    let Some(start) = rule.find("<js>") else {
        return execute_legado_rule(rule, html, context);
    };
    let script_start = start + "<js>".len();
    let Some(end_offset) = rule[script_start..].find("</js>") else {
        return execute_legado_rule(rule, html, context);
    };
    let script_end = script_start + end_offset;
    let before = rule[..start].trim();
    let script = rule[script_start..script_end].trim();
    let after = rule[script_end + "</js>".len()..].trim();

    let input = if before.is_empty() {
        vec![html.to_string()]
    } else {
        execute_legado_rule(before, html, context)?
    };

    let mut js_results = Vec::new();
    let runtime = js_runtime::DefaultJsRuntime::new();
    for item in input {
        let mut child_context = context.clone();
        child_context.src = if context.src.is_empty() {
            html.to_string()
        } else {
            context.src.clone()
        };
        child_context.result = vec![LegadoValue::String(item)];
        let vars = js_runtime::build_runtime_vars(&child_context, html);
        let value = runtime.eval(script, &vars)?;
        match value {
            LegadoValue::Null => {}
            LegadoValue::Array(values) => {
                js_results.extend(values.into_iter().map(|v| v.as_string_lossy()));
            }
            other => js_results.push(other.as_string_lossy()),
        }
    }

    if after.is_empty() {
        return Ok(js_results);
    }

    let mut out = Vec::new();
    let after = if after.starts_with("@css:")
        || after.starts_with("@json:")
        || after.starts_with("@XPath:")
        || after.starts_with("@js:")
    {
        after
    } else {
        after.strip_prefix('@').unwrap_or(after)
    };
    for item in js_results {
        out.extend(execute_legado_rule(after, &item, context)?);
    }
    Ok(out)
}

/// 执行正则规则
fn execute_regex_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let pattern = if let Some(expr) = rule.strip_prefix("regex:") {
        expr.trim()
    } else {
        rule.trim()
    };

    // 支持 /pattern/flags 格式
    let (pattern, flags) = if pattern.starts_with('/') {
        let rest = &pattern[1..];
        if let Some(last_slash) = rest.rfind('/') {
            (&rest[..last_slash], &rest[last_slash + 1..])
        } else {
            (rest, "")
        }
    } else {
        (pattern, "")
    };

    let mut builder = regex::RegexBuilder::new(pattern);
    if flags.contains('i') {
        builder.case_insensitive(true);
    }
    if flags.contains('s') {
        builder.dot_matches_new_line(true);
    }
    if flags.contains('m') {
        builder.multi_line(true);
    }

    let re = builder
        .build()
        .map_err(|e| format!("Regex compile error: {}", e))?;

    let results: Vec<String> = re
        .captures_iter(html)
        .map(|cap| {
            if cap.len() > 1 {
                cap[1].to_string()
            } else {
                cap[0].to_string()
            }
        })
        .collect();

    Ok(results)
}

/// 执行 AllInOne 正则规则
fn execute_all_in_one_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let all_in_one = regex_rule::parse_all_in_one(rule)?;
    let rows = regex_rule::execute_all_in_one(&all_in_one, html);
    // 展平为字符串列表
    let strings: Vec<String> = rows.into_iter().flat_map(|row| row).collect();
    Ok(strings)
}

/// 执行 JSOUP Default 选择器链
fn execute_default_rule(rule: &str, html: &str) -> Result<Vec<String>, String> {
    let chain = selector::parse_legado_selector(rule);
    selector::execute_selector_chain(&chain, html)
}

/// 检测规则是否包含组合符
fn contains_combinator(rule: &str) -> bool {
    // 简单检测：不在引号或括号内的 || / && / %%
    let mut in_bracket = 0;
    let mut in_quote = false;
    let mut i = 0;
    let chars: Vec<char> = rule.chars().collect();

    while i < chars.len() {
        match chars[i] {
            '"' => in_quote = !in_quote,
            '[' => in_bracket += 1,
            ']' => {
                if in_bracket > 0 {
                    in_bracket -= 1
                }
            }
            '|' if !in_quote && in_bracket == 0 => {
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    return true;
                }
            }
            '&' if !in_quote && in_bracket == 0 => {
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    return true;
                }
            }
            '%' if !in_quote && in_bracket == 0 => {
                if i + 1 < chars.len() && chars[i + 1] == '%' {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// 执行组合规则（||, &&, %%）
fn execute_combinator_rule(
    rule: &str,
    html: &str,
    context: &RuleContext,
) -> Result<Vec<String>, String> {
    // 分离组合符
    let parts = split_combinator(rule);
    let mut shared_context = context.clone();

    if rule.contains("%%") {
        // 交错合并
        let all_results: Vec<Vec<String>> = parts
            .iter()
            .map(|p| {
                execute_rule_part_with_context(p, html, &mut shared_context).unwrap_or_default()
            })
            .collect();

        let max_len = all_results.iter().map(|v| v.len()).max().unwrap_or(0);
        let mut merged = Vec::new();
        for i in 0..max_len {
            for results in &all_results {
                if let Some(s) = results.get(i) {
                    merged.push(s.clone());
                }
            }
        }
        Ok(merged)
    } else if rule.contains("||") {
        // 第一个非空结果
        for part in &parts {
            match execute_rule_part_with_context(part, html, &mut shared_context) {
                Ok(results) if !results.is_empty() => return Ok(results),
                _ => continue,
            }
        }
        Ok(vec![])
    } else if rule.contains("&&") {
        // 合并所有结果
        let mut merged = Vec::new();
        for part in &parts {
            if let Ok(mut results) = execute_rule_part_with_context(part, html, &mut shared_context)
            {
                merged.append(&mut results);
            }
        }
        Ok(merged)
    } else {
        // 单一规则
        execute_legado_rule(rule, html, context)
    }
}

fn execute_rule_part_with_context(
    rule: &str,
    html: &str,
    context: &mut RuleContext,
) -> Result<Vec<String>, String> {
    let trimmed = rule.trim();
    if trimmed.starts_with("@put:") {
        execute_put_rule(trimmed, html, context)
    } else if trimmed.starts_with("@get:") || trimmed.starts_with("@get.") {
        execute_get_rule(trimmed, context)
    } else {
        execute_legado_rule(trimmed, html, context)
    }
}

/// 分割组合符
fn split_combinator(rule: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_bracket = 0;
    let mut in_quote = false;
    let chars: Vec<char> = rule.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '"' => in_quote = !in_quote,
            '[' => in_bracket += 1,
            ']' if in_bracket > 0 => in_bracket -= 1,
            '|' if !in_quote && in_bracket == 0 && i + 1 < chars.len() && chars[i + 1] == '|' => {
                parts.push(current.trim().to_string());
                current.clear();
                i += 1; // skip second |
            }
            '&' if !in_quote && in_bracket == 0 && i + 1 < chars.len() && chars[i + 1] == '&' => {
                parts.push(current.trim().to_string());
                current.clear();
                i += 1;
            }
            '%' if !in_quote && in_bracket == 0 && i + 1 < chars.len() && chars[i + 1] == '%' => {
                parts.push(current.trim().to_string());
                current.clear();
                i += 1;
            }
            _ => current.push(chars[i]),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_js_after_css_rule() {
        let html = r#"<div><a>Book</a></div>"#;
        let context = RuleContext::new("https://example.com", html);
        let result =
            execute_legado_rule("@css:a@text<js>result + '-ok'</js>", html, &context).unwrap();
        assert_eq!(result, vec!["Book-ok"]);
    }

    #[test]
    fn test_inline_js_as_rule() {
        let html = "source";
        let context = RuleContext::new("https://example.com", html);
        let result = execute_legado_rule("<js>src + '-ok'</js>", html, &context).unwrap();
        assert_eq!(result, vec!["source-ok"]);
    }

    #[test]
    fn test_inline_js_before_following_rule() {
        let html = r#"<div><span>A</span></div>"#;
        let context = RuleContext::new("https://example.com", html);
        let result = execute_legado_rule(
            "@css:div@html<js>result.replace('span', 'a').replace('span', 'a')</js>@css:a@text",
            html,
            &context,
        )
        .unwrap();
        assert_eq!(result, vec!["A"]);
    }

    #[test]
    fn test_put_get_in_combinator_rule() {
        let html = r#"<div><a>Book</a></div>"#;
        let context = RuleContext::new("https://example.com", html);
        let result =
            execute_legado_rule("@put:title=@css:a@text&&@get:title", html, &context).unwrap();
        assert_eq!(result, vec!["Book"]);
    }

    #[test]
    fn test_get_from_context_variables() {
        let html = "";
        let mut context = RuleContext::new("https://example.com", html);
        context.set_variable("token", LegadoValue::String("abc".into()));
        let result = execute_legado_rule("@get:token", html, &context).unwrap();
        assert_eq!(result, vec!["abc"]);
    }

    #[test]
    fn test_get_brace_syntax_from_context_variables() {
        let html = "";
        let mut context = RuleContext::new("https://example.com", html);
        context.set_variable("token", LegadoValue::String("abc".into()));
        let result = execute_legado_rule("@get:{token}", html, &context).unwrap();
        assert_eq!(result, vec!["abc"]);
    }
}
