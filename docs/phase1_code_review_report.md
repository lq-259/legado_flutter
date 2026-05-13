# Phase 1 Code Review Report

> ⚠️ **历史审查报告**：本报告生成于 2026-04-30 22:36:55（审核模型: gpt-5.5），记录当时的 P1 问题。当前代码已继续演进，部分问题已修复。请勿将本报告中的所有未修复项直接视为当前事实；最新状态以 `CURRENT_STATUS.md` 和最新验证为准。

**审核时间**: 2026-04-30 22:36:55
**审核模型**: gpt-5.5
**审核范围**: 4 个 crate

---

## 📦 core-net
**职责**: 网络层 - HTTP 客户端、Cookie、代理

## 总体结论：core-net 仍有 P1 级问题

该 crate 结构较清晰，职责划分基本合理：`client`、`cookie`、`proxy`、`retry`、`encoding` 分层明确。但目前仍存在若干生产可用性问题，尤其是：

- **Cookie 持久化语义仍未完全修复**：代码注释声称只保存持久化 Cookie，但实现直接序列化整个 `CookieStore`，语义依赖第三方实现细节，测试覆盖不足。
- **HTTP 并发限制范围不完整**：信号量只覆盖 `send()`，不覆盖 body 读取。
- **存在 panic 风险**：`Semaphore::acquire().await.expect(...)`、指数退避溢出风险、`Default` 中 `expect`。
- **代理日志泄露敏感信息**：`info!("使用代理: {}", proxy_url)` 会打印用户名密码。
- **`clear_domain` 未实现但暴露为 API**。
- **错误类型大量使用 `Box<dyn Error>` / `String`，不利于上层分类处理。**

整体评价：**P1**  
建议在合入主分支前修复 Cookie 持久化语义、panic 风险、代理敏感信息泄露和未实现 API。

---

# 文件审查

---

## client.rs

评分：**B-**

### 优点

- 基于 `reqwest::Client` 封装，结构清晰。
- 支持默认 header、超时、代理、cookie store、重试、并发限制。
- `get_text` 在消费 response body 前先读取 headers，避免了常见 ownership 问题。
- charset 解析逻辑基本可用。

### 问题与风险

#### 1. 注释错误：`cookie_store(true); // 启用 Brotli 压缩`

```rust
.cookie_store(true);       // 启用 Brotli 压缩
```

这里实际是启用 reqwest 内部 Cookie Store，不是 Brotli。

建议改为：

```rust
.cookie_store(true); // 启用 Cookie 自动管理
```

如果要启用 Brotli，需要确认 `reqwest` feature 和相关配置。

---

#### 2. `HttpClient` 与 `CookieManager` 是两套 Cookie 系统，语义割裂

`client.rs` 使用：

```rust
.cookie_store(true)
```

这会启用 reqwest 内部 cookie store。

但 `cookie.rs` 又定义了独立的 `CookieManager`。当前二者没有集成：

- `HttpClient` 的 Cookie 无法持久化到 `CookieManager`。
- `CookieManager` 中保存的 Cookie 无法自动用于 `HttpClient` 请求。
- 上层很容易误以为 `CookieManager::save_persistent_cookies` 能保存 `HttpClient` 的请求 Cookie，但实际不能。

这是网络层 API 语义上的重要问题。

建议：

- 使用 `reqwest_cookie_store::CookieStoreMutex` 或 `CookieStoreRwLock` 统一管理。
- `HttpClient` 持有共享 cookie store。
- `CookieManager` 作为该共享 store 的 facade。
- 或者明确拆分：`HttpClient` 不内置 cookie store，所有 Cookie 必须通过 `CookieManager` 注入 header。

---

#### 3. 并发限制只覆盖 `send()`，不覆盖 body 读取

当前逻辑：

```rust
let permit = self.semaphore.acquire().await.expect("信号量已关闭");
let response = request_fn().send().await;
drop(permit);
return Ok(resp);
```

这只限制了请求发送阶段。对于 `get_text()`：

```rust
let response = self.get(url).await?;
let bytes = response.bytes().await?;
```

body 读取发生在 permit 已释放后。

如果响应 body 很大或大量请求同时读取 body，实际并发资源控制会失效。

建议：

- 提供 `get_text_with_retry`，将 `send + bytes` 放在 permit 生命周期内。
- 或者将 API 改为返回完整 body，而不是裸 `Response`。
- 如果必须返回 `Response`，则文档中要明确并发限制只覆盖建立请求阶段，不覆盖 body 消费。

---

#### 4. `Semaphore::acquire().await.expect(...)` 有 panic 风险

```rust
let permit = self.semaphore.acquire().await
    .expect("信号量已关闭");
```

虽然当前代码没有显式关闭 semaphore，但库代码中不应依赖 panic。

建议改为错误返回。由于函数返回 `reqwest::Error` 不方便表达 semaphore 错误，也说明当前错误类型设计不够好。建议引入自定义错误：

```rust
#[derive(thiserror::Error, Debug)]
pub enum NetError {
    #[error("request failed: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("semaphore closed")]
    SemaphoreClosed,

    #[error("invalid header: {0}")]
    InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),
}
```

---

#### 5. `max_concurrent = 0` 会导致所有请求永久等待

```rust
Semaphore::new(config.max_concurrent)
```

如果用户传入 0，所有请求都会阻塞。

建议在 `HttpClient::new` 中校验：

```rust
if config.max_concurrent == 0 {
    return Err("max_concurrent must be greater than 0".into());
}
```

更推荐返回结构化错误。

---

#### 6. 重试退避存在溢出风险

```rust
100 * 2u64.pow(retries as u32)
```

如果 `max_retries` 被配置得很大，会有溢出风险。Debug 下可能 panic，Release 下可能 wrap。

建议使用 `saturating_mul` / `checked_pow`，并设置最大 backoff：

```rust
let backoff = Duration::from_millis(
    100u64
        .saturating_mul(1u64.checked_shl(retries as u32).unwrap_or(u64::MAX))
        .min(10_000)
);
```

或复用 `retry.rs` 中的 `RetryExecutor`。

---

#### 7. `retry.rs` 没有被 `client.rs` 复用，逻辑重复

`client.rs` 自己实现了一套 retry，`retry.rs` 又有一套通用 retry。重复逻辑容易产生行为差异。

建议：

- `HttpClient` 复用 `RetryExecutor`。
- 或删除 `retry.rs`，避免死代码。

---

#### 8. `post` 被声明为 async 但没有 await

```rust
pub async fn post(&self, url: &str) -> RequestBuilder {
    self.client.post(url)
}
```

这个函数没有异步操作，不应该是 `async`。

建议：

```rust
pub fn post(&self, url: &str) -> RequestBuilder {
    self.client.post(url)
}
```

---

#### 9. `request_with_retry` 的泛型约束不必要

```rust
F: Fn() -> RequestBuilder + Clone,
```

这里没有使用 `Clone`。

建议：

```rust
F: Fn() -> RequestBuilder,
```

---

#### 10. 非 2xx 响应不会作为错误返回

```rust
if resp.status().is_success() {
    return Ok(resp);
} else {
    return Ok(resp);
}
```

这可以接受，但要明确 API 语义。很多调用者会以为 `get()` 返回 `Ok` 表示 HTTP 语义成功。

建议提供两类 API：

```rust
get_raw() -> Result<Response, NetError>
get_success() -> Result<Response, NetError> // 内部 error_for_status
```

---

#### 11. `get_text` charset 处理重复且不完整

`encoding.rs` 已经提供了检测函数，但 `client.rs` 又有自己的逻辑。

问题：

- charset 未使用 `encoding_rs::Encoding::for_label`。
- `gbk` / `gb2312` / `gb18030` 手写映射有限。
- 无 charset 时只检测 BOM，不做智能检测。
- `String::from_utf8_lossy` 对非 UTF-8 内容可能产生乱码。

建议统一到 `encoding.rs`：

```rust
if let Some(label) = charset {
    if let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes()) {
        let (text, _, _) = enc.decode(&bytes);
        return Ok(text.into_owned());
    }
}

let (text, _) = crate::encoding::detect_and_decode(&bytes);
Ok(text)
```

---

### 建议修复优先级

高：

- 修复 CookieManager 与 HttpClient 的割裂。
- 修复 semaphore panic 和 `max_concurrent = 0`。
- 修复代理日志泄密。
- 统一 retry 实现。

中：

- `post` 去掉 async。
- charset 解析统一。
- backoff 增加上限和 saturating 逻辑。

---

## cookie.rs

评分：**C+**

这是本 crate 中最需要重点修复的文件。

### 优点

- API 目标明确：添加、读取、保存、加载、清空 Cookie。
- 基本测试覆盖了添加、读取、清空、持久化加载。
- 相比会话 Cookie 混淆的问题，注释已经开始强调“仅持久化 Cookie”的语义，这是正确方向。

### 重点问题：P1-1/2 Cookie 持久化测试与 API 语义仍未完全修复

代码注释写道：

```rust
// save_persistent_cookies / load_persistent_cookies 仅保存/加载持久化 Cookie
// cookie_store 的 serde_json 序列化默认会跳过到期和会话 Cookie。
```

但实际实现是：

```rust
let json = serde_json::to_string_pretty(&self.store)?;
```

以及：

```rust
let store: CookieStore = serde_json::from_str(&contents)?;
```

这意味着当前行为完全依赖 `cookie_store` 的 serde 实现细节。代码本身没有显式过滤：

- session cookie
- expired cookie
- non-persistent cookie

因此 API 名称 `save_persistent_cookies` 的语义没有由本 crate 保证。

这属于之前指出的 **P1-1/2** 问题：**仍未充分修复**。

### 建议

必须二选一：

#### 方案 A：显式实现“只保存持久化 Cookie”

保存时遍历 cookie store，只导出：

- 未过期 Cookie
- 存在 `Expires` 或 `Max-Age` 的 Cookie

加载时也进行校验，避免加载 session cookie。

如果 `cookie_store` 当前 API 不方便遍历持久化属性，需要考虑使用它提供的专用 save/load API，或切换到 `reqwest_cookie_store` 生态中更适合持久化的接口。

#### 方案 B：修改 API 名称和文档

如果确实想保存整个 CookieStore，则应改名：

```rust
save_cookies
load_cookies
```

并删除“仅持久化 Cookie”的承诺。

---

### 测试不足

当前持久化测试只有：

```rust
manager.add_cookie("loaded=true; Max-Age=3600", "https://example.com").unwrap();
```

只验证了持久化 Cookie 能保存。

但没有验证：

1. 会话 Cookie 不会保存。
2. 过期 Cookie 不会保存。
3. `Expires` 格式 Cookie 能保存。
4. `Max-Age=0` 的 Cookie 不会保存。
5. 加载不存在文件时行为是否符合预期。
6. 文件损坏时错误是否可诊断。
7. 同名 Cookie 覆盖规则。
8. path/domain 匹配行为。

至少应补充：

```rust
#[test]
fn test_session_cookie_not_persisted() {
    let mut manager = CookieManager::default();
    manager.add_cookie("session=abc", "https://example.com").unwrap();

    let path = temp_dir().join("test_session_cookie.json");
    manager.save_persistent_cookies(&path).unwrap();

    let loaded = CookieManager::load_persistent_cookies(&path).unwrap();
    let cookies = loaded.get_cookies("https://example.com").unwrap();

    assert!(!cookies.contains("session=abc"));

    fs::remove_file(&path).ok();
}
```

如果这个测试失败，就说明 P1-1/2 未修复。

---

### `clear_domain` 未实现但暴露 API

```rust
pub fn clear_domain(&mut self, _domain: &str) {
    debug!("清除域名 Cookie: {}", _domain);
    warn!("clear_domain 功能需要完整实现");
}
```

这是危险 API：调用方会以为清除了，但实际没有。

建议：

- 要么实现。
- 要么删除。
- 要么返回 `Result<(), NetError>` 并返回 `Unsupported`。

例如：

```rust
pub fn clear_domain(&mut self, domain: &str) -> Result<(), NetError> {
    // implemented
}
```

或临时：

```rust
pub fn clear_domain(&mut self, _domain: &str) -> Result<(), NetError> {
    Err(NetError::Unsupported("clear_domain is not implemented"))
}
```

---

### `Default` 中不应 `expect`

```rust
impl Default for CookieManager {
    fn default() -> Self {
        Self::new().expect("创建 CookieManager 失败")
    }
}
```

当前 `new()` 实际不会失败，却返回 `Result`。这导致 Default 中出现不必要 panic。

建议：

```rust
pub fn new() -> Self {
    Self {
        store: CookieStore::default(),
    }
}

impl Default for CookieManager {
    fn default() -> Self {
        Self::new()
    }
}
```

---

### 文件 IO 错误处理可以更好

目前统一返回：

```rust
Result<Self, Box<dyn std::error::Error>>
```

建议定义错误类型：

```rust
#[derive(thiserror::Error, Debug)]
pub enum CookieError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("cookie parse error: {0}")]
    Cookie(String),
}
```

---

### 临时文件名可能冲突

测试中：

```rust
let path = temp_dir().join("test_cookies.json");
```

并发测试时可能冲突。

建议使用 `tempfile`：

```rust
let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("cookies.json");
```

---

### 安全问题

Cookie 中可能包含 session/token 等敏感信息。当前保存为 pretty JSON 明文。

建议至少：

- 文档明确是明文保存。
- 上层应将文件权限限制为用户可读写。
- 桌面或移动端可考虑系统 keystore/keychain。
- 不要在日志中输出具体 Cookie 内容。

当前：

```rust
debug!("添加 Cookie: {}", cookie_str);
```

这会泄露敏感 Cookie。

建议改为：

```rust
debug!("添加 Cookie for url: {}", url);
```

不要打印 cookie value。

---

## encoding.rs

评分：**B-**

### 优点

- 有 BOM 检测。
- 提供文件读取和 charset 解析辅助函数。
- 有基本测试。

### 问题

#### 1. 编码检测算法过于简化

当前逻辑：

```rust
for window in bytes.windows(3) {
    if window[0] >= 0x81 && window[0] <= 0xFE {
        if window.len() > 1 && window[1] >= 0x40 && window[1] <= 0xFE {
            gb_count += 1;
        }
    }
}
```

问题：

- `windows(3)` 对短输入不友好。
- GBK/GB18030 检测逻辑过于粗糙。
- UTF-8 校验只检查部分 2 字节情况，不检查完整 UTF-8 序列。
- ASCII 文本会被判为 UTF-8，这没问题，但非中文二进制数据可能误判。

建议优先用标准能力：

```rust
if let Ok(text) = std::str::from_utf8(bytes) {
    return (text.to_string(), UTF_8);
}
```

然后再尝试 GB18030。

---

#### 2. 未处理 UTF-16 BOM

目前只处理 UTF-8 BOM：

```rust
if bytes.len() >= 3 && bytes[0..3] == [0xEF, 0xBB, 0xBF]
```

建议使用：

```rust
if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
    let (text, _, _) = encoding.decode(&bytes[bom_len..]);
    return (text.into_owned(), encoding);
}
```

这也能和 `client.rs` 统一。

---

#### 3. charset 解析大小写不敏感处理不足

```rust
.find(|s| s.trim().starts_with("charset="))
```

不能识别：

```text
Content-Type: text/html; Charset=UTF-8
Content-Type: text/html; CHARSET=utf-8
```

建议：

```rust
.find_map(|part| {
    let (k, v) = part.trim().split_once('=')?;
    if k.trim().eq_ignore_ascii_case("charset") {
        Some(v.trim().trim_matches('"').trim_matches('\'').to_ascii_lowercase())
    } else {
        None
    }
})
```

---

#### 4. `read_file_with_encoding` 返回 `Result<_, String>` 不够 Rust 惯用

```rust
pub fn read_file_with_encoding<P: AsRef<Path>>(path: P) -> Result<(String, String), String>
```

建议返回结构化错误：

```rust
pub fn read_file_with_encoding<P: AsRef<Path>>(path: P) -> std::io::Result<(String, String)>
```

或者自定义错误类型。

---

## lib.rs

评分：**A-**

### 优点

- 模块导出清晰。
- 主要类型 re-export 合理。
- 默认常量集中定义，方便上层使用。

### 问题

#### 1. 常量没有被实际复用

`DEFAULT_MAX_RETRIES`、`DEFAULT_BASE_BACKOFF_MS` 等在 `client.rs`、`retry.rs` 中没有统一使用。

建议在 `HttpClientConfig::default()` 和 `RetryConfig::default()` 中复用这些常量，避免默认值漂移。

---

#### 2. `parse_charset_from_content_type` 未导出

如果希望上层或 `client.rs` 复用 `encoding.rs` 的 charset 解析函数，可以导出：

```rust
pub use encoding::{detect_and_decode, parse_charset_from_content_type};
```

---

## proxy.rs

评分：**B-**

### 优点

- 代理类型建模简单清晰。
- 支持 URL 解析和认证信息。
- `ProxyManager` 支持默认代理和书源级代理，符合业务需求。

### 问题

#### 1. 日志会泄露代理用户名和密码

```rust
info!("使用代理: {}", proxy_url);
info!("Set default proxy: {}", config.to_url());
info!("Set proxy for source {}: {}", source_id, config.to_url());
```

`to_url()` 可能返回：

```text
http://user:pass@proxy.example.com:8080
```

这是敏感信息泄露。

建议增加脱敏方法：

```rust
pub fn to_safe_url(&self) -> String {
    match (&self.username, &self.password) {
        (Some(_), Some(_)) => {
            format!("{}://***:***@{}:{}", self.proxy_type, self.host, self.port)
        }
        _ => format!("{}://{}:{}", self.proxy_type, self.host, self.port),
    }
}
```

日志使用：

```rust
info!("Set default proxy: {}", config.to_safe_url());
```

---

#### 2. `from_url` 对缺失 host 默认使用 `127.0.0.1` 不合理

```rust
let host = parsed.host_str().unwrap_or("127.0.0.1").to_string();
```

对于非法 URL，静默默认到 localhost 可能导致意外代理配置。

建议：

```rust
let host = parsed.host_str()?.to_string();
```

---

#### 3. 默认端口不应所有协议都是 1080

```rust
let port = parsed.port().unwrap_or(1080);
```

HTTP 通常是 8080 或 80，HTTPS 是 443，SOCKS5 常见是 1080。更好的做法是端口必填，或按协议区分。

建议：

```rust
let port = parsed.port().or_else(|| match proxy_type {
    ProxyType::Http => Some(80),
    ProxyType::Https => Some(443),
    ProxyType::Socks5 => Some(1080),
})?;
```

或更严格：

```rust
let port = parsed.port()?;
```

---

#### 4. 用户名和密码没有 percent-decoding

`to_url()` 会 encode：

```rust
let user_encoded = urlencoding::encode(user);
let pass_encoded = urlencoding::encode(pass);
```

但 `from_url()` 没有 decode：

```rust
config.username = Some(user.to_string());
config.password = Some(pass.to_string());
```

如果 URL 中是：

```text
http://user%40mail:p%40ss@example.com:8080
```

解析后会保留编码形式。

建议使用 percent decode。

---

#### 5. host 没有处理 IPv6

`to_url()`：

```rust
format!("{}://{}:{}", self.proxy_type, self.host, self.port)
```

如果 host 是 IPv6：

```text
::1
```

会生成非法 URL：

```text
http://::1:8080
```

应该是：

```text
http://[::1]:8080
```

建议使用 `url::Url` 构造，而不是手写 format。

---

#### 6. 需确认 reqwest SOCKS feature

`ProxyType::Socks5` 需要 `reqwest` 启用 `socks` feature，否则运行时或构建配置可能不支持。

建议在 Cargo.toml 中确认：

```toml
reqwest = { version = "...", features = ["socks", "cookies", "rustls-tls"] }
```

---

## retry.rs

评分：**B**

### 优点

- 通用异步 retry executor 设计清楚。
- 支持最大重试次数、基础退避、最大退避。
- 使用 `FnMut() -> Future`，适合闭包中更新状态。

### 问题

#### 1. 退避计算仍有溢出风险

```rust
let backoff_ms = self.config.base_backoff_ms * 2u64.pow(retries as u32);
```

如果 `max_retries` 很大，仍可能溢出。

建议：

```rust
let multiplier = 1u64.checked_shl(retries as u32).unwrap_or(u64::MAX);
let backoff_ms = self
    .config
    .base_backoff_ms
    .saturating_mul(multiplier)
    .min(self.config.max_backoff_ms);
```

---

#### 2. 没有 jitter，容易造成惊群效应

大量请求同时失败时会同时重试。

建议增加 jitter：

```rust
let jitter = rand::random::<u64>() % 100;
let delay = backoff_ms + jitter;
```

或使用成熟库如 `backoff` / `tower::retry`。

---

#### 3. 无法判断错误是否可重试

当前所有错误都会重试：

```rust
Err(e) => {
    if retries >= self.config.max_retries {
        return Err(e);
    }
    ...
}
```

但业务中有些错误不应该重试：

- 400
- 401
- 403
- 404
- 解析错误
- 参数错误

建议允许传入 predicate：

```rust
pub async fn execute_if<F, Fut, T, E, P>(
    &self,
    mut operation: F,
    should_retry: P,
) -> Result<T, E>
where
    P: Fn(&E) -> bool,
```

---

#### 4. 未被 client.rs 使用

当前 `RetryExecutor` 独立存在，但 `HttpClient` 自己实现 retry。

建议统一。

---

# 重点问题复查

开发要求中列出的重点问题如下：

## P1-1/2: cookie 持久化测试与 API 语义

状态：**未完全修复**

原因：

- API 声称 `save_persistent_cookies` 只保存持久化 Cookie。
- 实现直接序列化整个 `CookieStore`。
- 没有显式过滤 session cookie。
- 测试没有覆盖 session cookie 不应持久化的场景。
- `HttpClient` 内部 Cookie store 与 `CookieManager` 未打通，整体 Cookie API 语义仍不清晰。

建议列为当前 crate 的最高优先级修复项。

---

## P1-3: ScriptEngine 资源限制

不适用于本 crate。该问题属于 `core-source/script_engine.rs`。

---

## P1-4: BookSourceParser 搜索规则模型

不适用于本 crate。该问题属于 `core-source/parser.rs`。

---

## P1-5: 相对 URL 规范化

不适用于本 crate 当前文件。该问题主要属于 `core-source/parser.rs`、`utils.rs`。  
不过 `proxy.rs` 中 URL 拼接也存在手写 format 的规范化风险，建议用 `url::Url` 构造。

---

## P1-6: EPUB metadata 解析

不适用于本 crate。该问题属于 `core-parser/epub.rs`。

---

## P1-7: UMD 章节读取

不适用于本 crate。该问题属于 `core-parser/umd.rs`。

---

## P1-8: 数据库迁移策略

不适用于本 crate。该问题属于 `core-storage/database.rs`。

---

# 剩余未修复问题清单

## P1 级

1. **Cookie 持久化语义未由代码保证**
   - `save_persistent_cookies` 直接序列化整个 store。
   - 缺少 session cookie 不保存的测试。
   - 建议显式过滤或重命名 API。

2. **`HttpClient` Cookie 与 `CookieManager` 割裂**
   - reqwest 内部 cookie store 无法通过 `CookieManager` 持久化。
   - 上层 API 语义容易误导。

3. **代理 URL 日志泄露认证信息**
   - `to_url()` 包含用户名密码。
   - 多处 info 日志直接打印。

4. **`clear_domain` 未实现但公开**
   - 调用方会误以为已经清除。
   - 应实现或返回 Unsupported 错误。

---

## P2 级

1. **Semaphore acquire 使用 `expect`，存在 panic 风险。**
2. **`max_concurrent = 0` 会导致请求永久等待。**
3. **指数退避计算存在溢出风险。**
4. **并发限制不覆盖 body 读取。**
5. **`client.rs` 和 `retry.rs` 重试逻辑重复。**
6. **`post` 不应为 async。**
7. **错误类型使用 `Box<dyn Error>` / `String`，不利于上层分类处理。**
8. **Cookie、代理日志可能泄露敏感信息。**

---

## P3 级

1. `encoding.rs` 检测算法较粗糙。
2. charset 解析逻辑重复且大小写处理不一致。
3. 测试使用固定 temp 文件名，可能并发冲突。
4. `ProxyConfig::from_url` 默认 host/port 行为不够严格。
5. `ProxyConfig::to_url` 手写 URL 拼接，不支持 IPv6 等边界情况。
6. `lib.rs` 默认常量未统一复用。

---

# 建议的修复顺序

1. **重构 Cookie 体系**
   - 明确 `CookieManager` 和 `HttpClient` 的关系。
   - 显式实现持久化 Cookie 过滤。
   - 增加 session cookie 不持久化测试。

2. **修复敏感信息日志**
   - Cookie value 不打日志。
   - Proxy URL 日志脱敏。

3. **修复 panic 和资源限制**
   - `Semaphore::acquire` 不使用 `expect`。
   - 校验 `max_concurrent > 0`。
   - backoff 使用 saturating 计算。

4. **统一 retry**
   - `HttpClient` 复用 `RetryExecutor`。
   - 增加错误可重试 predicate。
   - 加入 max backoff 和 jitter。

5. **统一编码处理**
   - `client.rs` 使用 `encoding.rs`。
   - 使用 `Encoding::for_label` 和 `Encoding::for_bom`。

6. **完善 proxy URL 构造**
   - 不手写 format。
   - 支持 IPv6。
   - 正确 percent decode。
   - 缺失 host/port 时返回错误。

---

## 📦 core-parser
**职责**: 解析层 - TXT/EPUB/UMD 解析、内容清洗

以下是对 `core-parser` crate 的代码审核结果。

## 总体结论

**crate 整体评级：P1**

该 crate 当前可以作为“原型级解析器”使用，但距离稳定可用于真实书籍解析还有明显差距。主要问题集中在：

1. **EPUB metadata 解析仍不完整**，之前的 **P1-6 未完全修复**。
2. **UMD 解析实现基本不符合真实 UMD 格式**，之前的 **P1-7 未修复**。
3. **TXT 分章逻辑存在严重 bug**，章节标题和内容对应关系错误，且默认正则缺少多行模式。
4. 存在多处资源限制缺失问题：大文件、zip bomb、异常章节数量、异常 offset 都可能造成 CPU/内存/磁盘 IO 风险。
5. 错误类型统一使用 `String`，不利于上层精确处理。
6. 多处正则在运行时重复编译，性能和风格上均可改进。

---

# 文件级审核

---

## `cleaner.rs`

**评分：B**

### 优点

- 结构清晰，`CleanerConfig` 和 `ContentCleaner` 分离合理。
- 支持规则预编译，至少 `remove_rules` 这一部分做得较好。
- `clean_chapters` 使用所有权转换，符合 Rust 惯用写法。
- 没有明显内存安全问题。

### 问题

#### 1. `unwrap()` 虽然当前常量正则安全，但风格上不佳

```rust
let re = Regex::new(r"<[^>]+>").unwrap();
```

这些正则是硬编码常量，panic 风险较低，但库代码中仍建议避免 `unwrap()`，可以使用 `once_cell::sync::Lazy` 预编译。

建议：

```rust
static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("valid regex"));
```

如果使用 `expect()`，至少给出明确错误信息。

#### 2. HTML 清洗顺序有逻辑问题

当前顺序：

1. 移除 HTML 标签
2. 解码实体
3. 应用移除规则

这会导致 `<script>bad()</script>` 先被 `<[^>]+>` 处理成 `bad()`，后面的 `<script[^>]*>.*?</script>` 规则已经无法匹配，脚本内容反而被保留。

应先移除：

- comment
- script
- style

再移除普通标签。

#### 3. 正则无法跨行匹配

默认规则：

```rust
r"<!--.*?-->"
r"<script[^>]*>.*?</script>"
r"<style[^>]*>.*?</style>"
```

Rust regex 中 `.` 默认不匹配换行，多行脚本、样式和注释无法删除。

建议使用：

```rust
(?is)<!--.*?-->
(?is)<script[^>]*>.*?</script>
(?is)<style[^>]*>.*?</style>
```

#### 4. HTML entity 解码非常不完整

当前只处理少量命名实体，不支持：

- 数字实体：`&#123;`
- 十六进制实体：`&#x4e2d;`
- 更多 HTML5 实体

建议使用成熟库，例如：

```rust
html_escape::decode_html_entities
```

#### 5. 重复实体替换

`CleanerConfig::replace_rules` 默认中已经包含 HTML 实体替换，同时 `decode_html_entities` 又做一遍，存在重复逻辑。

#### 6. `clean_text` 每次都会重新构造 cleaner

```rust
pub fn clean_text(content: &str) -> String {
    let cleaner = ContentCleaner::default();
    cleaner.clean(content)
}
```

如果高频调用，会重复编译默认正则。

建议使用静态全局 cleaner 或让调用方复用 `ContentCleaner`。

### 建议

- 使用 `once_cell::sync::Lazy` 预编译固定正则。
- 调整清洗顺序，先删除脚本/样式/注释，再删除普通标签。
- 使用 HTML entity 解码库。
- 将错误类型从 `String` 改为自定义 error 或 `thiserror`。
- 为大文本清洗增加长度限制或调用方可配置限制。

---

## `epub.rs`

**评分：C**

### 优点

- 解析流程结构清晰：container -> opf -> spine -> chapter。
- 使用 `quick-xml` 而不是手写 XML，方向正确。
- 对缺失章节文件使用 `warn!` 而不是直接 panic，容错性尚可。

### 主要问题

## P1-6：EPUB metadata 解析未完全修复

当前 metadata 解析仍非常简化。

支持的字段只有：

```rust
title
creator
language
identifier
```

但 `BookMetadata` 中还有：

```rust
publisher
description
```

没有解析。

此外，也没有处理：

- `dc:publisher`
- `dc:description`
- `dc:date`
- `dc:subject`
- `meta property="dcterms:modified"`
- `meta name="cover" content="..."`
- 多作者
- 多 identifier
- OPF2 / OPF3 差异
- namespace 解析
- `refines` 属性
- `opf:role`
- `file-as`

因此之前提出的 **P1-6 EPUB metadata 解析问题只能算部分修复，仍未达到可接受水平**。

---

### 2. XML 属性值没有正确 unescape / decode

当前：

```rust
String::from_utf8_lossy(&attr.value).to_string()
```

这没有处理 XML 实体，也没有考虑属性转义。

建议使用 quick-xml 的属性解码 API，例如：

```rust
attr.decode_and_unescape_value(reader.decoder())
```

具体 API 依赖 quick-xml 版本。

---

### 3. `current_tag` 没有在 End 时清理，可能导致误解析

当前逻辑：

```rust
Ok(Event::Text(ref e)) if in_metadata => {
    let text = e.unescape().unwrap_or_default().to_string();
    if current_tag.ends_with(":title") ...
}
```

如果遇到 metadata 中的空白文本、缩进换行，`current_tag` 可能仍是上一个标签，存在覆盖或误赋值风险。

例如：

```xml
<dc:title>Book</dc:title>
    
<dc:creator>Alice</dc:creator>
```

中间的 whitespace text 可能被当作 title。

应该：

- 在 `Event::Start` 记录目标 tag。
- 在对应 `Event::End` 清空。
- 忽略纯空白文本。
- 只处理明确关注的 tag。

---

### 4. `e.unescape().unwrap_or_default()` 会吞掉 XML 错误

```rust
let text = e.unescape().unwrap_or_default().to_string();
```

这里不应该静默吞掉 XML 解码错误，否则 metadata 解析失败时上层无法感知。

建议：

```rust
let text = e.unescape()
    .map_err(|e| format!("解析 metadata 文本失败: {}", e))?
    .to_string();
```

---

### 5. 章节路径构造不适合 zip 内路径

当前：

```rust
let base_path = std::path::Path::new(opf_path).parent().unwrap_or(std::path::Path::new(""));
let file_path = base_path.join(&item.href);
let file_path_str = file_path.to_string_lossy().to_string();
```

这在 Windows 上可能产生反斜杠 `\`，而 zip 内路径一般要求 `/`。

另外没有处理：

- URL percent encoding
- `../`
- `./`
- fragment：`chapter.xhtml#part`
- 空格编码
- manifest href 是 URL 路径，不是本地文件系统路径

建议不要使用 `std::path::Path` 处理 zip 内部路径，而应使用 URL/path normalization 逻辑。

例如：

```rust
fn join_epub_path(base: &str, href: &str) -> String {
    // 使用 / 分割并规范化 . 和 ..
}
```

---

### 6. 没有过滤 manifest 的 media type

当前只要 spine 引用到了 manifest，就尝试读取，不判断：

```rust
item.media_type
```

应该只解析：

- `application/xhtml+xml`
- `text/html`
- `application/xml`

不要误读图片、CSS、字体等资源。

---

### 7. 没有解析 NCX / NAV 目录

当前章节标题来自 HTML `<title>` 或 `<h1>`：

```rust
extract_title_from_html
```

这会导致大量 EPUB 的目录标题不准确。

应优先解析：

- EPUB2: `toc.ncx`
- EPUB3: `nav.xhtml`

spine 只表示阅读顺序，不一定提供章节标题。

---

### 8. HTML 清洗使用正则，不可靠

```rust
let re = regex::Regex::new(r"<[^>]+>").unwrap();
```

HTML 不建议使用正则清洗，容易被：

- 属性中的 `>`
- CDATA
- script/style
- 注释
- ruby 标注
- 嵌套标签

影响。

建议使用 `scraper`、`html5ever` 或统一复用 `ContentCleaner`。

---

### 9. EPUB 资源限制缺失

当前没有限制：

- zip entry 数量
- 单个 entry 解压后大小
- 总解压大小
- XML 大小
- 章节大小
- 章节数量

存在 zip bomb / 超大 EPUB 导致内存或 CPU 消耗问题。

建议增加配置：

```rust
pub struct EpubParserConfig {
    pub max_entry_size: u64,
    pub max_total_uncompressed_size: u64,
    pub max_chapters: usize,
    pub max_xml_size: usize,
}
```

---

### 10. 正则重复编译

`extract_title_from_html` 和 `clean_html_content` 每次调用都会编译正则。

建议静态预编译或使用 cleaner。

### 建议

- 完善 OPF metadata 解析，至少覆盖 `publisher`、`description`、`date`、`subject`、`cover`。
- 正确解析 XML namespace 和属性 unescape。
- 解析 NCX / NAV 获取章节标题。
- 修复 zip 内路径规范化。
- 增加 zip bomb 和大文件限制。
- 使用 HTML parser 替代正则。
- 将 `String` 错误改为结构化错误。

---

## `lib.rs`

**评分：A-**

### 优点

- 模块导出清晰。
- re-export 方便上层调用。
- `Chapter::new` 简单直观。

### 问题

#### 1. `Chapter::new` 放在 `lib.rs` 中不够内聚

当前：

```rust
impl Chapter {
    pub fn new(title: &str, content: &str, index: usize) -> Self {
        ...
    }
}
```

更建议放到 `types.rs` 中，类型和其 impl 保持在一起。

#### 2. 文档中提到 MOBI，但实际没有 MOBI 模块

```rust
负责解析各种电子书格式（TXT/EPUB/UMD/MOBI）
```

当前 crate 没有 MOBI 支持，文档与实现不一致。

### 建议

- 将 `impl Chapter` 移动到 `types.rs`。
- 修正文档，或者补充 MOBI 模块。
- 可以考虑导出便捷函数：

```rust
pub use txt::parse_txt_file;
pub use epub::parse_epub_file;
pub use umd::parse_umd_file;
```

---

## `txt.rs`

**评分：C**

### 优点

- 配置结构清晰。
- 支持自定义章节正则和清洗规则。
- 有基础编码检测逻辑。
- 没有 unsafe 代码。

### 严重问题

## 1. 默认章节正则缺少多行模式，基本无法正常分章

当前：

```rust
let chapter_regex = r"(?i:^第?[一二三四五六七八九十百千万\d]+[章回节卷集].*$|^Chapter\s+\d+.*$)";
```

Rust regex 中 `^` 和 `$` 默认匹配整个文本的开始和结束，而不是每一行。对于多行 TXT，应使用 `(?m)`：

```rust
(?im)^第?[一二三四五六七八九十百千万\d]+[章回节卷集].*$|^Chapter\s+\d+.*$
```

或者：

```rust
(?im)^(第?[一二三四五六七八九十百千万\d]+[章回节卷集].*|Chapter\s+\d+.*)$
```

当前实现很可能只匹配文件第一行或最后一行，导致分章失败。

---

## 2. 分章逻辑错误，章节标题和内容错位

当前算法：

```rust
for mat in re.find_iter(content) {
    if mat.start() > last_end {
        let chapter_content = &content[last_end..mat.start()].trim();
        let title = if chapter_index == 0 {
            "正文".to_string()
        } else {
            chapters[chapter_index - 1].title.clone()
        };
        chapters.push(...)
    }

    last_end = mat.end();
}
```

问题是：

- `mat` 是章节标题，但没有保存为当前章节标题。
- 当前章节内容是标题之后到下一个标题之前，但被赋予了上一章标题。
- 第一章通常会被命名成 `"正文"`。
- 最后一章被命名成 `"最后一章"`，不是实际标题。

正确思路：

1. 找到所有标题匹配位置。
2. 每个标题对应从 `title.end()` 到下一个 `title.start()` 的内容。
3. 标题文本就是 `mat.as_str()`。

示例改法：

```rust
let matches: Vec<_> = re.find_iter(content).collect();

if matches.is_empty() {
    return Ok(vec![Chapter {
        title: "正文".to_string(),
        content: self.clean_content(content),
        index: 0,
        href: None,
    }]);
}

let mut chapters = Vec::new();

let preface = content[..matches[0].start()].trim();
if !preface.is_empty() {
    chapters.push(Chapter {
        title: "正文".to_string(),
        content: self.clean_content(preface),
        index: chapters.len(),
        href: None,
    });
}

for (i, mat) in matches.iter().enumerate() {
    let title = mat.as_str().trim().to_string();
    let body_start = mat.end();
    let body_end = matches.get(i + 1).map_or(content.len(), |next| next.start());
    let body = content[body_start..body_end].trim();

    chapters.push(Chapter {
        title,
        content: self.clean_content(body),
        index: chapters.len(),
        href: None,
    });
}
```

---

## 3. `clean_rules` 删除 HTML entities，而不是解码

默认配置：

```rust
r"&nbsp;|&lt;|&gt;|&quot;|&amp;"
```

然后：

```rust
text = re.replace_all(&text, "").to_string();
```

这会把 `&lt;` 删除，而不是变成 `<`。

建议复用 `ContentCleaner` 或使用 HTML entity 解码库。

---

## 4. 编码检测逻辑过于简化

当前 UTF-8 检测只做了窗口级别的局部判断，不能正确验证完整 UTF-8 序列，也不能检测：

- UTF-16LE / UTF-16BE BOM
- GB18030 四字节序列
- Big5
- Shift-JIS
- 含 replacement 的失败解码

建议：

- 使用 `encoding_rs` 的 BOM 检测。
- 或引入 `chardetng`。
- 至少优先尝试严格 UTF-8：

```rust
if let Ok(s) = std::str::from_utf8(bytes) {
    return (s.to_string(), UTF_8);
}
```

---

## 5. 大文件一次性读取

```rust
let bytes = fs::read(path)
```

对于超大 TXT 会直接占用大量内存。TXT 解析通常确实需要全文分章，但仍建议增加最大文件大小限制。

---

## 6. 正则在每次解析时重新编译

`parse_content` 每次编译章节正则，`clean_content` 每次编译清洗规则和空行正则。

建议：

- 在 `TxtParser::new` 中预编译。
- 配置中保留原始字符串，parser 中保存 `Option<Regex>` 和 `Vec<Regex>`。

---

### 建议

- 修复默认章节正则，加入 `(?m)`。
- 重写分章逻辑，确保标题和内容对应。
- 复用 `ContentCleaner` 处理 HTML/entity。
- 增加最大文件大小限制。
- 使用更可靠的编码检测。
- 在构造 parser 时预编译正则，并返回 `Result<TxtParser, Error>`。

---

## `types.rs`

**评分：A-**

### 优点

- 类型简单清晰。
- 派生了 `Serialize` / `Deserialize`，方便跨层传递。
- `BookMetadata` 使用 `Option`，表达缺省字段合理。

### 问题

#### 1. `BookMetadata` 字段较少

EPUB metadata 可能还需要：

```rust
pub subjects: Vec<String>,
pub description: Option<String>,
pub publisher: Option<String>,
pub published_date: Option<String>,
pub modified_date: Option<String>,
pub cover_href: Option<String>,
pub identifiers: Vec<String>,
pub contributors: Vec<String>,
```

当前只有一个 `identifier`，无法表达多个 identifier。

#### 2. `Chapter` 缺少源位置信息

对于 EPUB/UMD/TXT，调试和增量加载时可能需要：

```rust
pub source_path: Option<String>,
pub byte_range: Option<(u64, u64)>,
```

当然这取决于上层设计。

#### 3. `EpubData` 未被 parser 便捷返回使用

`parse_epub_file` 返回：

```rust
Result<(BookMetadata, Vec<Chapter>), String>
```

而不是：

```rust
Result<EpubData, Error>
```

既然定义了 `EpubData`，可以考虑统一返回它。

### 建议

- 将 `Chapter::new` 移动到本文件。
- 扩展 `BookMetadata`。
- 统一 EPUB parser 返回 `EpubData`。
- 考虑为类型增加 `PartialEq`, `Eq`，便于测试：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
```

---

## `umd.rs`

**评分：D**

### 优点

- 结构上拆分了 header、index、content 读取。
- 使用 `read_exact` 和 `seek`，基本 IO 操作是合理的。
- 对 UTF-8 失败后尝试 GBK 解码，有一定兼容性考虑。

### 严重问题

## P1-7：UMD 章节读取问题未修复

当前实现仍然不能认为是真正的 UMD 解析器。

### 1. UMD 文件格式假设过于简化，基本不符合真实 UMD

当前假设 UMD 文件头是：

```text
magic: 4 bytes = UMD\x1A
version: 2 bytes
encrypt_info: 1 byte
chapter_count: 4 bytes
content_length: 4 bytes
chapter_offsets: chapter_count * 8 bytes
```

真实 UMD 文件通常是 chunk/tag 结构，并不是这样线性排列的简单格式。常见 UMD 需要解析：

- 文件标识
- 书籍类型
- metadata chunk
- chapter title chunk
- chapter offset chunk
- content chunk
- 压缩块
- possibly zlib
- 编码信息
- 加密/混淆

因此当前代码大概率无法解析真实 UMD 文件。

---

### 2. `chapter_count` 未限制，存在资源消耗风险

```rust
for _ in 0..chapter_count {
```

如果文件伪造一个极大的 `chapter_count`，会导致长时间读取或大量 offsets 分配。

建议限制：

```rust
const MAX_CHAPTERS: u32 = 100_000;
if chapter_count > MAX_CHAPTERS {
    return Err("章节数量过大".to_string());
}
```

---

### 3. offset 没有校验

当前没有检查：

- offset 是否小于文件长度
- offset 是否递增
- end_offset 是否大于 offset
- offset 是否落在内容区
- 是否存在重叠章节
- 是否越界

---

### 4. `end - offset` 可能下溢或造成巨大分配

```rust
let size = (end - offset) as usize;
let mut buf = vec![0u8; size];
```

如果 `end < offset`：

- debug 模式下可能 panic。
- release 模式下可能整数下溢，得到巨大值，导致 OOM 风险。

应改为：

```rust
let size = end
    .checked_sub(offset)
    .ok_or_else(|| "章节 offset 非递增".to_string())?;
```

并限制最大章节大小。

---

### 5. 最后一章直接 `read_to_end`

```rust
file.read_to_end(&mut buf)
```

如果最后一个 offset 错误，可能读取整个剩余文件，包括非正文数据，甚至造成大内存消耗。

应基于内容区长度计算最后一章结束位置，而不是 EOF。

---

### 6. `content_buf.clone()` 不必要

```rust
let content = if let Ok(text) = String::from_utf8(content_buf.clone()) {
```

这里 clone 会复制整章内容。

建议：

```rust
let content = match String::from_utf8(content_buf) {
    Ok(text) => text,
    Err(err) => {
        let bytes = err.into_bytes();
        let (text, _, _) = encoding_rs::GBK.decode(&bytes);
        text.into_owned()
    }
};
```

---

### 7. `content_buf` 不需要 `mut`

```rust
let mut content_buf = if let Some(end) = end_offset {
```

变量没有被修改，会有 warning。

---

### 8. 忽略加密和压缩

```rust
// 实际 UMD 可能需要解密处理
```

目前 `encrypt_info` 读取后未使用。若文件是加密/压缩 UMD，解析结果会是乱码或失败。

---

### 9. 清洗内容会破坏段落

```rust
let re = regex::Regex::new(r"\s+").unwrap();
let cleaned = re.replace_all(content, " ");
```

这会把所有换行压成空格，小说段落结构丢失。

建议至少保留段落换行。

---

### 建议

- 重新实现 UMD chunk/tag 解析。
- 严格校验 offset、文件长度、章节数量。
- 加入最大章节大小和最大总内容大小限制。
- 支持 UMD 常见压缩/编码/加密逻辑。
- 解析真实章节标题，而不是 `第 N 章`。
- 避免不必要 clone。
- 保留段落结构。

---

# 重点问题复查

以下为开发者要求重点检查项的复查结果。

## P1-1/2: cookie 持久化测试与 API 语义

**涉及 crate：core-net，不在本次 core-parser 文件范围内。**

本次无法确认是否修复。

---

## P1-3: ScriptEngine 资源限制

**涉及 crate：core-source/script_engine.rs，不在本次 core-parser 文件范围内。**

本次无法确认是否修复。

---

## P1-4: BookSourceParser 搜索规则模型

**涉及 crate：core-source/parser.rs，不在本次 core-parser 文件范围内。**

本次无法确认是否修复。

---

## P1-5: 相对 URL 规范化

**主要涉及 core-source/parser.rs, utils.rs。**

但本 crate 的 EPUB zip 内路径拼接也存在类似问题：

```rust
let file_path = base_path.join(&item.href);
```

该实现不适合 EPUB 内部 URL 路径，尤其在 Windows 平台会产生 `\`。因此从 EPUB 角度看，路径规范化仍有缺陷。

状态：**本 crate 内相关问题未完全修复。**

---

## P1-6: EPUB metadata 解析

**状态：未完全修复。**

当前只解析：

- title
- creator
- language
- identifier

未解析或不完整支持：

- publisher
- description
- subject
- date
- multiple creators
- multiple identifiers
- OPF3 meta property
- cover
- namespace-aware metadata
- NCX/NAV 目录标题

应继续作为 P1 处理。

---

## P1-7: UMD 章节读取

**状态：未修复。**

当前实现是简化的假设格式，不符合真实 UMD chunk/tag 结构。章节 offset、压缩、编码、加密、标题读取均未正确实现。

应继续作为 P1 处理，甚至如果 UMD 是核心功能，可升级为 P0/P1 边界问题。

---

## P1-8: 数据库迁移策略

**涉及 core-storage/database.rs，不在本次 core-parser 文件范围内。**

本次无法确认是否修复。

---

# 剩余未修复问题清单

## P1 级别

1. **EPUB metadata 解析不完整。**
2. **UMD 解析基本不可用于真实 UMD 文件。**
3. **TXT 默认分章正则缺少多行模式，分章逻辑严重错误。**
4. **EPUB/UMD/TXT 均缺少资源限制，存在大文件或恶意文件导致内存/CPU 消耗风险。**
5. **EPUB zip 内路径处理不正确，Windows 下尤其容易失败。**
6. **UMD offset 未校验，存在下溢、巨大分配、越界读取风险。**

## P2 级别

1. 大量错误使用 `String`，缺少结构化 error。
2. 多处正则运行时重复编译。
3. HTML 清洗使用正则，可靠性不足。
4. HTML entity 解码不完整。
5. EPUB 没有解析 NCX / NAV。
6. TXT 编码检测过于简化。
7. 清洗器处理 script/style/comment 的顺序不正确。

## P3 级别

1. `Chapter::new` 建议移动到 `types.rs`。
2. 文档提到 MOBI，但未实现。
3. 部分字段加了 `#[allow(dead_code)]`，说明类型设计或实现尚未完整。
4. `EpubData` 定义后未被 parser 返回使用。
5. `umd.rs` 存在不必要 clone 和无用 `mut`。

---

# 建议优先级

建议按以下顺序修复：

1. **先修 TXT 分章逻辑**：这是最容易修复且影响最大的 bug。
2. **给 EPUB/UMD/TXT 增加资源限制**：防止恶意文件导致 OOM 或长时间阻塞。
3. **完善 EPUB metadata 和路径规范化**。
4. **重写 UMD 解析器，基于真实 UMD chunk/tag 格式实现。**
5. **统一错误类型，引入 `thiserror`。**
6. **提取通用清洗逻辑，避免 EPUB/TXT/cleaner 各自写一套不一致的 HTML/entity 处理。**

总体来看，`core-parser` 当前代码结构还算清晰，但实现完整性不足，尤其 EPUB 和 UMD 仍处于简化版阶段。若要用于生产环境，建议继续以 **P1** 级别跟进。

---

## 📦 core-storage
**职责**: 存储层 - 数据库、书签/进度/源数据 DAO

## 总体结论

**crate: `core-storage`**  
**整体评级：P1（存在会导致核心功能不可用或数据风险的高优先级问题）**

该 crate 的整体结构比较清晰，DAO / models / database / manager 分层明确，使用 `rusqlite`、`serde`、`chrono` 等也比较直接。但当前版本存在几个严重问题：

1. **`book_dao.rs` 和 `source_dao.rs` 的 INSERT SQL 占位符数量错误**，会导致创建/更新书籍、书源失败。
2. **默认数据库路径 `legado.db` 可能因父目录为空而初始化失败**。
3. **数据库迁移策略仍然不够安全，P1-8 未充分修复**。
4. **`INSERT OR REPLACE` 用于书源批量导入存在数据破坏风险**，可能触发删除旧行、破坏外键关系。
5. **错误处理信息丢失较多，部分错误被吞掉或转成过于笼统的错误**。
6. **约束、索引、唯一性设计不足，存在重复章节、重复书签、模糊 URL 查询等潜在逻辑问题**。

---

# 文件级审查

---

## `book_dao.rs`

**评分：D**

### 主要问题

#### 1. 严重 Bug：`INSERT` 占位符数量错误

`books` 表插入字段数量是 **20 个**：

```sql
id, source_id, source_name, name, author, cover_url, chapter_count,
latest_chapter_title, intro, kind, last_check_time, last_check_count,
total_word_count, can_update, order_time, latest_chapter_time,
custom_cover_path, custom_info_json, created_at, updated_at
```

但 SQL 中：

```sql
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

只有 **19 个占位符**。

而 `params![]` 中传入了 20 个参数。

这会导致 `BookDao::upsert()` 在运行时报错，例如：

```text
SqliteFailure: 19 values for 20 columns
```

影响：

- `BookDao::create()` 一定失败。
- 上层所有书籍创建、更新逻辑不可用。

应修改为 20 个占位符：

```sql
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

---

#### 2. `ON CONFLICT(id)` 更新时没有更新 `source_id`

当前更新字段中缺少：

```sql
source_id = excluded.source_id
```

如果书籍需要从一个书源迁移到另一个书源，当前逻辑不会生效。

是否允许修改 `source_id` 取决于业务语义。如果不允许，建议明确注释；如果允许，应补上。

---

#### 3. `search()` 的 LIKE 未转义

```rust
let pattern = format!("%{}%", keyword);
```

如果用户输入 `%` 或 `_`，会被 SQLite 当作通配符。

这不算 SQL 注入，因为用了参数绑定，但会造成搜索语义偏差。

建议提供 LIKE 转义：

```rust
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
```

然后 SQL 使用：

```sql
WHERE name LIKE ? ESCAPE '\' OR author LIKE ? ESCAPE '\'
```

---

#### 4. 删除书籍依赖外键级联，但需确保每个连接都开启 foreign_keys

`database::init_database()` 中开启了：

```rust
PRAGMA foreign_keys = ON
```

但 `get_connection()` 没有开启。若上层通过 `get_connection()` 获得连接，再调用 DAO，则级联删除不会生效。

建议统一封装连接创建，所有连接都设置：

```sql
PRAGMA foreign_keys = ON
```

---

### 改进建议

- 修复 `INSERT` 占位符数量。
- 明确 `source_id` 是否允许更新。
- 提取公共 `SELECT` 列表，减少重复 SQL。
- 增加 `LIKE` 转义。
- 给 `BookDao` 增加单元测试，至少覆盖：
  - `create()`
  - `upsert()`
  - `get_by_id()`
  - `delete()` 后章节级联删除

---

## `chapter_dao.rs`

**评分：B-**

### 优点

- 基本 CRUD 清晰。
- `book_id` 外键有 `ON DELETE CASCADE`。
- `get_by_book()` 按 `index_num` 排序符合章节列表语义。
- 没有明显 `unwrap()` 滥用。

### 问题

#### 1. `get_by_url()` 未限定 `book_id`

```rust
pub fn get_by_url(&self, url: &str) -> SqlResult<Option<Chapter>>
```

SQL：

```sql
FROM chapters WHERE url = ?
```

但同一个章节 URL 有可能在不同书籍、不同书源中重复。当前方法会返回任意一个匹配结果。

建议改为：

```rust
pub fn get_by_book_and_url(&self, book_id: &str, url: &str) -> SqlResult<Option<Chapter>>
```

SQL：

```sql
WHERE book_id = ? AND url = ?
```

---

#### 2. 缺少唯一约束

当前 `chapters` 表没有约束：

```sql
UNIQUE(book_id, index_num)
UNIQUE(book_id, url)
```

这会导致同一本书重复插入相同章节。

建议根据业务语义添加：

```sql
UNIQUE(book_id, index_num)
```

或：

```sql
UNIQUE(book_id, url)
```

也可以两者都加。

---

#### 3. `upsert()` 只按 `id` 冲突更新

如果同一本书同一个 `index_num` 的章节被重新抓取，但生成了新的 UUID，就会产生重复章节。

更合理的策略可能是：

```sql
ON CONFLICT(book_id, index_num) DO UPDATE SET ...
```

或者：

```sql
ON CONFLICT(book_id, url) DO UPDATE SET ...
```

---

#### 4. `start` / `end` 使用 `i32` 可能不足

如果章节内容偏大，字节偏移、字符偏移使用 `i32` 有溢出风险。建议使用 `i64` 或 `usize`，数据库中仍可用 `INTEGER`。

---

### 改进建议

- 增加章节唯一约束。
- 修改 `get_by_url()` 为按 `book_id + url` 查询。
- 明确章节 upsert 的业务冲突键。
- 考虑将 `start` / `end` 改为 `i64`。

---

## `database.rs`

**评分：C-**

### 优点

- 有集中初始化逻辑。
- 有版本号概念。
- 创建表和创建索引分离。
- 迁移尝试使用事务包裹，这是正确方向。

### 主要问题

#### 1. P1-8：数据库迁移策略仍未完全修复

之前重点问题：

> P1-8: 数据库迁移策略（core-storage/database.rs）

当前虽然有：

```rust
const DB_VERSION: i32 = 1;
```

以及：

```rust
migrate_database(&conn, version, DB_VERSION)?;
```

但迁移策略仍然不够安全。

主要问题：

##### 1.1 查询版本依赖 `app_settings`，但表可能不存在

```rust
let version: i32 = conn.query_row(
    "SELECT COALESCE((SELECT value FROM app_settings WHERE key = 'db_version'), '0')",
    [],
    |row| row.get(0)
).unwrap_or(0);
```

如果 `app_settings` 不存在，这里会报错，然后被：

```rust
.unwrap_or(0)
```

吞掉。

这会掩盖真实数据库错误，例如：

- app_settings 损坏
- schema 异常
- 权限问题
- 数据库不是合法 SQLite 文件

建议只对 `no such table` 等预期错误降级，其他错误应该返回。

---

##### 1.2 `unwrap_or(0)` 会吞掉数据库错误

这段属于错误处理反模式：

```rust
.unwrap_or(0)
```

如果 `query_row` 因 IO、锁、损坏数据库失败，当前逻辑会误认为是新数据库，然后尝试 `create_tables()`，可能造成更严重后果。

建议：

```rust
let version = match query_db_version(&conn) {
    Ok(v) => v,
    Err(DbVersionError::NoSettingsTable) => 0,
    Err(e) => return Err(e.into()),
};
```

---

##### 1.3 未处理数据库版本高于当前程序版本

当前逻辑：

```rust
if version == 0 {
    create_tables(&conn)?;
    set_db_version(&conn, DB_VERSION)?;
} else if version < DB_VERSION {
    migrate_database(&conn, version, DB_VERSION)?;
}
```

如果数据库版本是 2，但程序只支持 1，则会直接继续运行。

这是危险的。

应显式拒绝：

```rust
if version > DB_VERSION {
    return Err(...);
}
```

---

##### 1.4 未知迁移版本只 warn，然后仍然设置到目标版本

```rust
match v {
    1 => create_tables(conn)?,
    _ => warn!("未知的数据库版本: {}", v),
}
```

如果未来 `DB_VERSION = 3`，但忘记写 `2` 或 `3` 的迁移，这段会打印 warn，然后：

```rust
set_db_version(conn, to_version)?;
```

这会把数据库标记为已迁移，实际 schema 却没有变。

这属于严重迁移风险。

未知版本应直接返回错误，而不是 warn。

---

##### 1.5 初始建表不在事务中

`version == 0` 时：

```rust
create_tables(&conn)?;
set_db_version(&conn, DB_VERSION)?;
```

如果创建到一半失败，会留下半初始化数据库。

建议新建数据库初始化也使用事务：

```rust
let tx = conn.transaction()?;
create_tables(&tx)?;
set_db_version(&tx, DB_VERSION)?;
tx.commit()?;
```

不过这需要 `init_database` 中的 `conn` 可变。

---

#### 2. 默认路径可能初始化失败

`lib.rs` 默认：

```rust
path: "legado.db".to_string()
```

`database.rs`：

```rust
if let Some(parent) = std::path::Path::new(db_path).parent() {
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }
}
```

对于 `"legado.db"`，`parent()` 可能是空路径 `""`。此时可能调用：

```rust
create_dir_all("")
```

导致失败。

建议过滤空父路径：

```rust
if let Some(parent) = Path::new(db_path).parent() {
    if !parent.as_os_str().is_empty() && !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }
}
```

这是一个比较实际的 Bug，因为默认配置就会触发。

---

#### 3. `get_connection()` 没有启用外键

```rust
pub fn get_connection(db_path: &str) -> SqlResult<Connection> {
    Connection::open(db_path)
}
```

这与 `init_database()` 行为不一致。

建议：

```rust
pub fn get_connection(db_path: &str) -> SqlResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    Ok(conn)
}
```

---

#### 4. 缺少 busy timeout

SQLite 在多线程或多进程访问时容易遇到：

```text
database is locked
```

建议初始化时设置：

```rust
conn.busy_timeout(std::time::Duration::from_secs(5))?;
```

---

#### 5. WAL 模式不在 `database.rs` 内统一处理

`StorageManager::new()` 中设置 WAL：

```rust
conn.pragma_update(None, "journal_mode", &"WAL")?;
```

但如果用户直接调用 `database::init_database()`，则不会启用 WAL。

这不一定是错误，但行为不一致。建议将数据库连接参数统一下沉到 database 层。

---

### 改进建议

- 修复版本读取逻辑，避免 `unwrap_or(0)`。
- 对 `version > DB_VERSION` 返回错误。
- 未知迁移版本必须失败，不能只 warn。
- 初始建表使用事务。
- 过滤空父目录。
- 所有连接统一开启 foreign_keys。
- 设置 busy timeout。
- 考虑使用 SQLite 内置 `PRAGMA user_version` 替代 `app_settings` 存版本。

---

## `lib.rs`

**评分：B-**

### 优点

- 对外导出清晰。
- `StorageManager` 统一管理 DAO，使用方便。
- `DatabaseConfig` 提供 WAL 配置。

### 问题

#### 1. `StorageManager::new()` 返回 `Box<dyn Error>` 信息较弱

```rust
pub fn new(config: DatabaseConfig) -> Result<Self, Box<dyn std::error::Error>>
```

库代码中建议定义自己的错误类型，例如：

```rust
pub enum StorageError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Serde(serde_json::Error),
}
```

并实现 `From`。

这样上层可以按错误类型处理，而不是只能显示字符串。

---

#### 2. `source_dao()` 需要 `&mut self`，其他 DAO 只需要 `&self`

```rust
pub fn source_dao(&mut self) -> SourceDao<'_>
```

原因是 `SourceDao` 内部持有：

```rust
conn: &'a mut Connection
```

这是因为 `batch_insert()` 需要事务，而 `rusqlite::Connection::transaction()` 需要 `&mut self`。

但这会导致 API 不一致：

```rust
manager.book_dao();     // &self
manager.chapter_dao();  // &self
manager.progress_dao(); // &self
manager.source_dao();   // &mut self
```

使用者会遇到借用限制。

可选改法：

- 将 `SourceDao` 拆成只读 DAO 和写入 DAO。
- `StorageManager` 提供专门的批量导入方法。
- 使用 `Connection::unchecked_transaction()`，但要谨慎。
- 统一所有 DAO 都通过 `&mut Connection`，但会降低并发灵活性。
- 使用连接池或 `Mutex<Connection>`，视项目并发模型而定。

---

#### 3. `init_database()` 函数名与 `database::init_database()` 重名

```rust
pub fn init_database(path: &str) -> Result<rusqlite::Connection, Box<dyn std::error::Error>>
```

虽然可用，但容易混淆。

可以命名为：

```rust
pub fn open_database(...)
```

或只 re-export database 层函数。

---

### 改进建议

- 定义 `StorageError`。
- 统一 DAO 借用风格。
- 考虑给 `StorageManager` 暴露事务方法。
- 避免重复命名造成 API 混淆。

---

## `models.rs`

**评分：B**

### 优点

- 数据结构清晰。
- `Serialize` / `Deserialize` 合理。
- 提供 `new_id()` 和 `now_timestamp()` 便捷函数。
- 没有复杂生命周期或所有权问题。

### 问题

#### 1. 缺少领域约束

目前字段都是裸类型：

```rust
pub source_type: i32
pub scope: i32
pub chapter_count: i32
```

这容易产生非法值，例如：

```rust
source_type = 999
scope = -1
chapter_count = -10
```

建议使用 enum：

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SourceType {
    Text = 0,
    Audio = 1,
    Image = 2,
    Rss = 3,
}
```

或者至少在 DAO 层增加校验。

---

#### 2. 计数字段使用 `i32` 可能不够稳妥

例如：

```rust
total_word_count: i32
start: i32
end: i32
```

字数、偏移量建议使用 `i64`。

SQLite 的 `INTEGER` 本身是 64 位有符号整数。

---

#### 3. 时间戳单位需要统一说明

`now_timestamp()` 返回秒：

```rust
Utc::now().timestamp()
```

但 `BookProgress.read_time` 注释为毫秒。

建议在字段名或类型层面区分：

```rust
created_at_sec
read_time_ms
```

或统一文档说明。

---

### 改进建议

- 用 enum 替代 magic integer。
- 偏移量、字数改为 `i64`。
- 给模型增加构造函数或 builder，统一默认值。
- 可考虑使用 `time` crate 或 `chrono::DateTime<Utc>`，视持久化需求决定。

---

## `progress_dao.rs`

**评分：B-**

### 优点

- 进度和书签逻辑放在同一个 DAO 中，业务上可以接受。
- `book_progress` 使用 `book_id` 主键，适合 upsert。
- 查询和转换逻辑清晰。

### 问题

#### 1. `update_progress()` 注释与实现不一致

注释：

```rust
// 获取现有进度以累加阅读时长
```

但代码：

```rust
let read_time = existing.map(|p| p.read_time).unwrap_or(0);
```

只是保留旧的 `read_time`，没有累加。

如果只是更新阅读位置，注释应修改。如果确实要累加，应传入增量或根据时间差计算。

---

#### 2. `add_read_time()` 对不存在的进度无效

```rust
UPDATE book_progress SET read_time = read_time + ? WHERE book_id = ?
```

如果该 `book_id` 没有进度记录，更新行数为 0，但函数仍返回 `Ok(())`。

这可能让调用者误以为成功。

建议：

```rust
let affected = self.conn.execute(...)?;
if affected == 0 {
    // 根据业务决定插入一条默认进度，或返回 NotFound
}
```

---

#### 3. 没有限制 `additional_ms` 非负

```rust
pub fn add_read_time(&self, book_id: &str, additional_ms: i64)
```

传入负数会减少阅读时长。

如果业务不允许，应校验：

```rust
if additional_ms < 0 {
    return Err(...);
}
```

---

#### 4. 书签缺少去重约束

当前可以无限添加相同位置的书签。

建议根据业务添加唯一约束：

```sql
UNIQUE(book_id, chapter_index, paragraph_index)
```

或者允许重复，但需要明确。

---

### 改进建议

- 修正 `update_progress()` 注释或实现。
- `add_read_time()` 检查影响行数。
- 校验 `additional_ms >= 0`。
- 添加书签唯一性约束或显式允许重复。
- 增加进度和书签 DAO 单元测试。

---

## `source_dao.rs`

**评分：D**

### 主要问题

#### 1. 严重 Bug：`upsert()` 占位符数量错误

`book_sources` 插入字段数量是 **17 个**：

```sql
id, name, url, source_type, group_name, enabled, custom_order, weight,
rule_search, rule_book_info, rule_toc, rule_content,
login_url, header, js_lib,
created_at, updated_at
```

但 SQL 中只有 **16 个占位符**：

```sql
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

`params![]` 中传入了 17 个参数。

这会导致：

- `SourceDao::upsert()` 失败。
- `SourceDao::create()` 失败。
- 书源无法正常创建或更新。

应改为：

```sql
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

---

#### 2. 严重 Bug：`batch_insert()` 同样占位符数量错误

`batch_insert()` 中：

```sql
INSERT OR REPLACE INTO book_sources (
    id, name, url, source_type, group_name, enabled, custom_order, weight,
    rule_search, rule_book_info, rule_toc, rule_content,
    login_url, header, js_lib,
    created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

字段 17 个，占位符 16 个。

批量导入一定失败。

---

#### 3. `INSERT OR REPLACE` 有数据破坏风险

`batch_insert()` 使用：

```sql
INSERT OR REPLACE INTO book_sources ...
```

SQLite 的 `REPLACE` 语义不是普通 update，而是：

1. 删除旧行。
2. 插入新行。

这可能导致：

- 触发外键约束问题。
- 如果未来 `books.source_id` 设置 `ON DELETE CASCADE`，会导致书籍被删除。
- `created_at` 等字段被重置。
- 与唯一键 `url` 冲突时可能删除另一个 `id` 的书源。

更安全的写法是：

```sql
INSERT INTO book_sources (...) VALUES (...)
ON CONFLICT(id) DO UPDATE SET ...
```

如果业务上 URL 唯一，也可以考虑：

```sql
ON CONFLICT(url) DO UPDATE SET ...
```

但必须明确主身份到底是 `id` 还是 `url`。

---

#### 4. `import_from_json()` 丢失 serde 错误细节

```rust
let sources: Vec<BookSource> = serde_json::from_str(json)
    .map_err(|_e| rusqlite::Error::InvalidQuery)?;
```

这会把 JSON 解析错误全部变成 `InvalidQuery`，调用者无法知道具体原因。

建议定义独立错误类型，不要强行塞进 `rusqlite::Error`。

例如：

```rust
pub enum SourceImportError {
    Json(serde_json::Error),
    Sql(rusqlite::Error),
}
```

---

#### 5. 注释乱码

```rust
/// еИЫеїЇжЦ∞зЪД SourceDao
```

应修复编码问题：

```rust
/// 创建新的 SourceDao
```

这是明显的代码风格和可维护性问题。

---

#### 6. 书源删除可能因书籍外键失败

`books.source_id` 外键：

```sql
FOREIGN KEY (source_id) REFERENCES book_sources(id)
```

没有 `ON DELETE CASCADE` 或 `ON DELETE SET NULL`。

因此如果某书源下有书籍，执行：

```rust
DELETE FROM book_sources WHERE id = ?
```

会失败。

这可能是合理的，但 DAO 方法没有说明。

建议业务上明确：

- 禁止删除有书籍的书源，返回错误。
- 删除书源时级联删除书籍。
- 删除书源时将书籍 source_id 置空，但当前 `source_id TEXT NOT NULL` 不支持。

---

### 改进建议

- 修复所有 17 字段 INSERT 的占位符数量。
- 避免 `INSERT OR REPLACE`，改用 `ON CONFLICT DO UPDATE`。
- 明确书源身份键：`id` 还是 `url`。
- 修复乱码注释。
- 定义导入错误类型，保留 JSON 解析错误。
- 补充批量导入单元测试。

---

# 重点历史问题复查

根据要求重点检查之前发现的问题：

## P1-1/2: cookie 持久化测试与 API 语义

涉及文件：

- `core-net/cookie.rs`

本次 crate 是 `core-storage`，未提供相关文件，无法确认。

**状态：不适用 / 未检查。**

---

## P1-3: ScriptEngine 资源限制

涉及文件：

- `core-source/script_engine.rs`

本次 crate 是 `core-storage`，未提供相关文件，无法确认。

**状态：不适用 / 未检查。**

---

## P1-4: BookSourceParser 搜索规则模型

涉及文件：

- `core-source/parser.rs`

本次 crate 是 `core-storage`，未提供相关文件，无法确认。

不过 `core-storage` 中 `BookSource` 的规则字段仍然只是：

```rust
pub rule_search: Option<String>,
pub rule_book_info: Option<String>,
pub rule_toc: Option<String>,
pub rule_content: Option<String>,
```

如果上层已经定义了结构化规则模型，这里仍以裸 JSON 字符串存储是可以接受的；但如果要求存储层也做 schema 校验，则当前没有体现。

**状态：不适用 / 需结合 core-source 判断。**

---

## P1-5: 相对 URL 规范化

涉及文件：

- `core-source/parser.rs`
- `utils.rs`

本次 crate 是 `core-storage`，未提供相关文件，无法确认。

`core-storage` 只是保存 URL，不负责规范化可以接受。

**状态：不适用。**

---

## P1-6: EPUB metadata 解析

涉及文件：

- `core-parser/epub.rs`

本次 crate 不涉及。

**状态：不适用。**

---

## P1-7: UMD 章节读取

涉及文件：

- `core-parser/umd.rs`

本次 crate 不涉及。

**状态：不适用。**

---

## P1-8: 数据库迁移策略

涉及文件：

- `core-storage/database.rs`

**状态：部分修复，但仍未完全解决。**

已有改善：

- 有 `DB_VERSION`。
- 有 `app_settings` 存储版本。
- 有 `migrate_database()`。
- 迁移时尝试事务包裹。

仍存在问题：

1. 版本读取使用 `unwrap_or(0)` 吞掉真实错误。
2. 未处理数据库版本高于程序支持版本。
3. 未知迁移版本只 warn，但仍设置版本号。
4. 初始建表不在事务中。
5. 迁移逻辑没有实际版本演进示例。
6. 没有 schema 校验。
7. 没有使用 SQLite `PRAGMA user_version`，虽然不是必须，但更惯用。
8. `get_connection()` 绕过初始化逻辑，可能拿到未开启外键的连接。

因此 P1-8 不能视为已完全修复。

---

# 其他跨文件问题

## 1. DAO 缺少系统性测试

目前只有 `database.rs` 中一个简单测试：

```rust
test_database_init()
```

但没有覆盖：

- 书源创建
- 书籍创建
- 章节创建
- 阅读进度 upsert
- 书签添加
- 外键级联
- 批量导入
- 迁移
- 默认路径
- 唯一冲突

由于当前存在 SQL 占位符数量错误，如果有基础 DAO 测试应能立刻发现。

建议至少添加以下测试：

```rust
#[test]
fn test_source_create_and_get() {}

#[test]
fn test_book_create_and_get() {}

#[test]
fn test_chapter_create_and_get_by_book() {}

#[test]
fn test_delete_book_cascades_chapters_progress_bookmarks() {}

#[test]
fn test_source_batch_insert() {}

#[test]
fn test_init_database_with_relative_path() {}
```

---

## 2. 错误类型设计不统一

当前大多数 DAO 返回：

```rust
rusqlite::Result<T>
```

这对纯数据库操作可以接受。

但 `import_from_json()` 同时包含 JSON 解析和数据库写入，却仍强行返回：

```rust
SqlResult<usize>
```

导致错误信息丢失。

建议：

- DAO 基础 CRUD 可以继续用 `rusqlite::Result`。
- 跨领域操作，如 JSON 导入、迁移、初始化，使用自定义错误类型。

---

## 3. 外键策略需要重新审视

当前关系：

```sql
books.source_id -> book_sources(id)
chapters.book_id -> books(id) ON DELETE CASCADE
book_progress.book_id -> books(id) ON DELETE CASCADE
bookmarks.book_id -> books(id) ON DELETE CASCADE
```

问题：

- 删除书籍会删除章节、进度、书签，合理。
- 删除书源时，如果存在书籍，会失败。
- `SourceDao::delete()` 没有说明这个行为。

建议：

如果业务希望删除书源但保留书籍，则可以：

```sql
source_id TEXT
FOREIGN KEY (source_id) REFERENCES book_sources(id) ON DELETE SET NULL
```

但这需要 `source_id` 允许 NULL。

如果业务希望删除书源连带删除书籍，则：

```sql
FOREIGN KEY (source_id) REFERENCES book_sources(id) ON DELETE CASCADE
```

如果业务希望禁止删除，则 DAO 应先检查并返回明确错误。

---

## 4. 缺少若干关键索引

已有索引：

```sql
idx_books_source_id
idx_chapters_book_id
idx_chapters_index
idx_bookmarks_book_id
```

建议补充：

```sql
CREATE INDEX IF NOT EXISTS idx_books_order_time ON books(order_time DESC);
CREATE INDEX IF NOT EXISTS idx_books_name ON books(name);
CREATE INDEX IF NOT EXISTS idx_books_author ON books(author);
CREATE INDEX IF NOT EXISTS idx_book_sources_enabled_order ON book_sources(enabled, custom_order, weight);
```

不过 `LIKE '%keyword%'` 普通索引无法有效使用。如需全文搜索，应考虑 FTS5。

---

## 5. SQL 重复较多

多个 DAO 中 SELECT 字段列表重复很多。

例如 `BookDao` 中每个查询都写一遍完整字段列表。

建议提取常量：

```rust
const BOOK_COLUMNS: &str = "
    id, source_id, source_name, name, author, cover_url, chapter_count,
    latest_chapter_title, intro, kind, last_check_time, last_check_count,
    total_word_count, can_update, order_time, latest_chapter_time,
    custom_cover_path, custom_info_json, created_at, updated_at
";
```

然后：

```rust
format!("SELECT {} FROM books WHERE id = ?", BOOK_COLUMNS)
```

这样可以减少列顺序不一致导致的隐性 bug。

---

# 剩余未修复问题清单

## 高优先级 P1

1. **`BookDao::upsert()` SQL 占位符数量错误。**
2. **`SourceDao::upsert()` SQL 占位符数量错误。**
3. **`SourceDao::batch_insert()` SQL 占位符数量错误。**
4. **默认数据库路径 `legado.db` 可能因空父目录处理错误导致初始化失败。**
5. **数据库迁移策略仍不安全：版本读取吞错、未知迁移不失败、未处理高版本数据库。**
6. **`INSERT OR REPLACE` 用于书源批量导入存在潜在数据破坏风险。**

## 中优先级 P2

1. `get_connection()` 未开启外键。
2. `chapter.get_by_url()` 未限定 `book_id`。
3. 章节缺少唯一约束，可能重复插入。
4. 书签缺少唯一约束，可能重复添加。
5. `add_read_time()` 对不存在记录静默成功。
6. `import_from_json()` 丢失 JSON 解析错误信息。
7. 删除书源时外键行为不明确。
8. LIKE 搜索未转义 `%` / `_`。
9. 缺少 busy timeout，容易遇到 SQLite 锁问题。

## 低优先级 P3

1. `SourceDao::new()` 注释乱码。
2. SQL 字段列表重复较多，可维护性一般。
3. magic integer 较多，建议使用 enum。
4. 时间戳单位、偏移量类型建议进一步规范。
5. `StorageManager` 中 DAO 借用风格不一致。

---

# 建议优先修复顺序

建议按以下顺序处理：

1. 修复 `book_dao.rs`、`source_dao.rs` 的 SQL 占位符数量。
2. 修复 `init_database()` 对相对路径父目录的处理。
3. 补充 DAO 基础单元测试，确保 CRUD 可用。
4. 重构迁移逻辑，彻底解决 P1-8。
5. 移除 `INSERT OR REPLACE`，改为 `ON CONFLICT DO UPDATE`。
6. 统一连接初始化逻辑，确保所有连接开启 foreign_keys、busy_timeout。
7. 增加必要唯一约束和索引。
8. 引入自定义错误类型，改善 JSON 导入、初始化、迁移错误表达。

---

# 最终评价

该 crate 的代码结构有较好的雏形，DAO 分层和模型定义都比较直观。但当前存在多个会直接影响核心功能的 SQL 错误，尤其是书籍和书源的创建/更新路径不可用。同时数据库迁移策略仍未达到安全可演进的标准。

**整体评级：P1**

在修复 SQL 占位符、默认路径初始化、迁移策略、批量导入 REPLACE 风险之前，不建议作为稳定存储层合入主分支或发布。

---

## 📦 core-source
**职责**: 源引擎 - 规则引擎、脚本引擎、JS 解析器

## 总体结论

**crate: core-source**  
**整体评价：P1（存在影响核心功能正确性的高优先级问题）**

该 crate 目前仍处于“原型/简化实现”阶段，结构上已经拆分为 `rule_engine / script_engine / parser / types / utils`，方向是合理的；但作为“书源规则引擎、脚本引擎、JS 解析器”的核心模块，当前实现与职责仍有明显差距：

- 搜索规则模型使用错误，`book_list` 被同时当作“搜索 URL 模板”和“结果列表选择器”使用。
- 搜索解析只返回第一个结果，未按 `book_list` 列表节点逐项解析。
- 相对 URL 规范化只在部分地方处理，章节 URL、封面 URL、书籍 URL等未统一归一化。
- Rhai 脚本资源限制已加入，但仍不完整，存在字符串切片 panic、正则 ReDoS、内存膨胀等风险。
- XPath / JSONPath / JavaScript 均为占位或简化实现，与注释宣称的能力不一致。
- 错误处理大量吞掉错误并返回 `Vec::new()` / `None`，上层无法区分“无结果”和“失败”。
- HTTP 请求缺少超时、状态码检查、header 支持、编码处理、重定向/UA/Referer 策略等源引擎必需能力。

---

# 文件级审查

## 1. `lib.rs`

**评分：B-**

### 优点

- crate 模块划分清晰。
- 主要类型通过 `pub use` 重新导出，API 使用方便。
- `parse_book_source` / `load_book_source_from_file` 简洁明了。
- `create_sample_book_source` 方便测试。

### 问题

#### 1. `validate_book_source` 语义不准确

```rust
if RuleExpression::parse(expr).is_none() && !expr.contains("{{keyword}}") {
    errors.push("搜索列表规则无效".to_string());
}
```

当前 `RuleExpression::parse` 对几乎所有非空字符串都会返回 `Some`，因为默认会当 CSS 处理。因此这个校验基本失效。

同时，`book_list` 在当前类型中是“搜索结果列表规则”，但这里又允许它包含 `{{keyword}}`，明显混淆了搜索 URL 和列表规则。

这是之前 **P1-4: BookSourceParser 搜索规则模型** 的延续问题。

#### 2. 错误类型过于粗糙

```rust
pub fn parse_book_source(json: &str) -> Result<BookSource, String>
```

核心 crate 不建议直接返回 `String`。建议定义统一错误类型，例如：

```rust
#[derive(thiserror::Error, Debug)]
pub enum SourceError {
    #[error("parse source json failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("rule error: {0}")]
    Rule(#[from] RuleError),
}
```

### 建议

- 拆分“搜索 URL 模板”和“搜索结果列表规则”。
- `validate_book_source` 应真正验证 CSS / Regex / JSONPath 等规则是否能解析。
- 不要返回 `String` 错误，使用 `thiserror` 定义结构化错误。

---

## 2. `types.rs`

**评分：C+**

### 优点

- 数据结构直观，`serde(default)` 使用较多，兼容缺省字段。
- 基本覆盖搜索、详情、目录、正文规则。
- timestamp 默认值合理。

### 主要问题

#### 1. 搜索模型缺字段，导致 parser 逻辑错误

当前：

```rust
pub struct SearchRule {
    pub book_list: Option<String>,
    pub name: Option<String>,
    pub author: Option<String>,
    pub book_url: Option<String>,
    ...
}
```

但实际书源搜索至少需要：

- `search_url` 或类似字段：搜索请求 URL 模板。
- `book_list`：搜索结果列表节点规则。
- `name` / `author` / `book_url`：相对于每个列表节点的子规则。

目前 `parser.rs` 把 `book_list` 当搜索 URL 构建：

```rust
let template = search_rule.book_list.clone()
    .unwrap_or_default()
    .replace("{{keyword}}", keyword)
```

这是严重模型错误。

**P1-4 未修复。**

#### 2. `enabled` 默认值可能不符合预期

```rust
#[serde(default)]
pub enabled: bool,
```

`bool` 默认是 `false`。如果导入书源时没有 `enabled` 字段，会默认禁用。通常书源导入默认应启用，建议：

```rust
#[serde(default = "default_enabled")]
pub enabled: bool;

fn default_enabled() -> bool {
    true
}
```

#### 3. `ExtractType` 重复定义

`types.rs` 里有一份：

```rust
pub enum ExtractType
```

`rule_engine.rs` 里又有一份：

```rust
pub enum ExtractType
```

这会造成概念重复和后续维护风险。建议只保留一份，放在 `types.rs` 或 `rule_engine.rs`，统一引用。

#### 4. 命名与 Legado / JSON 字段兼容性不足

如果目标兼容 Legado，可能需要处理 camelCase 字段，例如：

- `bookSourceName`
- `bookSourceUrl`
- `ruleSearch`
- `ruleBookInfo`
- `ruleToc`
- `ruleContent`

当前 Rust 字段是 snake_case，除非输入 JSON 也是 snake_case，否则会解析失败。建议增加：

```rust
#[serde(rename_all = "camelCase")]
```

或显式 `alias`。

### 建议

- 重构 `SearchRule`，增加 `search_url` 字段。
- 明确兼容 JSON 字段命名策略。
- 合并重复的 `ExtractType`。
- 给 `BookSource` 实现更合理的默认值。

---

## 3. `rule_engine.rs`

**评分：C**

### 优点

- 提供了统一的 `RuleExpression` 和 `RuleEngine`。
- `RuleError` 实现了 `Display` 和 `Error`。
- CSS / Regex 基本可用。
- 测试覆盖了基础类型判断。

### 主要问题

#### 1. XPath 是伪实现

```rust
let css_equiv = xpath[2..].replace("@", "");
```

这类转换非常脆弱。例如：

```xpath
//div[@class='test']
```

会变成：

```css
div[class='test']
```

这个特例可能勉强工作，但复杂 XPath 完全不支持：

- `//div/text()`
- `//a/@href`
- `contains(@class, 'x')`
- `//ul/li[1]`
- `following-sibling`
- `parent`
- `normalize-space`

注释里写“支持 XPath”，但实际不成立。建议要么引入真正 XPath 库，要么在 README / 类型层面标记为未支持。

#### 2. JSONPath 是极简实现且有 bug

```rust
let parts: Vec<&str> = path.trim_start_matches("$.")
    .split('.')
```

无法正确处理：

- `$[0]`
- `$.data.items[0]`
- `$.data[*].name`
- `$.data[?(@.id==1)]`
- `@Json:` 前缀

尤其当前 parse 中：

```rust
} else if trimmed.starts_with("$.") || trimmed.starts_with("@Json:") {
    Some(Self {
        rule_type: RuleType::JsonPath,
        expression: trimmed.to_string(),
```

如果是 `@Json:$.data`，后续 `evaluate_jsonpath` 不会去掉 `@Json:`，必然解析失败。

#### 3. Regex `/pattern/flags` 解析不完整

```rust
if trimmed.starts_with('/') && trimmed.ends_with('/') {
```

这只能识别无 flags 的 `/pattern/`。如果是 `/abc/i`，不会满足 `ends_with('/')`，会被当 CSS。

虽然后面 `parse_regex_with_flags` 支持 flags，但 `parse` 阶段已经把 `/abc/i` 排除掉了。

应改为更可靠的解析逻辑。

#### 4. 正则存在 ReDoS 风险

`regex` crate 本身避免传统回溯型灾难，但仍可能因为：

- 超长输入
- 复杂 Unicode 正则
- 大量捕获
- 无限 captures_iter 结果收集

导致 CPU / 内存占用过高。

建议限制：

- content 最大长度
- 返回结果最大数量
- 单条结果最大长度

#### 5. CSS 默认提取行为可能不符合预期

当前默认：

```rust
_ => element.inner_html()
```

多数书源中无后缀规则可能期望文本，或者根据规则上下文决定。至少需要明确语义。

#### 6. `OwnText` 实现错误

```rust
ExtractType::OwnText => {
    element.text().collect::<Vec<_>>().join("")
}
```

这和 `Text` 一样，包含所有子节点文本，不是 own text。

#### 7. `execute_rule_first` 使用 `remove(0)`

```rust
Some(v.remove(0))
```

`remove(0)` 是 O(n)。虽然结果不大时影响不明显，但可改成：

```rust
self.execute_rule(rule_str, content).ok()?.into_iter().next()
```

#### 8. `JavaScript` 类型名与实际 Rhai 不一致

注释说支持 JavaScript，但实际 `ScriptEngine` 是 Rhai，并且 `RuleEngine` 遇到 JavaScript 直接返回不支持：

```rust
RuleType::JavaScript => Err(RuleError::NotSupported("Use ScriptEngine ".into())),
```

这会误导调用方。

### 建议

- 引入成熟 JSONPath 库，例如 `jsonpath-rust`。
- 引入真正 XPath 支持，或删除对 XPath 的“支持”声明。
- 统一规则语法解析，补全 `@Json:`、`@XPath:`、`js:` 前缀处理。
- 增加规则执行资源限制：最大输入、最大输出数量、最大输出长度。
- 修复 `OwnText`。
- `RuleEngine` 应能与 `ScriptEngine` 协作处理脚本规则，而不是直接 NotSupported。

---

## 4. `script_engine.rs`

**评分：B-**

### 优点

- 使用 Rhai 替代 JS，封装清晰。
- 已设置：

```rust
engine.set_max_operations(100_000);
engine.set_max_call_levels(50);
```

这说明之前 **P1-3: ScriptEngine 资源限制** 有部分修复。
- 提供上下文变量注入。
- 提供编译 AST 和执行 AST 能力。
- 基础测试可用。

### 主要问题

#### 1. P1-3 只部分修复，资源限制仍不完整

当前限制了操作数和调用层级，但仍缺少：

- 最大字符串长度
- 最大数组/map 大小
- 最大脚本源码长度
- 最大 AST 常量大小
- 最大执行时间/取消机制
- 禁止或限制大规模字符串拼接
- 正则函数资源限制

例如脚本可以不断构造大字符串或大数组，在操作数限制触发前造成内存压力。

#### 2. `substring` 存在 UTF-8 边界 panic 风险

```rust
s[start..end].to_string()
```

`start` 和 `end` 是字节索引，但输入字符串可能包含中文、emoji 等多字节字符。如果索引不在 UTF-8 边界会 panic。

例如：

```rust
substring("测试", 1, 2)
```

可能 panic。

应使用字符索引：

```rust
s.chars()
    .skip(start as usize)
    .take((end - start) as usize)
    .collect()
```

这是一个明确的安全稳定性问题。

#### 3. `parse_json` 静默吞错

```rust
serde_json::from_str::<JsonValue>(s).unwrap_or(JsonValue::Null)
```

脚本侧无法区分 JSON 本身就是 null，还是解析失败。建议提供：

- `parse_json` 返回 Result-like 对象；
- 或额外函数 `is_json_valid`；
- 或错误转字符串。

#### 4. `to_json_string` 里 clone Dynamic 较多

```rust
if let Some(json_val) = val.clone().try_cast::<JsonValue>()
```

可接受，但在大对象下会有额外成本。

#### 5. `dynamic_to_result` 多次 clone

```rust
if let Some(s) = val.clone().try_cast::<String>() { ... }
if let Some(i) = val.clone().try_cast::<i64>() { ... }
...
```

这是常见写法但会增加开销。Rhai `Dynamic` 类型转换确实有一定限制，但可以考虑先检查类型或用更低 clone 的模式。

#### 6. `ScriptResult::as_string(self)` 消费自身

```rust
pub fn as_string(self) -> Option<String>
```

测试里能用，但 API 通常期望：

```rust
pub fn as_str(&self) -> Option<&str>
pub fn into_string(self) -> Option<String>
```

否则调用方只是想借用字符串也必须移动。

#### 7. `eval_ast` 注入上下文不完整

`eval` 注入：

```rust
result, content, url, headers, source_name
```

但 `eval_ast` 只注入：

```rust
result, content, url
```

行为不一致，容易产生 bug。

### 建议

- 修复 `substring` UTF-8 panic。
- 增加脚本源码长度、输出长度、数组长度限制。
- 如有可能，使用 Rhai 的进度回调/中断机制实现超时。
- 限制注册函数中的正则输入长度和 pattern 长度。
- 统一 `eval` 和 `eval_ast` 上下文注入。
- 改进 `ScriptResult` 的借用型 API。

---

## 5. `parser.rs`

**评分：D**

这是当前问题最多、影响核心功能正确性的文件。

### 优点

- 提供了搜索、详情、章节列表、章节内容四个主流程。
- 异步 HTTP 使用方式基本正确。
- 有 tracing 日志。

### 严重问题

#### 1. 搜索 URL 构建逻辑错误

当前：

```rust
let template = search_rule.book_list.clone()
    .unwrap_or_default()
    .replace("{{keyword}}", keyword)
```

`book_list` 应该是“搜索结果列表规则”，不是 URL 模板。搜索 URL 应该来自独立字段，例如：

```rust
source.search_url
source.rule_search.search_url
```

这导致当前 search 基本无法正确工作。

**P1-4 未修复。**

#### 2. 没有按照列表节点逐项解析

当前搜索解析：

```rust
let names = rules.name.as_ref()
    .and_then(|r| self.rule_engine.execute_rule_first(r, &html));

let authors = rules.author.as_ref()
    .and_then(|r| self.rule_engine.execute_rule_first(r, &html));
```

这只会在整个 HTML 上取第一个 name、author、cover、url，最后只 push 一个 result。

正确流程应该是：

1. 用 `book_list` 规则提取列表节点。
2. 对每个节点分别执行 name/author/book_url 等规则。
3. 组装多个 `SearchResult`。

类似：

```rust
let items = rule_engine.execute_rule_as_nodes(book_list_rule, &html)?;
for item in items {
    let name = rule_engine.execute_rule_first(name_rule, item.html())?;
    ...
}
```

当前 `RuleEngine` 只返回字符串，没有返回 DOM 节点能力，因此模型和引擎都需要调整。

#### 3. `unwrap` 风险

```rust
let rules = source.rule_search.as_ref().unwrap();
```

虽然前面 match 已经处理过 `None`，逻辑上安全，但代码结构脆弱。建议保留绑定，避免后续改动引入 panic：

```rust
let rules = match &source.rule_search {
    Some(rules) => rules,
    None => return vec![],
};
```

#### 4. HTTP 错误处理过度吞噬

所有方法失败时都返回：

- `vec![]`
- `None`

调用者无法区分：

- 网络失败
- HTTP 404/500
- 规则错误
- 解析无结果
- 书源未配置

核心库建议返回：

```rust
Result<Vec<SearchResult>, SourceError>
Result<Option<BookDetail>, SourceError>
```

如果确实希望给 UI 层简化，可额外提供 `*_lossy` 便捷方法。

#### 5. 没有检查 HTTP status

```rust
let html = response.text().await
```

如果返回 404、403、500，也会继续解析错误页。建议：

```rust
let response = response.error_for_status()?;
```

#### 6. HTTP Client 没有配置超时

```rust
http_client: HttpClient::new()
```

这可能导致请求长时间挂起。建议：

```rust
HttpClient::builder()
    .timeout(Duration::from_secs(15))
    .user_agent(...)
    .build()?
```

#### 7. 未使用书源 header

`BookSource` 有：

```rust
pub header: Option<String>
```

但 HTTP 请求完全没有使用。很多书源依赖：

- User-Agent
- Referer
- Cookie
- Accept-Language
- Authorization

#### 8. URL 规范化不完整

搜索里只对搜索 URL 做了 base join：

```rust
base.join(&template)
```

但对解析出来的这些 URL 没有归一化：

- `book_url`
- `cover_url`
- `chapter_url`
- `next_chapter_url`

例如：

```rust
book_url: book_urls.unwrap_or(search_url.clone())
```

如果 `book_urls` 是 `/book/123`，最终会原样返回 `/book/123`。

**P1-5 相对 URL 规范化只部分修复，parser 中仍未修复。**

#### 9. 章节 URL 没有 join base

```rust
let url = chapter_urls.get(i)
    .cloned()
    .unwrap_or_default();
```

应基于 `book_url` 或当前章节列表页 URL 归一化。

#### 10. 内容规则为空时仍返回 Some

```rust
None => {
    warn!("书源 {} 未配置内容规则", source.name);
    String::new()
}
...
Some(ChapterContent { content, ... })
```

未配置内容规则应该返回错误或 `None`，否则调用方会误以为解析成功但章节为空。

#### 11. `script_engine` 未使用

```rust
#[allow(dead_code)]
script_engine: ScriptEngine,
```

作为源引擎，如果规则支持脚本，parser 应该能执行脚本规则。现在脚本引擎完全未接入。

#### 12. 章节 index 使用 i32 有潜在溢出

```rust
index: i as i32
```

实际章节数不太可能超过 `i32::MAX`，但惯用上可以用 `usize` 或安全转换。

### 建议

- 重构搜索规则模型，增加 `search_url`。
- `RuleEngine` 增加节点级解析能力，不能只返回字符串。
- 所有外部方法改为返回 `Result<T, SourceError>`。
- HTTP Client 增加 timeout、UA、headers、status 检查。
- 对所有解析出的 URL 调用 `utils::build_full_url`。
- 将 `ScriptEngine` 接入规则执行流程。
- 搜索、目录、内容都应支持多结果和分页/下一页。

---

## 6. `utils.rs`

**评分：B-**

### 优点

- `build_full_url` 使用 `url::Url::join`，方向正确。
- placeholder 替换简单可用。
- 有基本单元测试。
- `merge_search_results` 简洁。

### 问题

#### 1. 相对 URL 规范化函数有，但 parser 没有系统使用

`build_full_url` 本身可用，但核心流程没有对 book_url / cover_url / chapter_url 统一调用。

这意味着 **P1-5 未完整修复**。

#### 2. `clean_html_fragment` 每次编译正则

```rust
let re = regex::Regex::new(r"\s+").unwrap();
```

每次调用都编译一次正则，性能不佳。建议：

```rust
static RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());
```

或者使用 `once_cell::sync::Lazy`。

#### 3. `unwrap` 虽然这里基本安全，但可避免

```rust
regex::Regex::new(r"\s+").unwrap()
```

固定正则 panic 风险很低，但核心库可使用 `expect("valid whitespace regex")`，语义更清楚。

#### 4. `merge_search_results` 去重 key 过弱

```rust
let key = format!("{}|{}", result.name, result.author);
```

不同书源、不同 URL 的同名同作者书可能被错误合并。建议至少使用：

- `source_id`
- `book_url`
- `name`
- `author`

或者由调用方传入去重策略。

#### 5. `replace_url_placeholders` 未处理更多常见占位符

目前只处理：

```rust
{{keyword}}
{{encode_keyword}}
```

书源中常见还可能需要：

- page
- timestamp
- source url
- encoded keyword with different charset
- JS 计算变量

当然这属于功能扩展，不是 bug。

### 建议

- 在 parser 中强制使用 `build_full_url`。
- `clean_html_fragment` 使用 Lazy Regex。
- 去重策略改为基于 URL 或 source + URL。
- URL 构建函数可以返回 `Result<Url, url::ParseError>`，避免静默返回原字符串。

---

# 重点历史问题复查

## P1-1/2: cookie 持久化测试与 API 语义（core-net/cookie.rs）

**本 crate 不包含 `core-net/cookie.rs`，无法确认。**

从当前 `core-source` 看，HTTP 请求也没有接入 cookie jar 或持久化 cookie 能力。如果 `core-source` 未来直接发请求，应考虑与 `core-net` 统一 HTTP client/cookie 策略。

状态：**本次无法验证。**

---

## P1-3: ScriptEngine 资源限制（core-source/script_engine.rs）

**部分修复，但不充分。**

已修复部分：

```rust
engine.set_max_operations(100_000);
engine.set_max_call_levels(50);
```

剩余问题：

- 缺最大脚本长度。
- 缺最大字符串/数组/map 输出限制。
- 缺执行超时/取消机制。
- 注册的正则函数缺 pattern/input 限制。
- `substring` 存在 UTF-8 切片 panic。

状态：**部分修复，仍需继续处理。**

---

## P1-4: BookSourceParser 搜索规则模型（core-source/parser.rs）

**未修复。**

主要问题：

```rust
search_rule.book_list
```

仍被当作搜索 URL 模板使用。

同时搜索结果没有按 `book_list` 列表节点逐项解析，只解析了第一个 name/author/book_url。

状态：**未修复，仍是 P1。**

---

## P1-5: 相对 URL 规范化（core-source/parser.rs, utils.rs）

**部分修复。**

已修复部分：

- `utils::build_full_url` 使用 `Url::join`。
- 搜索 URL 构建中使用了 `base.join(&template)`。

未修复部分：

- `book_url` 未规范化。
- `cover_url` 未规范化。
- `chapter_url` 未规范化。
- `next_chapter_url` 未实现。
- 详情页 URL / 目录页 URL 的 base 选择策略不明确。

状态：**部分修复，parser 中仍未系统应用。**

---

## P1-6: EPUB metadata 解析（core-parser/epub.rs）

**本 crate 不包含 `core-parser/epub.rs`，无法确认。**

状态：**本次无法验证。**

---

## P1-7: UMD 章节读取（core-parser/umd.rs）

**本 crate 不包含 `core-parser/umd.rs`，无法确认。**

状态：**本次无法验证。**

---

## P1-8: 数据库迁移策略（core-storage/database.rs）

**本 crate 不包含 `core-storage/database.rs`，无法确认。**

状态：**本次无法验证。**

---

# 主要潜在 Bug 清单

## P1 级

1. **搜索规则模型错误**
   - `book_list` 被错误用作搜索 URL。
   - 需要新增 `search_url` 或等价字段。

2. **搜索结果解析错误**
   - 只解析整个 HTML 的第一个结果。
   - 没有按列表节点逐项解析。

3. **相对 URL 未系统规范化**
   - `book_url`、`cover_url`、`chapter_url` 等会原样返回相对路径。

4. **脚本引擎仍有资源和 panic 风险**
   - `substring` 对 UTF-8 字符串可能 panic。
   - 缺内存、输出长度、执行时间限制。

5. **HTTP 请求缺超时和状态码检查**
   - 可能挂起或解析错误页。

6. **错误被吞掉**
   - 网络失败、规则错误、无结果无法区分。

## P2 级

1. XPath / JSONPath 功能与宣称不符。
2. `ScriptEngine` 未接入 `BookSourceParser`。
3. `enabled` 默认 false 可能导致导入书源默认禁用。
4. `ExtractType` 重复定义。
5. `OwnText` 实现与语义不符。
6. `eval` 与 `eval_ast` 注入上下文不一致。
7. `clean_html_fragment` 每次编译正则。
8. JSONPath `@Json:` 前缀未处理。

---

# 建议的重构方向

## 1. 调整类型模型

建议：

```rust
pub struct SearchRule {
    pub search_url: Option<String>,
    pub book_list: Option<String>,
    pub name: Option<String>,
    pub author: Option<String>,
    pub book_url: Option<String>,
    pub cover_url: Option<String>,
    pub kind: Option<String>,
    pub last_chapter: Option<String>,
}
```

或者搜索 URL 放在 `BookSource` 顶层。

---

## 2. 解析 API 改为 Result

例如：

```rust
pub async fn search(
    &self,
    source: &BookSource,
    keyword: &str,
) -> Result<Vec<SearchResult>, SourceError>
```

不要直接返回空 Vec。

---

## 3. 增加节点级规则执行

当前 `RuleEngine` 只返回字符串，无法支持列表节点上下文。建议增加：

```rust
execute_nodes(rule, html) -> Vec<NodeContext>
execute_rule_in_node(rule, node) -> Vec<String>
```

否则搜索、目录等多项解析会一直不准确。

---

## 4. 统一 URL 规范化

所有 URL 字段都经过：

```rust
build_full_url(current_page_url, extracted_url)
```

包括：

- 搜索结果 book_url
- cover_url
- detail chapters_url
- chapter url
- next_chapter_url

---

## 5. 完善脚本限制

至少增加：

- 脚本源码最大长度。
- 输入 content 最大长度。
- 输出字符串最大长度。
- 数组/map 最大长度。
- 正则 pattern/input 最大长度。
- UTF-8 安全 substring。
- 可选执行超时或取消标记。

---

# 最终评分汇总

| 文件 | 评分 | 说明 |
|---|---:|---|
| `lib.rs` | B- | 结构清晰，但校验逻辑弱，错误类型粗糙 |
| `types.rs` | C+ | 基础结构有了，但搜索模型错误，字段兼容性不足 |
| `rule_engine.rs` | C | CSS/Regex 基础可用，但 XPath/JSONPath 简化过度，语义不完整 |
| `script_engine.rs` | B- | 已加入部分资源限制，但仍有 panic 和资源滥用风险 |
| `parser.rs` | D | 核心流程逻辑错误，搜索模型、列表解析、URL 规范化、错误处理都需重构 |
| `utils.rs` | B- | 工具函数方向正确，但未被系统使用，有小性能问题 |

---

# crate 整体评价

**P1**

原因：当前核心搜索流程和规则模型仍然不正确，会直接影响 crate 的主要职责。脚本资源限制虽有进步，但还没有达到安全执行外部书源脚本的要求。建议在进入大规模接入书源或上层 UI 前，优先修复 `parser.rs` 和 `types.rs` 的模型问题。

---

---
🟢 审核完成