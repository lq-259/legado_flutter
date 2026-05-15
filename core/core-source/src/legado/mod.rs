//! Legado 书源规则兼容层

pub mod context;
pub mod http;
pub mod import;
pub mod js_runtime;
pub mod js_shim;
pub mod regex_rule;
pub mod rule;
pub mod selector;
pub mod url;
pub mod value;

pub use context::RuleContext;
pub use http::LegadoHttpClient;
pub use import::{import_legado_source, normalize_legado_rule, LegadoBookSource};
pub use js_runtime::{DefaultJsRuntime, JsRuntime, JsRuntimeConfig};
pub use js_shim::{build_js_vars, is_js_rule};
pub use rule::{
    execute_legado_rule, execute_legado_rule_values, execute_legado_rule_values_with_cookie_jar,
    execute_legado_rule_values_with_http_state, execute_legado_rule_with_cookie_jar,
    execute_legado_rule_with_http_state,
};
pub use selector::LegadoSelectorChain;
pub use url::{resolve_rule_template, resolve_url_template, LegadoUrl, UrlOption};
pub use value::LegadoValue;
