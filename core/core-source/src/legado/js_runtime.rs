//! Legado JavaScript runtime abstraction.
//!
//! Full Legado source compatibility requires executing user-provided JavaScript
//! from `@js:`, `<js></js>`, URL templates, and URL options. This module keeps
//! the rule engine independent from the concrete embedded JS engine.

#[cfg(feature = "js-quickjs")]
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "js-quickjs")]
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
#[cfg(feature = "js-quickjs")]
use std::sync::OnceLock;

#[cfg(feature = "js-quickjs")]
thread_local! {
    static LEGADO_SET_CONTENT: std::cell::RefCell<Option<(String, String)>> = std::cell::RefCell::new(None);
    static LEGADO_JS_VARIABLES: std::cell::RefCell<HashMap<String, LegadoValue>> = std::cell::RefCell::new(HashMap::new());
}

use super::context::RuleContext;
use super::value::LegadoValue;

static CACHE_DB_PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

pub fn set_cache_db_path(path: Option<String>) {
    let _ = CACHE_DB_PATH.set(path);
}

fn get_cache_db_path() -> Option<&'static String> {
    CACHE_DB_PATH.get().and_then(|opt| opt.as_ref())
}

/// JavaScript runtime configuration.
#[derive(Clone)]
pub struct JsRuntimeConfig {
    pub timeout_ms: u64,
    pub max_script_len: usize,
}

impl Default for JsRuntimeConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            max_script_len: 100_000,
        }
    }
}

/// Common interface for embedded JavaScript runtimes.
pub trait JsRuntime: Send + Sync {
    fn eval(
        &self,
        script: &str,
        vars: &HashMap<String, LegadoValue>,
    ) -> Result<LegadoValue, String>;

    fn eval_string(
        &self,
        script: &str,
        vars: &HashMap<String, LegadoValue>,
    ) -> Result<String, String> {
        self.eval(script, vars).map(|v| v.as_string_lossy())
    }
}

/// Build the standard Legado JS variable map for a rule execution context.
pub fn build_runtime_vars(context: &RuleContext, html: &str) -> HashMap<String, LegadoValue> {
    let mut vars = context.all_variables();
    vars.insert(
        "baseUrl".into(),
        LegadoValue::String(context.base_url.clone()),
    );
    vars.insert(
        "base_url".into(),
        LegadoValue::String(context.base_url.clone()),
    );
    vars.insert(
        "src".into(),
        LegadoValue::String(
            if context.src.is_empty() {
                html
            } else {
                &context.src
            }
            .to_string(),
        ),
    );
    vars.insert("title".into(), LegadoValue::String(context.title.clone()));
    vars.insert("key".into(), LegadoValue::String(context.key.clone()));
    vars.insert("keyword".into(), LegadoValue::String(context.key.clone()));
    vars.insert("page".into(), LegadoValue::Int(context.page as i64));
    vars.insert("result".into(), context.get_variable("result"));
    vars.insert("book".into(), context.get_variable("book"));
    vars.insert("chapter".into(), context.get_variable("chapter"));
    vars.insert(
        "__legado_variables__".into(),
        LegadoValue::Map(context.all_variables()),
    );
    vars
}

/// Evaluate JS using the default configured runtime.
pub fn eval_default(
    script: &str,
    vars: &HashMap<String, LegadoValue>,
) -> Result<LegadoValue, String> {
    DefaultJsRuntime::new().eval(script, vars)
}

/// Execute JS with shared HTTP state for java.ajax/get/post/getCookie.
#[cfg(feature = "js-quickjs")]
pub fn eval_default_with_http_state(
    script: &str,
    vars: &HashMap<String, LegadoValue>,
    cookie_jar: Arc<reqwest::cookie::Jar>,
    default_headers: Vec<(String, String)>,
) -> Result<LegadoValue, String> {
    let _guard = JsCookieJarOverride::install(cookie_jar);
    let _vars_guard = JsVariablesOverride::install(vars.clone());
    let mut vars = vars.clone();
    vars.insert(
        "__legado_default_headers__".into(),
        headers_to_legado_value(&default_headers),
    );
    DefaultJsRuntime::new().eval(script, &vars)
}

#[cfg(not(feature = "js-quickjs"))]
pub fn eval_default_with_http_state(
    script: &str,
    vars: &HashMap<String, LegadoValue>,
    _cookie_jar: Arc<reqwest::cookie::Jar>,
    _default_headers: Vec<(String, String)>,
) -> Result<LegadoValue, String> {
    eval_default(script, vars)
}

pub fn eval_default_with_cookie_jar(
    script: &str,
    vars: &HashMap<String, LegadoValue>,
    cookie_jar: Arc<reqwest::cookie::Jar>,
) -> Result<LegadoValue, String> {
    eval_default_with_http_state(script, vars, cookie_jar, Vec::new())
}

/// Mutable URL request data modified by Legado URL option.js scripts.
#[derive(Debug, Clone)]
pub struct UrlJsContext {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl UrlJsContext {
    pub fn new(url: &str, headers: &[(String, String)]) -> Self {
        Self {
            url: url.to_string(),
            headers: headers.to_vec(),
        }
    }
}

/// Execute URL option.js and return the updated URL/header state.
pub fn eval_url_option_js(script: &str, context: &UrlJsContext) -> Result<UrlJsContext, String> {
    let mut vars = HashMap::new();
    vars.insert(
        "__legado_url__".into(),
        LegadoValue::String(context.url.clone()),
    );
    vars.insert(
        "__legado_headers__".into(),
        headers_to_legado_value(&context.headers),
    );
    let wrapped = format!(
        "(function(){{ {}; return JSON.stringify({{url: java.url, headers: java.headerMap.toObject()}}); }})()",
        script
    );
    let value = eval_default(&wrapped, &vars)?;
    let json = value.as_string_lossy();
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| format!("URL option.js result parse error: {e}; result={json}"))?;
    let url = parsed
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or(&context.url)
        .to_string();
    let headers = parsed
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_else(|| context.headers.clone());
    Ok(UrlJsContext { url, headers })
}

fn headers_to_legado_value(headers: &[(String, String)]) -> LegadoValue {
    let mut map = HashMap::new();
    for (key, value) in headers {
        map.insert(key.clone(), LegadoValue::String(value.clone()));
    }
    LegadoValue::Map(map)
}

#[cfg(feature = "js-quickjs")]
pub type DefaultJsRuntime = QuickJsRuntime;

#[cfg(all(not(feature = "js-quickjs"), feature = "js-boa"))]
pub type DefaultJsRuntime = BoaJsRuntime;

#[cfg(all(not(feature = "js-quickjs"), not(feature = "js-boa")))]
pub type DefaultJsRuntime = NoopJsRuntime;

/// Runtime used when no JS feature is enabled.
pub struct NoopJsRuntime {
    config: JsRuntimeConfig,
}

impl NoopJsRuntime {
    pub fn new() -> Self {
        Self {
            config: JsRuntimeConfig::default(),
        }
    }
}

impl Default for NoopJsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl JsRuntime for NoopJsRuntime {
    fn eval(
        &self,
        script: &str,
        _vars: &HashMap<String, LegadoValue>,
    ) -> Result<LegadoValue, String> {
        if script.len() > self.config.max_script_len {
            return Err(format!(
                "Script too long: {} > {}",
                script.len(),
                self.config.max_script_len
            ));
        }
        Err("JavaScript runtime is disabled".into())
    }
}

#[cfg(feature = "js-quickjs")]
pub struct QuickJsRuntime {
    config: JsRuntimeConfig,
}

#[cfg(feature = "js-quickjs")]
impl QuickJsRuntime {
    pub fn new() -> Self {
        Self {
            config: JsRuntimeConfig::default(),
        }
    }
}

#[cfg(feature = "js-quickjs")]
impl Default for QuickJsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "js-quickjs")]
impl JsRuntime for QuickJsRuntime {
    fn eval(
        &self,
        script: &str,
        vars: &HashMap<String, LegadoValue>,
    ) -> Result<LegadoValue, String> {
        if script.len() > self.config.max_script_len {
            return Err(format!(
                "Script too long: {} > {}",
                script.len(),
                self.config.max_script_len
            ));
        }

        let cleaned = clean_js_script(script);
        if cleaned.is_empty() {
            return Ok(LegadoValue::Null);
        }

        use rquickjs::{Context, Runtime};

        let runtime = Runtime::new().map_err(|e| format!("quickjs runtime: {e}"))?;
        let context = Context::full(&runtime).map_err(|e| format!("quickjs context: {e}"))?;
        let timeout = self.config.timeout_ms;
        if timeout > 0 {
            let start = std::time::Instant::now();
            runtime.set_interrupt_handler(Some(Box::new(move || {
                start.elapsed() > std::time::Duration::from_millis(timeout)
            })));
        }

        context.with(|ctx| {
            for (name, value) in vars {
                let stmt = legado_value_to_js_var(name, value);
                ctx.eval::<(), _>(stmt.as_str())
                    .map_err(|e| format!("set '{name}': {e}"))?;
            }

            register_quickjs_bridge(&ctx)?;
            ctx.eval::<(), _>(PREAMBLE)
                .map_err(|e| format!("preamble: {e}"))?;
            let expression = js_script_to_expression(cleaned);
            let json = ctx
                .eval::<String, _>(format!("JSON.stringify(({}))", expression))
                .map_err(|e| format!("eval: {e}"))?;
            js_json_to_legado(&json)
        })
    }
}

#[cfg(feature = "js-boa")]
pub struct BoaJsRuntime {
    config: JsRuntimeConfig,
}

#[cfg(feature = "js-boa")]
impl BoaJsRuntime {
    pub fn new() -> Self {
        Self {
            config: JsRuntimeConfig::default(),
        }
    }
}

#[cfg(feature = "js-boa")]
impl Default for BoaJsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "js-boa")]
impl JsRuntime for BoaJsRuntime {
    fn eval(
        &self,
        script: &str,
        vars: &HashMap<String, LegadoValue>,
    ) -> Result<LegadoValue, String> {
        if script.len() > self.config.max_script_len {
            return Err(format!(
                "Script too long: {} > {}",
                script.len(),
                self.config.max_script_len
            ));
        }

        let cleaned = clean_js_script(script);
        if cleaned.is_empty() {
            return Ok(LegadoValue::Null);
        }

        use boa_engine::{Context, Source};

        let mut context = Context::default();
        for (name, value) in vars {
            let js_stmt = legado_value_to_js_var(name, value);
            context
                .eval(Source::from_bytes(js_stmt.as_bytes()))
                .map_err(|e| format!("set '{name}': {e}"))?;
        }

        context
            .eval(Source::from_bytes(PREAMBLE.as_bytes()))
            .map_err(|e| format!("preamble: {e}"))?;
        let expression = js_script_to_expression(cleaned);
        let wrapped = format!("JSON.stringify(({}))", expression);
        let result = context
            .eval(Source::from_bytes(wrapped.as_bytes()))
            .map_err(|e| format!("eval: {e}"))?;
        js_json_to_legado(&format!("{}", result.display()))
    }
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn clean_js_script(script: &str) -> &str {
    script
        .trim()
        .strip_prefix("@js:")
        .or_else(|| script.trim().strip_prefix("js:"))
        .unwrap_or(script.trim())
        .trim()
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn js_script_to_expression(script: &str) -> String {
    let script = script.trim();
    let is_iife = script.starts_with("(function(") || script.starts_with("(function ");
    if contains_return_statement(script) && !is_iife {
        format!("(function(){{{}}})()", script)
    } else if needs_direct_eval(script) && !is_iife {
        format!(
            "eval({})",
            serde_json::to_string(script).unwrap_or_else(|_| "''".into())
        )
    } else {
        script.to_string()
    }
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn contains_return_statement(script: &str) -> bool {
    let trimmed = script.trim();
    trimmed.starts_with("return ")
        || trimmed == "return"
        || trimmed.contains("\nreturn ")
        || trimmed.contains(";return ")
        || trimmed
            .lines()
            .any(|line| line.trim_start().starts_with("return "))
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn needs_direct_eval(script: &str) -> bool {
    let trimmed = script.trim_start();
    script.contains(';')
        || script.contains('\n')
        || trimmed.starts_with("var ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("if(")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("for(")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("while(")
        || trimmed.starts_with("try ")
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn legado_value_to_js_var(name: &str, value: &LegadoValue) -> String {
    format!("var {} = {};", name, legado_value_to_js_expr(value))
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn legado_value_to_js_expr(value: &LegadoValue) -> String {
    match value {
        LegadoValue::Null => "null".into(),
        LegadoValue::Bool(true) => "true".into(),
        LegadoValue::Bool(false) => "false".into(),
        LegadoValue::Int(i) => i.to_string(),
        LegadoValue::Float(f) => f.to_string(),
        LegadoValue::String(s) | LegadoValue::Html(s) => {
            serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
        }
        LegadoValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(legado_value_to_js_expr).collect();
            format!("[{}]", items.join(", "))
        }
        LegadoValue::Map(map) => {
            let mut pairs = Vec::new();
            for (key, value) in map {
                let key = serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into());
                pairs.push(format!("{}:{}", key, legado_value_to_js_expr(value)));
            }
            format!("{{{}}}", pairs.join(","))
        }
    }
}

#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
fn js_json_to_legado(json: &str) -> Result<LegadoValue, String> {
    if json == "undefined" {
        return Ok(LegadoValue::Null);
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("JS result JSON parse error: {e}; result={json}"))?;
    Ok(LegadoValue::from_json_value(&value))
}

#[cfg(feature = "js-quickjs")]
fn register_quickjs_bridge(ctx: &rquickjs::Ctx<'_>) -> Result<(), String> {
    use rquickjs::Function;

    let global = ctx.globals();
    global
        .set(
            "__legado_base64_encode",
            Function::new(ctx.clone(), java_base64_encode)
                .map_err(|e| format!("register base64Encode: {e}"))?,
        )
        .map_err(|e| format!("set base64Encode: {e}"))?;
    global
        .set(
            "__legado_base64_decode",
            Function::new(ctx.clone(), java_base64_decode)
                .map_err(|e| format!("register base64Decode: {e}"))?,
        )
        .map_err(|e| format!("set base64Decode: {e}"))?;
    global
        .set(
            "__legado_md5",
            Function::new(ctx.clone(), java_md5_encode)
                .map_err(|e| format!("register md5Encode: {e}"))?,
        )
        .map_err(|e| format!("set md5Encode: {e}"))?;
    global
        .set(
            "__legado_md5_16",
            Function::new(ctx.clone(), java_md5_encode_16)
                .map_err(|e| format!("register md5Encode16: {e}"))?,
        )
        .map_err(|e| format!("set md5Encode16: {e}"))?;
    global
        .set(
            "__legado_encode_uri",
            Function::new(ctx.clone(), java_encode_uri)
                .map_err(|e| format!("register encodeURI: {e}"))?,
        )
        .map_err(|e| format!("set encodeURI: {e}"))?;
    global
        .set(
            "__legado_encode_uri_component",
            Function::new(ctx.clone(), java_encode_uri_component)
                .map_err(|e| format!("register encodeURIComponent: {e}"))?,
        )
        .map_err(|e| format!("set encodeURIComponent: {e}"))?;
    global
        .set(
            "__legado_decode_uri",
            Function::new(ctx.clone(), java_decode_uri)
                .map_err(|e| format!("register decodeURI: {e}"))?,
        )
        .map_err(|e| format!("set decodeURI: {e}"))?;
    global
        .set(
            "__legado_http_request",
            Function::new(ctx.clone(), java_http_request)
                .map_err(|e| format!("register http request: {e}"))?,
        )
        .map_err(|e| format!("set http request: {e}"))?;
    global
        .set(
            "__legado_get_cookie",
            Function::new(ctx.clone(), java_get_cookie)
                .map_err(|e| format!("register getCookie: {e}"))?,
        )
        .map_err(|e| format!("set getCookie: {e}"))?;
    global
        .set(
            "__legado_get_string",
            Function::new(ctx.clone(), java_get_string)
                .map_err(|e| format!("register getString: {e}"))?,
        )
        .map_err(|e| format!("set getString: {e}"))?;
    global
        .set(
            "__legado_get_string_list",
            Function::new(ctx.clone(), java_get_string_list)
                .map_err(|e| format!("register getStringList: {e}"))?,
        )
        .map_err(|e| format!("set getStringList: {e}"))?;
    global
        .set(
            "__legado_get_elements",
            Function::new(ctx.clone(), java_get_elements)
                .map_err(|e| format!("register getElements: {e}"))?,
        )
        .map_err(|e| format!("set getElements: {e}"))?;
    global
        .set(
            "__legado_aes_decode_to_string",
            Function::new(ctx.clone(), java_aes_decode_to_string)
                .map_err(|e| format!("register aesDecodeToString: {e}"))?,
        )
        .map_err(|e| format!("set aesDecodeToString: {e}"))?;
    global
        .set(
            "__legado_aes_base64_decode_to_string",
            Function::new(ctx.clone(), java_aes_base64_decode_to_string)
                .map_err(|e| format!("register aesBase64DecodeToString: {e}"))?,
        )
        .map_err(|e| format!("set aesBase64DecodeToString: {e}"))?;
    global
        .set(
            "__legado_aes_encode_to_string",
            Function::new(ctx.clone(), java_aes_encode_to_string)
                .map_err(|e| format!("register aesEncodeToString: {e}"))?,
        )
        .map_err(|e| format!("set aesEncodeToString: {e}"))?;
    global
        .set(
            "__legado_aes_encode_to_base64_string",
            Function::new(ctx.clone(), java_aes_encode_to_base64_string)
                .map_err(|e| format!("register aesEncodeToBase64String: {e}"))?,
        )
        .map_err(|e| format!("set aesEncodeToBase64String: {e}"))?;
    global
        .set(
            "__legado_aes_decode_to_byte_array",
            Function::new(ctx.clone(), java_aes_decode_to_byte_array)
                .map_err(|e| format!("register aesDecodeToByteArray: {e}"))?,
        )
        .map_err(|e| format!("set aesDecodeToByteArray: {e}"))?;
    global
        .set(
            "__legado_aes_base64_decode_to_byte_array",
            Function::new(ctx.clone(), java_aes_base64_decode_to_byte_array)
                .map_err(|e| format!("register aesBase64DecodeToByteArray: {e}"))?,
        )
        .map_err(|e| format!("set aesBase64DecodeToByteArray: {e}"))?;
    global
        .set(
            "__legado_aes_encode_to_byte_array",
            Function::new(ctx.clone(), java_aes_encode_to_byte_array)
                .map_err(|e| format!("register aesEncodeToByteArray: {e}"))?,
        )
        .map_err(|e| format!("set aesEncodeToByteArray: {e}"))?;
    global
        .set(
            "__legado_aes_encode_to_base64_byte_array",
            Function::new(ctx.clone(), java_aes_encode_to_base64_byte_array)
                .map_err(|e| format!("register aesEncodeToBase64ByteArray: {e}"))?,
        )
        .map_err(|e| format!("set aesEncodeToBase64ByteArray: {e}"))?;
    global
        .set(
            "__legado_time_format",
            Function::new(ctx.clone(), java_time_format)
                .map_err(|e| format!("register timeFormat: {e}"))?,
        )
        .map_err(|e| format!("set timeFormat: {e}"))?;
    global
        .set(
            "__legado_html_format",
            Function::new(ctx.clone(), java_html_format)
                .map_err(|e| format!("register htmlFormat: {e}"))?,
        )
        .map_err(|e| format!("set htmlFormat: {e}"))?;
    global
        .set(
            "__legado_get_zip_string_content",
            Function::new(ctx.clone(), java_get_zip_string_content)
                .map_err(|e| format!("register getZipStringContent: {e}"))?,
        )
        .map_err(|e| format!("set getZipStringContent: {e}"))?;
    global
        .set(
            "__legado_get_zip_byte_array_content",
            Function::new(ctx.clone(), java_get_zip_byte_array_content)
                .map_err(|e| format!("register getZipByteArrayContent: {e}"))?,
        )
        .map_err(|e| format!("set getZipByteArrayContent: {e}"))?;
    global
        .set(
            "__legado_read_file",
            Function::new(ctx.clone(), java_read_file)
                .map_err(|e| format!("register readFile: {e}"))?,
        )
        .map_err(|e| format!("set readFile: {e}"))?;
    global
        .set(
            "__legado_read_txt_file",
            Function::new(ctx.clone(), java_read_txt_file)
                .map_err(|e| format!("register readTxtFile: {e}"))?,
        )
        .map_err(|e| format!("set readTxtFile: {e}"))?;
    global
        .set(
            "__legado_base64_decode_to_byte_array",
            Function::new(ctx.clone(), java_base64_decode_to_byte_array)
                .map_err(|e| format!("register base64DecodeToByteArray: {e}"))?,
        )
        .map_err(|e| format!("set base64DecodeToByteArray: {e}"))?;
    global
        .set(
            "__legado_log",
            Function::new(ctx.clone(), java_log).map_err(|e| format!("register log: {e}"))?,
        )
        .map_err(|e| format!("set log: {e}"))?;
    global
        .set(
            "__legado_cache_get",
            Function::new(ctx.clone(), java_cache_get)
                .map_err(|e| format!("register cache get: {e}"))?,
        )
        .map_err(|e| format!("set cache get: {e}"))?;
    global
        .set(
            "__legado_cache_put",
            Function::new(ctx.clone(), java_cache_put)
                .map_err(|e| format!("register cache put: {e}"))?,
        )
        .map_err(|e| format!("set cache put: {e}"))?;
    global
        .set(
            "__legado_set_content",
            Function::new(ctx.clone(), java_set_content)
                .map_err(|e| format!("register setContent: {e}"))?,
        )
        .map_err(|e| format!("set setContent: {e}"))?;
    global
        .set(
            "__legado_download_file",
            Function::new(ctx.clone(), java_download_file)
                .map_err(|e| format!("register downloadFile: {e}"))?,
        )
        .map_err(|e| format!("set downloadFile: {e}"))?;
    global
        .set(
            "__legado_get_file",
            Function::new(ctx.clone(), java_get_file)
                .map_err(|e| format!("register getFile: {e}"))?,
        )
        .map_err(|e| format!("set getFile: {e}"))?;
    global
        .set(
            "__legado_delete_file",
            Function::new(ctx.clone(), java_delete_file)
                .map_err(|e| format!("register deleteFile: {e}"))?,
        )
        .map_err(|e| format!("set deleteFile: {e}"))?;
    global
        .set(
            "__legado_unzip_file",
            Function::new(ctx.clone(), java_unzip_file)
                .map_err(|e| format!("register unzipFile: {e}"))?,
        )
        .map_err(|e| format!("set unzipFile: {e}"))?;
    global
        .set(
            "__legado_get_txt_in_folder",
            Function::new(ctx.clone(), java_get_txt_in_folder)
                .map_err(|e| format!("register getTxtInFolder: {e}"))?,
        )
        .map_err(|e| format!("set getTxtInFolder: {e}"))?;
    global
        .set(
            "__legado_utf8_to_gbk",
            Function::new(ctx.clone(), java_utf8_to_gbk)
                .map_err(|e| format!("register utf8ToGbk: {e}"))?,
        )
        .map_err(|e| format!("set utf8ToGbk: {e}"))?;
    global
        .set(
            "__legado_query_base64_ttf",
            Function::new(ctx.clone(), java_query_base64_ttf)
                .map_err(|e| format!("register queryBase64Ttf: {e}"))?,
        )
        .map_err(|e| format!("set queryBase64Ttf: {e}"))?;
    global
        .set(
            "__legado_query_ttf",
            Function::new(ctx.clone(), java_query_ttf)
                .map_err(|e| format!("register queryTtf: {e}"))?,
        )
        .map_err(|e| format!("set queryTtf: {e}"))?;
    global
        .set(
            "__legado_replace_font",
            Function::new(ctx.clone(), java_replace_font)
                .map_err(|e| format!("register replaceFont: {e}"))?,
        )
        .map_err(|e| format!("set replaceFont: {e}"))?;
    Ok(())
}

#[cfg(feature = "js-quickjs")]
fn java_base64_encode(input: String) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

#[cfg(feature = "js-quickjs")]
fn java_base64_decode(input: String) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input.as_bytes())
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_base64_decode_to_byte_array(input: String) -> String {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.as_bytes())
        .unwrap_or_default();
    serde_json::to_string(&bytes).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn java_log(_msg: String) -> String {
    String::new()
}

#[cfg(feature = "js-quickjs")]
fn java_cache_get(key: String) -> String {
    if let Some(val) = current_js_variables().get(&key) {
        return val.as_string_lossy();
    }
    if let Some(db_path) = get_cache_db_path() {
        if let Ok(conn) = rusqlite::Connection::open(db_path) {
            let dao = core_storage::cache_dao::CacheDao::new(&conn);
            if let Ok(Some(val)) = dao.get(&key) {
                return val;
            }
        }
    }
    String::new()
}

#[cfg(feature = "js-quickjs")]
fn java_cache_put(key: String, value: String) -> String {
    set_current_js_variable(key.clone(), LegadoValue::String(value.clone()));
    if let Some(db_path) = get_cache_db_path() {
        if let Ok(conn) = rusqlite::Connection::open(db_path) {
            let dao = core_storage::cache_dao::CacheDao::new(&conn);
            let _ = dao.put(&key, &value);
        }
    }
    value
}

#[cfg(feature = "js-quickjs")]
fn java_md5_encode(input: String) -> String {
    format!("{:x}", md5::compute(input.as_bytes()))
}

#[cfg(feature = "js-quickjs")]
fn java_md5_encode_16(input: String) -> String {
    let md5 = java_md5_encode(input);
    md5.get(8..24).unwrap_or_default().to_string()
}

#[cfg(feature = "js-quickjs")]
fn java_encode_uri(input: String) -> String {
    urlencoding::encode(&input).to_string()
}

#[cfg(feature = "js-quickjs")]
fn java_encode_uri_component(input: String) -> String {
    urlencoding::encode(&input).to_string()
}

#[cfg(feature = "js-quickjs")]
fn java_decode_uri(input: String) -> String {
    urlencoding::decode(&input)
        .map(|s| s.into_owned())
        .unwrap_or(input)
}

#[cfg(feature = "js-quickjs")]
fn java_utf8_to_gbk(input: String) -> String {
    let bytes = input.as_bytes();
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    decoded.into_owned()
}

#[cfg(feature = "js-quickjs")]
fn java_http_request(method: String, url: String, body: String, headers_json: String) -> String {
    let cookie_jar = current_js_cookie_jar();
    std::thread::spawn(move || {
        let _cookie_guard = JsCookieJarOverride::install(cookie_jar);
        java_http_request_blocking(method, url, body, headers_json)
    })
    .join()
    .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_http_request_blocking(
    method: String,
    url: String,
    body: String,
    headers_json: String,
) -> String {
    let cookie_jar = current_js_cookie_jar();

    let mut charset = None;
    let mut proxy_url: Option<String> = None;
    let mut header_pairs: Vec<(String, String)> = Vec::new();

    if let Ok(headers_val) = serde_json::from_str::<serde_json::Value>(&headers_json) {
        if let Some(map) = headers_val.as_object() {
            for (key, value) in map {
                if key.eq_ignore_ascii_case("charset") {
                    charset = value.as_str().map(|s| s.to_string());
                    continue;
                }
                if key.eq_ignore_ascii_case("proxy") {
                    proxy_url = value.as_str().map(|s| s.to_string());
                    continue;
                }
                let value = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string());
                header_pairs.push((key.clone(), value));
            }
        }
    }

    let client = if let Some(ref p_url) = proxy_url {
        if let Ok(proxy) = reqwest::Proxy::all(p_url) {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(15))
                .cookie_provider(cookie_jar.clone())
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                .proxy(proxy)
                .build()
                .unwrap_or_else(|_| js_http_client(cookie_jar.clone()))
        } else {
            js_http_client(cookie_jar.clone())
        }
    } else {
        js_http_client(cookie_jar)
    };

    let mut request = if method.eq_ignore_ascii_case("POST") {
        client.post(&url).body(body)
    } else {
        client.get(&url)
    };

    for (key, value) in &header_pairs {
        request = request.header(key.as_str(), value.as_str());
    }

    if method.eq_ignore_ascii_case("POST")
        && !header_pairs
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
    {
        request = request.header("Content-Type", "application/x-www-form-urlencoded");
    }

    let response = match request.send() {
        Ok(response) => response,
        Err(_) => return String::new(),
    };

    let headers_map: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().to_lowercase(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let max_bytes: usize = 10 * 1024 * 1024;
    let mut buf = vec![0u8; 0];
    {
        use std::io::Read;
        if response
            .take((max_bytes + 1) as u64)
            .read_to_end(&mut buf)
            .is_err()
            || buf.len() > max_bytes
        {
            return String::new();
        }
    }
    let bytes = buf;

    let charset =
        charset.unwrap_or_else(|| super::url::guess_charset_from_response(&headers_map, &bytes));
    let (decoded, _) = super::url::decode_response_bytes(&bytes, &charset);
    decoded
}

#[cfg(feature = "js-quickjs")]
fn js_http_client(cookie_jar: Arc<reqwest::cookie::Jar>) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(15))
        .cookie_provider(cookie_jar)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .expect("failed to build JS HTTP client")
}

#[cfg(feature = "js-quickjs")]
fn default_js_cookie_jar() -> Arc<reqwest::cookie::Jar> {
    static JAR: OnceLock<Arc<reqwest::cookie::Jar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(reqwest::cookie::Jar::default()))
        .clone()
}

#[cfg(feature = "js-quickjs")]
thread_local! {
    static COOKIE_JAR_OVERRIDE: RefCell<Option<Arc<reqwest::cookie::Jar>>> = const { RefCell::new(None) };
}

#[cfg(feature = "js-quickjs")]
fn current_js_cookie_jar() -> Arc<reqwest::cookie::Jar> {
    COOKIE_JAR_OVERRIDE
        .with(|override_jar| override_jar.borrow().clone())
        .unwrap_or_else(default_js_cookie_jar)
}

#[cfg(feature = "js-quickjs")]
struct JsCookieJarOverride {
    previous: Option<Arc<reqwest::cookie::Jar>>,
}

#[cfg(feature = "js-quickjs")]
impl JsCookieJarOverride {
    fn install(cookie_jar: Arc<reqwest::cookie::Jar>) -> Self {
        let previous =
            COOKIE_JAR_OVERRIDE.with(|override_jar| override_jar.replace(Some(cookie_jar)));
        Self { previous }
    }
}

#[cfg(feature = "js-quickjs")]
impl Drop for JsCookieJarOverride {
    fn drop(&mut self) {
        COOKIE_JAR_OVERRIDE.with(|override_jar| {
            override_jar.replace(self.previous.take());
        });
    }
}

#[cfg(feature = "js-quickjs")]
struct JsVariablesOverride {
    previous: HashMap<String, LegadoValue>,
}

#[cfg(feature = "js-quickjs")]
impl JsVariablesOverride {
    fn install(vars: HashMap<String, LegadoValue>) -> Self {
        let previous = LEGADO_JS_VARIABLES.with(|cell| cell.replace(vars));
        Self { previous }
    }
}

#[cfg(feature = "js-quickjs")]
impl Drop for JsVariablesOverride {
    fn drop(&mut self) {
        LEGADO_JS_VARIABLES.with(|cell| {
            cell.replace(std::mem::take(&mut self.previous));
        });
    }
}

#[cfg(feature = "js-quickjs")]
fn current_js_variables() -> HashMap<String, LegadoValue> {
    LEGADO_JS_VARIABLES.with(|cell| cell.borrow().clone())
}

#[cfg(feature = "js-quickjs")]
fn set_current_js_variable(key: String, value: LegadoValue) {
    LEGADO_JS_VARIABLES.with(|cell| {
        cell.borrow_mut().insert(key, value);
    });
}

#[cfg(feature = "js-quickjs")]
fn java_get_cookie(tag: String, key: String) -> String {
    use reqwest::cookie::CookieStore;

    let Ok(url) = tag.parse::<url::Url>() else {
        return String::new();
    };
    let Some(header) = current_js_cookie_jar().cookies(&url) else {
        return String::new();
    };
    let Ok(cookie) = header.to_str() else {
        return String::new();
    };
    if key.is_empty() || key == "null" || key == "undefined" {
        return cookie.to_string();
    }
    for pair in cookie.split(';') {
        let Some((name, value)) = pair.trim().split_once('=') else {
            continue;
        };
        if name == key {
            return value.to_string();
        }
    }
    String::new()
}

#[cfg(feature = "js-quickjs")]
fn java_set_content(content: String, base_url: String) -> String {
    LEGADO_SET_CONTENT.with(|cell| {
        let _prev = cell.borrow_mut().replace((content, base_url));
    });
    String::new()
}

#[cfg(feature = "js-quickjs")]
fn java_get_string(rule: String, content: String, base_url: String) -> String {
    let (effective_content, effective_base_url) = LEGADO_SET_CONTENT.with(|cell| {
        cell.borrow_mut()
            .take()
            .unwrap_or_else(|| (content, base_url))
    });
    let result = {
        let context = RuleContext::new(&effective_base_url, &effective_content);
        super::rule::execute_legado_rule(&rule, &effective_content, &context)
            .ok()
            .and_then(|items| items.into_iter().next())
            .unwrap_or_default()
    };
    // Release any remaining setContent that wasn't consumed
    LEGADO_SET_CONTENT.with(|cell| {
        let _dangling = cell.borrow_mut().take();
    });
    result
}

#[cfg(feature = "js-quickjs")]
fn java_get_string_list(rule: String, content: String, base_url: String) -> String {
    let (effective_content, effective_base_url) = LEGADO_SET_CONTENT.with(|cell| {
        cell.borrow_mut()
            .take()
            .unwrap_or_else(|| (content, base_url))
    });
    let result = {
        let context = RuleContext::new(&effective_base_url, &effective_content);
        let items = super::rule::execute_legado_rule(&rule, &effective_content, &context)
            .unwrap_or_default();
        serde_json::to_string(&items).unwrap_or_else(|_| "[]".into())
    };
    LEGADO_SET_CONTENT.with(|cell| {
        let _dangling = cell.borrow_mut().take();
    });
    result
}

#[cfg(feature = "js-quickjs")]
fn java_get_elements(rule: String, content: String, _base_url: String) -> String {
    let (effective_content, _) = LEGADO_SET_CONTENT.with(|cell| {
        cell.borrow_mut()
            .take()
            .unwrap_or_else(|| (content, _base_url))
    });
    let selector = rule
        .trim()
        .strip_prefix("@css:")
        .unwrap_or(rule.trim())
        .split("##")
        .next()
        .unwrap_or_default()
        .trim();
    if selector.is_empty() {
        return "[]".into();
    }
    let Ok(selector) = parse_scraper_selector_safely(selector) else {
        return "[]".into();
    };
    let document = scraper::Html::parse_fragment(&effective_content);
    let elements: Vec<serde_json::Value> =
        document.select(&selector).map(element_to_json).collect();
    serde_json::to_string(&elements).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn parse_scraper_selector_safely(selector: &str) -> Result<scraper::Selector, String> {
    catch_unwind(AssertUnwindSafe(|| scraper::Selector::parse(selector)))
        .map_err(|_| format!("CSS selector parse panic for '{}'", selector))?
        .map_err(|_| format!("CSS selector parse error for '{}'", selector))
}

#[cfg(feature = "js-quickjs")]
fn element_to_json(element: scraper::ElementRef<'_>) -> serde_json::Value {
    let attrs = element
        .value()
        .attrs()
        .map(|(key, value)| {
            (
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            )
        })
        .collect();
    let children = element
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .map(element_to_json_shallow)
        .collect::<Vec<_>>();
    serde_json::json!({
        "tagName": element.value().name(),
        "text": element.text().collect::<String>(),
        "ownText": element.children()
            .filter_map(|child| match child.value() {
                scraper::node::Node::Text(text) => Some(text.text.to_string()),
                _ => None,
            })
            .collect::<String>(),
        "html": element.inner_html(),
        "outerHtml": element.html(),
        "attrs": serde_json::Value::Object(attrs),
        "children": children,
    })
}

#[cfg(feature = "js-quickjs")]
fn element_to_json_shallow(element: scraper::ElementRef<'_>) -> serde_json::Value {
    let attrs = element
        .value()
        .attrs()
        .map(|(key, value)| {
            (
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            )
        })
        .collect();
    serde_json::json!({
        "tagName": element.value().name(),
        "text": element.text().collect::<String>(),
        "ownText": element.children()
            .filter_map(|child| match child.value() {
                scraper::node::Node::Text(text) => Some(text.text.to_string()),
                _ => None,
            })
            .collect::<String>(),
        "html": element.inner_html(),
        "outerHtml": element.html(),
        "attrs": serde_json::Value::Object(attrs),
        "children": [],
    })
}

#[cfg(feature = "js-quickjs")]
fn java_aes_decode_to_string(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    let bytes = hex_to_bytes(&data).unwrap_or_else(|| data.into_bytes());
    aes_decrypt(&bytes, key.as_bytes(), &transformation, iv.as_bytes())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_aes_base64_decode_to_string(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .unwrap_or_default();
    aes_decrypt(&bytes, key.as_bytes(), &transformation, iv.as_bytes())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_aes_encode_to_string(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    aes_encrypt(
        data.as_bytes(),
        key.as_bytes(),
        &transformation,
        iv.as_bytes(),
    )
    .map(|bytes| bytes_to_hex(&bytes))
    .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_aes_encode_to_base64_string(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    use base64::Engine;
    aes_encrypt(
        data.as_bytes(),
        key.as_bytes(),
        &transformation,
        iv.as_bytes(),
    )
    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
    .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_aes_decode_to_byte_array(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    let bytes = hex_to_bytes(&data).unwrap_or_else(|| data.into_bytes());
    let out =
        aes_decrypt(&bytes, key.as_bytes(), &transformation, iv.as_bytes()).unwrap_or_default();
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn java_aes_base64_decode_to_byte_array(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .unwrap_or_default();
    let out =
        aes_decrypt(&bytes, key.as_bytes(), &transformation, iv.as_bytes()).unwrap_or_default();
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn java_aes_encode_to_byte_array(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    let out = aes_encrypt(
        data.as_bytes(),
        key.as_bytes(),
        &transformation,
        iv.as_bytes(),
    )
    .unwrap_or_default();
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn java_aes_encode_to_base64_byte_array(
    data: String,
    key: String,
    transformation: String,
    iv: String,
) -> String {
    use base64::Engine;
    let base64 = aes_encrypt(
        data.as_bytes(),
        key.as_bytes(),
        &transformation,
        iv.as_bytes(),
    )
    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
    .unwrap_or_default();
    serde_json::to_string(base64.as_bytes()).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn aes_encrypt(data: &[u8], key: &[u8], transformation: &str, iv: &[u8]) -> Option<Vec<u8>> {
    use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyInit, KeyIvInit};

    let normalized = transformation.to_uppercase();
    if normalized.contains("/ECB/") {
        return match key.len() {
            16 => Some(
                ecb::Encryptor::<aes::Aes128>::new_from_slice(key)
                    .ok()?
                    .encrypt_padded_vec_mut::<Pkcs7>(data),
            ),
            24 => Some(
                ecb::Encryptor::<aes::Aes192>::new_from_slice(key)
                    .ok()?
                    .encrypt_padded_vec_mut::<Pkcs7>(data),
            ),
            32 => Some(
                ecb::Encryptor::<aes::Aes256>::new_from_slice(key)
                    .ok()?
                    .encrypt_padded_vec_mut::<Pkcs7>(data),
            ),
            _ => None,
        };
    }
    if iv.len() != 16 {
        return None;
    }
    match key.len() {
        16 => Some(
            cbc::Encryptor::<aes::Aes128>::new_from_slices(key, iv)
                .ok()?
                .encrypt_padded_vec_mut::<Pkcs7>(data),
        ),
        24 => Some(
            cbc::Encryptor::<aes::Aes192>::new_from_slices(key, iv)
                .ok()?
                .encrypt_padded_vec_mut::<Pkcs7>(data),
        ),
        32 => Some(
            cbc::Encryptor::<aes::Aes256>::new_from_slices(key, iv)
                .ok()?
                .encrypt_padded_vec_mut::<Pkcs7>(data),
        ),
        _ => None,
    }
}

#[cfg(feature = "js-quickjs")]
fn aes_decrypt(data: &[u8], key: &[u8], transformation: &str, iv: &[u8]) -> Option<Vec<u8>> {
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyInit, KeyIvInit};

    let normalized = transformation.to_uppercase();
    if normalized.contains("/ECB/") {
        return match key.len() {
            16 => ecb::Decryptor::<aes::Aes128>::new_from_slice(key)
                .ok()?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .ok(),
            24 => ecb::Decryptor::<aes::Aes192>::new_from_slice(key)
                .ok()?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .ok(),
            32 => ecb::Decryptor::<aes::Aes256>::new_from_slice(key)
                .ok()?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .ok(),
            _ => None,
        };
    }
    if iv.len() != 16 {
        return None;
    }
    match key.len() {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv)
            .ok()?
            .decrypt_padded_vec_mut::<Pkcs7>(data)
            .ok(),
        24 => cbc::Decryptor::<aes::Aes192>::new_from_slices(key, iv)
            .ok()?
            .decrypt_padded_vec_mut::<Pkcs7>(data)
            .ok(),
        32 => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv)
            .ok()?
            .decrypt_padded_vec_mut::<Pkcs7>(data)
            .ok(),
        _ => None,
    }
}

#[cfg(feature = "js-quickjs")]
fn hex_to_bytes(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 || !input.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(out)
}

#[cfg(feature = "js-quickjs")]
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(feature = "js-quickjs")]
fn java_time_format(input: String) -> String {
    let trimmed = input.trim();
    let Ok(mut timestamp) = trimmed.parse::<i64>() else {
        return trimmed.to_string();
    };
    if timestamp.abs() >= 1_000_000_000_000 || timestamp.abs() < 1_000_000_000 {
        timestamp /= 1000;
    }
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.naive_local().format("%Y/%m/%d %H:%M").to_string())
        .unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_html_format(input: String) -> String {
    static RE_BR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("(?i)<br\\s*/?>").unwrap());
    static RE_P: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("(?i)</p\\s*>").unwrap());
    static RE_SCRIPT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("(?is)<script.*?</script>").unwrap());
    static RE_STYLE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("(?is)<style.*?</style>").unwrap());
    static RE_TAGS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("(?is)<[^>]+>").unwrap());

    let mut out = input;
    let replacements = [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
    ];
    for (from, to) in replacements {
        out = out.replace(from, to);
    }
    out = RE_BR.replace_all(&out, "\n").to_string();
    out = RE_P.replace_all(&out, "\n").to_string();
    out = RE_SCRIPT.replace_all(&out, "").to_string();
    out = RE_STYLE.replace_all(&out, "").to_string();
    out = RE_TAGS.replace_all(&out, "").to_string();
    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(feature = "js-quickjs")]
fn java_get_zip_string_content(url: String, path: String) -> String {
    let bytes = get_zip_entry_bytes(&url, &path).unwrap_or_default();
    String::from_utf8(bytes.clone()).unwrap_or_else(|_| {
        let (decoded, _, _) = encoding_rs::GBK.decode(&bytes);
        decoded.into_owned()
    })
}

#[cfg(feature = "js-quickjs")]
fn java_get_zip_byte_array_content(url: String, path: String) -> String {
    let bytes = get_zip_entry_bytes(&url, &path).unwrap_or_default();
    serde_json::to_string(&bytes).unwrap_or_else(|_| "[]".into())
}

const MAX_ZIP_DOWNLOAD: u64 = 50 * 1024 * 1024;
const MAX_ZIP_ENTRY: u64 = 10 * 1024 * 1024;

#[cfg(feature = "js-quickjs")]
fn get_zip_entry_bytes(url: &str, path: &str) -> Option<Vec<u8>> {
    use std::io::{Cursor, Read};

    let mut response = js_http_client(current_js_cookie_jar())
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .ok()?;
    if response
        .content_length()
        .is_some_and(|len| len > MAX_ZIP_DOWNLOAD)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_ZIP_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_ZIP_DOWNLOAD {
        return None;
    }
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let file = archive.by_name(path).ok()?;
    let mut out = Vec::new();
    file.take(MAX_ZIP_ENTRY + 1).read_to_end(&mut out).ok()?;
    if out.len() as u64 > MAX_ZIP_ENTRY {
        return None;
    }
    Some(out)
}

#[cfg(feature = "js-quickjs")]
fn resolve_file_path(path: &str) -> Option<String> {
    let root = std::env::var("LEGADO_FILE_ROOT").ok()?;
    let root = std::path::Path::new(&root);
    let root_canon = root.canonicalize().ok()?;
    let rel = std::path::Path::new(path);
    if !is_safe_relative_path(rel) {
        return None;
    }
    let candidate = root_canon.join(rel);
    let Ok(canonical) = candidate.canonicalize() else {
        return None;
    };
    if !canonical.starts_with(&root_canon) {
        return None;
    }
    Some(canonical.to_string_lossy().to_string())
}

#[cfg(feature = "js-quickjs")]
fn resolve_write_path(path: &str) -> Option<String> {
    let root = std::env::var("LEGADO_FILE_ROOT").ok()?;
    let root = std::path::Path::new(&root);
    let root_canon = root.canonicalize().ok()?;
    let rel = std::path::Path::new(path);
    if !is_safe_relative_path(rel) {
        return None;
    }
    let candidate = root_canon.join(rel);
    if candidate == root_canon {
        return Some(root_canon.to_string_lossy().to_string());
    }
    let parent = candidate.parent()?;
    std::fs::create_dir_all(parent).ok()?;
    let parent_canon = parent.canonicalize().ok()?;
    if !parent_canon.starts_with(&root_canon) {
        return None;
    }
    let file_name = candidate.file_name()?;
    let resolved = parent_canon.join(file_name);
    Some(resolved.to_string_lossy().to_string())
}

#[cfg(feature = "js-quickjs")]
fn is_safe_relative_path(path: &std::path::Path) -> bool {
    !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

#[cfg(feature = "js-quickjs")]
fn java_download_file(url: String, path: String) -> String {
    use std::io::{Read, Write};

    let resolved = match resolve_write_path(&path) {
        Some(p) => p,
        None => return String::new(),
    };
    let tmp_path = format!("{}.download", resolved);
    let mut response = match js_http_client(current_js_cookie_jar()).get(&url).send() {
        Ok(response) => response,
        Err(_) => return String::new(),
    };
    if response
        .content_length()
        .is_some_and(|len| len > MAX_ZIP_DOWNLOAD)
    {
        return String::new();
    }
    let mut file = match std::fs::File::create(&tmp_path) {
        Ok(file) => file,
        Err(_) => return String::new(),
    };
    let mut ok = true;
    let mut total = 0u64;
    let mut buf = [0u8; 16 * 1024];
    loop {
        let read = match response.read(&mut buf) {
            Ok(read) => read,
            Err(_) => {
                ok = false;
                break;
            }
        };
        if read == 0 {
            break;
        }
        total = match total.checked_add(read as u64) {
            Some(total) if total <= MAX_ZIP_DOWNLOAD => total,
            _ => {
                ok = false;
                break;
            }
        };
        if file.write_all(&buf[..read]).is_err() {
            ok = false;
            break;
        }
    }
    if ok && file.flush().is_err() {
        ok = false;
    }
    drop(file);
    if !ok {
        let _ = std::fs::remove_file(&tmp_path);
        return String::new();
    }
    match std::fs::rename(&tmp_path, &resolved) {
        Ok(_) => "true".to_string(),
        Err(_) => {
            let _ = std::fs::remove_file(&tmp_path);
            String::new()
        }
    }
}

#[cfg(feature = "js-quickjs")]
fn java_get_file(path: String) -> String {
    let resolved = match resolve_file_path(&path) {
        Some(p) => p,
        None => return String::new(),
    };
    std::fs::read_to_string(&resolved).unwrap_or_default()
}

#[cfg(feature = "js-quickjs")]
fn java_delete_file(path: String) -> String {
    let resolved = match resolve_file_path(&path) {
        Some(p) => p,
        None => return String::new(),
    };
    match std::fs::remove_file(&resolved) {
        Ok(_) => "true".to_string(),
        Err(_) => String::new(),
    }
}

#[cfg(feature = "js-quickjs")]
fn java_unzip_file(zip_path: String, dest_dir: String) -> String {
    use std::io::Read;

    let resolved_zip = match resolve_file_path(&zip_path) {
        Some(p) => p,
        None => return String::new(),
    };
    let resolved_dest = match resolve_write_path(&dest_dir) {
        Some(p) => p,
        None => return String::new(),
    };
    std::fs::create_dir_all(&resolved_dest).ok();
    let Ok(dest_canonical) = std::path::Path::new(&resolved_dest).canonicalize() else {
        return String::new();
    };
    let bytes = match std::fs::read(&resolved_zip) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    let cursor = std::io::Cursor::new(bytes);
    let Ok(mut archive) = zip::ZipArchive::new(cursor) else {
        return String::new();
    };
    let mut total_unzipped: u64 = 0;
    let mut ok = true;
    for i in 0..archive.len() {
        let Ok(file) = archive.by_index(i) else {
            ok = false;
            continue;
        };
        let Some(file_name) = file.enclosed_name() else {
            ok = false;
            continue;
        };
        let out_path = dest_canonical.join(&file_name);
        if !out_path.starts_with(&dest_canonical) {
            ok = false;
            continue;
        }
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).ok();
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let parent = out_path.parent().unwrap_or(&dest_canonical);
        let Ok(parent_canonical) = parent.canonicalize() else {
            ok = false;
            continue;
        };
        if !parent_canonical.starts_with(&dest_canonical) {
            ok = false;
            continue;
        }
        let Some(file_name) = out_path.file_name() else {
            ok = false;
            continue;
        };
        let out_resolved = parent_canonical.join(file_name);
        let mut buf = Vec::new();
        if file.take(MAX_ZIP_ENTRY + 1).read_to_end(&mut buf).is_err() {
            ok = false;
            continue;
        }
        if buf.len() as u64 > MAX_ZIP_ENTRY {
            ok = false;
            continue;
        }
        total_unzipped = match total_unzipped.checked_add(buf.len() as u64) {
            Some(total) if total <= MAX_ZIP_DOWNLOAD => total,
            _ => {
                ok = false;
                continue;
            }
        };
        if std::fs::write(&out_resolved, &buf).is_err() {
            ok = false;
            continue;
        }
    }
    if ok {
        "true".to_string()
    } else {
        String::new()
    }
}

#[cfg(feature = "js-quickjs")]
fn java_get_txt_in_folder(dir_path: String) -> String {
    let resolved = match resolve_file_path(&dir_path) {
        Some(p) => p,
        None => return "[]".into(),
    };
    let Ok(entries) = std::fs::read_dir(&resolved) else {
        return "[]".into();
    };
    let txt_files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().map(|ext| ext == "txt").unwrap_or(false) {
                Some(path.file_name()?.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    serde_json::to_string(&txt_files).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn font_mappings_json(bytes: &[u8]) -> String {
    let face = match ttf_parser::Face::parse(bytes, 0) {
        Ok(f) => f,
        Err(_) => return "null".to_string(),
    };
    let cmap = match face.tables().cmap {
        Some(c) => c,
        None => return "null".to_string(),
    };
    let mut mappings: std::collections::HashMap<u32, u16> = std::collections::HashMap::new();
    for subtable in cmap.subtables {
        if !subtable.is_unicode() {
            continue;
        }
        subtable.codepoints(|codepoint| {
            if let Some(glyph) = subtable.glyph_index(codepoint) {
                mappings.insert(codepoint, glyph.0);
            }
        });
    }
    serde_json::to_string(&mappings).unwrap_or_else(|_| "null".to_string())
}

#[cfg(feature = "js-quickjs")]
fn java_query_base64_ttf(base64: String) -> String {
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&base64) {
        Ok(b) => b,
        Err(_) => return "null".to_string(),
    };
    font_mappings_json(&bytes)
}

#[cfg(feature = "js-quickjs")]
fn java_query_ttf(input: String) -> String {
    let bytes = if input.starts_with("http://") || input.starts_with("https://") {
        match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()
            .and_then(|c| c.get(&input).send().ok())
            .and_then(|r| {
                let max_bytes: usize = 10 * 1024 * 1024;
                let mut buf = Vec::new();
                use std::io::Read;
                r.take((max_bytes + 1) as u64)
                    .read_to_end(&mut buf)
                    .ok()
                    .filter(|_| buf.len() <= max_bytes)
                    .map(|_| buf)
            }) {
            Some(b) => b,
            None => return "null".to_string(),
        }
    } else if input.len() > 100 && !input.contains('/') && !input.contains('\\') {
        use base64::Engine;
        match base64::engine::general_purpose::STANDARD.decode(&input) {
            Ok(b) => b,
            Err(_) => return "null".to_string(),
        }
    } else {
        match read_allowed_file(&input) {
            Some(b) => b,
            None => return "null".to_string(),
        }
    };
    font_mappings_json(&bytes)
}

#[cfg(feature = "js-quickjs")]
fn java_replace_font(text: String, font1_json: String, font2_json: String) -> String {
    let mapping1: std::collections::HashMap<u32, u16> =
        serde_json::from_str(&font1_json).unwrap_or_default();
    let mapping2: std::collections::HashMap<u32, u16> =
        serde_json::from_str(&font2_json).unwrap_or_default();
    let mut glyph_to_codepoint: std::collections::HashMap<u16, u32> =
        std::collections::HashMap::new();
    for (&codepoint, &glyph) in &mapping2 {
        glyph_to_codepoint.insert(glyph, codepoint);
    }
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        let codepoint = ch as u32;
        if let Some(&glyph1) = mapping1.get(&codepoint) {
            if let Some(&replacement_cp) = glyph_to_codepoint.get(&glyph1) {
                if let Some(replacement_char) = std::char::from_u32(replacement_cp) {
                    result.push(replacement_char);
                    continue;
                }
            }
        }
        result.push(ch);
    }
    result
}

#[cfg(feature = "js-quickjs")]
fn java_read_file(path: String) -> String {
    let bytes = read_allowed_file(&path).unwrap_or_default();
    serde_json::to_string(&bytes).unwrap_or_else(|_| "[]".into())
}

#[cfg(feature = "js-quickjs")]
fn java_read_txt_file(path: String, charset: String) -> String {
    let bytes = read_allowed_file(&path).unwrap_or_default();
    decode_file_text(&bytes, &charset)
}

#[cfg(feature = "js-quickjs")]
fn read_allowed_file(path: &str) -> Option<Vec<u8>> {
    let root = std::env::var("LEGADO_FILE_ROOT").ok()?;
    let root = std::path::PathBuf::from(root).canonicalize().ok()?;
    let requested = root
        .join(path.trim_start_matches('/'))
        .canonicalize()
        .ok()?;
    if !requested.starts_with(&root) || !requested.is_file() {
        return None;
    }
    std::fs::read(requested).ok()
}

#[cfg(feature = "js-quickjs")]
fn decode_file_text(bytes: &[u8], charset: &str) -> String {
    if charset.trim().is_empty() {
        return String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| {
            let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
            decoded.into_owned()
        });
    }
    let label = charset.trim().to_lowercase();
    let encoding = match label.as_str() {
        "gbk" | "gb2312" | "gb18030" => {
            encoding_rs::Encoding::for_label(b"gbk").unwrap_or(encoding_rs::UTF_8)
        }
        "big5" => encoding_rs::Encoding::for_label(b"big5").unwrap_or(encoding_rs::UTF_8),
        "shift_jis" | "shift-jis" => {
            encoding_rs::Encoding::for_label(b"shift_jis").unwrap_or(encoding_rs::UTF_8)
        }
        "euc-kr" => encoding_rs::Encoding::for_label(b"euc-kr").unwrap_or(encoding_rs::UTF_8),
        _ => encoding_rs::UTF_8,
    };
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

// Basic Legado compatibility object. Network-backed java.* functions are added
// in the bridge layer later; keeping stubs here lets ordinary JS expressions run.
#[cfg(any(feature = "js-quickjs", feature = "js-boa"))]
const PREAMBLE: &str = r#"
var java = {
    url: typeof __legado_url__ !== 'undefined' ? String(__legado_url__) : '',
    headerMap: {
        _items: typeof __legado_headers__ !== 'undefined' ? __legado_headers__ : {},
        put: function(key, value) { this._items[String(key)] = String(value); return String(value); },
        get: function(key) { return this._items[String(key)] || ''; },
        remove: function(key) { delete this._items[String(key)]; },
        toObject: function() { return this._items; }
    },
    _defaultHeaders: typeof __legado_default_headers__ !== 'undefined' ? __legado_default_headers__ : {},
    _mergeHeaders: function(headers) {
        var merged = {};
        for (var key in this._defaultHeaders) merged[key] = this._defaultHeaders[key];
        if (headers) for (var key2 in headers) merged[key2] = headers[key2];
        return merged;
    },
    ajax: function(url) { return __legado_http_request('GET', String(url), '', JSON.stringify(this._mergeHeaders(null))); },
    connect: function(url) { return this.get(String(url || ''), {}); },
    ajaxAll: function(urlList) {
        var out = [];
        for (var i = 0; i < urlList.length; i++) {
            var body = __legado_http_request('GET', String(urlList[i]), '', JSON.stringify(this._mergeHeaders(null)));
            out.push({ body: function() { return body; }, toString: function() { return body; } });
        }
        return out;
    },
    post: function(url, body, headers) {
        var headersJson = JSON.stringify(this._mergeHeaders(headers));
        var responseBody = __legado_http_request('POST', String(url), String(body || ''), headersJson);
        return { body: function() { return responseBody; }, toString: function() { return responseBody; } };
    },
    get: function(keyOrUrl, headers) {
        if (arguments.length <= 1) return this._vars[String(keyOrUrl)] || '';
        var headersJson = JSON.stringify(this._mergeHeaders(headers));
        var responseBody = __legado_http_request('GET', String(keyOrUrl), '', headersJson);
        return { body: function() { return responseBody; }, toString: function() { return responseBody; } };
    },
    getCookie: function(tag, key) { return __legado_get_cookie(String(tag || ''), key == null ? '' : String(key)); },
    _vars: typeof __legado_variables__ !== 'undefined' ? __legado_variables__ : {},
    put: function(key, value) { this._vars[String(key)] = value; return value; },
    getFromMemory: function(key) { return __legado_cache_get(String(key || '')); },
    putMemory: function(key, value) { return __legado_cache_put(String(key || ''), value == null ? '' : String(value)); },
    setContent: function(content, baseUrl) { return __legado_set_content(String(content || ''), String(baseUrl || '')); },
    getString: function(rule, isUrl) {
        var content = typeof result !== 'undefined' && result !== null ? String(result) : String(src || '');
        return __legado_get_string(String(rule || ''), content, String(baseUrl || ''));
    },
    getStringList: function(rule, isUrl) {
        var content = typeof result !== 'undefined' && result !== null ? String(result) : String(src || '');
        return JSON.parse(__legado_get_string_list(String(rule || ''), content, String(baseUrl || '')));
    },
    getElements: function(rule) {
        var content = typeof result !== 'undefined' && result !== null ? String(result) : String(src || '');
        return JSON.parse(__legado_get_elements(String(rule || ''), content, String(baseUrl || ''))).map(function(item) {
            return java._wrapElement(item);
        });
    },
    _wrapElement: function(item) {
        return {
            _item: item || {},
            tagName: function() { return String(this._item.tagName || ''); },
            nodeName: function() { return this.tagName(); },
            text: function() { return String(this._item.text || ''); },
            ownText: function() { return String(this._item.ownText || ''); },
            html: function() { return String(this._item.html || ''); },
            outerHtml: function() { return String(this._item.outerHtml || ''); },
            attr: function(name) {
                var attrs = this._item.attrs || {};
                var key = String(name || '');
                return attrs[key] == null ? '' : String(attrs[key]);
            },
            hasAttr: function(name) {
                var attrs = this._item.attrs || {};
                return Object.prototype.hasOwnProperty.call(attrs, String(name || ''));
            },
            id: function() { return this.attr('id'); },
            className: function() { return this.attr('class'); },
            classNames: function() {
                return this.className().split(/\s+/).filter(function(x) { return x.length > 0; });
            },
            hasClass: function(name) {
                return this.classNames().indexOf(String(name || '')) >= 0;
            },
            absUrl: function(name) {
                var value = this.attr(name);
                if (!value) return '';
                return java._resolveUrl(value, String(baseUrl || ''));
            },
            children: function() {
                var children = this._item.children || [];
                return children.map(function(child) { return java._wrapElement(child); });
            },
            child: function(index) {
                var items = this.children();
                return items[Number(index) || 0] || null;
            },
            childNodeSize: function() { return (this._item.children || []).length; },
            select: function(rule) {
                return JSON.parse(__legado_get_elements(String(rule || ''), String(this._item.html || ''), String(baseUrl || ''))).map(function(child) {
                    return java._wrapElement(child);
                });
            },
            selectFirst: function(rule) {
                var items = this.select(rule);
                return items.length > 0 ? items[0] : null;
            },
            toString: function() { return this.text(); }
        };
    },
    _resolveUrl: function(value, base) {
        value = String(value || '');
        base = String(base || '');
        if (/^https?:\/\//i.test(value)) return value;
        var origin = (base.match(/^(https?:\/\/[^\/]+)/i) || [''])[0];
        if (value.indexOf('//') === 0) {
            var scheme = (base.match(/^(https?:)/i) || ['https:'])[0];
            return scheme + value;
        }
        if (value.charAt(0) === '/') return origin ? origin + value : value;
        var pathBase = base.split('#')[0].split('?')[0];
        pathBase = pathBase.substring(0, pathBase.lastIndexOf('/') + 1);
        return pathBase + value;
    },
    log: function(msg) { return __legado_log(String(msg || '')); },
    encodeURI: function(str) { return __legado_encode_uri(String(str)); },
    encodeURIComponent: function(str) { return __legado_encode_uri_component(String(str)); },
    decodeURI: function(str) { return __legado_decode_uri(String(str)); },
    base64Encode: function(str, _flags) { return __legado_base64_encode(String(str || '')); },
    base64Decode: function(str, _flags) { return __legado_base64_decode(String(str || '')); },
    base64DecodeToByteArray: function(str, _flags) { return JSON.parse(__legado_base64_decode_to_byte_array(String(str || ''))); },
    md5Encode: function(str) { return __legado_md5(String(str)); },
    md5Encode16: function(str) { return __legado_md5_16(String(str)); },
    aesDecodeToString: function(str, key, transformation, iv) { return __legado_aes_decode_to_string(String(str || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || '')); },
    aesBase64DecodeToString: function(str, key, transformation, iv) { return __legado_aes_base64_decode_to_string(String(str || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || '')); },
    aesEncodeToString: function(data, key, transformation, iv) { return __legado_aes_encode_to_string(String(data || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || '')); },
    aesEncodeToBase64String: function(data, key, transformation, iv) { return __legado_aes_encode_to_base64_string(String(data || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || '')); },
    aesDecodeToByteArray: function(str, key, transformation, iv) { return JSON.parse(__legado_aes_decode_to_byte_array(String(str || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || ''))); },
    aesBase64DecodeToByteArray: function(str, key, transformation, iv) { return JSON.parse(__legado_aes_base64_decode_to_byte_array(String(str || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || ''))); },
    aesEncodeToByteArray: function(data, key, transformation, iv) { return JSON.parse(__legado_aes_encode_to_byte_array(String(data || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || ''))); },
    aesEncodeToBase64ByteArray: function(data, key, transformation, iv) { return JSON.parse(__legado_aes_encode_to_base64_byte_array(String(data || ''), String(key || ''), String(transformation || 'AES/CBC/PKCS5Padding'), String(iv || ''))); },
    timeFormat: function(value) { return __legado_time_format(value == null ? '' : String(value)); },
    htmlFormat: function(value) { return __legado_html_format(value == null ? '' : String(value)); },
    getZipStringContent: function(url, path) { return __legado_get_zip_string_content(String(url || ''), String(path || '')); },
    getZipByteArrayContent: function(url, path) { return JSON.parse(__legado_get_zip_byte_array_content(String(url || ''), String(path || ''))); },
    readFile: function(path) { return JSON.parse(__legado_read_file(String(path || ''))); },
    readTxtFile: function(path, charsetName) { return __legado_read_txt_file(String(path || ''), charsetName == null ? '' : String(charsetName)); },
    downloadFile: function(url, path) { return __legado_download_file(String(url || ''), String(path || '')); },
    getFile: function(path) { return __legado_get_file(String(path || '')); },
    deleteFile: function(path) { return __legado_delete_file(String(path || '')); },
    unzipFile: function(zipPath, destDir) { return __legado_unzip_file(String(zipPath || ''), String(destDir || '')); },
    getTxtInFolder: function(dirPath) { return JSON.parse(__legado_get_txt_in_folder(String(dirPath || ''))); },
    utf8ToGbk: function(str) { return __legado_utf8_to_gbk(String(str)); },
    queryBase64Ttf: function(base64) { return __legado_query_base64_ttf(String(base64)); },
    queryTtf: function(input) { return __legado_query_ttf(String(input)); },
    replaceFont: function(text, font1Json, font2Json) { return __legado_replace_font(String(text), String(font1Json || ''), String(font2Json || '')); }
};

var cache = {
    getFromMemory: function(key) { return __legado_cache_get(String(key || '')); },
    putMemory: function(key, value) { return __legado_cache_put(String(key || ''), value == null ? '' : String(value)); },
    get: function(key) { return __legado_cache_get(String(key || '')); },
    put: function(key, value) { return __legado_cache_put(String(key || ''), value == null ? '' : String(value)); }
};

var source = {
    getKey: function() {
        if (typeof __source_url__ !== 'undefined') return __source_url__;
        if (typeof baseUrl !== 'undefined') {
            var m = String(baseUrl).match(/https?:\/\/[^\/]+/);
            return m ? m[0] : '';
        }
        return '';
    },
    key: ''
};
if (typeof __source_url__ !== 'undefined') source.key = __source_url__;
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn file_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_simple_eval() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("1 + 2", &vars).unwrap();
        assert!(matches!(result, LegadoValue::Int(3)));
    }

    #[test]
    fn test_string_concat() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval(r#""hello " + "world""#, &vars).unwrap();
        assert_eq!(result.as_str(), Some("hello world"));
    }

    #[test]
    fn test_with_variable() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://x.com/read/17047/".into()),
        );
        vars.insert("key".into(), LegadoValue::String("test".into()));
        let result = rt.eval("baseUrl + 's?q=' + key", &vars).unwrap();
        assert_eq!(result.as_str(), Some("https://x.com/read/17047/s?q=test"));
    }

    #[test]
    fn test_regex_match() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://x.com/read/17047/".into()),
        );
        let result = rt.eval("baseUrl.match(/read\\/(\\d+)/)[1]", &vars).unwrap();
        assert_eq!(result.as_str(), Some("17047"));
    }

    #[test]
    fn test_source_get_key() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "__source_url__".into(),
            LegadoValue::String("https://ixdzs8.com".into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://ixdzs8.com/read/17047/".into()),
        );
        let result = rt.eval("source.getKey()", &vars).unwrap();
        assert_eq!(result.as_str(), Some("https://ixdzs8.com"));
    }

    #[test]
    fn test_empty_script() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("", &vars).unwrap();
        assert!(result.is_null());
    }

    #[test]
    fn test_java_base64_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let encoded = rt.eval("java.base64Encode('test')", &vars).unwrap();
        assert_eq!(encoded.as_str(), Some("dGVzdA=="));

        let decoded = rt.eval("java.base64Decode('dGVzdA==')", &vars).unwrap();
        assert_eq!(decoded.as_str(), Some("test"));
    }

    #[test]
    fn test_java_md5_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.md5Encode('123')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("202cb962ac59075b964b07152d234b70"));

        let result = rt.eval("java.md5Encode16('123')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("ac59075b964b0715"));
    }

    #[test]
    fn test_java_uri_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval("java.encodeURIComponent('你好 test')", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("%E4%BD%A0%E5%A5%BD%20test"));

        let result = rt
            .eval("java.decodeURI('%E4%BD%A0%E5%A5%BD%20test')", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("你好 test"));
    }

    #[test]
    fn test_java_ajax_bridge() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/ajax");
            then.status(200).body("ajax-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval(&format!("java.ajax('{}')", server.url("/ajax")), &vars)
            .unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("ajax-ok"));
    }

    #[test]
    fn test_java_get_bridge_response_body() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/get")
                .header("X-Test", "1");
            then.status(200).body("get-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.get('{}', {{'X-Test':'1'}}).body()",
            server.url("/get")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("get-ok"));
    }

    #[test]
    fn test_java_post_bridge_response_body() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/post")
                .header("X-Test", "1")
                .body("a=1");
            then.status(200).body("post-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.post('{}', 'a=1', {{'X-Test':'1'}}).body()",
            server.url("/post")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("post-ok"));
    }

    #[test]
    fn test_java_ajax_uses_meta_charset_detection() {
        let server = httpmock::MockServer::start();
        let body = b"<html><head><meta charset=gbk></head><body>\xc4\xe3\xba\xc3</body></html>";
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/gbk-meta");
            then.status(200)
                .header("Content-Type", "text/html")
                .body(body.as_slice());
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval(&format!("java.ajax('{}')", server.url("/gbk-meta")), &vars)
            .unwrap();

        mock.assert();
        assert!(result.as_str().unwrap_or_default().contains("你好"));
    }

    #[test]
    fn test_java_get_explicit_charset_header() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/gbk-explicit");
            then.status(200)
                .header("Content-Type", "text/plain")
                .body(&b"\xc4\xe3\xba\xc3"[..]);
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.get('{}', {{charset:'gbk'}}).body()",
            server.url("/gbk-explicit")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("你好"));
    }

    #[test]
    fn test_java_get_string_bridge() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<div><a href="/b/1">Book</a></div>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let result = rt
            .eval("java.getString('@css:a@text', false)", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("Book"));
    }

    #[test]
    fn test_java_get_string_list_bridge() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<ul><li>A</li><li>B</li></ul>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let result = rt
            .eval("java.getStringList('@css:li@text', false).join(',')", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("A,B"));
    }

    #[test]
    fn test_java_get_elements_bridge() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<ul><li>A</li><li>B</li></ul>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let result = rt
            .eval("java.getElements('@css:li').length", &vars)
            .unwrap();
        assert!(matches!(result, LegadoValue::Int(2)));
    }

    #[test]
    fn test_java_get_elements_element_methods() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(
                r#"<div class="book" data-id="7"><a href="/b/7"><span>Book</span></a></div>"#
                    .into(),
            ),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let script = "var el = java.getElements('@css:div.book')[0]; [el.text(), el.attr('data-id'), el.selectFirst('a').attr('href'), el.selectFirst('span').text()].join('|')";
        let result = rt.eval(script, &vars).unwrap();

        assert_eq!(result.as_str(), Some("Book|7|/b/7|Book"));
    }

    #[test]
    fn test_java_get_elements_to_string_returns_text() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<ul><li>A</li><li>B</li></ul>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let result = rt
            .eval("java.getElements('@css:li').join(',')", &vars)
            .unwrap();

        assert_eq!(result.as_str(), Some("A,B"));
    }

    #[test]
    fn test_java_get_elements_more_element_methods() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert("src".into(), LegadoValue::String(r#"<div id="root" class="book hot" data-id="7">Own <a href="/b/7">Link</a><span>Span</span></div>"#.into()));
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com/base/page.html".into()),
        );

        let script = "var el = java.getElements('@css:div')[0]; [el.tagName(), el.id(), el.hasClass('hot'), el.ownText().trim(), el.children().length, el.child(0).tagName(), el.childNodeSize(), el.selectFirst('a').absUrl('href')].join('|')";
        let result = rt.eval(script, &vars).unwrap();

        assert_eq!(
            result.as_str(),
            Some("div|root|true|Own|2|a|2|https://example.com/b/7")
        );
    }

    #[test]
    fn test_java_get_elements_has_attr_and_class_names() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<p class="a b" title="T">Text</p>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let script = "var el = java.getElements('@css:p')[0]; [el.hasAttr('title'), el.hasAttr('missing'), el.classNames().join(',')].join('|')";
        let result = rt.eval(script, &vars).unwrap();

        assert_eq!(result.as_str(), Some("true|false|a,b"));
    }

    #[test]
    fn test_url_option_js_bridge() {
        let context = UrlJsContext::new("https://example.com/a", &[("A".into(), "1".into())]);
        let updated = eval_url_option_js(
            "java.url = java.url + '?p=1'; java.headerMap.put('X-Test', 'ok')",
            &context,
        )
        .unwrap();

        assert_eq!(updated.url, "https://example.com/a?p=1");
        assert!(updated.headers.contains(&("A".into(), "1".into())));
        assert!(updated.headers.contains(&("X-Test".into(), "ok".into())));
    }

    #[test]
    fn test_java_put_get_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval("java.put('token', 'abc') && java.get('token')", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("abc"));
    }

    #[test]
    fn test_java_get_from_context_variables() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        let mut stored = HashMap::new();
        stored.insert("token".into(), LegadoValue::String("abc".into()));
        vars.insert("__legado_variables__".into(), LegadoValue::Map(stored));
        let result = rt.eval("java.get('token')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("abc"));
    }

    #[test]
    fn test_cache_memory_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval(
                "cache.putMemory('bid', '42'); cache.getFromMemory('bid')",
                &vars,
            )
            .unwrap();
        assert_eq!(result.as_str(), Some("42"));
    }

    #[test]
    fn test_java_get_cookie_bridge() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/cookie");
            then.status(200)
                .header("Set-Cookie", "sid=abc; Path=/")
                .body("cookie-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.ajax('{}') && java.getCookie('{}', 'sid')",
            server.url("/cookie"),
            server.url("/")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("abc"));
    }

    #[test]
    fn test_java_get_cookie_all_bridge() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/cookie-all");
            then.status(200)
                .header("Set-Cookie", "token=xyz; Path=/")
                .body("cookie-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.ajax('{}') && java.getCookie('{}', null)",
            server.url("/cookie-all"),
            server.url("/")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert!(result.as_str().unwrap_or_default().contains("token=xyz"));
    }

    #[test]
    fn test_java_aes_cbc_base64_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = r#"
            (function(){
                var key = '1234567890123456';
                var iv = 'abcdefghijklmnop';
                var enc = java.aesEncodeToBase64String('hello', key, 'AES/CBC/PKCS5Padding', iv);
                return java.aesBase64DecodeToString(enc, key, 'AES/CBC/PKCS5Padding', iv);
            })()
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn test_java_aes_ecb_hex_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = r#"
            (function(){
                var key = '1234567890123456';
                var enc = java.aesEncodeToString('hello', key, 'AES/ECB/PKCS5Padding', '');
                return java.aesDecodeToString(enc, key, 'AES/ECB/PKCS5Padding', '');
            })()
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn test_java_aes_256_cbc_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = r#"
            (function(){
                var key = '12345678901234567890123456789012';
                var iv = 'abcdefghijklmnop';
                var enc = java.aesEncodeToBase64String('hello-256', key, 'AES/CBC/PKCS5Padding', iv);
                return java.aesBase64DecodeToString(enc, key, 'AES/CBC/PKCS5Padding', iv);
            })()
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert_eq!(result.as_str(), Some("hello-256"));
    }

    #[test]
    fn test_java_aes_byte_array_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = r#"
            (function(){
                var key = '1234567890123456';
                var iv = 'abcdefghijklmnop';
                var enc = java.aesEncodeToBase64String('hello', key, 'AES/CBC/PKCS5Padding', iv);
                return java.aesBase64DecodeToByteArray(enc, key, 'AES/CBC/PKCS5Padding', iv).join(',');
            })()
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert_eq!(result.as_str(), Some("104,101,108,108,111"));
    }

    #[test]
    fn test_java_aes_encode_to_byte_array_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = r#"
            (function(){
                var key = '1234567890123456';
                var iv = 'abcdefghijklmnop';
                return java.aesEncodeToByteArray('hello', key, 'AES/CBC/PKCS5Padding', iv).length > 0;
            })()
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert!(matches!(result, LegadoValue::Bool(true)));
    }

    #[test]
    fn test_java_time_format_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.timeFormat(0)", &vars).unwrap();
        assert_eq!(result.as_str(), Some("1970/01/01 00:00"));

        let result = rt.eval("java.timeFormat(60000)", &vars).unwrap();
        assert_eq!(result.as_str(), Some("1970/01/01 00:01"));
    }

    #[test]
    fn test_java_html_format_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval(
                "java.htmlFormat('<p>A&nbsp;&amp;B</p><br><script>x</script><p>C</p>')",
                &vars,
            )
            .unwrap();
        assert_eq!(result.as_str(), Some("A &B\nC"));
    }

    #[test]
    fn test_java_get_zip_string_content_bridge() {
        let zip = build_test_zip("dir/a.txt", b"zip-ok");
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/test.zip");
            then.status(200).body(zip.clone());
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.getZipStringContent('{}', 'dir/a.txt')",
            server.url("/test.zip")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("zip-ok"));
    }

    #[test]
    fn test_java_get_zip_byte_array_content_bridge() {
        let zip = build_test_zip("a.bin", &[1, 2, 3]);
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/bytes.zip");
            then.status(200).body(zip.clone());
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.getZipByteArrayContent('{}', 'a.bin').join(',')",
            server.url("/bytes.zip")
        );
        let result = rt.eval(&script, &vars).unwrap();

        mock.assert();
        assert_eq!(result.as_str(), Some("1,2,3"));
    }

    fn build_test_zip(path: &str, content: &[u8]) -> Vec<u8> {
        use std::io::{Cursor, Write};

        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        writer.start_file(path, options).unwrap();
        writer.write_all(content).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn test_java_read_txt_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "file-ok").unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.readTxtFile('a.txt')", &vars).unwrap();

        assert_eq!(result.as_str(), Some("file-ok"));
    }

    #[test]
    fn test_java_read_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), [1, 2, 3]).unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.readFile('a.bin').join(',')", &vars).unwrap();

        assert_eq!(result.as_str(), Some("1,2,3"));
    }

    #[test]
    fn test_java_read_file_blocks_path_escape() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let escaped = format!("{}/secret.txt", outside.path().display());
        let result = rt
            .eval(&format!("java.readTxtFile('{}')", escaped), &vars)
            .unwrap();

        assert_eq!(result.as_str(), Some(""));
    }

    #[test]
    fn test_java_connect_bridge() {
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/connect-test");
            then.status(200).body("connect-ok");
        });

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!("java.connect('{}').body()", server.url("/connect-test"));
        let result = rt.eval(&script, &vars).unwrap();
        mock.assert();
        assert_eq!(result.as_str(), Some("connect-ok"));
    }

    #[test]
    fn test_java_base64_decode_to_byte_array_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval("java.base64DecodeToByteArray('AQIDBA==').join(',')", &vars)
            .unwrap();
        assert_eq!(result.as_str(), Some("1,2,3,4"));
    }

    #[test]
    fn test_java_log_bridge() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.log('test-message')", &vars).unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    #[test]
    fn test_java_set_content_bridge() {
        let rt = DefaultJsRuntime::new();
        let mut vars = HashMap::new();
        vars.insert(
            "src".into(),
            LegadoValue::String(r#"<html><body><div class="x">ignored</div></body></html>"#.into()),
        );
        vars.insert(
            "baseUrl".into(),
            LegadoValue::String("https://example.com".into()),
        );

        let script = r#"
            java.setContent('<html><body><div class="x">setcontent-ok</div></body></html>', 'https://example.com');
            java.getString('@css:div.x@text', false)
        "#;
        let result = rt.eval(script, &vars).unwrap();
        assert_eq!(result.as_str(), Some("setcontent-ok"));
    }

    #[test]
    fn test_java_base64_encode_decode_flags() {
        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let encoded = rt.eval("java.base64Encode('test', 0)", &vars).unwrap();
        assert_eq!(encoded.as_str(), Some("dGVzdA=="));
        let decoded = rt.eval("java.base64Decode('dGVzdA==', 0)", &vars).unwrap();
        assert_eq!(decoded.as_str(), Some("test"));
    }

    #[test]
    fn test_java_download_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let server = httpmock::MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/dl/test.txt");
            then.status(200).body("downloaded-content");
        });

        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let script = format!(
            "java.downloadFile('{}/dl/test.txt', 'saved.txt')",
            server.base_url()
        );
        let result = rt.eval(&script, &vars).unwrap();
        mock.assert();
        assert_eq!(result.as_str(), Some("true"));

        let saved = std::fs::read_to_string(dir.path().join("saved.txt")).unwrap();
        assert_eq!(saved, "downloaded-content");
    }

    #[test]
    fn test_java_get_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "file-content").unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.getFile('test.txt')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("file-content"));
    }

    #[test]
    fn test_java_delete_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("to_delete.txt");
        std::fs::write(&path, "delete-me").unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.deleteFile('to_delete.txt')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("true"));
        assert!(!path.exists());
    }

    #[test]
    fn test_java_unzip_file_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let zip_data = build_test_zip("inner.txt", b"zipped-content");
        std::fs::write(dir.path().join("test.zip"), &zip_data).unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt.eval("java.unzipFile('test.zip', 'out')", &vars).unwrap();
        assert_eq!(result.as_str(), Some("true"));

        let extracted = std::fs::read_to_string(dir.path().join("out").join("inner.txt")).unwrap();
        assert_eq!(extracted, "zipped-content");
    }

    #[test]
    fn test_java_get_txt_in_folder_bridge() {
        let _guard = file_env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        std::fs::write(dir.path().join("c.dat"), "c").unwrap();
        std::env::set_var("LEGADO_FILE_ROOT", dir.path());

        let rt = DefaultJsRuntime::new();
        let vars = HashMap::new();
        let result = rt
            .eval("java.getTxtInFolder('.').join(',')", &vars)
            .unwrap();
        let files: Vec<&str> = result.as_str().unwrap_or("").split(',').collect();
        assert!(files.contains(&"a.txt"));
        assert!(files.contains(&"b.txt"));
        assert!(!files.contains(&"c.dat"));
    }
}
