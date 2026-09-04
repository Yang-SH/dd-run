#!/usr/bin/env bash
# 产出**单文件** dd-run.exe（宿主 + 5 个内置扩展 exe 内嵌于一体，真·一个可执行）。
#
# 原理：
# - ADR-1 进程隔离是硬约束——宿主仍通过 spawn 独立子进程与扩展通信（
#   dd-host::process::ExtensionProcess）。"单文件" = 把 5 个扩展 exe 的**字节**
#   内嵌进宿主 exe，运行时物化到 %APPDATA%/dd-run/cache/embedded/ 再 spawn。
# - 内嵌机制：本脚本先把 dd-ext 的 5 个 exe 拷入 crates/dd-gui/assets/embed/，
#   再构建 dd-gui；其 build.rs 用 include_bytes! 把已就位的 exe 编入宿主
#   （生成 src 侧 embedded.rs 的 EMBEDDED 表）。产物即单文件 dd-run.exe。
#
# 用法：bash tools/package.sh
set -euo pipefail

cd "$(dirname "$0")/.."   # 仓库根

# 1) windows-gnu 工具链 self-contained bin（含 as.exe，链接必需）
export PATH="/c/Users/y7398/.rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained:$PATH"
TOOLCHAIN="+stable-x86_64-pc-windows-gnu"

REL="target/x86_64-pc-windows-gnu/release"
EMBED="crates/dd-gui/assets/embed"
BINS="dd-ext-apps dd-ext-calc dd-ext-system dd-ext-websearch dd-ext-shell"

# 2) 先构建 5 个内置扩展（release）——它们是待内嵌的字节源
echo "==> [1/4] 构建扩展（dd-ext）"
cargo ${TOOLCHAIN} build --release -p dd-ext

# 3) 把扩展 exe 拷入宿主 assets/embed（供 dd-gui build.rs 内嵌）
echo "==> [2/4] 拷贝扩展到 $EMBED"
mkdir -p "$EMBED"
for bin in $BINS; do
  cp "${REL}/${bin}.exe" "$EMBED/"
done
ls -1 "$EMBED"/*.exe

# 4) 构建宿主 dd-gui → 产物 dd-run.exe（build.rs 见 assets/embed 有 exe → 内嵌）
echo "==> [3/4] 构建宿主 dd-gui（→ dd-run.exe，内嵌扩展）"
cargo ${TOOLCHAIN} build --release -p dd-gui --bin dd-run

# 5) 归集单文件到 dist/
echo "==> [4/4] 产出单文件到 dist/"
VER=$(grep -m1 '^version' crates/dd-gui/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')
OUT="dist/dd-run-${VER}.exe"
mkdir -p dist
cp "${REL}/dd-run.exe" "$OUT"
echo "✅ 单文件已生成：$OUT（$(du -h "$OUT" | cut -f1)）"

# 可选：同时把开发自检 CLI 与示例扩展放到 dist/dev/（非分发必需，便于排查）
DEV="dist/dev"
mkdir -p "$DEV"
cp "${REL}/dd-run-cli.exe"   "$DEV/" 2>/dev/null || true
cp "${REL}/dd-ext-sample.exe" "$DEV/" 2>/dev/null || true
echo "（可选自检工具已放 $DEV/：dd-run-cli.exe、dd-ext-sample.exe）"
