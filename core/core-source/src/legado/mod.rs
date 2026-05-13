//! Legado 书源规则兼容层

pub mod value;
pub mod import;
pub mod url;
pub mod http;
pub mod selector;
pub mod regex_rule;
pub mod context;
pub mod rule;
pub mod js_runtime;
pub mod js_shim;

pub use value::LegadoValue;
pub use import::{LegadoBookSource, import_legado_source, normalize_legado_rule};
pub use url::{LegadoUrl, UrlOption, resolve_url_template, resolve_rule_template};
pub use http::LegadoHttpClient;
pub use selector::LegadoSelectorChain;
pub use context::RuleContext;
pub use rule::{
    execute_legado_rule,
    execute_legado_rule_values,
    execute_legado_rule_values_with_cookie_jar,
    execute_legado_rule_with_cookie_jar,
    execute_legado_rule_values_with_http_state,
    execute_legado_rule_with_http_state,
};
pub use js_runtime::{DefaultJsRuntime, JsRuntime, JsRuntimeConfig};
pub use js_shim::{is_js_rule, build_js_vars};
