# 全局热键默认值切换为 Alt+Space 方案

> **状态**：已落地（代码与文档回写完成；真机 6 条待用户会话） ｜ **版本**：v1.5 ｜ **最后更新**：2026-11-23
> **关联**：[implementation.md](./implementation.md) · [stability-usability-security-plan.md](./stability-usability-security-plan.md)（R-21） · [altrun-对照优化方案-2026-11-21.md](./altrun-对照优化方案-2026-11-21.md)

---

## 1. 结论与范围

**决策**：全局热键默认值由 `Win+Alt+Space` 改为 `Alt+Space`（`HOTKEY_MODS_DEFAULT` 由 `0b1001` 改为 `0b0001`，主键不变），对齐 ALTRun 的默认唤起键（ALTRun 默认 Alt+Space 属外部已知事实；关联的本仓库对照文档未记载该键位，待补来源）。

**关键机制（实测结论，非推断）**：`RegisterHotKey` 注册 `MOD_ALT + VK_SPACE` 成功后，Windows 在输入层把**整个组合**从投递链路摘除——前台窗口连 `WM_SYSKEYDOWN(VK_MENU)` 都收不到，`SC_KEYMENU`（系统菜单）无从触发。因此**不需要低级键盘钩子、不需要子类化窗口过程、不需要改窗口样式**；「屏蔽 Alt+Space 唤出系统菜单」是注册成功后的天然结果，代价是该组合被 dd-run 全局接管（其他程序的窗口菜单同时失效）。

**范围**：仅 `dd-gui` 的默认值常量、相关文案与 R-21 黑名单语义；不涉协议、清单、数据结构；**已存在用户配置不迁移**（尊重用户现值，需要时用设置页「恢复默认」一键切换）。

**已落地**：截至本文 v1.3，批次一（C1–C9 代码 + 单测）与批次二（文档回写）均已完成；验收数字见 §6，真机 6 条待用户会话（前置见 §5）。

---

## 2. 取证

### 2.1 真机实测（2026-11-23，本机 Windows）

方法：ctypes 自建带 `WS_SYSMENU` 的窗口并抢前台，`keybd_event` 注入 `Alt↓ Space↓ Space↑ Alt↑`，由窗口过程记录实际收到的消息；两轮对照（未注册 / 已注册线程级热键）。探针脚本为一次性临时脚本，事后未留存（仓库与 `target/` 均无副本；`target/` 已 gitignore，不构成仓库依赖）——本轮结论按记档采信，不可从工作区复现。

| 轮次 | 注册动作 | 前台窗口收到 | 热键线程 | 系统菜单 |
|---|---|---|---|---|
| 对照 | 不注册 | `WM_SYSKEYDOWN(0x12)`、`WM_SYSKEYDOWN(0x20)`、`WM_SYSCOMMAND(0xF100 = SC_KEYMENU)` | 无 | 弹出 |
| 实测 | `MOD_ALT \| MOD_NOREPEAT + VK_SPACE`（**返回 1，GetLastError = 0**） | **0 条键盘消息** | `WM_HOTKEY(lParam = 0x200001)` | 不弹 |

**三条结论**：

1. `Alt+Space` 在本机**空闲可注册**（未被系统或第三方占用）。
2. 注册成功后系统菜单不再出现：探针只验证了自建的前台窗口（对照轮证明探针有效，非假阴性）；「任何应用均如此」是 RegisterHotKey 系统级截获机制的推论，非本轮逐应用实测。
3. 项目既有记载与此一致——`crates/dd-gui/src/app/keys/hotkey_capture.rs` 约 :12 的 R-21 注释已写明「RegisterHotKey 可成功，劫持一切应用」，但**用户可见文案与实测相反**（见 §3.3），这是本次要一并修正的偏差。

### 2.2 代码事实

| 位置 | 现状 |
|---|---|
| `crates/dd-gui/src/settings.rs` 约 :172 | `HOTKEY_MODS_DEFAULT = 0b1000 \| 0b0001`（Win+Alt）；`HOTKEY_MODS_MASK = 0b1111` |
| `crates/dd-gui/src/settings.rs` 约 :584 | 解析条件 `masked & 0b1011 != 0`——纯 Alt（1）合法，无需改判据 |
| `crates/dd-gui/src/hotkey.rs` 约 :28 / :32 | `HOTKEY_MODIFIERS_DEFAULT` / `HOTKEY_VK_DEFAULT` 两个 `pub const` **全仓零引用**（`crate::hotkey::` 引用面只有 `HotkeyThread` / `HotkeyEvent` / `HOTKEY_SEQ_STARTUP` / `HotkeyCommand` / `pack_command_slots`） |
| `crates/dd-gui/src/tray.rs` 约 :76 | `DEFAULT_HOTKEY_LABEL = "Win+Alt+Space"`（托盘「主线程未写入」兜底） |
| `crates/dd-gui/src/tray.rs` 约 :537 | 单测硬编码断言 `"dd-run — Win+Alt+Space"` |
| `crates/dd-gui/src/app/keys/hotkey_capture.rs` 约 :18 | `is_system_reserved_combo` 命中 `(0x1, 0x20)` = Alt+Space |
| `crates/dd-gui/src/ui/settings_view/hotkey_dialog.rs` 约 :161 / :168 | 该判定唯一 UI 调用点（候选命中 → 警示行）；现状颜色为 `p.accent`（品牌蓝，项目文档惯称「黄线」，与代码不符） |
| `crates/dd-gui/src/text.rs` 约 :596 | `set.hotkey.warn_reserved`：「可能被系统优先响应，建议更换」——与 §2.1 实测相反 |
| `crates/dd-gui/src/app/keys.rs` 约 :32 / :42 | R-21 单测：Alt+Space 正例断言 + 「默认热键不命中黑名单」负例 |
| `crates/dd-gui/src/ui/settings_view/general.rs` 约 :206 | 「恢复默认」按钮——经 `apply_hotkey_default` 直接走 seq 注册，**不经捕获** |
| `crates/dd-gui/src/main.rs` 约 :111 / :127 | 启动写托盘标签 + `HotkeyThread::spawn(settings.hotkey_mods, hotkey_vk)` |

> 注：本节为 **v1.0 取证轮的变更前快照**，供改动清单对照；落地后的代码状态见 §6 批次记录与 `implementation.md` §2「全局热键默认值 Alt+Space」。

---

## 3. 方案

### 3.1 决策 D1：默认值改为纯 Alt

- `HOTKEY_MODS_DEFAULT`：`0b1000 | 0b0001` → `0b0001`；主键保持 `VK_SPACE (0x20)`，默认组合在 UI 的呈现为 `Alt+Space`（`hotkey_mods_label(1)` 输出 `Alt`）。
- 只影响「新装 / 配置字段缺失或非法」的场景；已存在的 `hotkey_mods` / `hotkey_vk` 现值原样生效（本机现值 `2 / 32` 即 Ctrl+Space，切换须用户操作）。

### 3.2 代码改动清单

| # | 文件 | 改动 |
|---|---|---|
| C1 | `crates/dd-gui/src/settings.rs` | 约 :172 常量改 `0b0001`；约 :86、:169–171、:581 注释「默认 Win+Alt」→「默认 Alt」；其中 :169 的「与 hotkey.rs 的 windows-sys 常量一致」须随 C2 删除常量一并删除 |
| C2 | `crates/dd-gui/src/hotkey.rs` | 删除约 :26–32 两个零引用常量；约 :1 模块注释、约 :832 / :892 / :898 用例注释同步默认组合 |
| C3 | `crates/dd-gui/src/tray.rs` | 约 :76 常量改 `"Alt+Space"`；约 :537 断言改 `"dd-run — Alt+Space"`；约 :6 / :10 注释同步 |
| C4 | `crates/dd-gui/src/text.rs` | 新增 `set.hotkey.warn_takeover`（中英双语）；约 :243–245 `set.hotkey.capture_fallback` 的「如 Alt+Space」示例改为其它系统组合键（原稿误记为 :592，:592 实为 `set.hotkey.occupied`）；约 :240 注释、约 :583 `set.hotkey.capturing_fallback` 的「不支持 Win / Alt+Space」一并同步 |
| C5 | `crates/dd-gui/src/app/keys/hotkey_capture.rs` | `is_system_reserved_combo` 移除 `(0x1, 0x20)`；新增 `is_menu_takeover_combo(mods, vk)`（Alt 掩码精确等于 1 且 vk = Space）；约 :9–17 的 R-21 doc 注释同步为三组合，并把 Alt+Space 移入 takeover 说明 |
| C6 | `crates/dd-gui/src/ui/settings_view/hotkey_dialog.rs` | 约 :160–171 拆双分支：takeover → `set.hotkey.warn_takeover` 中性说明（`p.accent`）；reserved → `set.hotkey.warn_reserved`，颜色由现状 `p.accent`（约 :168）改为 `p.danger`，与 takeover 区分（原稿「原黄线」与代码不符） |
| C7 | `crates/dd-gui/src/app/keys.rs` | 约 :32 删除 Alt+Space 正例断言；新增 takeover 正负例；约 :26–28 doc 注释的「四组合含 Alt+Space」改为三组合；约 :27、:41「默认热键 Ctrl+Space」与约 :47 断言消息「Ctrl+Space 是默认热键」一并改为新默认 Alt+Space |
| C8 | `crates/dd-gui/src/settings/tests.rs` | 约 :420 / :1357 注释「默认 Win+Alt」→「默认 Alt」（断言用常量，自动跟随，不改数据） |
| C9 | `crates/dd-gui/src/app/mod.rs`、`crates/dd-gui/src/ui/settings_view/general.rs`、`crates/dd-gui/src/platform/capture_hook.rs`、`crates/dd-gui/src/settings.rs` | 注释中的默认组合字样同步（`capture_hook.rs` 约 :432 的 `0b1001` 断言是**编码锚**，指 Win+Alt 编码而非默认值，**不动**；`app/mod.rs` 约 :917 是历史计时测量说明而非默认值声明，**不动**） |

**实施顺序约束**：C1 / C5 / C7 必须原子同批落地——只做 C1 时默认值变 `Alt+Space` 而黑名单仍含该组合，`keys.rs` 约 :42「默认不命中黑名单」断言必红；只做到 C5 时黑名单已移除而 `keys.rs` 约 :32「Alt+Space 必命中」旧断言未删，同样必红。三处同时落地才绿。因此 §5「三关」门禁须在 C1–C9 **全部完成后**统一执行，不可逐步跑；C2（零引用常量删除）、C4→C6（新增文案键与 UI 消费）、C8/C9（注释）之间无中间态编译或测试风险（i18n 完整性单测 `text.rs` 约 :991–1000 仅查两语非空与 key 唯一，不查总数）。

### 3.3 文案与 R-21 语义拆分

| 组合 | 判定函数 | UI 表现 | 语义 |
|---|---|---|---|
| Alt+Space | `is_menu_takeover_combo` | `p.accent` 中性说明（`set.hotkey.warn_takeover`）：「该组合会被 dd-run 全局接管，其他程序的窗口系统菜单将不再弹出」 | 可控的已知副作用，**不劝阻**（且是默认值） |
| Ctrl+Esc / Ctrl+Shift+Esc / Alt+F4 | `is_system_reserved_combo` | `set.hotkey.warn_reserved` 警示文案（建议更换），颜色由现状 `p.accent` 改 `p.danger` | 与系统键位正面冲突，仍劝阻；与 takeover 中性说明形成视觉区分 |

理由：现有 `warn_reserved` 文案「可能被系统优先响应，建议更换」在 Alt+Space 上已被 §2.1 实测否定；默认值若还带「建议更换」的警示，UI 自相矛盾。另外现状警示行实为 `p.accent`（项目文档惯称「黄线」），若两支同色则语义拆分不可见，故 reserved 改 `p.danger`。

### 3.4 不做项

| 项 | 不做理由 |
|---|---|
| 常驻低级键盘钩子兜底「注册失败也吞 Alt+Space」 | 违反本项目钩子纪律（仅捕获期、30s 超时、双保险卸载）；被第三方先注册属系统级竞争，代价与收益不成比例 |
| 配置迁移（自动把旧默认 Win+Alt+Space 改成 Alt+Space） | 改绑是用户决策；自动迁移会在用户不知情时改变全系统行为 |
| 改写历史文档中的 `Win+Alt+Space` 记载 | 属时点快照（`refactor-structure-2026-10-21.md`、`stability-usability-security-plan.md`、`refactor-layering-plan.md`）；仅 `refactor-structure` 约 :194「冒烟必读」可加一行时效注记 |
| 窗口子类化 / 删除系统菜单项 | §2.1 已证注册成功即无需；面板本身 `with_decorations(false)`，无可弹菜单 |

---

## 4. 冲突与风险矩阵

| 项 | 判定 | 处置 |
|---|---|---|
| `tray.rs` 约 :537 硬编码断言 | ❌ 必改（否则测试红） | C3 |
| `keys.rs` 约 :32 Alt+Space 正例断言 | ❌ 必改（否则测试红） | C7 |
| `keys.rs` 约 :42「默认不命中黑名单」断言 | ✅ 仍成立 | 黑名单移除 Alt+Space 后，默认值自然不命中；仅注释需修 |
| 命令热键槽（N17） | ✅ 已覆盖 | `settings.rs` 约 :1178 已有「与面板唤起热键重复」拒绝 |
| 微软拼音 IME | ✅ 净收益 | Ctrl+Space 被 IME 吃键为历史记档问题；Alt+Space 不在其默认键位 |
| UI 金图 6 张 | ✅ 预计零漂移 | 基准仅 `settings_appearance`（分类 = Appearance），热键卡在常规页；仍须实跑 snapshot 门禁确认 |
| 本机改绑路径 | ✅ 已解除前提（2026-11-23 v1.4） | 原限制：本机 LL 钩子被安全软件拦截 → 捕获恒回落，回落态按 Alt+Space 会弹系统菜单、须用「恢复默认」绕过捕获。现捕获期临时注册独立 `RegisterHotKey` 接管键（见 §8），Alt+Space 在回落态亦可录入；「恢复默认」仍保留 |
| 新增 takeover 提示腿 | ✅ 本机可达（v1.4） | 同上接管键落地后，回落态按 Alt+Space 落候选 → 对话框显示 `set.hotkey.warn_takeover` 中性说明，「保存」走既有 seq 确认流；真机腿待用户会话 |
| 注册失败（第三方占用，如 PowerToys Run / uTools / Listary） | ⚠️ 走既有降级 | 无热键 + `toast.hotkey_unregistered` + 红色「未注册」徽标（R-15 路径）；本文不改该路径 |
| 全系统副作用 | ⚠️ 产品取舍 | 所有应用失去 Alt+Space 窗口菜单；由 C4 文案明示 |
| 双实例 / 启动竞态 | ✅ 无新增行为 | 第二实例注册失败走同一降级路径 |

---

## 5. 验收标准

**单测**

- `keys.rs`：`PaletteApp::is_menu_takeover_combo(1, 0x20)` 为真；`(2, 0x20)`、`(5, 0x20)`（Alt+Shift）、`(1, 0x73)` 为假；`PaletteApp::is_system_reserved_combo` 仍命中 Ctrl+Esc / Ctrl+Shift+Esc / Alt+F4，且对 Alt+Space 为假。（两者均为 `impl PaletteApp` 关联函数，调用须带前缀。）
- `settings/tests.rs`：默认解析结果等于新常量；纯 Shift 仍回落默认。
- `tray.rs`：`r19_tray_hotkey_label_dynamic` 的默认 tooltip 断言改后通过。
- 命令槽冲突用例沿用常量，自动通过。

**真机（本机可执行）**

**前置**：本机 `config.json` 现值为 `hotkey_mods=2 / hotkey_vk=32`（Ctrl+Space），而 C1 只改默认值常量、§3.1 明确不迁移已有配置。切换路径二选一（两条本机均可执行）：①设置页「更改」直接捕获 Alt+Space——§8 v1.4 捕获期接管键已解除「回落态录不进 Alt+Space」的前提；②「恢复默认」（或清空 config 的 hotkey 字段）经 `apply_hotkey_default → apply_captured_hotkey → re_register` 走 seq 重注册、不经捕获（同 §4「本机改绑路径」）。

1. 干净启动：Windows 启动注册成功路径**无成功日志**（`hotkey.rs` 约 :350–368，仅失败时 `log::debug!`），故判据改为「无注册失败日志 + 热键卡无『未注册』徽标 + 实按生效」；若点「恢复默认」触发重注册，日志应为 `[dd-gui] 全局热键注册成功（seq=N）：Alt+Space`（`hotkey.rs` 约 :413–418；原稿所写「全局热键注册成功：Alt+Space」只是非 Windows 路径的约 :651 格式，本机不适用）。
2. 记事本前台按 Alt+Space → 面板出现，**记事本无系统菜单**。
3. 面板可见时再按 → 隐藏（Toggle 语义不变）。
4. 托盘 tooltip = `dd-run — Alt+Space`。
5. 用助手进程先占 Alt+Space → 启动 → 错误 toast + 热键卡红色「未注册」徽标。
6. 改绑回 Ctrl+Space → Alt+Space 的系统菜单行为恢复。

**门禁**

`cargo test --workspace`（基线数字以实跑为准）、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`、`node tools/docscan.mjs`、`cargo build --release`（记录体积）。

---

## 6. 实施批次与回滚

| 批次 | 内容 | 停等 |
|---|---|---|
| 一 | C1–C9 代码 + 单测；三关 | ✅ 已完成（2026-11-23） |
| 二 | `implementation.md`（总表 / §2 / §3.1 测试基线）、`CHANGELOG.md` 新条目、`stability-usability-security-plan.md` 的 R-21 条目（约 :36 / :385）**及单测描述行（约 :279）**加指向本文的语义注记（黑名单由四组合收为三组合）；同步三份 active 清单中的「四组合含 Alt+Space」表述：`user-session-walkthrough-checklist.md` 约 :47、`未完善事项清单-2026-10-25.md` 约 :125、`实施计划-2026-10-28.md` 约 :221（均非 §3.4 的时点快照，不改会留「三/四组合」并存矛盾）；INDEX 行数回填（`node tools/docscan.mjs --write-index`）、本文状态改「已落地」 | ✅ 已完成（2026-11-23） |

**批次状态**：批次一 ✅ 已落地（C1–C9 + 单测；746/0/7、clippy/fmt/docscan 全绿、release 10,527,744 B）；批次二 ✅ 已落地（上列文档全部回写、INDEX 回填、本文状态更新）。真机 6 条待用户会话。

**回滚**：还原 2 个常量（`settings.rs` 约 :172 / `tray.rs` 约 :76）+ 恢复 `hotkey.rs` 约 :26–32 被删的 2 个常量（共 4 个、跨 3 文件）；撤销 `warn_takeover` 新增与 `capture_fallback`/`capturing_fallback` 两处文案改动（2 条文案键）；恢复 `is_system_reserved_combo` 的黑名单一条、移除 `is_menu_takeover_combo` 及其单测；回退 C6 的 UI 双分支与颜色改动；注释类改动（C1/C2/C3/C5/C7/C8/C9）按需回退。用户配置不动，无数据迁移，无需重启以外的收尾。

---

## 7. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-11-23 | 初稿。含真机取证实测两轮对照（未注册 / 已注册）与 9 项代码改动清单；核对轮修正 6 处（`tray.rs` 断言漏项、`keys.rs` 正例必红、两个零引用常量、本机钩子被拦导致的改绑路径前提、`capture_fallback` 文案矛盾、docscan 行数回填） |
| v1.1 | 2026-11-23 | 二次核对修正：Windows 启动成功无日志导致真机验收第 1 条不成立；C4 行号误记（:243 非 :592）且漏 `capturing_fallback`；「黄线」实为 `p.accent` 致两支同色；C1/C5/C7 注释漏项、C6 未指明文案键；回滚常量计数与范围修正；§2.1 实测/推论口径收窄、探针未留存注记、ALTRun 来源缺口、门禁命令口径与单测调用前缀 |
| v1.2 | 2026-11-23 | 实施顺序约束回写：§3.2 明确 C1/C5/C7 原子同批与「三关」门禁统一执行时机；§5 真机段补前置（本机现值 Ctrl+Space，须先「恢复默认」）；§6 批次二补 `stability` 约 :279 与三份 active 清单的 R-21 表述同步 |
| v1.3 | 2026-11-23 | 批次一 / 批次二落地回写：状态改「已落地」、未落地段改写、§2.2 标为变更前快照、§6 两批次状态与验收数字、版本演进补记；代码侧 C1–C9 全量落地（746/0/7、clippy/fmt/docscan 全绿、release 10,527,744 B） |
| v1.4 | 2026-11-23 | **回落捕获下 Alt+Space 可录**（§8）：新增捕获期临时接管热键（独立 id `0xDD05` + `HotkeyEvent::CaptureCombo`），解除 §4 两条「本机不可达」前提；回归单测 +5，fmt / clippy / 测试三关全绿。真机腿待用户会话 |
| v1.5 | 2026-11-23 | 进度口径整理：§5 前置改为「捕获 Alt+Space」与「恢复默认」二选一（v1.4 已解除前者前提）；同步 `user-session-walkthrough-checklist.md` 约 :47 的「接管提示腿本机不可达」表述与 `stability-usability-security-plan.md` V-8 场景（拆分 Ctrl+Esc 黄线 / Alt+Space 中性接管） |

---

## 8. 回落捕获下 Alt+Space 录入（v1.4，2026-11-23）

### 8.1 问题

§4 两条风险记档均指向同一前提：本机 `WH_KEYBOARD_LL` 被安全软件拦截 → 捕获恒回落 egui 应用层路径；此时按 Alt+Space 被 Windows 解释为「弹出窗口系统菜单」（winit 对 `WM_SYSKEYDOWN` 调 `DefWindowProc`），Space 不送达 egui → 候选永不成立。旧口径要求用「恢复默认」绕过捕获完成切换，且 C6 的 takeover 说明本机无法真机触发。

### 8.2 机制

复用 §2.1 已实测的机制：`RegisterHotKey(MOD_ALT | MOD_NOREPEAT, VK_SPACE)` 成功后 Windows 把整个组合从输入投递链摘除——系统菜单不弹，`WM_HOTKEY` 改投注册线程。与 §3.4「常驻钩子兜底」不做项不冲突：本方案**仅在捕获期（≤30s）**注册、结束即注销，不常驻。

### 8.3 落地

| 点 | 实现 |
|---|---|
| 独立热键 | `hotkey.rs`：`HOTKEY_CAPTURE_ID = 0xDD05`（沿项目既有 `0xDDxx` 号段取空位，不与 slot 0..3 = `0xDD02..0xDD04` 撞号）；`HOTKEY_CAPTURE_MSG`（WM_APP+4）携带 mods/vk，vk 0 = 注销 |
| 事件 | `HotkeyEvent::CaptureCombo{mods,vk}`；`poll_hotkey` 捕获期落候选（与钩子 Combo / egui Key 同语义：只记候选，等「保存」走 seq 确认流），非捕获期忽略陈旧事件 |
| 生命周期 | `begin_capture` → `refresh_capture_sink` 按需注册（当前 slot 0 已是 Alt+Space **且注册成功**时跳过，避免同组合重复注册；启动注册失败时仍注册接管键——此时没有 Toggle 可翻译）；`end_hotkey_capture` / 保存 / 捕获超时 / hide → 注销 |
| 自锁防护 | 真实注册（slot 0 改绑 / 命令槽同步）**之前**先发注销命令——`PostThreadMessageW` 同线程 FIFO 保证顺序；否则 `RegisterHotKey` 对已注册组合返回 `ERROR_HOTKEY_ALREADY_REGISTERED`，保存 Alt+Space 会被自己的接管键挡住。失败回滚后重新注册接管键供重试 |
| Toggle 兼容 | slot 0 本就是 Alt+Space 时其 `WM_HOTKEY` 仍是 `Toggle`；捕获期 `poll_hotkey` 把它翻译成候选（其余组合的 Toggle 继续忽略，防面板被隐藏） |
| 文案 | `capture_fallback` / `capturing_fallback` 改为明示「Alt+Space 可录」，不再声称系统组合键一概不可录 |

### 8.4 验收

- 单测：`hotkey.rs` id 不撞号 + 桩线程 no-op；`lifecycle.rs` CaptureCombo 仅捕获期落候选、Toggle 翻译仅对当前 Alt+Space 生效；`app/keys.rs` 注册决策纯函数（已注册 Alt+Space 不重复注册 / 启动注册失败时仍注册 / 非捕获期不注册）。
- 真机（待用户会话）：本机现值 Ctrl+Space → 设置页「更改」→ 按 Alt+Space（系统菜单**不弹**、对话框显示 Alt+Space keycap + takeover 中性说明）→「保存」→ 托盘 tooltip 与热键卡显示 Alt+Space；取消 / 超时后系统菜单行为恢复。
