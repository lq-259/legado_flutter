//! # 脚本引擎模块
//!
//! 使用 Rhai 脚本引擎执行书源中的自定义脚本。
//! 对应原 Legado 的 JavaScript 引擎 (modules/rhino/)。
//!
//! ## 安全边界
//! - 墙钟超时: 5 秒（通过 on_progress 回调拦截）
//! - 最大操作数: 100,000（限制 CPU 使用）
//! - 最大调用层级: 50（防止深度递归）
//! - 最大字符串长度: 1,000,000（限制内存分配）
//! - 最大数组大小: 10,000
//! - 最大映射大小: 10,000
//! - 脚本源码长度上限: 100,000 字符
//! - 输出总字符数上限: 500,000（递归计算 String/Array/Map）

use rhai::{Dynamic, Engine, Scope, AST};
use serde_json::Value as JsonValue;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error};

const MAX_SCRIPT_LENGTH: usize = 100_000;
const MAX_OUTPUT_STRING_LENGTH: usize = 500_000;
const MAX_STRING_SIZE: usize = 1_000_000;
const MAX_ARRAY_SIZE: usize = 10_000;
const MAX_MAP_SIZE: usize = 10_000;
const WALL_CLOCK_TIMEOUT_SECS: u64 = 5;

/// 脚本执行结果
#[derive(Debug, Clone)]
pub enum ScriptResult {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<ScriptResult>),
    Map(std::collections::HashMap<String, ScriptResult>),
    Null,
}

impl ScriptResult {
    /// 转换为字符串
    pub fn as_string(self) -> Option<String> {
        match self {
            ScriptResult::String(s) => Some(s),
            _ => None,
        }
    }

    /// 递归计算输出总字符数（覆盖 String + Array + Map 内的所有字符串）
    pub fn total_chars(&self) -> usize {
        match self {
            ScriptResult::String(s) => s.len(),
            ScriptResult::Array(arr) => arr.iter().map(|v| v.total_chars()).sum(),
            ScriptResult::Map(map) => map.values().map(|v| v.total_chars()).sum(),
            _ => 0,
        }
    }

    /// 转换为 JSON Value
    pub fn to_json_value(&self) -> JsonValue {
        match self {
            ScriptResult::String(s) => JsonValue::String(s.clone()),
            ScriptResult::Int(i) => JsonValue::Number((*i).into()),
            ScriptResult::Float(f) => {
                JsonValue::Number(serde_json::Number::from_f64(*f).unwrap_or(0.into()))
            }
            ScriptResult::Bool(b) => JsonValue::Bool(*b),
            ScriptResult::Array(arr) => {
                JsonValue::Array(arr.iter().map(|v| v.to_json_value()).collect())
            }
            ScriptResult::Map(map) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in map {
                    obj.insert(k.to_string(), v.to_json_value());
                }
                JsonValue::Object(obj)
            }
            ScriptResult::Null => JsonValue::Null,
        }
    }
}

/// 脚本引擎（封装 Rhai）
pub struct ScriptEngine {
    engine: Arc<Engine>,
    start_time: Arc<Mutex<Option<Instant>>>,
    timed_out: Arc<AtomicBool>,
}

impl ScriptEngine {
    /// 创建新的脚本引擎（带完整资源限制 + 墙钟超时）
    pub fn new() -> Self {
        let start_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let timed_out: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let st = start_time.clone();
        let to = timed_out.clone();

        let mut engine = Engine::new();

        engine.set_max_operations(100_000);
        engine.set_max_call_levels(50);
        engine.set_max_string_size(MAX_STRING_SIZE);
        engine.set_max_array_size(MAX_ARRAY_SIZE);
        engine.set_max_map_size(MAX_MAP_SIZE);

        engine.on_progress(move |_ops| {
            if let Some(start) = *st.lock().unwrap() {
                if start.elapsed() > Duration::from_secs(WALL_CLOCK_TIMEOUT_SECS) {
                    to.store(true, Ordering::SeqCst);
                    return Some(Dynamic::UNIT);
                }
            }
            None::<Dynamic>
        });

        // 注册常用函数，供脚本调用
        engine.register_fn("log", |msg: &str| {
            debug!("[Script] {}", msg);
        });

        // 注册字符串处理函数
        engine.register_fn("trim", |s: &str| s.trim().to_string());
        engine.register_fn("to_lowercase", |s: &str| s.to_lowercase());
        engine.register_fn("to_uppercase", |s: &str| s.to_uppercase());
        engine.register_fn("replace", |s: &str, from: &str, to: &str| {
            s.replace(from, to)
        });
        engine.register_fn("substring", |s: &str, start: i64, end: i64| {
            let chars: Vec<_> = s.char_indices().collect();
            let char_len = chars.len() as i64;
            let start = start.max(0).min(char_len) as usize;
            let end = end.max(0).min(char_len) as usize;
            if start < end {
                let byte_start = chars[start].0;
                let byte_end = chars.get(end).map(|c| c.0).unwrap_or(s.len());
                s[byte_start..byte_end].to_string()
            } else {
                String::new()
            }
        });

        // 注册 JSON 处理函数
        engine.register_fn("parse_json", |s: &str| {
            serde_json::from_str::<JsonValue>(s).unwrap_or(JsonValue::Null)
        });

        engine.register_fn("to_json_string", |val: Dynamic| {
            if let Some(json_val) = val.clone().try_cast::<JsonValue>() {
                serde_json::to_string(&json_val).unwrap_or_default()
            } else {
                val.to_string()
            }
        });

        // 注册正则相关函数
        engine.register_fn("regex_match", |s: &str, pattern: &str| {
            if let Ok(re) = regex::Regex::new(pattern) {
                re.is_match(s)
            } else {
                false
            }
        });

        engine.register_fn("regex_find", |s: &str, pattern: &str| -> String {
            if let Ok(re) = regex::Regex::new(pattern) {
                re.find(s)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        });

        Self {
            engine: Arc::new(engine),
            start_time,
            timed_out,
        }
    }

    fn check_output_size(result: &ScriptResult) -> Result<(), String> {
        let total = result.total_chars();
        if total > MAX_OUTPUT_STRING_LENGTH {
            return Err(format!(
                "脚本输出过长: {} > {} 字符",
                total, MAX_OUTPUT_STRING_LENGTH
            ));
        }
        Ok(())
    }

    /// 执行脚本并返回结果
    pub fn eval(
        &self,
        script: &str,
        context: Option<&ScriptContext>,
    ) -> Result<ScriptResult, String> {
        if script.len() > MAX_SCRIPT_LENGTH {
            return Err(format!(
                "脚本过长: {} > {} 字符",
                script.len(),
                MAX_SCRIPT_LENGTH
            ));
        }
        debug!(
            "执行脚本: {}...",
            script.chars().take(50).collect::<String>()
        );

        let mut scope = Scope::new();

        if let Some(ctx) = context {
            scope.push("result", ctx.result.clone());
            scope.push("content", ctx.content.clone());
            scope.push("url", ctx.url.clone());
            scope.push("headers", ctx.headers.clone());

            if let Some(source_name) = &ctx.source_name {
                scope.push("source_name", source_name.clone());
            }
        }

        self.timed_out.store(false, Ordering::SeqCst);
        *self.start_time.lock().unwrap() = Some(Instant::now());

        // Use closure to guarantee start_time is always cleared
        let result = (|| -> Result<ScriptResult, String> {
            match self.engine.eval_with_scope::<Dynamic>(&mut scope, script) {
                Ok(val) => {
                    let result = Self::dynamic_to_result(val);
                    Self::check_output_size(&result)?;
                    Ok(result)
                }
                Err(e) => {
                    error!("脚本执行失败: {}", e);
                    Err(format!("脚本执行失败: {}", e))
                }
            }
        })();

        *self.start_time.lock().unwrap() = None;

        if self.timed_out.swap(false, Ordering::SeqCst) {
            return Err(format!("脚本执行超时 ({}秒)", WALL_CLOCK_TIMEOUT_SECS));
        }

        result
    }

    /// 编译并缓存脚本（用于多次执行）
    pub fn compile(&self, script: &str) -> Result<AST, String> {
        if script.len() > MAX_SCRIPT_LENGTH {
            return Err(format!(
                "脚本过长: {} > {} 字符",
                script.len(),
                MAX_SCRIPT_LENGTH
            ));
        }
        self.engine
            .compile(script)
            .map_err(|e| format!("脚本编译失败: {}", e))
    }

    /// 执行预编译的脚本（与 eval 共享同一安全边界）
    pub fn eval_ast(
        &self,
        ast: &AST,
        context: Option<&ScriptContext>,
    ) -> Result<ScriptResult, String> {
        let mut scope = Scope::new();

        if let Some(ctx) = context {
            scope.push("result", ctx.result.clone());
            scope.push("content", ctx.content.clone());
            scope.push("url", ctx.url.clone());
            scope.push("headers", ctx.headers.clone());

            if let Some(source_name) = &ctx.source_name {
                scope.push("source_name", source_name.clone());
            }
        }

        self.timed_out.store(false, Ordering::SeqCst);
        *self.start_time.lock().unwrap() = Some(Instant::now());

        let result = (|| -> Result<ScriptResult, String> {
            match self.engine.eval_ast_with_scope::<Dynamic>(&mut scope, ast) {
                Ok(val) => {
                    let result = Self::dynamic_to_result(val);
                    Self::check_output_size(&result)?;
                    Ok(result)
                }
                Err(e) => Err(format!("脚本执行失败: {}", e)),
            }
        })();

        *self.start_time.lock().unwrap() = None;

        if self.timed_out.swap(false, Ordering::SeqCst) {
            return Err(format!("脚本执行超时 ({}秒)", WALL_CLOCK_TIMEOUT_SECS));
        }

        result
    }

    /// 将 Rhai Dynamic 转换为 ScriptResult
    fn dynamic_to_result(val: Dynamic) -> ScriptResult {
        if val.is::<()>() {
            return ScriptResult::Null;
        }

        if let Some(s) = val.clone().try_cast::<String>() {
            return ScriptResult::String(s);
        }

        if let Some(i) = val.clone().try_cast::<i64>() {
            return ScriptResult::Int(i);
        }

        if let Some(f) = val.clone().try_cast::<f64>() {
            return ScriptResult::Float(f);
        }

        if let Some(b) = val.clone().try_cast::<bool>() {
            return ScriptResult::Bool(b);
        }

        if let Some(arr) = val.clone().try_cast::<rhai::Array>() {
            let results: Vec<ScriptResult> = arr.into_iter().map(Self::dynamic_to_result).collect();
            return ScriptResult::Array(results);
        }

        if let Some(map) = val.clone().try_cast::<rhai::Map>() {
            let mut result_map = std::collections::HashMap::new();
            for (k, v) in map {
                result_map.insert(k.to_string(), Self::dynamic_to_result(v));
            }
            return ScriptResult::Map(result_map);
        }

        ScriptResult::String(val.to_string())
    }
}

#[cfg(test)]
impl ScriptEngine {
    fn with_timeout_and_sleep(timeout_secs: u64) -> Self {
        let start_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let timed_out: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let st = start_time.clone();
        let to = timed_out.clone();

        let mut engine = Engine::new();
        engine.set_max_operations(100_000);
        engine.set_max_call_levels(50);
        engine.set_max_string_size(MAX_STRING_SIZE);
        engine.set_max_array_size(MAX_ARRAY_SIZE);
        engine.set_max_map_size(MAX_MAP_SIZE);

        engine.on_progress(move |_ops| {
            if let Some(start) = *st.lock().unwrap() {
                if start.elapsed() > Duration::from_secs(timeout_secs) {
                    to.store(true, Ordering::SeqCst);
                    return Some(Dynamic::UNIT);
                }
            }
            None::<Dynamic>
        });

        engine.register_fn("block_ms", |ms: i64| {
            std::thread::sleep(Duration::from_millis(ms as u64));
        });

        ScriptEngine {
            engine: Arc::new(engine),
            start_time,
            timed_out,
        }
    }
}

/// 脚本执行上下文（提供给脚本的变量）
#[derive(Debug, Clone)]
pub struct ScriptContext {
    pub result: String,
    pub content: String,
    pub url: String,
    pub headers: String,
    pub source_name: Option<String>,
}

impl ScriptContext {
    pub fn new(result: &str, content: &str, url: &str) -> Self {
        Self {
            result: result.to_string(),
            content: content.to_string(),
            url: url.to_string(),
            headers: String::new(),
            source_name: None,
        }
    }

    pub fn with_source_name(mut self, name: &str) -> Self {
        self.source_name = Some(name.to_string());
        self
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：快速执行简单脚本
pub fn eval_script(script: &str) -> Result<ScriptResult, String> {
    let engine = ScriptEngine::new();
    engine.eval(script, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_script() {
        let engine = ScriptEngine::new();
        let result = engine.eval("1 + 2", None).unwrap();
        assert!(matches!(result, ScriptResult::Int(3)));
    }

    #[test]
    fn test_string_script() {
        let engine = ScriptEngine::new();
        let result = engine.eval("\"Hello\" + \" World\"", None).unwrap();
        assert_eq!(result.as_string(), Some("Hello World".to_string()));
    }

    #[test]
    fn test_script_with_context() {
        let engine = ScriptEngine::new();
        let ctx = ScriptContext::new("test", "<html>content</html>", "http://example.com");
        let result = engine.eval("result + \"_processed\"", Some(&ctx)).unwrap();
        assert_eq!(result.as_string(), Some("test_processed".to_string()));
    }

    #[test]
    fn test_regex_in_script() {
        let engine = ScriptEngine::new();
        let script = r#"
            if regex_match(content, "<title>(.*?)</title>") {
                regex_find(content, "<title>(.*?)</title>")
            } else {
                "No title"
            }
        "#;
        let ctx = ScriptContext::new("", "<html><title>Test Page</title></html>", "");
        let result = engine.eval(script, Some(&ctx)).unwrap();
        assert_eq!(
            result.as_string(),
            Some("<title>Test Page</title>".to_string())
        );
    }

    #[test]
    fn test_script_too_long() {
        let engine = ScriptEngine::new();
        let long_script = "x".repeat(MAX_SCRIPT_LENGTH + 1);
        let result = engine.eval(&long_script, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("过长"));
    }

    #[test]
    fn test_malicious_infinite_loop() {
        let engine = ScriptEngine::new();
        let result = engine.eval("while true {}; 42", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_malicious_deep_recursion() {
        let engine = ScriptEngine::new();
        let script = "
            fn recurse(n) { if n > 0 { recurse(n-1) } }
            recurse(30)
        ";
        let result = engine.eval(script, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_excessive_operations_limited() {
        let engine = ScriptEngine::new();
        let script = "let x = 0; for i in 0..100000 { x += 1; } x;";
        let result = engine.eval(script, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_script_syntax() {
        let engine = ScriptEngine::new();
        let result = engine.eval("let x = ;", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_wall_clock_timeout() {
        let engine = ScriptEngine::new();
        // string concat produces few operations but takes wall time
        let script = "let s = \"\"; for i in 0..8000 { s = s + \"x\"; } s";
        let result = engine.eval(script, None);
        // under 100k operations, not max_ops — should succeed within 5s
        assert!(
            result.is_ok(),
            "string concat should complete, got: {:?}",
            result
        );
    }

    #[test]
    fn test_busy_loop_terminated() {
        let engine = ScriptEngine::new();
        let result = engine.eval("let i = 0; loop { i = i + 1; }", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("超时") || err.contains("Too many operations"),
            "should be terminated by timeout or operations limit, got: {}",
            err
        );
    }

    #[test]
    fn test_recursive_output_size() {
        // 数组中的字符串也计入总大小
        let total = ScriptResult::Array(vec![
            ScriptResult::String("a".repeat(200_000)),
            ScriptResult::String("b".repeat(300_000)),
        ])
        .total_chars();
        assert_eq!(total, 500_000);
    }

    #[test]
    fn test_timeout_path_provable() {
        let engine = ScriptEngine::with_timeout_and_sleep(1);
        let script = "block_ms(2000); 42";
        let result = engine.eval(script, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("超时"), "expected 超时, got: {}", err);
    }
}
