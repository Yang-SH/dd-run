#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 PyMin 示例装进 dd-run 的扩展目录（生成「绝对路径」清单）。

用法::

    python install.py                      # 装到 %APPDATA%\\dd-run\\extensions.d\\
    python install.py <目标目录>            # 自定义（例如 <dd-run.exe 同目录>\\extensions.d\\）
    python install.py --python <解释器路径> # 换一个解释器写进清单

为什么需要「生成」而不是直接给一份固定清单
--------------------------------------------
1. 清单的 ``entry.args`` **不支持** ``${EXT_DIR}`` 展开（``manifest-schema.md`` §4 只覆盖
   ``entry.command`` / ``entry.cwd`` / ``icon``）——表达不了「解释器 + 脚本」这种两段式命令行；
2. 用 ``.cmd`` 桥接（同目录的 ``dd-ext-pymin.cmd``）虽然可行，但它写的是裸 ``python``，
   **依赖解释器位于 PATH 上**。Windows 上这远非默认（Store 版 / 未勾选 Add-to-PATH /
   IDE 自带解释器都不在 PATH），而且 GUI 从资源管理器启动时继承的是**系统 PATH**——
   于是出现「命令行自检全绿，双击 dd-run 却启动不了」的经典假阳性。

所以这里把 ``sys.executable`` 与脚本的**绝对路径**直接写进清单：宿主直接 spawn
``command``、不经 shell，**零 PATH 依赖**。
"""
import argparse
import json
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXT_ID = "com.example.pymin"
SCRIPT_NAME = "dd_ext_pymin.py"
CAPABILITIES = ["host/show_status", "host/set_clipboard", "host/open_url"]


def default_target() -> Path:
    """按 ``manifest-schema.md`` §2 取当前平台的扩展目录。"""
    appdata = os.environ.get("APPDATA")
    if appdata:
        return Path(appdata) / "dd-run" / "extensions.d"
    return Path.home() / ".config" / "dd-run" / "extensions.d"


def main() -> int:
    parser = argparse.ArgumentParser(description="安装 PyMin 示例（生成绝对路径清单）")
    parser.add_argument("target", nargs="?", help="目标扩展目录（缺省 %APPDATA%\\dd-run\\extensions.d）")
    parser.add_argument("--python", default=sys.executable,
                        help="写进清单的解释器路径（默认当前解释器）")
    args = parser.parse_args()

    target = Path(args.target).expanduser() if args.target else default_target()
    target.mkdir(parents=True, exist_ok=True)

    interpreter = Path(args.python).resolve()
    if not interpreter.is_file():
        print(f"✗ 解释器不存在：{interpreter}", file=sys.stderr)
        return 1

    source = HERE / SCRIPT_NAME
    if not source.is_file():
        print(f"✗ 找不到 {source}", file=sys.stderr)
        return 1

    # 把脚本拷到目标目录，使该目录自包含
    installed_script = target / SCRIPT_NAME
    if source != installed_script:
        shutil.copyfile(source, installed_script)

    manifest = {
        "schema_version": "1.0",
        "id": EXT_ID,
        "name": "PyMin Example",
        "version": "1.0.0",
        "description": "Python extension sample (full protocol surface). See docs/extensions.md.",
        "author": "dd-run",
        "license": "MIT",
        "homepage": "https://github.com/Yang-SH/dd-run/blob/main/docs/extensions.md",
        # ★ 关键：绝对路径。宿主直接 spawn，不经 shell → 不依赖 PATH。
        "entry": {"command": str(interpreter), "args": [str(installed_script)]},
        "frozen": False,
        "capabilities": CAPABILITIES,
    }
    manifest_path = target / f"{EXT_ID}.json"
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"✓ 已安装到 {target}")
    print(f"  清单   : {manifest_path}")
    print(f"  解释器 : {interpreter}")
    print(f"  脚本   : {installed_script}")
    print()
    print("下一步：dd-run 设置 → 扩展 → PyMin Example → 点「重试」（或重启 dd-run）。")
    print("自检  ：dd-run-cli --conformance --extensions-dir "
          f'"{target}" --ext-id {EXT_ID}')
    return 0


if __name__ == "__main__":
    sys.exit(main())
