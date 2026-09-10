#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dd-run 扩展示例（Python 3 · 仅标准库 · 全表面）。

契约来源（本文件逐条对齐，不臆造）：
  - ``docs/protocol.md``         —— JSON-RPC over NDJSON 的完整定义
  - ``docs/manifest-schema.md``  —— 清单字段、发现路径、校验规则
  - ``docs/extensions.md``       —— 与本示例配套的上手指南

覆盖的协议表面
  host → ext：initialize / top_level_commands / fallback_commands / get_items /
              get_command / invoke / close
  ext → host：items_changed（通知）/ host/show_status / host/set_clipboard /
              host/open_url（请求）
  演示的 CommandResult：ShowToast / Dismiss / Confirm（+ 确认后重发 invoke）

三条铁律（违反必然导致宿主解析失败）
  1. **stdout 只允许出现协议消息**：一行一条**紧凑** JSON、以 ``\\n`` 结尾。
     任何 ``print`` 调试都必须写 stderr（见 ``log()``）。
  2. **UTF-8 编码**，且行内不得出现裸换行（JSON 字符串里的换行转义为 ``\\n``）。
  3. **通知（无 ``id``）不得回复**；对端对我方请求的响应（无 ``method``）忽略即可。

两个 Windows 专属陷阱（本示例已处理，见 ``_setup_stdio()``）
  - ``\\n`` 被文本模式翻译成 ``\\r\\n``；
  - stdout 默认编码可能是 cp936 而非 UTF-8（中文标题会变成乱码）。
  协议 §2.2 容忍 CRLF，但**编码错误不容忍**——务必显式指定。

手工调试：``python dd_ext_pymin.py``，然后手工把 NDJSON 打进 stdin：

    {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":"1.0"}}
"""
import json
import sys
import datetime

PROVIDER_ID = "com.example.pymin"
DISPLAY_NAME = "PyMin 示例"
PROTOCOL_VERSION = "1.0"

# 声明要用到的 host/* 能力（协议 §7.4：**未声明的会被宿主回 -32601**）
CAPABILITIES = ["host/show_status", "host/set_clipboard", "host/open_url"]

DOC_URL = "https://github.com/Yang-SH/dd-run/blob/main/docs/extensions.md"

# 进程内状态：便签列表（仅演示用，进程退出即丢——不落盘）
_state = {"notes": []}
# 扩展自己的 id 空间（协议 §3.3：宿主与扩展各自从 1 开始，互不相干）
_own_id = 1


# ── 传输层 ────────────────────────────────────────────────────────────

def _setup_stdio():
    """把 stdout 固定为「UTF-8 + 不做换行翻译」，绕开两个 Windows 陷阱。"""
    try:
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
        sys.stdin.reconfigure(encoding="utf-8")
    except (AttributeError, OSError):  # Python < 3.7 或非常规管道
        pass


def send(obj):
    """发出一条协议消息（紧凑单行 JSON + \\n + flush）。"""
    sys.stdout.write(json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def log(message):
    """诊断日志——**只走 stderr**（协议 §2.5：stdout 不得出现非协议内容）。"""
    print(f"[pymin] {message}", file=sys.stderr, flush=True)


def reply(mid, result):
    send({"jsonrpc": "2.0", "id": mid, "result": result})


def reply_error(mid, code, message, data=None):
    err = {"code": code, "message": message}
    if data is not None:
        err["data"] = data
    send({"jsonrpc": "2.0", "id": mid, "error": err})


def notify(method, params=None):
    """通知：**无 ``id`` 字段**，接收方不得回复（协议 §3.1）。"""
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    send(msg)


def host_call(method, params):
    """反向请求宿主（``host/*``，协议 §7）。

    **不阻塞等应答**：宿主会按消息到达顺序处理并回包，我方在 read loop 中
    忽略对应 ``id`` 的响应即可（协议 §14 的示例正是这种时序——扩展的
    ``host/set_clipboard`` 早于 ``invoke`` 的 result 到达宿主）。
    """
    global _own_id
    mid = _own_id
    _own_id += 1
    log(f"→ host {method}（我方 id={mid}）")
    send({"jsonrpc": "2.0", "id": mid, "method": method, "params": params})


# ── 数据模型（协议 §8） ───────────────────────────────────────────────

def _item(cid, title, subtitle=None, section="PyMin 示例", command=None, **extra):
    item = {"id": cid, "title": title, "section": section, "command": command or {"kind": "invoke"}}
    if subtitle:
        item["subtitle"] = subtitle
    item.update(extra)
    return item


def top_level_commands():
    """§6.1 首屏命令。注意 ``command.kind`` 只有两种：``invoke`` / ``page``。"""
    return [
        _item("pymin.now", "PyMin：当前时间", "Host 往返演示（Toast + 剪贴板）",
              tags=["python", "demo"], text_to_suggest="pymin "),
        _item("pymin.notes", "PyMin：便签列表", "嵌套页演示（get_items + 搜索过滤）",
              # icon 为可选字段；glyph 取 Segoe Fluent Icons 码位（U+E721 = Search）
              icon={"type": "glyph", "value": "\uE721"},
              command={"kind": "page", "page_id": "pymin.notes"}),
        _item("pymin.note.add", "PyMin：添加便签", "取 context.query 作为内容，并广播 items_changed"),
        _item("pymin.wipe", "PyMin：清空便签", "Confirm 二次确认演示"),
        _item("pymin.web", "PyMin：打开扩展文档", "host/open_url 演示"),
    ]


def fallback_commands():
    """§6.2 兜底模板：``title`` 里的 ``{query}`` 由宿主替换为当前搜索词。

    **返回值非空**即表示本扩展具备兜底能力 → 宿主按 ``has_fallback`` 视为
    fresh（不落磁盘桩）。返回空数组表示「无兜底能力」。
    """
    return [
        _item("pymin.note.fromquery", "PyMin：添加便签「{query}」",
              "把当前搜索词存为便签", section="PyMin 兜底"),
    ]


def get_command(cid):
    """§6.4 按 id 取回真实命令（桩复热用）。找不到返回 ``None`` → ``command: null``。

    **只覆盖顶层命令**：兜底模板 id 与嵌套页项 id 不在此列（宿主对前者已
    不做校验，见 `docs/extensions.md` §"兜底为什么绕开 get_command"）。
    """
    for item in top_level_commands():
        if item["id"] == cid:
            return item
    return None


def get_items(page_id, search_text):
    """§6.3 按页全量拉取（**禁止**增量/流式推送）。"""
    if page_id != "pymin.notes":
        raise LookupError(page_id)
    notes = _state["notes"]
    if search_text:
        needle = search_text.lower()
        notes = [n for n in notes if needle in n.lower()]
    items = [
        _item(f"pymin.note.{index}", text, f"便签 #{index + 1}", section="便签")
        for index, text in enumerate(notes)
    ]
    return {"items": items, "has_more_items": False, "is_loading": False}


# ── invoke（协议 §6.5） ───────────────────────────────────────────────

def _result(kind, **args):
    res = {"kind": kind}
    if args:
        res["args"] = args
    return res


def handle_invoke(params):
    """返回 ``CommandResult``（§8.3）；返回 ``None`` 表示命令不存在 → ``-32002``。"""
    cid = params.get("id")
    context = params.get("context") or {}
    query = (context.get("query") or "").strip()

    if cid == "pymin.now":
        now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        host_call("host/show_status",
                  {"message": f"现在是 {now}", "state": "info", "duration_ms": 2500})
        host_call("host/set_clipboard", {"text": now})
        return _result("ShowToast", message=f"= {now}", duration_ms=2500)

    if cid == "pymin.note.add":
        return _add_note(query or "（空白便签）")

    if cid == "pymin.note.fromquery":  # 兜底模板：同样从 context.query 取内容
        return _add_note(query or "（空白便签）")

    if cid == "pymin.wipe":
        if context.get("confirmed"):
            count = len(_state["notes"])
            _state["notes"].clear()
            # 列表变了 → 通知宿主重拉该页（§7.1；宿主随后自行 get_items）
            notify("items_changed", {"page_id": "pymin.notes"})
            return _result("ShowToast", message=f"已清空 {count} 条便签", duration_ms=2000)
        # 未确认：请求宿主弹二次确认；确认后宿主会**重发 invoke** 并带上
        # context.confirmed = true（协议 §8.3 脚注）
        return _result("Confirm", title="清空全部便签？", description="此操作不可撤销。",
                       confirm_label="清空", is_critical=True)

    if cid == "pymin.web":
        host_call("host/open_url", {"url": DOC_URL})
        return _result("Dismiss")

    if isinstance(cid, str) and cid.startswith("pymin.note."):
        # 嵌套页里的便签项（sender=list_item）：按 id 末段索引定位
        try:
            index = int(cid.rsplit(".", 1)[1])
            text = _state["notes"][index]
        except (ValueError, IndexError):
            return None
        host_call("host/set_clipboard", {"text": text})
        return _result("ShowToast", message=f"已复制便签：{text}", duration_ms=2000)

    return None


def _add_note(text):
    _state["notes"].append(text)
    notify("items_changed", {"page_id": "pymin.notes"})
    return _result("ShowToast", message=f"已添加便签：{text}", duration_ms=2000)


# ── 请求分派 ──────────────────────────────────────────────────────────

def version_supported(requested):
    """§5.3 版本协商：只接受主版本相同且次版本不高于自己的请求。"""
    try:
        major, minor = (int(part) for part in str(requested).split(".")[:2])
    except (TypeError, ValueError):
        return False
    return major == 1 and minor <= 0


def handle(msg):
    """处理一条对端消息；返回 ``"exit"`` 表示可以退出了。"""
    mid = msg.get("id")
    method = msg.get("method")
    params = msg.get("params") or {}

    if method is None:
        # 无 method = 对端对我方 host/* 请求的响应 → 记录后忽略（§3.3）
        log(f"← host 应答 id={mid}（已忽略）")
        return None
    if mid is None:
        log(f"← 收到通知 {method}（无人会回复它，按 §3.1 直接丢弃）")
        return None

    try:
        if method == "initialize":
            if not version_supported(params.get("protocol_version")):
                reply_error(mid, -32004, "Unsupported protocol version",
                            {"requested": params.get("protocol_version"),
                             "supported_versions": [PROTOCOL_VERSION]})
                return "exit"
            reply(mid, {
                "protocol_version": PROTOCOL_VERSION,
                "provider": {
                    "id": PROVIDER_ID,
                    "display_name": DISPLAY_NAME,
                    # 顶层命令含动态内容（便签数）→ 不声明可缓存
                    "frozen": False,
                    # 兜底非空 → 宿主视为 fresh（§6.2）
                    "has_fallback": True,
                },
                "capabilities": CAPABILITIES,
            })
            # 可选的「已就绪」通知（§5.2）——宿主持有 initialize 的 result 即视为就绪
            notify("initialized", {})
            return None

        if method == "top_level_commands":
            reply(mid, {"commands": top_level_commands()})
            return None

        if method == "fallback_commands":
            reply(mid, {"commands": fallback_commands()})
            return None

        if method == "get_items":
            page_id = params.get("page_id")
            if not isinstance(page_id, str):
                reply_error(mid, -32602, "Invalid params", {"missing": ["page_id"]})
                return None
            try:
                reply(mid, get_items(page_id, params.get("search_text")))
            except LookupError:
                reply_error(mid, -32005, "page_not_found", {"page_id": page_id})
            return None

        if method == "get_command":
            cid = params.get("id")
            reply(mid, {"command": get_command(cid) if isinstance(cid, str) else None})
            return None

        if method == "invoke":
            result = handle_invoke(params)
            if result is None:
                reply_error(mid, -32002, "command_not_found", {"id": params.get("id")})
            else:
                # §6.5：JSON-RPC 的 `result` 字段**就是** CommandResult 本体。
                # ⚠️ 不要再包一层 `{"result": ...}`——宿主解开信封后直接按
                # CommandResult 解析，多包一层会报 `missing field \`kind\``。
                reply(mid, result)
            return None

        if method == "close":
            reply(mid, {})          # §6.6：回 result 后应尽快自行退出（建议 ≤ 1s）
            return "exit"

        reply_error(mid, -32601, "Method not found", {"method": method})
        return None

    except Exception as exc:  # noqa: BLE001 —— 任何内部异常都必须回 -32603 而不是崩溃
        log(f"内部异常：{exc!r}")
        reply_error(mid, -32603, "Internal error", {"detail": str(exc)})
        return None


def main():
    _setup_stdio()
    log(f"启动：{PROVIDER_ID}（Python {sys.version.split()[0]}）")
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue                      # §2.2 规则 5：空行忽略
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError as exc:
            reply_error(None, -32700, "Parse error", {"detail": str(exc)})
            continue
        if isinstance(msg, list):
            reply_error(None, -32600, "Invalid Request", {"detail": "batch not supported"})
            continue                      # §3.4 不支持批处理
        if handle(msg) == "exit":
            break
    log("退出")


if __name__ == "__main__":
    main()
