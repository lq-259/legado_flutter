//! Legado 统一值类型
//!
//! 用于在规则执行过程中统一表示 HTML 节点、JSON 值、字符串、数组等。
//! 对应 Legado 的 Any 类型处理。

use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Legado 规则执行中的统一值类型
#[derive(Debug, Clone)]
pub enum LegadoValue {
    /// 字符串
    String(String),
    /// 整数
    Int(i64),
    /// 浮点数
    Float(f64),
    /// 布尔值
    Bool(bool),
    /// HTML 片段（字符串形式）
    Html(String),
    /// 数组
    Array(Vec<LegadoValue>),
    /// 键值对
    Map(HashMap<String, LegadoValue>),
    /// 空值
    Null,
}

impl LegadoValue {
    pub fn is_null(&self) -> bool {
        matches!(self, LegadoValue::Null)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            LegadoValue::String(s) | LegadoValue::Html(s) => s.is_empty(),
            LegadoValue::Array(a) => a.is_empty(),
            LegadoValue::Map(m) => m.is_empty(),
            LegadoValue::Null => true,
            _ => false,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            LegadoValue::String(s) | LegadoValue::Html(s) => s.len(),
            LegadoValue::Array(a) => a.len(),
            LegadoValue::Map(m) => m.len(),
            _ => 0,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            LegadoValue::String(s) | LegadoValue::Html(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_string_lossy(&self) -> String {
        match self {
            LegadoValue::String(s) | LegadoValue::Html(s) => s.clone(),
            LegadoValue::Int(i) => i.to_string(),
            LegadoValue::Float(f) => f.to_string(),
            LegadoValue::Bool(b) => b.to_string(),
            LegadoValue::Null => String::new(),
            LegadoValue::Array(arr) => {
                arr.iter()
                    .map(|v| v.as_string_lossy())
                    .collect::<Vec<_>>()
                    .join("")
            }
            LegadoValue::Map(m) => {
                let mut s = String::new();
                for (_, v) in m {
                    s.push_str(&v.as_string_lossy());
                }
                s
            }
        }
    }

    pub fn as_array(&self) -> Option<&[LegadoValue]> {
        match self {
            LegadoValue::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&HashMap<String, LegadoValue>> {
        match self {
            LegadoValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&LegadoValue> {
        match self {
            LegadoValue::Map(m) => m.get(key),
            LegadoValue::Array(arr) => {
                if let Ok(idx) = key.parse::<usize>() {
                    arr.get(idx)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// 转为 JSON Value
    pub fn to_json_value(&self) -> JsonValue {
        match self {
            LegadoValue::String(s) | LegadoValue::Html(s) => JsonValue::String(s.clone()),
            LegadoValue::Int(i) => JsonValue::Number((*i).into()),
            LegadoValue::Float(f) => {
                serde_json::Number::from_f64(*f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
            LegadoValue::Bool(b) => JsonValue::Bool(*b),
            LegadoValue::Null => JsonValue::Null,
            LegadoValue::Array(arr) => {
                JsonValue::Array(arr.iter().map(|v| v.to_json_value()).collect())
            }
            LegadoValue::Map(m) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in m {
                    obj.insert(k.clone(), v.to_json_value());
                }
                JsonValue::Object(obj)
            }
        }
    }

    /// 从 JSON Value 转换
    pub fn from_json_value(v: &JsonValue) -> Self {
        match v {
            JsonValue::Null => LegadoValue::Null,
            JsonValue::Bool(b) => LegadoValue::Bool(*b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    LegadoValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    LegadoValue::Float(f)
                } else {
                    LegadoValue::String(n.to_string())
                }
            }
            JsonValue::String(s) => LegadoValue::String(s.clone()),
            JsonValue::Array(arr) => {
                LegadoValue::Array(arr.iter().map(Self::from_json_value).collect())
            }
            JsonValue::Object(obj) => {
                let mut map = HashMap::new();
                for (k, v) in obj {
                    map.insert(k.clone(), Self::from_json_value(v));
                }
                LegadoValue::Map(map)
            }
        }
    }
}

impl std::fmt::Display for LegadoValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LegadoValue::String(s) => write!(f, "{}", s),
            LegadoValue::Int(i) => write!(f, "{}", i),
            LegadoValue::Float(v) => write!(f, "{}", v),
            LegadoValue::Bool(b) => write!(f, "{}", b),
            LegadoValue::Null => write!(f, ""),
            LegadoValue::Html(s) => write!(f, "{}", s),
            LegadoValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            LegadoValue::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

/// 将 LegadoValue 数组转为字符串数组
pub fn legado_values_to_strings(values: &[LegadoValue]) -> Vec<String> {
    values.iter().map(|v| v.as_string_lossy()).collect()
}

/// 将字符串数组转为 LegadoValue 数组
pub fn strings_to_legado_values(strings: &[String]) -> Vec<LegadoValue> {
    strings.iter().map(|s| LegadoValue::String(s.clone())).collect()
}
