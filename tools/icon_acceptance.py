"""A-IC 文件图标验收（v3.6，2026-09-19）——可复现、可机读、带判定。

把 2026-09-19 落地时的一次性探针固化为**验收工具**：直驱 release 版
`dd-ext-search.exe`（协议 JSON-RPC over NDJSON，不经 GUI），逐项判定
`docs/search-file.md` §10.2–§10.3 的图标口径。

判据（与 `docs/search-file-ctrl-f-icons-plan.md` §6.2 一致）：

| 编号 | 判据 | 通过线 |
|---|---|---|
| A-IC-01 | 同扩展名共图、跨类型异图（键 → 图标文件 → 内容指纹三元组对齐） | 全部成立 |
| A-IC-02a | 缓存命中（同查询重复）单次 `icon_ms` | ≤ 2 ms / 30 条 |
| A-IC-02b | 首次·按扩展名（每查询 1–3 个新键） | ≤ 40 ms（观察线 60 ms） |
| A-IC-02c | 首次·按真实路径（`.exe` 30 个新键，**已知未达标项 E1**） | ≤ 40 ms（不达标即 FAIL 并如实报告） |
| A-IC-04 | 不可访问路径回落类别 glyph 且不崩（`kind=results`） | 有回落且无异常 |
| A-IC-06 | sidecar 体积增量 vs 变更前基线 | ≤ 64 KB（**已知未达标项 E2**） |

用法（**必须**用带 psutil 的隔离 venv 解释器——`Ext` 顶层 import psutil）：

    ~/.workbuddy/binaries/python/envs/default/Scripts/python.exe tools/icon_acceptance.py

可选参数：`--ext <sidecar.exe>`、`--out <json>`、`--baseline-bytes <N>`、`--budget-bytes <N>`、
`--cold`（**清空 `file-icons` 缓存后再测**——`02b`/`02c` 的「首抽」语义只有冷缓存才成立；
实测教训：热缓存下 `ext:exe` 从 703.8 ms 降到 98.7 ms，会把「首抽」误报成达标）。

退出码：0 = 全部通过；1 = 有 FAIL；2 = 环境未就绪（Everything/`es.exe` 均不可用 → 查询返回引导项）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sys
from collections import OrderedDict

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from search_acceptance import Ext, parse_timing  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_EXE = REPO / "target" / "x86_64-pc-windows-gnu" / "release" / "dd-ext-search.exe"
DEFAULT_OUT = REPO / "target" / "acceptance" / "icon-acceptance.json"
# 变更前基线（2026-09-19 改造前的 release 产物实测值）
BASELINE_BYTES_DEFAULT = 769_536
BUDGET_BYTES_DEFAULT = 64 * 1024

SELF_ICON_EXTS = ("exe", "lnk", "msi", "url")
HIT_MS = 2.0
FIRST_MS = 40.0
WATCH_MS = 60.0


def md5(path: pathlib.Path) -> str:
    try:
        return hashlib.md5(path.read_bytes()).hexdigest()[:12]
    except OSError:
        return "?"


def cache_dir() -> pathlib.Path:
    return pathlib.Path(os.environ.get("APPDATA", "")) / "dd-run" / "cache" / "file-icons"


def run_query(ext: "Ext", q: str) -> dict:
    before = len(parse_timing(ext.stderr_snapshot()))
    msg, ms, kind = ext.get_items(q, timeout=30.0)
    timings = parse_timing(ext.stderr_snapshot())[before:]
    items = (msg.get("result") or {}).get("items") or []
    rows = []
    for it in items:
        ic = it.get("icon") or {}
        icon_file = ic.get("value") if ic.get("type") == "path" else None
        p = pathlib.Path(it.get("subtitle") or "")
        rows.append(
            {
                "title": it.get("title"),
                "path": it.get("subtitle"),
                "on_disk": "dir" if p.is_dir() else ("file" if p.is_file() else "missing"),
                "icon_type": ic.get("type"),
                "icon_file": pathlib.Path(icon_file).name if icon_file else None,
                "icon_md5": md5(pathlib.Path(icon_file)) if icon_file else None,
            }
        )
    return {
        "query": q,
        "kind": kind,
        "roundtrip_ms": round(ms, 2),
        "icon_ms": [t.get("icon_ms") for t in timings],
        "channel": [t.get("channel") for t in timings],
        "items": len(rows),
        "icon_types": {k: sum(1 for r in rows if r["icon_type"] == k) for k in ("path", "glyph")},
        "on_disk": {k: sum(1 for r in rows if r["on_disk"] == k) for k in ("file", "dir", "missing")},
        "groups": group_by_key(rows),
        "rows": rows,
    }


def group_by_key(rows: list[dict]) -> dict:
    """「缓存键 → 图标文件|指纹」分组（只看磁盘上确为文件的条目）。

    键的期望语义与 `dd-ext/src/bin/search.rs::icon_cache_key_for` 一致：
    目录 → `dir`；`.exe`/`.lnk`/`.msi`/`.url` → 真实路径；其余 → 扩展名共图。
    """
    groups: "OrderedDict[str, set]" = OrderedDict()
    for r in rows:
        if r["on_disk"] != "file":
            continue
        name = (r["title"] or "").rsplit(".", 1)
        suffix = name[1].lower() if len(name) == 2 else "(none)"
        key = f"path#{r['path']}" if suffix in SELF_ICON_EXTS else f"ext#{suffix}"
        groups.setdefault(key, set()).add(f"{r['icon_file']}|{r['icon_md5']}")
    return {k: sorted(v) for k, v in groups.items()}


def one_ms(q: dict) -> float | None:
    vals = [v for v in q["icon_ms"] if v is not None]
    return max(vals) if vals else None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ext", default=str(DEFAULT_EXE))
    ap.add_argument("--out", default=str(DEFAULT_OUT))
    ap.add_argument("--baseline-bytes", type=int, default=BASELINE_BYTES_DEFAULT)
    ap.add_argument("--budget-bytes", type=int, default=BUDGET_BYTES_DEFAULT)
    ap.add_argument(
        "--cold",
        action="store_true",
        help="先清空 file-icons 缓存（仅限该目录），使「首抽」判据成立",
    )
    args = ap.parse_args()

    exe = pathlib.Path(args.ext)
    rep: dict = {"exe": str(exe), "cold": bool(args.cold), "checks": [], "queries": []}

    cd = cache_dir()
    if args.cold and cd.is_dir():
        # 安全性：只清「本工具自己的」缓存目录（目录名精确匹配，可再生成）
        assert cd.name == "file-icons", cd
        removed = 0
        for f in cd.glob("*.png"):
            f.unlink()
            removed += 1
        rep["cold_removed"] = removed
        print(f"[info] --cold：已清空 {removed} 个缓存 PNG（{cd}）")

    def check(cid: str, ok: bool, detail: str) -> None:
        rep["checks"].append({"id": cid, "pass": bool(ok), "detail": detail})

    ext = Ext(exe)
    ext.initialize()
    # 按扩展名档（第 2 次为缓存命中）→ 按真实路径档（第 2 次为命中）→ 回落档
    for q in ("ext:rs", "ext:rs", "ext:pdf", "ext:png", "ext:zip", "ext:exe", "ext:exe", "ext:lnk"):
        rep["queries"].append(run_query(ext, q))
    ext.stop()

    q_rs_first, q_rs_hit, q_pdf, q_png, q_zip, q_exe_first, q_exe_hit, q_lnk = rep["queries"]

    if q_rs_first["kind"] != "results":
        rep["env"] = "not_ready"
        pathlib.Path(args.out).write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"环境未就绪：Everything 与 es.exe 均不可用（kind={q_rs_first['kind']}）→ 跳过（rc=2）")
        return 2

    # A-IC-01：同扩展名共图（rs/pdf/png/zip 各 1 张图），且四类互异
    ext_icons = {}
    for q in (q_rs_hit, q_pdf, q_png, q_zip):
        key, val = next(iter(q["groups"].items()))
        ext_icons[key] = val
    same_ext_single = all(len(v) == 1 for v in ext_icons.values())
    distinct_md5 = {v[0].split("|")[1] for v in ext_icons.values()}
    check(
        "A-IC-01",
        same_ext_single and len(distinct_md5) >= 3,
        f"同扩展名共图={same_ext_single}；跨类型不同指纹={len(distinct_md5)}/{len(ext_icons)}"
        f"（{', '.join(ext_icons)}）",
    )

    # A-IC-02a：缓存命中 ≤ 2ms
    hit = one_ms(q_rs_hit)
    check("A-IC-02a", hit is not None and hit <= HIT_MS, f"缓存命中 icon_ms={hit} ms（判据 ≤ {HIT_MS}）")

    # A-IC-02b：按扩展名首抽（取 rs/pdf/png/zip 首次值中的最大值）
    firsts = [one_ms(x) for x in (q_rs_first, q_pdf, q_png, q_zip)]
    worst = max(v for v in firsts if v is not None)
    check(
        "A-IC-02b",
        worst <= FIRST_MS,
        f"按扩展名首抽 max={worst:.2f} ms（判据 ≤ {FIRST_MS}，观察线 {WATCH_MS}；各档 {firsts}）",
    )

    # A-IC-02c：按真实路径首抽（30 新键）——已知未达标项 E1，按实测报告
    e1 = one_ms(q_exe_first)
    check(
        "A-IC-02c",
        e1 is not None and e1 <= FIRST_MS,
        f"按真实路径首抽（30 新键）icon_ms={e1} ms（判据 ≤ {FIRST_MS}）"
        + ("　← 未达标 E1，处置选项见 docs/search-file.md §10.4" if e1 and e1 > FIRST_MS else ""),
    )

    # A-IC-04：不可访问路径回落类别 glyph 且不崩
    lnk_glyph = q_lnk["icon_types"]["glyph"]
    check(
        "A-IC-04",
        q_lnk["kind"] == "results" and lnk_glyph > 0 and q_lnk["icon_types"]["path"] > 0,
        f".lnk 批：path={q_lnk['icon_types']['path']} / glyph={lnk_glyph}（回落生效且 kind={q_lnk['kind']}）",
    )

    # A-IC-06：体积增量
    size = exe.stat().st_size if exe.is_file() else 0
    delta = size - args.baseline_bytes
    check(
        "A-IC-06",
        delta <= args.budget_bytes,
        f"sidecar={size} B，基线={args.baseline_bytes} B，增量={delta} B（预算 {args.budget_bytes} B）"
        + ("　← 未达标 E2" if delta > args.budget_bytes else ""),
    )

    cd = cache_dir()
    if not args.cold:
        rep["warm_cache_note"] = "未指定 --cold：A-IC-02b/02c 可能读到已缓存键，非「首抽」口径"
        print("[info] 未指定 --cold：02b/02c 反映的是当前缓存状态，非「首抽」口径")
    if cd.is_dir():
        files = sorted(cd.glob("*.png"))
        rep["cache"] = {
            "dir": str(cd),
            "files": len(files),
            "bytes": sum(f.stat().st_size for f in files),
        }

    failed = [c for c in rep["checks"] if not c["pass"]]
    rep["summary"] = {"total": len(rep["checks"]), "failed": len(failed)}
    pathlib.Path(args.out).write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")

    width = max(len(c["id"]) for c in rep["checks"])
    for c in rep["checks"]:
        print(f"[{'PASS' if c['pass'] else 'FAIL'}] {c['id']:<{width}}  {c['detail']}")
    if "cache" in rep:
        print(f"[info] 图标缓存：{rep['cache']['files']} 文件 / {rep['cache']['bytes']} B（{rep['cache']['dir']}）")
    print(f"[info] 证据：{args.out}　失败 {len(failed)}/{len(rep['checks'])} 项")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
