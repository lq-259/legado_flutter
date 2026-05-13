# Legado Flutter

> 基于开源阅读应用 [Legado](https://github.com/gedoor/legado) 的 Flutter + Rust 重构版本

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Flutter Version](https://img.shields.io/badge/Flutter-3.0+-blue.svg)](https://flutter.dev)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org)

---

## 📖 项目简介

Legado Flutter 是对原 Android 应用 Legado（开源阅读）的完全重构，采用 **Flutter + Rust** 技术栈实现跨平台支持。

### 为什么重构？

- **跨平台**：原版仅支持 Android，重构后支持 Android、iOS、Windows、macOS、Linux
- **性能提升**：Rust 核心引擎提供更高性能和更好内存安全性
- **可维护性**：清晰的模块划分和现代化的技术栈
- **功能对等**：保持与原版功能对等，同时扩展新特性

### 技术架构

```
┌─────────────────────────────────────────┐
│           Flutter UI Layer               │
│    (书架/阅读器/搜索/书源管理/设置)        │
└──────────────────┬──────────────────────┘
                   │
         flutter_rust_bridge (FFI)
                   │
┌──────────────────▼──────────────────────┐
│          Rust Core Engine                │
│                                          │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐ │
│  │ core-net │ │core-     │ │core-    │ │
│  │ 网络引擎  │ │parser    │ │storage  │ │
│  │          │ │ 格式解析  │ │ SQLite  │ │
│  └──────────┘ └──────────┘ └─────────┘ │
│                │                        │
│         ┌──────▼──────┐                 │
│         │ core-source │                 │
│         │ 书源规则引擎 │                 │
│         └─────────────┘                 │
└──────────────────────────────────────────┘
```

详见 [架构设计文档](docs/ARCHITECTURE.md)

---

## 🚀 快速开始

### 前置要求

- **Flutter**: >=3.0.0 ([安装指南](https://docs.flutter.dev/get-started/install))
- **Rust**: edition 2024 ([安装指南](https://www.rust-lang.org/tools/install))
- **flutter_rust_bridge_codegen**: ^2.0.0
  ```bash
  cargo install flutter_rust_bridge_codegen
  ```

### 构建步骤

#### 1. 克隆项目
```bash
git clone https://github.com/yourusername/legado_flutter.git
cd legado_flutter
```

#### 2. 初始化 Rust 核心
```bash
cd core
cargo build --release
```

#### 3. 生成 FFI 绑定
```bash
cd bridge
flutter_rust_bridge_codegen generate
```

#### 4. 安装 Flutter 依赖
```bash
cd flutter_app
flutter pub get
```

#### 5. 运行应用
```bash
# Android
flutter run

# iOS (需要 macOS)
flutter run -d ios

# 桌面平台
flutter run -d windows  # 或 macos / linux
```

---

## 📂 项目结构

```
legado_flutter/
├── core/                   # Rust 核心引擎 (workspace)
│   ├── core-net/          # 网络引擎 (HTTP/Cookie/代理)
│   ├── core-parser/       # 格式解析 (TXT/EPUB/UMD)
│   ├── core-storage/      # 存储引擎 (SQLite)
│   └── core-source/       # 书源规则引擎 (核心)
├── flutter_app/           # Flutter 应用
│   ├── lib/
│   │   ├── app/          # 应用配置
│   │   ├── features/     # 功能模块 (书架/阅读器/搜索等)
│   │   ├── models/       # 数据模型
│   │   ├── services/     # 服务层 (状态管理)
│   │   └── widgets/      # 通用组件
│   └── pubspec.yaml
├── bridge/                # Flutter-Rust 桥接层
│   └── src/lib.rs
├── docs/                  # 项目文档
│   ├── ARCHITECTURE.md   # 架构设计
│   └── ROADMAP.md        # 开发路线图
└── README.md
```

---

## 🎯 开发路线图

| 阶段 | 状态 | 描述 |
|------|------|------|
| Phase 0 | ✅ 进行中 | 基础设施搭建 |
| Phase 1 | 📋 待开始 | Rust 核心引擎 |
| Phase 2 | 📋 待开始 | Flutter UI 框架 |
| Phase 3 | 📋 待开始 | 功能整合与桥接 |
| Phase 4 | 📋 待开始 | 高级服务移植 |
| Phase 5 | 📋 待开始 | 平台适配与发布 |

详细路线图见 [ROADMAP.md](docs/ROADMAP.md)

---

## 🤝 贡献指南

欢迎贡献！请参考以下流程：

1. Fork 本项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 提交 Pull Request

### 贡献方向

- 🐛 报告 Bug
- 💡 提出新功能建议
- 📝 完善文档
- 💻 提交代码（Rust/Flutter/文档）
- 🧪 编写测试用例

---

## 📄 开源协议

本项目采用 MIT 协议 - 详见 [LICENSE](LICENSE)

原 Legado 项目采用 AGPL-3.0，本重构项目采用 MIT 以便更广泛的使用。

---

## 🙏 致谢

- [Legado](https://github.com/gedoor/legado) - 原项目作者 gedoor 及贡献者
- [Flutter](https://flutter.dev) - UI 框架
- [Rust](https://www.rust-lang.org) - 核心引擎语言
- [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) - FFI 桥接方案

---

## 📧 联系方式

- 项目 Issues: [GitHub Issues](https://github.com/yourusername/legado_flutter/issues)
- 讨论区: [GitHub Discussions](https://github.com/yourusername/legado_flutter/discussions)

---

**注意**: 本项目与原 Legado 无官方关联，是由社区驱动的重构项目。
