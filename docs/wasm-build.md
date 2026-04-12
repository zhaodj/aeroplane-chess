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

```bash
./scripts/serve-wasm.sh
```

默认地址：`http://127.0.0.1:8000`

可自定义端口：

```bash
./scripts/serve-wasm.sh 9000
```
