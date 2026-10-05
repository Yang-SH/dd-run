// docscan.py 的 node 等效（本机 python 存根不可用期间的替代运行时）。
// 仅复刻「跨文件 .md 链接可达性」检查（.doclinks.txt 的失效链接部分），
// 结构指标仍在原 python 版。用法：node tools/docscan.mjs
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
let report = "== broken links ==\n" + (broken.length ? broken.join("\n") + "\n" : "(none)\n\n");
report += "== outbound link counts ==\n";
for (const [k, v] of Object.entries(links).sort()) report += `${k.padEnd(44)} -> ${v} unique\n`;
writeFileSync(join(ROOT, ".doclinks.txt"), report);
console.log(broken.length ? `BROKEN ${broken.length}` : "links OK (0 broken)");
