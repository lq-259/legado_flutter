# Legado Flutter + Rust 重构计划文档

> ⚠️ **此为原始计划文档**（v1.0, 2026-04-30，状态: 待审阅）。代码已继续演进，阶段状态可能与原计划有偏差。最新状态以 `CURRENT_STATUS.md` 为准。

> 架构师: opencode/big-pickle  
> 版本: v1.0 | 日期: 2026-04-30  
> 状态: 待审阅

---

## 目录

1. [项目概述](#1-项目概述)
2. [原 Legado 架构分析](#2-原-legado-架构分析)
3. [新架构设计](#3-新架构设计)
4. [技术方案详解](#4-技术方案详解)
5. [分阶段实施计划](#5-分阶段实施计划)
6. [Phase 1 详细设计](#6-phase-1-详细设计)
7. [风险和应对](#7-风险和应对)
8. [附录](#8-附录)

---

## 1. 项目概述

### 1.1 项目背景

Legado（阅读）是一款开源的 Android 小说阅读器，拥有 46K+ GitHub Stars，支持：

- **自定义书源**：通过规则配置从不同网站抓取书籍内容
- **本地阅读**：支持 TXT、EPUB、UMD 等格式
- **高度可定制**：阅读界面、翻页方式、主题等均可配置
- **脚本支持**：通过 JavaScript 实现复杂的书源逻辑

原项目使用 Kotlin 开发，包含 **813 个 Kotlin 文件**，核心模块包括：

| 模块 | 文件数 | 核心功能 |
|------|--------|------|
| `app/` | ~300 | 主应用、UI、数据库、服务 |
| `modules/book/` | ~150 | 书籍解析（EPUB/TXT/UMD） |
| `modules/rhino/` | ~50 | JavaScript 脚本引擎 |
| `data/entities/` | ~30 | 数据实体定义 |
| `model/webBook/` | ~40 | 书源规则解析 |
| `model/analyzeRule/` | ~30 | 规则引擎核心 |
| `service/` | ~60 | 后台服务（更新、TTS、下载） |
| `ui/` | ~200 | 用户界面（书架、阅读器、搜索等） |

### 1.2 重构目标

| 目标 | 说明 |
|------|------|
| **跨平台支持** | 从 Android Only → Android/iOS/Windows/macOS/Linux |
| **性能提升** | Rust 核心引擎 + Flutter 渲染，内存占用降低 30%+ |
| **架构现代化** | 清晰的分层架构、类型安全、可测试性 |
| **功能对等** | 保持与原版功能对等，同时扩展新特性 |
| **社区友好** | 开放架构、完善文档、低门槛贡献 |

### 1.3 核心挑战

1. **书源规则引擎**：原项目支持 5 种规则类型（CSS/XPath/JSONPath/Regex/JS），需完整移植
2. **数据模型复杂**：813 个 Kotlin 文件，涉及 50+ 数据实体
3. **脚本引擎**：原项目使用 Rhino (Java JS 引擎)，需迁移到 Rust 可用方案
4. **阅读体验**：原项目阅读器高度可定制，需完整移植排版引擎

---

## 2. 原 Legado 架构分析

### 2.1 整体架构（分层）

```
┌─────────────────────────────────────────────────┐
│                    UI 层                      │
│  (ui/book/, ui/main/, ui/config/, ...)      │
└────────────────────┬────────────────────────┘
                     │
┌────────────────────▼────────────────────────┐
│                业务逻辑层                      │
│  model/webBook/ (书源解析)                   │
│  model/analyzeRule/ (规则引擎)                │
│  modules/rhino/ (JS 脚本)                   │
│  service/ (后台服务：更新/TTS/下载)          │
└────────────────────┬────────────────────────┘
                     │
┌────────────────────▼────────────────────────┐
│                  数据层                        │
│  data/entities/ (Room 实体)                  │
│  help/storage/ (数据库管理)                    │
│  help/http/ (网络请求 - OkHttp)              │
│  modules/book/ (本地书籍解析)                │
└───────────────────────────────────────────────┘
```

### 2.2 书源规则系统（核心中的核心）

原 Legado 的书源规则是一个嵌套的 JSON 结构：

```json
{
  "bookSourceName": "示例书源",
  "bookSourceUrl": "https://example.com",
  "searchUrl": "/search?q={{key}}",
  "ruleSearch": {
    "bookList": ".book-list .item",
    "name": ".title@text",
    "author": ".author@text",
    "bookUrl": "a@href",
    "coverUrl": "img@src"
  },
  "ruleBookInfo": {
    "name": "h1@text",
    "author": ".author@text",
    "intro": ".intro@text",
    "coverUrl": ".cover img@src"
  },
  "ruleToc": {
    "chapterList": "#chapter-list li",
    "chapterName": "a@text",
    "chapterUrl": "a@href"
  },
  "ruleContent": {
    "content": "#content@html"
  },
  "jsLib": "function customFunc() { ... }",
  "loginUrl": "https://example.com/login",
  "header": "User-Agent: xxx"
}
```

**规则类型详解**：

| 规则前缀 | 类型 | 示例 | 说明 |
|----------|------|------|------|
| 无前缀或 `@@` | CSS 选择器 | `.title@text` | 使用 Jsoup CSS 选择器 |
| `//` 或 `@XPath:` | XPath | `//div[@class='book']` | XML 路径表达式 |
| `$.` 或 `@Json:` | JSONPath | `$.data.books[*].title` | JSON 数据提取 |
| `/pattern/` | 正则 | `/<title>(.*?)<\/title>/` | 正则表达式匹配 |
| `js:` 或特殊字段 | JavaScript | `jsLib` 字段 | Rhino 引擎执行 |

**提取类型后缀**：

| 后缀 | 说明 | 适用场景 |
|------|------|----------|
| `@text` | 提取文本内容 | 书名、作者、章节名 |
| `@html` | 提取 HTML 结构 | 正文内容（保留格式） |
| `@href` | 提取链接地址 | 书籍链接、章节链接 |
| `@src` | 提取资源地址 | 封面图片 |
| `@ownText` | 仅提取元素自身文本 | 去除广告、子元素干扰 |

**特殊语法**：

- `{{key}}` - 占位符，搜索时替换为关键词
- `##pattern##replacement` - 正则替换（用于清洗内容）
- `.0` / `.-1` - 选择第 N 个或倒数第 N 个元素
- `text.文本` - 文本选择器（选择包含指定文本的元素）

### 2.3 数据模型（核心实体）

```kotlin
// 书源实体
@Entity(tableName = "book_sources")
data class BookSource(
    @PrimaryKey var bookSourceUrl: String,
    var bookSourceName: String,
    var bookSourceType: Int,  // 0=小说, 1=音频, 2=图片, 3=RSS
    var bookSourceGroup: String?,
    var enabled: Boolean = true,
    var customOrder: Int = 0,
    var ruleSearch: String?,  // JSON 格式的规则
    var ruleBookInfo: String?,
    var ruleToc: String?,
    var ruleContent: String?,
    // ... 更多字段
)

// 书籍实体
@Entity(tableName = "books")
data class Book(
    @PrimaryKey var bookUrl: String,
    var bookSourceName: String?,
    var name: String,
    var author: String?,
    var coverUrl: String?,
    var chapterCount: Int = 0,
    var latestChapterTitle: String?,
    var intro: String?,
    var lastCheckTime: Long = 0,
    // ... 更多字段
)

// 章节实体
@Entity(tableName = "chapters")
data class Chapter(
    @PrimaryKey var url: String,
    var bookUrl: String,
    var index: Int,
    var title: String,
    var isVolume: Boolean = false,
    // ... 更多字段
)
```

### 2.4 核心流程分析

**搜索流程**：

```
用户输入关键词
  → SearchActivity (UI)
  → ViewModel 调用数据源
  → BookSource.analyzeRule() 解析搜索规则
    → OkHttp 发起请求 (help/http/)
    → 获取 HTML/JSON 响应
    → AnalyzeRule (model/analyzeRule/)
      → 根据规则类型调用对应解析器:
        - JsoupParser (CSS)
        - XPathParser (XPath)
        - JsonPathParser (JSONPath)
        - RegexParser (Regex)
      → 提取字段: name, author, bookUrl, coverUrl
    → 返回 List<SearchBook>
  → UI 渲染结果列表
```

**阅读流程**：

```
用户点击书籍
  → 获取书籍详情 (ruleBookInfo)
  → 获取章节列表 (ruleToc)
  → 保存章节到数据库
  → 打开阅读器 (ReaderActivity)
  → 翻页时:
    → 根据章节 URL 获取内容 (ruleContent)
    → 可选: JS 脚本后处理 (modules/rhino/)
    → 内容清洗 (替换规则)
    → 排版渲染 (TextPaint/CustomPaint)
  → 退出时保存进度
```

---

## 3. 新架构设计

### 3.1 整体架构

```
┌───────────────────────────────────────────────────────────┐
│                      Flutter UI 层                      │
│                                                              │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐       │
│  │ 书架     │ │ 阅读器   │ │ 搜索/书源管理  │       │
│  │          │ │          │ │ 设置/其他...  │       │
│  └────┬─────┘ └────┬─────┘ └────────┬─────────┘       │
│       │            │                │                  │
│  ┌────▼────────────▼────────────────▼─────────┐    │
│  │          Flutter Services Layer                 │    │
│  │  (状态管理: Riverpod / 路由 / 平台通道)     │    │
│  └────────────────┬───────────────────────────┘    │
├───────────────────┼──────────────────────────────┤
│  ┌────────────────▼───────────────────────────┐    │
│  │         flutter_rust_bridge (FFI)           │    │
│  │  ┌─────────────┐     ┌──────────────────┐ │    │
│  │  │ Dart → Rust │     │ Rust → Dart      │ │    │
│  │  │ (call)       │     │ (stream/callback)│ │    │
│  │  └─────────────┘     └──────────────────┘ │    │
│  └────────────────┬───────────────────────────┘    │
├───────────────────┼──────────────────────────────┤
│  ┌────────────────▼───────────────────────────┐    │
│  │            Rust Core Engine                  │    │
│  │                                             │    │
│  │  ┌──────────┐ ┌──────────┐ ┌────────┐ │    │
│  │  │ core-net │ │ core-    │ │ core-  │ │    │
│  │  │ 网络引擎 │ │ parser   │ │storage │ │    │
│  │  │          │ │ 格式解析 │ │ SQLite │ │    │
│  │  └────┬─────┘ └────┬─────┘ └───┬───┘ │    │
│  │       │            │            │       │    │
│  │  ┌────▼────────────▼────────────▼───┐ │    │
│  │  │          core-source                │ │    │
│  │  │  书源规则引擎 (核心)             │ │    │
│  │  │  - RuleEngine (CSS/XPath/Regex) │ │    │
│  │  │  - ScriptEngine (Rhai/JS)      │ │    │
│  │  │  - Parser (搜索/详情/章节)     │ │    │
│  │  └────────────────────────────────┘ │    │
│  └──────────────────────────────────────┘    │
└───────────────────────────────────────────────────┘
```

### 3.2 模块职责

#### 3.2.1 Flutter UI 层

| 模块 | 职责 | 对应原模块 |
|------|------|-----------|
| **features/bookshelf** | 书架展示（网格/列表）、分组管理、搜索过滤 | `ui/book/`, `ui/main/` |
| **features/reader** | 阅读器核心：翻页、排版、字体、主题、进度、书签、TTS | `ui/book/read/`, `service/` |
| **features/search** | 全网搜索、发现页、书源搜索 | `ui/book/search/` |
| **features/source** | 书源管理（导入/编辑/启用/禁用/校验） | `ui/book/source/` |
| **features/settings** | 应用设置、主题、字体、阅读偏好 | `ui/config/`, `ui/about/` |

#### 3.2.2 Flutter Services 层

| 组件 | 职责 | 技术选型 |
|------|------|-----------|
| **State Management** | 全局状态管理 | Riverpod 2.x |
| **Router** | 页面路由管理 | go_router |
| **Theme System** | 主题切换、动态配色 | Material 3 |
| **Bridge Service** | 调用 Rust 核心 | flutter_rust_bridge |

#### 3.2.3 Rust Core 层

| 模块 | 职责 | 对应原模块 |
|------|------|-----------|
| **core-net** | HTTP 请求、Cookie 管理、代理、并发控制 | `help/http/`, `lib/cronet/` |
| **core-parser** | TXT/EPUB/UMD 解析、章节提取、内容清洗 | `modules/book/`, `help/book/` |
| **core-storage** | SQLite 数据库、CRUD 操作、迁移 | `data/entities/`, `help/storage/` |
| **core-source** | **核心**：书源规则引擎、脚本执行、数据解析 | `model/webBook/`, `model/analyzeRule/` |

---

## 4. 技术方案详解

### 4.1 技术选型对比

| 层面 | 选择 | 备选 | 理由 |
|------|------|------|------|
| **UI 框架** | Flutter 3.x | Compose Multiplatform | 成熟稳定、生态丰富、性能优秀 |
| **状态管理** | Riverpod 2.x | Bloc, Provider | 类型安全、编译期检查、依赖注入友好 |
| **Rust 版本** | Edition 2024 | 2021 | 最新特性、更好的 async/await 支持 |
| **JS 引擎** | Rhai + quickjs-rs | Boa | Rhai 轻量、quickjs-rs 兼容性好 |
| **数据库** | SQLite (rusqlite) | surrealdb | 轻量、可靠、移动端标准 |
| **HTTP 客户端** | reqwest + rustls | hyper, isahc | 异步、TLS 支持、生态成熟 |
| **HTML 解析** | scraper (CSS) + quick-xml | html5ever | 覆盖 CSS/XML 解析需求 |
| **FFI 桥接** | flutter_rust_bridge 2.x | manual FFI | 自动生成、Stream 支持、类型安全 |

### 4.2 数据模型设计（Rust Core）

```rust
// === 书源 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub source_type: i32,  // 0=小说, 1=音频, 2=图片, 3=RSS
    pub group: Option<String>,
    pub enabled: bool,
    pub custom_order: i32,
    pub weight: i32,
    
    // 规则（JSON 格式）
    pub rule_search: Option<SearchRule>,
    pub rule_book_info: Option<BookInfoRule>,
    pub rule_toc: Option<TocRule>,
    pub rule_content: Option<ContentRule>,
    
    // 其他配置
    pub login_url: Option<String>,
    pub header: Option<String>,
    pub js_lib: Option<String>,
    // ...
}

// === 搜索规则 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRule {
    pub book_list: Option<String>,  // CSS/XPath/JsonPath 表达式
    pub name: Option<String>,
    pub author: Option<String>,
    pub book_url: Option<String>,
    pub cover_url: Option<String>,
    pub kind: Option<String>,
    pub last_chapter: Option<String>,
}

// === 书籍 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub source_id: String,
    pub source_name: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub chapter_count: i32,
    pub latest_chapter_title: Option<String>,
    pub intro: Option<String>,
    pub kind: Option<String>,
    pub last_check_time: Option<i64>,
    pub can_update: bool,
    pub order_time: i64,
    // ...
}

// === 章节 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub book_id: String,
    pub index: i32,
    pub title: String,
    pub url: String,
    pub is_volume: bool,
    // ...
}

// === 阅读进度 ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookProgress {
    pub book_id: String,
    pub chapter_index: i32,
    pub paragraph_index: i32,
    pub offset: i32,
    pub read_time: i64,
    pub updated_at: i64,
}
```

### 4.3 书源规则引擎设计

```
规则解析流程：
┌───────────────────────────────────────────────┐
│  书源规则 JSON (ruleSearch/ruleToc/...)    │
└───────────────────────┬───────────────────┘
                        │
            ┌───────────▼──────────┐
            │   RuleParser (解析)   │
            │  - 识别规则类型      │
            │  - 提取表达式       │
            └───────────┬──────────┘
                        │
            ┌───────────▼──────────┐
            │  RuleExpression       │
            │  - rule_type: Css    │
            │  - expression: "..." │
            └───────────┬──────────┘
                        │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼──────┐ ┌──────▼──────┐ ┌────▼─────┐
│ CssEngine   │ │ XPathEngine │ │ JsonPath  │
│ (scraper)   │ │ (quick-xml) │ │ Engine    │
└───────┬──────┘ └──────┬──────┘ └────┬─────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            │
                ┌───────────▼──────────┐
                │   RegexEngine         │
                │  (regex crate)       │
                └───────────┬──────────┘
                            │
                    ┌───────▼──────┐
                    │  结果合并      │
                    │  + JS 后处理   │
                    └──────────────┘
```

### 4.4 Dart-Rust 接口设计（flutter_rust_bridge）

```rust
// bridge/src/api.rs - 暴露给 Dart 的 API

// 搜索书籍
pub async fn search_books(
    keyword: String,
    source_ids: Vec<String>,
) -> Result<Vec<SearchResult>, FfiError> {
    // 调用 core-source
}

// 获取书籍详情
pub async fn get_book_info(
    source_id: String,
    book_url: String,
) -> Result<BookDetail, FfiError> {
    // ...
}

// 获取章节列表
pub async fn get_chapters(
    book_id: String,
) -> Result<Vec<Chapter>, FfiError> {
    // ...
}

// 获取章节内容（Stream 推送）
pub fn get_chapter_content_stream(
    chapter_id: String,
) -> impl Stream<Item = ChapterContentChunk> {
    // ...
}
```

```dart
// flutter_app/lib/services/book_service.dart

class BookService {
  // 搜索书籍
  Future<List<SearchResult>> searchBooks(String keyword) async {
    return await RustLib.searchBooks(
      keyword: keyword,
      sourceIds: await _getEnabledSourceIds(),
    );
  }
  
  // 获取书籍详情
  Future<BookDetail> getBookInfo(String sourceId, String bookUrl) async {
    return await RustLib.getBookInfo(
      sourceId: sourceId,
      bookUrl: bookUrl,
    );
  }
}
```

---

## 5. 分阶段实施计划

### 5.1 阶段总览

| 阶段 | 名称 | 预计时间 | 依赖 | 状态 |
|------|------|---------|------|------|
| **Phase 0** | 基础设施搭建 | 1-2周 | 无 | ✅ 完成 |
| **Phase 1** | Rust 核心引擎 | 3-4周 | Phase 0 | 🔄 进行中 |
| **Phase 2** | Flutter UI 框架 | 2-3周 | Phase 0 | 📋 待开始 |
| **Phase 3** | 功能整合与桥接 | 2-3周 | Phase 1+2 | 📋 待开始 |
| **Phase 4** | 高级服务移植 | 2-3周 | Phase 3 | 📋 待开始 |
| **Phase 5** | 平台适配与发布 | 1-2周 | Phase 4 | 📋 待开始 |
| **Phase 6+** | 后续规划 | 持续 | Phase 5 | 📋 待开始 |

### 5.2 Phase 0: 基础设施搭建 ✅

**目标**：建立项目基础结构，配置开发环境。

**已完成任务**：
- [x] Rust Workspace 配置（core/Cargo.toml）
- [x] 四个核心 crate 的 Cargo.toml
- [x] Flutter 项目配置（pubspec.yaml）
- [x] Bridge 层框架（bridge/src/lib.rs）
- [x] 文档编写（ARCHITECTURE.md, ROADMAP.md, README.md）

### 5.3 Phase 1: Rust 核心引擎 🔄

**目标**：实现四个核心 Rust crate 的基础功能。

**依赖**：Phase 0

**详细设计见 [Section 6](#6-phase-1-详细设计)**

**任务清单**：

#### 1.1 core-net（网络引擎）
- [ ] HttpClient 封装（reqwest + rustls）
- [ ] Cookie 持久化管理（cookie_store）
- [ ] 并发控制（Semaphore）
- [ ] 代理支持（HTTP/HTTPS/SOCKS5）
- [ ] 重试机制（指数退避）
- [ ] 编码处理（encoding_rs）

#### 1.2 core-parser（格式解析）
- [ ] TXT 文件解析（自动编码检测）
- [ ] EPUB 格式解析（quick-xml + zip）
- [ ] UMD 格式解析（二进制解析）
- [ ] 章节分割算法
- [ ] 内容清洗（正则替换 + HTML 标签移除）

#### 1.3 core-storage（存储引擎）
- [ ] SQLite 数据库初始化（rusqlite）
- [ ] 数据实体定义（Book, Chapter, BookSource 等）
- [ ] DAO 层实现（CRUD + 事务）
- [ ] 数据库迁移机制
- [ ] WAL 模式优化

#### 1.4 core-source（书源引擎 - **核心**）
- [ ] 书源规则 JSON 解析
- [ ] RuleEngine 实现（CSS/XPath/JsonPath/Regex）
- [ ] ScriptEngine 集成（Rhai）
- [ ] SearchParser（搜索流程）
- [ ] BookInfoParser（详情解析）
- [ ] TocParser（章节列表解析）
- [ ] ContentParser（内容解析）
- [ ] JS 脚本支持（Rhino 迁移到 Rhai）

**交付物**：
- 可独立测试的 Rust 核心库
- 单元测试覆盖核心功能（目标 80%+）
- 书源规则引擎最小可用版本

### 5.4 Phase 2: Flutter UI 框架 📋

**目标**：搭建 Flutter 应用基础框架。

**依赖**：Phase 0

**并行开发**：可与 Phase 1 并行

**任务清单**：

#### 2.1 应用架构
- [ ] Riverpod Provider 结构设计
- [ ] 路由管理（go_router）
- [ ] 主题系统（Material 3 + 动态配色）
- [ ] 国际化支持基础（intl）

#### 2.2 书架模块 (features/bookshelf)
- [ ] 书架网格/列表视图切换
- [ ] 分组管理 UI
- [ ] 书籍封面加载（cached_network_image）
- [ ] 搜索过滤功能

#### 2.3 搜索模块 (features/search)
- [ ] 搜索框和发现页
- [ ] 搜索结果展示
- [ ] 多书源选择器

#### 2.4 书源管理 (features/source)
- [ ] 书源列表展示
- [ ] 书源导入（URL/文件）
- [ ] 书源编辑 UI
- [ ] 启用/禁用切换

#### 2.5 设置模块 (features/settings)
- [ ] 主题切换
- [ ] 字体设置
- [ ] 阅读偏好配置

**交付物**：
- 可运行的 Flutter 应用框架
- 主要页面 UI 实现（可能无完整功能）
- 状态管理架构搭建完成

### 5.5 Phase 3: 功能整合与桥接 📋

**目标**：将 Flutter UI 与 Rust 核心引擎通过 flutter_rust_bridge 连接。

**依赖**：Phase 1 + Phase 2

**任务清单**：

#### 3.1 FFI 桥接完善
- [ ] 定义完整的 Dart-Rust 接口
- [ ] 实现异步调用（search, getBookInfo, getChapters, getContent）
- [ ] 实现 Stream 推送（进度、日志、TTS）
- [ ] 错误处理桥接
- [ ] 类型映射（Rust struct ↔ Dart class）

#### 3.2 搜索流程整合
- [ ] Flutter 调用 Rust 搜索接口
- [ ] 搜索结果展示
- [ ] 多书源并发搜索
- [ ] 搜索历史持久化

#### 3.3 书架数据整合
- [ ] 书架数据从 SQLite 加载
- [ ] 添加/删除书籍
- [ ] 阅读进度保存和恢复
- [ ] 封面缓存机制

#### 3.4 书源管理整合
- [ ] 书源导入（URL/文件/二维码）
- [ ] 书源规则校验
- [ ] 书源启用/禁用
- [ ] 书源导出功能

**交付物**：
- 可完整搜索书籍的应用
- 书架基本功能可用
- 书源管理功能可用

### 5.6 Phase 4: 高级服务移植 📋

**目标**：移植原 Legado 的高级功能。

**依赖**：Phase 3

**任务清单**：

#### 4.1 阅读器核心 (features/reader)
- [ ] 翻页引擎（仿真/覆盖/滑动/滚动）
- [ ] 排版引擎（字体/行距/段距/对齐）
- [ ] 主题切换（白天/夜晚/护眼/自定义）
- [ ] 进度保存和恢复
- [ ] 书签功能
- [ ] TTS 语音朗读（Rust 后端 + Flutter 前端）

#### 4.2 内容处理
- [ ] 章节内容解析（通过 core-source）
- [ ] 替换规则应用（全局/书源级）
- [ ] 图片加载和缓存（阅读器内）
- [ ] 广告过滤规则

#### 4.3 后台服务
- [ ] 定时更新书架（后台任务）
- [ ] 下载管理（整本/章节批量下载）
- [ ] 推送通知（更新提醒）
- [ ] 自动备份

#### 4.4 同步服务
- [ ] WebDAV 同步（书源/进度/书架）
- [ ] 本地备份/恢复
- [ ] 跨设备同步（端到端加密）

**交付物**：
- 完整可用的阅读器
- TTS 功能可用
- 后台服务运行正常
- 同步功能可用

### 5.7 Phase 5: 平台适配与发布 📋

**目标**：完成各平台适配，打包发布。

**依赖**：Phase 4

**任务清单**：

#### 5.1 Android 适配
- [ ] 权限处理（存储/网络/通知）
- [ ] 通知渠道配置
- [ ] 应用图标和启动页
- [ ] 原生插件集成（如有需要）
- [ ] 性能优化（启动速度、内存占用）

#### 5.2 iOS 适配
- [ ] 签名和证书配置
- [ ] 后台模式配置
- [ ] 应用图标和启动屏
- [ ] App Store 合规检查

#### 5.3 桌面平台（可选）
- [ ] Windows 适配（msix 打包）
- [ ] macOS 适配（dmg 打包）
- [ ] Linux 适配（AppImage/Flatpak）

#### 5.4 测试与优化
- [ ] 单元测试补充（目标 90%+）
- [ ] 集成测试
- [ ] 性能优化（内存/启动速度/渲染帧率）
- [ ] 崩溃上报集成（sentry 等）

#### 5.5 发布准备
- [ ] 编写应用介绍
- [ ] 截图准备（各种屏幕尺寸）
- [ ] 隐私政策
- [ ] 开源协议确认（MIT/AGPL 选择）

**交付物**：
- Android APK/AAB 包
- iOS IPA 包（如需要）
- 桌面平台安装包（可选）
- 完整的测试报告

### 5.8 Phase 6+: 后续规划 📋

**社区生态建设**：
- [ ] 插件系统（用户自定义功能扩展）
- [ ] 社区书源分享平台
- [ ] AI 辅助书源生成
- [ ] 多设备同步（端到端加密）

**功能扩展**：
- [ ] 有声书支持（TTS 增强）
- [ ] 漫画阅读模式
- [ ] RSS 订阅功能
- [ ] 笔记和标注系统

---

## 6. Phase 1 详细设计

### 6.1 core-net（网络引擎）

#### 6.1.1 模块结构

```
core-net/
├── Cargo.toml
└── src/
    ├── lib.rs          # 模块导出
    ├── client.rs       # HttpClient 封装
    ├── cookie.rs       # Cookie 管理
    ├── proxy.rs        # 代理配置
    ├── retry.rs        # 重试机制
    └── encoding.rs     # 编码处理
```

#### 6.1.2 核心类型定义

```rust
// client.rs
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub max_concurrent: usize,
    pub max_retries: usize,
    pub user_agent: String,
    pub proxy: Option<ProxyConfig>,
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub enum ProxyType {
    Http,
    Https,
    Socks5,
}

pub struct HttpClient {
    client: Client,
    semaphore: Arc<Semaphore>,
    config: HttpClientConfig,
}

impl HttpClient {
    pub fn new(config: HttpClientConfig) -> Result<Self, Box<dyn Error>> { ... }
    
    pub async fn get(&self, url: &str) -> Result<Response, reqwest::Error> { ... }
    
    pub async fn post(&self, url: &str) -> RequestBuilder { ... }
    
    pub async fn get_text(&self, url: &str) -> Result<String, Box<dyn Error>> { ... }
    
    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Box<dyn Error>> { ... }
}
```

#### 6.1.3 关键实现细节

**并发控制**：
```rust
pub async fn get(&self, url: &str) -> Result<Response, reqwest::Error> {
    let _permit = self.semaphore.acquire().await
        .expect("Semaphore closed");
    
    self.request_with_retry(|| self.client.get(url)).await
}
```

**重试机制（指数退避）**：
```rust
async fn request_with_retry<F>(&self, request_fn: F) -> Result<Response, reqwest::Error>
where
    F: Fn() -> RequestBuilder + Clone,
{
    let mut retries = 0;
    loop {
        let response = request_fn().send().await;
        
        match response {
            Ok(resp) if resp.status().is_success() => return Ok(resp),
            Ok(resp) if resp.status().is_server_error() && retries < self.config.max_retries => {
                let backoff = Duration::from_millis(100 * 2u64.pow(retries as u32));
                tokio::time::sleep(backoff).await;
                retries += 1;
            }
            Ok(resp) => return Ok(resp),  // 客户端错误，不重试
            Err(e) if retries < self.config.max_retries && (e.is_timeout() || e.is_connect()) => {
                let backoff = Duration::from_millis(100 * 2u64.pow(retries as u32));
                tokio::time::sleep(backoff).await;
                retries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

**编码处理**：
```rust
pub async fn get_text(&self, url: &str) -> Result<String, Box<dyn Error>> {
    let response = self.get(url).await?;
    let bytes = response.bytes().await?;
    
    // 1. 尝试从 Content-Type 头获取编码
    let encoding = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').find(|s| s.trim().starts_with("charset=")))
        .map(|s| s.trim().replace("charset=", "").trim().to_lowercase());
    
    // 2. 使用 encoding_rs 解码
    let text = match encoding.as_deref() {
        Some("gbk") | Some("gb2312") | Some("gb18030") => {
            let (text, _, _) = encoding_rs::GB18030.decode(&bytes);
            text.into_owned()
        }
        Some("big5") => {
            let (text, _, _) = encoding_rs::BIG5.decode(&bytes);
            text.into_owned()
        }
        _ => {
            // 尝试 BOM 检测
            let (text, encoding, _) = encoding_rs::Encoding::for_bom(&bytes)
                .unwrap_or((encoding_rs::UTF_8, 0));
            let (text, _, _) = encoding.decode(&bytes);
            text.into_owned()
        }
    };
    
    Ok(text)
}
```

### 6.2 core-parser（格式解析）

#### 6.2.1 模块结构

```
core-parser/
├── Cargo.toml
└── src/
    ├── lib.rs          # 模块导出
    ├── txt.rs          # TXT 解析
    ├── epub.rs         # EPUB 解析
    ├── umd.rs          # UMD 解析（可选）
    ├── cleaner.rs      # 内容清洗
    └── types.rs        # 通用类型定义
```

#### 6.2.2 核心类型定义

```rust
// types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub content: String,
    pub index: usize,
    pub href: Option<String>,  // EPUB 使用
}

#[derive(Debug, Clone)]
pub struct BookMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub identifier: Option<String>,
}

// TXT 解析配置
pub struct TxtParserConfig {
    pub chapter_regex: Option<String>,  // 章节标题匹配正则
    pub remove_empty_lines: bool,
    pub clean_rules: Vec<String>,
}

// EPUB 解析结果
pub struct EpubData {
    pub metadata: BookMetadata,
    pub chapters: Vec<Chapter>,
}
```

#### 6.2.3 关键实现细节

**TXT 自动编码检测**：
```rust
fn detect_encoding(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    // 1. BOM 检测
    if bytes.len() >= 3 && bytes[0..3] == [0xEF, 0xBB, 0xBF] {
        return encoding_rs::UTF_8;
    }
    
    // 2. 统计常见中文字符
    let mut gb_count = 0;
    let mut utf8_valid = true;
    
    for window in bytes.windows(3) {
        // GBK 双字节范围检测
        if window[0] >= 0x81 && window[0] <= 0xFE {
            if window[1] >= 0x40 && window[1] <= 0xFE {
                gb_count += 1;
            }
        }
        
        // UTF-8 有效性检查（简化）
        if window[0] & 0x80 != 0 {
            if window[0] & 0xE0 == 0xC0 && window.len() > 1 {
                if window[1] & 0xC0 != 0x80 { utf8_valid = false; }
            }
        }
    }
    
    if gb_count > 10 { encoding_rs::GB18030 }
    else if utf8_valid { encoding_rs::UTF_8 }
    else { encoding_rs::GB18030 }  // 默认假设中文
}
```

**EPUB 解析流程**：
```rust
impl EpubParser {
    pub fn parse_file(&self, path: &Path) -> Result<EpubData, String> {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file)?;
        
        // 1. 读取 container.xml 找到 content.opf
        let content_opf_path = self.find_content_opf(&mut archive)?;
        
        // 2. 解析 content.opf 获取元数据和 manifest
        let (metadata, manifest, spine) = self.parse_content_opf(&mut archive, &content_opf_path)?;
        
        // 3. 按 spine 顺序解析章节
        let chapters = self.parse_chapters(&mut archive, &content_opf_path, &manifest, &spine)?;
        
        Ok(EpubData { metadata, chapters })
    }
}
```

### 6.3 core-storage（存储引擎）

#### 6.3.1 模块结构

```
core-storage/
├── Cargo.toml
└── src/
    ├── lib.rs          # 模块导出 + StorageManager
    ├── database.rs     # 数据库初始化 + 表创建
    ├── book_dao.rs     # 书籍 DAO
    ├── source_dao.rs   # 书源 DAO
    ├── chapter_dao.rs  # 章节 DAO
    ├── progress_dao.rs  # 阅读进度 DAO
    └── models.rs      # 数据实体定义（Rust struct）
```

#### 6.3.2 数据库表设计（SQLite）

```sql
-- 书源表
CREATE TABLE book_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL UNIQUE,
    source_type INTEGER DEFAULT 0,
    group_name TEXT,
    enabled INTEGER DEFAULT 1,
    custom_order INTEGER DEFAULT 0,
    weight INTEGER DEFAULT 0,
    
    -- 规则（JSON 格式存储）
    rule_search TEXT,
    rule_book_info TEXT,
    rule_toc TEXT,
    rule_content TEXT,
    
    -- 其他配置
    login_url TEXT,
    header TEXT,
    js_lib TEXT,
    
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 书籍表
CREATE TABLE books (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    source_name TEXT,
    name TEXT NOT NULL,
    author TEXT,
    cover_url TEXT,
    chapter_count INTEGER DEFAULT 0,
    latest_chapter_title TEXT,
    intro TEXT,
    kind TEXT,
    last_check_time INTEGER,
    can_update INTEGER DEFAULT 1,
    order_time INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (source_id) REFERENCES book_sources(id) ON DELETE CASCADE
);

-- 章节表
CREATE TABLE chapters (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    index_num INTEGER NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    content TEXT,
    is_volume INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- 阅读进度表
CREATE TABLE book_progress (
    book_id TEXT PRIMARY KEY,
    chapter_index INTEGER DEFAULT 0,
    paragraph_index INTEGER DEFAULT 0,
    offset INTEGER DEFAULT 0,
    read_time INTEGER DEFAULT 0,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- 书签表
CREATE TABLE bookmarks (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    chapter_index INTEGER NOT NULL,
    paragraph_index INTEGER NOT NULL,
    content TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- 替换规则表
CREATE TABLE replace_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    pattern TEXT NOT NULL,
    replacement TEXT NOT NULL,
    enabled INTEGER DEFAULT 1,
    scope INTEGER DEFAULT 0,  -- 0=全局, 1=书源, 2=书籍
    sort_number INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 索引
CREATE INDEX idx_books_source_id ON books(source_id);
CREATE INDEX idx_chapters_book_id ON chapters(book_id);
CREATE INDEX idx_chapters_index ON chapters(book_id, index_num);
CREATE INDEX idx_bookmarks_book_id ON bookmarks(book_id);
```

#### 6.3.3 DAO 层设计

```rust
// book_dao.rs
pub struct BookDao<'a> {
    conn: &'a Connection,
}

impl<'a> BookDao<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    
    pub fn upsert(&self, book: &Book) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO books (...) VALUES (?, ?, ...)
             ON CONFLICT(id) DO UPDATE SET ...",
            params![book.id, book.source_id, ...]
        )?;
        Ok(())
    }
    
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<Book>> {
        // ...
    }
    
    pub fn get_all(&self) -> SqlResult<Vec<Book>> {
        // ...
    }
    
    pub fn search(&self, keyword: &str) -> SqlResult<Vec<Book>> {
        let pattern = format!("%{}%", keyword);
        let mut stmt = self.conn.prepare(
            "SELECT * FROM books WHERE name LIKE ? OR author LIKE ? ORDER BY order_time DESC"
        )?;
        // ...
    }
}
```

### 6.4 core-source（书源引擎 - **核心**）

#### 6.4.1 模块结构

```
core-source/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块导出
    ├── rule_engine.rs      # 规则引擎（CSS/XPath/JsonPath/Regex）
    ├── script_engine.rs    # 脚本引擎（Rhai）
    ├── parser.rs           # 书源解析器（搜索/详情/章节/内容）
    ├── types.rs            # 类型定义（BookSource, SearchRule, ...）
    └── utils.rs            # 工具函数
```

#### 6.4.2 核心类型定义

```rust
// types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub source_type: i32,
    pub enabled: bool,
    
    #[serde(default)]
    pub rule_search: Option<SearchRule>,
    #[serde(default)]
    pub rule_book_info: Option<BookInfoRule>,
    #[serde(default)]
    pub rule_toc: Option<TocRule>,
    #[serde(default)]
    pub rule_content: Option<ContentRule>,
    
    pub js_lib: Option<String>,
    pub header: Option<String>,
    // ...
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRule {
    pub book_list: Option<String>,
    pub name: Option<String>,
    pub author: Option<String>,
    pub book_url: Option<String>,
    pub cover_url: Option<String>,
    pub kind: Option<String>,
    pub last_chapter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookInfoRule {
    pub name: Option<String>,
    pub author: Option<String>,
    pub intro: Option<String>,
    pub cover_url: Option<String>,
    pub kind: Option<String>,
    // ...
}

// 规则表达式
#[derive(Debug, Clone)]
pub struct RuleExpression {
    pub rule_type: RuleType,
    pub expression: String,
    pub extract_type: ExtractType,  // @text, @html, @href, @src
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuleType {
    Css,        // CSS 选择器
    XPath,      // XPath 表达式
    JsonPath,    // JSONPath 表达式
    Regex,      // 正则表达式
    JavaScript,  // JavaScript 脚本
}
```

#### 6.4.3 RuleEngine 设计

```rust
// rule_engine.rs
pub struct RuleEngine {
    css_engine: CssEngine,      // 基于 scraper
    xpath_engine: XPathEngine,  // 基于 quick-xml
    jsonpath_engine: JsonPathEngine,
    regex_engine: RegexEngine,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            css_engine: CssEngine::new(),
            xpath_engine: XPathEngine::new(),
            jsonpath_engine: JsonPathEngine::new(),
            regex_engine: RegexEngine::new(),
        }
    }
    
    /// 解析规则表达式字符串，自动识别类型
    pub fn parse_rule(rule_str: &str) -> Option<RuleExpression> {
        let trimmed = rule_str.trim();
        
        // 1. 检查提取类型后缀 (@text, @html, etc.)
        let (expr, extract_type) = Self::parse_extract_type(trimmed);
        
        // 2. 识别规则类型
        let rule_type = if expr.starts_with("//") || expr.starts_with("@XPath:") {
            RuleType::XPath
        } else if expr.starts_with("$.") || expr.starts_with("@Json:") {
            RuleType::JsonPath
        } else if expr.starts_with('/') && expr.ends_with('/') {
            RuleType::Regex
        } else if expr.contains('{') && expr.contains('}') && expr.starts_with('$') {
            RuleType::JsonPath
        } else {
            RuleType::Css  // 默认作为 CSS
        };
        
        Some(RuleExpression {
            rule_type,
            expression: expr.to_string(),
            extract_type,
        })
    }
    
    /// 执行规则，返回匹配结果
    pub fn evaluate(&self, rule: &RuleExpression, content: &str) -> Result<Vec<String>, RuleError> {
        match rule.rule_type {
            RuleType::Css => self.css_engine.evaluate(&rule.expression, content),
            RuleType::XPath => self.xpath_engine.evaluate(&rule.expression, content),
            RuleType::JsonPath => self.jsonpath_engine.evaluate(&rule.expression, content),
            RuleType::Regex => self.regex_engine.evaluate(&rule.expression, content),
            RuleType::JavaScript => Err(RuleError::NotSupported("Use ScriptEngine".into())),
        }
    }
}

// CSS 引擎实现
struct CssEngine;

impl CssEngine {
    fn evaluate(&self, selector: &str, html: &str) -> Result<Vec<String>, RuleError> {
        let document = Html::parse_document(html);
        let selector = Selector::parse(selector)
            .map_err(|e| RuleError::ParseError(e.to_string()))?;
        
        let results: Vec<String> = document
            .select(&selector)
            .map(|elem| {
                // 根据 extract_type 决定返回什么
                // @text -> elem.text().collect()
                // @html -> elem.html()
                // @href -> elem.value().attr("href")
                // ...
                elem.inner_html()  // 简化
            })
            .collect();
        
        Ok(results)
    }
}
```

#### 6.4.4 ScriptEngine 设计（Rhai）

```rust
// script_engine.rs
use rhai::{Engine, Scope, Dynamic};

pub struct ScriptEngine {
    engine: Arc<Engine>,
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        
        // 注册常用函数
        engine.register_fn("get_elements", |html: &str, selector: &str| -> Array {
            // 使用 scraper 获取元素
            // ...
        });
        
        engine.register_fn("regex_find", |text: &str, pattern: &str| -> String {
            // 正则查找
            // ...
        });
        
        Self {
            engine: Arc::new(engine),
        }
    }
    
    pub fn eval(&self, script: &str, context: &ScriptContext) -> Result<ScriptResult, String> {
        let mut scope = Scope::new();
        
        // 注入变量
        scope.push("content", context.content.clone());
        scope.push("result", context.result.clone());
        scope.push("url", context.url.clone());
        
        // 执行脚本
        let result = self.engine.eval_with_scope::<Dynamic>(&mut scope, script)
            .map_err(|e| format!("Script error: {}", e))?;
        
        Ok(ScriptResult::from(result))
    }
}
```

#### 6.4.5 BookSourceParser 设计（整合流程）

```rust
// parser.rs
pub struct BookSourceParser {
    rule_engine: RuleEngine,
    script_engine: ScriptEngine,
    http_client: HttpClient,
}

impl BookSourceParser {
    pub async fn search(&self, source: &BookSource, keyword: &str) -> Vec<SearchResult> {
        // 1. 构建搜索 URL（替换 {{key}}）
        let search_url = self.build_search_url(source, keyword);
        
        // 2. HTTP 请求
        let html = self.http_client.get_text(&search_url).await?;
        
        // 3. 解析搜索结果规则
        let rules = source.rule_search.as_ref()?;
        
        // 4. 执行 bookList 规则，获取结果列表
        let book_list_expr = RuleEngine::parse_rule(&rules.book_list?)?;
        let book_items = self.rule_engine.evaluate(&book_list_expr, &html)?;
        
        // 5. 对每个结果，提取字段
        let mut results = Vec::new();
        for item_html in book_items {
            let name = self.extract_field(&rules.name, &item_html).await;
            let author = self.extract_field(&rules.author, &item_html).await;
            let book_url = self.extract_field(&rules.book_url, &item_html).await;
            let cover_url = self.extract_field(&rules.cover_url, &item_html).await;
            
            // 可选：JS 后处理
            // ...
            
            results.push(SearchResult { name, author, book_url, cover_url, ... });
        }
        
        results
    }
    
    async fn extract_field(&self, rule_str: &Option<String>, html: &str) -> Option<String> {
        let rule = RuleEngine::parse_rule(rule_str.as_ref()?)?;
        let results = self.rule_engine.evaluate(&rule, html).ok()?;
        results.into_iter().next()
    }
}
```

---

## 7. 风险和应对

### 7.1 技术风险

| 风险 | 影响 | 概率 | 应对措施 |
|------|------|------|----------|
| **flutter_rust_bridge 稳定性** | 高 | 低 | 使用稳定版本，及时跟进更新 |
| **Rhai 脚本引擎功能限制** | 中 | 中 | 评估是否满足书源脚本需求，必要时切换 quickjs-rs |
| **书源规则引擎复杂度超预期** | 高 | 高 | 优先实现核心搜索流程，其他功能迭代完善 |
| **XPath 支持不完整** | 中 | 中 | 使用 sxd-xpath 库或简化实现 |
| **性能瓶颈** | 中 | 中 | 使用 Flutter 性能工具 + Rust profiling 定位问题 |
| **编码处理方式不兼容** | 中 | 中 | 完整测试各种中文编码（GBK/GB18030/BIG5） |

### 7.2 进度风险

| 风险 | 影响 | 概率 | 应对措施 |
|------|------|------|----------|
| **Phase 1 复杂度超预期** | 高 | 高 | 优先实现核心搜索流程，其他功能迭代完善 |
| **桥接层调试困难** | 中 | 中 | 编写详细的日志和错误提示，建立调试指南 |
| **Flutter UI 与原版差距大** | 中 | 中 | 参考原版 UI，逐步逼近视觉效果 |
| **测试覆盖率不足** | 低 | 中 | 每个阶段强制要求单元测试，目标 80%+ |

### 7.3 依赖风险

| 风险 | 影响 | 概率 | 应对措施 |
|------|------|------|----------|
| **关键 crate 停止维护** | 高 | 低 | 选择活跃维护的库，准备 fork 方案 |
| **Rust 版本升级导致兼容问题** | 中 | 低 | 锁定版本，渐进式升级 |
| **Flutter 版本升级导致兼容问题** | 中 | 低 | 锁定版本，跟随稳定版 |

---

## 8. 附录

### 8.1 开发环境配置

**必需工具**：
- Rust (edition 2024) - `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Flutter 3.x - https://docs.flutter.dev/get-started/install
- Android Studio / Xcode (如需移动端调试)
- VS Code / IntelliJ IDEA (推荐插件: Rust-analyzer, Flutter)

**可选工具**：
- sqlitebrowser - SQLite 数据库查看工具
- Postman - API 测试工具
- Wireshark - 网络请求抓包分析

### 8.2 调试指南

**Rust Core 调试**：
```bash
# 启用 tracing 日志
RUST_LOG=debug cargo run

# 单元测试
cargo test --package core-net
cargo test --package core-parser
cargo test --package core-storage
cargo test --package core-source
```

**Flutter 调试**：
```bash
# 启用网络日志
import 'dart:developer';
debugPrint("Search keyword: $keyword");

# Widget 调试
debugPaintBaselinesEnabled = true;
debugPaintPointersEnabled = true;
```

**桥接层调试**：
```bash
# 生成绑定代码时查看日志
flutter_rust_bridge_codegen generate --verbose

# 查看生成的 Dart 代码
cat flutter_app/lib/bridge_generated.dart
```

### 8.3 性能基准

**目标指标**（与原 Legado 对比）：

| 指标 | 原 Legado | 目标 | 测量方法 |
|------|-----------|------|------------|
| 应用启动时间 | ~2s | <1.5s | Flutter 性能工具 |
| 搜索响应时间（10 书源） | ~5s | <3s | 并发优化 |
| 章节加载时间 | ~500ms | <300ms | Rust 性能测试 |
| 内存占用（书架 1000 书） | ~200MB | <150MB | Dart/Flutter 内存工具 |
| 数据库查询（书籍列表） | ~100ms | <50ms | SQLite EXPLAIN QUERY PLAN |

### 8.4 贡献指南

**代码规范**：
- Rust: 遵循 rustfmt 和 clippy 建议
- Dart: 遵循 Dart style guide 和 lint 规则
- 注释: 使用中文注释（模块级）+ 英文注释（函数级复杂逻辑）
- 提交: 使用 Conventional Commits 格式

**PR 流程**：
1. Fork 项目
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

---

## 审阅检查清单

在批准此计划之前，请确认：

- [ ] 架构设计合理，分层清晰
- [ ] 技术选型符合项目需求
- [ ] Phase 1 四个 crate 设计完整
- [ ] 书源规则引擎设计覆盖原 Legado 核心功能
- [ ] 风险应对措施合理
- [ ] 时间估算符合实际
- [ ] 文档清晰易懂

---

**文档结束**

> 请审阅此计划文档，提出修改意见。审阅通过后，将进入 build mode 执行 Phase 1 实现。
