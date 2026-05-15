//! # core-net - 网络引擎模块
//!
//! 负责处理所有 HTTP 请求相关功能，对应原 Legado 的 `help/http/` 和 `lib/cronet/` 模块。
//! 使用 reqwest + rustls 实现跨平台 TLS 支持，避免系统依赖。
//! tokio 提供异步运行时，支持高并发书源请求。

pub mod client;
pub mod cookie;
pub mod downloader;
pub mod encoding;
pub mod proxy;
pub mod retry;

// 重新导出主要类型，方便上层调用
pub use client::{HttpClient, HttpClientConfig};
pub use cookie::CookieManager;
pub use encoding::detect_and_decode;
pub use proxy::{ProxyConfig, ProxyManager, ProxyType};

use std::time::Duration;

/// 请求超时配置
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// 重试配置
pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_BASE_BACKOFF_MS: u64 = 100;
