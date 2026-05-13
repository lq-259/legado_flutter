//! # 重试机制模块
//!
//! 提供带指数退避的重试逻辑。
//! 对应原 Legado 的网络重试逻辑。

use std::time::Duration;
use tracing::warn;

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_backoff_ms: 100,
            max_backoff_ms: 10000, // 10秒
        }
    }
}

/// 重试执行器
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// 执行带重试的操作（指数退避）
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut retries = 0;
        
        loop {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(e) => {
                    if retries >= self.config.max_retries {
                        return Err(e);
                    }
                    
                    // 计算退避时间：base * 2^retries
                    let backoff_ms = self.config.base_backoff_ms * 2u64.pow(retries as u32);
                    let backoff_ms = backoff_ms.min(self.config.max_backoff_ms);
                    
                    warn!("操作失败: {}，{}ms 后重试（{}/{}）", 
                         e, backoff_ms, retries + 1, self.config.max_retries);
                    
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    retries += 1;
                }
            }
        }
    }
}
