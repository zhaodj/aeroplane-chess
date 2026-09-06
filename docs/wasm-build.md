# WASM 打包与本地预览

## 前置

1. 安装 `wasm-bindgen` CLI（只需一次）：

```bash
cargo install wasm-bindgen-cli
```

2. 已安装 Rust 工具链（脚本会自动补 `wasm32-unknown-unknown` 目标）。

## 打包

```bash
./scripts/build-wasm.sh
```

输出目录：`dist/`

## 本地预览

画布显示尺寸由 CSS 和安全区 `env(safe-area-inset-*)` 控制，物理像素缓冲区由 Bevy/winit 管理，页面不再重复写入 `canvas.width/height`。触控设备通过 `maxTouchPoints` / `any-pointer: coarse` 识别；PWA 支持横竖屏旋转。不要把设备物理分辨率直接作为 UI 逻辑尺寸。

响应式布局与交互检查见 [自适应 UI 验收说明](ui-adaptive-acceptance.md)。

```bash
./scripts/serve-wasm.sh
```

默认地址：`http://127.0.0.1:8000`

可自定义端口：

```bash
./scripts/serve-wasm.sh 9000
```
