#!/usr/bin/env bash
# 产出**单文件** dd-run.exe（宿主二进制，内置扩展 in-process）。
#
# 原理（M9 起）：
# - 内置扩展（apps/calc/system/websearch/shell）**进程内化**——宿主直接调
#   `dd_ext::serve_line`，不 spawn 子进程、不内嵌 exe（见 docs/m9-inprocess-builtins.md）。
#   单文件 = 仅宿主 dd-run.exe 一个二进制；体积较 M8/内嵌期大幅下降。
# - 文件搜索 sidecar（dd-ext-search.exe）**保持子进程**（ADR-1 对其仍生效），
#   随绿色包 dist/extensions.d/ 携带，不内嵌。
#
# 用法：bash tools/package.sh
set -euo pipefail

cd "$(dirname "$0")/.."   # 仓库根

# 1) windows-gnu 工具链 self-contained bin（含 as.exe 等），**追加**到 PATH 末尾：
#    优先使用 PATH 上已有的可用 mingw（如本机 /c/Strawberry/c/bin），self-contained
#    仅作兜底。前置会遮蔽可用链接器、导致 ld 找不到 crt2.o/libkernel32.a。
#    路径经 $HOME/RUSTUP_HOME 推导（CI runner 同样适用），目录不存在则跳过。
#
#    ⚠️ 已知环境坑（2026-09-11）：rustup 自带的 self-contained `as.exe` **不在其目录内
#    自带运行时 DLL**（依赖 `libintl-8.dll` / `libzstd.dll` / `zlib1.dll`）。若 PATH 上
#    没有提供这些 DLL 的 mingw 运行时，`as.exe` 启动即 `STATUS_DLL_NOT_FOUND`，
#    链接期报 `dlltool ... as exited with status 53`（表现为 release 构建在
#    `windows-core`/`windows` 处失败）。**修复**：任选其一——
#      (a) 把某个 mingw 运行时的 `bin/`（含那三个 DLL）加到 PATH；
#      (b) 直接把这三个 DLL 拷进上面 `$SELF_CONTAINED` 目录（挨着 as.exe，一劳永逸）；
#      (c) 装系统 mingw-w64（其 bin 通常已含这些 DLL）。
SELF_CONTAINED="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained"
if [ -d "$SELF_CONTAINED" ]; then
  export PATH="$PATH:$SELF_CONTAINED"
fi
TOOLCHAIN="+stable-x86_64-pc-windows-gnu"

REL="target/x86_64-pc-windows-gnu/release"
EMBED="crates/dd-gui/assets/embed"

# 2) 构建 dd-ext（release）——产出 5 个内置 bin（dev/调试 + 第三方兼容，不内嵌）
#    与文件搜索 sidecar dd-ext-search.exe（下一步复制到 dist/extensions.d/）。
echo "==> [1/4] 构建 dd-ext（release）"
cargo ${TOOLCHAIN} build --release -p dd-ext

# 3) 清理 assets/embed/ 中残留的内置 exe（M9 起不再内嵌；确保 build.rs 恒生成空表）。
echo "==> [2/4] 清理内嵌目录（M9 起不再内嵌内置）"
mkdir -p "$EMBED"
rm -f "$EMBED"/*.exe

# 4) 构建宿主 dd-gui → 产物 dd-run.exe（build.rs 见 assets/embed 无 exe → 不内嵌）。
echo "==> [3/4] 构建宿主 dd-gui（→ dd-run.exe，纯宿主单文件）"
cargo ${TOOLCHAIN} build --release -p dd-gui --bin dd-run

# 5) 归集单文件到 dist/
echo "==> [4/4] 产出单文件到 dist/"
VER=$(grep -m1 '^version' crates/dd-gui/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')
OUT="dist/dd-run-${VER}.exe"
mkdir -p dist
cp "${REL}/dd-run.exe" "$OUT"
echo "✅ 单文件已生成：$OUT（$(du -h "$OUT" | cut -f1)）"

# 6) 文件搜索 sidecar（ADR-1 仍生效：保持子进程，随绿色包携带，不内嵌）。
echo "==> 归集文件搜索 sidecar 到 dist/extensions.d"
mkdir -p dist/extensions.d
cp "${REL}/dd-ext-search.exe" dist/extensions.d/
cp examples/extensions.d/com.ddrun.filesearch.json dist/extensions.d/

# 可选：同时把开发自检 CLI 与示例扩展放到 dist/dev/（非分发必需，便于排查）
DEV="dist/dev"
mkdir -p "$DEV"
cp "${REL}/dd-run-cli.exe"   "$DEV/" 2>/dev/null || true
cp "${REL}/dd-ext-sample.exe" "$DEV/" 2>/dev/null || true
echo "（可选自检工具已放 $DEV/：dd-run-cli.exe、dd-ext-sample.exe）"
