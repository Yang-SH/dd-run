"""E2E 首屏计时解析（P2-5，验收报告 §5 #4 的采样判读工具）。

输入 = dd-run 宿主 stderr 捕获日志（`dd-run.exe 2> gui.log` 或 debug 构建
控制台重定向），逐行匹配宿主埋点：

    [dd-gui] E2E 首屏: input→paint {total} ms（去抖/进页等待 {wait} + get_items 往返 {rtt} + 渲染 {paint}）；page=… query=… 字符 items=…

输出 = 每项指标（total/wait/rtt/paint）的 count/min/p50/p95/max + 对
「输入 → 首屏 ≤ 200 ms」门禁的判定（p95 < 200 → PASS）。退出码：0 全过 /
1 有超门禁 / 2 无样本或文件不可读。

用法：
    python tools/gui_e2e_parse.py gui.log [more.log ...] [--budget-ms 200]
"""
import argparse
import re
import statistics
import sys

LINE = re.compile(
    r"E2E 首屏: input→paint (?P<total>\d+) ms"
    r"（去抖/进页等待 (?P<wait>\d+) \+ get_items 往返 (?P<rtt>\d+) \+ 渲染 (?P<paint>\d+)）"
    r"；page=(?P<page>\S+) query=(?P<q>\d+) 字符 items=(?P<items>\d+)"
)

METRICS = ("total", "wait", "rtt", "paint")


def parse(paths: list[str]) -> list[dict]:
    samples = []
    for path in paths:
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                for ln, line in enumerate(fh, 1):
                    m = LINE.search(line)
                    if m:
                        d = m.groupdict()
                        d["src"] = f"{path}:{ln}"
                        samples.append(d)
        except OSError as exc:
            print(f"[error] 读不了 {path}：{exc}")
            sys.exit(2)
    return samples


def stats(values: list[int]) -> dict:
    vs = sorted(values)
    n = len(vs)

    def pct(p: float) -> int:
        # nearest-rank：ceil(p*n)-1（n=1 时恒取唯一值）
        idx = max(0, -(-int(p * 100 * n) // 100) - 1)
        return vs[min(idx, n - 1)]

    return {
        "n": n,
        "min": vs[0],
        "p50": pct(0.50),
        "p95": pct(0.95),
        "max": vs[-1],
        "mean": round(statistics.fmean(vs), 1),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("logs", nargs="+", help="宿主 stderr 捕获日志（一个或多个）")
    ap.add_argument("--budget-ms", type=int, default=200, help="首屏门禁（默认 200）")
    args = ap.parse_args()

    samples = parse(args.logs)
    if not samples:
        print("无 E2E 样本——确认日志来自插桩后的构建，且会话里有过文件搜索页输入")
        return 2

    by_metric = {m: [int(s[m]) for s in samples] for m in METRICS}
    pages: dict[str, int] = {}
    for s in samples:
        pages[s["page"]] = pages.get(s["page"], 0) + 1

    print(f"样本 {len(samples)} 条；页面分布：{pages}")
    print(f"{'指标':<8}{'n':>5}{'min':>8}{'p50':>8}{'p95':>8}{'max':>8}{'mean':>9}")
    for m in METRICS:
        st = stats(by_metric[m])
        print(
            f"{m:<8}{st['n']:>5}{st['min']:>8}{st['p50']:>8}{st['p95']:>8}"
            f"{st['max']:>8}{st['mean']:>9}"
        )

    st_total = stats(by_metric["total"])
    ok = st_total["p95"] < args.budget_ms
    print(
        f"门禁：total p95 {st_total['p95']} ms {'<' if ok else '>='} 预算 {args.budget_ms} ms"
        f" → {'PASS' if ok else 'FAIL'}"
    )
    # 离群提示（口径边界：样本结算遇面板隐藏会把隐藏期计入）
    slow = [s["src"] for s in samples if int(s["total"]) >= args.budget_ms]
    if slow:
        print(f"超预算样本 {len(slow)} 条：{slow[:10]}{'…' if len(slow) > 10 else ''}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
