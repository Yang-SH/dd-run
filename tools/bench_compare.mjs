#!/usr/bin/env node
// §4.5（dd-run-改进分析.md）：criterion 基线比对——性能趋势回归门。
//
// 用法：node tools/bench_compare.mjs <criterion目录> <基线名> <对比名> [--threshold 20]
//   criterion 目录 = 共享 CARGO_TARGET_DIR 下的 criterion/（两份基线由
//   `cargo bench -- --save-baseline <名>` 产出，目录结构 <bench>/<名>/estimates.json）。
//
// 判定：中位数（median point_estimate，纳秒）对比基线**退化超过阈值**（默认 20%）
// 才 exit 1——GitHub runner 裸机噪音大，靠高阈值防误报（分析文档 §4.5 口径）。
// 单侧缺失的 bench（新增 / 删除）只提示不判红。

import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const [dir, baseName, headName] = args;
const thresholdIdx = args.indexOf("--threshold");
const thresholdPct = thresholdIdx >= 0 ? Number(args[thresholdIdx + 1]) : 20;

if (!dir || !baseName || !headName || Number.isNaN(thresholdPct)) {
  console.error("用法: node tools/bench_compare.mjs <criterion目录> <基线名> <对比名> [--threshold 20]");
  process.exit(2);
}

const medianOf = (bench, name) => {
  const file = path.join(dir, bench, name, "estimates.json");
  if (!fs.existsSync(file)) return null;
  const est = JSON.parse(fs.readFileSync(file, "utf8"));
  return est.median?.point_estimate;
};

const benches = fs
  .readdirSync(dir, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .filter((e) => fs.existsSync(path.join(dir, e.name, baseName, "estimates.json")) || fs.existsSync(path.join(dir, e.name, headName, "estimates.json")))
  .map((e) => e.name);

if (benches.length === 0) {
  console.error(`::error::criterion 目录 ${dir} 下没有任何基线（base=${baseName} / head=${headName}）——bench 是否实际跑过？`);
  process.exit(2);
}

const fmt = (ns) => (ns >= 1e6 ? `${(ns / 1e6).toFixed(2)} ms` : ns >= 1e3 ? `${(ns / 1e3).toFixed(1)} µs` : `${ns.toFixed(0)} ns`);

let failed = false;
const rows = [];
for (const bench of benches) {
  const base = medianOf(bench, baseName);
  const head = medianOf(bench, headName);
  if (base == null || head == null) {
    rows.push(`  ${bench}: 单侧缺失（base=${fmt(base)} head=${fmt(head)}）——不判红`);
    continue;
  }
  const pct = ((head - base) / base) * 100;
  const bad = pct > thresholdPct;
  if (bad) failed = true;
  rows.push(`  ${bad ? "❌" : "✅"} ${bench}: ${fmt(base)} → ${fmt(head)}（${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%，阈值 +${thresholdPct}%）`);
}

console.log(`bench 趋势比对（基线 ${baseName} vs ${headName}，中位数）：\n${rows.join("\n")}`);
if (failed) {
  console.error(`::error::存在退化超过 ${thresholdPct}% 的 bench——请核实改动是否引入热路径回退（§4.5）`);
  process.exit(1);
}
console.log("全部 bench 在阈值内。");
