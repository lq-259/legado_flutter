//! # bridge - Flutter-Rust 桥接层
//!
//! 通过 flutter_rust_bridge 实现 Dart 与 Rust 的双向调用。

mod frb_generated;

pub use api::*;
pub mod api;

/// 测试函数 - 验证桥接是否正常工作
pub fn ping() -> String {
    "pong".to_string()
}
