# Agent Harness

本仓库是一个 Rust/Bevy 飞行棋项目，当前主线是单机 2D 游戏，已经包含桌面、WebAssembly 和 Android 平板构建入口。Agent 接手任务时应优先遵循本文件，并把 `docs/` 中的设计文档作为背景资料。

## Project Shape

- 语言与版本：Rust 2024 edition。
- 游戏引擎：Bevy `0.19.0`，实际依赖以 [Cargo.toml](/Users/zhaodaojun/Documents/source/aeroplane-chess/Cargo.toml) 为准。
- 核心库：`src/lib.rs` 输出 `lib` 和 `cdylib`，供桌面、WASM、Android 复用。
- Web 入口：`web/` + `scripts/build-wasm.sh`，构建产物在 `dist/`。
- Android 入口：`platforms/android/` + `scripts/build-android.sh`。
- 静态资源：`assets/`，音频资源已提交，不要在常规代码任务中重生成。

## Architecture Map

- `src/domain/`：纯规则层，优先保持可单测，不引入渲染/UI 依赖。
- `src/gameplay/`：回合、行动解析、AI、技能流程等应用层逻辑。
- `src/plugins/`：Bevy plugin 与 ECS 表现层，连接规则、输入、动画、音频、UI。
- `src/ui/`：菜单、HUD、结算等 UI 入口。
- `src/platform/`：平台能力与输入适配。
- `src/data/`：棋盘、技能和模式配置。
- `docs/`：规则、技术方案、WASM/Android 构建、试玩记录和视觉参考。

## Edit Rules

- 先检查 `git status --short`。如果存在用户未提交改动，只在确实需要的文件上工作，不回滚用户改动。
- 规则变更优先落在 `domain` 或 `gameplay`，表现变更优先落在 `plugins`/`ui`。
- 不把 `target/`、`dist/`、`platforms/android/app/build/`、`platforms/android/app/src/main/jniLibs/` 作为源文件修改。
- 需要改资源生成逻辑时，优先改 `scripts/`，再显式重生成对应资源。
- 文档中旧版本信息可能滞后，依赖版本以 `Cargo.toml` 和 lockfile 为准。

## Verification

常规任务优先使用统一入口：

```bash
./scripts/harness-check.sh quick
```

推荐校验层级：

- `quick`：格式检查 + Rust lib 单测，适合小改动。
- `standard`：格式检查 + native check + clippy + lib 单测，适合提交前。
- `wasm`：WASM target check + dev 打包，适合 Web 相关改动。
- `android`：Android debug APK 构建，适合 Android 平台改动。
- `all`：standard + wasm + android，适合发布前或大改动。

如果缺少平台依赖（如 `wasm-bindgen`、`wasm-opt`、Android SDK/NDK、`cargo-ndk`），记录缺失项，不要用无关改动绕过。

## Useful Commands

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets
cargo test --lib
./scripts/build-wasm.sh dev
./scripts/build-wasm.sh release
./scripts/serve-wasm.sh 8000
./scripts/build-android.sh debug
```

