# Legado Flutter 重构架构设计文档

> ⚠️ **此为原始架构设计文档**（v0.1, 2026-04-30）。代码已继续演进，实施阶段可能与原设计有偏差。最新状态以 `CURRENT_STATUS.md` 为准。

> 基于开源阅读应用 Legado (legado) 的 Flutter + Rust 重构方案
> 版本: v0.1 | 日期: 2026-04-30

---

## 1. 架构总览

```
┌─────────────────────────────────────────────────┐
│                Flutter UI Layer                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ 书架/搜索 │ │  阅读器   │ │ 书源管理/设置...  │ │
│  └────┬─────┘ └────┬─────┘ └────────┬─────────┘ │
│       │            │                │            │
│  ┌────▼────────────▼────────────────▼─────────┐  │
│  │           Flutter Services Layer            │  │
│  │  (状态管理: Riverpod / 平台通道 / 路由)      │  │
│  └────────────────┬───────────────────────────┘  │
├───────────────────┼──────────────────────────────┤
│  ┌────────────────▼───────────────────────────┐  │
│  │         flutter_rust_bridge (FFI)          │  │
│  │  ┌─────────────┐     ┌──────────────────┐  │  │
│  │  │ Dart -> Rust │     │ Rust -> Dart     │  │  │
│  │  │ (call)       │     │ (stream/callback)│  │  │
│  │  └─────────────┘     └──────────────────┘  │  │
│  └────────────────┬───────────────────────────┘  │
├───────────────────┼──────────────────────────────┤
│  ┌────────────────▼───────────────────────────┐  │
│  │            Rust Core Engine                 │  │
│  │                                            │  │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────┐  │  │
│  │  │ core-net │ │ core-    │ │ core-storage│  │  │
│  │  │ 网络引擎  │ │ parser   │ │ 存储引擎    │  │  │
│  │  │          │ │ 格式解析  │ │ (SQLite)    │  │  │
│  │  └────┬─────┘ └────┬─────┘ └──────┬──────┘  │  │
│  │       │            │              │          │  │
│  │  ┌────▼────────────▼──────────────▼──────┐   │  │
│  │  │          core-source                   │   │  │
│  │  │     书源规则引擎 (规则解析/JS沙箱)      │   │  │
│  │  └───────────────────────────────────────┘   │  │
│  └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## 2. 核心模块职责

### 2.1 Rust Core 层

| 模块 | 职责 | 与 Legado 原模块对应 |
|------|------|---------------------|
| **core-net** | HTTP 请求管理、Cookie 持久化、并发控制、代理支持 | `help/http/`, `lib/cronet/` |
| **core-parser** | TXT/EPUB/UMD/MOBI 格式解析、章节提取、内容清洗 | `modules/book/`, `help/book/` |
| **core-storage** | 书架/书源/进度/书签/替换规则的持久化 (SQLite) | `data/entities/`, `help/storage/` |
| **core-source** | **核心中的核心**：书源规则引擎、JS沙箱执行、规则表达式(XPath/CSS/Regex/JSONPath)解析 | `model/webBook/`, `model/analyzeRule/`, `help/source/`, `modules/rhino/` |

### 2.2 Flutter UI 层

| 模块 | 职责 | 对应原 UI 模块 |
|------|------|---------------|
| **features/bookshelf** | 书架展示（网格/列表）、分组管理、搜索过滤 | `ui/book/`, `ui/main/` |
| **features/reader** | 阅读器核心：翻页、排版、字体、主题、进度、书签、TTS | `ui/book/read/`, `service/` |
| **features/search** | 全网搜索、发现页、书源搜索 | `ui/book/search/` |
| **features/source** | 书源管理（导入/编辑/启用/禁用/校验） | `ui/book/source/` |
| **features/settings** | 应用设置、主题、字体、阅读偏好 | `ui/config/`, `ui/about/` |

### 2.3 Bridge 层

通过 `flutter_rust_bridge` 实现 Dart ↔ Rust 双向调用：

```
Dart → Rust:   同步/异步函数调用（搜索书籍、获取章节、解析内容）
Rust → Dart:   Stream 推送（下载进度、TTS 状态、日志流）
```

## 3. 核心数据流

### 3.1 搜索流程
```
用户输入关键词
  → Flutter UI (SearchPage)
  → Dart 调用 search_books(keyword, sources)
  → FFI Bridge
  → Rust core-source::search()
      → core-net::fetch() 并发请求各书源
      → core-source::analyze() 用书源规则解析 HTML
      → 返回统一格式的 SearchResult[]
  → FFI Bridge
  → Flutter UI 渲染结果列表
```

### 3.2 阅读流程
```
用户点击书籍
  → Flutter UI 请求书籍详情/章节列表
  → Rust core-source::book_info() + core-storage::get_progress()
  → 返回 Book + Chapter[] + 阅读进度
  → Flutter Reader 渲染
  → 翻页时:
      → 请求章节内容 (core-source::chapter_content / core-parser::parse)
      → Rust 解析并返回排版后的文本
      → Flutter TextPainter / CustomPainter 渲染
  → 退出时:
      → 保存进度到 core-storage
```

### 3.3 规则解析流程 (核心)
```
书源规则 JSON (如 {规则书源名, 规则URL, 规则列表, 规则图书信息, ...})
  → core-source::RuleEngine
      → 解析规则表达式类型 (XPath/CSS/Regex/JSONPath)
      → 对 HTML/JSON 内容执行提取
      → 可选: JS 脚本预处理/后处理 (quickjs/boa 沙箱)
  → 返回结构化数据
```

## 4. 数据模型 (核心 Entity)

```rust
// === 书源 ===
struct BookSource {
    id: String,
    name: String,
    url: String,
    rule_search_url: String,
    rule_book_info: RuleBookInfo,
    rule_chapter_list: RuleExpression,
    rule_content: RuleExpression,
    // ...
}

// === 书籍 ===
struct Book {
    id: String,
    source_id: String,
    name: String,
    author: String,
    cover_url: String,
    chapter_count: i32,
    // ...
}

// === 章节 ===
struct Chapter {
    id: String,
    book_id: String,
    index: i32,
    title: String,
    url: String,
    // ...
}

// === 阅读进度 ===
struct BookProgress {
    book_id: String,
    chapter_index: i32,
    paragraph_index: i32,
    offset: i32,
    updated_at: i64,
}

// === 替换规则 ===
struct ReplaceRule {
    id: String,
    name: String,
    pattern: String,
    replacement: String,
    enabled: bool,
}
```

## 5. 技术选型

| 层面 | 选择 | 理由 |
|------|------|------|
| UI 框架 | Flutter 3.x | 跨平台、高性能、单一代码库 |
| 状态管理 | Riverpod 2.x | 类型安全、编译期可靠、依赖注入友好 |
| Rust 核心 | Rust edition 2024 | 高性能、内存安全、跨平台 |
| JS 引擎 | quickjs-rs (或 boa) | 轻量级、沙箱化执行书源脚本 |
| 数据库 | SQLite (rusqlite) | 轻量、可靠、广泛使用 |
| HTTP | reqwest + rustls | 异步、TLS 支持 |
| HTML 解析 | scraper (CSS) + quick-xml + regex | 覆盖 XPath/CSS/Regex/JSONPath |
| FFI 桥接 | flutter_rust_bridge 2.x | 自动生成绑定、Stream 支持 |

## 6. 阶段路线图

```
Phase 0: 基础设施搭建     [当前]    1-2 周
Phase 1: Rust 核心引擎    [下一步]  3-4 周
Phase 2: Flutter UI 框架  [并行]    2-3 周
Phase 3: 功能整合与桥接   [Phase1+2后] 2-3 周
Phase 4: 高级服务移植     [之后]    2-3 周
Phase 5: 平台适配与发布   [最后]    1-2 周
```

详见 `ROADMAP.md`
