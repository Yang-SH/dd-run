#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dd-run 文件搜索（Everything / P2 传输层）真机验收测量脚本。

用途：为 `docs/search-file.md` §7.4 的 A-33-05 / A-33-06 / A-33-07 / A-33-10
采集**可复现**的测量证据，输出 JSON + CSV 到 `--out`（默认 `target/acceptance/`）。

设计要点
--------
- 直接按协议（JSON-RPC 2.0 over NDJSON，§2.2）驱动 sidecar `dd-ext-search.exe`，
  **不经过 GUI**，避免宿主调度与渲染开销混入「扩展进程内查询」计时。
- 计时口径 = **写入请求行 → 读到匹配 id 的响应行**（含 stdin/stdout 管道往返，约 0.1ms）。
  该值 **≥** 扩展进程内查询耗时；故「本值 p95 < 30ms」可推出 A-33-05 的进程内 p95 达标
  （用上界达标证明内层达标）。
- 子进程资源（RSS / 线程数 / 句柄数）用 psutil 采样 → A-33-07 的时间序列。
- stderr 用独立线程按**到达时刻**打戳收集 → A-33-10「通道切换日志时间戳」的证据来源。
- **不调用 PowerShell**（本机安全策略拦截 bash/Python → PowerShell 的调用路径）。

用法
----
    python tools/search_acceptance.py env                  # 打印环境与矩阵输入
    python tools/search_acceptance.py bench -n 1000        # A-33-05(IPC 半) + A-33-07
    python tools/search_acceptance.py fault -n 20          # A-33-06(部分) + A-33-10

注意：`fault` 会**停止并重启 Everything 桌面实例**（服务实例不动），测完自动恢复。
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PAGE_ID = "files.results"
# 信封的 jsonrpc 恒为 "2.0"（§2.1）；协议版本 "1.0" 走 initialize.params.protocol_version，两者不可混用。
JSONRPC_VERSION = "2.0"
PROTOCOL_VERSION = "1.0"
MAX_MESSAGE_BYTES = 1024 * 1024
EVERYTHING_EXE = Path(r"C:\Program Files\Everything\Everything.exe")
# 启动 Everything 桌面实例时用于尽量脱离父进程作业对象（沙箱会在命令结束时回收进程树）
CREATE_BREAKAWAY_FROM_JOB = 0x01000000
DETACHED_PROCESS = 0x00000008

try:
    import psutil  # type: ignore
except Exception:  # pragma: no cover - 无 psutil 时降级为仅计时
    psutil = None


# ─── 环境探测 ────────────────────────────────────────────────────────
def find_extension_exe(explicit: str | None) -> Path:
    """定位 dd-ext-search 产物：显式路径 → release → debug → dist。"""
    cands: list[Path] = []
    if explicit:
        cands.append(Path(explicit))
    triple = "x86_64-pc-windows-gnu"
    suffix = ".exe" if os.name == "nt" else ""
    cands += [
        REPO / "target" / triple / "release" / f"dd-ext-search{suffix}",
        REPO / "target" / triple / "debug" / f"dd-ext-search{suffix}",
        REPO / "dist" / "extensions.d" / f"dd-ext-search{suffix}",
    ]
    for c in cands:
        if c.is_file():
            return c
    raise SystemExit(
        "未找到 dd-ext-search 产物。请先运行：\n"
        "  cargo +stable-x86_64-pc-windows-gnu build --release -p dd-ext\n"
        "或用 --ext 指定路径。"
    )


def find_es_exe() -> Path | None:
    """复刻扩展内部的三级定位（§9.2 P2）：DDRUN_ES_PATH → PATH → DDRUN_EVERYTHING_DIR。"""
    env_path = os.environ.get("DDRUN_ES_PATH")
    if env_path and Path(env_path).is_file():
        return Path(env_path)
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if d:
            p = Path(d) / "es.exe"
            if p.is_file():
                return p
    every_dir = os.environ.get("DDRUN_EVERYTHING_DIR")
    if every_dir:
        p = Path(every_dir) / "es.exe"
        if p.is_file():
            return p
    for guess in (r"C:\Program Files\Everything\es.exe", r"C:\Program Files (x86)\Everything\es.exe"):
        if Path(guess).is_file():
            return Path(guess)
    return None


def _session_of(pid: int) -> int | None:
    """进程所属会话 ID（0 = 服务会话）。psutil 7.x 已无 `session_id()`，故走 Win32 API。"""
    try:
        sid = ctypes.c_uint32()
        ok = ctypes.windll.kernel32.ProcessIdToSessionId(  # type: ignore[attr-defined]
            ctypes.c_uint32(pid), ctypes.byref(sid)
        )
        return int(sid.value) if ok else None
    except Exception:
        return None


def everything_procs() -> list[dict]:
    """列出 Everything.exe 进程；session 0 = Everything 服务实例，== 当前会话 = 桌面实例。"""
    out: list[dict] = []
    if psutil is None:
        return out
    for p in psutil.process_iter(["pid", "name"]):
        try:
            if (p.info.get("name") or "").lower() != "everything.exe":
                continue
        except Exception:
            continue
        pid = p.info["pid"]
        try:
            rss = p.memory_info().rss
        except Exception:
            rss = None
        out.append({"pid": pid, "session": _session_of(pid), "rss": rss})
    return sorted(out, key=lambda d: d["pid"])


def our_session() -> int | None:
    return _session_of(os.getpid())


def windows_version() -> str:
    import platform

    rel, ver, _, _ = platform.win32_ver()
    return f"Windows {rel} ({ver})"


def classify(msg: dict) -> str:
    """`get_items` 响应归类：guide（Everything 不可用）/ hint（空查询）/ error / results / empty。"""
    if "error" in msg:
        return "error"
    items = (msg.get("result") or {}).get("items") or []
    if not items:
        return "empty"
    ids = [str(i.get("id", "")) for i in items]
    if any(i.startswith("files.guide") for i in ids):
        return "guide"
    if any(i.startswith("files.hint") for i in ids):
        return "hint"
    if any(i.startswith("files.error") for i in ids):
        return "error"
    if any(i.startswith("files.open") for i in ids):
        return "results"
    return "other"


# ─── 协议驱动 ────────────────────────────────────────────────────────
class Ext:
    """最小协议客户端：只实现验收需要的 initialize / get_items。"""

    def __init__(self, exe: Path, log_level: str = "debug") -> None:
        env = dict(os.environ)
        env["DDRUN_LOG"] = log_level
        env.setdefault("DDRUN_LANG", "zh-CN")
        self.proc = subprocess.Popen(
            [str(exe)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        self.stderr_lines: list[tuple[float, str]] = []
        self._t0 = time.perf_counter()
        self._id = 0
        self._lock = threading.Lock()
        threading.Thread(target=self._drain_err, daemon=True).start()
        self.ps = psutil.Process(self.proc.pid) if psutil is not None else None

    def _drain_err(self) -> None:
        assert self.proc.stderr is not None
        for raw in iter(self.proc.stderr.readline, b""):
            line = raw.decode("utf-8", "replace").rstrip("\r\n")
            if line:
                with self._lock:
                    self.stderr_lines.append((time.perf_counter() - self._t0, line))

    def call(self, method: str, params: dict, timeout: float = 5.0) -> tuple[dict, float]:
        assert self.proc.stdin is not None and self.proc.stdout is not None
        self._id += 1
        want = self._id
        payload = json.dumps(
            {"jsonrpc": JSONRPC_VERSION, "id": want, "method": method, "params": params},
            ensure_ascii=False,
            separators=(",", ":"),
        )
        t0 = time.perf_counter()
        self.proc.stdin.write(payload.encode("utf-8") + b"\n")
        self.proc.stdin.flush()
        deadline = t0 + timeout
        while True:
            if time.perf_counter() > deadline:
                raise TimeoutError(f"{method} 无响应（>{timeout}s）")
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError(f"{method} 时扩展 stdout 关闭（rc={self.proc.poll()}）")
            t1 = time.perf_counter()
            try:
                msg = json.loads(line.decode("utf-8"))
            except Exception:
                continue
            if msg.get("id") == want:  # 按 id 严格匹配，超时请求的迟到响应会被跳过
                return msg, (t1 - t0) * 1000.0

    def initialize(self) -> dict:
        params = {
            "protocol_version": PROTOCOL_VERSION,
            "host": {"name": "dd-run", "version": "acceptance-harness", "platform": "windows"},
            "transport": {"framing": "ndjson", "max_message_bytes": MAX_MESSAGE_BYTES},
            "capabilities": ["host/open_url", "host/show_status", "host/set_clipboard"],
            "locale": "zh-CN",
        }
        msg, _ = self.call("initialize", params)
        return msg

    def get_items(self, query: str, timeout: float = 5.0) -> tuple[dict, float, str]:
        msg, ms = self.call("get_items", {"page_id": PAGE_ID, "search_text": query}, timeout)
        return msg, ms, classify(msg)

    def metrics(self) -> dict:
        if self.ps is None:
            return {}
        try:
            mi = self.ps.memory_info()
            return {
                "rss": mi.rss,
                "vms": mi.vms,
                "threads": self.ps.num_threads(),
                "handles": self.ps.num_handles(),
            }
        except Exception:
            return {}

    def stderr_snapshot(self) -> list[dict]:
        with self._lock:
            return [{"t": round(t, 3), "line": s} for t, s in self.stderr_lines]

    def alive(self) -> bool:
        return self.proc.poll() is None

    def stop(self) -> None:
        try:
            if self.proc.poll() is None:
                if self.proc.stdin:
                    self.proc.stdin.close()
                self.proc.wait(timeout=3)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


# ─── 统计 ────────────────────────────────────────────────────────────
def first_item_text(msg: dict) -> str | None:
    """取首个结果项的 title/subtitle（用于把 `error` 归类落到具体文案，便于定因）。"""
    items = (msg.get("result") or {}).get("items") or []
    if not items:
        return None
    it = items[0]
    return f"{it.get('title', '')} | {it.get('subtitle', '')}".strip(" |")


def pct(values: list[float], p: float) -> float:
    if not values:
        return float("nan")
    xs = sorted(values)
    k = min(len(xs) - 1, max(0, int(round((p / 100.0) * (len(xs) - 1)))))
    return xs[k]


def summarize(values: list[float]) -> dict:
    if not values:
        return {}
    return {
        "n": len(values),
        "min": round(min(values), 3),
        "p50": round(statistics.median(values), 3),
        "p95": round(pct(values, 95), 3),
        "p99": round(pct(values, 99), 3),
        "max": round(max(values), 3),
        "mean": round(statistics.fmean(values), 3),
    }


# 查询样本：覆盖短词 / 文件名 / 长词 / 通配 / 中文（`%`/`#` 见 A-33-03，随 §4 一并验证）
QUERIES = [
    "cargo", "main.rs", "test", "config", "readme", "target", "node_modules", "dd-run",
    "search_file", "Everything", "*.rs", "lib", "c", "a", "报告", "src",
]


# ─── env：环境与矩阵输入 ─────────────────────────────────────────────
def cmd_env(args: argparse.Namespace) -> int:
    exe = find_extension_exe(args.ext)
    es = find_es_exe()
    info = {
        "os": windows_version(),
        "python": sys.version.split()[0],
        "psutil": getattr(psutil, "__version__", None),
        "extension_exe": str(exe),
        "extension_mtime": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(exe.stat().st_mtime)),
        "es_exe": str(es) if es else None,
        "everything_procs": everything_procs(),
        "everything_exe": str(EVERYTHING_EXE) if EVERYTHING_EXE.is_file() else None,
    }
    print(json.dumps(info, ensure_ascii=False, indent=2))
    return 0


# ─── breakdown：耗时分解（探活 / IPC / 评分）─────────────────────────
def cmd_breakdown(args: argparse.Namespace) -> int:
    """把单次 `get_items` 的耗时拆到「探活 / IPC 往返 / 评分」三段，定位性能瓶颈。

    - 空查询：`everything_available()` 仍会执行（求值在调用前），但直接返回提示项
      → 该值即「探活 + 协议往返」的地板；
    - 无命中查询：走完整 IPC 往返，结果集为空 → IPC 地板（不含评分）；
    - 命中查询：IPC + skim 评分 + 排序 → 评分增量 ≈ 命中 − 无命中。
    """
    exe = find_extension_exe(args.ext)
    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)
    ext = Ext(exe)
    try:
        init = ext.initialize()
        if "result" not in init:
            raise SystemExit(f"initialize 失败：{json.dumps(init, ensure_ascii=False)}")
        for _ in range(args.warmup):
            ext.get_items("cargo")

        cases = [
            ("empty_query_probe_only", ""),
            ("ipc_no_match", "zzzqqqxxx_no_such_thing_999999"),
            ("ipc_hit_narrow", "cargo"),
            ("ipc_hit_wide", "a"),
        ]
        result = {}
        for name, q in cases:
            lat = []
            kinds: dict[str, int] = {}
            for _ in range(args.n):
                try:
                    _, ms, kind = ext.get_items(q, timeout=args.timeout)
                except Exception as e:
                    kind, ms = f"exception:{type(e).__name__}", args.timeout * 1000.0
                lat.append(ms)
                kinds[kind] = kinds.get(kind, 0) + 1
            result[name] = {"query": q, "kinds": kinds, "latency_ms": summarize(lat)}
            print(f"[{name:22s}] {result[name]['latency_ms']} kinds={kinds}")
    finally:
        ext.stop()

    ipc = result["ipc_no_match"]["latency_ms"].get("p50", 0.0)
    probe = result["empty_query_probe_only"]["latency_ms"].get("p50", 0.0)
    hit = result["ipc_hit_narrow"]["latency_ms"].get("p50", 0.0)
    evidence = {
        "mode": "breakdown",
        "started": time.strftime("%Y-%m-%d %H:%M:%S"),
        "os": windows_version(),
        "extension_exe": str(exe),
        "n_per_case": args.n,
        "cases": result,
        # ⚠️ 口径说明（2026-09-16）：以下 derived 是**跨场景相减**的粗分，仅给出量级参考。
        # 实测证明「hit − ipc」**不等于**评分成本（评分实测仅 ≈0.3 ms）——该差值主要来自
        # IPC 查询耗时随查询词变化，详见 docs/search-file-p2-acceptance-2026-09-15.md §4.1.1。
        # 权威分段请读扩展自报的 `get_items 计时:` 行（search.rs 的 QueryTiming）。
        "derived": {
            "probe_plus_pipe_p50_ms": probe,
            "ipc_roundtrip_p50_ms": round(ipc - probe, 3),
            "hit_vs_nomatch_delta_p50_ms": round(hit - ipc, 3),
        },
        "stderr": ext.stderr_snapshot()[-20:],
    }
    (outdir / "breakdown.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(evidence["derived"], ensure_ascii=False, indent=2))
    print(f"[out] {outdir}/breakdown.json")
    return 0


# ─── bench：A-33-05（IPC 半）+ A-33-07 ────────────────────────────────
def cmd_bench(args: argparse.Namespace) -> int:
    exe = find_extension_exe(args.ext)
    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)
    ext = Ext(exe)
    lat: list[float] = []
    series: list[dict] = []
    rows: list[tuple] = []
    kinds: dict[str, int] = {}
    try:
        init = ext.initialize()
        if "result" not in init:
            raise SystemExit(f"initialize 失败：{json.dumps(init, ensure_ascii=False)}")
        init_class = "ok"
        cold = ext.metrics()
        for _ in range(args.warmup):
            ext.get_items("cargo")
        base = ext.metrics()  # 稳态基线：预热后取样，避免把首次分配算成「增长」
        for i in range(args.n):
            q = QUERIES[i % len(QUERIES)]
            try:
                _, ms, kind = ext.get_items(q, timeout=args.timeout)
            except Exception as e:
                kind, ms = f"exception:{type(e).__name__}", args.timeout * 1000.0
            lat.append(ms)
            kinds[kind] = kinds.get(kind, 0) + 1
            rows.append((i, q, round(ms, 3), kind))
            if i % max(1, args.sample_every) == 0 or i == args.n - 1:
                m = ext.metrics()
                if m:
                    series.append({"i": i, **m})
        end = ext.metrics()
        alive_after = ext.alive()
    finally:
        ext.stop()

    stats = summarize(lat)
    grew: dict[str, dict] = {}
    for k in ("rss", "threads", "handles"):
        b, e = (base or {}).get(k), (end or {}).get(k)
        if b:
            grew[k] = {"base": b, "end": e, "pct": round((e - b) / b * 100.0, 2)}
    cold_grew: dict[str, dict] = {}
    for k in ("rss", "threads", "handles"):
        c, e = (cold or {}).get(k), (end or {}).get(k)
        if c:
            cold_grew[k] = {"cold": c, "end": e, "pct": round((e - c) / c * 100.0, 2)}
    es = find_es_exe()
    verdict = {
        "a33_05_ipc_p50_lt_10ms": stats.get("p50", 9e9) < 10.0,
        "a33_05_ipc_p95_lt_30ms": stats.get("p95", 9e9) < 30.0,
        "a33_05_baseline_vs_es_p95_reduction_ge_50pct": (
            "blocked: es.exe 未安装，无法采 run_es 基线" if es is None else "todo"
        ),
        "a33_07_no_crash": alive_after and not any(k.startswith("exception") for k in kinds),
        "a33_07_rss_growth_lt_10pct": grew.get("rss", {}).get("pct", 1e9) < 10.0,
    }
    evidence = {
        "mode": "bench",
        "started": time.strftime("%Y-%m-%d %H:%M:%S"),
        "os": windows_version(),
        "extension_exe": str(exe),
        "exe_mtime": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(exe.stat().st_mtime)),
        "es_exe": str(es) if es else None,
        "everything_procs": everything_procs(),
        "init": init_class,
        "queries": args.n,
        "warmup": args.warmup,
        "result_kinds": kinds,
        "latency_ms": stats,
        "resource_series": series,
        "resources": {"cold": cold, "base": base, "end": end, "growth_steady": grew, "growth_cold": cold_grew},
        "verdict": verdict,
        "stderr": ext.stderr_snapshot()[-40:],
    }
    (outdir / "bench.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    (outdir / "bench-latency.csv").write_text(
        "i,query,ms,kind\n" + "\n".join(f"{i},{q},{ms},{k}" for i, q, ms, k in rows), encoding="utf-8"
    )
    (outdir / "bench-resources.csv").write_text(
        "i,rss,vms,threads,handles\n"
        + "\n".join(f"{s['i']},{s['rss']},{s['vms']},{s['threads']},{s['handles']}" for s in series),
        encoding="utf-8",
    )
    print(json.dumps({"latency_ms": stats, "kinds": kinds, "growth": grew, "verdict": verdict},
                     ensure_ascii=False, indent=2))
    print(f"[out] {outdir}/bench.json + bench-latency.csv + bench-resources.csv")
    return 0


def ensure_desktop_instance(ext: "Ext", timeout: float, interval: float) -> dict:
    """确保 Everything 桌面实例在运行且 IPC 可用（本项前置条件），必要时自行拉起。

    注意：本机沙箱会在命令结束时回收进程树，故脚本**不能**保证测完后 Everything 仍在运行
    —— 由调用方在报告里显式提示用户手动启动。
    """
    if any(p["session"] == our_session() for p in everything_procs()):
        return {"started": False, "reason": "桌面实例已在运行"}
    if not EVERYTHING_EXE.is_file():
        return {"started": False, "reason": f"未找到 {EVERYTHING_EXE}"}
    try:
        subprocess.Popen(
            [str(EVERYTHING_EXE)],
            creationflags=CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS,
        )
    except Exception as e:
        return {"started": False, "reason": f"启动失败：{type(e).__name__}: {e}"}
    t0 = time.perf_counter()
    while time.perf_counter() - t0 < timeout:
        try:
            _, _, kind = ext.get_items("cargo")
        except Exception as e:
            kind = f"exception:{type(e).__name__}"
        if kind in ("results", "empty", "hint"):
            return {"started": True, "ready_after_s": round(time.perf_counter() - t0, 2), "kind": kind}
        time.sleep(interval)
    return {"started": True, "ready_after_s": None, "note": f"{timeout}s 内未就绪"}


# ─── fault：A-33-06（部分）+ A-33-10 ──────────────────────────────────
def cmd_fault(args: argparse.Namespace) -> int:
    exe = find_extension_exe(args.ext)
    outdir = Path(args.out)
    outdir.mkdir(parents=True, exist_ok=True)
    es = find_es_exe()

    ext = Ext(exe)
    phases: list[dict] = []
    proc_after: list[dict] = []
    poll_rows: list[dict] = []
    recovered_at_n: int | None = None
    recovered_after_s: float | None = None
    stderr_all: list[dict] = []
    restart_note = ""
    stop_t = time.perf_counter()
    procs_after_restart: list[dict] = []
    try:
        init = ext.initialize()
        if "result" not in init:
            raise SystemExit(f"initialize 失败：{json.dumps(init, ensure_ascii=False)}")

        # 前置：桌面实例必须在运行（必要时自行拉起并等 IPC 就绪）
        ensured = ensure_desktop_instance(ext, args.recover_timeout, args.poll_interval)
        print(f"[ensure] {ensured}")
        gui = [p for p in everything_procs() if p["session"] == our_session()]
        if not gui:
            raise SystemExit(f"Everything 桌面实例不可用，无法执行故障注入：{ensured}")

        def run_phase(name: str, n: int, timeout: float) -> dict:
            lat: list[float] = []
            kinds: dict[str, int] = {}
            for i in range(n):
                try:
                    _, ms, kind = ext.get_items(QUERIES[i % len(QUERIES)], timeout=timeout)
                except Exception as e:
                    kind, ms = f"exception:{type(e).__name__}", timeout * 1000.0
                lat.append(ms)
                kinds[kind] = kinds.get(kind, 0) + 1
            rec = {"phase": name, "n": n, "kinds": kinds, "latency_ms": summarize(lat)}
            phases.append(rec)
            print(f"[{name}] kinds={kinds} {rec['latency_ms']}")
            return rec

        run_phase("A_before_stop", args.n, args.timeout)

        # 注入①：停止桌面实例（保留服务实例）——A-33-10 的「Everything 退出」
        for p in gui:
            try:
                psutil.Process(p["pid"]).terminate()
            except Exception as e:
                print(f"[warn] 停止 pid={p['pid']} 失败：{e}")
        time.sleep(args.settle)
        proc_after = everything_procs()
        run_phase("B_ipc_down", args.n, args.timeout)

        # 注入②：重启桌面实例 —— A-33-10 的「重启后自动回 IPC」
        restarted = False
        if EVERYTHING_EXE.is_file():
            try:
                # 沙箱/父进程退出可能带走子进程；breakaway 让新实例尽量脱离作业对象
                subprocess.Popen(
                    [str(EVERYTHING_EXE)],
                    creationflags=CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS,
                )
                restarted = True
            except Exception as e:
                restart_note = f"Popen 失败：{type(e).__name__}: {e}"
        else:
            restart_note = f"未找到 {EVERYTHING_EXE}"

        # 基于时间的恢复轮询（不能用「N 次查询」——引导项响应仅 0.1ms，N 次会瞬间跑完）
        t_restart = time.perf_counter()
        while time.perf_counter() - t_restart < args.recover_timeout:
            try:
                msg, ms, kind = ext.get_items("cargo", timeout=args.timeout)
            except Exception as e:
                msg, kind, ms = {}, f"exception:{type(e).__name__}", args.timeout * 1000.0
            elapsed = round(time.perf_counter() - t_restart, 2)
            row = {"n": len(poll_rows) + 1, "t": elapsed, "kind": kind, "ms": round(ms, 3)}
            if kind == "error":
                row["detail"] = first_item_text(msg)
            poll_rows.append(row)
            if kind in ("results", "empty", "hint") and recovered_at_n is None:
                recovered_at_n, recovered_after_s = len(poll_rows), elapsed
                break
            time.sleep(args.poll_interval)

        procs_after_restart = everything_procs()
        stderr_all = ext.stderr_snapshot()
        alive = ext.alive()
    finally:
        ext.stop()

    down = phases[1] if len(phases) > 1 else {"kinds": {}, "latency_ms": {}, "n": 0}
    churn_logs = [s for s in stderr_all if ("IPC" in s["line"] or "回落" in s["line"] or "重建" in s["line"])]
    evidence = {
        "mode": "fault",
        "started": time.strftime("%Y-%m-%d %H:%M:%S"),
        "os": windows_version(),
        "extension_exe": str(exe),
        "es_exe": str(es) if es else None,
        "ensure_desktop_instance": ensured,
        "everything_gui_pids_before": [p["pid"] for p in gui],
        "everything_procs_after_stop": proc_after,
        "everything_procs_after_restart": procs_after_restart,
        "restarted_by_harness": restarted,
        "restart_note": restart_note,
        "settle_s": args.settle,
        "recover_timeout_s": args.recover_timeout,
        "poll_interval_s": args.poll_interval,
        "stop_elapsed_s": round(time.perf_counter() - stop_t, 2),
        "phases": phases,
        "recovery_poll": poll_rows,
        "extension_alive_at_end": alive,
        "channel_logs": churn_logs,
        "verdict": {
            "a33_06_ipc_down_100pct_guide": down["kinds"].get("guide", 0) == down.get("n", 0)
            and down.get("n", 0) > 0,
            "a33_06_fallback_to_es_exe": (
                "blocked: es.exe 未安装，es 回落分支无法触发" if es is None else "todo"
            ),
            "a33_06_single_request_le_2000ms": down["latency_ms"].get("max", 9e9) <= 2000.0,
            "a33_10_ipc_recovered_after_queries": recovered_at_n,
            "a33_10_ipc_recovered_after_seconds": recovered_after_s,
            "a33_10_not_stuck_in_fallback": recovered_at_n is not None,
        },
    }
    (outdir / "fault.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(evidence["verdict"], ensure_ascii=False, indent=2))
    print("[recovery poll]", json.dumps(poll_rows[:12], ensure_ascii=False))
    print(f"[out] {outdir}/fault.json")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="文件搜索 P2 真机验收测量")
    sub = ap.add_subparsers(dest="mode", required=True)

    p = sub.add_parser("env", help="打印环境与矩阵输入")
    p.add_argument("--ext", default=None)
    p.set_defaults(func=cmd_env)

    p = sub.add_parser("bench", help="1000 次查询计时 + 资源序列（A-33-05 IPC 半 / A-33-07）")
    p.add_argument("-n", type=int, default=1000)
    p.add_argument("--warmup", type=int, default=5)
    p.add_argument("--sample-every", type=int, default=50)
    p.add_argument("--ext", default=None)
    p.add_argument("--out", default="target/acceptance")
    p.add_argument("--timeout", type=float, default=5.0)
    p.set_defaults(func=cmd_bench)

    p = sub.add_parser("breakdown", help="耗时分解：探活 / IPC 往返 / 评分")
    p.add_argument("-n", type=int, default=100)
    p.add_argument("--warmup", type=int, default=5)
    p.add_argument("--ext", default=None)
    p.add_argument("--out", default="target/acceptance")
    p.add_argument("--timeout", type=float, default=5.0)
    p.set_defaults(func=cmd_breakdown)

    p = sub.add_parser("fault", help="故障注入：停止/重启 Everything（A-33-06 部分 / A-33-10）")
    p.add_argument("-n", type=int, default=20)
    p.add_argument("--settle", type=float, default=8.0)
    p.add_argument("--recover-timeout", type=float, default=90.0)
    p.add_argument("--poll-interval", type=float, default=2.0)
    p.add_argument("--ext", default=None)
    p.add_argument("--out", default="target/acceptance")
    p.add_argument("--timeout", type=float, default=5.0)
    p.set_defaults(func=cmd_fault)

    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
