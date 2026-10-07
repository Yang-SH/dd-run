// O5-a 穷举核查（一次性工具，用完可删）：
// 1) 解析四个运行时系统字体的 cmap，取覆盖码位并集；
// 2) 扫描 dd-gui / dd-ext 全部 .rs 源码字符串字面量与 char 字面量（剥注释）；
// 3) 报告未被任何运行时字体覆盖的码位（= 移除 default_fonts 后的 tofu 风险面）。
// 用法：node tools/glyph_coverage_check.mjs
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';

const FONTS = [
  'C:/Windows/Fonts/segoeui.ttf',
  'C:/Windows/Fonts/msyh.ttc',
  'C:/Windows/Fonts/seguisym.ttf',
  'C:/Windows/Fonts/SegoeIcons.ttf',
  'C:/Windows/Fonts/segmdl2.ttf',
];

// ---------- cmap 解析（TTF / TTC，format 4 / 12） ----------
function parseCmap(buf) {
  const tables = [];
  if (buf.readUInt32BE(0) === 0x74746366) {
    // ttcf
    const num = buf.readUInt32BE(8);
    for (let i = 0; i < num; i++) tables.push(buf.readUInt32BE(12 + i * 4));
  } else {
    tables.push(0);
  }
  const covered = new Set();
  for (const off of tables) {
    const numTables = buf.readUInt16BE(off + 4);
    let cmapOff = -1;
    for (let i = 0; i < numTables; i++) {
      const rec = off + 12 + i * 16;
      if (buf.readUInt32BE(rec) === 0x636d6170) cmapOff = buf.readUInt32BE(rec + 8); // TTC 内为文件绝对偏移
    }
    if (cmapOff < 0) continue;
    const n = buf.readUInt16BE(cmapOff + 2);
    for (let i = 0; i < n; i++) {
      const rec = cmapOff + 4 + i * 8;
      const platformID = buf.readUInt16BE(rec);
      const encodingID = buf.readUInt16BE(rec + 2);
      const subOff = cmapOff + buf.readUInt32BE(rec + 4);
      // 只取 Unicode 全码位表（format 12）与 BMP 表（format 4）
      const isUnicode =
        platformID === 0 || platformID === 3 || (platformID === 0 && encodingID <= 6);
      if (!isUnicode) continue;
      const fmt = buf.readUInt16BE(subOff);
      if (fmt === 4) {
        const segCountX2 = buf.readUInt16BE(subOff + 6);
        const segCount = segCountX2 / 2;
        const endBase = subOff + 14;
        const startBase = endBase + segCountX2 + 2;
        for (let s = 0; s < segCount; s++) {
          const endCp = buf.readUInt16BE(endBase + s * 2);
          const startCp = buf.readUInt16BE(startBase + s * 2);
          if (startCp === 0xffff) continue;
          for (let cp = startCp; cp <= endCp && cp !== 0xffff; cp++) covered.add(cp);
        }
      } else if (fmt === 12) {
        const nGroups = buf.readUInt32BE(subOff + 12);
        for (let g = 0; g < nGroups; g++) {
          const recG = subOff + 16 + g * 12;
          const startCp = buf.readUInt32BE(recG);
          const endCp = buf.readUInt32BE(recG + 4);
          for (let cp = startCp; cp <= endCp; cp++) covered.add(cp);
        }
      }
    }
  }
  return covered;
}

// format 4 逐码位展开对大区段（如 msyh CJK）可能上万次循环，量级仍可忽略。
const union = new Map(); // cp -> 首个命中的字体名
for (const f of FONTS) {
  if (!fs.existsSync(f)) {
    console.log(`[warn] 缺字体文件：${f}`);
    continue;
  }
  const buf = fs.readFileSync(f);
  const set = parseCmap(buf);
  console.log(`[info] ${path.basename(f)}：${set.size} 码位`);
  for (const cp of set) if (!union.has(cp)) union.set(cp, path.basename(f));
}

// ---------- Rust 源码字符串字面量提取（剥注释） ----------
function stripComments(src) {
  let out = '';
  let i = 0;
  let state = 'code'; // code | line | block | str | rawstr | char | lifetime
  let rawHash = 0;
  while (i < src.length) {
    const c = src[i];
    const n = src[i + 1];
    if (state === 'code') {
      if (c === '/' && n === '/') { state = 'line'; i += 2; continue; }
      if (c === '/' && n === '*') { state = 'block'; i += 2; continue; }
      if (c === '"') {
        // r#"..."# 原始字符串
        let h = i - 1, hashes = 0;
        while (h >= 0 && src[h] === '#') { hashes++; h--; }
        if (h >= 0 && src[h] === 'r') {
          state = 'rawstr'; rawHash = hashes; out += '"'; i += 1 + hashes + 1; continue;
        }
        state = 'str'; out += '"'; i++; continue;
      }
      if (c === "'") {
        // char 字面量 or 生命周期 'a —— 启发式：\x 转义或 'x' 形态
        const m = /^'(\\.|[^'\\])'/.exec(src.slice(i));
        if (m) { out += ' '; extractLiteral(m[1]); i += m[1].length + 2; continue; }
        i++; continue; // 生命周期等
      }
      out += c; i++; continue;
    }
    if (state === 'line') { if (c === '\n') { state = 'code'; out += '\n'; } i++; continue; }
    if (state === 'block') {
      if (c === '*' && n === '/') { state = 'code'; i += 2; } else i++;
      continue;
    }
    if (state === 'str') {
      if (c === '\\') { out += c + (n ?? ''); i += 2; continue; }
      if (c === '"') { state = 'code'; out += '"'; i++; continue; }
      out += c; i++; continue;
    }
    if (state === 'rawstr') {
      const term = '"' + '#'.repeat(rawHash);
      if (src.startsWith(term, i)) { state = 'code'; out += '"'; i += term.length; continue; }
      out += c; i++; continue;
    }
  }
  return out;

  function extractLiteral(inner) {
    if (inner.startsWith('\\u{')) {
      const cp = parseInt(inner.slice(3, -1), 16);
      chars.push(cp);
    } else if (inner.startsWith('\\x') || inner.startsWith('\\n') || inner.startsWith('\\t') || inner.startsWith('\\r') || inner.startsWith('\\0') || inner.startsWith("\\'") || inner.startsWith('\\\\')) {
      // ASCII 控制符/转义，必然被 segoeui 覆盖，忽略
    } else {
      chars.push(inner.codePointAt(0));
    }
  }
}

const chars = []; // 收集源码中出现的全部码位
function collectFromString(s) {
  // 处理 Rust 字符串转义：\u{XXXX} 解码为码位；其余转义（\n \t \" \\ \x..）均为
  // ASCII/控制符，被 segoeui 必然覆盖，跳过反斜杠后的 1-2 个字符即可。
  let i = 0;
  while (i < s.length) {
    if (s[i] === '\\') {
      const m = /^\\u\{([0-9a-fA-F]{1,6})\}/.exec(s.slice(i));
      if (m) {
        chars.push(parseInt(m[1], 16));
        i += m[0].length;
        continue;
      }
      i += 2;
      continue;
    }
    chars.push(s.codePointAt(i));
    i += String.fromCodePoint(s.codePointAt(i)).length;
  }
}

// 再跑一遍字符串字面量抽取（在剥注释后的文本上抓 "..." 与 r#"..."#）
function extractStrings(src) {
  const re = /"(?:[^"\\]|\\.)*"/g;
  let m;
  while ((m = re.exec(src))) collectFromString(m[0].slice(1, -1));
}

function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (e.name.endsWith('.rs')) {
      const src = fs.readFileSync(p, 'utf8');
      const stripped = stripComments(src);
      extractStrings(stripped);
      // char 字面量在 stripComments 里已 push 进 chars
    }
  }
}

walk(path.resolve('crates/dd-gui/src'));
walk(path.resolve('crates/dd-ext/src'));
walk(path.resolve('crates/dd-host/src'));

const uniq = [...new Set(chars)].sort((a, b) => a - b);
const missing = uniq.filter((cp) => !union.has(cp));
console.log(`\n[info] 源码字面量总码位数：${uniq.length}`);
if (missing.length === 0) {
  console.log('[PASS] 全部字面量码位均被运行时字体栈覆盖，无 tofu 风险。');
} else {
  console.log(`[FAIL] ${missing.length} 个码位无覆盖（移除 default_fonts 后将 tofu）：`);
  for (const cp of missing) {
    console.log(
      `  U+${cp.toString(16).toUpperCase().padStart(4, '0')} ${printable(cp)} ${contextOf(cp)}`
    );
  }
}

function printable(cp) {
  if (cp < 0x20 || (cp >= 0x7f && cp < 0xa0)) return '(ctrl)';
  try { return String.fromCodePoint(cp); } catch { return '(?)'; }
}
// 找出含该码位的源文件行，便于定位
function contextOf(cp) {
  const ch = printable(cp);
  if (ch === '(ctrl)') return '';
  return '';
}
