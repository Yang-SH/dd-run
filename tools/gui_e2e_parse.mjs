#!/usr/bin/env node
/// E2E 首屏计时解析（P2-5，验收报告 §5 #4 的采样判读工具）——Node 版。
///
/// 与 [`gui_e2e_parse.py`](./gui_e2e_parse.py) 逐行对齐（2026-10-07 移植：本机
/// 无 Python 解释器，项目判读工具已有 docscan.mjs / glyph_coverage_check.mjs
/// 零依赖 Node 先例）。输入 = dd-run 宿主 stderr 捕获日志（`dd-run.exe 2> gui.log`
/// 或 debug 构建控制台重定向），逐行匹配宿主埋点：
///
///     [dd-gui] E2E 首屏: input→paint {total} ms（去抖/进页等待 {wait} + get_items 往返 {rtt} + 渲染 {paint}）；page=… query=… 字符 items=…
///
/// 输出 = 每项指标（total/wait/rtt/paint）的 count/min/p50/p95/max + 对
/// 「输入 → 首屏 ≤ 200 ms」门禁的判定（p95 < 200 → PASS）。退出码：0 全过 /
/// 1 有超门禁 / 2 无样本或文件不可读。
///
/// 用法：
///     node tools/gui_e2e_parse.mjs gui.log [more.log ...] [--budget-ms 200]

import { readFileSync } from "node:fs";

const LINE =
  /E2E 首屏: input→paint (?<total>\d+) ms（去抖\/进页等待 (?<wait>\d+) \+ get_items 往返 (?<rtt>\d+) \+ 渲染 (?<paint>\d+)）；page=(?<page>\S+) query=(?<q>\d+) 字符 items=(?<items>\d+)/;

const METRICS = ["total", "wait", "rtt", "paint"];

function parse(paths) {
  const samples = [];
  for (const path of paths) {
    let lines;
    try {
      lines = readFileSync(path, "utf-8").split("\n");
    } catch (e) {
      console.error(`[error] 读不了 ${path}：${e.message}`);
      process.exit(2);
    }
    lines.forEach((line, i) => {
      const m = LINE.exec(line);
      if (m) samples.push({ ...m.groups, src: `${path}:${i + 1}` });
    });
  }
  return samples;
}

function stats(values) {
  const vs = [...values].sort((a, b) => a - b);
  const n = vs.length;
  const pct = (p) => {
    // nearest-rank：ceil(p*n)-1（n=1 时恒取唯一值）
    const idx = Math.max(0, Math.ceil((p * 100 * n) / 100) - 1);
    return vs[Math.min(idx, n - 1)];
  };
  return {
    n,
    min: vs[0],
    p50: pct(0.5),
    p95: pct(0.95),
    max: vs[n - 1],
    mean: Math.round((vs.reduce((a, b) => a + b, 0) / n) * 10) / 10,
  };
}

function main() {
  const args = process.argv.slice(2);
  let budgetMs = 200;
  const bi = args.indexOf("--budget-ms");
  if (bi >= 0) {
    budgetMs = parseInt(args[bi + 1], 10);
    args.splice(bi, 2);
  }
  if (args.length === 0) {
    console.error("用法：node tools/gui_e2e_parse.mjs gui.log [more.log ...] [--budget-ms 200]");
    process.exit(2);
  }

  const samples = parse(args);
  if (samples.length === 0) {
    console.log("无 E2E 样本——确认日志来自插桩后的构建，且会话里有过文件搜索页输入");
    process.exit(2);
  }

  const byMetric = Object.fromEntries(METRICS.map((m) => [m, samples.map((s) => parseInt(s[m], 10))]));
  const pages = {};
  for (const s of samples) pages[s.page] = (pages[s.page] ?? 0) + 1;

  console.log(`样本 ${samples.length} 条；页面分布：${JSON.stringify(pages)}`);
  console.log("指标          n     min     p50     p95     max    mean");
  for (const m of METRICS) {
    const st = stats(byMetric[m]);
    console.log(
      `${m.padEnd(8)}${String(st.n).padStart(6)}${String(st.min).padStart(8)}${String(st.p50).padStart(8)}` +
        `${String(st.p95).padStart(8)}${String(st.max).padStart(8)}${String(st.mean).padStart(8)}`
    );
  }

  const stTotal = stats(byMetric.total);
  const ok = stTotal.p95 < budgetMs;
  console.log(`门禁：total p95 ${stTotal.p95} ms ${ok ? "<" : ">="} 预算 ${budgetMs} ms → ${ok ? "PASS" : "FAIL"}`);
  // 离群提示（口径边界：样本结算遇面板隐藏会把隐藏期计入）
  const slow = samples.filter((s) => parseInt(s.total, 10) >= budgetMs).map((s) => s.src);
  if (slow.length > 0) {
    console.log(`超预算样本 ${slow.length} 条：${slow.slice(0, 10).join(", ")}${slow.length > 10 ? "…" : ""}`);
  }
  process.exit(ok ? 0 : 1);
}

main();
