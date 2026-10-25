//! §5.4（dd-run-改进分析.md）：扩展开发脚手架——`--ext-new <ID>` 一键生成
//! 「清单 + 最小实现 + 启动器 + conformance CI 模板」，零基础用户十分钟内
//! 产出通过 `--conformance` 的扩展。
//!
//! 布局（**与宿主扫描同构**）：`scan_dir` 平铺读取扩展目录下的 `*.json`
//! 清单（不递归子目录），故生成物直接落在目标目录根部：
//!
//! ```text
//! <DIR>/<ID>.json      清单（entry = ${EXT_DIR}/start，Windows 自动补 .cmd）
//! <DIR>/main.py        最小实现（Python 3 标准库，零依赖）
//! <DIR>/start          POSIX 启动器（exec python3 main.py，附执行位）
//! <DIR>/start.cmd      Windows 启动器（py/py -3 回退）
//! <DIR>/conformance.yml  GitHub Action 模板（拷入 .github/workflows/ 即用）
//! <DIR>/README.md      三步上手 + 已知坑（python PATH）
//! ```
//!
//! 最小实现刻意**只覆盖必选表面**（initialize / top_level_commands /
//! get_command / invoke / close）：`has_fallback=false`（conformance §4 跳过
//! 调用）、无 page 命令（§6 是 warn 非 fail）、`capabilities=[]`（§8 无反向
//! 请求）——面越窄，"绿灯"语义越干净；扩展面按需对照 `examples/python-minimal`
//! （全表面参考实现）逐步加。

use std::path::{Path, PathBuf};

/// 校验扩展 id（manifest-schema §「id」：反向域名建议；扫描器会再验一遍，
/// 这里给出**脚手架层**的友好错误而非扫描器的 skip 行）。
pub fn validate_ext_id(id: &str) -> Result<(), String> {
    let segs: Vec<&str> = id.split('.').collect();
    if segs.len() < 2 {
        return Err(format!(
            "`{id}` 不像反向域名（至少两段，如 `com.example.hello`）"
        ));
    }
    for seg in &segs {
        if seg.is_empty() {
            return Err(format!("`{id}` 含空段（连续点或首尾点）"));
        }
        let first = seg.chars().next().expect("非空段必有首字符");
        if !first.is_ascii_lowercase() {
            return Err(format!("`{id}` 段 `{seg}` 应以小写字母开头"));
        }
        if !seg
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!(
                "`{id}` 段 `{seg}` 含非法字符（只允许小写字母 / 数字 / 连字符）"
            ));
        }
    }
    Ok(())
}

/// 在 `out_dir` 生成脚手架（ refuse 覆盖已有文件），返回扩展目录。
pub fn ext_new(id: &str, out_dir: &Path) -> Result<PathBuf, String> {
    validate_ext_id(id)?;
    let files: Vec<(String, String)> = vec![
        (format!("{id}.json"), manifest_json(id)),
        ("main.py".into(), main_py(id)),
        ("start".into(), start_sh(id)),
        ("start.cmd".into(), start_cmd(id)),
        ("install.py".into(), install_py(id)),
        ("conformance.yml".into(), conformance_yml(id)),
        ("README.md".into(), readme_md(id)),
    ];
    for (name, _) in &files {
        let p = out_dir.join(name);
        if p.exists() {
            return Err(format!(
                "`{}` 已存在（脚手架拒绝覆盖，请换目录或先清理）",
                p.display()
            ));
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("创建目录失败：{e}"))?;
    for (name, content) in &files {
        let p = out_dir.join(name);
        std::fs::write(&p, content.as_bytes())
            .map_err(|e| format!("写 `{}` 失败：{e}", p.display()))?;
    }
    // POSIX 启动器附执行位（Windows 文件系统上 no-op）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            out_dir.join("start"),
            std::fs::Permissions::from_mode(0o755),
        );
    }
    Ok(out_dir.to_path_buf())
}

fn manifest_json(id: &str) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "id": "{id}",
  "name": "{id}",
  "version": "0.1.0",
  "description": "脚手架生成的最小 dd-run 扩展（--ext-new）。",
  "entry": {{ "command": "${{EXT_DIR}}/start" }},
  "frozen": false,
  "capabilities": [],
  "platforms": ["windows", "macos", "linux"]
}}
"#
    )
}

fn main_py(id: &str) -> String {
    format!(
        r#"#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""{id} —— dd-run 最小扩展示例（脚手架生成，Python 3 仅标准库）。

契约来源：docs/protocol.md · docs/manifest-schema.md · docs/extensions.md
三条铁律（违反必然导致宿主解析失败，详见 examples/python-minimal 的完整讲解）：
  1. stdout 只允许出现协议消息：一行一条紧凑 JSON、以 \n 结尾；
  2. UTF-8 编码，行内不得有裸换行；
  3. 通知（无 id）不得回复。

本文件只覆盖**必选表面**（initialize / top_level_commands / get_command /
invoke / close）；要加兜底 / 嵌套页 / host/* 能力，对照
examples/python-minimal/dd_ext_pymin.py（全表面参考实现）逐步扩展。
"""
import json
import sys

PROVIDER_ID = "{id}"          # 必须与清单 id 一致（§7 规则注释）
PROTOCOL_VERSION = "1.0"


def _setup_stdio():
    # Windows 双陷阱：\n 被翻译成 \r\n、stdout 默认编码可能是 cp936。
    try:
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
        sys.stdin.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):
        pass


def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def log(message):
    # 诊断日志只走 stderr（协议 §2.5：stdout 不得出现非协议内容）。
    print(f"[{{PROVIDER_ID}}] {{message}}", file=sys.stderr, flush=True)


def reply(mid, result):
    send({{"jsonrpc": "2.0", "id": mid, "result": result}})


def reply_error(mid, code, message):
    send({{"jsonrpc": "2.0", "id": mid, "error": {{"code": code, "message": message}}}})


def top_level_commands():
    """§6.1 首屏命令；command.kind 只允许 invoke / page（§8.2）。"""
    return [
        {{"id": "hello", "title": "Hello", "section": "{id}",
          "command": {{"kind": "invoke"}}}},
    ]


def get_command(cid):
    """§6.4 按 id 取回真实命令（桩复热用）；找不到返回 None → command: null。"""
    if cid == "hello":
        return top_level_commands()[0]
    return None


def handle_invoke(params):
    """§6.5：result 字段就是 CommandResult 本体（不要再包一层）。"""
    cid = params.get("id")
    context = params.get("context") or {{}}
    query = (context.get("query") or "").strip()
    if cid == "hello":
        message = f"Hello，{{query}}" if query else "Hello，dd-run！"
        return {{"kind": "ShowToast", "args": {{"message": message, "duration_ms": 2000}}}}
    return None  # → -32002 command_not_found


def handle(msg):
    """处理一条请求；返回 "exit" 表示可以退出。"""
    mid = msg.get("id")
    method = msg.get("method")
    params = msg.get("params") or {{}}

    if method is None or mid is None:
        return None  # 对端响应 / 通知：忽略（§3.1、§3.3）

    if method == "initialize":
        reply(mid, {{
            "protocol_version": PROTOCOL_VERSION,
            "provider": {{"id": PROVIDER_ID, "display_name": PROVIDER_ID,
                          "frozen": False, "has_fallback": False}},
            "capabilities": [],   # 要用 host/* 时在此声明（§7.4：未声明回 -32601）
        }})
        return None
    if method == "top_level_commands":
        reply(mid, {{"commands": top_level_commands()}})
        return None
    if method == "get_command":
        cid = params.get("id")
        reply(mid, {{"command": get_command(cid) if isinstance(cid, str) else None}})
        return None
    if method == "invoke":
        result = handle_invoke(params)
        if result is None:
            reply_error(mid, -32002, "command_not_found")
        else:
            reply(mid, result)
        return None
    if method == "close":
        reply(mid, {{}})       # §6.6：回 result 后尽快自行退出
        return "exit"
    reply_error(mid, -32601, "Method not found")
    return None


def main():
    _setup_stdio()
    log("启动")
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue              # §2.2 规则 5：空行忽略
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError as exc:
            reply_error(None, -32700, "Parse error")
            log(f"parse error: {{exc}}")
            continue
        if isinstance(msg, list):
            reply_error(None, -32600, "Invalid Request")
            continue              # §3.4 不支持批处理
        if handle(msg) == "exit":
            break
    log("退出")


if __name__ == "__main__":
    main()
"#
    )
}

fn start_sh(id: &str) -> String {
    format!(
        "#!/bin/sh\n# {id} 启动器（POSIX）：依赖 python3 在 PATH 上。\nexec python3 \"$(dirname \"$0\")/main.py\" \"$@\"\n"
    )
}

fn start_cmd(id: &str) -> String {
    format!(
        "@echo off\r\nrem {id} 启动器（Windows，手工运行用）：依赖 python 在 PATH 上（py 启动器优先）。\r\nrem 注意：宿主**不会**自动把 entry 补全成 .cmd（F4 fail-closed，BatBadBut 纵深防御）——\r\nrem 让 dd-run 宿主启动本扩展请用 install.py 写绝对路径清单（见下）。\r\nsetlocal\r\nset \"PY=python\"\r\nwhere py >nul 2>nul && set \"PY=py -3\"\r\n%PY% \"%~dp0main.py\" %*\r\n"
    )
}

fn install_py(id: &str) -> String {
    format!(
        r##"#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把本扩展装进 dd-run 的扩展目录（生成「绝对路径」清单）——pymin 示例同款。

用法::

    python install.py                      # 装到 %APPDATA%\dd-run\extensions.d\
    python install.py <目标目录>            # 自定义（自检：配 --conformance 用）

为什么需要「生成」而不是用仓库里的 <ID>.json
--------------------------------------------
1. Windows 上宿主对清单可执行文件只做「原样优先 → 补 .exe」（F4 fail-closed，
   BatBadBut 纵深防御）——不会把 `start` 补成 `start.cmd`；
2. `entry.args` 不支持 ${{EXT_DIR}} 展开（manifest-schema §4），表达不了
   「解释器 + 脚本」两段式命令行；
3. 裸 `python` 依赖系统 PATH（GUI 从资源管理器启动继承系统 PATH）——
   「命令行自检全绿、双击 dd-run 启动不了」的经典假阳性。

故把 sys.executable 与脚本的**绝对路径**写进清单：宿主直接 spawn，零 PATH 依赖。
"""
import argparse
import json
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXT_ID = "{id}"
SCRIPT_NAME = "main.py"


def default_target():
    appdata = os.environ.get("APPDATA")
    if appdata:
        return Path(appdata) / "dd-run" / "extensions.d"
    return Path.home() / ".config" / "dd-run" / "extensions.d"


def main() -> int:
    parser = argparse.ArgumentParser(description="安装本扩展（生成绝对路径清单）")
    parser.add_argument("target", nargs="?", help="目标扩展目录（缺省用户数据 extensions.d）")
    parser.add_argument("--python", default=sys.executable, help="写进清单的解释器路径")
    args = parser.parse_args()

    target = Path(args.target).expanduser() if args.target else default_target()
    target.mkdir(parents=True, exist_ok=True)

    interpreter = Path(args.python).resolve()
    if not interpreter.is_file():
        print(f"✗ 解释器不存在：{{interpreter}}", file=sys.stderr)
        return 1

    installed_script = target / SCRIPT_NAME
    shutil.copyfile(HERE / SCRIPT_NAME, installed_script)

    manifest = {{
        "schema_version": "1.0",
        "id": EXT_ID,
        "name": EXT_ID,
        "version": "0.1.0",
        "description": "脚手架生成的最小 dd-run 扩展（--ext-new；绝对路径清单）。",
        "entry": {{"command": str(interpreter), "args": [str(installed_script)]}},
        "frozen": False,
        "capabilities": [],
    }}
    manifest_path = target / f"{{EXT_ID}}.json"
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"✓ 已安装到 {{target}}")
    print(f"  清单   : {{manifest_path}}")
    print(f"  解释器 : {{interpreter}}")
    print()
    print("下一步：重启 dd-run 面板（或设置 → 扩展 → 重试）。")
    print(f"自检  ：dd-run-cli --conformance --extensions-dir \\"{{target}}\\" --ext-id {{EXT_ID}}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
"##
    )
}

fn conformance_yml(id: &str) -> String {
    format!(
        r#"# {id} —— dd-run conformance CI 模板（脚手架生成）。
# 用法：把本文件拷到你仓库的 .github/workflows/conformance.yml。
# 前提：扩展文件（<ID>.json / main.py / start*）位于仓库根（或把下面的
# --extensions-dir 改成实际所在子目录）。
name: conformance

on:
  push:
  pull_request:

jobs:
  conformance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # GitHub 托管 runner **不预装 Rust**——cargo install 前必须先装工具链
      # （版本 tag 而非 SHA 钉扎：本模板进的是用户仓库，随上游更新即可）。
      - uses: actions-rust-lang/setup-rust-toolchain@v1

      - uses: actions/setup-python@v5
        with:
          python-version: "3.x"

      - name: Install dd-run CLI (from upstream repo)
        run: cargo install --git https://github.com/Yang-SH/dd-run dd-run-cli

      - name: Conformance
        run: dd-run-cli --conformance --extensions-dir . --ext-id {id}
"#
    )
}

fn readme_md(id: &str) -> String {
    format!(
        r#"# {id}（dd-run 扩展脚手架）

由 `dd-run-cli --ext-new {id}` 生成。

## 自检

```bash
# Linux / macOS：仓库里的 <ID>.json（entry = ${{EXT_DIR}}/start）开箱即绿
dd-run-cli --conformance --extensions-dir . --ext-id {id}

# Windows：先装（写绝对路径清单，绕开 PATH 与 .cmd 补全两大坑），再自检
python install.py ./installed
dd-run-cli --conformance --extensions-dir ./installed --ext-id {id}
```

## 本机试用

```bash
python install.py          # 装到用户数据目录 extensions.d，重启 dd-run 面板即可见
```

## CI

把 `conformance.yml` 拷到你仓库的 `.github/workflows/`（ubuntu runner 自带
Python；CLI 经 `cargo install --git` 获取）。

## 文件说明

| 文件 | 作用 |
| --- | --- |
| `{id}.json` | 仓库清单（`entry = ${{EXT_DIR}}/start`，POSIX/CI 直接可用） |
| `main.py` | 最小实现（Python 3 标准库；契约：docs/protocol.md） |
| `start` / `start.cmd` | 双平台启动器（`start.cmd` 仅手工运行用——宿主不会补全 .cmd，见下） |
| `install.py` | Windows/本机安装器：写**绝对路径**清单到目标目录（pymin 示例同款） |
| `conformance.yml` | GitHub Action 模板（跑 conformance） |

## 下一步

- 加命令：在 `top_level_commands()` 里加条目，`get_command()` 同步登记；
- 加兜底 / 嵌套页 / host/* 能力：对照 `examples/python-minimal/dd_ext_pymin.py`
  （全表面参考实现），并在清单里同步 `has_fallback` / `capabilities`；
- 已知坑：① 宿主对清单可执行文件只做「原样优先 → 补 .exe」，**不补 .cmd**
  （F4 fail-closed，BatBadBut 纵深防御）——宿主启动一律走 `install.py` 的
  绝对路径清单；② `start.cmd` 依赖 `python` 在系统 PATH，仅适合手工调试。
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_validation_accepts_reverse_dns_and_rejects_bad_shapes() {
        assert!(validate_ext_id("com.example.hello").is_ok());
        assert!(validate_ext_id("io.github.someone.my-ext").is_ok());
        assert!(validate_ext_id("hello").is_err(), "单段非反向域名");
        assert!(validate_ext_id("com..hello").is_err(), "空段");
        assert!(validate_ext_id("com.Hello").is_err(), "大写");
        assert!(validate_ext_id("com.1hello").is_err(), "段不以字母开头");
        assert!(validate_ext_id("com.ex ample").is_err(), "空格");
    }

    /// 生成物经 dd_host 真实扫描器加载——"清单合法"由权威校验器判定，
    /// 不靠脚手架自证（§5.4 验收的自动化部分）。
    #[test]
    fn generated_manifest_loads_via_real_scanner() {
        let tmp = std::env::temp_dir().join(format!("dd-run-scaffold-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let dir = ext_new("com.example.scaffold", &tmp).unwrap();
        let opts = dd_host::manifest::ScanOptions {
            platform: "windows".into(),
            host_version: "0.2.1".into(),
            home: std::path::PathBuf::new(),
        };
        let outcome = dd_host::manifest::scan_dir(&dir, &opts);
        assert!(
            outcome.dir_error.is_none(),
            "目录错误：{:?}",
            outcome.dir_error
        );
        assert!(
            outcome.skipped.is_empty(),
            "生成清单不应被扫描器跳过：{:?}",
            outcome.skipped
        );
        assert_eq!(outcome.loaded.len(), 1, "应恰好加载 1 个扩展");
        let ext = &outcome.loaded[0];
        assert_eq!(ext.manifest.id, "com.example.scaffold");
        assert_eq!(ext.manifest.entry.command, "${EXT_DIR}/start");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 覆盖保护：已有同名清单时拒绝生成。
    #[test]
    fn refuses_to_overwrite_existing_files() {
        let tmp =
            std::env::temp_dir().join(format!("dd-run-scaffold-test2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("com.example.x.json"), b"{}").unwrap();
        let err = ext_new("com.example.x", &tmp).unwrap_err();
        assert!(err.contains("已存在"), "应拒绝覆盖：{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
