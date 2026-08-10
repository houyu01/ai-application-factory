# AI Application Factory
<img width="3022" height="1468" alt="image" src="https://github.com/user-attachments/assets/d2c989d2-c4d8-49ac-956a-47ca2866fee6" />


一个短剧和互动影视游戏创作的平台，一个兼容了阿里云、腾讯云、火山引擎多个引擎混用的创作平台底座，支持web/桌面端/ipad版

## 当前迭代版本

本项目当前以 **Tauri 桌面端** 为唯一持续迭代的产品实现：
`frontend/` 负责 TypeScript 界面，`src-tauri/` 负责 Rust 本地服务、数据存储、任务与模型提供商调用。

后续的功能开发、界面调整、问题修复和 AI 生成代码均在这套 Tauri 实现中完成。应用由 Rust 管理本地 SQLite、媒体和生成任务；迁移完成范围见 [Rust 迁移审计清单](docs/rust-migration-audit.md)。

<img width="2896" height="1452" alt="image" src="https://github.com/user-attachments/assets/ff38ffe8-cfe1-49fd-915f-c7787c12788d" />

一句话生成并扩写10w字以上的剧本，剧本分镜全自动

<img width="2894" height="1446" alt="image" src="https://github.com/user-attachments/assets/6b583650-6d26-4265-82c9-8add4ace7082" />
素材自动生成
