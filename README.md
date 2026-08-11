# AI Application Factory
<img width="3022" height="1468" alt="image" src="https://github.com/user-attachments/assets/d2c989d2-c4d8-49ac-956a-47ca2866fee6" />


一个短剧和互动影视游戏创作的平台，一个兼容了阿里云、腾讯云、火山引擎多个引擎混用的创作平台底座，支持web/桌面端/ipad版

## 当前迭代版本

本项目当前以 **Tauri 桌面端** 为唯一持续迭代的产品实现：
`frontend/` 负责 TypeScript 界面，`src-tauri/` 负责 Rust 本地服务、数据存储、任务与模型提供商调用。

后续的功能开发、界面调整、问题修复和 AI 生成代码均在这套 Tauri 实现中完成。应用由 Rust 管理本地 SQLite、媒体和生成任务；迁移完成范围见 [Rust 迁移审计清单](docs/rust-migration-audit.md)。

## iPad / App Store 打包

工程已经初始化为通用 iOS 应用（同时支持 iPhone 与 iPad，iPad 使用 `arm64` 真机包）。首次打包只需配置本机的 Apple 签名信息：

```bash
cp .env.ios.example .env.ios
```

在 `.env.ios` 中填写 `APPLE_DEVELOPMENT_TEAM`，值为 Apple Developer 网站 Membership 页面中的 10 位 Team ID。不要提交这个文件；它已被 Git 忽略。

然后执行唯一的 App Store IPA 打包命令：

```bash
npm run build:ipad
```

该命令运行 `tauri ios build --export-method app-store-connect`，产物位于 `src-tauri/gen/apple/build/arm64/AI Application Factory.ipa`。当前 Bundle ID 是 `com.aiapplicationfactory.desktop`，在 Apple Developer 和 App Store Connect 创建 App ID / App 时必须完全一致；如需改为自己的反向域名，请先改 `src-tauri/tauri.conf.json` 的 `identifier`，再执行 `npx tauri ios init --ci --skip-targets-install` 重新生成 Xcode 工程。

本机推荐使用 Xcode 自动签名：在 Xcode 的 Settings > Accounts 登录已加入 Apple Developer Program 的账户。Xcode 会管理 Apple Distribution 证书和 App Store Connect provisioning profile。自动签名用于 CI 时，在 `.env.ios` 中另填 `APPLE_API_ISSUER`、`APPLE_API_KEY` 和本机受保护的 `APPLE_API_KEY_PATH`；手动/CI 签名则填写 `IOS_CERTIFICATE`、`IOS_CERTIFICATE_PASSWORD`、`IOS_MOBILE_PROVISION` 三项 Base64 值。完整字段说明见 `.env.ios.example`。

每次向 App Store Connect 上传新构建前，把 `src-tauri/tauri.ios.conf.json` 的 `bundle.iOS.bundleVersion` 递增为未上传过的整数；用户可见版本仍在 `src-tauri/tauri.conf.json` 的 `version` 中管理。

<img width="2896" height="1452" alt="image" src="https://github.com/user-attachments/assets/ff38ffe8-cfe1-49fd-915f-c7787c12788d" />

一句话生成并扩写10w字以上的剧本，剧本分镜全自动

<img width="2894" height="1446" alt="image" src="https://github.com/user-attachments/assets/6b583650-6d26-4265-82c9-8add4ace7082" />
素材自动生成
