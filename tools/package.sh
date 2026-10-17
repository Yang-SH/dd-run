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

# 0) 自愈：宿主 bash 环境可能缺 POSIX coreutils。例如 WorkBuddy 托管的
#    PortableGit bash 以非交互方式启动（BASH_ENV 注入、不读 /etc/profile），
#    PATH 里没有 git 的 usr/bin → dirname/sed/du/cut/cp 全部找不到，会让下面
#    的 `cd "$(dirname "$0")/.."` 当场失败，且 cwd 停在 bash 安装根导致 cargo
#    找不到 Cargo.toml。仅在检测到 dirname 缺失时才把可用的 git/MSYS usr/bin
#    前置兜底；正常环境（coreutils 齐全）此块完全不执行、零影响。
#    `/usr/bin` `/bin` 是 MSYS 对当前 git 安装的映射，通常已足够。
if ! command -v dirname >/dev/null 2>&1; then
  for __d in /usr/bin /bin \
             "$HOME/.workbuddy/binaries/PortableGit/versions/"*/usr/bin \
             "/c/Program Files/Git/usr/bin" \
             "/c/Program Files (x86)/Git/usr/bin"; do
    if [ -d "$__d" ]; then
      PATH="$__d:$PATH"
    fi
  done
  export PATH
  echo "[package.sh] PATH 缺 coreutils，已自愈前置 git/MSYS usr/bin" >&2
fi

cd "$(dirname "$0")/.."   # 仓库根

# ============================================================
# O6 批次四（2026-10-11）：非 Windows 分支——Linux/macOS 免安装单二进制包。
# 形态与运行时依赖清单见 docs/optimization-plan.md §2.6.3。要点：
# - 单二进制 dd-run（内置扩展 in-process）+ README-dist.txt 打 tar.gz；
#   filesearch sidecar 为 Windows 专属不随包（聚合层优雅降级）→ 无签名面。
# - .cargo/config.toml 硬编码 build.target = windows-gnu（单机开发口径），
#   此处显式 --target 宿主三元组覆盖（与 CI 门禁 CARGO_BUILD_TARGET 同语义，
#   O6 首批 E0463 的既有修复口径）。
# Windows 分支在下方原样保留（本分支结束即 exit 0，逐字节不动）。
# ============================================================
OS_NAME="$(uname -s)"
if [ "$OS_NAME" = "Linux" ] || [ "$OS_NAME" = "Darwin" ]; then
  HOST_TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
  ARCH="${HOST_TRIPLE%%-*}"   # x86_64 | aarch64
  REL="target/${HOST_TRIPLE}/release"

  echo "==> [package.sh 非 Windows 分支] OS=$OS_NAME 宿主三元组=$HOST_TRIPLE"
  echo "==> [1/3] 构建宿主 dd-gui（→ dd-run 单文件；内置扩展 in-process）"
  cargo build --release --target "$HOST_TRIPLE" -p dd-gui --bin dd-run

  VER=$(grep -m1 '^version' crates/dd-gui/Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')
  case "$OS_NAME" in
    Linux)  TAG="linux-${ARCH}" ;;
    Darwin) TAG="macos-${ARCH}" ;;
  esac

  OUTDIR="dist/dd-run-${VER}-${TAG}"
  mkdir -p "$OUTDIR"
  cp "${REL}/dd-run" "$OUTDIR/dd-run"
  chmod 755 "$OUTDIR/dd-run"

  echo "==> [2/3] 生成 README-dist.txt（运行时依赖 + 数据目录说明）"
  if [ "$OS_NAME" = "Linux" ]; then
    DATA_DIR_LINE='$XDG_CONFIG_HOME/dd-run（缺省 ~/.config/dd-run）'
    # 依赖树实证（O6 批次四核对产出修正）：winit 0.30.13 仅 dlopen
    # xlib/xlib_xcb/xcursor/xinput2（x11-dl）+ libxcb（x11rb libloading）+
    # xkbcommon-dl 两库；randr 等走 x11rb 纯 Rust 协议不加载额外库；
    # GL = glutin_egl_sys(libEGL) / glutin_glx_sys(libGL)。
    PKG_LINE='X11 会话：libx11-6 libx11-xcb1 libxcb1 libxcursor1 libxi6 libxkbcommon0 libxkbcommon-x11-0 libegl1 libgl1；Wayland 会话：libwayland-client0 libxkbcommon0 libegl1 libgl1（Debian/Ubuntu 包名，按会话类型二选一；其他发行版按库名对应安装）'
  else
    DATA_DIR_LINE='$HOME/Library/Application Support/dd-run'
    PKG_LINE='（macOS 无外部依赖，系统框架自足）'
  fi
  cat > "$OUTDIR/README-dist.txt" <<EOF
dd-run ${VER}（${TAG}）— 免安装单文件版

dd-run ${VER} (${TAG}) — portable single-file build

运行 / Run：
  ./dd-run

系统依赖（仅 Linux；缺库表现为启动即报错，不会静默失败）：
  ${PKG_LINE}
  （渲染后端 glow 经 dlopen 加载 libGL/libEGL，mesa 系驱动即含。）

数据目录 / Data dir：
  ${DATA_DIR_LINE}
  （config.json 设置 / extensions.d/ 扩展 / cache/ 缓存 / logs/ panic 取证）

说明：
  - 5 个内置扩展（apps/calc/system/websearch/shell）in-process 内置于宿主
    进程，单文件即全部；第三方扩展放数据目录 extensions.d/ 即被发现。
  - 文件搜索扩展（Everything sidecar）为 Windows 专属，不随本包分发；
    面板内该类别自动缺席，其余功能不受影响。
  - Linux 下 Wayland 会话不支持全局热键（应用内降级为无热键并提示），
    X11 会话功能完整。
EOF

  echo "==> [3/3] 打包 dist/dd-run-${VER}-${TAG}.tar.gz"
  ( cd dist && tar -czf "dd-run-${VER}-${TAG}.tar.gz" "dd-run-${VER}-${TAG}" )
  echo "✅ 产物：dist/dd-run-${VER}-${TAG}.tar.gz（$(du -h "dist/dd-run-${VER}-${TAG}.tar.gz" | cut -f1)）"
  exit 0
fi

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

# 6.5) O14/F12 批次二（可选）：发行方签名——DDRUN_SIGN_KEY 指向 64 字符 hex
# 种子文件时，对随包清单签发 .sig（与 dd-host::signing 验签互逆）；未设置则
# 跳过，产物不含 .sig，宿主对随包扩展回落信任同意流（R-12 钉扎仍生效）。
if [ -n "${DDRUN_SIGN_KEY:-}" ] && [ -f "${DDRUN_SIGN_KEY}" ]; then
  echo "==> 签发随包扩展 .sig（发行方钥，O14/F12 批次二）"
  cargo ${TOOLCHAIN} build --release -p dd-run-cli
  "${REL}/dd-run-cli.exe" --sign-ext \
    --manifest dist/extensions.d/com.ddrun.filesearch.json \
    --exe dist/extensions.d/dd-ext-search.exe \
    --key "${DDRUN_SIGN_KEY}"
else
  echo "==> DDRUN_SIGN_KEY 未设置，跳过 .sig 签发（随包扩展回落信任同意流）"
fi

# 可选：同时把开发自检 CLI 与示例扩展放到 dist/dev/（非分发必需，便于排查）。
# 拷贝前先构建——target/release 里可能残留旧产物（2026-10-11 密钥仪式踩坑：
# dist/dev 的 dd-run-cli.exe 是 O14 批次二之前的版本，缺 --gen-sign-key 等子命令）。
DEV="dist/dev"
mkdir -p "$DEV"
cargo ${TOOLCHAIN} build --release -p dd-run-cli -p dd-ext-sample
cp "${REL}/dd-run-cli.exe"   "$DEV/" 2>/dev/null || true
cp "${REL}/dd-ext-sample.exe" "$DEV/" 2>/dev/null || true
echo "（可选自检工具已放 $DEV/：dd-run-cli.exe、dd-ext-sample.exe）"

# 7) 便携包 zip（2026-10-17 收口批）：**单一来源** tools/portable_zip.ps1——
#    与 release.yml 同一脚本，消除「本地只出 exe、zip 靠 CI 补」的成员集合
#    不一致（此前每次本地重打包都要手工补 zip）。放在签名（§6.5）之后，
#    故随包 .sig 若已生成会一并入包。pwsh/powershell 均不可用时仅告警跳过，
#    不使整个打包失败（exe + sidecar 仍已就绪）。
ZIP_PS="tools/portable_zip.ps1"
PS_BIN=""
for __b in pwsh powershell; do
  if command -v "$__b" >/dev/null 2>&1; then PS_BIN="$__b"; break; fi
done
if [ -n "$PS_BIN" ]; then
  echo "==> 生成便携包 dist/dd-run-${VER}-portable.zip（$PS_BIN）"
  "$PS_BIN" -NoProfile -File "$ZIP_PS"
else
  echo "⚠️ 未找到 pwsh/powershell，跳过便携包生成（可手动跑见 tools/portable_zip.ps1 头注）" >&2
fi
