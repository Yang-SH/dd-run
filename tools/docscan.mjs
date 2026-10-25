// docscan.py 的 node 等效（本机 python 存根不可用期间的替代运行时）。
// 检查项：① 跨文件 .md 链接可达性；② INDEX.md 行数登记 vs `wc -l` 实测
// （2026-10-06 六日审计引入——批量文档回写时行数同步屡次遗漏，工具化把关）。
// 用法：node tools/docscan.mjs（任何失效链接或行数漂移 → exit 1，可作批次收口门禁）
//       node tools/docscan.mjs --write-index（B3：行数漂移**自动回填** INDEX 的
//       行数列后重校验——人只维护状态/读者列，行数列不再手工同步；「文件不存在」
//       类漂移不可自动修仍 exit 1。CI 门禁**不带**此参数，回填仅本地使用）
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, relative, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const WRITE_INDEX = process.argv.includes("--write-index");

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
// 行数列 = 行内**第一段** `| 纯数字 |`（状态/版本列均非纯数字，行数列必先于
// 描述列出现）——与下方校验取同一次出现，回填即替换该段。
const COUNT_SEG_RE = /\|\s*(\d+)\s*\|/;
let idxText = readFileSync(idxPath, "utf-8");
const idxLines = idxText.split("\n");
const wcLines = (p) => readFileSync(p, "utf-8").replace(/\n$/, "").split("\n").length;
const drifts = [];
const autoFixed = [];
let checked = 0;
for (let i = 0; i < idxLines.length; i++) {
  const line = idxLines[i];
  const dm = line.match(/\]\((\.{0,2}\/[^)#]+\.md)(#[^)]*)?\)/);
  const cm = line.match(COUNT_SEG_RE);
  if (!dm || !cm) continue;
  const full = dm[1].replace(/^\.\.\//, "").replace(/^\.\//, "docs/");
  if (!existsSync(full)) {
    drifts.push(`${dm[1]}  ->  文件不存在`);
    continue;
  }
  checked++;
  const real = wcLines(full);
  if (String(real) !== cm[1]) {
    if (WRITE_INDEX) {
      // 整行唯一替换（INDEX 登记行不含重复行）；写回保持原行尾（CRLF 由
      // split("\n") 保留在各元素尾部，join 后逐字节复原未改动行）。
      const newLine = line.replace(COUNT_SEG_RE, `| ${real} |`);
      const replaced = idxText.replace(line, newLine);
      if (replaced === idxText) {
        drifts.push(`${full}  INDEX=${cm[1]}  real=${real}  （整行替换未命中，需手工修）`);
        continue;
      }
      idxText = replaced;
      idxLines[i] = newLine;
      autoFixed.push(`${full}  INDEX=${cm[1]} -> ${real}`);
    } else {
      drifts.push(`${full}  INDEX=${cm[1]}  real=${real}`);
    }
  }
}
if (WRITE_INDEX && autoFixed.length) {
  writeFileSync(idxPath, idxText);
  // 回填后以新文本重算校验（drifts 只保留不可自动修的项）。
  const reLines = readFileSync(idxPath, "utf-8").split("\n");
  drifts.length = 0;
  for (const line of reLines) {
    const dm = line.match(/\]\((\.{0,2}\/[^)#]+\.md)(#[^)]*)?\)/);
    const cm = line.match(COUNT_SEG_RE);
    if (!dm || !cm) continue;
    const full = dm[1].replace(/^\.\.\//, "").replace(/^\.\//, "docs/");
    if (!existsSync(full)) {
      drifts.push(`${dm[1]}  ->  文件不存在`);
      continue;
    }
    if (String(wcLines(full)) !== cm[1]) drifts.push(`${full}  INDEX=${cm[1]}  real=${wcLines(full)}`);
  }
}

// ── ③ 文档引用文件存在性（O13 粗一致性机检，2026-10-17） ──
// 定位：只做「符号存在性」最粗一层，高精度优先——宁可漏检也不误报卡 CI。
// 规则：只看**反引号包裹**的**仓库根相对路径**——即以 `crates/` `tools/` `docs/`
// `examples/` `.github/` 开头、带已知文件扩展名、不含通配符/变量/空格的 token。
// 刻意不判「简写源码路径」（如 `ui/panel.rs`、`builtins/shell.rs`）与上游/外部
// 引用（`src/modules/cmdpal/…`、`file://…`、`~/.config/…`）——这些在文档里是
// 惯用简写或外部锚点，判存在性必然误报。构建产物前缀（`dist/` `target/`，
// gitignored）本就不在上列前缀内，CI 上必不存在故天然排除。
const REF_ROOT_PREFIX_RE = /^(crates|tools|docs|examples|\.github)\//;
const REF_EXT_RE = /\.(rs|md|json|ya?ml|toml|sh|ps1|mjs|py|txt|html|svg|ico|hex)$/i;
const refBases = [ROOT];
const refBad = [];
const refSeen = new Set();
let refChecked = 0;
for (const p of files) {
  const rel = relative(ROOT, p).replaceAll("\\", "/");
  const txt = readFileSync(p, "utf-8");
  for (const m of txt.matchAll(/`([^`\n]+)`/g)) {
    const tok = m[1].trim();
    if (!REF_ROOT_PREFIX_RE.test(tok)) continue;
    if (/[*{}$\s]/.test(tok)) continue;
    if (!REF_EXT_RE.test(tok)) continue;
    if (refSeen.has(tok)) continue;
    refSeen.add(tok);
    refChecked++;
    const hit = refBases.some((b) => existsSync(join(b, tok)));
    if (!hit) refBad.push(`${rel}  ->  \`${tok}\``);
  }
}

// ── ④ 契约文档禁行号锚（B2①，2026-10-24）──
// 规则：三份**对扩展作者可见**的契约文档（protocol / manifest-schema /
// extensions）内出现 `x.rs:行号` 即 fail——行号随代码漂移（治理前 16 处锚
// 多处已失效），一律改「文件 + 符号名」锚。其余 docs 的 ~77 处行号锚第二
// 阶段清理后再扩面（分析文档 §3.3 改法①「分批放开」）。
const CONTRACT_DOCS = new Set(["docs/protocol.md", "docs/manifest-schema.md", "docs/extensions.md"]);
const anchorBad = [];
let anchorChecked = 0;
for (const p of files) {
  const rel = relative(ROOT, p).replaceAll("\\", "/");
  if (!CONTRACT_DOCS.has(rel)) continue;
  anchorChecked++;
  const txt = readFileSync(p, "utf-8");
  for (const m of txt.matchAll(/[A-Za-z_]+\.rs:\d+/g)) anchorBad.push(`${rel}  ->  ${m[0]}`);
}

let report = "== broken links ==\n" + (broken.length ? broken.join("\n") + "\n" : "(none)\n\n");
report += "== INDEX line counts ==\n";
report += drifts.length ? drifts.join("\n") + "\n" : `(${checked} rows checked, 0 drift)\n\n`;
report += "== referenced file paths (O13) ==\n";
report += refBad.length ? refBad.join("\n") + "\n" : `(${refChecked} unique paths checked, 0 missing)\n\n`;
report += "== contract-doc line anchors (B2①) ==\n";
report += anchorBad.length ? anchorBad.join("\n") + "\n" : `(${anchorChecked} docs checked, 0 anchors)\n\n`;
report += "== outbound link counts ==\n";
for (const [k, v] of Object.entries(links).sort()) report += `${k.padEnd(44)} -> ${v} unique\n`;
writeFileSync(join(ROOT, ".doclinks.txt"), report);

const fail = broken.length + drifts.length + refBad.length + anchorBad.length;
console.log(
  `links: ${broken.length ? `BROKEN ${broken.length}` : "OK (0 broken)"}  |  ` +
    `index counts: ${drifts.length ? `DRIFT ${drifts.length}` : `OK (${checked} rows)`}  |  ` +
    `referenced paths: ${refBad.length ? `MISSING ${refBad.length}` : `OK (${refChecked} unique)`}  |  ` +
    `contract anchors: ${anchorBad.length ? `FAIL ${anchorBad.length}` : `OK (${anchorChecked} docs, 0 anchors)`}` +
    (WRITE_INDEX && autoFixed.length ? `  |  auto-fixed: ${autoFixed.length}（${autoFixed.join("; ")}）` : ""),
);
if (drifts.length) console.log(drifts.join("\n"));
if (refBad.length) console.log(refBad.join("\n"));
if (anchorBad.length) console.log(anchorBad.join("\n"));
process.exitCode = fail ? 1 : 0;
