# 用户会话真机走查清单（真实键鼠腿）

> **状态**：已落地（本机腿 2026-10-07 用户批量确认收口；R-21 需无拦截他机、V-14 需多屏；**2026-11-24 场次 A：A1–A9/A15/A17/A18/A21 过，A10/A12/A14/A16/A20 部分，A13/A19 未执行；2026-11-25 A11 闭环、A12 Esc 通过、A10/A20 导出通过**）｜ **版本**：v1.19 ｜ **最后更新**：2026-11-25
> **关联**：[implementation.md](./implementation.md) §7 · [future-features-plan.md](./future-features-plan.md) §4 · [security-audit-2026-09-23.md](./security-audit-2026-09-23.md) · [stability-usability-security-plan.md](./stability-usability-security-plan.md) · [INDEX.md](./INDEX.md)
>
> **为什么需要你**：本机安全软件在系统级过滤注入输入（V-7 通道清单穷尽：posted 消息不经 IME / SendInput 受理不达 / osk UIA 合成键同样被拦），物理键盘为唯一通路——以下腿无法自动化。
> **通用纪律**：开放腿交付件 = **清单 P1-2 确认的同代候选包**（v0.3.0 与旧 `dist/` 仅作历史证据）；走查只使用隔离 APPDATA，测后清理隔离目录，不动真实用户配置与常驻实例；结果按「记录位置」回写对应文档并销勾。

---

## 1. N 系列（future-features-plan §4 落地注记的验收草案）

| # | 腿 | 操作 | 预期 | 记录位置 |
|---|---|---|---|---|
| ✅ 1.1 | N1 执行腿（url） | 首屏点「直达」组 GitHub 行 | 默认浏览器开 `https://github.com` | future-features-plan §4.1 验收清单 |
| ✅ 1.2 | N1 执行腿（path） | 点「C:\Windows」类 path 命令 | 资源管理器打开该目录（ShellExecute 双击等价） | 同上 |
| ✅ 1.3 | N1 拼音/别名 | 键入 `gh` / 拼音首字母 | 直达条目命中 | 同上 |
| ✅ 1.4 | N1 S-03 白名单 | config 预置 `javascript:` target 命令并执行 | 拦截 + toast，不执行 | 同上 |
| ✅ 1.5 | N2 设置页腿 | 设置页扩展卡 apps「设置」→ 添加/删除屏蔽片段 → 返回首屏 | 片段命中行消失；清空后恢复 | §4.2 |
| ✅ 1.6 | N3 Chrome 腿 | 装 Chrome 并登录书签后重启 | Chrome 书签入「书签」组（本机当前无 Chrome 书签文件，10-04 记档） | §4.3 |
| ✅ 1.7 | N3 拼音/打开 | 键入中文书名拼音 → Enter | 命中 + 默认浏览器打开 | §4.3 |
| ✅ 1.8 | N4 调高腿 | warm_capacity 调到 16 → 多次唤起各扩展 | 第三方/sidecar 扩展保活数增加、stub 减少 | §4.4 |
| ✅ 1.9 | N5 导出→清→导入 | 设置页「导出」→ 改乱配置 → 「导入并覆盖」（两步确认） | 逐项还原；机器态（热键/自启/尺寸）保留本机值 | §4.5 |

## 2. T9 背景图（settings-personalization-plan §5）

| # | 腿 | 操作 | 预期 |
|---|---|---|---|
| ✅ 2.1 | 设置页按钮腿 | 「外观」→ 背景图卡：输入合法图片路径 → 「设为背景」 | 面板立即显示图；材质卡置灰 + 提示行 |
| ✅ 2.2 | 错误行 | 输入不存在路径 → 设为背景 | danger 行「文件不存在」；运行期解码失败显示解码错误行 |
| ✅ 2.3 | fit / 滑杆 | 拖动不透明度（低档观察是否柔和面板色）、着色强度；切换 Fill/Stretch | 即时生效；松手落盘；重启后保持 |
| ✅ 2.4 | 清除恢复 | 「清除」 | 材质链路恢复、置灰解除 |
| ✅ 2.5 | resize 重裁剪 | 拖动面板边缘改变宽高 | Fill 档裁剪窗口随帧重算、无重解码卡顿 |

## 3. 安全回归（security-audit-2026-09-23.md §真机回归）

- [x] Apps 启动；WebSearch（2026-10-07 批量确认）—— 开网页；**文件搜索「打开」**（S-03 关键回归面）
- [x] Shell 危险命令确认弹窗（2026-10-07 批量确认）——（S-06）
- [x] Calc 复制后（2026-10-07 批量确认）——「扩展已写入剪贴板」提示（S-07）
- [x] 文件搜索「显示所在目录」（2026-10-07 批量确认）——定位正确（S-08）
- [x] 第三方扩展信任审批流（2026-10-07 批量确认）——（S-05：待批准 → 允许/阻止 → 首屏即时生效）

## 4. 稳定性方案剩余腿（stability-usability-security-plan.md）

- [x] **R-22 / V-7**（2026-10-07 批量确认）——：真实 IME 组词中回车——仅上屏不触发选中；非组词回车行为不变（物理键盘 + 真实 IME，自动化不可能性已源码级论证）
- [x] **R-23**（2026-10-07 批量确认）——：文件搜索页粘贴一次超长文本（>256 字符）——入框截为 256、不向 es.exe 传离谱参数
- [ ] **R-21 黄线腿**（需**无拦截机器**）：捕获对话框录 Ctrl+Esc / Ctrl+Shift+Esc / Alt+F4 三系统组合 → 黄线警示可录（2026-11-23：Alt+Space 已移出黑名单、改为默认组合的中性接管说明，见 [方案](./hotkey-alt-space-default-2026-11-23.md) §3.3；接管提示腿的「本机不可达」前提已由捕获期接管键解除，见方案 §8 v1.4，真机腿待会话）
- [x] **R-08**（2026-10-07 自动化会话收口：桩 es.exe 直驱真实 sidecar，两形态验证——stability-plan v1.23）——：es.exe 错误路径冒烟（本机 Everything 运行中不可达——临时停 Everything 后触发 sidecar 错误分支，错误文案不截断多字节）
- [x] **E2E 采样**（2026-10-07 收口：带词进页子集 7 条 p95 102 ms ≤ 200 PASS；随轮验证 Ctrl+F 光标置尾腿）——根页输入关键词 → Ctrl+F 带词进页（埋点 = 带词进页），数轮后 `node tools/gui_e2e_parse.mjs <日志>` 判读（2026-10-07 移植 Node 版，本机无 Python；带日志实例 stderr 已重定向即可采）
- [ ] **V-14 余腿**：显示器热拔插 / DPI 变更 / RDP 拓扑下面板自适应居中

## 5. O5-a default_fonts 移除回归（optimization-plan §2.1，2026-10-07 落地）

> 程序化穷举（`tools/glyph_coverage_check.mjs`，942 码位 × 运行时字体栈）已 PASS、首屏真机抽查零 tofu；以下为穷举不可覆盖的真机确认腿。
>
> **状态更新（2026-11-24）**：11-02/03 的白帧复发阻塞已由后续场次 A 的真机走查解除；A8 双语 tofu 扫查与 A9 emoji 用户数据均已通过。历史阻塞与机制未知记录保留在 implementation.md §7，不作为当前未执行项。

- [x] **全页面 tofu 走查**：zh / en 两语言 × 全部页面（首屏 / 设置页各卡 / 危险命令确认对话框 / Toast / 右键菜单 / 嵌套页）扫一遍——重点 `→` / `↵` 键帽与符号字形
- [x] **emoji 用户数据确认**：设一条含 emoji 的自定义命令名或书签标题——确认呈 tofu（缺 NotoEmoji 兜底，**接受的取舍**）；确认 UI 文案本身零影响

---

## 6. T11 Steam 游戏搜索开关（settings-personalization-plan §5 T11，2026-10-07 落地）

> 自动化腿已过（打标 / roundtrip / 过滤 3 单测 + 595 全绿）；以下为真机确认腿。

- [x] **默认隐藏腿**（2026-10-07 批量确认）——：干净 config 启动（`search_steam_games` 缺省）→ 空查询首屏与输入游戏名关键词均**无** Steam 游戏行（对照：Steam 库任一已装游戏，如输入其名称首字母）
- [x] **开关开启腿**（2026-10-07 批量确认）——：设置 → 搜索 → 「搜索行为」卡第二行「Steam 游戏」开 → 即时回到面板输入同一关键词 → 游戏行出现且可启动
- [x] **开关关闭腿**（2026-10-07 批量确认）——：关回开关 → 游戏行即时消失（空查询与关键词两分支）
- [x] **组合腿**（2026-10-07 批量确认）——：「搜索应用」关 + 「Steam 游戏」开 → 游戏仍不可见（AND 叠加）；再开「搜索应用」→ 普通应用出现、游戏按开关档位出现
- [x] **持久化腿**（2026-10-07 批量确认）——：重启后开关档位与 config.json `search_steam_games` 一致；旧 config（无该字段）启动回落「关」

---

## 7. v0.3.0 真机走查筹备（2026-11-23）

> **当前执行说明**：腿号与场次结构保留历史规划；§7.1 命令已改为隔离 APPDATA、指定测试 PID 的同代候选包流程。当前开放腿必须传清单 P1-2 候选包，不得终止通配进程或触碰真实用户目录。

> 历史目的：把 [清单 v1.16](./未完善事项清单-2026-10-25.md) §5 的 **25 条腿**（平台 6 + ALTRun 12 + 热键 7）按「本机 → 他机 → 多屏 → 跨平台」排成四场；当前未闭环项以清单 v1.39 §1.1 为准；P1-14～P1-17 代码与 CI 回归已落地，真实腿仍待对应环境。
> 腿号索引：平台 = 清单 §5.1–5.6 / §5.12；ALTRun = §5.13–5.24；热键 = §5.25。

### 7.1 通用前置（每场开始前，约 3 分钟）

```powershell
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$CandidateZip)

# 1) 全部路径与用户数据隔离；CandidateZip 必须是清单 P1-2 的同代候选包
$root = Join-Path $env:TEMP ("ddrun-walk-" + Get-Date -Format "yyyyMMdd-HHmmss")
New-Item -ItemType Directory -Path $root -Force | Out-Null
$env:APPDATA = $root
Get-FileHash $CandidateZip -Algorithm SHA256
Expand-Archive $CandidateZip (Join-Path $root "app") -Force

# 2) 只记录本次测试进程 PID；禁止 Get-Process dd-run* | Stop-Process
$exe = Get-ChildItem (Join-Path $root "app") -Filter "dd-run-*.exe" | Select-Object -First 1
if (-not $exe) { throw "候选包中未找到 dd-run-*.exe" }
$proc = Start-Process $exe.FullName -PassThru
"TEST_PID=$($proc.Id)" | Set-Content (Join-Path $root "test-pid.txt")

# 3) 如确需保存隔离配置快照，只复制 $env:APPDATA\dd-run，不触碰真实用户目录
$d = Join-Path $env:APPDATA "dd-run"
Get-ChildItem $d -File -ErrorAction SilentlyContinue |
  Get-FileHash -Algorithm SHA256 |
  Export-Csv (Join-Path $root "before-hashes.csv") -NoTypeInformation

# 收尾：只结束本次 PID，复核哈希后删除整个 $root 临时目录
Stop-Process -Id $proc.Id -ErrorAction SilentlyContinue
Get-ChildItem $d -File -ErrorAction SilentlyContinue |
  Get-FileHash -Algorithm SHA256 |
  Export-Csv (Join-Path $root "after-hashes.csv") -NoTypeInformation
```

- **隔离跑（强制）：`$CandidateZip` 传清单 P1-2 的同代候选包；APPDATA、日志、配置快照均在 `$root`。禁止终止通配进程或备份真实 `%APPDATA%`。历史 v0.3.0 下载命令仅用于复现旧证据，不作为当前开放腿交付口径。
- **采集**：截图 + `$root\dd-run\logs\dd-run.log` 关键字（`热键` / `命令热键槽` / `N12 进入目录` / `保存`）+ 剪贴板断言（`Get-Clipboard`）；证据存 `$root`，不入库。
- **收尾**：只结束本次测试 PID，复核隔离目录前后哈希，删除 `$root`；不得终止常驻实例或恢复真实用户目录。

### 7.2 场次 A：本机物理键鼠（约 20–30 分钟，25 条中最先做）

| 序 | 腿 | 前置 / 操作 | 判据 |
|---|---|---|---|
| ✅ A1 | §5.25 热键 1/7 | 干净启动（隔离目录） | 无注册失败日志、热键卡无「未注册」徽标、实按生效 |
| ✅ A2 | §5.25 2/7 | 记事本前台按 `Alt+Space` | 面板出现，**记事本无系统菜单** |
| ✅ A3 | §5.25 3/7 | 面板可见时再按 | 隐藏（Toggle） |
| ✅ A4 | §5.25 4/7 | 悬停托盘图标 | tooltip = `dd-run — Alt+Space` |
| ✅ A5 | §5.25 7/7 | 设置 → 更改 → 捕获 `Alt+Space`（系统菜单不弹）→ 保存 | 对话框显示 keycap + 中性接管说明；保存后托盘/热键卡为 Alt+Space；取消/超时后菜单行为恢复 |
| ✅ A6 | §5.25 5/7 | 先起一个占位进程注册 Alt+Space，再启动 dd-run | 错误 toast + 红色「未注册」徽标（R-15 路径） |
| ✅ A7 | §5.25 6/7 | 改绑回 `Ctrl+Space` | 记事本 `Alt+Space` 系统菜单恢复 |
| ✅ A8 | §5.6 tofu | 设置页切 zh / en，逐页扫（首屏 / 各卡 / 确认框 / Toast / 右键菜单）；Windows 快速命令/系统组按用户要求排除 | 无 tofu；`→` / `↵` 键帽正常 |
| ✅ A9 | §5.6 emoji | 自定义命令名或书签标题含 emoji | 实测为单色字形（非 tofu）、UI 文案零影响 |
| 🟨 A10 | §5.14 余腿 | 置顶组 Enter 打开 / 取消置顶 / 导入导出往返 | 组序与打开行为正确；2026-11-25 导出及备份内容通过，导入确认待人工 |
| ✅ A11 | §5.15 余腿 | 关「搜索历史」→ 查询；再开 → 清空按钮 | 关闭即停记并清空；开启保留既有历史；清空后 `Ctrl+↑` 无回填（2026-11-25 修复条件写反后通过） |
| 🟨 A12 | §5.16 余腿 | 速查页 `Esc` 返回；IME 组词期按 `?` | Esc 返回根页已通过（2026-11-25）；代码级非空根页/嵌套页/IME 反例已由 4 条回归锁定；IME 组词态不误触仍待真实键盘 |
| 🟨 A13 | §5.17 余腿 | 连续改命令 4 次 | `bak.3` 挤出最旧档、三档内容正确 |
| 🟨 A14 | §5.18 余腿 | 触发骤降 toast → 点「从上次备份恢复」 | 条目还原、日志时间线可 grep |
| ✅ A15 | §5.19 余腿 | 容量调到 12 | 红字警告消失 |
| 🟨 A16 | §5.20 余腿 | 失效直达行按 Enter；恢复文件后重聚合 | 拒绝 toast 已过；恢复文件后重聚合/红字消失未取证 |
| ✅ A17 | §5.21 余腿 | 中文路径目录浏览 / 右键 reveal / Everything 常规查询 | 列表与打开正确、无回归 |
| ✅ A18 | §5.22 余腿 | 绑定已被占用组合 / 改绑已有槽 / slot 0 回归 | 冲突提示；改绑走 ReRegistered；唤起键不受影响 |
| 🟨 A19 | §5.23 余腿 | 多窗口切换（含最小化恢复）/ 提权窗口 / 拼音标题 / 空查询 / 停用开关 / 面板自身 | 各态符合预期，提权被拒有 toast |
| 🟨 A20 | §5.24 余腿 | 设置页表单建片段 / 右键删除 / 导入导出往返 | 校验与即时消失正确；2026-11-25 导出及备份内容通过，导入确认待人工 |
| ✅ A21 | §5.13 更新下载校验 | **构造低版本验收副本**（v0.2.1 不含 O17，不能直接跑全链）→「检查更新」Available → 下载 → 校验 → Explorer 定位；再截短 zip 重试 | 正常：Success toast + zip/SUMS 留存 + Explorer 定位；篡改：Error toast + 目录清空 |

> **场次 A 执行记录（2026-11-24，本机 v0.3.0 隔离实例，自动化注入键鼠）**：**A1–A7 全部通过**——A1 干净启动无失败日志、热键卡无「未注册」徽标；A2 记事本前台 `Alt+Space` → 面板出现且系统菜单窗口类 `#32768` 不出现（自动化取证）；A3 Toggle 出现→隐藏→出现；A4 托盘 UIA 可访问名 = `dd-run — Alt+Space`；A5 回落态捕获 `Alt+Space` 显示 keycap + 蓝色中性接管说明 → 保存成功（日志 `全局热键注册成功（seq=2）：Alt+Space`、`config.json hotkey_mods=1`）；A6 占位进程先注册 → 启动日志 `全局热键注册失败（1+32），降级为无热键运行` + 红「未注册」徽标 + 热键实按无效；A7 设置改绑 `Ctrl+Space`（日志 seq=1）后 Toggle 生效、记事本 `Alt+Space` 系统菜单恢复且面板不弹，再改回 `Alt+Space`（seq=2）正常。证据：面板/设置/捕获对话框截图 + 隔离目录日志（`%TEMP%\ddrun-walk-a1\dd-run\logs\dd-run.log`）+ `config.json` 两值双向切换。
> **续跑记录（2026-11-24，自动化）**：**A9 ✅**（关键词 `zzqqnote` 命中自定义命令「📝 zzqqnote」（「直达」组）；emoji 经系统字体回退渲染为**单色字形**而非 tofu——优于清单预期取舍，UI 文案零影响）；**A15 ✅**（`warm_capacity=1` → 「已装扩展 8 个 / 保活容量 1」+ 红字「已装扩展数已达保活容量…建议调大容量」；调大至 12/13 后警告消失）；**A16 ◐**（失效 path 行右列红字「路径不存在」；Enter → toast「路径不存在：C:\NoSuchDir\zzqq.exe」；恢复文件后重聚合/标记消失未取证）；**A14 ◐**（启动日志 `启动骤降检测：custom_commands 1 条，备份环最新档 10 条` 命中；面板 toast 未在截图窗口捕获，恢复按钮腿未点）；**A12 ◐**（自动化注入 `?` 直接入框、未打开速查页——真机 2026-11-22 已验通过，判为注入路径差异非缺陷）；**A17 ◐**（截图未取到文件页，待重跑）。
> **当前未闭环**：A10/A20 导入确认（导出已于 2026-11-25 通过）、A12 IME 组词防误触（Esc 已过）、A13 `bak.3` 挤出、A14 备份恢复按钮、A16 恢复文件后重聚合/标记消失、A19 窗口切换完整行为；其余 A1–A9/A11/A15/A17/A18/A21 已通过。
>
> **续跑记录 2（2026-11-24，自动化）**：**A10 ✅（导入导出往返除外）**——预置 `pinned_keys`（`com.ddrun.host\u001fcustom:zzqqnote`）→ 空查询「置顶」组居首、Enter 打开 notepad；右键「取消置顶」→ 置顶组消失、行回落「直达」组、`config.pinned_keys` 清空；**A11 ◐**——`Ctrl+↑` 回填历史生效（回填 `?`）、`record_search_history=false` 后查询隐藏不写 `history.json`（前后逐字节相同）；清空按钮与开关联动（关闭即清空）未跑；**A17 ✅**——`Ctrl+F` → 粘贴 `C:\Windows` → 文件页列出子项（addins / appcompat…）、右列父路径、页脚 `com.ddrun.filesearch`；目录 Enter / 中文路径 / 右键 reveal / Everything 常规回归沿用 2026-11-22 已取证部分。
>
> **续跑记录 3（2026-11-24，自动化）**：**A20 ✅（导入导出往返除外）**——设置页「文本片段」表单录入（名称「邮件签名」/ 关键词 `sig` / 双行正文）→ 面板「片段」组命中 → Enter 后剪贴板与 `config.json` 文本**逐码位一致**（含 LF）；右键「删除此片段」→ 行即时消失、`snippets` 清空；**A18 ✅**——命令槽 1 预置 `Alt+Space`（与 slot 0 冲突）→ 启动日志 `命令热键槽 1 注册失败（Alt+Space）`，slot 0 Toggle 正常、命令不误触发（冲突处置 = 保 slot 0）；改绑 `Ctrl+Alt+F9` 后面板隐藏态触发：日志 `命令热键槽 1 触发：zzqqnote` + notepad 启动、面板不弹。
>
> **续跑记录 4（2026-11-24，自动化 + 临时验收钩子）**：**A21 ✅**——先下载官方 v0.2.1（portable zip SHA256 `9864c8bf44d4bfc0b238a1cf3922b71f56846a615e104fc11ad943a06e45bbe3`）并确认该 tag 无 `update_check.rs`，不能执行 O17；按实施计划 §4.1「构造低版本号」口径，先制作 v0.3.0 Release exe 的 2 字节版本串对照副本并留存改动范围证据；因桌面鼠标注入被拦截，最终全链改用同源码临时验收构建（版本常量改为 `0.2.1` + 自动触发钩子），走真实 `start_update_check` / `start_update_download`。正常链：`latest=v0.3.0` → Available → 下载 zip **4,647,368 B** + SUMS **291 B** → SHA256 `eb4500f58df341b4c66c7cc4c2d28b265014a2d1488d2be5cbfc98b9a034ee7d` → Success toast「已下载并校验通过：解压覆盖安装目录即完成更新」+ Explorer `updates` 定位；篡改链：应用目录 curl 适配器真实下载后将 zip 截短为 **1,024 B** → Error toast「校验失败：哈希不匹配…」+ `cache/updates` **0 文件**。证据：`%TEMP%\walk-2026-11-24-a21\`（`normal-success-toast-screen.png` / `normal-zip-hash.txt` / `normal-explorer-windows.txt` / `tamper-error-toast-screen.png` / `tamper-proof.txt` / `tamper-cleanup.txt` / `binary-patch-proof.txt` / `auto-binary-proof.txt`）。说明：桌面顶层拦截层使物理鼠标点击不可达，故用临时自动触发钩子；钩子与版本改动已全部还原，工作副本无代码差异。直接 GitHub 资产直链首轮曾 TLS/连接失败，curl 适配器仅加 6 次重试，仍访问真实 URL。回归：`update_check` **11 passed / 0 failed**，`cargo fmt --all -- --check` 与 `node tools/docscan.mjs` 全绿。
>
> **续跑记录 5（2026-11-24，Computer Use）**：**A8 ✅（Windows 快速命令/系统组不适用）**——v0.3.0 隔离实例 zh/en 双语扫查首屏、设置页外观/常规/搜索/扩展全部卡片、危险操作确认框、Toast、右键菜单，未发现 tofu，`→` / `↵` 键帽正常；English 扩展页仍显示内置扩展中文显示名，属既有翻译覆盖问题、非 tofu。系统嵌套页/Windows 快速命令按用户要求未测。证据：`%TEMP%\walk-2026-11-24-a8\evidence\`（`a8-zh-contact-sheet.png` / `a8-en-contact-sheet.png` / `A8-summary.txt`）。CI run 45370055751（`9bc2bf1`）最终 success。
>
> **续跑记录 6（2026-11-25，Computer Use + 隔离 APPDATA）**：**A10/A20 导出通过、导入确认未触发**——新打 portable 包在 `target/acceptance/a10-a20-20261125-01/appdata` 启动；预置 `custom_commands=[往返置顶]`、`pinned_keys=[com.ddrun.host\u001fcustom:roundtrip]`、多行片段「往返片段 / rt / 第一行\n第二行」，设置页「导出」成功，备份含三项业务数据与 `exported_from=0.3.0`，且不含 `autostart` / `panel_size` / `panel_last_pos`。随后结束隔离实例、清空三项业务字段并写入机器态哨兵（`panel_size=[701,601]`、`panel_last_pos=[222,111]`、`hotkey_mods=3`、`hotkey_vk=122`）；重启后「导入并覆盖」可武装为「确认导入」，但 Computer Use 多次单击/双击均未进入应用逻辑，日志无“设置备份已导入”，配置仍为空。证据：该目录下 `exported-backup.json`、`pre-import-config.json`、`backup-buttons.png`、`import-buttons.png` 与 `dd-run/logs/dd-run.log`。常驻实例已按原路径恢复，配置 SHA256 仍为 `69780066389D2585189635F53394B5E4D354D5CAE8B5A259ACB6B5A1861F80F8`；两条腿保持部分完成，待人工确认导入。
>
> **续跑记录 7（2026-11-25，Computer Use + TDD 修复）**：**A11 ✅**——预置 `history.json={version:1,queries:["alpha","beta"]}`；清空按钮点击后 toast「搜索历史已清空」、文件删除、根页 `Ctrl+↑` 不再回填。首次关闭开关真机抓出条件写反：配置已关但历史仍在；新增 `history_toggle_off_clears_and_on_preserves` 回归测试（Red：缺实现；Green：通过），将清空条件从开启时改为关闭时。三关 **759 passed / 0 failed / 7 ignored**，重打包 SHA256 `ecef3da9…9837` 后复验：开启保留 `alpha/beta`，关闭删除 `history.json`。**A12 ◐**——根页搜索框输入文本 `?` 打开「键位速查」页，`Esc` 返回根页通过；IME 组词期防误触仍待真实键盘。证据：`target/acceptance/a11-a12-20261125-01/`。

### 7.3 场次 B：他机（无拦截 + Chrome / Everything，约 20 分钟）

| 序 | 腿 | 前置 | 判据 |
|---|---|---|---|
| B1 | §5.3 R-21 黄线 | 该机 LL 钩子未被安全软件拦截 | 捕获 `Ctrl+Esc` / `Ctrl+Shift+Esc` / `Alt+F4` → danger 黄线警示且可录；`Alt+Space` → 中性接管说明（不劝阻） |
| B2 | §5.1 A-33-08 | 另一套机器组合 + Everything | 跨环境矩阵复跑通过（**A-33 唯一红线**；P2-6 另计） |
| B3 | §5.9 N3 Chrome（条件保留） | 该机有 Chrome 书签 | Chrome 书签入「书签」组且可打开 |
| B4 | §5.13 备选 | 若 A21 不便，可在该机做 | 同 A21 |

### 7.4 场次 C：多屏 / RDP（约 10 分钟）

- **§5.4 V-14**：显示器热拔插 / DPI 变更 / RDP 断连重连期间反复唤起-隐藏 → 无崩溃、窗口落在可见屏、居中正确。

### 7.5 场次 D：非 Windows（约 15 分钟）

- **§5.2 O6 ④**：linux / macOS 包实按热键、Wayland 降级、双开提示。
- **§5.12 D1**：Linux 面板中文可读截图留档。

### 7.6 记录与回写

- 每腿完成 → 勾选本清单对应行 / [清单 §5](./未完善事项清单-2026-10-25.md) 行尾注 `✅ <日期> 真机：<证据要点>`；证据路径写 `%TEMP%\walk-<日期>\`。
- 发现缺陷 → 新开 implementation 台账行 + CHANGELOG 修复节，并按需回写方案文档；**不要**在走查中就改判据。
- 四场全部完成后：更新清单 §5 计数、方案状态与 INDEX；残留条件腿（I3 / N3 Chrome）维持记档。

---

## 销项纪律

每过一腿：对应文档验收行打勾 + 注记日期；全部销项后本文档移入 `docs/archive/` 并在 INDEX 状态列标注。
