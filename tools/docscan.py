# -*- coding: utf-8 -*-
"""文档体检：统计 Markdown 文档的结构指标并校验跨文件链接可达性。

用法：
    python tools/docscan.py

输出（写入仓库根，便于比对）：
    .docscan.txt  —— 每份文档的体积 / 行数 / 最长行 / 标题数 / 二级标题数 / 代码块数 / 表格行数
    .doclinks.txt —— 失效链接清单 + 每份文档的出链统计

设计意图：文档整理与维护时用它做"客观体检"——最长行超标（如状态挤成一行）、
标题层级失衡、链接改路径后失效，都能一眼看出。
"""
import io
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

files = []
for d in [os.path.join(ROOT, "docs"), ROOT]:
    for f in sorted(os.listdir(d)):
        if f.endswith(".md"):
            p = os.path.join(d, f)
            if p not in files:
                files.append(p)

rows = []
for p in files:
    raw = open(p, "rb").read()
    txt = raw.decode("utf-8", errors="replace")
    lines = txt.split("\n")
    rows.append({
        "path": os.path.relpath(p, ROOT).replace("\\", "/"),
        "kb": round(len(raw) / 1024, 1),
        "lines": len(lines),
        "longest": max((len(l) for l in lines), default=0),
        "heads": len([l for l in lines if re.match(r"^#{1,6} ", l)]),
        "h2": len([l for l in lines if l.startswith("## ")]),
        "fences": txt.count("```") // 2,
        "tbl_lines": len(re.findall(r"^\|", txt, re.M)),
    })

out = io.StringIO()
out.write(f"{'path':<44}{'KB':>7}{'lines':>7}{'maxLen':>8}{'head#':>7}{'H2':>5}{'code':>6}{'tblLn':>7}\n")
out.write("-" * 92 + "\n")
for r in sorted(rows, key=lambda x: -x["kb"]):
    out.write(f"{r['path']:<44}{r['kb']:>7}{r['lines']:>7}{r['longest']:>8}"
              f"{r['heads']:>7}{r['h2']:>5}{r['fences']:>6}{r['tbl_lines']:>7}\n")
out.write("-" * 92 + "\n")
out.write(f"TOTAL files={len(rows)}  KB={sum(r['kb'] for r in rows):.1f}  "
          f"lines={sum(r['lines'] for r in rows)}\n")
open(os.path.join(ROOT, ".docscan.txt"), "w", encoding="utf-8").write(out.getvalue())

# 跨文件链接可达性
link_re = re.compile(r"\]\((\.{0,2}/?[^)]+\.md)(#[^)]*)?\)")
links, broken = {}, []
for p in files:
    txt = open(p, "r", encoding="utf-8", errors="replace").read()
    rel = os.path.relpath(p, ROOT).replace("\\", "/")
    for m in link_re.finditer(txt):
        target = m.group(1)
        resolved = os.path.normpath(os.path.join(os.path.dirname(p), target)).replace("\\", "/")
        links.setdefault(rel, []).append(resolved)
        if not os.path.exists(resolved):
            broken.append(f"{rel} -> {os.path.relpath(resolved, ROOT).replace(chr(92), '/')}")

with open(os.path.join(ROOT, ".doclinks.txt"), "w", encoding="utf-8") as fh:
    fh.write("== broken links ==\n")
    fh.write("\n".join(sorted(set(broken))) if broken else "(none)\n")
    fh.write("\n\n== outbound link counts ==\n")
    for src in sorted(links):
        fh.write(f"{src:<44} -> {len(links[src])} links, {len(set(links[src]))} unique\n")
print("ok")
