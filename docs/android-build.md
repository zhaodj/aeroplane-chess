# Android 平板构建

本项目的 Android 入口在 `platforms/android/`，Rust 游戏本体仍在 `src/`。当前路线跟随 Bevy 0.18.1 官方示例：使用 `cargo-ndk` 编译 Rust `cdylib`，再由 Gradle + GameActivity 打包 APK。

## 前置

本机已安装并配置：

- Android SDK：`~/Library/Android/sdk`
- SDK Platform：`platforms;android-34`
- Build Tools：`build-tools;34.0.0`
- NDK：`ndk;26.1.10909125`
- CMake：`cmake;3.22.1`
- Platform Tools：`platform-tools`
- Emulator：`emulator`
- AVD：`aeroplane_tablet_api34`
- System Image：`system-images;android-34;google_apis;arm64-v8a`
- GameActivity：`androidx.games:games-activity:3.0.5`
- JDK：Homebrew `openjdk@17`
- Gradle：Homebrew `gradle@8`
- Rust Android target：`aarch64-linux-android`
- Rust 构建工具：`cargo-ndk`

新机器需要安装 Android Studio 或 Android command-line tools，并确保 `ANDROID_SDK_ROOT`、`ANDROID_NDK_ROOT` 指向实际目录。还需要安装构建工具：

```bash
cargo install cargo-ndk
```

如果 `platforms/android/` 下没有 Gradle wrapper，需要系统里有 `gradle` 命令，或安装 Homebrew `gradle@8`。当前脚本会自动识别 `/opt/homebrew/opt/gradle@8/bin/gradle`。

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

## 模拟器测试

启动本机 Pixel Tablet AVD：

```bash
emulator -avd aeroplane_tablet_api34
```

安装 debug APK：

```bash
adb install -r platforms/android/app/build/outputs/apk/debug/app-debug.apk
```

查看已连接设备：

```bash
adb devices
```

## 设备目标

- 默认 ABI：`arm64-v8a`
- 自适应横竖屏：`android:screenOrientation="fullSensor"`，Activity 同步使用 `SCREEN_ORIENTATION_FULL_SENSOR`；旋转时重排棋盘、HUD 和菜单，不重开对局
- 保留沉浸模式，但不将内容布局到系统导航栏下；刘海屏使用 `LAYOUT_IN_DISPLAY_CUTOUT_MODE_NEVER`，布局尺寸由安全窗口提供
- `minSdk 31`：匹配 Bevy 默认 GameActivity 路线
- 支持 large/xlarge screen，面向安卓平板优先

## 未来平台扩展

- iPad/iOS：继续复用 `src/lib.rs` 的库入口，在 `platforms/ios` 增加 Xcode 工程。
- macOS/Windows：保留 `src/main.rs` 桌面入口，按需在 `platforms/desktop` 增加分发脚本或安装包配置。
- Web：继续使用 `scripts/build-wasm.sh` 和 `web/`。
