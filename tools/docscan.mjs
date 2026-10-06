// docscan.py 的 node 等效（本机 python 存根不可用期间的替代运行时）。
// 检查项：① 跨文件 .md 链接可达性；② INDEX.md 行数登记 vs `wc -l` 实测
// （2026-10-06 六日审计引入——批量文档回写时行数同步屡次遗漏，工具化把关）。
// 用法：node tools/docscan.mjs（任何失效链接或行数漂移 → exit 1，可作批次收口门禁）
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, relative, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const files = [];
for (const d of [join(ROOT, "docs"), ROOT]) {
  for (const f of readdirSync(d).sort()) {
    if (f.endsWith(".md")) {
      const p = join(d, f);
      if (!files.includes(p)) files.push(p);
    }
  }
}

// ── ① 链接可达性 ──
const linkRe = /\]\((\.{0,2}\/?[^)#]+\.md)(#[^)]*)?\)/g;
const links = {}, broken = [];
for (const p of files) {
  const txt = readFileSync(p, "utf-8");
  const rel = relative(ROOT, p).replaceAll("\\", "/");
  const out = new Set();
  for (const m of txt.matchAll(linkRe)) {
    const target = resolve(dirname(p), m[1]);
    if (!existsSync(target)) {
      broken.push(`${rel}  ->  ${m[1]}`);
    } else {
      out.add(m[1]);
    }
  }
  if (out.size) links[rel] = out.size;
}

// ── ② INDEX 行数登记 vs 实测（wc -l 口径：结尾换行不计为一行） ──
const idxPath = join(ROOT, "docs", "INDEX.md");
const idxLines = readFileSync(idxPath, "utf-8").split("\n");
const wcLines = (p) => readFileSync(p, "utf-8").replace(/\n$/, "").split("\n").length;
const drifts = [];
let checked = 0;
for (const line of idxLines) {
  const dm = line.match(/\]\((\.{0,2}\/[^)#]+\.md)(#[^)]*)?\)/);
  const cm = line.match(/\|\s*(\d+)\s*\|/);
  if (!dm || !cm) continue;
  const full = dm[1].replace(/^\.\.\//, "").replace(/^\.\//, "docs/");
  if (!existsSync(full)) {
    drifts.push(`${dm[1]}  ->  文件不存在`);
    continue;
  }
  checked++;
  const real = wcLines(full);
  if (String(real) !== cm[1]) drifts.push(`${full}  INDEX=${cm[1]}  real=${real}`);
}

let report = "== broken links ==\n" + (broken.length ? broken.join("\n") + "\n" : "(none)\n\n");
report += "== INDEX line counts ==\n";
report += drifts.length ? drifts.join("\n") + "\n" : `(${checked} rows checked, 0 drift)\n\n`;
report += "== outbound link counts ==\n";
for (const [k, v] of Object.entries(links).sort()) report += `${k.padEnd(44)} -> ${v} unique\n`;
writeFileSync(join(ROOT, ".doclinks.txt"), report);

const fail = broken.length + drifts.length;
console.log(`links: ${broken.length ? `BROKEN ${broken.length}` : "OK (0 broken)"}  |  index counts: ${drifts.length ? `DRIFT ${drifts.length}` : `OK (${checked} rows)`}`);
process.exitCode = fail ? 1 : 0;
