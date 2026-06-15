# Android 平板构建

本项目的 Android 入口在 `platforms/android/`，Rust 游戏本体仍在 `src/`。当前路线跟随 Bevy 0.18.1 官方示例：使用 `cargo-ndk` 编译 Rust `cdylib`，再由 Gradle + GameActivity 打包 APK。

## 前置

1. 安装 Android Studio，并确保 `ANDROID_SDK_ROOT` 指向 SDK 目录。
2. 安装 NDK Side by side，并设置 `ANDROID_NDK_ROOT` 指向实际 NDK 目录。
3. 安装构建工具：

```bash
cargo install cargo-ndk
```

如果 `platforms/android/` 下没有 Gradle wrapper，需要系统里有 `gradle` 命令，或用 Android Studio 打开该目录构建。

## 构建

```bash
./scripts/build-android.sh
```

Release 包：

```bash
./scripts/build-android.sh release
```

输出目录：

```text
platforms/android/app/build/outputs/apk/
```

## 设备目标

- 默认 ABI：`arm64-v8a`
- 默认横屏：`android:screenOrientation="landscape"`
- `minSdk 31`：匹配 Bevy 默认 GameActivity 路线
- 支持 large/xlarge screen，面向安卓平板优先

## 未来平台扩展

- iPad/iOS：继续复用 `src/lib.rs` 的库入口，在 `platforms/ios` 增加 Xcode 工程。
- macOS/Windows：保留 `src/main.rs` 桌面入口，按需在 `platforms/desktop` 增加分发脚本或安装包配置。
- Web：继续使用 `scripts/build-wasm.sh` 和 `web/`。
