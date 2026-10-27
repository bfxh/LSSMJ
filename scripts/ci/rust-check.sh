#!/usr/bin/env bash
# 阶段 1 Rust 基础门（v3 §6.1 命令集，v5 实测顺序）。
# 单一事实源：本地复跑与 CI 调用同一脚本，避免命令口径漂移。
# 注意：--all-targets --no-run 只证明 GPU 测试可编译，不证明 WGSL 运行时正确；
# 本脚本不执行任何 GPU 测试，无 GPU 环境也不得出现"跳过即绿"。
set -euo pipefail

MANIFEST="crates/conv-core/Cargo.toml"

echo "== toolchain =="
rustc -Vv
cargo -V

echo "== fmt =="
cargo fmt --manifest-path "$MANIFEST" --all -- --check

echo "== clippy (-D warnings) =="
cargo clippy --manifest-path "$MANIFEST" --locked --all-targets -- -D warnings

echo "== unit tests (lib) =="
cargo test --manifest-path "$MANIFEST" --locked --lib

echo "== CPU integration tests =="
cargo test --manifest-path "$MANIFEST" --locked --test cpu_legs

echo "== doc tests =="
cargo test --manifest-path "$MANIFEST" --locked --doc

echo "== compile GPU test targets (no run) =="
cargo test --manifest-path "$MANIFEST" --locked --all-targets --no-run

echo "rust-check: ALL GATES PASS (CPU/compile only; GPU runtime NOT verified here)"
