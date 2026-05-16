# Legado Flutter+Rust 重构 - 当前状态

> 自动化追踪文件 | 每次新session请先读此文件恢复上下文
> 
> **原始计划见**: `docs/plan.md`（v1.0, 2026-04-30，历史参考）
> **架构设计见**: `docs/ARCHITECTURE.md`（v0.1, 2026-04-30，历史参考）
> **历史路线图见**: `docs/ROADMAP.md`（v0.1, 2026-04-30，历史参考）
> **历史代码审查见**: `docs/phase1_code_review_report.md`（2026-04-30 22:36:55，GPT-5.5，已过期声明）

---

## 📊 总体进度

> **平台优先级**：🚀 **优先 Android**（APK 已编译，ping/pong 通过）| ⏸️ Linux 桌面方向暂停

| 阶段 | 名称 | 状态 |
|------|------|------|
| **Phase 0** | 基础设施搭建 | ⚠️ Android APK 编译通过，ping/pong FRB smoke 通过；Linux 正式打包暂停 ⏸️ |
| **Phase 0.5** | 编译错误修复 | ✅ 完成 |
| **Phase 1** | Rust 核心引擎 | 🔨 Legado 兼容层大幅推进：QuickJS 默认运行时、`java.*` bridge、真实书源合集导入、parser 接入 Legado HTTP/rule；仍缺 WebView bridge 和完整 DOM Element 语义 |
| **Phase 2** | Flutter UI 框架 | ✅ 完整实现（2026-05-04）：封面加载、网格/列表切换、字号设置、搜索历史、在线搜索、JSON 书源导入 |
| **Phase 3** | 功能整合与桥接 | ✅ 完成（2026-05-05）：多书源并发搜索、封面缓存、书源校验、书源导出、回归测试（7 Rust + 1 Flutter） |
| **Phase 4** | 高级服务移植 | 🔨 进行中：阅读器核心 ✅、替换规则 ✅、下载管理 ✅、核心链路打通 ✅ (2026-05-05)；待实现：TTS、WebDAV 同步 |
| **Phase 4.5** | API Server + HTTP Client (前后端分离) | 🔨 进行中 (2026-05-06)：Rust axum API 服务器所有路由 `cargo check` 通过；Flutter Dio HTTP 客户端 `flutter analyze` 通过；Provider 层双模式 (FRB/http)；待完成：页面 HTTP 切换、端到端联调 |
| **Phase 5** | 平台适配与发布 | ⚠️ Android 图标/启动页/通知渠道/权限已适配；✅ 真机 smoke test 通过（通知权限路由恢复已验证） |

> **当前真实阶段一句话**：Android APK 已编译通过，FRB smoke 验证通过；Phase 1-3 全部审查修复；✅ core-source Legado/parser/JS/import 回归与 no-default/js-boa/api-server checks 均通过且无新增 warning；🔨 Phase 4 高级服务大部分完成（阅读器/替换规则/下载管理/核心链路 ✅）；🔄 Phase 4.5 API Server + HTTP Client 核心架构完成 (2026-05-06)，待页面 HTTP 切换和端到端联调；TTS/WebDAV 待开发。

---

## 🏗️ Phase 0: 基础设施搭建 — ⚠️ Android APK 编译通过，ping/pong FRB smoke 通过；Linux 暂停

### 平台状态

| 平台 | 状态 | 备注 |
|------|------|------|
| **Android** | ✅ APK 编译成功，`ping()` → `pong` FRB smoke 通过 | 当前主攻平台 |
| **Linux 桌面** | ⏸️ 暂停 | 开发环境 FRB smoke 此前已验证通过，正式 bundle packaging 暂停 |

### FRB 当前真实状态

| 项目 | 状态 |
|------|------|
| `flutter_rust_bridge_codegen` 2.12.0 | ✅ 可用，已生成文件 |
| `flutter_rust_bridge.yaml` 配置 | ✅ `rust_input: crate::api`, `rust_root: core/bridge/`, `dart_output: flutter_app/lib/src/rust` |
| `core/bridge` bridge crate | ✅ 真实 bridge crate 位置（含 Cargo.toml, src/lib.rs, src/api.rs, src/frb_generated.rs） |
| `core/bridge/src/api.rs` | ✅ 35+ 函数：ping/init_legado/get_db_version + Books CRUD + Sources CRUD + Chapters CRUD + Progress/Bookmarks + Online Search/Content + 书源校验/导出 + Replace Rules CRUD (JSON 序列化方案) |
| `flutter_app/lib/src/rust/api.dart` | ✅ 35+ Dart 函数（getAllBooks, searchBooksOnline, saveSource, getBookChapters, validateSourceRules, validateSourceFromDb, exportAllSources, getReplaceRules, saveReplaceRule, deleteReplaceRule, setReplaceRuleEnabled 等，复杂类型通过 JSON 字符串传递） |
| `flutter_app/lib/src/rust/frb_generated.dart` (generated) | ✅ RustLib 类定义、初始化逻辑 |
| `flutter_app/lib/src/rust/frb_generated.io.dart` (generated) | ✅ 原生平台 FFI 加载 |
| `flutter_app/lib/src/rust/frb_generated.web.dart` (generated) | ✅ Web 平台加载 |
| `RustLib.init()` 在 `main.dart` | ✅ 开发环境已接入，非阻断方式 |
| `ping()` smoke 在 Linux desktop 已验证 | ✅ `[FRB smoke] ping() returned: pong` |
| **`ping()` smoke 在 Android 已验证** | ✅ **APK 编译通过，FRB ping/pong 测试通过** |
| Android native dynamic library build/packaging 集成 | ✅ 已集成 |
| iOS/native dynamic library build/packaging 集成 | ❌ 未完成 |
| 正式 Linux bundle `libbridge.so` 打包（CMake/native assets） | ⏸️ 暂停 |

**结论**：Android APK 编译通过，FRB smoke 已验证（`ping()` → `pong`）。Linux 桌面正式打包暂停⏸️，不宣称全平台/生产打包闭环。

### ⚠️ 手工 frb_generated 补丁（2026-05-05）

`flutter_rust_bridge_codegen generate` 在 2026-05-05 两次超时（300s/600s），因此以下 3 个 API 的桥接代码是**手工编辑**而非 codegen 生成：

| API | Dart funcId | Rust wire function |
|-----|------------|-------------------|
| `validate_source_rules` | 42 | `wire__crate__api__validate_source_rules_impl` |
| `validate_source_from_db` | 43 | `wire__crate__api__validate_source_from_db_impl` |
| `export_all_sources` | 44 | `wire__crate__api__export_all_sources_impl` |
| `get_replace_rules` | 45 | `wire__crate__api__get_replace_rules_impl` |
| `save_replace_rule` | 46 | `wire__crate__api__save_replace_rule_impl` |
| `delete_replace_rule` | 47 | `wire__crate__api__delete_replace_rule_impl` |
| `set_replace_rule_enabled` | 48 | `wire__crate__api__set_replace_rule_enabled_impl` |
| `replace_book_chapters_preserving_content` | 49 | `wire__crate__api__replace_book_chapters_preserving_content_impl` |
| `replace_book_chapters` | 50 | `wire__crate__api__replace_book_chapters_impl` |

涉及文件：
- `flutter_app/lib/src/rust/frb_generated.dart` — Dart abstract API + impl（funcId 42-50）
- `core/bridge/src/frb_generated.rs` — Rust wire functions + dispatcher（funcId 42-50）

**⚠️ 关键约束**：后续任意 `flutter_rust_bridge_codegen generate` 运行将**覆盖**这些手工改动。重新生成前必须确认 `core/bridge/src/api.rs` 中这些函数仍然存在，否则 funcId 映射会错乱。功能验证：`cargo check/test` 全部通过 + `flutter test` 全部通过。

### Phase 0.5: 编译错误修复 — ✅ 完成

已验证通过：
- `cargo check --workspace` ✅
- `cargo test --workspace` ✅ **82 passed**
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `flutter --no-version-check analyze` ✅

---

## 🎯 Phase 1: Rust 核心引擎 — 🔨 Legado 兼容层大幅推进；仍缺 WebView/完整 DOM 语义

### 整体评价

核心 Rust crates 当前能 check/test 通过，很多历史 P1 已修复。`core-source` 已从早期 Rhai/简化规则引擎推进到 QuickJS 默认运行时 + Legado HTTP/rule/import/parser 兼容层。**但不能等同于完整 Legado 核心引擎完成**，仍缺 WebView bridge、完整 DOM/Element 对象语义和更多真实站点端到端验证。

> ⚠️ 历史审查报告 `docs/phase1_code_review_report.md` 生成于 2026-04-30 22:36:55。当前代码已继续演进，部分问题已修复。请勿将历史报告中的所有未修复项直接视为当前事实。

### 已修复/已改善的历史 P1 项（不再构成当前 P1）

- `clear_domain 未实现` ✅ 当前有实现和测试
- `substring 中文/emoji 会 panic` ✅ 当前用 `char_indices()`
- `ScriptEngine 缺超时/内存/输出限制` ✅ 已改善：当前有 operations/call/string/array/map 限制、墙钟超时、输出长度限制
- `Parser 只返回首个结果` ✅ 当前批量提取并循环生成多个结果
- `book_list 被当搜索 URL 模板` ✅ 与当前代码不符：当前搜索 URL 来自 `search_rule.search_url`
- `URL 未归一化` ✅ 已改善：多处调用 `build_full_url`
- `DB 迁移策略未修复` ✅ 已改善：有版本迁移和事务回滚，测试覆盖 v1→v2
- `book_dao/source_dao SQL 占位符数量` ✅ 已匹配，DAO 测试通过

### Legado 兼容层进展（2026-05-06）

- `core-source/src/legado/` 已形成独立兼容层：`import/url/http/rule/js_runtime/context/value/selector/regex_rule`。
- 默认 JS runtime 改为 QuickJS (`js-quickjs` feature)，Boa 保留为 `js-boa` 可选 feature，`--no-default-features` 可编译。
- `@js:`、`js:`、`<js></js>`、URL `{{JS}}` 模板、URL option.js 均已接入 JS runtime。
- JS runtime 支持多语句脚本，返回最后一个表达式，覆盖真实书源常见 `var ...; ...; result;` 写法。
- 已实现大量 `java.*` bridge：`ajax/get/post/getCookie/put/get/base64/md5/URI/AES/timeFormat/htmlFormat/getString/getStringList/getElements/getZip*/readFile/readTxtFile`。
- JS bridge 本地文件读取受 `LEGADO_FILE_ROOT` 限制，阻止路径逃逸。
- parser 已接入 `LegadoHttpClient` 和 `execute_legado_rule`：搜索/详情/目录/正文流程支持 URL option.js、source header、cookie jar、charset、POST body、通用 `@js:`。
- parser 通用 `@js:` 执行与 `LegadoHttpClient` 共享 cookie jar，并继承 source header；显式 JS headers 可覆盖默认 header。
- 普通 HTTP 和 JS bridge 共用 charset 探测/解码：Content-Type、HTML meta charset、显式 `charset` header option 均有回归。
- Legado 导入支持单源数组和合集数组，宽松处理字段类型不稳定；真实样本覆盖 `sy/axdzs.json`、`sy/sdg.json`、`sy/22biqu - grok.json`、`sy/1778070297.json`。
- 旧 `RuleEngine` 仍作为 parser 兼容兜底；已接入 Legado `##` replace 规则后处理。

### 仍存在的深层引擎缺口（产品级深度工作）

- WebView 相关仍未实现：`webView:true`、`webJs`、`sourceRegex` 需要 Flutter/Android WebView bridge。
- `java.getElements` 当前主要返回字符串数组，不是完整 Legado/Jsoup DOM Element 对象。
- JS HTTP bridge 和 async `LegadoHttpClient` 已共享 cookie/header/charset 语义，但底层仍分别使用 blocking reqwest 与 async reqwest。
- UMD 仍不是真实 UMD chunk/tag 解析器。
- EPUB metadata 有基础解析，但仍较简化。

### 已通过审查修复的安全/稳定性缺口（2026-05-04，3轮 Rust 核心审查）

- ✅ Proxy URL 日志脱敏（`redact_proxy_credentials`）
- ✅ Set-Cookie 日志只记录 cookie name，value 不进日志
- ✅ `@Json:` 前缀剥离
- ✅ Regex flags 检测修正（`starts_with('/') && !starts_with("//")`）
- ✅ EPUB3 cover-image 支持多 token（`split_whitespace()`）
- ✅ `source_dao` URL 冲突不再 DELETE，改为查询复用已有 id（保留 books foreign key）
- ✅ `source_dao` 错误区分（只对 `QueryReturnedNoRows` fallback，其他 DB 错误直接返回）
- ✅ Semaphore 注释改为设计决策说明

### Phase 1 深层引擎 bug 修复（2026-05-04，代码审查发现 4 项 + 测试修复 1 项）

| 严重度 | 文件 | 问题 | 修复 |
|--------|------|------|------|
| 🔴 High | `search_page.dart` | 在线搜索使用 `searchBooksOnline` 导致 storage/core-source schema 不匹配 | 改为 `searchWithSourceFromDb(dbPath, sourceId, keyword)`，该 API 内部处理 storage→core-source 转换 |
| 🔴 High | `search_page.dart` | 保存在线搜索结果时缺少 `chapter_count` 等字段导致 deserialize 失败 | 新增 `_saveResultToBookshelf` 方法，填充所有缺失字段的默认值（0/true/now） |
| 🔴 High | `rule_engine.rs` | XPath 绝对路径检测：`(` 字符同时出现在 XPath 函数和 regex 捕获组中，导致 regex 误判为 XPath | 增加 regex delimiter 检测（最后一个 `/` 之后只有字母→regex），再用 XPath 特征检测 |
| 🔴 High | `rule_engine.rs` | JSONPath bracket 表示法 `$[0]`、`$['key']`、`$["key"]` 未被识别 | 增加 `$[` 前缀检测 |
| 🟡 Medium | `source_dao.rs` | `import_from_json` 无法解析真实 Legado 导出格式（camelCase 字段 + 嵌套 rule 对象） | 新增 `LegadoBookSource` struct（`serde(rename)`）+ `legado_to_storage` 转换，fallback 尝试 |

### 各 crate 状态

- **core-net**: HttpClient 封装，Cookie 持久化管理，POST/GET 统一 Cookie 生命周期，httpmock 集成测试覆盖 Set-Cookie 提取/注入/持久化（30 tests）
- **core-parser**: UMD 畸形输入防护（章节数上限、offset 校验、大章节限制），TXT/EPUB 基础框架（19 tests）
- **core-storage**: SQLite 数据库 + PRAGMA user_version 增量迁移（v1→v2），DAO 层框架，Legado 格式书源导入（11 tests）
- **core-source**: Legado 兼容层（真实导入/URL/HTTP/rule/QuickJS/java bridge/parser 接入），旧 RuleEngine 作为兜底；parser + JS runtime + Legado import/rule/url + rule_engine 回归持续通过

### 构建状态

最近专项验证（2026-05-06）：`cargo test -p core-source parser::tests` ✅ 14 passed；`js_runtime` ✅ 34 passed；`legado::rule::tests` ✅ 5 passed；`legado::url::tests` ✅ 10 passed；`legado::import::tests` ✅ 8 passed；`rule_engine::tests` ✅ 7 passed；`cargo check -p core-source --no-default-features` ✅；`cargo check -p core-source --no-default-features --features js-boa` ✅；`cargo check -p api-server` ✅。

---

## 🔄 Phase 2: Flutter UI 框架 — ✅ 完整实现（2026-05-04）

所有 ROADMAP 2.1-2.5 功能已实现，`flutter analyze` 无任何 issue，`flutter test` 48/48 通过。

### Phase 2 新增功能（2026-05-04）

| 功能 | 文件 | 实现 |
|------|------|------|
| 书籍封面网络加载 | `bookshelf_page.dart` | `Image.network` + `errorBuilder`/`loadingBuilder`，无效 URL 显示占位图标 |
| 书架网格/列表切换 | `bookshelf_page.dart` | `ConsumerStatefulWidget` + `_isGridView` + `GridView.builder` (crossAxisCount:3) |
| 字号设置（滑块+持久化+阅读器集成） | `settings_page.dart`, `providers.dart`, `reader_page.dart`, `main.dart` | `fontSizeProvider` (14-28, 默认18), `settings.json` 持久化, 阅读器实时应用, 启动恢复 |
| 搜索历史记录 | `search_page.dart`, `providers.dart` | 最多20条, `settings.json` 持久化, 点击复用, 一键清除 |
| 在线多书源搜索切换 | `search_page.dart` | 云/手机图标切换, 通过 `searchWithSourceFromDb` 调用（内部处理 storage→core-source 转换，避免 schema 不匹配） |
| 搜索结果保存到书架 | `search_page.dart` | `_saveResultToBookshelf` 方法填充缺失字段默认值，确保 deserialize 成功 |
| JSON 书源导入 | `source_page.dart` | 对话框输入, 调用 `importSourcesFromJson`, 支持内部格式 + Legado 导出格式（camelCase），显示成功数量 |

### 构建状态（2026-05-04 最终验证）

| 检查项 | 结果 |
|--------|------|
| `flutter analyze` | ✅ **No issues found** |
| `cargo check --workspace` | ✅ 通过 |
| `cargo test --workspace` | ✅ **101 passed**, 0 failed |
| `cargo clippy --workspace -- -D warnings` | ⚠️ 1 pre-existing `type_complexity` at `epub.rs:96` |
| `flutter test` | ✅ **8 passed**, 0 failed |
| `flutter build apk --debug` | ⏱️ 超时（环境资源受限），此前已通过 |

### Phase 2 代码审查修复（2026-05-02，3轮）

**P1 修复：**
| 问题 | 文件 | 修复 |
|------|------|------|
| 添加书源 JSON 不完整 | `api.rs:92-103` | 新增 `create_source(name, url)` API |
| DB 初始化竞态 | `providers.dart` | `allBooksProvider` 等 await `dbInitializedProvider.future` |
| error swallowing | `providers.dart` | `dbInitializedProvider` 改为 `rethrow` |
| Android DB 相对路径 | `providers.dart` | `dbDirProvider` 通过 `path_provider` 获取绝对路径 |
| Android 无 INTERNET 权限 | `AndroidManifest.xml` | 添加 INTERNET 权限声明 |

**P2 修复：** setState 无 mounted 检查 (`search_page.dart`)

**新增依赖：** `path_provider: ^2.1.0`

**生成的 Dart API 原有：** 29+ 函数 (ping/init_legado + Books/Sources/Chapters CRUD + Progress/Bookmarks + Online Search/Content)

---

## 📱 Android 平台专项适配 — ✅ 完成（2026-05-03）

### 应用标识
| 项目 | 内容 |
|------|------|
| 包名 | `io.legado.app.flutter`（`build.gradle.kts` namespace + applicationId） |
| 应用名 | `Legado Reader`（AndroidManifest.xml label + main.dart title） |
| MainActivity | `android/app/src/main/kotlin/io/legado/app/flutter/MainActivity.kt` |

### 自适应图标
| 资源 | 文件 |
|------|------|
| 前景矢量图 | `res/drawable/ic_launcher_foreground.xml` — 书本矢量图（白页+浅蓝右页+灰色书脊+红色书签） |
| 背景矢量图 | `res/drawable/ic_launcher_background.xml` — 品牌蓝 #1976D2 |
| adaptive-icon 配置 | `res/mipmap-anydpi-v26/ic_launcher.xml` |
| legacy PNG（5种密度） | `res/mipmap-*/ic_launcher.png` — Python PIL 生成的品牌书本图标 |

### 启动页品牌化
| 资源 | 文件 |
|------|------|
| 品牌色定义 | `res/values/colors.xml` — brand_primary/brand_primary_dark/brand_primary_light/splash_background |
| 暗色覆盖 | `res/values-night/colors.xml` — splash_background #0D47A1 |
| 启动背景 | `res/drawable/launch_background.xml` / `res/drawable-v21/launch_background.xml` |
| Android 12+ splash | `res/values-v31/styles.xml` + `res/values-night-v31/styles.xml` |

### 通知基础
| 组件 | 说明 |
|------|------|
| `MainActivity.kt` | `legado_download` 通知渠道（IMPORTANCE_LOW），`onResume()` 中创建 |
| MethodChannel `legado/notifications` | `hasPermission()` / `requestPermission()` Dart ↔ Kotlin |
| `lib/core/notification_service.dart` | 封装 channel 调用，统一返回 `Future<bool>` |
| `AndroidManifest.xml` | `POST_NOTIFICATIONS` 权限已声明 |
| **P2 修复（第2轮审查）** | 已从 `main.dart` 移除冷启动 `requestPermission()`，改为业务触发点调用 |
| **P3 修复（第2轮审查）** | `MainActivity.kt` `requestPermission()` 已增加 pending guard，连续调用返回 `PERMISSION_REQUEST_PENDING` error |

### 权限
| 权限 | 用途 |
|------|------|
| `android.permission.INTERNET` | 网络访问 |
| `android.permission.POST_NOTIFICATIONS` | Android 13+ 通知权限（不在冷启动时请求） |

---

## 🔍 代码审查记录

| 轮次 | 日期 | 发现问题 | 状态 |
|------|------|---------|------|
| 第1轮 | 2026-05-02 | P1×4: createSource API / DB初始化竞态 / 相对路径 / INTERNET权限 + P2×1: mounted检查 | ✅ 全部修复 |
| 第2轮 | 2026-05-03 | P2: 冷启动通知权限请求 / P3: pendingResult 覆盖 | ✅ 全部修复 |
| 第3轮 | 2026-05-03 | 审查确认无新阻断问题 | ✅ 通过 |
| Rust 第1轮 | 2026-05-04 | P1×3: Proxy/Cookie日志脱敏, @Json:前缀剥离 + P2×4: Regex flags, Semaphore, EPUB3 cover, source_dao URL冲突 | ✅ 全部修复 |
| Rust 第2轮 | 2026-05-04 | P1×2: Set-Cookie value泄露, source_dao DELETE破坏书籍关联 + P2×2: Semaphore, EPUB3 cover multi-token | ✅ 全部修复 |
| Rust 第3轮 | 2026-05-04 | P2×1: source_dao 错误吞没（QueryReturnedNoRows区分） + P3×1: Set-Cookie name仍记录 | ✅ 全部修复 |
| Phase 1 深层审查 | 2026-05-04 | 🔴×4: 在线搜索schema/保存书籍/XPath误判/JSONPath bracket + 🟡×1: Legado导入格式 + 测试修复: regex delimiter检测 | ✅ 全部修复 |
| Rust 第4轮 | 2026-05-04 | 🟡×1: evaluate_regex 缺 m/s/x/u/U 标志（仅有i/g）+ 🟢×1: 缺失 ownText/XPath 评估测试 | ✅ 全部修复 |
| Phase 3 第1轮审查 | 2026-05-05 | [High]×2: mounted guard after await, per-source timeout + [Medium]×2: validation false warnings (CSS/XPath), JSONPath compilation | ✅ 全部修复 |
| Phase 3 第2轮审查 | 2026-05-05 | [Medium]×2: search URL relative path false warning, XPath empty branch + [Low]×1: mounted guard before ref.invalidate | ✅ 全部修复 |
| Phase 3 回归测试 | 2026-05-05 | 7 Rust tests (search URL/XPath/JSONPath/CSS validation) + 1 Flutter test (dispose during async search) | ✅ 全部通过（101 total） |
| Phase 3 第3轮审查 | 2026-05-05 | [Medium]×1: parser HTTP client 无超时 + [Low]×1: 手工 frb_generated 补丁需文档化 | ✅ 全部修复 |
| Phase 3 第4轮审查 | 2026-05-05 | [Low]×1: CURRENT_STATUS.md 验证计数过期（94/48 → 101/8） | ✅ 全部修复 |
| 核心链路打通 | 2026-05-05 | 🔴: 搜索→书架→阅读链路断点（Book 模型缺 book_url，保存时不拉取章节导致阅读器无章节） | ✅ 修复（Book 新增 book_url + DB migration v4 + book_dao 全量更新 + search_page 章节自动拉取） |
| 核心链路审查修复 | 2026-05-05 | Critical×1: 新库初始化重复 book_url；High×3: 搜索结果空 id、Reader dispose/乱序、书架路由未 encode | ✅ 修复（migration v4 幂等 + 空 DB 直接置 DB_VERSION + 稳定 bookId + URI query encode + Reader request token/mounted guard） |
| 核心链路第二轮审查修复 | 2026-05-05 | High×1: SourceDao::upsert() 返回实际写入 ID（URL 去重时 callers 拿错误 ID）；Medium×1: 搜索结果缺 source_id 时章节拉取必然失败；Low×1: Reader _openChapter() 未校验 index 边界 | ✅ 修复（upsert 返回 SqlResult\<String\> + create() 使用 effective_id + save_source 适配 + source_id 有效性检查 + Reader chapters 空列表/index 越界防御） |
| 核心链路第三轮审查修复 | 2026-05-05 | High×1: 在线搜索稳定 bookId（parser 随机 UUID 导致重复书）；Medium×1: 缺 source_id 时提示优化；Low×1: Reader bounds check 加 mounted guard | ✅ 修复（在线结果用 source_id+book_url 哈希作 ID + 离线结果信任 DB ID + source_id 缺失时提示"无有效书源" + Reader setState 前检查 mounted） |
| Legado 兼容层专项推进 | 2026-05-06 | QuickJS 默认 runtime、`java.*` bridge、真实书源合集导入、parser 接入 Legado HTTP/rule、共享 cookie/header/charset、warning cleanup | ✅ 专项回归通过（parser/js_runtime/legado import-rule-url/rule_engine + no-default/js-boa/api-server checks） |

---

## 🏗️ 项目结构

```
legado_flutter/
├── core/                  # Rust workspace
│   ├── core-net/         # HTTP网络引擎（reqwest + rustls）
│   ├── core-parser/      # 格式解析（TXT/EPUB/UMD）
│   ├── core-storage/     # SQLite存储引擎
│   ├── core-source/      # 书源规则引擎（核心）
│   └── bridge/           # flutter_rust_bridge层（真实位置在 core/bridge/）
├── flutter_app/          # Flutter UI层
└── docs/                 # 文档
```

---

## ⚙️ 环境

- **Rust**: 1.95.0 (`~/.cargo/bin/cargo`)
- **Cargo**: 1.95.0
- **flutter_rust_bridge_codegen**: 2.12.0
- **工作目录**: `/root/data/workspaces/doro_FriendMessage_641981595/legado_flutter/core`

---

## 📝 下一步 — Phase 3 功能整合 ✅ 完成（2026-05-05）

> **平台方向**：Android 优先，Linux 桌面暂停 ⏸️

### ✅ 已完成里程碑
真机 smoke test ✅ | Phase 2 UI ✅ | Phase 1 深层审查修复 ✅ | Rust 第4轮审查（RegexBuilder） ✅ | **Phase 3 功能整合 ✅**

### 🟢 最新验证（2026-05-05）

| 检查项 | 结果 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cargo test --workspace` | ✅ **101 passed**, 0 failed |
| `cargo clippy --workspace -- -D warnings` | ⚠️ 1 pre-existing warning |
| `flutter test` | ✅ **49 passed**, 0 failed |
| `flutter analyze` | ⏱️ 超时（环境约束，此前验证 clean）|

### ✅ Phase 3 功能整合 — 已完成

| 任务 | 优先级 | 状态 |
|------|--------|------|
| 多书源并发搜索 | 🔴 P1 | ✅ 完成 |
| 封面本地缓存机制 | 🔴 P1 | ✅ 完成 |
| 书源规则校验 | 🟡 P1 | ✅ 完成 |
| 书源导出功能 | 🟡 P1 | ✅ 完成 |
| Stream 推送（进度/日志实时推送） | 🟡 P1 | ⬜ 待开始 |

### Phase 3 实现详情

**多书源并发搜索** (`search_page.dart`):
- 使用 `getEnabledSources` 获取所有已启用书源
- `Future.wait` 并发搜索所有书源
- `_searchWithSource` 辅助函数隔离各书源错误
- 按 `name_author` 去重，结果标注来源书源名称

**封面本地缓存** (`bookshelf_page.dart`):
- 添加 `cached_network_image: ^3.4.0` 依赖
- `_buildCover` 使用 `CachedNetworkImage` 替代 `Image.network`
- 持久化磁盘缓存 + placeholder + errorWidget

**书源规则校验** (`core-source/src/lib.rs`, `api.rs`, `source_page.dart`):
- `ValidationIssue` 结构体 (field/severity/message)
- 全面校验: 搜索规则/详情规则/目录规则/内容规则 (CSS/XPath/Regex/JSONPath/JS)
- `validateSourceFromDb` API: 从DB加载书源并校验
- UI: 点击书源 → "校验规则" → 弹窗显示结果（error红/warning橙/info蓝）

**书源导出** (`api.rs`, `source_page.dart`):
- `exportAllSources` API: 导出所有书源为 JSON 数组
- UI: AppBar "导出" 按钮 → 复制到剪贴板

**修复**:
- `validate_rule_expressions` 中 `.is_ok()` → `.is_some()`（RuleExpression::parse 返回 Option）
- 恢复被误删的 `parse_book_source` 函数
- `_showSourceActions` 中合并重复的校验按钮为 `TextButton.icon`
- `parser.rs` HTTP client 添加 15s request + 15s connect timeout（配合 Dart `.timeout()` 兜底）
- `frb_generated.dart` / `frb_generated.rs` 手工添加 funcId 42-44（codegen 超时，详见 FRB 补丁章节）

### 🔮 中期方向 — 下一步行动计划 (2026-05-06)

**B. Phase 4 高级服务**（核心功能）
- 替换规则桥接 API + 管理 UI ✅ 完成
- 下载管理后台服务 + 队列 + 启动恢复 ✅ 完成
- 搜索→书架→阅读核心链路打通 + 审查修复 ✅ 完成
- 替换规则在阅读器中应用 ✅ 完成
- TTS 语音朗读、WebDAV 同步备份/恢复 ⬜ 待开发

**C. Phase 1 深层引擎完善**（技术债务）
- Legado WebView bridge（`webView:true` / `webJs` / `sourceRegex`）⬜ 待开发
- 完整 DOM/Element 对象语义（当前 `java.getElements` 主要返回字符串数组）⬜ 待开发
- 更多真实站点端到端搜索/目录/正文验证 ⬜ 待扩展
- UMD 解析器重写、EPUB metadata 完善 ⬜ 待开始

**D. Phase 4.5: API Server + HTTP Client (前后端分离) — 下一步**

| 优先级 | 任务 | 说明 | 涉及文件 |
|--------|------|------|----------|
| 🔴 高 | SearchPage 在线搜索切 HTTP | `_doSearch()` 中替换 rust_api 调用为 `searchApiProvider` → `POST /api/search` | `search_page.dart`, `providers.dart` |
| 🔴 高 | BookshelfPage 添加切 HTTP | `_saveResultToBookshelf()` 替换为 `bookshelfApiProvider.addBook()` → `POST /api/bookshelf` | `search_page.dart` (保存逻辑在搜索页) |
| 🔴 高 | ReaderPage 章节内容切 HTTP | `_openChapter()` 替换为 `readerApiProvider.getChapterContent()` → `GET /.../content?chapter_index=N` | `reader_page.dart` |
| 🟡 中 | 补充 HTTP 模式缺失路由 | 阅读进度 (GET/PUT `/progress`)、下载管理、替换规则 | `api-server/src/routes/` |
| 🟢 低 | 端到端联调测试 | 启动 `api-server` (0.0.0.0:3000)，`backendMode` 切 HTTP，验证完整链路 | 手动测试 |
| 🟢 低 | 移除 FRB 依赖（可选） | HTTP 模式稳定后可精简包体积 | `Cargo.toml`, `pubspec.yaml` |

> ⏱️ `flutter build apk --debug` 当前环境超时（20分钟），无法完成 APK 编译。此前验证：`flutter analyze` ✅ clean、`flutter test` ✅ 8/8 passed、`cargo check/test/clippy` ✅ 全通过。

### 构建/验证命令

```bash
# Rust
cd core && cargo check --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings

# Flutter
cd flutter_app && flutter --no-version-check analyze && xvfb-run flutter --no-version-check test

# Android APK 构建
cd flutter_app && flutter build apk --debug

# ADB（设备已连接 d34e43d9）
/opt/android-sdk/platform-tools/adb -s d34e43d9 install -r build/app/outputs/flutter-apk/app-debug.apk
/opt/android-sdk/platform-tools/adb -s d34e43d9 logcat -c && /opt/android-sdk/platform-tools/adb -s d34e43d9 logcat -s flutter,AndroidRuntime,MainActivity,FRB
```

### 新增 Dart API 一览 (flutter_app/lib/src/rust/api.dart)

| 类别 | 函数 | 说明 |
|------|------|------|
| 核心 | `ping()`, `initLegado()`, `getDbVersion()` | 原有 3 个 |
| 书架 | `getAllBooks()`, `searchBooksOffline()`, `saveBook()`, `deleteBook()` | 返回/接收 JSON |
| 书源 | `getAllSources()`, `getEnabledSources()`, `saveSource()`, `deleteSource()`, `setSourceEnabled()`, `importSourcesFromJson()` | 返回/接收 JSON |
| 章节 | `getBookChapters()`, `updateChapterContent()`, `saveChapter()`, `deleteChapter()` | 返回/接收 JSON |
| 进度 | `saveReadingProgress()`, `getReadingProgress()` | 基本类型 + JSON |
| 书签 | `getBookmarks()`, `addBookmark()`, `deleteBookmark()` | 返回/接收 JSON |
| 在线搜索 | `searchBooksOnline()`, `getBookInfoOnline()`, `getChapterListOnline()`, `getChapterContentOnline()` | 异步, JSON |
| 便捷 | `searchWithSourceFromDb()`, `getChapterContentWithSourceFromDb()` | 异步, DB+在线组合 |
