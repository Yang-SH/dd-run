# dd-run 优化方向规划（O9–O14）

> **状态**：v1.5 落地稿｜ O9 / O10（=F4）/ O11 / **O12** 已收口（2026-10-11）；O13、O14 维持原节奏 ｜ **日期**：2026-10-10（v1.4/v1.5 落地记录 2026-10-11）｜ **输入**：独立第三轮严审 + **同日仓库逐项复核三轮**（HEAD `7606900`）
> **v1.1 校订记录**：① 删除原 O15（内存长会话真机复验）——经核实**已于 2026-10-03 由 V-13 基线收口**（见 §4），v1.0 将其列为待办属误判；② 移除对严审报告"R-1/R-2/N-1~N-3"的悬空编号引用（严审报告实际无此编号体系，且 `R-xx` 已被项目内部修订号占用，见 shell.rs R-10 / trust.rs R-12 注释）；③ O9 机制描述按源码修正为"切段后**每段**首词判定"；④ O12 前置核对已完成并给出结论；⑤ O13 文档计数按实际口径修正。
> **v1.2 校订记录**：① O9 链路类型名修正：实际为 `CommandResult::Confirm`（v1.1 误写 `ConfirmIntent`，全仓无此类型）；② O9 证据归属修正：绕过面为严审报告 **P4 已确认**发现（非本文新取证），本文补 `cmd /C`、`pwsh` 两类并做源码行级核实；③ §4 LRU 行修正：**已由 N4 落地**（2026-10-03），v1.1 误标"未立项"；④ §5 性能表述归属核准：冷启动基线不属 V-13 口径（V-13 为内存台账），改为"V 系列真机走查实测台账"；v1.0 曾误写"冷启动 200 ms"已废弃（查无该实测口径）；⑤ O10 设计要点按实际工程结构修正：`[workspace.package]` 节当前不存在、crate 为独立 `[package]` 声明；⑥ O14 补编号衔接：修复实施方案已预留 **F12（扩展签名）"单独立项"**，本文 O14 即其立项载体；⑦ §7 "O1–O8 均已闭环"表述修正为分项状态。
> **v1.3 校订记录**：① **O10 降级为 F4 落地跟踪**：查证修复实施方案 **F4 = "MSRV 声明 + 清单解析拒绝 `.cmd`/`.bat`"**，与本文 O10 事项同一且 F4 设计更全（含 manifest.rs 侧）；经 git log 逐条核实，F4 是 F 系列唯一未落地项（HEAD `7606900` 实测全仓仍无 `rust-version`、`manifest.rs:303` 仍在补全 `.cmd`/`.bat`）→ O10 不再另立方案，改为衔接跟踪，设计以 F4 为准；② 关联行与 §7 "F1–F11 已全部落地"修正为分项状态（F1–F3、F5–F11 已落地，F4 待实施）；③ §5 体积表述修正：**O5-a（default_fonts 移除）已于 2026-10-07 落地**（宿主单件 −1.35 MiB，字形穷举闭环无 tofu），不再表述为"有 tofu 风险"；不做的是 O5-b/O5-c 与 `panic=abort`；④ §4 I3 状态措辞对齐 INDEX §5 口径"保留不做（视真机）"。
> **v1.5 落地记录（2026-10-11，同日第二批）**：**O12 收口**——按 §3 设计要点执行（`settings_view/`、`settings/`、`keys/` 三目录化 + 重导出），三个独立 commit（`d191340`/`a558d0f`/`eda5c0e`），§6.3 验收全过；实施备注：拆分伴生的可见性最小改动 = 跨子模块项 `pub(super)` 化（编译器指示），tests 内层 `mod tests { }` 包装移除后由 cargo fmt 统一降级缩进。O13 维持「看 O12 落地后维护痛点再定」——拆分后三目录文件均已 <1000 行（最大 977 为 settings 单测），机检必要性下降，暂缓。
> **v1.4 落地记录（2026-10-11）**：**O9 + F4（经 O10）+ O11 按 §6 验收总标准一个批次收口**——① O9 落地：`NESTED_SHELL_HEADS`（cmd/call/powershell/pwsh/wmic/robocopy 六词，段首词命中即整段过确认门）+ 含 `^` 转义查询整体确认，新增单测 3 组 11 断言（7 条绕过样例 + `^` 例 + 回归负例）；② O10/F4 落地：6 crate 声明 `rust-version`——**声明值实测修正为 1.95**（依赖图 eframe/egui 0.36.2 要求 1.95、自有源码用 1.88 API `as_chunks`，1.77.2 为不可兑现声明；安全不变式 ≥ BatBadBut 修复线 1.77.2 仍满足，偏差说明见修复实施方案 §F4 实施版）+ `manifest.rs` 裸名只补 `.exe` + 新测试 `resolve_executable_no_longer_completes_cmd_bat` + manifest-schema.md §4 同步；③ O11 落地：两个 workflow 全部 action SHA pin（`actions/checkout@3d3c42e…`、`Swatinem/rust-cache@6323deb…`、`taiki-e/install-action@ab68953…`、`softprops/action-gh-release@efb3536…`）+ job `timeout-minutes`（check/audit 30、release 45）+ 根目录 3 个设计稿 HTML 移入 `docs/archive/`（README/文档活链接同步改写）；随批修复存量 clippy `explicit_auto_deref`（panel.rs，干净 HEAD 即触发，非本批引入）。**验收**：`cargo test --workspace` 611 passed / `clippy --workspace --all-targets -- -D warnings` 全绿 / `fmt --check` 零差异。
> **关联**：[optimization-plan.md](./optimization-plan.md)（O1–O8）· [future-features-plan.md](./future-features-plan.md)（N1–N5，已全部落地）· [dd-run-代码严审报告.md](./dd-run-代码严审报告.md) · [dd-run-修复实施方案.md](./dd-run-修复实施方案.md)（F1–F3、F5–F11 已落地；**F4 待实施，由本文 O10 跟踪**；F12 预留）· [security-audit-2026-09-23.md](./security-audit-2026-09-23.md)（S-01~S-11）· [refactor-layering-plan.md](./refactor-layering-plan.md) · [memory-optimization-plan.md](./memory-optimization-plan.md) · [INDEX.md](./INDEX.md)
> **立项判据**（沿用 future-features-plan §1 同款硬判据）：**可行**（零冻结契约改动、有既有代码惯例可复用、依赖克制、可通过单测与真机走查验收）与**无冲突**（不与冻结协议 v1.0、README 非目标、既有方案在途项、已证伪项、已占用编号空间冲突）。逐项冲突核查见 §2。

---

## 1. 结论总表（30 秒版）

| 编号 | 提案 | 一句话说明 | 协议/清单 | 依赖增量 | 优先级 | 改动量 | 依据（已逐项核实到源码/文档） |
|---|---|---|---|---|---|---|---|
| O9 | 确认门覆盖嵌套 shell | 确认门触发词表补 `cmd`/`powershell`/`pwsh`/`call`/`wmic`/`robocopy` 等，闭合"借壳执行"绕过面 | 零改动 | 零 | **P1** | 小 | 严审报告 **P4 已确认**（5 类）+ 本文补 2 类并做 shell.rs 行级核实 |
| O10 | ~~声明 MSRV~~ → **F4 落地跟踪** | 事项与修复实施方案 **F4（MSRV + 拒绝 `.cmd`/`.bat`）同一**，不另立方案；本文仅衔接跟踪与补结构核实 | 零改动 | 零 | **P1** | 极小 | 修复实施方案 §F4 已立项；git log 逐条核实 **F4 为 F 系列唯一未落地项**（HEAD 无 `rust-version`、`manifest.rs:303` 仍在补全 `.cmd`/`.bat`） |
| O11 | CI/仓库卫生三件套 | ① Actions SHA pin ② workflow 加 `timeout-minutes` ③ 根目录 HTML 设计稿归档 | 零改动 | 零 | P2 | 小 | 本文新取证（workflows 与仓库根目录核实） |
| O12 | 巨型文件拆分 | `settings_view.rs`（4104 行）等三文件按既有分层惯例拆分 | 零改动 | 零 | P2 | 中 | 两轮严审一致结论；layering plan 前置核对已完成（见 §3） |
| O13 | 文档一致性 CI 扫描 | plan 类文档"状态列"与代码符号的粗一致性机检，接在既有 docscan 之后 | 零改动 | 零 | P3 | 低–中 | 既有 doc-audit 机制的延伸；已有漂移实例佐证（见 §3） |
| O14 | 扩展签名 | 宿主内置公钥 + 作者签名，唯一能同时关闭台账"非防篡改"定性与非 Windows fail-open 的方案 | **协议增量**（v1.1 候选） | 依赖待选型 | 战略 | 大 | security-audit §4.4.2"明确不做①"+ trust.rs:14–15 自记边界；**承接修复实施方案预留的 F12** |

**建议节奏**：O9 + F4（经 O10 跟踪）一个批次收口（合计改动 < 100 行）；O11/O12 随下个常规批次；O13 看 O12 落地后的维护痛点再决定；O14 独立立项选型。

**一句话**：本项目"内功"（安全门、协议健壮性、依赖治理、测试纪律）已达标同类第一梯队，剩余价值在**对外延伸**——安全侧以 O9 + F4（O10 跟踪）低成本收口，战略侧 O14（扩展签名）与 O6（跨平台，既有在途）二选一作为下一个里程碑。

---

## 2. 冲突面核查矩阵

| 提案 | 冻结协议 v1.0 | README 非目标 | 既有在途项 | 已证伪项 | 编号空间 | 结论 |
|---|---|---|---|---|---|---|
| O9 | 不改（纯 dd-ext 内词表扩充） | 不改（仍是"护栏非沙箱"，不新增拦截语义） | 基于 F6（弹窗文案，UI-6/P4）与 R-10（切段判定）两个已落地项增固；绕过面为 P4 **已确认**发现，非推测 | 无冲突 | 不占用 S/P/F/N 已用号 | ✅ |
| O10 | 不改（沿用 F4 既定设计：逐 crate `[package]` 各加一行 + `manifest.rs` 拒绝规则） | 不改 | **同一事项已由修复实施方案 F4 立项**——本文不重复设计，改为落地跟踪（git 核实 F4 待实施）；本文 §3 仅补 `[workspace.package]` 结构核实供参考，以 F4 为准 | 无冲突 | 新号 O10 仅作跟踪索引，不产生第二份方案 | ✅ |
| O11 | 不改 | 不改 | Dependabot 双生态已在跑（`dependabot.yml`：cargo + github-actions），SHA pin 是其增强 | 无冲突 | 新号 | ✅ |
| O12 | 不改（仅 crate 内文件组织） | 不改 | **前置核对已完成（2026-10-10）**：refactor-layering-plan.md 是 `main.rs` 5123→~150 行拆分的落地后整理稿，`settings_view.rs` 正是其**产物**而非待拆项，该 plan 未覆盖 UI 层再拆分 → 可立项，按其"搬运单位 = 完整定义块、函数体一字不改"惯例执行 | 无冲突 | 新号 | ✅ |
| O13 | 不改 | 不改 | docscan（`tools/docscan.mjs`，另有 `.py` 版）+ doc-audit-2026-09-29 已存在，本项为其 CI 化延伸 | 无冲突 | 新号 | ✅ |
| O14 | **协议增量**，走 v1.1 候选流程（v1.0 冻结不破坏） | 不改（非"扩展商店"，仅签名信任） | security-audit §4.4.2"明确不做①：扩展签名/公钥验签"——当时明确不做，本文正式立项；**修复实施方案头部已预留 F12"单独立项，不在本版范围"，O14 即承接该编号的立项载体，无双编号冲突** | 无冲突 | 新号 O14（与 F12 衔接） | ✅（协议增量须走版本流程） |

---

## 3. 各提案详设

### O9 — 确认门覆盖嵌套 shell（P1）

> ✅ **已落地（2026-10-11）**：按本节设计实施，单测 3 组全绿；见 v1.4 落地记录。

**动机与证据**（严审报告 **P4 已确认**该绕过面，本文补两类并做源码行级核实）：`is_dangerous_query`（`crates/dd-ext/src/builtins/shell.rs:168`）自 R-10（2026-09-29）起按 `&`/`|`/`\r`/`\n` **切段后每段**各自过 `is_dangerous_command`（shell.rs:170–173），每段取**首词**（含扩展名剥离归一，`cmd.exe` → `cmd`）查 `DANGEROUS_COMMANDS`（12 词）与 `DANGEROUS_SUBCOMMANDS`。已知绕过面——**段首词本身是嵌套 shell 启动器时，其参数中的危险命令不触发确认**（P4 已确认 `call del`、`d^el`、`powershell -Command`、`wmic`、`robocopy /MIR` 五类；本文补 `cmd /C`、`pwsh` 两类）：

- `cmd /C del /s /q C:\x`（段首词 `cmd` 不在表内，实际执行 `del`）
- `call del …`（`call` 同理）
- `powershell -Command "Remove-Item …"`、`pwsh`、`wmic process call …`、`robocopy /MIR`（各自等价破坏力）
- `d^el …`（`^` 为 cmd 转义符，执行期还原为 `del`；`^` 使首词不匹配词表）

现有 12 词词表（`format`/`diskpart`/`cipher`/`del`/`erase`/`rd`/`rmdir`/`takeown`/`icacls`/`vssadmin`/`bcdedit`/`shutdown`）不含上述任何启动器词，绕过面成立。

**定位不变式**：本项**只扩大确认弹窗触发面，不新增拦截**，与 F6 落地的弹窗文案（shell.rs:242"dd-run 不会拦截或沙箱化任何命令"）及 R-10 既定注释（"宁可多问一次……护栏而非沙箱边界"）一致。目标场景是粘贴社交工程（用户从网页复制一整行命令），嵌套 shell 恰是该场景的主要载体。

**设计要点**（复用两个既有惯例：R-10 的切段+首词归一化管线、`DANGEROUS_SUBCOMMANDS` 的二元组表结构）：
1. 新增"嵌套 shell 头部词"集合：`cmd`、`powershell`、`pwsh`、`call`、`wmic`、`robocopy`（大小写不敏感，走现有 `split('.')` 扩展名剥离，`cmd.exe` 自动覆盖）；
2. `d^el` 类 `^` 转义：不做全文还原（误报面不可控），**对含 `^` 的查询整体弹确认**——"含转义符"本身就是可疑信号；
3. 命中后走现有 `CommandResult::Confirm` 链路（`is_critical: true`，与 shell.rs:234–248 危险命令分支同构），弹窗文案复用 F6 版本，不另写新文案。

**验收判据**：新增单测逐条覆盖上述绕过样例（P4 五类 + 本文补两类，共 7 条；每例断言返回 `Confirm` 而非 `ShowToast`）；既有非危险命令样例（`echo hello`、`git status`）回归不弹确认；真机走查一例正向 + 一例绕过。

**改动面**：仅 `shell.rs` 词表与判定函数 + 单测，约 < 60 行。

---

### O10 — MSRV（P1）＝修复实施方案 F4 落地跟踪

> ✅ **已落地（2026-10-11，O10 随之销项）**：F4 实施完成；MSRV 声明值经实测修正为 1.95（详见修复实施方案 §F4 实施版注记与 v1.4 落地记录），安全不变式 ≥1.77.2 仍满足。

> **v1.3 变更**：查证后确认本事项与修复实施方案 **F4（"MSRV 声明 + 清单解析拒绝 `.cmd`/`.bat`"，§231 起）为同一事项**，且 F4 设计更完整（含 manifest.rs 侧）。按"不重复立项"判据，本文**不再另立方案**，O10 降级为 F4 的落地跟踪索引，设计、批次与 commit 均以 F4 为准。

**落地状态核实（2026-10-10，HEAD `7606900`）**：F 系列经 git log 逐条核对——F1（`9640b3b`）、F2/F5/F6（`de081f8`）、F3（`8e0eb88`）、F7（`2afa214`）、F8（`18e85e8`）、F9/F10/F11（`7606900`）均已落地；**F4 无对应提交，为 F 系列唯一未落地项**。源码佐证：全仓仍无 `rust-version`（根 `Cargo.toml` 与各 crate manifest 均无），`manifest.rs:303` 仍按 `[".exe", ".cmd", ".bat"]` 补全。

**本文补充核实（供实施参考，不改 F4 设计）**：根 `[workspace]` 无 `[workspace.package]` 节、各 crate 为独立 `[package]` 声明（已核实 `crates/dd-ext/Cargo.toml` 等）；F4 采用的逐 crate 各加一行（×6）方案与现状结构吻合、改动最小，无需引入 workspace 继承。MSRV 取值以 F4 v1.1 修订为准（**1.77.2**，CVE-2024-24576 修复线）。

**验收判据**：直接沿用修复实施方案 §F4——`cargo test -p dd-host`；`rg "rust-version" crates/` 能命中；旧工具链构建被拒。落地后在修复实施方案 F4 节与 CHANGELOG 回写，O10 随之销项。

---

### O11 — CI/仓库卫生三件套（P2）

> ✅ **已落地（2026-10-11）**：SHA pin（4 个 action 取 deref 后 commit SHA + 行尾版本注释）+ `timeout-minutes`（check/audit 30、release 45）+ 3 个 HTML 归档 `docs/archive/`（README/文档活链接同步改写、INDEX §3.G 注记更新）。

依据（本文新取证，workflows 与仓库根目录核实）：

1. **Actions SHA pin**：`ci.yml`/`release.yml` 全部 action（`actions/checkout@v7`、`Swatinem/rust-cache@v2`、`taiki-e/install-action@v2`、`softprops/action-gh-release@v3`）按 major tag 引用，tag 可被强制推送。改为 `action@<commit-SHA>` + 行尾注释版本号；Dependabot 的 github-actions 生态已在跑（`dependabot.yml` 已登记），后续升级 PR 自动跟进 SHA。
2. **`timeout-minutes`**：两个 workflow 的 job 均未设置（已核实全部 job 无该字段），挂起 job 默认占满 6 小时配额。建议 check 类 30 分钟、release 类 45 分钟。
3. **根目录 HTML 归档**：`cmdpal-ui-mockups.html`、`cmdpal-ui-optimization-v5.html`、`cmdpal-window-material-effects.html` 为设计稿，移入 `docs/archive/`（该目录已存在，有 doc-audit-2026-09-13 等归档先例）。

**验收判据**：workflow 内 action 全部 SHA 引用；人为 sleep 的探针 job 能被 timeout 截断（可仅代码评审确认）；根目录无散落 HTML。

**改动面**：约 10 行 workflow + 3 个文件移动。

---

### O12 — 巨型文件拆分（P2，前置核对已完成）

> ✅ **已落地（2026-10-11）**：三文件全部拆分——settings_view.rs 4104→7 文件（mod 488 + appearance 839 / widgets 774 / general 764 / search 557 / extensions 497 / hotkey_dialog 262）、settings.rs 2299→4 文件（mod 754 + prefs 540 / tests 977 / search 52）、keys.rs 1330→4 文件（mod 176 + settings_actions 728 / navigation 254 / hotkey_capture 202）。零行为改动（行级 multiset 核对），外部路径经重导出不变；每文件独立 commit。验收：611 passed / clippy `-D warnings` / fmt / docscan 全绿 + conformance（calc/bookmarks）与 roundtrip 自检通过。

**动机**（两轮严审一致结论，行数已核实）：`crates/dd-gui/src/ui/settings_view.rs` 4104 行、`crates/dd-gui/src/settings.rs` 2299 行、`crates/dd-gui/src/app/keys.rs` 1330 行。函数级组织尚可，但检索、评审、合并冲突成本随行数上涨。

**前置核对结论（2026-10-10 完成）**：`refactor-layering-plan.md` 是 `main.rs`（5123 行 → ~150 行入口）拆分的**落地后整理稿**，其拆分产物即含 `ui/settings_view.rs`（该 plan 行号映射表可见 `draw_ext_chip`/`draw_version_chip` 等落入 settings_view）。该 plan **未覆盖** UI 层巨型文件的再拆分 → 本项可立项，且直接沿用其既定纪律："搬运单位 = 完整函数/常量/结构体定义块（含文档注释），函数体一字不改"。

**设计要点**：按既有页面/分区惯例拆子模块（如 `settings_view/` 目录 + `mod.rs` 索引），**纯文件移动 + `pub use` 重导出，零行为改动**；拆分批次单独成 commit，禁止与功能改动混批。

**验收判据**：拆分前后 `cargo test --workspace` 全绿、行为等价（roundtrip + conformance 自检通过）、无新增 clippy 警告。

---

### O13 — 文档一致性 CI 扫描（P3）

**动机**：`docs/` 现有 23 篇 md（其中 plan 类 10 份，2026-10-10 实测口径），plan 与实现的漂移目前依赖人工纪律（`tools/docscan.mjs` + doc-audit-2026-09-29 已治理一轮）。**漂移实例（本文实测）**：`optimization-plan.md` 头部结论仍写"内存 M1–M3 已落地『长会话真机复验待做』"，而 memory-plan 侧 V-13 基线已于 2026-10-03 收口（见 §4）——同一状态在两份文档中口径不一致，正是机检要拦的漂移类型。

**设计要点**：在 `tools/docscan.mjs` 基础上追加机检规则（本地脚本 + CI 可选 job）：① plan 文档"状态列"声明为 ✅ 已落地的项，抽验其引用的代码符号/文件是否存在（grep 级）；② 状态"在途"项列出清单供评审；③ 明确不做强一致（注释措辞多样，只做"符号存在性"这一层粗检，避免误报卡 CI）。

**验收判据**：对既有文档全量跑一遍，零误报；人为构造一处失配能被检出。

**节奏**：建议 O12 落地后视维护痛点再决定是否立项，避免为少量活跃文档建全套机制。

---

### O14 — 扩展签名（战略级）

**动机**（两处边界均已核实到源码/审计原文）：
1. **台账非防篡改**：trust.rs:14–15 自记"**不是防篡改**。能写 `extensions.d` 的攻击者同样能写 `trust.json`。真正意义上的防篡改需要扩展签名（宿主内置公钥 + 作者签名），**不在本项范围**"；security-audit §4.4.2"明确不做①：扩展签名/公钥验签"。
2. **非 Windows fail-open**：trust.rs:288–291——`if !hash_available() { Trust::AutoTrusted }`（注释"D2 缺口声明：非 Windows 不做门禁"），即无哈希能力平台全部自动信任。

签名是唯一能同时闭合两处的方案，也是将来开放第三方分发生态的前置。

**设计要点（选型阶段，本文不锁死）**：
1. 密钥方案：宿主内置公钥（ed25519 系），Release 工作流私钥签名 sidecar exe + 清单；
2. 协议影响：清单 schema 增加签名字段（或旁挂 `.sig` 文件）→ **属协议增量，走 v1.1 候选流程，v1.0 冻结不破坏**；
3. 降级路径：无签名/验签失败 → 回落现有 S-05 用户同意流（不比现状差）；
4. 与 O6（跨平台）的关系：签名机制平台无关，可先行；跨平台侧只是哈希/验签 API 替换（Windows CNG `BCrypt` → 各平台原语），恰好顺带消解 trust.rs 的 D2 缺口。

**验收判据**（框架级）：合法签名首方扩展免确认拉起；篡改一字节后拒绝并回落 Pending；无签名第三方走既有同意流。

**改动面**：大（协议 + 宿主 + 打包链 + 文档），建议独立里程碑，与 O6 二选一作为下一主战役。

---

## 4. 既有在途项提醒（不重复立项，仅登记）

| 项 | 归属 | 状态 |
|---|---|---|
| 跨平台 macOS/Linux | **O6**（optimization-plan，战略，INDEX §5 已知） | 在途，本文不重编；若选为下一主战役，与 O14 二选一 |
| I3 32px 图标回退 | 既有方案标注"视真机" | 保留不做（视真机效果再定，不阻塞交付——INDEX §5 口径） |
| LRU 容量可配 | **N4**（future-features-plan §4.4） | **✅ 已落地（2026-10-03，`Settings.warm_capacity`）**——O8 当时"转功能项"的结论已由 N 系列兑现，无遗留 |
| **内存长会话真机复验** | memory-optimization-plan V-13 基线 | **✅ 已收口（2026-10-03）**：500 次 `WM_HOTKEY` 显隐循环 PRIV +12.2 MB 门限内 + 托盘常驻 10.4 h 恒 104.4 MB 零增长（`implementation.md` §3.1.1）。注：`optimization-plan.md` 头部仍残留"待做"旧表述，以本行为准（该残留即 §3 O13 的漂移实例） |

## 5. 明确不做（防错漏 / 防冲突）

- **体积再压缩（进一步投入）**：O5-a（default_fonts 移除）**已于 2026-10-07 落地**（宿主单件 −1.35 MiB，字形穷举闭环无 tofu——v1.2 前版本"有 tofu 风险"的表述已过时）；O5-b/O5-c 明确不做/暂缓，`panic=abort` 已证伪。体积已达标，不再投入；
- **扩展商店 / 云同步 / 遥测 / 账号**：README 非目标，N 系列规划明确排除，维持；
- **性能/内存微优化**：F8 已消除每帧深拷贝、冷启动与内存基线均有 V 系列真机走查实测台账（内存收口见 `implementation.md` §3.1.1 V-13），当前热路径**没有新的确定性收益点**，禁止无基线的"顺手优化"；
- **危险命令拦截/沙箱化**：O9 也**不改变**"护栏非沙箱"定位，不新增任何拦截语义。

## 6. 验收总标准

1. O9 / **F4（经 O10 跟踪）** / O11 合并为一个收口批次，`cargo test --workspace` + clippy `-D warnings` + fmt 全绿；
2. O9 绕过样例单测逐条可复现（先红后绿）；
3. O12 拆分批次 commit 独立、行为等价自检通过；
4. 本文档登记入 INDEX.md，O9–O14 编号在后续文档中沿用。

## 7. 与既有文档关系

- O9 证据归属：绕过面已在严审报告 P4 确认、本文补两类并核实到行级；O10＝F4 跟踪（收口动作在修复实施方案 F4 节回写）；O11 为本文新取证——收口后在严审报告追加落地注记；
- **F 系列状态（git 逐条核实，2026-10-10；v1.4 落地后 2026-10-11 更新）**：**F1–F11 已全部落地（F4 于 2026-10-11 收口）**；F12（扩展签名）预留"单独立项"，由本文 O14 承接。N1–N5 已全部落地且真机走查收口；O1–O4/O7/O8 已落地、O5-a 已落地而 O5-b/c 与 `panic=abort` 不做/证伪（§5）、O6 战略在途（§4）——本文不重开任何已收口项；
- O14 若立项，即 F12 的立项载体，security-audit 附录续编号入账（沿用 S-xx 续位）。

---

*依据：独立第三轮严审 + 同日仓库复核（静态精读，HEAD `7606900`，未做编译/运行复现）；文中行号与数值均以 2026-10-10 实测为准。*
