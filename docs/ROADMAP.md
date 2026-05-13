# Legado Flutter 重构路线图

> ⚠️ **此为原始计划文档**（v0.1, 2026-04-30）。代码已继续演进，阶段状态可能与原计划有偏差。最新状态以 `CURRENT_STATUS.md` 为准。

> 基于开源阅读应用 Legado 的 Flutter + Rust 重构项目
> 版本: v0.1 | 日期: 2026-04-30

---

## 项目总览

从零开始重构 Legado Android 应用，使用 Flutter + Rust 技术栈实现跨平台支持，保持与原版功能对等的同时提升性能和可维护性。

> **🚀 当前平台优先级**：**Android 优先**（APK 已编译，FRB ping/pong 通过） | **⏸️ Linux 桌面方向暂停**

---

## Phase 0: 基础设施搭建 [进行中 — Android 优先] 1-2周

### 目标
建立项目基础结构，配置开发环境，打通 Flutter-Rust 桥接链路。当前优先验证 Android 平台。

### 技术任务

#### 0.1 Rust Workspace 配置
- [x] 创建 core/Cargo.toml workspace 配置
- [x] 创建 core-net/Cargo.toml（网络引擎）
- [x] 创建 core-parser/Cargo.toml（格式解析）
- [x] 创建 core-storage/Cargo.toml（存储引擎）
- [x] 创建 core-source/Cargo.toml（书源引擎）
- [x] 验证 workspace 依赖解析（`cargo check` 通过）

#### 0.2 Flutter 项目配置
- [x] 创建 flutter_app/pubspec.yaml
- [x] 配置依赖：flutter_riverpod, flutter_rust_bridge, freezed
- [x] 运行 flutter pub get（`flutter analyze` 通过）

#### 0.3 Bridge 层搭建
- [x] 创建 bridge/src/lib.rs
- [x] 配置 flutter_rust_bridge_codegen（v2.12.0）
- [x] 生成初始绑定代码（`ping`, `init_legado`, `get_db_version`）
- [x] Android APK 编译通过，FRB `ping()` → `pong` smoke 通过
- [ ] iOS native 动态库集成（待启）

#### 0.4 文档编写
- [x] docs/ARCHITECTURE.md（架构设计）
- [x] docs/ROADMAP.md（本文件）
- [x] README.md（项目说明）

### 交付物
- 完整的项目目录结构
- 所有 Cargo.toml 和 pubspec.yaml 配置
- 基础桥接层代码框架（Android 已验证）
- 完整的项目文档

### 暂停项 ⏸️
- Linux desktop bundle `libbridge.so` 正式打包（CMake/native assets）

---

## Phase 1: Rust 核心引擎 3-4周

### 目标
实现四个核心 Rust crate 的基础功能，使其可以独立运行和测试。

### 技术任务

#### 1.1 core-net 网络引擎
- [ ] 实现 HttpClient 封装（基于 reqwest）
- [ ] Cookie 持久化管理（使用 cookie_store）
- [ ] 并发请求控制（Semaphore 限制）
- [ ] 代理支持（HTTP/HTTPS/SOCKS5）
- [ ] 请求重试和超时机制
- [ ] 对应原 Legado: `help/http/`, `lib/cronet/`

#### 1.2 core-parser 格式解析
- [ ] TXT 文件解析（编码自动检测）
- [ ] EPUB 格式解析（基于 quick-xml）
- [ ] 章节分割和内容清洗
- [ ] 支持正则替换规则
- [ ] 对应原 Legado: `modules/book/`, `help/book/`

#### 1.3 core-storage 存储引擎
- [ ] SQLite 数据库初始化
- [ ] 书架数据表设计（books, chapters, progress）
- [ ] 书源数据表设计（sources, replace_rules）
- [ ] DAO 层实现（CRUD 操作）
- [ ] 数据库迁移机制
- [ ] 对应原 Legado: `data/entities/`, `help/storage/`

#### 1.4 core-source 书源引擎（核心）
- [ ] 书源规则 JSON 解析
- [ ] RuleEngine 实现（XPath/CSS/Regex/JSONPath）
- [ ] HTML 解析（基于 scraper）
- [ ] Rhai 脚本引擎集成
- [ ] 搜索规则执行
- [ ] 图书信息提取规则
- [ ] 章节列表解析规则
- [ ] 章节内容解析规则
- [ ] 对应原 Legado: `model/webBook/`, `model/analyzeRule/`

### 交付物
- 可独立测试的 Rust 核心库
- 单元测试覆盖核心功能
- 书源规则引擎最小可用版本

---

## Phase 2: Flutter UI 框架 2-3周

### 目标
搭建 Flutter 应用基础框架，实现主要页面结构。

### 技术任务

#### 2.1 应用架构
- [ ] Riverpod Provider 结构设计
- [ ] 路由管理（go_router）
- [ ] 主题系统（Material3 + 动态配色）
- [ ] 国际化支持基础

#### 2.2 书架页面 (features/bookshelf)
- [ ] 书架网格/列表视图
- [ ] 分组管理 UI
- [ ] 书籍封面加载
- [ ] 搜索过滤功能
- [ ] 对应原 Legado: `ui/book/`, `ui/main/`

#### 2.3 搜索页面 (features/search)
- [ ] 搜索框和发现页
- [ ] 多书源搜索结果展示
- [ ] 搜索历史记录
- [ ] 对应原 Legado: `ui/book/search/`

#### 2.4 书源管理 (features/source)
- [ ] 书源列表展示
- [ ] 书源导入/编辑 UI
- [ ] 启用/禁用切换
- [ ] 书源校验功能
- [ ] 对应原 Legado: `ui/book/source/`

#### 2.5 设置页面 (features/settings)
- [ ] 主题切换
- [ ] 字体设置
- [ ] 阅读偏好配置
- [ ] 对应原 Legado: `ui/config/`, `ui/about/`

### 交付物
- 可运行的 Flutter 应用框架
- 主要页面 UI 实现（可能无完整功能）
- 状态管理架构搭建完成

---

## Phase 3: 功能整合与桥接 2-3周

### 目标
将 Flutter UI 与 Rust 核心引擎通过 flutter_rust_bridge 连接起来。

### 技术任务

#### 3.1 FFI 桥接完善
- [ ] 定义完整的 Dart-Rust 接口
- [ ] 实现异步调用（搜索、获取章节等）
- [ ] 实现 Stream 推送（进度、日志）
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
- [ ] 书源导入（URL/文件）
- [ ] 书源规则校验
- [ ] 书源启用/禁用
- [ ] 书源导出功能

### 交付物
- 可完整搜索书籍的应用
- 书架基本功能可用
- 书源管理功能可用

---

## Phase 4: 高级服务移植 2-3周

### 目标
移植原 Legado 的高级功能，提升应用完整性和用户体验。

### 技术任务

#### 4.1 阅读器核心 (features/reader)
- [ ] 翻页引擎（仿真/覆盖/滑动）
- [ ] 排版引擎（字体/行距/段距）
- [ ] 主题切换（白天/夜晚/护眼）
- [ ] 进度保存和恢复
- [ ] 书签功能
- [ ] TTS 语音朗读（Rust 后端 + Flutter 前端）
- [ ] 对应原 Legado: `ui/book/read/`, `service/`

#### 4.2 内容处理
- [ ] 章节内容解析（通过 core-source）
- [ ] 替换规则应用
- [ ] 图片加载和缓存
- [ ] 广告过滤规则

#### 4.3 后台服务
- [ ] 定时更新书架（后台任务）
- [ ] 下载管理（整本/章节）
- [ ] 推送通知（更新提醒）
- [ ] 对应原 Legado: `service/`

#### 4.4 同步服务
- [ ] WebDAV 同步（书源/进度/书架）
- [ ] 本地备份/恢复
- [ ] 对应原 Legado: `help/sync/`

### 交付物
- 完整可用的阅读器
- TTS 功能可用
- 后台服务运行正常
- 同步功能可用

---

## Phase 5: 平台适配与发布 1-2周

### 目标
完成各平台适配，打包发布。

### 技术任务

#### 5.1 Android 适配 🚀 **当前主攻平台**
- [x] APK 编译通过，FRB `ping()` → `pong` smoke 通过
- [x] 权限处理（INTERNET + POST_NOTIFICATIONS）
- [x] 通知渠道配置（`legado_download`，IMPORTANCE_LOW）
- [x] 应用图标（adaptive icon + legacy PNG 品牌书本图标）
- [x] 启动页品牌化（含 Android 12+ splash screen）
- [x] 通知权限 MethodChannel（`legado/notifications`，含 pending guard）
- [x] 包名/应用名修改（`io.legado.app.flutter` / `Legado Reader`）
- [x] NDK 28.2.13676358 配置
- [x] 3 轮代码审查修复全部通过
- [ ] 🔴 **真机 smoke test**（阻塞：Xiaomi 设备需手动开启"通过 USB 安装"）

#### 5.2 iOS 适配
- [ ] 签名和证书配置
- [ ] 后台模式配置
- [ ] 应用图标和启动屏
- [ ] App Store 合规检查

#### 5.3 桌面平台（可选）
- [ ] Windows 适配
- [ ] macOS 适配
- [ ] ~~Linux 适配~~ ⏸️ 暂停

#### 5.4 测试与优化
- [ ] 单元测试补充
- [ ] 集成测试
- [ ] 性能优化（内存/启动速度）
- [ ] 崩溃上报集成

#### 5.5 发布准备
- [ ] 编写应用介绍
- [ ] 截图准备
- [ ] 隐私政策
- [ ] 开源协议确认

### 交付物
- Android APK/AAB 包（已可编译 Debug APK）
- iOS IPA 包（待启动）
- 桌面平台安装包（⏸️ 暂停）
- 完整的测试报告

---

## 时间总览

| 阶段 | 名称 | 预计时间 | 依赖 |
|------|------|---------|------|
| Phase 0 | 基础设施搭建 | 1-2周 | 无 |
| Phase 1 | Rust 核心引擎 | 3-4周 | Phase 0 |
| Phase 2 | Flutter UI 框架 | 2-3周 | Phase 0 |
| Phase 3 | 功能整合与桥接 | 2-3周 | Phase 1 + Phase 2 |
| Phase 4 | 高级服务移植 | 2-3周 | Phase 3 |
| Phase 5 | 平台适配与发布 | 1-2周 | Phase 4 |
| **总计** | | **11-17周** | |

---

## 风险与应对

### 技术风险
1. **flutter_rust_bridge 稳定性** - 使用稳定版本，及时跟进更新
2. **Rhai 脚本引擎功能限制** - 评估是否满足书源脚本需求，必要时切换 quickjs-rs
3. **性能瓶颈** - 使用 Flutter 性能工具 + Rust profiling 定位问题

### 进度风险
1. **Phase 1 复杂度超预期** - 优先实现核心搜索流程，其他功能迭代完善
2. **桥接层调试困难** - 编写详细的日志和错误提示，建立调试指南

---

## 后续规划（Phase 6+）

- 插件系统（用户自定义功能扩展）
- 社区书源分享平台
- AI 辅助书源生成
- 多设备同步（端到端加密）
