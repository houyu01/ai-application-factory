# 客户端控制的短剧游戏平台
兼容国内外各大云厂商的模型，可以混用(seedream的图片生成、wanxiang的视频生成)
<img width="3014" height="1700" alt="image" src="https://github.com/user-attachments/assets/23ac1272-96f7-4972-9c93-90a7e9c5aefe" />

## Android 平板 APK

首次在开发机上执行 `npm run init:android`，它会生成 Tauri 的 Android 工程并安装所需 Rust target。然后复制签名配置：

```bash
cp .env.android.example .env.android
```

在 `.env.android` 中填入本地 Android keystore 的绝对路径、别名和密码。首次可用 JDK 的 `keytool -genkeypair -v -keystore /安全路径/ai-application-factory-upload.jks -alias ai-application-factory -keyalg RSA -keysize 2048 -validity 10000` 创建密钥库；密钥和 `.env.android` 不应提交到 Git。

```bash
npm run build:android:pad
```

该命令构建 arm64 与 armv7 的通用、已签名 release APK，适用于主流实体 Android 平板。最终安装包始终归档为 `dist/ai-application-factory-<version>-android-pad.apk`。

## 轻松制作短剧
超短脚本扩写，并自动分镜

```
一个小说，主要是关于男主角从一个山村小伙子，成长为一代仙门大侠的故事，小时候男主角被灭满门，被青云山的道人收养，男主角在仙门内一路修炼成长，在门内小有所成之后，与其他仙门共同剿灭魔道，结识了魔道女少主，并与魔道女少主相爱，为正道所不容，但是最后通过与正道摩擦，发现了隐藏在正道内部的一个秘密，原来大boss就是正道魁首，男主与正道魁首多番较量，最终揭露他的真面目，最后男主带着魔道少主一起归隐山林

自动拆分短剧需要的素材
<img width="1512" height="1668" alt="image" src="https://github.com/user-attachments/assets/ecdda242-8d7a-4a5c-9469-1c7798e8db27" />

一句话生成并扩写10w字以上的剧本，剧本分镜全自动

<img width="2894" height="1446" alt="image" src="https://github.com/user-attachments/assets/6b583650-6d26-4265-82c9-8add4ace7082" />
素材自动生成
