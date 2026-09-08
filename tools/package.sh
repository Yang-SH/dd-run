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

# 1) windows-gnu 工具链 self-contained bin（含 as.exe 等），**追加**到 PATH 末尾：
#    优先使用 PATH 上已有的可用 mingw（如本机 /c/Strawberry/c/bin），self-contained
#    仅作兜底。前置会遮蔽可用链接器、导致 ld 找不到 crt2.o/libkernel32.a。
#    路径经 $HOME/RUSTUP_HOME 推导（CI runner 同样适用），目录不存在则跳过。
SELF_CONTAINED="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained"
if [ -d "$SELF_CONTAINED" ]; then
  export PATH="$PATH:$SELF_CONTAINED"
fi
TOOLCHAIN="+stable-x86_64-pc-windows-gnu"

REL="target/x86_64-pc-windows-gnu/release"
EMBED="crates/dd-gui/assets/embed"
BINS="dd-ext-apps dd-ext-calc dd-ext-system dd-ext-websearch dd-ext-shell"

# 2) 先构建 5 个内置扩展（release）——它们是待内嵌的字节源
echo "==> [1/5] 构建扩展（dd-ext）"
cargo ${TOOLCHAIN} build --release -p dd-ext

# 3) 把扩展 exe 拷入宿主 assets/embed（供 dd-gui build.rs 内嵌）
echo "==> [2/5] 拷贝扩展到 $EMBED"
mkdir -p "$EMBED"
for bin in $BINS; do
  cp "${REL}/${bin}.exe" "$EMBED/"
done
ls -1 "$EMBED"/*.exe

# 4) 构建宿主 dd-gui → 产物 dd-run.exe（build.rs 见 assets/embed 有 exe → 内嵌）
echo "==> [3/5] 构建宿主 dd-gui（→ dd-run.exe，内嵌扩展）"
cargo ${TOOLCHAIN} build --release -p dd-gui --bin dd-run

# 5) 归集单文件到 dist/
echo "==> [4/5] 产出单文件到 dist/"
VER=$(grep -m1 '^version' crates/dd-gui/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')
OUT="dist/dd-run-${VER}.exe"
mkdir -p dist
cp "${REL}/dd-run.exe" "$OUT"
echo "✅ 单文件已生成：$OUT（$(du -h "$OUT" | cut -f1)）"

# 6) 文件搜索 sidecar（批次 7.4 定案：不内嵌，随绿色包 dist/extensions.d/ 携带——
#    免安装分发；清单 ${EXT_DIR} 解析到本目录，自动定位 exe）
echo "==> [5/5] 归集文件搜索 sidecar 到 dist/extensions.d"
mkdir -p dist/extensions.d
cp "${REL}/dd-ext-search.exe" dist/extensions.d/
cp examples/extensions.d/com.ddrun.filesearch.json dist/extensions.d/

# 可选：同时把开发自检 CLI 与示例扩展放到 dist/dev/（非分发必需，便于排查）
DEV="dist/dev"
mkdir -p "$DEV"
cp "${REL}/dd-run-cli.exe"   "$DEV/" 2>/dev/null || true
cp "${REL}/dd-ext-sample.exe" "$DEV/" 2>/dev/null || true
echo "（可选自检工具已放 $DEV/：dd-run-cli.exe、dd-ext-sample.exe）"
