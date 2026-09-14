# optimization-plan.md 核对报告（2026-09-14）

> **状态**：核查记录 ｜ **版本**：v1.0 ｜ **最后更新**：2026-09-14
> **关联**：[optimization-plan.md](./optimization-plan.md) · [INDEX.md](./INDEX.md) · [protocol.md](./protocol.md) · [implementation.md](./implementation.md) · [memory-optimization-plan.md](./memory-optimization-plan.md) · [m9-inprocess-builtins.md](./m9-inprocess-builtins.md) · [doc-code-diff-2026-09-13.md](./doc-code-diff-2026-09-13.md)

---

## 1. 核对范围与方法

- **被核对对象**：`docs/optimization-plan.md`（137 行、CRLF、v1.0、2026-09-14、状态「规划中」）。
- **事实源**：`crates/**` 源码、各 `Cargo.toml`、`dist/` 产物、`docs/`（INDEX / protocol / implementation / memory-optimization-plan / m9-inprocess-builtins / doc-code-diff-2026-09-13）。
- **判据**：代码与产物为唯一事实源——文档与代码冲突记 `❌`；列举不全记 `⚠️`；文档内部或跨文档打架记「矛盾」；违反本仓 `INDEX.md` §4.2 格式规约记 `🟨`。
- **方法**：静态扫描（Grep 符号 / 日志框架 / 依赖面）+ 逐条行号取证（Read）+ 产物字节实测（`os.path.getsize`）+ 跨文档对照。
- **约束**：**只读核对，未改动任何文件**（仅新增本报告）。

---

## 2. 结论摘要

| 判定 | 计数 | 摘要 |
|---|---|---|
| ❌ 事实错误 | 4 | 行号失效 ×2、字节数过期 ×1、归因错误 ×1（含两处归因） |
| ⚠️ 遗漏 / 不严谨 | 6 | |
| 🟨 规范不符 | 2 | |
| ❓ 数据存疑 / 需补注 | 3 | |
| ✅ 抽检通过 | 20+ | 见 §5 |

**结论先行**：文档的**骨架、结论方向与绝大部分取证是准确的**——最关键的行号（`ext_inprocess.rs:262`、`dd-run-cli/src/main.rs:472,501`、`platform.rs:60/146`）、依赖引用面、协议现状表均逐条吻合。但仍存在 **4 处确凿事实错误**（尤以「eprintln 日志示例行号」指向非日志行、体积字节数与 `dist` 产物不符为要）与若干规范不符项。

**交付判定**：⚠️ **未达到「可直接发布 / 交付」标准——须按 §3 修正后方可定稿。**

---

## 3. 逐条问题清单

### A. 事实错误（❌，必改）

| # | 位置 | 文档声称 | 事实（依据） | 修改建议 |
|---|---|---|---|---|
| **E-1** | §2.4（L85） | 日志示例「`dd-gui/src/platform.rs:24`、`app/invoke.rs:11`」 | ❌ `platform.rs:24` 是**空行**、`invoke.rs:11` 是 `use eframe::egui;`，两处均非 `eprintln!`。真实落点：首个 `eprintln!` 在 `platform.rs:81`、`app/invoke.rs:50` | 改「如 `dd-gui/src/platform.rs`（约 :81 起）、`app/invoke.rs`（约 :50 起）、`dd-run-cli` 等」，去掉失效精确行号 |
| **E-2** | §2.1（L39）；并涉 §0（L14）「8.2 MB」 | `dist/dd-run-0.1.1.exe` = **8,601,088 B** | ❌ 实测当前产物 = **8,623,616 B**（8.224 MiB，差 22,528 B）。8,601,088 系 2026-09-12 重编快照（`implementation.md` 同值）；违反 INDEX §4.2「数字必须实测取证，不得沿用历史快照」 | 重新实测更新；补注「字节数随重编变化，引用前先实测」 |
| **E-3** | §4（L136） | 「`doc-code-diff-2026-09-13.md` … 其 **§5** 待人工决策项（P-01 / `-32002` / `PageInfo`）」 | ❌ 该文 §5 **仅含 P-01**；`-32002`、`PageInfo` 不在其中（二者是 **INDEX §5** 的独立项，§2.2 已正确归到 INDEX §5） | 改「其 §5（P-01）与 **INDEX §5**（`-32002`/`PageInfo`）…」 |
| **E-4** | §1 O8（L29）、§2.7（L118 及表头 L108） | 「LRU 容量不可配」标注来源「**已知（INDEX §5）**」/ 表头「来自 INDEX §5」 | ❌ **INDEX §5 并无 LRU 项**。LRU「可配置」不成立一事记于 doc-code-diff I-01，且已在 `implementation.md:142` 修正，属**已闭环**，非 INDEX §5 开放项 | 删除「（INDEX §5）」归因，或改标「新提出（承 doc-code-diff I-01）」并移出「来自 INDEX §5」表 |

### B. 遗漏 / 不严谨（⚠️，建议改）

| # | 位置 | 问题 | 依据 | 修改建议 |
|---|---|---|---|---|
| **W-1** | §2.3（L72-74） | 三处方法字面量行号枚举**均漏 `fallback_commands`**：`lib.rs` 列 177/182/200/230/272/312（漏 **:189**）；`ext_inprocess.rs` 列 99/118/133/150/157/167（漏 **:125**）；`process.rs` 列 358/382/397/426/433（漏 **:411**） | 三文件 `match` 均为 **7 个方法臂** | 补齐遗漏行号，或改「7 臂：`initialize`/`top_level_commands`/`fallback_commands`/`get_command`/`invoke`/`get_items`/`close`」 |
| **W-2** | §2.5（L93） | 「`egui` `default-features=false`」 | 实为 **`eframe`**（`dd-gui/Cargo.toml:20`；§2.1 自己写的也是 eframe） | 改 `eframe` |
| **W-3** | §2.1（L42） | 「它们（三个依赖）是 `dd-ext` crate 的 `[dependencies]`」 | `fuzzy-matcher`/`chrono` 在 `[dependencies]` ✅，但 `everything-ipc` 在 `[target.'cfg(windows)'.dependencies]`（`dd-ext/Cargo.toml:54`） | 注明 everything-ipc 属 Windows target 依赖段 |
| **W-4** | §1（L31） | 「**状态图例**：🔴 高 / 🟠 中 / 🟡 低 / ⚪ 战略」 | 这四个 emoji 出现在**优先级**列（状态列为文字） | 改「优先级图例」 |
| **W-5** | §1（L22-23）↔ §2.7 | O1/O2 在 §1 状态列标「**新发现**（P-01 / P-18）」，而 §2.7 将其列入「来自 INDEX §5」（已知）；P-01/P-18 **确为 INDEX §5 既有开放项** | INDEX §5 第 1、2 行 | §1 状态改「已知（INDEX §5，P-01/P-18）」，与 §2.7 及 INDEX 对齐 |
| **W-6** | §0（L14）↔ §1 | §0 称剩余空间「集中在…**五处**」，§1 却列 **O1–O8 八项**（O5 体积 / O7 验收 / O8 卫生未纳入「五处」） | 同文对照 | §0 补一句「另含体积边际项 O5、验收项 O7、文档卫生 O8」，使口径闭合 |

### C. 规范不符（🟨，按本仓 INDEX §4.2）

| # | 位置 | 问题 | 修改建议 |
|---|---|---|---|
| **S-1** | 全篇（§2.1/2.3/2.4/2.5 等） | 大量**硬编码精确行号**（`platform.rs:60/146`、`ext_inprocess.rs:262`、`process.rs:358` …）。INDEX §4.2 明文要求「行号写成「约 :NNN」，避免随代码漂移」 | 统一改为「约 :NNN」。本文档因硬编码行号已实际出现 E-1 类失效风险 |
| **S-2** | §2.3（L75） | 「CLI：`dd-run-cli/src/main.rs` **多處**」——繁体字混用 | 改「多处」 |

### D. 数据存疑 / 建议补注（❓）

| # | 位置 | 内容 | 建议 |
|---|---|---|---|
| **Q-1** | §0（L14） | 「过滤 3.7 ms 达标」 | 该值在 `doc-code-diff-2026-09-13.md`（I-05）被标**存疑**（代码无法证实，仅真机记录）。补口径注「（M4 真机记录值，见 m4-record §3.7）」 |
| **Q-2** | §0 / §2 | 「内存 M1–M3 已落地」 | 与 `memory-optimization-plan.md` 一致 ✅，但该方案「真机复验（长会话数字稳定）」标 ⚠️ 待做。补注以免被读成全部闭环 |
| **Q-3** | §2.1（L39） | 体积「↓21%」及其分母 10,857,984 B | 与 implementation.md / m9 记录一致 ✅，但 doc-code-diff（D-02）已记「8,553,984 与 8,601,088 两档划分存疑」。建议保留 ↓21% 的同时按 E-2 用**最新实测值**重算 |

---

## 4. 矛盾专项（跨段 / 跨文档）

| 事实 | 出处 A | 出处 B | 真值 |
|---|---|---|---|
| O1/O2 属「新发现」还是「已知」 | §1 状态列「新发现」 | §2.7「来自 INDEX §5」+ INDEX §5 第 1/2 行 | **已知**（INDEX §5 既有）——§1 口径需改 |
| LRU 项来源 | §1 O8 / §2.7 标「INDEX §5」 | INDEX §5 无此项 | **非 INDEX §5**——归因需改 |
| 体积字节数 | §2.1「8,601,088 B」 | `dist/` 实测 8,623,616 B | **8,623,616 B（当前）** |

---

## 5. 抽检通过（✅，供对照，无需改）

- 文件规模：`crates/**/*.rs` = **73 个 / 31,580 行**（§0「73 个 .rs、约 3.1 万行」）✅
- `catch_unwind` 落点：`ext_inprocess.rs:262`、`dd-run-cli/src/main.rs:472,501` ✅（O5-b「`panic=abort` 不可行」证伪逻辑成立）
- `egui::FontDefinitions::default()`：`platform.rs:60`、`:146` ✅
- `eframe features = ["glow","default_fonts"]` ✅（`dd-gui/Cargo.toml:20-23`）
- 依赖引用面：`everything-ipc`/`fuzzy-matcher`/`chrono` **全仓仅 `dd-ext/src/bin/search.rs` 命中** ✅
- 日志框架：全仓 `use log` / `tracing::` / `env_logger` **零命中** ✅
- 协议现状 7 行表：与 `protocol.md` §2.3 / §3.3 / §10 / §9.2 注记**逐条吻合** ✅（超限 P-01、`jsonrpc` 校验、批处理、`result`/`error` 互斥、致命性、`-32002` 无产出）
- `PageInfo` 不接线 + `GetItemsResult` 仅 `items`/`has_more_items`/`is_loading` ✅（`protocol.md:585,331-332`）
- CI 现状：`.github/workflows/{ci,release}.yml` 存在，但**无 `cargo audit/udeps/outdated`** ✅（O3 成立）
- 引用文档 5 份（memory-optimization-plan / refactor-layering-plan / apps-filtering-plan / icons-typography-plan / search-file）**全部存在、链接可达** ✅
- 本文档行尾**全 CRLF（bare-LF=0）**、元信息块合规（「规划中」∈ 5 取值）✅
- INDEX §3.D 登记「137 行 / 规划中」与实际一致 ✅

---

## 6. 核对方法与复现

```bash
# 文件规模与产物字节
python -c "import glob,os;rs=glob.glob('crates/**/*.rs',recursive=True);print(len(rs),sum(open(f,'rb').read().count(b'\n') for f in rs));print(os.path.getsize('dist/dd-run-0.1.1.exe'))"
# 行尾
python -c "b=open('docs/optimization-plan.md','rb').read();print(b.count(b'\r\n'),b.count(b'\n')-b.count(b'\r\n'))"
# 关键符号/落点（人工比对）
rg -n "catch_unwind" crates
rg -n "FontDefinitions" crates/dd-gui/src/platform.rs
rg -n "eprintln!" crates/dd-gui/src/platform.rs crates/dd-gui/src/app/invoke.rs
rg -n "use log|tracing::|env_logger" crates
```

---

## 7. 待人工决策

1. ~~是否将本报告登记进 `INDEX.md` §3.E~~ → **已登记**（见 §8）。
2. **E-2 体积字节数的基准口径**（仍未定）：本次采「随构建漂移的最新实测 + 注明测量日」；若定为「发布时固定快照」，须在 `optimization-plan.md` 中注明测量时点，否则每次重编都会再次过期。
3. **E-4 LRU「容量可配置」的定位**：本次保留为功能项（承 doc-code-diff I-01）；若判定「已闭环、不再提」，可删去 §2.7 该行与 §1 O8 中的相应表述。

---

## 8. 修订落实（2026-09-14，v1.1 修订阶段）

按本报告逐条修订 `optimization-plan.md`（v1.0 → **v1.1**，137 行，CRLF 未变），并登记 `INDEX.md`。

| 编号 | 处置 |
|---|---|
| **E-1** | §2.4 日志示例改「`platform.rs` 约 :81 起 / `app/invoke.rs` 约 :50 起」 |
| **E-2** | §2.1 体积改 **8,623,616 B（2026-09-14 实测）**，补「随重编漂移，引用前须重新实测」 |
| **E-3** | §4 归因改为「其 §5（P-01）与 INDEX §5（`-32002`/`PageInfo`）」 |
| **E-4** | 删「（INDEX §5）」；§2.7 表头去「来自 INDEX §5」；LRU 行改标「承 doc-code-diff I-01」 |
| **W-1** | §2.3 补齐三处 `fallback_commands` 行号（约 :189/:125/:411），改「7 臂」枚举 |
| **W-2** | §2.5「`egui`」→「`eframe`」 |
| **W-3** | §2.1 注明 `everything-ipc` 在 `[target.'cfg(windows)'.dependencies]` |
| **W-4** | §1「状态图例」→「优先级图例」 |
| **W-5** | §1 O1/O2 状态「新发现」→「已知（INDEX §5，P-01/P-18）」 |
| **W-6** | §0 结论补「另含体积边际项（O5）、真机验收项（O7）与文档卫生项（O8）」 |
| **S-1** | 全篇硬编码行号统一改「约 :NNN」 |
| **S-2** | 「多處」→「多处」 |
| **Q-1** | §0「3.7 ms」补「M4 真机记录值，见 m4-record §3.7」 |
| **Q-2** | §0「内存 M1–M3」补「长会话真机复验待做」 |
| **Q-3** | §2.1 ↓21% 按最新实测值重算（8,623,616 / 10,857,984 ≈ −20.6%） |

**未改动**：文档结论方向、O1–O8 划分、Phase 1–3 排序、§2.2 协议现状表（抽检一致）均保持原样。

**登记**：本报告入 [`INDEX.md`](./INDEX.md) §3.E；`optimization-plan.md` 状态仍「规划中」。

---

> **本阶段约束复述**：本报告为只读核对 + 修订落实记录；对 `optimization-plan.md`、`INDEX.md` 的修订均在**工作副本**，**未 commit**，交由用户人工审核。
