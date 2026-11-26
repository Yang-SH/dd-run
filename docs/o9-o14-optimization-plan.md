# dd-run 优化方向规划（O9–O14）

> **状态**：已落地 ｜ **版本**：v1.11 ｜ **O9–O14 全部收口**（O9 / O10（=F4）/ O11 / O12 于 2026-10-11；**O13 于 2026-10-17**；O14 由 extension-signing-plan 独立立项并已发版验证，P0-2 于 2026-11-25 销项）｜ **最后更新**：2026-11-25（首版 2026-10-10）｜ **输入**：独立第三轮严审 + **同日仓库逐项复核三轮**（HEAD `7606900`）
> **v1.1 校订记录**：① 删除原 O15（内存长会话真机复验）——经核实**已于 2026-10-03 由 V-13 基线收口**（见 §4），v1.0 将其列为待办属误判；② 移除对严审报告"R-1/R-2/N-1~N-3"的悬空编号引用（严审报告实际无此编号体系，且 `R-xx` 已被项目内部修订号占用，见 shell.rs R-10 / trust.rs R-12 注释）；③ O9 机制描述按源码修正为"切段后**每段**首词判定"；④ O12 前置核对已完成并给出结论；⑤ O13 文档计数按实际口径修正。
> **v1.2 校订记录**：① O9 链路类型名修正：实际为 `CommandResult::Confirm`（v1.1 误写 `ConfirmIntent`，全仓无此类型）；② O9 证据归属修正：绕过面为严审报告 **P4 已确认**发现（非本文新取证），本文补 `cmd /C`、`pwsh` 两类并做源码行级核实；③ §4 LRU 行修正：**已由 N4 落地**（2026-10-03），v1.1 误标"未立项"；④ §5 性能表述归属核准：冷启动基线不属 V-13 口径（V-13 为内存台账），改为"V 系列真机走查实测台账"；v1.0 曾误写"冷启动 200 ms"已废弃（查无该实测口径）；⑤ O10 设计要点按实际工程结构修正：`[workspace.package]` 节当前不存在、crate 为独立 `[package]` 声明；⑥ O14 补编号衔接：修复实施方案已预留 **F12（扩展签名）"单独立项"**，本文 O14 即其立项载体；⑦ §7 "O1–O8 均已闭环"表述修正为分项状态。
> **v1.3 校订记录**：① **O10 降级为 F4 落地跟踪**：查证修复实施方案 **F4 = "MSRV 声明 + 清单解析拒绝 `.cmd`/`.bat`"**，与本文 O10 事项同一且 F4 设计更全（含 manifest.rs 侧）；经 git log 逐条核实，F4 是 F 系列唯一未落地项（HEAD `7606900` 实测全仓仍无 `rust-version`、`manifest.rs` 仍在补全 `.cmd`/`.bat`）→ O10 不再另立方案，改为衔接跟踪，设计以 F4 为准；② 关联行与 §7 "F1–F11 已全部落地"修正为分项状态（F1–F3、F5–F11 已落地，F4 待实施）；③ §5 体积表述修正：**O5-a（default_fonts 移除）已于 2026-10-07 落地**（宿主单件 −1.35 MiB，字形穷举闭环无 tofu），不再表述为"有 tofu 风险"；不做的是 O5-b/O5-c 与 `panic=abort`；④ §4 I3 状态措辞对齐 INDEX §5 口径"保留不做（视真机）"。
> **v1.11 审计修正（2026-11-25）**：O14 降级路径改为“Absent 回落 S-05、Invalid 直接 Pending”；补空钉扎版本 fail-closed 回归。
> **v1.10 收口记录（2026-11-25）**：O14/P0-2 首方 sidecar 用户 Deny 优先已修复，54 组合矩阵通过，O9–O14 全部收口。
> **v1.9 校订记录（2026-11-25）**：O14 当前表改为零协议改动、旁挂 `.sig`；冲突矩阵同步“已落地/P0-2 待修”（时点记录）。
> **v1.8 校订记录（2026-11-25）**：O14 改为“已落地、P0-2 待修”，不再宣称 O9–O14 全部收口；其余历史验收记录不变。
> **v1.7 校订记录（2026-11-25）**：结论总表 O10/O13、O14 改动面、§4 O6 状态与 §7 关系说明改为当前完成态；消除顶部“已收口”与正文实施前快照的矛盾，历史验收记录不变。
> **v1.6 落地记录（2026-10-17）**：**O13 收口**（收口批）——`tools/docscan.mjs` 在既有「跨文件链接可达 + INDEX 行数登记」两项之外新增**第③项「文档引用文件存在性」**：只看反引号包裹的**仓库根相对路径**（`crates/` `tools/` `docs/` `examples/` `.github/` 前缀 + 已知文件扩展名；排除通配符 / 变量 / **简写源码路径**（`ui/panel.rs` 类）/ 上游或外部引用（`src/modules/cmdpal/…`、`file://…`、`~/.config/…`）），**高精度优先**；`ci.yml` Windows job 增 `node tools/docscan.mjs` 收口门（与「文档回写六处」第⑥步同源）。**验收**：全量实跑 **78 条唯一路径、0 误报**（首轮宽口径 113 条中 111 条为简写源码路径 / 上游引用类噪音，据此收敛规则）；人为构造失配（临时文件引用一个不存在的 `tools/` 路径）**被检出 exit 1**、删除后复跑 exit 0。**随批实修**：机检首跑即抓出 **4 处归档后未更新的链接标签**——`[`docs/m9-inprocess-builtins.md`]` → `docs/archive/…`（cmdpal-platform-agnostic-design.md ×2 / implementation.md ×1）、`[`docs/doc-code-diff-2026-09-13.md`]` → `docs/archive/…`（CHANGELOG.md ×1）。
> **v1.5 落地记录（2026-10-11，同日第二批）**：**O12 收口**——按 §3 设计要点执行（`settings_view/`、`settings/`、`keys/` 三目录化 + 重导出），三个独立 commit（`d191340`/`a558d0f`/`eda5c0e`），§6.3 验收全过；实施备注：拆分伴生的可见性最小改动 = 跨子模块项 `pub(super)` 化（编译器指示），tests 内层 `mod tests { }` 包装移除后由 cargo fmt 统一降级缩进。O13 维持「看 O12 落地后维护痛点再定」——拆分后三目录文件均已 <1000 行（最大 977 为 settings 单测），机检必要性下降，暂缓。
> **v1.4 落地记录（2026-10-11）**：**O9 + F4（经 O10）+ O11 按 §6 验收总标准一个批次收口**——① O9 落地：`NESTED_SHELL_HEADS`（cmd/call/powershell/pwsh/wmic/robocopy 六词，段首词命中即整段过确认门）+ 含 `^` 转义查询整体确认，新增单测 3 组 11 断言（7 条绕过样例 + `^` 例 + 回归负例）；② O10/F4 落地：6 crate 声明 `rust-version`——**声明值实测修正为 1.95**（依赖图 eframe/egui 0.36.2 要求 1.95、自有源码用 1.88 API `as_chunks`，1.77.2 为不可兑现声明；安全不变式 ≥ BatBadBut 修复线 1.77.2 仍满足，偏差说明见修复实施方案 §F4 实施版）+ `manifest.rs` 裸名只补 `.exe` + 新测试 `resolve_executable_no_longer_completes_cmd_bat` + manifest-schema.md §4 同步；③ O11 落地：两个 workflow 全部 action SHA pin（`actions/checkout@3d3c42e…`、`Swatinem/rust-cache@6323deb…`、`taiki-e/install-action@ab68953…`、`softprops/action-gh-release@efb3536…`）+ job `timeout-minutes`（check/audit 30、release 45）+ 根目录 3 个设计稿 HTML 移入 `docs/archive/`（README/文档活链接同步改写）；随批修复存量 clippy `explicit_auto_deref`（panel.rs，干净 HEAD 即触发，非本批引入）。**验收**：`cargo test --workspace` 611 passed / `clippy --workspace --all-targets -- -D warnings` 全绿 / `fmt --check` 零差异。
> **关联**：[optimization-plan.md](./optimization-plan.md)（O1–O8）· [future-features-plan.md](./future-features-plan.md)（N1–N5，已全部落地）· [dd-run-代码严审报告.md](./dd-run-代码严审报告.md) · [dd-run-修复实施方案.md](./dd-run-修复实施方案.md)（**F1–F11 已全部落地**，F4 经本文 O10 跟踪；F12 由 [extension-signing-plan.md](./extension-signing-plan.md) 承接并已发版）· [security-audit-2026-09-23.md](./security-audit-2026-09-23.md)（S-01~S-11）· [refactor-layering-plan.md](./refactor-layering-plan.md) · [memory-optimization-plan.md](./memory-optimization-plan.md) · [INDEX.md](./INDEX.md)
> **立项判据**（沿用 future-features-plan §1 同款硬判据）：**可行**（零冻结契约改动、有既有代码惯例可复用、依赖克制、可通过单测与真机走查验收）与**无冲突**（不与冻结协议 v1.0、README 非目标、既有方案在途项、已证伪项、已占用编号空间冲突）。逐项冲突核查见 §2。

---

## 1. 结论总表（30 秒版）

| 编号 | 提案 | 一句话说明 | 协议/清单 | 依赖增量 | 优先级 | 改动量 | 依据（已逐项核实到源码/文档） |
|---|---|---|---|---|---|---|---|
| O9 | 确认门覆盖嵌套 shell | 确认门触发词表补 `cmd`/`powershell`/`pwsh`/`call`/`wmic`/`robocopy` 等，闭合"借壳执行"绕过面 | 零改动 | 零 | **P1** | 小 | 严审报告 **P4 已确认**（5 类）+ 本文补 2 类并做 shell.rs 行级核实 |
| O10 | ~~声明 MSRV~~ → **F4 已落地跟踪** | F4 已完成：6 crate `rust-version=1.95`；清单解析拒绝 `.cmd`/`.bat` | 零改动 | 零 | **P1** | 极小 | 2026-10-11 落地；详细验收见 F4 实施版与 v1.4 记录 |
| O11 | CI/仓库卫生三件套 | ① Actions SHA pin ② workflow 加 `timeout-minutes` ③ 根目录 HTML 设计稿归档 | 零改动 | 零 | P2 | 小 | 本文新取证（workflows 与仓库根目录核实） |
| O12 | 巨型文件拆分 | `settings_view.rs`（4104 行）等三文件按既有分层惯例拆分 | 零改动 | 零 | P2 | 中 | 两轮严审一致结论；layering plan 前置核对已完成（见 §3） |
| O13 | 文档一致性 CI 扫描 | 已落地：跨文件链接、INDEX 行数、引用路径存在性与行号锚机检 | 零改动 | 零 | P3 | 低–中 | `tools/docscan.mjs` 已接入 Windows CI；语义漂移仍由人工核对承接 |
| O14 | 扩展签名 | 已收口：宿主内置公钥 + 清单旁挂 `.sig`；用户 Deny 优先（P0-2 已销项） | **零协议改动**（旁挂 `.sig`） | ed25519-dalek/sha2（已入库） | 战略 | 大 | security-audit S-12；F12 承接；54 组合矩阵与全仓门禁通过 |

**历史节奏（已执行完毕）**：O9 + F4 于 2026-10-11 同批收口；O11/O12 同日完成；O13 于 2026-10-17 收口；O14 独立立项并已发版验证。

**一句话**：O9–O14 已全部收口；首方 sidecar 用户 Deny 优先级缺陷 P0-2 已修复并销项。F4/O10 无待实施项。

---

## 2. 冲突面核查矩阵

| 提案 | 冻结协议 v1.0 | README 非目标 | 既有在途项 | 已证伪项 | 编号空间 | 结论 |
|---|---|---|---|---|---|---|
| O9 | 不改（纯 dd-ext 内词表扩充） | 不改（仍是"护栏非沙箱"，不新增拦截语义） | 基于 F6（弹窗文案，UI-6/P4）与 R-10（切段判定）两个已落地项增固；绕过面为 P4 **已确认**发现，非推测 | 无冲突 | 不占用 S/P/F/N 已用号 | ✅ |
| O10 | 不改（沿用 F4 既定设计：逐 crate `[package]` 各加一行 + `manifest.rs` 拒绝规则） | 不改 | **同一事项已由修复实施方案 F4 立项**——本文不重复设计，改为落地跟踪（git 核实 F4 待实施）；本文 §3 仅补 `[workspace.package]` 结构核实供参考，以 F4 为准 | 无冲突 | 新号 O10 仅作跟踪索引，不产生第二份方案 | ✅ |
| O11 | 不改 | 不改 | Dependabot 双生态已在跑（`dependabot.yml`：cargo + github-actions），SHA pin 是其增强 | 无冲突 | 新号 | ✅ |
| O12 | 不改（仅 crate 内文件组织） | 不改 | **前置核对已完成（2026-10-10）**：refactor-layering-plan.md 是 `main.rs` 5123→~150 行拆分的落地后整理稿，`settings_view.rs` 正是其**产物**而非待拆项，该 plan 未覆盖 UI 层再拆分 → 可立项，按其"搬运单位 = 完整定义块、函数体一字不改"惯例执行 | 无冲突 | 新号 | ✅ |
| O13 | 不改 | 不改 | docscan（`tools/docscan.mjs`，另有 `.py` 版）+ doc-audit-2026-09-29 已存在，本项为其 CI 化延伸 | 无冲突 | 新号 | ✅ |
| O14 | 不改（旁挂 `.sig`，协议/清单字节零改） | 不改（非"扩展商店"，仅签名信任） | security-audit 原“明确不做①”已由本项正式立项；F12 承接 | 无冲突 | O14/F12 已占用 | ✅ 已收口（P0-2 已销项） |

---

## 3. 各提案详设

### O9 — 确认门覆盖嵌套 shell（P1）

> ✅ **已落地（2026-10-11）**：按本节设计实施，单测 3 组全绿；见 v1.4 落地记录。

**动机与证据**（严审报告 **P4 已确认**该绕过面，本文补两类并做源码行级核实）：`is_dangerous_query`（`crates/dd-ext/src/builtins/shell.rs` 的 `is_dangerous_query`）自 R-10（2026-09-29）起按 `&`/`|`/`\r`/`\n` **切段后每段**各自过 `is_dangerous_command`（同文件切段循环），每段取**首词**（含扩展名剥离归一，`cmd.exe` → `cmd`）查 `DANGEROUS_COMMANDS`（12 词）与 `DANGEROUS_SUBCOMMANDS`。已知绕过面——**段首词本身是嵌套 shell 启动器时，其参数中的危险命令不触发确认**（P4 已确认 `call del`、`d^el`、`powershell -Command`、`wmic`、`robocopy /MIR` 五类；本文补 `cmd /C`、`pwsh` 两类）：

- `cmd /C del /s /q C:\x`（段首词 `cmd` 不在表内，实际执行 `del`）
- `call del …`（`call` 同理）
- `powershell -Command "Remove-Item …"`、`pwsh`、`wmic process call …`、`robocopy /MIR`（各自等价破坏力）
- `d^el …`（`^` 为 cmd 转义符，执行期还原为 `del`；`^` 使首词不匹配词表）

现有 12 词词表（`format`/`diskpart`/`cipher`/`del`/`erase`/`rd`/`rmdir`/`takeown`/`icacls`/`vssadmin`/`bcdedit`/`shutdown`）不含上述任何启动器词，绕过面成立。

**定位不变式**：本项**只扩大确认弹窗触发面，不新增拦截**，与 F6 落地的弹窗文案（shell.rs 的 `is_dangerous_command` 确认分支："dd-run 不会拦截或沙箱化任何命令"）及 R-10 既定注释（"宁可多问一次……护栏而非沙箱边界"）一致。目标场景是粘贴社交工程（用户从网页复制一整行命令），嵌套 shell 恰是该场景的主要载体。

**设计要点**（复用两个既有惯例：R-10 的切段+首词归一化管线、`DANGEROUS_SUBCOMMANDS` 的二元组表结构）：
1. 新增"嵌套 shell 头部词"集合：`cmd`、`powershell`、`pwsh`、`call`、`wmic`、`robocopy`（大小写不敏感，走现有 `split('.')` 扩展名剥离，`cmd.exe` 自动覆盖）；
2. `d^el` 类 `^` 转义：不做全文还原（误报面不可控），**对含 `^` 的查询整体弹确认**——"含转义符"本身就是可疑信号；
3. 命中后走现有 `CommandResult::Confirm` 链路（`is_critical: true`，与 shell.rs 的危险命令分支同构），弹窗文案复用 F6 版本，不另写新文案。

**验收判据**：新增单测逐条覆盖上述绕过样例（P4 五类 + 本文补两类，共 7 条；每例断言返回 `Confirm` 而非 `ShowToast`）；既有非危险命令样例（`echo hello`、`git status`）回归不弹确认；真机走查一例正向 + 一例绕过。

**改动面**：仅 `shell.rs` 词表与判定函数 + 单测，约 < 60 行。

---

### O10 — MSRV（P1）＝修复实施方案 F4 落地跟踪

> ✅ **已落地（2026-10-11，O10 随之销项）**：F4 实施完成；MSRV 声明值经实测修正为 1.95（详见修复实施方案 §F4 实施版注记与 v1.4 落地记录），安全不变式 ≥1.77.2 仍满足。

> **v1.3 变更**：查证后确认本事项与修复实施方案 **F4（"MSRV 声明 + 清单解析拒绝 `.cmd`/`.bat`"，§231 起）为同一事项**，且 F4 设计更完整（含 manifest.rs 侧）。按"不重复立项"判据，本文**不再另立方案**，O10 降级为 F4 的落地跟踪索引，设计、批次与 commit 均以 F4 为准。

**落地状态核实（2026-10-10，HEAD `7606900`）**：F 系列经 git log 逐条核对——F1（`9640b3b`）、F2/F5/F6（`de081f8`）、F3（`8e0eb88`）、F7（`2afa214`）、F8（`18e85e8`）、F9/F10/F11（`7606900`）均已落地；**F4 无对应提交，为 F 系列唯一未落地项**。源码佐证：全仓仍无 `rust-version`（根 `Cargo.toml` 与各 crate manifest 均无），`manifest.rs` 仍按 `[".exe", ".cmd", ".bat"]` 补全（F4 落地前状态）。

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

> ✅ **已落地（2026-10-11）**：三文件全部拆分——settings_view.rs 4104→7 文件（mod 486 + appearance 843 / widgets 772 / general 762 / search 556 / extensions 496 / hotkey_dialog 260）、settings.rs 2299→4 文件（mod 759 + prefs 534 / tests 976 / search 52）、keys.rs 1330→4 文件（mod 176 + settings_actions 728 / navigation 254 / hotkey_capture 202；行数为 cargo fmt 后终态）。零行为改动（行级 multiset 核对），外部路径经重导出不变；每文件独立 commit。验收：611 passed / clippy `-D warnings` / fmt / docscan 全绿 + conformance（calc/bookmarks）与 roundtrip 自检通过。

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

**落地注记（2026-10-17）**：O12 落地后机检必要性虽下降，但「文档回写六处」第⑥步已把 `docscan` 作为收口门、且本次收口批实修出 4 处归档陈旧标签——故按**第①项（引用存在性）以高精度规则落地**，第②项（在途清单）与第③项（强一致）维持不做（只做「存在性」一层粗检，避免误报卡 CI）。规则与验收见文首 v1.6 落地记录。

---

### O14 — 扩展签名（战略级）

**动机**（两处边界均已核实到源码/审计原文）：
1. **台账非防篡改**：`trust.rs` 模块文档自记"**不是防篡改**。能写 `extensions.d` 的攻击者同样能写 `trust.json`。真正意义上的防篡改需要扩展签名（宿主内置公钥 + 作者签名），**不在本项范围**"；security-audit §4.4.2"明确不做①：扩展签名/公钥验签"。
2. **非 Windows fail-open**：原 `trust.rs` 288–291 行——`if !hash_available() { Trust::AutoTrusted }`（注释"D2 缺口声明：非 Windows 不做门禁"），即无哈希能力平台全部自动信任（O14 落地后该分支已由签名门禁取代）。

签名是唯一能同时闭合两处的方案，也是将来开放第三方分发生态的前置。

**设计要点（选型阶段，本文不锁死）**：
1. 密钥方案：宿主内置公钥（ed25519 系），Release 工作流私钥签名 sidecar exe + 清单；
2. 协议影响：清单 schema 增加签名字段（或旁挂 `.sig` 文件）→ **属协议增量，走 v1.1 候选流程，v1.0 冻结不破坏**；
3. 降级路径：无签名 → 回落现有 S-05/钉扎链；验签失败 → `Pending` + 告警（fail-closed，不回落同意流）；
4. 与 O6（跨平台）的关系：签名机制平台无关，可先行；跨平台侧只是哈希/验签 API 替换（Windows CNG `BCrypt` → 各平台原语），恰好顺带消解 trust.rs 的 D2 缺口。

**验收判据**（框架级）：合法签名首方扩展免确认拉起；篡改一字节后拒绝并回落 Pending；无签名第三方走既有同意流。

**改动面**：大（宿主 + 打包链 + 文档），已作为独立里程碑实施完毕；O6 与 O14 的“二选一”是立项时历史建议，现已双双落地。

---

## 4. 既有在途项提醒（不重复立项，仅登记）

| 项 | 归属 | 状态 |
|---|---|---|
| 跨平台 macOS/Linux | **O6**（optimization-plan，战略） | 代码与三平台发布已落地；仅余非 Windows 真机腿（清单 P1-10/P1-13） |
| I3 32px 图标回退 | 既有方案标注"视真机" | 保留不做（视真机效果再定，不阻塞交付——INDEX §5 口径） |
| LRU 容量可配 | **N4**（future-features-plan §4.4） | **✅ 已落地（2026-10-03，`Settings.warm_capacity`）**——O8 当时"转功能项"的结论已由 N 系列兑现，无遗留 |
| **内存长会话真机复验** | memory-optimization-plan V-13 基线 | **✅ 已收口（2026-10-03）**：500 次 `WM_HOTKEY` 显隐循环 PRIV +12.2 MB 门限内 + 托盘常驻 10.4 h 恒 104.4 MB 零增长（`implementation.md` §3.1.1 V-13）。历史“optimization-plan 头部待做”漂移已在 v1.14 修正 |

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
- **F 系列状态（git 逐条核实，2026-10-10；v1.4 落地后 2026-10-11 更新）**：**F1–F11 已全部落地（F4 于 2026-10-11 收口）**；F12（扩展签名）由本文 O14 承接并已发版。N1–N5 已全部落地且真机走查收口；O1–O4/O7/O8 已落地、O5-a 已落地而 O5-b/c 与 `panic=abort` 不做/证伪；O6 代码与三平台发布已落地，仅余真机腿——本文不重开任何已收口项；
- O14 若立项，即 F12 的立项载体，security-audit 附录续编号入账（沿用 S-xx 续位）。

---

*依据：独立第三轮严审 + 同日仓库复核（静态精读，HEAD `7606900`，未做编译/运行复现）；文中行号与数值均以 2026-10-10 实测为准。*
