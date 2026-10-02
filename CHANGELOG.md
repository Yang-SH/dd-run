# 更新日志（Changelog）

本文件记录 dd-run 的版本变更。

## [Unreleased]

### 稳定性（V-4/V-5 真机走查完成 + V-8 注记收口，2026-10-02）

- **V-4（R-16 open_url 失败 toast）**：失败腿 file:// 命令指向不存在文件 → `ShellExecuteW = 2` → 错误 toast「打开失败：ShellExecuteW = 2」可见（截图留档，文案 = `toast.open_fail`"打开失败：{e}" 含原因摘要）；成功腿（sample https 命令两腿）静默零 toast。**判据口径修正**：断网不构成 `open_url` 失败源（`webbrowser::open` 拉起默认浏览器即成功，断网失败宿主不可见）；`webbrowser::open` 失败分支（无默认浏览器关联）需摘除用户默认浏览器关联方可模拟，环境侵入不做 → 注记，与 file:// 腿共用同一 toast 单点 + r16 单测锚定。
- **V-5（R-19 托盘热键标签动态化）**：改绑 Ctrl+Shift+P 后重启，UIAutomation 实读托盘 tooltip = 「dd-run — Ctrl+Shift+P」（NIM_ADD 携带新组合；右键 NIM_MODIFY 懒刷新后二次读数一致）；托盘菜单「显示/隐藏面板 Ctrl+Shift+P」动态尾缀可见（截图）；改绑组合下 Toggle 正常（唤起位置与基线一致）。即时腿注记：`sync_tray_hotkey_label` 唯一写点在 `poll_hotkey` seq 配对确认分支内（未知 seq 事件在 sync 前被忽略），注入不可等价触发；与重启后腿共用同一 sync 单点 + r19 单测锚定（口径同 R-08）。
- **V-8（R-21 热键危险组合）注记口径收口**：等价腿真机——config 直写 Alt+Space（= 黄线警示「仍要保存」终态）启动：`RegisterHotKey` 成功 + 热键链路 alive（Toggle 唤起居中 (635,250)）；黄线警示腿渲染于捕获对话框，合成输入受限未视觉复核（口径同 R-08），黑名单广度由 `r21_hotkey_blacklist_matches` 锚定。
- **环境发现**：egui 设置页对 posted 鼠标点击不响应（10-01 v9s1「设置页截图」实为根页，9-30 v2 同证；GetWindowRect 无阴影偏移、ClientToScreen 修正后仍不响应）→ 设置页视觉腿一律注记口径（R-08 先例）。V-6/V-9/V-18en 待执行（V-18en 首跑 toast 已触发但 TTL 3000 ms 内未捕获——唤起须提前至 ~2 s 重跑）。

### 稳定性（V-10 真机走查完成：R-03 入站队列上界实证，2026-10-02）

- **V-10 判据（Win11 25H2 build 26200.9457、debug 构建）**：
  - **构造**：flood 扩展（用户目录 `com.example.flood`，trust.json 预置 allow 双哈希）完成协议握手后以 128 KiB/帧 合法通知帧高频刷 stdout（扩展侧仪表日志对账：共写 **16,000 帧 ≈2.0 GB @ 128 fps**）；
  - **RSS 收敛于队列上界且不再增长**：队列填满（128 帧 × 128 KiB = 16 MiB）后 94 个 1 s 粒度采样 PRIV 恒 106.4–109.0 MB（波动 ≤2.6 MB），期间 ≥12,000 帧被丢弃而注入持续；另以 640 KiB 帧预跑标定队列涨落全周期（满 173.4 MB → 空闲回收释放后 89.1 MB，Δ84.3 MB ≈ 128 × 640 KiB）；
  - **恢复可见恰一次溢出告警**：t=97 s（已注入 ≈12,150 帧 ≥10,000）唤起 → 错误 toast「扩展 com.example.flood 输出过快，已丢弃 12016 条消息（面板隐藏期间积压溢出）」可见（截图留档 `%TEMP%\ddrun-v10c\`）；
  - **每扩展每会话至多一次**：复注 episode 2（丢弃 1,807 帧，dd-host 逐 episode 日志留痕）→ 二次唤起无新 toast（全日志「toast 告警」恰 1 条，截图对照）；
  - **回归**：告警后 3 次显隐切换正常、config.json 逐字节未变、panic.log 零新增。
- **走查环境注记**：(a) 帧尺寸取 128 KiB 而非判据示例的 1 MiB——`Decoder::push` 的 S-02 增量缓冲每 push 从头重扫找 `\n`，debug 读路径单帧成本 O(F²)（1 MiB 级帧实测 ≈7–17 fps，10,000 帧需 10+ 分钟，超出 warm 池 120 s 空闲回收租期；128 KiB 帧实测 128 fps），队列上界口径不变（上界 = 容量 × 单帧上界，机制与 ≤128 MiB 示例同源）；(b) 隐藏期注入窗口以「单次 warm 租期 ≤120 s」为界——R-03 与 C 批空闲回收构成双层防护（回收即清空队列，实测 PRIV 173.4→89.1 MB）；(c) 注入/采样/取证脚本与 flood 扩展走查后弃置（仓库零残留）。**R-03 至此三口径收口（单测 + V-10 + 回归）**。

### 稳定性（V-11 真机走查完成：R-04 超时降级实证，2026-10-01）

- **V-11 判据（Win11 25H2 build 26200.9457、debug 构建）**：
  - **构造**：用户开始菜单放置指向死 UNC 的 `.lnk`（目标 `\\203.0.113.43\share\dead.exe`，RFC 5737 TEST-NET 保证静默丢包；SMB metadata 实测挂 ~21 s）；
  - **T 内超时返回**：冷启动完成 **5073 ms**（基线 867–990 ms）——apps 首调恰在 T=5 s 超时按期返回（差额 73 ms = 聚合其余开销），非永久「加载中」；
  - **首屏降级落地**：计算器/系统/网络搜索/Shell 四内置照常 warm，挂死滞留线程不影响聚合 scope 其余线程；apps 未 warm（Failed 状态经聚合落定——扩展卡 Failed + Retry 渲染为既有已测路径；设置页卡片现场因会话合成输入受限未视觉复核，注记口径同 R-08）；
  - **收尾复验**：删除 `.lnk` 后冷启动恢复 990 ms、应用 warm 115 命令；全程 panic.log 零新增。
- **走查环境注记**：(a) 命令行转义经 WScript.Shell 造 .lnk 会把 UNC 存坏（读回 `C:\192.0.2.1`），须脚本文件内干净构造；(b) 本机 SMB 对不可达主机 metadata 挂 ~21 s（raw TCP 同），且**失败按目标地址短期缓存**（同目标二查 5 ms）——探测与 .lnk 目标须用不同 IP；(c) 超时滞留线程受 OS 网络超时上界自然结束，符合 R-04「不 kill」设计。**R-04 至此三口径收口（单测 + V-11 + 回归）**。

### 稳定性（R-26：单实例互斥落地 + V-15 真机走查完成，2026-10-01）

- **背景**（R 系列加固批四收官项）：双开的实际危害——① 第二实例热键注册失败触发「可能被其他程序占用」toast，**误导排障方向**（真凶是自己）；② 托盘双图标；③ config.json 并发写丢设置（R-02 原子写只保证文件不损坏，不解决 last-writer-wins）。
- **实现**（`main.rs` `enforce_single_instance`，O4 init 后、eframe 创建前）：
  - `CreateMutexW` 命名 `Local\dd-run-single-instance`（本登录会话命名空间，与 per-user 数据目录口径一致）；`ERROR_ALREADY_EXISTS` → 双语 `MessageBoxW`「dd-run 已在运行…」提示后 `process::exit(0)`；
  - **句柄有意不 CloseHandle**：命名互斥体随进程退出由内核释放，「句柄存活 = 实例存活」正是单实例语义（命名 + 句柄语义留痕函数文档——单测不可达项的代码评审口径）；
  - 互斥体创建失败降级不互斥照常启动（护栏不成为启动失败新单点）；非 Windows 占位 noop；**零新依赖零特性变更**。
- **V-15 真机走查（全自动：双开 → 弹窗取证 → 程序化确认）**：
  - 实例 A 运行中启动实例 B → B 弹双语警告弹窗（窗口类 #32770、标题 dd-run，截图留档）→ 程序化点击「确定」后 B 退出，B 日志命中互斥口径；
  - **原实例三面无损**：A 进程存活、config.json SHA-256 双开前后逐位一致（互斥拦截先于设置读取）、热键链路 alive（`WM_HOTKEY` 注入 Toggle 完整显隐循环，唤起位置 (635,250) 与 V-1 基线一致）、panic.log 零新增；
  - 附带开机自启复启口径：Run 键复启 = 再次执行 dd-run.exe，与本走查第二实例路径相同（命中互斥弹窗退出）。
- **门禁**：`cargo test --workspace` **573/573 全绿**、fmt 零差异、clippy 零告警。**批四代码项（R-03/R-04/R-26）全部落地**；余 V-10/V-11 真机走查与 V-13 内存基线（批四收尾）待执行。

### 稳定性（R-04：in-process 调用超时 + apps 负缓存，2026-10-01）

- **背景**（R 系列加固批四第二项）：in-process 内置调用原「纯函数调用无超时」——但 apps 首调枚举 ~400 应用（COM + 逐项图标提取），死 UNC 快捷方式的 `is_file()` / `GetImage` 可阻塞至网络超时：聚合 `thread::scope` 的 join 被拖死（首屏永久「加载中」）或该扩展 `inflight` 永占（回复 busy 直到重启）。
- **修复**：
  - **统一超时包装**：`ExtClient::InProcess` 改持 `Option<InProcessExtension>` + 可注入 `timeout`（初值 5 s，`INPROCESS_CALL_TIMEOUT`）；全部协议方法（initialize / top_level / fallback / get_command / get_items / invoke）经 `run_with_timeout`——扩展对象移入工作线程 + `recv_timeout`，按时归还放回槽位，**超时返回协议层 `Timeout`、扩展对象随线程自然滞留**（不 kill，受 OS 网络超时上界约束），后续调用答「滞留不可用」；
  - **滞留不回 warm 集**：`store_warm_process` 单点拒绝（invoke / page / fallback 三条归还链路共用），`mark_source_failed` 把该扩展**本会话标记 Failed**——扩展卡错误行 + Retry 按钮（重试经重聚合重建全新客户端）；
  - **apps 负缓存**：枚举体抽 `enumerate_apps` + `negative_cached`——枚举 panic 就地接住，空列表随 `OnceLock` 落负缓存，二次聚合不再重付全量枚举（恢复口径与会话级 Failed 一致：重启宿主）；
  - **panic 隔离面不变**：M9 `catch_unwind` 在扩展侧先行接住（in-process 调用线程消失分支仅为理论兜底）。
- **测试**：r04 两条单测（假慢扩展 300 ms × 注入 50 ms 超时 → `Timeout` + 滞留 → `inflight` 复位 + Failed；枚举 panic → 负缓存命中、枚举体计数恰 1）绿；`cargo test --workspace` **573/573 全绿**（+2），fmt 零差异、clippy 零告警。
- **真机冒烟**：五内置扩展全 warm（115+1+5+1+1 命令）、冷启动 867 ms 与基线一致——in-process 改走工作线程后聚合链路无回归。
- **待执行**：V-11 真机走查（构造指向死 UNC 的 `.lnk` → 首屏降级落地、apps 呈 Failed + Retry、无永久「加载中」；超时阈值 T 初值 5 s）。

### 稳定性（R-03：扩展 stdout / host 请求入站队列定容 + 溢出告警，2026-10-01）

- **背景**（R 系列加固批四首项）：子进程读线程把帧推入**无界** `mpsc::channel`，仅在面板可见的 `ui()` 里排空——流氓/有缺陷扩展在面板隐藏期间刷 stdout（每行 ≤1 MiB 不触发 TooLarge）→ 内存无界增长 → OOM；`host_requests.push` 同构。
- **修复**：
  - **入站帧队列定容 128**（`InboundGate`：`sync_channel(128)` + `try_send`）——满则**丢新帧**并置位溢出标志，不阻塞读线程（阻塞会反压子进程 stdout 管道）；RSS 上界 ≈ 128 帧 × 1 MiB，不再随注入增长；
  - **溢出告警**：`ExtensionProcess::take_inbound_overflow()`（episode 语义，观察即复位）→ dd-host `log::warn` + 面板错误 toast `toast.ext_overflow`（zh/en 双语），**每扩展每会话至多一次**防刷屏（`overflow_warned` 集，与 R-17 同口径）；
  - **`host_requests` 定容 32 drop-oldest**（`push_capped` 参数化，溢出记日志留痕）；
  - **in-process 适配器同口径**：host_requests 定容 32 + 顺带补齐 notifications 的诊断总线截断（64），与子进程路径 M9 R1 终态等价纪律对齐。
- **测试**：r03 三条单测（超容队列恒 128 + 标志置位 + episode 复位 / 恰容不误报负例 / >32 条保留最新 32 条最早丢弃）绿；`cargo test --workspace` **571/571 全绿**（+3），fmt 零差异、clippy 零告警。
- **环境注记**：本会话裸测 GNU 工具链 `dlltool.exe` 不在 PATH（rustup 工具链 `self-contained` 目录内含），补 PATH 后门禁正常——与产品无关。
- **待执行**：V-10 真机走查（隐藏期 ≥10 000 帧注入 → RSS 收敛至队列上界且不再增长 + 恢复可见后一次溢出告警）。

### 稳定性（V-16 真机走查完成：R-25 panic 取证落盘，2026-10-01）

- **V-16 判据（Win11 25H2 build 26200.9457、debug 构建 @ `50d77c6` + 临时探针）**：
  - **注入 panic**：`main.rs` 挂临时环境变量门控探针（`DDRUN_V16_PROBE=1` 时后台线程 `v16-probe` 启动 4 s 后 `panic!`；走查后移除，工作树零残留）→ `%APPDATA%\dd-run\logs\panic.log` **恰新增一条**：`[2026-10-01T10:46:52.808+08:00] thread "v16-probe" panicked at crates\dd-gui\src\main.rs:62:17:` + message——时间戳（RFC3339 本地时区毫秒）/ message / location 三要素齐备（另有线程名）；
  - **回归：默认 panic 流程不变**——panic 落在后台线程：进程同 PID 存活继续运行（ui 循环日志持续输出），stderr 同步出现默认 hook 输出（`thread 'v16-probe' panicked …` + `RUST_BACKTRACE` 提示）= 落盘后标准 panic 流程未被替换；
  - **正常退出路径零新增**：移除探针重建后干净实例运行 12 s（冷启动完成 885 ms）→ 枚举进程顶层窗口 `PostMessageW(WM_CLOSE)` 优雅退出（进程消失）→ `panic.log` 保持 2 行（仅探针条目）不变。
- **环境注记**：winit/eframe 0.36 事件循环不消费线程级 `PostThreadMessageW(WM_QUIT)`（投递返回 True 但进程不退出）——优雅退出改用枚举顶层窗口 `WM_CLOSE`（CloseRequested → `run_native` 正常返回），与托盘「退出」同汇于正常关机链路。
- **文档**：stability-usability-security-plan.md v1.9（§6.4 R-25 勾选与注记、§10 演进、文档头版本补同步 v1.6→v1.9——v1.7/v1.8 两轮头部漏更正）。零代码改动（探针走查后移除）。

### 稳定性（V-12 真机走查完成：R-07 剪贴板写入移出 UI 线程，2026-10-01）

- **V-12 判据（Win11 25H2 build 26200.9457、debug 构建 @ `0258eb1`）**：
  - **基线复制（成功路径）**：calc 查询 `7-3` → Enter → `host/set_clipboard` —— 日志 `入队：3 字节 → 成功`，剪贴板内容实测 `= 4`；
  - **占用态复制（失败路径）**：独立占用器进程持隐藏窗口句柄 `OpenClipboard` 不释放（`OpenClipboard(NULL)` 在本机 200 次尝试恒失败，属环境限制——改真实窗口句柄后即成功；其他进程开启被阻塞实证）→ calc 查询 `4-1` → 日志 `入队 → 失败：The native clipboard is not accessible due to being held by another party.`，**失败 toast 可见**（「扩展 com.ddrun.calc 剪贴板写入失败：…」{e} 含原因摘要，截图留档）；
  - **核心判据：占用期间工作线程阻塞时 UI 零冻结**——6 次显隐迁移 avg 64.2 ms / max 105 ms（与无占用基线 66 ms 无差别）；
  - **无污染**：占用期写入失败未改变剪贴板内容（释放后实测完好），用户原剪贴板文本保存还原。
- **走查注记**：查询含 `/` 字符时 A3 过滤 0 命中（兜底模板不在命中集），改用 `-` 触发 calc；失败 toast 截图须落在到达（~T+1s）与过期（T+4s）窗口内。
- **文档**：stability-usability-security-plan.md v1.8（§6.4 R-07 勾选与注记、§10 演进）。零代码改动。

### 稳定性（V-2 真机走查完成：R-02 原子写 + R-17 保存失败一次性 toast，2026-10-01）

- **V-2 判据（R-02 + R-17 同场，Win11 25H2 build 26200.9457、debug 构建 @ `0258eb1`）**：
  - **20 次原子写（内容各异）**：外部 `MoveWindow` 变尺寸 → 隐藏触发 `persist_panel_size → save_settings_with_feedback → atomic_write`，panel_size 逐次变化（660×540…740×600），rename 替换既有文件 20 次，零 panic、零 `.tmp` 残留；
  - **重启设置保留**：强杀 + 重启后 config.json SHA-256 逐字节一致；
  - **无卡顿回归判据**：连续 20 次唤起-隐藏单次迁移延迟均值 66 ms / 峰值 424 ms（轮询分辨率 40 ms）；
  - **R-17 恰一次口径**：`config.json` 置只读（先以 MoveFileExW 独立实验证实 rename-over-readonly 失败机制）→ 变尺寸后隐藏触发保存失败 → 错误 toast「设置保存失败——本次修改可能未保存，请检查配置目录是否可写」可见（3000ms TTL 内截图留档）→ 过期 3.3 s 后第二次失败保存**无新 toast**（每会话恰一次）→ `attrib -R` 恢复可写 → 保存成功且静默；
  - **终态还原**：尺寸回默认 → panel_size 写回 null → config.json 与基线逐值一致。
- **环境注记**：本会话合成鼠标事件被 winit/egui 忽略（hover 像素级对照实验证实）、修饰键取真实键态（GetAsyncKeyState）——设置页开关无法经注入触达；「切换设置 20 次」以同链路（`atomic_write` 成败反馈完全同源）的变尺寸落盘替代，设置页开关路径由 r02/r17 单测覆盖。首跑时序缺陷（两次失败间隔 < toast TTL，残影误判「重复弹」）发现后以正确时序（间隔 >3 s）补跑收口。
- **文档**：stability-usability-security-plan.md v1.7（§6.4 R-02/R-17 勾选与注记、§10 演进）。零代码改动。

### 稳定性（批一真机走查首批收口：R-01 V-1 完成 + 批一冒烟 + R-24 全量补跑，2026-10-01）

- **V-1（R-01 最小化 `screen_rect` 崩溃面）**：Win11 25H2 build 26200.9457、debug 构建 @ `0258eb1`——普通唤起-隐藏 30 次 + 「可见→SW_MINIMIZE→驻留 ≥1.6 s（≥1 个 1 Hz 看门狗帧落在最小化态）→最小化态下热键唤起」20 次，全程进程存活、零 panic、stderr 零错误日志、`%APPDATA%\dd-run\logs\panic.log` 零新增；基线/终态唤起位置逐位一致（635,250,1285,782 = 1920×1080 工作区居中，遮罩/缩放热区判据内含，截图留档对比一致）。**环境注记**：本会话合成键盘输入被 UIPI 拦截（GetAsyncKeyState 探针证实），唤起改用 `PostThreadMessageW(WM_HOTKEY)` 直注热键线程——与 `RegisterHotKey` 成功后系统投递的消息同源，覆盖的正是 R-01 修复的 show/hide/最小化重绘路径；真实键盘热键链路由 9-30 V-3 走查侧证。
- **批一收尾冒烟（V-1 同场）**：R-05——启动日志确认热键 / 托盘 / CJK 字体三线程全部正常起跳、无降级 `log::error!` 触发，热键线程承载 73 对 show/hide 全程正常；R-06——启动期 websearch 引擎配置读路径正常、全程日志零锁相关异常。
- **R-24 遗留门禁收口**：`cargo test --workspace` **568/568 全绿**补跑确认（21 目标，0 失败，`Os error 231` 环境批已自愈）；fmt 零差异、clippy 全仓零告警。
- **R-08 注记（冒烟未达，留待后续真机会话）**：本机 Everything 运行中（IPC 主通道健康），es.exe 错误分支现场不可达；文件搜索页因会话内合成输入受限（UIPI 拦截 SendInput + winit 不消费投递 WM_CHAR / 修饰键）未能到达，测试会话中检测到用户回场操作后为免干扰主动停止注入。错误路径以 `r08_error_truncate_multibyte_safe` 单测为准。
- **文档**：stability-usability-security-plan.md v1.6（文档头状态「规划中」→「实施中」、§6.4 勾选与注记、§10 演进），INDEX.md 同步（v1.6 / 实施中）。零代码改动。

### 可用性（R-24：sidecar 自述更正——移除已废弃的 `f ` 前缀入口描述，2026-09-30）

- **背景**（R 系列加固批三）：`f ` 前缀直达入口 2026-09-19 已移除（search.md §2 ⚠），但 `dd-ext-search` 的 spec 自述与顶层入口副标题仍写「输入 f 后空格直接进入」——该字段当前未上屏，属潜伏误导，描述一旦展示即穿帮。
- **修复**（`dd-ext/src/bin/search.rs`）：两处文案更正为 Ctrl+F 口径（zh：「Ctrl+F 一键直达」/「或按 Ctrl+F 直达」；en：「Ctrl+F to jump in」/「press Ctrl+F to jump in」），与 search.md §2 一致；双语文案提为四条常量（`EXT_DESC_*` / `TOP_LEVEL_SUBTITLE_*`）供单测双语锚定。
- **回归测试**：`r24_sidecar_description_matches_spec`——zh/en 四条常量逐一断言不含 `f ` 前缀字样、含 Ctrl+F 关键短语；spec.description 与顶层副标题经 `tr` 与常量同源（防绕开常量内联漂移）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `r24_sidecar_description_matches_spec` 及 dd-ext-search 54 条全绿；分目标实测 dd-protocol 30 / websearch 4 / dd-host lib 73 / dd-ext lib 118 / dd-gui lib 260 全绿。全量 `cargo test --workspace`（预期 568/568）被 `Os error 231` 环境批（第二次，spawn 依赖用例挂起/失败，与本改动无关）暂时阻塞，环境自愈后补跑确认。无独立真机 V 项（该字段当前未上屏），文案随批三代码评审核对。

### 可用性（R-23：查询长度 clamp 256 字符，2026-09-30）

- **背景**（R 系列加固批三）：`set_query` 入口无任何 clamp——超大粘贴直入模糊匹配（nucleo 打分），文件搜索页还会经 200ms debounce 把查询送进 es.exe argv，超长输入只会离谱失败。
- **修复**：
  - **`state.rs`**：新增 `PanelState::MAX_QUERY_CHARS = 256`（**字符数**，多字节字符不产生半字符）；`set_query` 入口静默截断，返回**是否发生截断**（早退与普通调用点零破坏——返回值此前为 `()`、无调用方消费）。
  - **`ui/panel.rs`**：截断发生时把输入框光标**置尾**（`TextEdit::load_state/store_state` + `TextCursorState::set_char_range`，光标 = 截断后文本末尾，续接输入语义自然）。
- **回归测试**：`r23_query_clamp_boundary`——恰 256 不截断（返回 false）/ 257、300 截为前 256（返回 true）/ 300 个汉字按字符数截（256 字符 = 768 字节，无半字符）/ 同值重喂走早退返回 false / 空串恢复。200ms debounce 链路消费的是 clamp 后的查询，行为不变。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **567 passed / 0 failed**。真机冒烟（粘贴一次超长文本，文件搜索页不向 es.exe 传离谱参数）随批三收尾执行。

### 可用性（R-22：IME 组词期回车守卫——上屏不再误执行选中项，2026-09-30）

- **背景**（R 系列加固批三）：中文输入组词中回车 =「上屏」手势，但 IME 的 Commit 与 Enter 键事件常**同帧到达**；`handle_keys` 无条件消费 Enter 并激活 → 上屏那一帧可能直接执行选中行（危险命令有确认门，普通命令没有）。
- **修复（`app/keys.rs`）**：键消费后新增 `ime_composing` 帧判定——本帧事件表含 `Event::Ime` 的 **Preedit（组词中）/ Commit（上屏）** 事件则 `enter` 强制为假，激活跳过一次；下一帧无 Ime 事件即恢复正常。Enter 本身已被消费（吞掉）：上屏文本经 Commit 事件入框、与键事件无关，不会被输入框重复接收。IME 开启但非组词（英文直输）无 Ime 事件，行为不变。确认对话框与右键菜单的 Enter 分支同帧同口径被守卫。
- **回归测试**：`r22_ime_frame_skips_activate`（Page 命令项栈深断言，不依赖扩展进程）——① 同帧 Preedit + Enter → 不激活且 Enter 被吞（事件表无残留）；①' 同帧 Commit + Enter 同口径；② 无 Ime 帧 Enter → 正常激活（回归锚）；③ 下一帧恢复（headless 无真实帧边界，以清空事件表模拟）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **566 passed / 0 failed**。真机走查 V-7（微软拼音 / 搜狗组词中回车仅上屏不触发选中项；非组词回车行为不变）待执行。

### 可用性（R-21：热键危险组合警示黑名单补全，2026-09-30）

- **说明**：警示**口径**（对话框黄线警示、不阻止——「保存」即用户确认动作，对齐 PowerToys「可能错误触发检测」语义）已随 2026-09-30 捕获对话框批先行落地，但黑名单当时仅覆盖 Alt+Space。本批补全方案要求的**名单广度**：
  - **`app/keys.rs`**：`is_system_reserved_combo` 由仅 Alt+Space 扩为四组合——`Alt+Space`（窗口菜单，RegisterHotKey 可成功但劫持一切应用）/ `Ctrl+Esc`（开始菜单）/ `Ctrl+Shift+Esc`（任务管理器）/ `Alt+F4`（关闭窗口）。Win 系组合（Win+D/L…）不在名单：基础捕获本就录不进 Win 修饰（回落提示另行覆盖），LL 钩子可录但系统响应优先级更高——名单随真机反馈再扩。
  - **`text.rs`**：`set.hotkey.warn_reserved` 文案由「会打开窗口菜单」（Alt+Space 专属）泛化为「系统常用快捷键……可能被系统优先响应」。
  - **回归测试**：`r21_hotkey_blacklist_matches`——四组合逐一命中；Win+D（方案指名负例）、默认热键 Ctrl+Space（捕获默认路径不得出现警示）、Shift+Esc / Ctrl+Alt+P / Alt+E（修饰键子集不误报）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **565 passed / 0 failed**。真机走查 V-8（捕获 Alt+Space / Ctrl+Shift+Esc → 黄线警示出现 → 仍保存 → 注册成功）待执行。

### 可用性（R-20：清单解析失败的扩展不再无声消失——扩展卡 skipped 警告行，2026-09-30）

- **背景**（R 系列加固批三）：清单 JSON 写错一个逗号 → 扩展**无声消失**——`outcome.skipped`（manifest.rs 逐清单校验的失败原因）在聚合层被整体丢弃，用户无从排查；对比之下信任门 Pending/Blocked、Failed+Retry 都有完整呈现，唯独此面空白（V-6）。
- **修复**：
  - **`aggregator.rs`**：`merge_sidecar_scan` 收集两处扫描的 `outcome.skipped`——**仅 `SkipReason::is_error()` 条目**（`OtherPlatform` 是「设计上静默跳过」，照旧无声，不制造假警告），`Display` 直出宿主校验原因；`sidecar_dir` 参数化便于单测注入。`ExtensionSources` 增 `skipped: Vec<(路径, 原因)>`。
  - **数据链**：`AggregatePayload.skipped` → `PaletteApp.skipped_manifests`（重聚合整体替换）。
  - **`ui/settings_view.rs`**：扩展卡顶部渲染警告行（`set.ext.skipped_warn` 键 zh/en 框架双语；{reason} 为宿主侧 zh 口径的校验原因文本）——danger 色 11px + 截断 + 悬停全文，复用 Failed 行既有样式；**只读、不可 Retry**（无扩展行可重试，修正清单后重聚合即恢复）。
  - **页脚 note 语义零改动**（2026-09-04 用户决策口径维持）。
- **回归测试**：`r20_skipped_aggregated_with_reason`——正例（语法错误清单 → `skipped` 恰含（该路径，非空原因），且不进入 loaded）+ 负例（全合法目录 → `skipped` 为空、合法清单正常加载）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **564 passed / 0 failed**。真机走查 V-6（放置 JSON 语法错误的第三方清单 → 扩展卡顶部出现「路径 + 原因」警告行）待执行。

### 可用性（R-19：托盘热键标签动态化——tooltip 与菜单尾缀跟随改绑，2026-09-30）

- **背景**（R 系列加固批三）：托盘 tooltip 静态「dd-run — Win+Alt+Space」（D25/D38 静态妥协）、菜单「显示/隐藏面板\tWin+Alt+Space」尾缀硬编码——用户改绑热键后托盘持续展示错误组合。
- **修复**：
  - **`tray.rs`**：静态 `TOOLTIP` 改为共享标签 `TRAY_HOTKEY_LABEL`（`Mutex<String>`，跨线程语义同既有 `TRAY_LANG` 原子量口径，R-06 `into_inner` 中毒纪律）；tooltip 纯函数 `tray_tooltip(label)`。NIM_ADD 取当前值；**`show_menu` 每次右键在托盘线程本机做 NIM_MODIFY 懒刷新**——无跨线程推送机制，D38「NIM_MODIFY 跨线程复杂度不成比例」结论维持，改绑后无需重启、下次交互即同步。
  - **`text.rs`**：`tray.toggle` 键去掉内嵌 `\tWin+Alt+Space` 尾缀（zh/en），尾缀由 `show_menu` 按当前组合名动态拼接。
  - **`settings.rs`**：新增 `hotkey_combo_label(mods, vk)` 纯函数（复用既有 `hotkey_mods_label`/`hotkey_vk_label`）。
  - **同步点**：启动初值在 `main.rs`（托盘线程 spawn 前）写入；改绑注册成功经 `poll_hotkey` 唯一写点（「保存」与「恢复默认」共用）`sync_tray_hotkey_label()` 同步。未写入（空串）回落默认组合。
- **回归测试**：`r19_tray_hotkey_label_dynamic`（① 空串回落默认 + 默认组合展示 = D38 静态口径回归锚「dd-run — Win+Alt+Space」；② 写入 `Ctrl+Shift+P` → tooltip = 「dd-run — Ctrl+Shift+P」（V-5）；③ 跨线程写读）；既有 `tooltip_fits_sztip_buffer` 重写为参数化 szTip 128 单元上限守卫（默认 + 最长合理组合）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **563 passed / 0 failed**。真机走查 V-5（改绑非默认组合 → tooltip / 菜单尾缀显示新组合；右键即时 + 重启后两路径）待执行。

### 可用性（R-18：热键 toast 入 i18n——收口锚定，2026-09-30）

- **说明**：核心改动已随 seq 确认协议批（`1a20c8f`）先行落地——`toast.hotkey_updated` / `toast.hotkey_failed` 两键入表（zh/en），两处调用点改走 `t(self.lang_effective, …)`，成功 toast 带动态 `{combo}` 组合名。本批做收口核对与锚定：
  - **复核**：全仓 `show_toast` / `show_error_toast` 调用点 grep 复核，**零硬编码中文字符串残留**（R-18 所述「全 UI 仅有的两处绕过」已清零）。
  - **回归测试**：新增 `r18_hotkey_toasts_localized`——两键 zh/en 非空 + 关键短语语义锚定（zh「全局热键已更新」/「已恢复原热键」、en 对应短语），防后续文案漂移（回归判据「zh 语义不变」）；既有 `i18n_table_complete_both_langs` 双向完备性覆盖在表键。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **562 passed / 0 failed**。en 语言下两条热键 toast 的真机确认随 V-3 en 复跑待执行。

### 可用性（R-17：设置保存失败可见——每会话一次性错误 toast，2026-09-30）

- **背景**（R 系列加固批三）：`Settings::save` 写盘失败仅 `log::warn!`——不可写目录（OneDrive 占位 / AV 锁 / 策略）下所有修改「看似成功」，重启即回滚且无解释（V-2）。
- **修复**：
  - **`settings.rs`**：`save()` 改返回**是否持久化成功**（`()` → `bool`，22 处调用点返回值本就未被使用，零破坏）；抽出可注入核心 `save_to(path)`（单测不触真实 config.json）。原子写落点（R-02 `atomic_write`）不变。
  - **`app/mod.rs`**：全仓 22 处 `settings.save()` 调用点（`keys.rs` ×17 / `settings_view.rs` ×3 / `lifecycle.rs` ×2）统一改走 `save_settings_with_feedback()`——失败且未提醒过 → 置位 `settings_save_warned` + 错误 toast；**每会话至多一次**（flag 不复位，恢复可写后的修改本就会成功，不重复轰炸）；成功路径静默（无 toast）。
  - **i18n**：新增 `toast.settings_save_fail`（zh：「设置保存失败——本次修改可能未保存，请检查配置目录是否可写」/ en 对应），双向完备性单测自动覆盖。
- **回归测试**：`r17_save_to_reports_persisted_and_failure`（可写目录 `true`；Windows `share_mode(0)` 独占锁注入失败 `false`，与 r02 同款注入）+ `r17_settings_save_failure_toasts_once_per_session`（首次失败 toast + 置位 / 过期后再失败不重复 / 成功静默三段）。R-02 既有原子写单测零改动全绿（同一落点）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **561 passed / 0 failed**。真机走查 V-2（配置目录置只读 → 失败 toast 恰一次、恢复可写后不再出现；含 R-02 判据）待执行。

### 可用性（R-16：`host/open_url` 失败可见性 + spawn 门禁测试密闭化，2026-09-30）

- **背景**（R 系列加固批三）：`host/open_url` 两处失败路径此前仅记日志（`file://` ShellExecute 失败 `log::debug!`、`webbrowser::open` 失败 `log::warn!`）——websearch 选中后面板已 Dismiss，浏览器/关联程序启动失败即「应用把命令吃了」，用户无从感知（V-4）。
- **修复（`app/host_actions.rs`）**：两处失败路径接入 `show_error_toast`（与 R-07 剪贴板失败同一套基建），文案键 `toast.open_fail`（zh：「打开失败：{e}」/ en：「Failed to open: {e}」，`{e}` 为失败原因摘要）；`file://` ShellExecute 失败日志由 debug 升 warn。成功路径行为与既有日志零变化（S-03 拦截 toast 口径不变）。
- **顺带修复（R-12 遗留，`aggregator.rs`）**：回归门执行时发现 `spawn_gate_passes_first_party_sidecar` 在本机必红——R-12 起首方 sidecar 每次 spawn 重验双哈希（fail-closed），该测试的假路径与本机真实 trust.json 钉扎状态耦合恒判 Pending。将 spawn 唯一入口的信任判定抽为可注入（新增 `spawn_and_initialize_with` + 生产判定 `assess_for_spawn` / 路由纯函数 `uses_sidecar_assessment`），测试改注入密闭化（不触真实台账），并新增路由单测 `spawn_gate_routes_first_party_sidecar_to_sidecar_assess`（首方 sidecar 走 `assess_sidecar` 分支 / 非首方不走，正负例）。生产判定逻辑零变化（R-12 三态语义不变）。
- **验证**：`cargo fmt --check` 零差异 · clippy 全仓零告警 · `cargo test --workspace` **559 passed / 0 failed**（i18n 双向完备性单测覆盖新键）。真机走查 V-4（无默认浏览器 / 断网两失败源出 toast）待执行。

### 可用性（设置页尺寸优化：基准 650×640 → 780×700 + 最小尺寸动态化，2026-09-30）

- **背景**（真机反馈）：设置页在基准 650×640 下内容列仅 ~454px——主题三卡贴边、材质四键/着色三键紧凑、滑杆顶满，且全局最小尺寸 460×400 允许把设置页拉到布局崩坏（内容列 ~280px）。
- **修复（`app/mod.rs`）**：
  - **基准提升**：`SETTINGS_W/H` 650×640 → **780×700**——内容列 ~584px，主题三卡/材质四键/滑杆宽松化，外观首屏多显 ~1.5 卡；幅度对齐 PowerToys CmdPal 设置窗（~896 宽）保守一档，保持 launcher 内嵌设置页语境；
  - **最小尺寸动态切换**：进/出设置页帧间 diff 收口点（与 `InnerSize` 同帧）发送 `ViewportCommand::MinInnerSize`——进设置页 = 本页 clamp 后有效尺寸（面板拉不小；小屏等比收缩后 min 随之收缩，恒 ≤ 实际尺寸、不反向撑大窗口），返回根页恢复全局下限 460×400（根页行为不变）。egui-winit 0.36 运行时真实应用该命令（`window.set_min_inner_size`，自动 DPI 换算，源码实证）。
- **回归测试**：`settings_size_is_max_of_baseline_and_root` 断言更新（780×700 基准 / 584×~524 小屏收缩；(900,700) 记忆值与 (460,400) 触底两例语义不变）；根页 `root_panel_size` 系列测试零改动（`APP_W` 未动）。
- **验证**：`cargo fmt --check` 无差异 / clippy 零告警 / dd-gui 256 passed（1 项既有机器环境批除外）· release + dist 重打包（sha256 与 release 一致）。真机走查待做：进设置页 780×700 且拖拽拉不小；返回根页恢复原尺寸与 460×400 下限；768p 小屏 780×700 原样放得下（工作区 clamp 数学已验证）。

### 可用性（热键捕获对话框对齐项目 Fluent 规格 + 捕获期双守卫，2026-09-30）

- **背景**（真机复测三项反馈）：① 对话框用裸 `egui::Window` 渲染，与项目设计风格不符（大白框、按钮无主次）；② 捕获期按键后面板隐藏、无法完成设置；③ 重复按键可"强制设置"被拦截的热键。
- **根因（代码核对实证）**：②②' 两个缺口**均只在基础捕获（LL 钩子自验证失败回落 egui）模式下发生**（真机截图红色降级提示佐证）：`poll_hotkey` 的 `Toggle` 分支无条件切换面板——回落模式下按键不被系统级吞掉，按下**当前已注册**的组合键仍送达 `WM_HOTKEY` → 面板即时隐藏 → `hide()` 复位捕获态、对话框销毁；`handle_focus_loss` 无捕获豁免——按 Win/Alt+Space 拉起开始菜单/系统菜单抢焦点 → 失焦自动隐藏，同样打断捕获。③ 核实**无自动应用路径**（钩子/egui 两路 Combo 均只写 `hotkey_pending`，保存是唯一注册入口）——现象即 ② 的切换/失焦循环销毁捕获态后反复重入完成保存；若 OS 层 `RegisterHotKey` 成功则热键真实生效（Ctrl+Space 类键通常只被 IME/第三方钩子在上游吃掉，不占用 Win32 热键表），属系统行为。
- **修复**：
  - **捕获期 Toggle 守卫**（`poll_hotkey`）：`hotkey_capturing` 时忽略 `Toggle`（钩子模式下 WM_HOTKEY 本就不会产生，守卫对钩子模式零行为变化；捕获结束恢复标准切换语义）；
  - **捕获期失焦豁免**（`handle_focus_loss`）：捕获是模态操作，系统 UI 抢焦点不销毁捕获；捕获结束恢复标准失焦隐藏（点击面板控件本身会带回焦点，正常路径无残留影响）；
  - **对话框重写为 `ui/confirm.rs` 同构 Fluent Dialog 规格**：Tooltip 层面板（panel 底 + 1px border + 圆角 8 + `dialog_shadow` + padding 20/20/16，内容显式定高消除大空框）+ Foreground 层 `theme::overlay` 遮罩（**点击遮罩 = 取消**，面板内点击不算，§10.1 语义）+ 高 32 按钮行（保存 = **accent 底白字主按钮**、禁用 = card 底灰字；重置/取消 = secondary；复用 `confirm::draw_dialog_button`）。
- **回归测试（+2）**：`toggle_ignored_while_hotkey_capturing`（捕获期 Toggle 不隐藏面板、捕获态存活；非捕获期照常切换）/ `focus_loss_hidden_while_hotkey_capturing`（捕获期失焦不隐藏；非捕获期照常隐藏；headless ctx 的 `viewport().focused` 恒 None = 未聚焦）。
- **验证**：`cargo fmt --check` 无差异 / clippy 零告警 / dd-gui 256 passed（1 项既有机器环境批除外）· release + dist 重打包 9,239,552 B。真机走查待做：回落模式下按当前热键/Win/Alt+Space——对话框保持打开、捕获态存活；对话框视觉与二次确认框一致。

### 可用性（热键改绑 seq 确认协议 + PowerToys 式捕获对话框，2026-09-30）

- **背景**（真机）：冲突组合（如被 PowerToys 占用的 Ctrl+Space）首次录入被正确拦截，但**同键再次录入显示「设置成功」而全局热键实际未注册**——旧链路「先写设置再异步注册 + 单槽 `hotkey_prev` 按到达顺序盲配回滚」：回滚分支多余补发的 `re_register(old)` 产生一次与用户操作无法区分的「假成功」`ReRegistered(true)`，与下一次尝试的失败事件错位配对后，失败事件落入「启动失败」分支（不还原设置），设置停留为冲突组合。
- **修复（`hotkey.rs` + `app/{mod,keys,lifecycle}.rs` + `ui/settings_view.rs`）**：
  - **seq 协议**：`HotkeyCommand{seq, mods, vk}`（`PostThreadMessageW` wParam=seq、lParam=mods<<32|vk）+ `ReRegistered{seq, ok}`——事件按 seq 配对，过期/未知事件一律忽略（roundtrip 单测锚定）；启动注册结果 seq=0（R-15 语义不变）；
  - **确认流**：捕获与「恢复默认」**只发请求不写设置**——`ReRegistered` 成功回发才是设置唯一写点；失败 → 对话框开着行内红色占用提示（候选保留可重试）/ 已关则 toast；2s 确认超时兜底（线程死亡/消息丢失不永久「应用中」）；UI 侧多余回滚命令删除，热键线程自动回滚保留；`hotkey_prev` 回滚状态机整体删除；
  - **PowerToys 式模态对话框**：全屏遮罩 + 居中窗（实时修饰键徽章 `GetAsyncKeyState` 每帧轮询 / 候选键帽 / 有效性提示行 / 占用红行 / [保存][重置][取消]，保存禁用态 = 无候选·未改动·应用中）；
  - **R-21 警示口径顺路落地**：Alt+Space 等系统保留组合保存前黄线警示、不阻止（对齐 PowerToys「可能错误触发检测」文案语义）。
- **回归测试（+6，`r15_hotkey_failed_state_flags` 按 seq 协议重写）**：`hotkey_confirm_seq_pairing_rejects_stale_and_mismatched`（假成功/错配事件不得清在途确认、不得动设置，失败后重试正常）/ `hotkey_confirm_timeout_fails_without_touching_settings` / `hotkey_command_pack_roundtrip`（含 u32::MAX 边界）/ `hotkey_failed_rollback_status_drives_unregistered_flag`（回滚也失败 → 「未注册」位；回滚成功 → 不误置）/ `hotkey_died_with_pending_confirm_still_toasts`（线程死亡恒有 toast）/ r15 三段流转。
- **核查收尾（同日代码核查识别的 2 个死角）**：① `ReRegistered` 增 `rolled_back` 字段——旧键回滚**也**失败时（新键与旧键同被抢占的极端边界）UI 置 R-15「未注册」位，设置页不再声称旧键生效；② `Died` 恒发错误 toast（原先「确认在途 + 对话框已关」场景完全无感知）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace` 零告警 / `cargo test --workspace` 全绿（除 1 项既有机器环境批）· release + dist 重打包 9,277,952 B。**真机走查已完成（2026-09-30，本机无 PowerToys，以 `RegisterHotKey` 助手进程等价占用）**：候选录入（Ctrl+Alt+J）后助手注册同键，连续两次保存均行内红色占用提示 + 日志 `seq=3/4` 双失败回滚、对话框保持、设置零变形；另完成 V-3（启动冲突错误 toast + 热键卡「未注册」徽标）、正常路径零误报、重注册后徽标复位。环境记档：本机 LL 键盘钩子被安全软件拦截（自回声验证失败）→ 被占用组合键对 egui 捕获层不可见，故以「录入后占用再保存」等价「与 PowerToys 同开连续录入」场景（详见 stability-usability-security-plan.md 批三 R-15 条）。

### 安全（R-12：首方 sidecar 首跑钉扎，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-12（中）——`assess` 对 `id ∈ FIRST_PARTY_IDS` 的随包 sidecar 白名单短路发生在哈希**之前**：`dd-ext-search.exe` 零完整性校验、从不参与 `hashes_match`。便携 zip 分发下 `extensions.d\` 在用户可写位置——同用户程序替换 sidecar exe（或加一份指向自己的 `com.ddrun.filesearch.json`）→ 每次启动（含开机自启）**静默拉起，零审批、台账零痕迹**。
- **修复（`dd-host/trust.rs` + `dd-gui/aggregator.rs` + `ui/settings_view.rs`）**：
  - **白名单短路废除**：`assess` 的 Sidecar 特判移除（S-05 既有测试 `first_party_sidecar_is_auto_trusted` 同步更名锚定新语义：无台账 → Pending）；
  - **新增 `assess_sidecar`（含可注入台账核心 `assess_sidecar_with_ledger`，单测不触真实 trust.json）**：① 首跑钉扎（双哈希 + `Decision::Allow` + `origin="sidecar"` + `pinned_host_version` 入账 → AutoTrusted）；② 钉扎有效（每次 spawn 重验双哈希）→ AutoTrusted；③ **同版篡改**（哈希不符且钉扎版本 == 当前宿主版本）→ `Pending` + `sidecar_tampered` 告警位，**绝不自动重钉**；④ **升级重钉**（哈希不符且钉扎版本 ≠ 当前宿主版本）→ 静默重钉新哈希，零摩擦升级（信任锚 = 应用分发包本身）；⑤ 用户 Deny 记录始终优先（哈希不符 → Pending，不静默覆盖拒绝）；
  - `TrustEntry` 增 `origin` / `pinned_host_version` 字段（serde default，旧台账文件零兼容负担；trust.json 非冻结契约、永不导出，N5 已定）；
  - 聚合（第一道）与 spawn 门（第二道、所有子进程 spawn 唯一入口）双路由接入；
  - 设置页扩展行新增篡改告警行（i18n 键 `set.ext.tamper_warn` zh/en，双语完备性单测绿）。
- **回归测试（+4）**：`r12_sidecar_pin_on_first_sight` / `r12_same_version_tamper_pending`（断言台账不被改写）/ `r12_upgrade_repin_silent` / `r12_deny_entry_overrides_pinning`（超出方案三项的 Deny 优先用例）；S-05 既有 18 项信任门测试全绿（第三方扩展信任行为零变化）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **546 passed / 0 failed**。V-9 真机走查（同版替换 `extensions.d\dd-ext-search.exe` → 设置页 Pending 告警 + Ctrl+F 明确提示；正常升级 → 静默重钉零摩擦）待执行。

### 安全（R-14：清单 / config.json 读盘限幅，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-14（低）——`load_manifest`（对 `extensions.d` 下每个 `.json`）与 `Settings::load`（config.json）的 JSON/schema 校验都发生在**读入之后**：4 GB 外形合法的 JSON → 每次启动整读+解析 → 启动期 OOM/长挂。
- **修复**：两处读入前 `fs::metadata` 限幅 **1 MiB**——`dd-host/manifest.rs` 超限记 `ParseError` 跳过（含 R-14 标记，不整读不解析）；`dd-gui/settings.rs` 的 `load` 最小参数化为 `load_from(path)`（对齐 R-09 参数化纪律）并接入限幅，超限记日志回落默认。
- **回归测试（+2）**：`r14_manifest_over_limit_skipped`（2 MiB 清单 → `ParseError` 跳过，**同目录其余扩展正常加载**，跳过项路径精确断言）/ `r14_config_over_limit_defaults`（2 MiB config.json → 回默认不 panic；限内正常文件照常解析）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **543 passed / 0 failed**。批二冒烟（放置一次超限清单后正常启动）随批二收尾。

### 安全（R-11：Start Menu `.url` 读盘限幅，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-11（中）——`url_target` / `url_icon_file` 对 `.url` 直接 `std::fs::read`，无 metadata 限幅。Start Menu 内容不受信（审计信任假设 C）；本扩展 M9 起 in-process，数 GB `.url`（恶意安装包放置）→ 每次启动首屏聚合整读 → OOM/挂死，无需扩展批准、持久复现（S-04 同类漏网）。
- **修复（`builtins/apps.rs`）**：抽共享 `read_url_limited(path)`——先 `fs::metadata`，**非普通文件或 `len > 64 KiB` 直接 `None`**；`url_target` / `url_icon_file` 两处接入。正常 `.url`（INI 文本，通常 <1 KiB）远小于上限，行为不变。
- **回归测试（+3）**：`r11_url_over_limit_rejected`（70 KiB 含合法 `URL=` 行 → 两个函数均 `None`）/ `r11_url_normal_parsed`（正常小文件照常解析出协议目标）/ `r11_url_directory_rejected`（目录 → `None`）；既有 `url_target_parses_ascii_and_utf16` 回归零改动。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **541 passed / 0 failed**。批二冒烟（正常 `.url` 应用不受影响）随批二收尾。

### 安全（R-13：calc 求值器递归深度上限，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-13（低）——`parse_atom` 的 `'('` 分支递归回 `parse_expr`，深度 ∝ 输入长度；calc 在宿主进程内执行，**栈溢出是 abort（`catch_unwind` 拦不住）**。入口：粘贴 ~10^5 个 `(` 后选中 `= …` 兜底项。
- **修复（`builtins/calc.rs`）**：`Parser` 加 `depth: u32` + `enter()` 守卫——括号嵌套与一元符号链各计一层，超 **256** 回 `EvalError::Domain`（递归在展开前拒绝）。**实现期加固**：`parse_unary` 的 `'+'/'-'` 自递归是方案未列出的同源无界向量（`-----1` 一类），一并纳入守卫。
- **回归测试（+2）**：`r13_depth_over_limit_rejected`（257 层括号 / 257 个负号 → `Domain`）/ `r13_depth_at_limit_ok`（256 层内正常求值 + 括号符号混合合法用例）；calc 既有求值/错误/格式化用例零改动全绿。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **538 passed / 0 failed**。真机冒烟（粘贴超深表达式选 calc 兜底项，宿主不崩）随批二收尾。

### 安全（R-10：S-06 确认门多段命令绕过封堵，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-10（中）——`is_dangerous_command` 只取**首段**首词比对，`echo hi & rd /s /q C:\…\Documents` 首词无害即**不弹确认直接执行破坏段**。S-06 的威胁模型（粘贴误触/社工）恰恰最常以一行多段命令出现。
- **修复（`builtins/shell.rs`）**：新增纯函数 `is_dangerous_query`——按 `&`（含 `&&`）/ `|` / 换行字符级切段后**每段**各自过既有 `is_dangerous_command`，任一段命中即 `Confirm{is_critical:true}`；`shell.run.query` 确认门改接此判定。零协议改动（沿用 §8.3 确认重发机制）；引号内分隔符也切段，属保守方向（宁可多问一次）；`&&` 产生的空段判定恒 false，不误报。
- **回归测试（+6）**：`r10_dangerous_seg_amp` / `r10_dangerous_seg_double_amp` / `r10_dangerous_seg_pipe` / `r10_dangerous_seg_newline`（各含「首段无害、次段危险」正例）+ `r10_safe_multiseg_no_confirm` / `r10_safe_single_no_confirm`（全无害多段与普通单段不误报）；S-06 既有单段判定正/负例测试零改动全绿。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **536 passed / 0 failed**。

### 稳定性（R-07：剪贴板写入移出 UI 线程，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-07（P2）——`host/set_clipboard` 在 UI 线程 `thread::spawn(...).join()`：arboard 打开 Win32 剪贴板（与剪贴板管理器争 `OpenClipboard`）期间整个面板冻结，spawn 毫无收益。
- **改造（新增 `app/clipboard_worker.rs`）**：常驻工作线程 + mpsc——`SET_CLIPBOARD` 分支校验（S-07 长度上限前置不变）后**入队**即返回；工作线程逐条写入（每次新建 arboard 实例，与改前持锁窗口一致），结果回传 UI 线程经 `poll_clipboard_results`（ui 循环同帧消费）反馈：成功沿用 S-07 既有口径（info 日志 + 轻量 toast），失败走 `show_error_toast`（与 R-16 同一套失败提示基建，新增 i18n 键 `toast.clipboard_fail` zh/en）；工作线程创建失败走 R-05 降级口径（`log::error!` + 提示失败，面板其余功能不受影响）。
- **回归测试（+1）**：`r07_clipboard_worker_roundtrip`（假写入注入：请求入队 → 工作线程消费 → 成功/失败结果回传 UI 线程，断言 ext_id/字节数/错误信息；请求端 drop 后工作线程自然退出）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 零告警 / `cargo test --workspace` **530 passed / 0 failed**（含 i18n 双语完备性单测）。V-12 真机走查（剪贴板占用器场景：面板无卡顿 + 结果 toast）待执行。

### 稳定性（R-25：崩溃取证落盘 panic.log，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-25（P1）——全仓无 `panic::set_hook`，O4 日志恒写 stderr 且 release 无控制台（`windows_subsystem = "windows"`）：R-01 这类未知崩溃修复后，未来任何同级崩溃用户手里无痕迹可报、开发者无从诊断。
- **新增 `dd-gui::crashlog`**：进程入口（O4 init 之前最早）挂 `panic::set_hook`——panic 记录（时间戳 + message + location + 线程名）**追加写入** `%APPDATA%\dd-run\logs\panic.log`。纪律：hook 内目录创建/写盘/时间获取任何失败一律静默放弃（不二次 panic）；落盘后调用 `take_hook()` 取回的原默认 hook——**默认 panic 流程不变**。格式化抽纯函数 `format_panic_entry`（多行 message 原样保留，缺 location/线程名显式占位）。
- **配套**：`dd-host::manifest::logs_dir()`（复用 `%APPDATA%\dd-run` 数据根口径）；dd-gui 声明 `chrono = "0.4"`——**Cargo.lock 仅 +1 行依赖声明，包集合与版本零变化**（chrono 已在依赖树内，供应链面零新增；§7 audit 约定结论：无新包引入，本地未装 cargo-audit，待 CI 化时补跑）。
- **回归测试（+3）**：`r25_panic_log_format_states`（含 location / 多行 message / 缺 location 三态 + 线程名与时间戳断言）/ `r25_append_unwritable_path_degrades`（父级是文件 → `append_entry` 返回 `Err` 不 panic）/ `r25_append_is_append`（两次写入同文件均为追加非覆盖）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` **零告警** / `cargo test --workspace` **529 passed / 0 failed**。V-16 真机走查（注入 panic → panic.log 追加一条、正常退出不新增）待执行。

### 稳定性（R-09：测试基线脱机化 + clippy 基线清零，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-09（P2）——`builtins/apps.rs` sys 测试断言本机开始菜单含 Flowframes，任何其他机器/CI 必失败（本机复验确红：枚举列表无 Flowframes，与方案 §2.2 基线一致）。
- **修复（`builtins/apps.rs` / `bin/search.rs`）**：
  - 开始菜单递归枚举自 `app_list` 最小参数化为 `collect_lnk_fallback_from_root(root, …)`（逐字搬移，生产行为不变）；
  - **新增 `r09_apps_enum_from_fixture`**：临时目录经 COM `IShellLinkW::SetPath`（槽 20）+ `IPersistFile::Save`（槽 6）现生成根级/子目录真实 `.lnk` 夹具（目标 `System32\notepad.exe`/`cmd.exe`）→ 枚举结果含两夹具项（根级收录 = Flowframes 回归；递归；目标 exe 过滤走真实链路）；
  - 机器相关断言更名 `machine_steam_installed_shown_uninstalled_filtered_root_lnk_shown` + `#[ignore]`（注明前提与 `--ignored` 复验方法）；
  - thread_local clippy 告警排查：clippy 1.96 `missing_const_for_thread_local` **误报**（初始化式已是 `const {}`，关联常量/结构体字面量/直接调用三种写法均仍触发）→ 定点 `#[allow]` 注明理由，初始化式保持 const 形式。
- **验收**：clippy 全仓 **零告警**（基线清零）/ `cargo fmt --check` 无差异 / 裸机 `cargo test --workspace --no-fail-fast` **连续两次 526 passed / 0 failed 全绿**（即本项验收门）。

### 稳定性（R-08：es.exe 错误信息截断字节边界 panic 修复，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-08（P2）——`parse_response` 的错误信息按**字节**截断（`&t[..t.len().min(200)]`），本地化/中文路径下 200 字节可落在多字节字符内 → `not a char boundary` panic（sidecar 进程内，进程退出由宿主熔断兜底，故低危）。
- **修复（`dd-ext/src/bin/search.rs`）**：抽出 `truncate_chars(s, max)` 纯函数——`s.chars().take(max).collect()` 按整字符截断；`parse_response` 错误路径接入。
- **回归测试（+1）**：`r08_error_truncate_multibyte_safe`——CJK（300×3 字节，字节边界 200 落在第 67 字符内）与 emoji（60×4 字节）各一：整字符截断、字符数 ≤200、端到端解析失败路径不 panic；ASCII 截断行为不变（回归）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 仅既有基线告警（thread_local，属 R-09）/ `cargo test --workspace` **525 passed / 0 failed**。es.exe 出错路径冒烟随批一收尾。

### 稳定性（R-06：锁中毒级联 panic 修复，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-06（P2）——任何持锁 panic 后锁中毒，`websearch.rs` 的 `.expect("引擎配置锁未中毒")` 使 websearch 整会话失效（被 `catch_unwind` 接住变 `-32603`，功能报废至重启）；sidecar `search.rs` 的非测试 `.lock().unwrap()` 中毒即进程退出。
- **修复**：统一改 `lock().unwrap_or_else(|e| e.into_inner())`（`PoisonError` 取回内层数据）——`websearch.rs` 读/写 2 处；`search.rs` 全部非测试 `.lock().unwrap()`（:76/:89/:133/:139/:509/:520 六处生产路径，文档 :116 实为 `#[cfg(test)]` 辅助函数、一并治理）。这些锁保护的都是可整体重建的配置/索引缓存，中毒续用安全。
- **回归测试（+2）**：`r06_websearch_poisoned_lock`（持锁线程 panic 制造中毒 → 读路径取回预置内容完整、写路径仍可注入生效）/ `r06_sidecar_poisoned_lock`（`PATH_INDEX` 中毒后 lookup 仍命中既有条目、register 新条目成功）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 仅既有基线告警（thread_local，属 R-09）/ `cargo test --workspace` **524 passed / 0 failed**。

### 稳定性（R-05：启动期线程创建 `.expect` 降级，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-05（P2）——启动期线程创建失败即 `.expect` panic 整个启动器，是 panic 面清点后仅存的 fail-fast 残留。
- **修复（`hotkey.rs` / `tray.rs` / `platform.rs`）**：6 处全部对齐托盘既有降级口径（失败 `log::error!` + 继续运行）——① Windows 热键线程（降级为无全局热键，回发 `ReRegistered(false)` 事件与注册失败同语义，`HotkeyThread._handle` 改 `Option`）；② 非 Windows 热键占位线程 + ③ 测试桩 dummy（共用 `dummy_handle` 助手）；④ Windows 托盘线程（降级为无托盘，`TrayThread._handle` 改 `Option`）；⑤ 非 Windows 托盘占位线程；⑥ CJK 字体加载线程（降级为默认字体，中文可能显示为方块）。
- **验收（代码评审口径）**：grep 复核 6 处启动期线程 `.expect(` 清零（platform.rs 余 3 处均为测试夹具，非本项范围）；各降级路径均有 `log::error!`；单测不可达（线程创建失败无法稳定注入），无独立 V 项。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 仅既有基线告警（thread_local，属 R-09）/ `cargo test --workspace` **522 passed / 0 failed**。五步冒烟随批一收尾执行。

### 稳定性（R-02：持久化非原子写盘修复，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-02（P0）——config.json / trust.json / 冻结缓存三处直接 `std::fs::write`，写盘中途崩溃/断电即半截文件：config 丢全部设置；trust.json 解析失败 → `LedgerState::Corrupt` fail-closed，**所有已批准扩展回 Pending 需重新审批**。写盘频率高（每次设置切换、每次隐藏面板 `persist_panel_size`）。
- **修复**：两 crate 各落一个 `atomic_write(path, bytes)` 助手（方案口径：同目录写 `.tmp` → `std::fs::rename` 覆盖，Windows 侧 std 已走 `MOVEFILE_REPLACE_EXISTING`；失败尽力清理 `.tmp` 残留；目录缺失自动创建；刻意不做 fsync，崩溃窗口从「整个写入时长」缩到「一次 rename」）——`dd-host`（`lib.rs`，`trust.rs` 台账与 `cache.rs` 冻结桩两处接入）与 `dd-gui`（`settings.rs`，config.json 接入）。
- **回归测试（+6）**：`r02_atomic_write_replaces_existing` / `r02_atomic_write_missing_dir` / `r02_atomic_write_failure_keeps_old` 各 crate × 3——失败注入为 Windows `share_mode(0)` 独占打开目标使 rename 共享冲突失败（Unix 目录置只读），断言原文件完整、无 `.tmp` 残留；零新增依赖（沿用 `std::env::temp_dir` 测试约定）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 仅既有基线告警（thread_local，属 R-09）/ `cargo test --workspace` **522 passed / 0 failed**。V-2 真机走查（切换设置 ≥20 次无 `.tmp` 残留、只读目录一次 toast）待执行——其 toast 口径（R-17）随批三落地后一并验证。

### 稳定性（R-01：修复最小化窗口触发 `screen_rect` unwrap 崩溃，2026-09-29）

- **背景**：[stability-usability-security-plan.md](docs/stability-usability-security-plan.md) R-01（P0）——egui-winit 0.36 在 Windows 窗口最小化时将 `raw.screen_rect` 置 `None`，`ui/chrome.rs` 两处 `unwrap()` 使可见路径（1 Hz 看门狗重绘与 OS 态瞬时错位即可触达）panic 整个启动器，为运行路径上唯一确认的全应用崩溃向量。
- **修复（`chrome.rs` 单文件）**：新增辅助函数 `current_screen_rect`——优先取本帧 `raw.screen_rect`，缺失时回落 egui 已解析的 `viewport_rect`（`begin_pass` 中由上一帧 screen_rect 续接）；`chrome_begin` / `chrome_end` 两处接入，不引入新分支语义。
- **回归测试（+2）**：`r01_screen_rect_prefers_raw`（`raw` 有值优先取 `raw`，防语义漂移）/ `r01_screen_rect_fallback_raw_none`（最小化帧 `raw` 为 `None` → 回落上一帧已解析矩形，不 panic）。
- **验证**：`cargo fmt --check` 无差异 / `cargo clippy --workspace --all-targets` 仅既有基线告警（`dd-ext` thread_local，属 R-09 顺手修）+ 并行构建锁文件噪音 / `cargo test --workspace` **516 passed / 0 failed**。V-1 真机走查（Win+D / 最小化期间唤起-隐藏 ≥50 次）待执行。

### 文档（全仓格式规约审计与补强，2026-09-29）

- **新增** [`docs/doc-audit-2026-09-29.md`](docs/doc-audit-2026-09-29.md)（v1.0，生效中）：按 [INDEX] §4 格式规约的全仓文档审计与补强——状态取值归一（2 处非法 + 里程碑 8 份历史格式，均保留原描述）；元信息块补全 14 份（版本/最后更新取 git 实测）；规约外 emoji → 纯文字 12 文件 62 处（优先级/平台图例连动改写，1 处引文豁免加注）；INDEX 行数字段按 `wc -l` 实测回填 17 行；裸代码围栏补 `text` 标注 7 处；368 个相对链接全部可达。建议项（11 份历史文档版本演进补建等）留档其 §4。登记于 `docs/INDEX.md`（v1.35）。零代码改动。

### 文档（稳定性·可用性·安全性加固方案 R1–R24，2026-09-29）

- **新增** [`docs/stability-usability-security-plan.md`](docs/stability-usability-security-plan.md)（v1.2，**规划中**）：S-01–S-11 审计闭环后的新一轮加固规划——三路只读代码审查（安全/稳定/可用独立取证）+ 全量 file:line 回读源码核对。24 项发现：稳定 R1–R9（❌ `screen_rect` 最小化崩溃 / 非原子写盘丢设置与审批态 / 无界入站队列 / in-process 无超时 / 测试基线红等）、安全 R10–R14（S-06 确认门 `&`/`|` 多段绕过 / `.url` 无限读 / 首方 sidecar 零校验 / calc 栈溢出）、可用 R15–R24（启动热键失败静默 / `open_url` 与设置保存失败静默 / 硬编码中文 toast / IME 误触发等）。全部**零冻结契约改动、零新增依赖**；四批实施顺序（崩溃收敛→安全→可用→改造型）+ §6.4 任务勾选清单 + 真机走查总清单 V-1~V-12 + 不做与缓办清单 + 风险声明。v1.1 按格式规约对齐（状态取值 / 标记 emoji / 行号「约 :NNN」）并更正 2 处子代理误报（R-23 落点、R-06 计数）；v1.2 补任务勾选清单并修 §1 表格内竖线转义。登记于 `docs/INDEX.md`（v1.35）。零代码改动。

### 设置（外观：滑杆百分比移至滑杆同一行，v4.20，2026-09-29）

- **背景**：真机截图反馈——外观页「不透明度」百分比 `40%` 挂在行头描述行右缘，与下方滑杆分离，拖动时读值需跨行；同款还有自定义着色「着色强度」。
- **修复（`settings_view.rs` 单文件）**：新增 `draw_slider_row_with_pct`——滑杆占满除百分比预留宽（最宽 `"100%"`）外的整行，百分比右对齐与滑钮垂直居中**同一行**；取值在滑杆绘制之后，拖动当帧即显新值；预留宽固定，值位数变化不引起行宽抖动。「不透明度」与「着色强度」两处同款接入；`draw_opacity_slider` 增 `width` 参数改由调用方预算轨宽；描述行恢复占满整行（行头锁宽修法随之退役）。
- **文档**：`docs/implementation.md` P2 落点行补 v4.20 注记。
- **验证**：`cargo fmt` 无差异 / `clippy -p dd-gui` 无告警 / `dd-gui --lib` 236 passed / 0 failed；重新打包 `dist/dd-run-0.1.1.exe`（8.7M）。

### 文档（后续功能规划 N1–N5，2026-09-28）

- **新增** [`docs/future-features-plan.md`](docs/future-features-plan.md)（v1.0，规划中）：v0.1.1 后的功能向增量规划——立项判据「可行 × 无冲突」（零冻结契约改动 / 既有惯例可复用 / 零新增依赖 / 信任边界相容）+ 编号空间核查（取 **N 系列**，不预占 D43+）+ 现状底座五惯例取证。五项提案：**N1 自定义直达命令**（URL/路径，刻意规避 `f ` 前缀带参形态）/ **N2 内置扩展配置通道**（泛化 `DD_WEBSEARCH_ENGINES` 惯例，试点 apps 屏蔽名单）/ **N3 浏览器书签搜索**（新内置 in-process 扩展，Chromium 系，零网络）/ **N4 LRU 预热容量可配置**（承接 optimization-plan §2.7.1 转功能项）/ **N5 设置导入导出**（`trust.json` 永不导出）。附不做与缓办边界（扩展商店 / 剪贴板历史 / manifest v1.1 schema / 带参前缀直达等）。零代码改动；登记于 `docs/INDEX.md`（v1.33）。

### 设置（修复：行描述与右侧控件重叠，2026-09-28）

- **背景**：真机截图反馈——外观页「不透明度」行百分比与描述首行行尾叠画、「恢复默认外观」卡「恢复默认」按钮压在描述上。
- **根因**：egui 0.36 `Ui::wrap_mode()`——vertical 内 Label 默认在**整行可用宽**内换行（horizontal 直下才是 Extend）。`horizontal { 图标; vertical(标题+长描述); right_to_left(控件) }` 结构里，长描述换行后左列占满整行，RTL 子区剩余宽度归零，控件自右缘向左叠画在文字上。语言卡 v4.15 / 按键卡 D42 已按「右控件宽预算 + `allocate_ui` 锁左列」修过，本批 5 处漏网同款补齐。
- **修复（`settings_view.rs` 单文件）**：①「不透明度」行——百分比按最宽 `"100%"` 预留，左列锁宽（描述 `.wrap()` 显式化）；②「恢复默认外观」卡——按钮按 `fluent_button` 同口径预算（文字 + 24，36 下限），左列 `allocate_ui` 锁宽；③开机自启卡 / ④搜索应用卡——开关 40 + 16 间距预算（同 D42 公式）；⑤扩展行——右侧控件宽按开关 40 + 按需小按钮（`允许/阻止/重试`，文字 + 16、32 下限）动态预算，左列 `set_max_width` 锁宽（名称 / version·id 换行、来源标签截断均以此为界）。
- **验证**：`cargo fmt` 无差异 / `clippy -p dd-gui` 无告警 / `dd-gui --lib` **236 passed / 0 failed**；真机走查（debug 构建，650px 默认宽，zh 暗色）——外观页「不透明度」`40%` 右对齐独立成列、「恢复默认」按钮与描述间距正常，常规页开机自启 / 按键交互、搜索页搜索应用、扩展页 5 行开关均无重叠。

### 设置（按键与交互排版 + 全设置页控件字号统一，D42 / v4.19，2026-09-27）

- **背景**：真机截图反馈——全设置页控件文字与行名同级（14pt）、32 高控件盒塞进 36 高行块占比 89%，观感「字体偏大、比例失调」；「按键与交互」卡行距局促。方案与逐项验收见 `docs/settings-keys-typography-plan.md`（v1.1 含落地勘误），设计稿决策 **D42**、版本 **v4.19** 入稿 `cmdpal-ui-mockups.html`。
- **K1 排版（同形态行全设置页统一）**：设置行块 36→**40** ×8、行↔行与卡头→行 1 间距 8→**12** ×4、行名-描述间距统一补 `+2` ×4（顺带修正搜索行为卡的错注释）、左宽公式归一（`−40−16` 开关宽）。仅 `settings_view.rs`。
- **K2 控件排印回调一档（部分修订 D34 ③④；正文/行名 14 semibold 与描述 12 不动）**：散落字面量收口为常量组 `CONTROL_H=28` / `CONTROL_FONT_PT=12` / `POPUP_ITEM_H=24`——自绘下拉框、次级按钮、URL 输入框文字 14→12（ramp base200）、盒高 32→28（Fluent compact 成对比例）、popup 选项行高 28→24；按钮宽度预测量同步 14→12。
- **K3 下拉宽度自适应**：新增纯函数 `dropdown_width = clamp(最长选项宽 + 40, 180, 260)`，Esc 键行为卡（原 200）与语言卡（原 180）接入，消除英文长文案溢出盒外；**搜索引擎卡维持 260 定值不接规则**（预设名为短名，接规则会收窄背离 v4.9 加宽初衷——方案 v1.0 认知有误，v1.1 勘误）。
- **实施期更正**：方案 v1.0「zh 落 180（两卡等宽）」估宽前提不成立——Esc 卡最长选项实为「先清除搜索内容，然后返回」（13 全角字），zh 实测 ≈188（headless）/ ≈196（真机雅黑）；单测断言改为两分支精确断言（短集 = 180 / 超长集 = 260）+ 真实 zh/en 集落 (180, 260]。
- **回归测试（+2）**：`control_typography_constants_d42`（常量锚定防漂移）/ `dropdown_width_clamps_zh_and_en`（两分支 + 区间断言，含 headless 无 CJK 字体的 fallback 字形注记）。
- **验证**：`cargo fmt` 单文件校验无差异 / `cargo clippy --workspace --all-targets` 0 告警 / `cargo test --workspace` **515 passed / 0 failed**（基线 513 → 515）；`tools/docscan.py` 失效链接 (none)。累计 diff +137/−39（`settings_view.rs` 单文件，工作副本 2026-09-27 三批完成、未拆批提交）。
- **真机走查（待做）**：V-1~V-7（亮/暗 × 中/英四象限；含优化前后并排截图留档）。

### 安全（低危收尾：S-07 / S-08 / S-09 / S-11，审计 11 项闭环，2026-09-24）

- **背景**：审计 11 项的最后 4 项低危全部落地，**审计闭环**；逐项实现与验收见 `docs/security-audit-2026-09-23.md` §5.2–§5.5。
- **S-07（`host/set_clipboard` 静默改写剪贴板）**：`dd-gui/app/host_actions.rs` —— ① **1 MiB 上限拒绝式前置**（纯函数 `clipboard_text_allowed`，按 UTF-8 字节计；超限 warn + toast，**不截断**——半截账号/地址比不写入更危险）；② 成功路径 `debug!` 升 `info!`（含扩展 id 与字节数，可溯源）；③ 写入后 2 s 轻量 toast「扩展 {id} 已写入剪贴板」（覆盖用户复制中的账号/地址前可见）。「可配置关闭」开关与 S-06 同口径刻意未做。
- **S-08（`explorer /select` 原始命令行仅拦 `"`）**：`dd-ext/src/bin/search.rs` `valid_reveal_path` 收紧为四条——空串 / 双引号 / **控制字符**（含 `\n` `\r` `\t` 与 0x7F）/ **`%` 与 `^`**（cmd 层展开与转义歧义字符）；拒绝发生在 spawn 前（`resolve_path_action` 前置门控回「路径已失效，请重新搜索」Toast，`spawn_reveal` 内同判据纵深防御，零副作用）。合法路径（空格/中文/UNC）不误伤。更彻底的 `SHOpenFolderAndSelectItems` COM 路线列为后续可选项未实施。
- **S-09（`FrozenCache` 文件名碰撞 → 跨扩展桩覆盖）**：`dd-host/src/cache.rs` —— 文件名主干改 `{sanitize(ext_id)}-{fnv1a32(ext_id):08x}`（字符折叠与 32 位指纹两道独立映射，**零依赖**纯本地 FNV-1a）；`load` 新名优先、**旧名兼容一个版本**，两路都校验快照内 `ext_id` 归属（伪造桩即使文件名命中也被拦下）；`invalidate_if_version_changed` / `remove` **双前缀清理**（防遗留旧名桩经兼容路径复活）。
- **S-11（`serve_line` 内 `.expect()` panic 面）**：`dd-ext/src/lib.rs` —— 新增 `make_result_checked`（显式收 `Result`）替换 initialize / top_level / fallback / get_command / get_items **5 处** `.expect("序列化 X")`，失败回 **`-32603 Internal error`** + 日志；`invoke` 结果序列化失败回 `-32603` 并**跳过全部副作用**；信封分支 `to_error_response().expect` 改 match（`None` 记日志不 panic）。`serve_line` 的「不 panic」承诺恢复完整，**无需 `#[cfg(test)]` 钩子**（helper 可直接注入 `Err`）。
- **回归测试（+5）**：`clipboard_text_allows_normal_and_boundary_sizes` / `clipboard_text_rejects_oversize`（10 万汉字放行、40 万汉字拒绝——验证按字节计）/ `frozen_collision_ids_do_not_overwrite`（`a.b` 与 `a_b` 互不覆盖，旧实现必失败）/ `frozen_legacy_name_compat_with_ext_id_check`（兼容读回 + 伪造拒绝）/ `serialization_failure_returns_internal_error_not_panic`。S-08 为扩展既有用例断言（换行/制表/DEL/%/^ 全拒）。
- **验证**：`dd-host --lib` **65 passed**（63+2）；`dd-ext --lib` **105 passed** + 1 既有机器绑定失败；`dd-gui --lib` **228 passed** + 6 failed（全部 `Os error 231` 环境批，失败名单无本批新测试）；`dd-ext-search --bin reveal` **1 passed**；`rustfmt --check` 5 个改动文件无差异；`clippy --workspace --all-targets` **无新增告警**（仅剩既有 `search.rs:614` 工具链误报）。测试基线 508 → **513**。
- **真机走查（待做）**：Calc 复制后出现剪贴板来源提示（S-07）；文件搜索「显示所在目录」定位正确（S-08）。

### 安全（S-05 扩展信任门禁：清单无完整性校验与信任分级，2026-09-24）

- **背景**：`extensions.d/*.json` 里任何通过清单九条规则的扩展都会被**静默拉起**（任意 `command` + 任意 `entry.env` + 任意 `cwd`），宿主既不校验来源与哈希、也不征求用户同意，设置页也看不出哪个是随包首方。方案与逐条验收见 `docs/security-audit-2026-09-23.md` §4.4.1–§4.4.3。
- **判据（选型时修掉一处漏洞）**：原设想「`com.ddrun.*` 自动信任」只按 **id 前缀**判定，而 id 是清单作者自填 → `com.ddrun.evil` 即可白拿信任。实际判据 = 「**来源 ∈ 随包 sidecar 目录**（`<宿主 exe 目录>\extensions.d\`）**AND** **id ∈ 首方白名单**」（当前仅 `com.ddrun.filesearch`，锚定既有 `owned_sidecar_name_key`）；用户数据目录里的一律需首次批准。
- **新增**：`crates/dd-host/src/trust.rs` —— 台账（`%APPDATA%\dd-run\trust.json`，宿主私有文件、非契约）、`assess()` 判定（5 条短路规则，末条 **fail-closed**）、SHA-256（Windows CNG `BCryptHashData`，**64 KiB 分块流式**，不为哈希把 exe 读进内存）。`dd-host` 新增 `chrono`（`decided_at` RFC3339；同版本已在依赖树内，`Cargo.lock` 零新增）与 `[target.'cfg(windows)'.dependencies] windows-sys`（`Win32_Security_Cryptography`）。
- **接线**：`aggregator` 扫描结果携带**来源**，`load_extension_sources` 返回具名结构（含判定表 + 台账状态）；`active` 过滤加**信任维度**（与既有「停用集」同一手法：`exts` 保留全集供设置页审批）；**`spawn_and_initialize_with_info` 加门禁**——它是所有子进程 spawn 的唯一入口（含 GUI 桩复热），无旁路。
- **设置页**：扩展行新增**来源标签**（内置 / 随包 / 用户安装）、**信任状态**（待批准 / 已阻止）、行内「允许 / 阻止」按钮、清单与 exe 路径 tooltip（含「打开所在目录」）、**同 id 撞车告警**（用户目录清单顶掉随包版本时）；卡片头显示待批准数与台账损坏提示。面板页脚在**空位**（无选中项时）提示待批准数，并在首次出现时给一次 toast（不打断）。
- **行为变更（用户可见）**：**用户目录里的第三方扩展首次不会被加载**，需在「设置 › 扩展」点一次「允许」；批准绑定到 `manifest` 与 `exe` 的哈希，**内容改变即回到待批准**；删掉/损坏台账 → 全部回到待批准（fail-closed）。随包 `dd-ext-search`（文件搜索）**零摩擦**，不受影响。
- **回归测试（+19）**：`dd-host::trust` 14 条（内置/首方短路、用户目录不得靠前缀冒充、`allow`+哈希一致放行、exe/清单变化即失效、`deny` 阻止且可撤销、损坏与版本不识别回落空台账、文件不可读仍待批准、**NIST 已知向量**（`""`/`"abc"`/1,000,000×`a`）、文件与内存哈希一致）；`aggregator` 4 条（`is_trusted` fail-closed、`pending_count` 只计待批准、spawn 门禁拒绝未获信任者、首方 sidecar 仍放行）；`settings_view` 1 条（来源与信任状态驱动 UI）。
- **验证**：`dd-host --lib` **63 passed**（+14）；新增单测全绿；`rustfmt --check` 改动文件无差异；`clippy --workspace --all-targets` **无新增告警**。**A11 哈希开销实测**（`.workbuddy/tmp/trust-probe/`，真实 sidecar `dd-ext-search.exe` 836,608 B）：哈希 **0.874 ms**、台账读 **0.068 ms**、已批准判定（两次哈希）**1.068 ms**、首方与待批准走短路 **≈0 ms** —— 判据 <10 ms。⚠️ 全仓 `cargo test --workspace --no-fail-fast` 本轮 = 487 passed / 21 failed，其中 **20 条为已知 `Os error 231`（`ERROR_PIPE_BUSY`）环境批**（piped-stdio spawn，本日已用零仓库代码探针定性）、另 1 条既有机器绑定用例；**失败名单中无本项新测试**，环境自愈后预期 **507 passed / 1 failed**。
- **已声明的缺口**：非 Windows **不做门禁**（保持旧行为 + 一次 warn）——P4 为 Windows 优先，若在该平台 fail-closed 会让任何扩展都无法使用。**定性提醒**：本项实现的是「**用户同意 + 变更检测**」，**不是防篡改**（能写 `extensions.d` 的攻击者同样能写 `trust.json`）；真正防篡改需扩展签名，明确不在本项范围。零协议 / 零清单字段变更。

### 安全（中危批量修复：S-02 / S-03 / S-04 / S-06 + S-10，2026-09-23）

- **背景**：承接 S-01（高危命令注入）之后的中危批次，覆盖「不受信输入打挂宿主」「能力语义越界」「无门槛执行」三类边界；逐项方案与验收见 `docs/security-audit-2026-09-23.md` §4–§5。
- **S-02（NDJSON 解码器无界缓冲 → 内存耗尽）**：`dd-protocol/src/framing.rs` —— `Decoder` 新增 `poisoned` 位，**未终止残留同样受 `max` 约束**；超限即 `reset()` + 毒化并**只报一次** `TooLarge`（后续字节丢弃，避免流错位与帧洪泛）；新增 `is_poisoned()` / `reset()`。PoC 复跑：4 MiB 无换行由 `buffered = 4 MiB`（UNBOUNDED）反转为 `buffered = 0 / frames = 1`（bounded）。
- **S-03（`host/open_url` 无 scheme 白名单）**：`dd-gui/src/platform.rs` 新增 `is_allowed_open_url`（**http / https / file**，大小写不敏感 + 拒绝空串/控制字符/裸引号），`app/host_actions.rs` 前置拦截（`warn` + toast，不静默），`file://` 打开前记 `info`（扩展 id + 路径）以溯源；`docs/protocol.md` §7.4 增「宿主执行策略（实现侧，非契约）」注。⚠️ **方案有意收窄**：初版为"只放行 http(s)"，核对消费者后发现**文件搜索「打开」依赖 `file://`**，故保留该 scheme 以免功能回归；被消除的是 `ms-msdt:` / `search-ms:` / `vbscript:` / `javascript:` / `data:` / `steam://` 等**无合法消费者**的静默处理器唤起。⚠️ **另修正一处本批实施时引入的二次缺陷**：前缀比较原用 `u[..p.len()]`（按**字节长度切 `&str`**），遇多字节开头的合法入参（如 `C:\中文\文件.txt`）会因落在字符边界内**直接 panic** → 改为 `as_bytes()` 比较，并加护栏 `open_url_handles_non_ascii_without_panicking`（取证与教训见审计文档 §4.2.1 注）。
- **S-04（图标读盘/解码无上限）**：`dd-gui/src/ui/icons.rs` —— 新增 `read_icon_limited`（**先 `metadata` 后读内容**，> 512 KB 直接拒，拒绝发生在读盘前）与显式 `image::Limits`（宽高 ≤ 512、`max_alloc` 16 MiB）。同时**修正原报告的事实错误**：`image` 默认已有 512 MiB 分配上限，缺失的是尺寸上限与读盘前校验。
- **S-06（Shell 兜底无门槛执行）**：`dd-ext/src/builtins/shell.rs` —— 新增 `is_dangerous_command`（12 个危险命令名 + `reg delete` / `net user` 子命令组合，**比命令名而非子串**、去路径去扩展名），命中且未确认 → 回 `Confirm{is_critical:true}`（描述含将被执行的原命令），确认后按 §8.3 重发放行；`where del` / `echo format` / `deleted_files.bat` / `reg query` / `net view` **不误伤**。
- **S-10（`entry.env` 可覆盖宿主关键环境变量）**：`dd-host/src/process.rs` —— 新增 `PROTECTED_ENV_KEYS`（18 个：`PATH` / `COMSPEC` / `SYSTEMROOT` / `USERPROFILE` / `TEMP` …）与 `filter_env_overrides`；`spawn` 只注入非受保护键并 `warn` 被拒键；`dd-host` 新增 `log = "0.4"`（facade，零传递依赖）以让拒绝可观测。业务变量 `DDRUN_LANG` / `DD_WEBSEARCH_ENGINES` 不受影响。
- **回归测试（+13）**：`unterminated_stream_is_bounded_and_reports_once` / `poisoned_decoder_discards_until_reset` / `residual_limit_boundary_is_exclusive`（S-02）；`open_url_allows_http_https_file_only` / `open_url_rejects_untrusted_schemes` / `open_url_handles_non_ascii_without_panicking`（S-03）；`read_icon_limited_rejects_oversize_without_reading` / `decode_icon_image_accepts_up_to_dimension_limit` / `decode_icon_image_rejects_over_dimension`（S-04）；`dangerous_command_detection_hits` / `dangerous_command_detection_does_not_over_block`（S-06）；`filter_env_overrides_blocks_protected_keys_case_insensitively` / `filter_env_overrides_keeps_business_vars`（S-10）。
- **验证**：靶向单测全绿 —— `dd-protocol` **30 passed**、`dd-host --lib` **49 passed**、`dd-gui --lib`（`icons`/`open_url`）**11 passed**、`dd-ext --lib`（`win_launch`/`dangerous_command`）**7 passed**；`rustfmt --check` 改动文件无差异；全仓 `cargo test --workspace --no-fail-fast` = **488 passed / 1 failed**（唯一失败为既有**机器绑定**用例 `steam_installed_shown_uninstalled_filtered_root_lnk_shown`，非回归）。
- **未做（待选型）**：S-05 扩展清单信任模型（信任台账 + 设置页审批），三方案 T1/T2/T3 见审计文档 §4.4；S-07/S-08/S-09/S-11 仍待做。零协议/清单字段变更（仅 §7.4 增实现侧策略注）。

### 安全（S-01 命令注入修复：`cmd /C start` 参数引号错配，P0，2026-09-23）

- **背景**：全量代码安全审计（79 个 `.rs` / 35,975 行）确认 11 项缺陷（1 高 / 5 中 / 5 低），清单与修复方案见 `docs/security-audit-2026-09-23.md`；本批处置其中唯一的**高危**项。
- **症状**：`.lnk` / 协议 URL 的启动路径为 `cmd.exe /C start "" <不受信串>`——Rust 的 Windows 参数引用规则（含空格才加引号、参数内 `"` 转义为 `\"`）与 **cmd.exe 不认 `\` 转义**的解析规则错配，参数内 `&` 越界成为命令分隔符。PoC 实测两种形态均注入成功（含 `&` 无空格；含空格 + 裸 `"`）；`.url` 里一行 `URL=https://a/"&calc&"`（可通过既有协议前缀白名单）即可让宿主执行任意命令。
- **修复**（`crates/dd-ext`，**换汇点而非加转义**）：新增 `src/win_launch.rs`——`shell_open` = `ShellExecuteW(verb="open")`（`lpFile` 不参与命令行解析，故注入面消失），配纯函数 `target_is_safe`；`builtins/apps.rs` 的 `launch_shortcut` / `launch_url` 改调它，**不再经 `cmd.exe`**；`lib.rs` 注册模块。`Launch::AppsFolder` 臂（`explorer.exe shell:AppsFolder\…`）保持不动——explorer 不是命令解释器，且 `parsing` 已被 `\` / `/` 过滤。
- **与初版方案的一处有意偏离**：`target_is_safe` 只拒「空串 / 控制字符 / 裸双引号」，**不拒** `&` `%` `|`（它们在 Windows 文件名与 URL 中合法，如 `…\Start Menu\Programs\Foo & Bar\app.lnk`，拒绝会造成既有应用无法启动的功能回归；而本修法下没有解释器可注入，故对安全零增益）。
- **回归测试**（`win_launch.rs`，+3）：`accepts_legitimate_targets`（8 例合法目标放行，防功能回归）/ `rejects_impossible_targets`（含两种注入形态）/ `launch_path_does_not_use_cmd`（源码断言 `apps.rs` 不含 `Command::new("cmd.exe")` 与 `"start"`——把这一类缺陷钉死，防日后被「便捷启动」改回）。
- **验证**：`cargo build -p dd-ext` exit 0；`cargo test -p dd-ext --lib -- win_launch` **3 passed**；全量 `cargo test --workspace --no-fail-fast` **475 passed / 1 failed**（唯一失败 `steam_installed_shown_uninstalled_filtered_root_lnk_shown` 为**机器绑定**用例，断言本机装有 Flowframes；基线同为 1 failed，非本次回归）；`cargo build -p dd-gui --release` 重链宿主 exe。真机验证（`.lnk` / `.url` 条目启动行为不变）待确认。

### 修复（退格键无法删除输入内容，B2 回归，2026-09-23）

- **症状**：默认设置下，文件搜索页及其它嵌套页的输入框**能键入字符，但 Backspace 无法删除已输入内容**（光标不动、字符删不掉）。
- **根因**：`keys.rs::handle_keys` 在函数开头 `consume_key(Modifiers::NONE, Key::Backspace)` **无条件**移除退格事件；而 `backspace_go_back` **默认 false**（B2，2026-09-20，默认 = 既有行为）。被 `consume_key` 移除的事件 TextEdit 收不到 → 默认配置下退格被白白吞掉。**症状选择性**（能打字、删不掉）的机制：字符输入走 `Event::Text`（不受 `consume_key` 影响），退格走 `Key` 事件（被消费）。
- **修复**（`crates/dd-gui/src/app/keys.rs`）：退格消费改为**按需**——初始按键批次不再消费 Backspace（仅保留 Esc / ↑ / ↓ / Enter / Tab / Shift+Tab）；仅当 `backspace_go_back == true` **且** 嵌套页 **且** 搜索框为空三者同真时，才 `consume_key(Backspace)` 并 `go_back_focused()`；其余情况一律不消费，退格留给输入框删字。
- **回归测试**（`crates/dd-gui/src/app/mod.rs`，+3）：`backspace_not_consumed_when_go_back_disabled` / `backspace_go_back_enabled_nested_empty_pops` / `backspace_go_back_enabled_but_query_nonempty_keeps_editing`（配 `key_still_in_queue` 辅助）。
- **验证**：`cargo test -p dd-gui backspace` **4 passed / 0 failed**；全量 `cargo test --workspace --no-fail-fast` **472 passed / 1 failed**（唯一失败 `steam_installed_shown_uninstalled_filtered_root_lnk_shown` 为本机**机器绑定**例，断言本机存在 Flowframes.lnk + Steam 游戏，非作者机必失败，**非回归**，见 `implementation.md` §3.1）；`cargo build -p dd-gui`（debug）+ `tools/package.sh`（release + `dist/`）重链产物。

### 新增（设置 / 个性化 / 材料样式：参照 PowerToys CmdPal，B1–B4，2026-09-20）

- **方案**（设计稿先行，零代码起步）：`docs/settings-personalization-plan.md` v1.1——CmdPal 取证（`SettingsModel` / `BackdropStyles` / `AppearancePage`）→ dd-run 差距分析 → M/P/B 三方面优化 + T1–T10 任务清单。**四批一次落地 T1–T8；T9（背景图）/T10 未做**。
- **材料（B1）**：① **注册表化**——`theme::backdrop_config` 单一来源（`theme::BackdropStyleConfig`：浓淡上限 / 可调位 / 行填充分档），设置页 pill·描述·DWM 映射（`From<Backdrop>`）全部派生，加档不再各处 `match`；② **新增云母 Alt**（`DWMSBT_TABBEDWINDOW`，材质四选：无 / 云母 / 云母 Alt / 亚克力），旧系统应用失败走既有回退链；③ 不透明度滑杆文案澄清为「材质色层的浓淡强度（非窗口透明度）」。
- **行为（B2/B4）**：**Esc 键行为**三档（返回上一级（默认）/ 先清搜索再返回 / 始终隐藏，纯决策函数 + 决策矩阵单测）；**退格键返回**（默认关；嵌套页 + 空搜索框时返回）；**单击激活开关**（默认开 = 既有行为；关 = 单击选中、双击执行）；**界面动效开关**（默认开；关档聚焦下划线等过渡直出终态）。
- **个性化（B3）**：**着色模式**三档（系统强调色（默认 = 既有行为）/ 无 / 自定义色 + 强度 0–100），只作用于**材质浓淡层基色**——Fluent 主题 token 与边框强调色不受影响；自定义档在材质卡条件显隐色块与强度滑杆（强度松手落盘、自定义色在指针松开时落盘）。
- **恢复默认外观（B2/T5）**：外观栏底部两步确认按钮（点击 →「确认重置」，5s 未再点自动撤销），重置主题 / 材质 / 浓淡 / 圆角 / 边框 / 密度，**不含**热键、语言、引擎、扩展、自启与面板尺寸。
- **兼容性**：所有新设置默认值 = 现有行为，旧 `config.json` 缺失字段一律回落默认（防御性解析），**零迁移、零行为变更**；新增枚举值（`mica_alt` / `esc_behavior` / `colorization`）只增不改。
- **验证**：`cargo test --workspace` **470 passed**（465 基线 + 5）；`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `release` 构建 exit 0。

### 新增（GUI 端到端首屏计时插桩：A-33-05 感知指标「输入→首屏 ≤200ms」，2026-09-19）

- **背景**：验收报告 §5 #4 此前只有扩展侧往返（A-33-05 的 IPC p50/p95），「输入到首屏 ≤200ms」的 GUI 段无数据——本批在宿主侧补齐插桩，**真机采样待做**。
- **口径**：`input→paint` 三段分解 = ①去抖/进页等待（页内 query 最后一次输入变化 → `get_items` 实际分派）+ ②`get_items` 往返（分派 → 非过期落地）+ ③渲染（落地 → 本帧 `draw_panel` 完成）。**上界**：不含 egui 呈现/垂直同步。
- **实现**：新增 `dd-gui/src/app/e2e.rs`（`E2eSample` 三段分解纯函数 + `e2e_report` 帧尾结算 + 3 单测）；接线 `app/mod.rs`（3 计时字段 + `draw_panel` 后结算，可见/隐藏帧两处）、`app/page.rs`（分派点 / 非过期落地建样本 / 失败与离页清理 / 带词进页起点）、`ui/panel.rs`（页内 query 变化 = 起点）。埋点**常驻 `log::debug!`**（不加 `debug_assertions` 守卫——感知指标属 release 真机口径；常态成本可忽略）。
- **采样判读（可复现）**：`dist\dd-run-0.1.1.exe 2> gui.log` 跑会话（`Ctrl+F` 进文件搜索页输入若干查询）→ `python tools/gui_e2e_parse.py gui.log` → min/p50/p95/max + 门禁判定（p95 < 200 → PASS）。
- **验证**：`cargo test --workspace` **465 passed**（462 + 3）/ `fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / release 构建 exit 0。

### 优化（文件搜索图标：sidecar 体积回到预算内——零依赖 PNG 编码器，E2，2026-09-19）

- **决策**：采纳下条 E1 记录中的「E2 选项 ②」——自写零依赖 PNG 编码器，`image[png]` 由 `[dependencies]` 移至 `[dev-dependencies]`（仅单测解码 oracle，不进交付产物）；未采纳「修订预算」。
- **实现**：新增 `crates/dd-ext/src/png.rs`（`encode_rgba`：PNG 魔数 + IHDR/IDAT/IEND + 块 CRC-32 + zlib 流[固定 Huffman deflate + 贪心 LZ77，窗口 32 KiB] + Adler-32；行滤波 None/Sub/Up 三档取产物最小）；`shell_icon.rs` 的 `hicon_to_png` / `bitmap_to_png` 编码尾部切换（Shell 抽取 / alpha 修正 / 缓存零改动，apps 经共享模块继承）。刻意不做动态 Huffman / 多块流 / 隔行 / 调色板（图标小图无可感知收益）。
- **正确性验证（非自证）**：单测用 `image`（dev-dependency）解码自产 PNG 并**逐字节比对像素**（平坦 / 渐变 / 圆图标 / 噪声 / 1×1 / 1×300 六组往返）+ 块结构走查 + Adler/CRC 已知向量；另以 **Python zlib / binascii**（与 Rust 实现无关）复验真机产物：file-icons **51/51**、apps-icons **88/88** 全过；release `dd-ext-apps.exe` 冷缓存回归 88 项 path 图标齐全。
- **实测**：`dd-ext-search.exe` **912,384 → 831,488 B（−80,896 B）**，相对改造前基线 769,536 B 增量 **61,952 B ≤ 65,536 B（A-IC-06 转 PASS，余量 3,584 B 压线）**；宿主 `dd-run.exe` 同步 **−12,800 B**（in-process 侧编码链剥除）；`cargo test --workspace` **462 passed**（452 + 10）；fmt 无差异 / clippy 0 告警 / release exit 0；`tools/icon_acceptance.py --cold` 两轮其余判定全 PASS。
- **如实记档**：A-IC-02b（扩展名档首抽）两轮 50.29 / 66.84 ms（判据 ≤40、观察线 60）——同轮缓存命中亦由 0.10 抬至 0.14–0.21 ms，且编码单张 32×32 实测仅 0.6188 ms（release 探针）→ 判为**机器负载敏感压线项**（E1 批次即压线 40.29 ms），非本批回归；建议空闲机器复测定标。
- **dist（同日已闭环）**：实例关闭后 `bash tools/package.sh` exit 0——`dd-run-0.1.1.exe` **8,780,800 B**、`dd-ext-search.exe` **831,488 B**；分发级冒烟全过（conformance 内置/示例各 9 步、GUI 5s 存活、sidecar 通道 `ipc`、zip 成员校验）。

### 修复（文件搜索图标：按真实路径档首抽 191–704 ms → 分层异步 + 自动刷新，E1，2026-09-19）

- **根因**：`get_items` 在同步路径上逐条 `SHGetFileInfoW` 抽图，且 `.exe`/`.lnk`/`.msi`/`.url` 按**真实路径**分键 → 键数随结果条数线性增长（30 条 = 30 次抽取 × 6–25 ms，冷缓存实测 191–704 ms）。在 `A-IC-02` 的「首次 ≤ 40 ms」预算下，**单键最坏成本已占 62%** —— 同步路径几乎没有优化空间（抽 1 个 exe 就吃掉大半预算）。
- **修复（分层异步 + 自动刷新）**：① **按键分层** —— `path:` 键**一律不进同步路径**（立即返回类别 glyph + 投递后台），`ext:`/`dir`/`noext` 键同键复用留在同步路径，并受 `ICON_SYNC_BUDGET = 15 ms` 时间预算兜底；② **后台 worker** 4 线程（首次入队惰性启动、按键去重、取任务时才持队列锁）抽图并写落盘 + 进程内缓存；③ 补齐后按 `ICON_NOTIFY_THROTTLE = 200 ms` 节流发 §7.1 `items_changed(files.results)` → 宿主**既有**链路（命中当前页 → 100 ms 合并窗口 → 重拉）自动把 glyph 换成真实图标。**协议零改动、宿主零改动**；`dd-ext/src/lib.rs` 新增跨线程通知器（共享 stdout 加锁，`write_all + flush` 同临界区，消息不交错；未安装时 no-op）。
- **实测**（`tools/icon_acceptance.py --cold`，release sidecar 直驱）：按真实路径档**同步** `icon_ms` **191–704 ms → 0.32 ms** ✅；后台补齐后重查 **`path=30 / glyph=0`，0.09 ms** ✅；`.lnk` 批补齐后 `path=17 / glyph=13`（不可访问路径回落仍生效）✅；按扩展名首抽 32.23 ms、缓存命中 0.10 ms、10 次查询全 `kind=results` ✅。
- **行为变更**：首次查询中的 exe/lnk/msi/url 行**先显示类别图标**，约 0.2–0.8 s 后自动替换为真实图标（用户无需再输入）。
- **代价与新证据**：`dd-ext-search.exe` 876,544 → **912,384 B（+35,840 B）**；**E2 探针**（临时剥离 `image` 调用后构建）实测 **876,544 → 779,776 B（−96,768 B）** → 若实施 E2 选项 ②（自写零依赖 PNG 编码器 + `image` 转 dev-dependency），sidecar ≈ **818 KB**、增量 ≈ **48 KB**，回到 64 KB 预算内。**E2 已于同日实施（见上条优化记录）**。
- **验证**：`cargo test --workspace` **452 passed / 0 failed**（447 基线 + 5 新增单测）；`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo build --release` **exit 0**；`tools/icon_acceptance.py --cold` 8 项判定中 7 项 PASS（仅 A-IC-06 体积项因 E2 未实施而 FAIL）。

### 文档（文件搜索用户指南解除 `es.exe` 前置表述，2026-09-19）

- **口径变更（零代码改动）**：`docs/search.md` 由「`es.exe` 仍受支持 / 仍建议安装 / 已发布版本仍依赖」改为 —— **唯一必需依赖是 Everything 在运行**；`es.exe` 降为**可选回落通道**（只在 IPC 直连不可用时出场，未安装则给可读提示、不静默失败）。改动面：§1 标题「前置条件」→「准备事项」、步骤 1 标「必需」/步骤 2 标「可选（推荐）」、步骤 4 验证改可选、配置项注、故障排查两行、已知边界引导项、§7 升级说明、§8 FAQ（原「还需要安装 `es.exe` 吗？**需要（建议）**」→「**不必须（可选，推荐）**」）。
- **决策依据**：真机验收两轮完成（A-33-05 / A-33-06 / A-33-07 / A-33-10 **通过**，回落静默失败缺陷已修）→ 原「待决策项」（`search-file.md` §1 / `INDEX.md` §5）**定稿解除**；**A-33-08 仍为唯一红线**（Win10 / Everything 1.5 / 提升 / 命名实例未覆盖），故文档以「直连不可用时回落」表述承接，**不作全覆盖承诺**。
- **同步回写**：`README.md` / `README.zh-CN.md` 文档表、`search-file.md`（v3.8：§1 口径 + §8 版本演进行）、`INDEX.md`（v1.15：§5 行 + 两处行数重算）、验收报告 §1 与 `doc-audit` / `doc-code-diff` 的红线注记（**就地追加「已解除」注、历史结论保留**）、`implementation.md` §2 + §7。

### 新增（文件搜索：面板内 `Ctrl+F` 直达 + 查询结果显示真实图标，2026-09-19）

- **`Ctrl+F` 一键直达**：面板内**任意页**按 `Ctrl+F` 进入文件搜索页（此前须输入 `f ` 前缀、点顶层入口项或兜底模板）。已在文件搜索页时**幂等**（仅聚焦、输入保留）；其他位置先回 Root 再进页（**栈深恒为 2**）；仅 Root 的查询带入（trim 后非空、**原样**带入——前缀剥离逻辑已于同日随 `f ` 前缀直达一起移除，见下条变更），设置页 / 其他嵌套页进空查询；扩展未加载或被禁用 → Error Toast，不产生空页。键位取证：`Ctrl+F` 此前**全仓无绑定**（面板内仅 Esc / ↑↓ / Tab / Shift+Tab / Enter / `Ctrl+,` / Shift+F10）；确认对话框与右键菜单活跃时不穿透，热键捕获模式下仍可作为自定义全局热键候选。根页 placeholder 追加 `Ctrl+F 搜文件` 提示。
- **文件结果图标 → Windows Shell 真实图标**：由「按扩展名的 12 类 Segoe glyph」改为 `dd_ext::shell_icon` 抽 32×32 Shell 图标 → PNG 落盘缓存（`%APPDATA%\dd-run\cache\file-icons\`），经协议**零改动**的 `IconKind::Path` 回传（复用宿主既有 path 图标链路）。缓存键分级：目录 → `dir`；`.exe`/`.lnk`/`.msi`/`.url` → 真实路径（每个程序自身图标）；其余 → `ext:<小写扩展名>`。抽取 / 编码 / 写盘任一失败 → 回落类别 glyph（零退化）；非 Windows 目标编译通过并恒走 glyph。
- **图标管线去重**：`HICON/HBITMAP → PNG`（掩码 alpha、掩码行 DWORD 对齐、`GetDIBits` 负高 top-down）与缓存三函数自 `builtins/apps.rs` **上移**为新模块 `dd-ext/src/shell_icon.rs`，应用图标与文件图标共用一份实现；apps 行为不变（仅可见性与位置）。
- **可观测性**：`get_items` 计时日志新增 `icon=` 段（结果项构造 / 取图耗时；`score_ms` 自本次起不含构造期）；`tools/search_acceptance.py` 的计时解析同步支持（`icon=` 为**可选**段，旧日志仍可解析）。
- **测试竞态修复（既有缺陷，本批暴露）**：`search.rs::path_index_evicts_beyond_capacity` 按 **id 阈值**清空 `PATH_INDEX`，会连带清掉并行用例的 pid（`invoke_copy…` 因此拿到 0 条副作用）；此前偶发，接入真实图标后**并行必现**（实测 5/5 失败、跳过该用例 3/3 通过、单线程 48/48 通过）。修法：8 个读回索引的用例统一持测试内串行锁（生产逻辑零改动）。
- **验证**：`fmt` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **448 passed**（基线 436 + 本批 12）；`cargo build --release` **exit 0**；真机直驱 release sidecar（`--cold` 冷缓存口径）实测 `icon_ms`：缓存命中 **0.09–0.44 ms**（30 条）、按扩展名首抽 **冷启首档 40.29 ms（压线）/ 其余 9.3–10.8 ms**、**按真实路径首抽 191–704 ms（30 新键，随文件构成波动）**；`file-icons/` 39–88 文件 / 34–63 KB。
- **验收工具（可复现）**：新增 `tools/icon_acceptance.py` —— A-IC-01/02a/02b/02c/04/06 **六项自动判定** + JSON 证据（`target/acceptance/icon-acceptance.json`）+ 退出码（0 全过 / 1 有 FAIL / 2 环境未就绪）。**`--cold` 是「首抽」判据的必要口径**：热缓存会把 `ext:exe` 从 703.8 ms 降到 98.7 ms，从而把未达标误读成达标。
- **两处未达标（如实记档，处置待决策，详见 `docs/search-file.md` §10.4）**：① 按真实路径档首次抽取 **703.8 ms**（预算 ≤ 40 ms）；② `dd-ext-search.exe` **769,536 → 876,544 B（+104.5 KB，预算 ≤ 64 KB）**，宿主 `dd-run.exe` 仅 **+3,072 B**。

### 变更（文件搜索：移除 `f ` 前缀直达，2026-09-19）

- **移除**：根页输入 `f ` + 空格**自动进文件搜索页**的便捷触发整条删除 —— `FILE_SEARCH_PREFIX` 常量、纯函数 `file_search_drill_target()`、`PaletteApp::maybe_drill_file_search()` 及其在 `ui()` 的每帧调用、状态 `file_drill` / `file_drill_armed`、`poll_page` 的落地回填分支，全部清掉（**无死代码残留**）；`Ctrl+F` 的查询带入不再剥离前缀。**`f ` 自此是普通搜索词**（`f report` = 搜同时含 `f` 与 `report` 的项）。
- **动机**：前缀会**劫持根视图的字面查询**（想搜字面 `f report` 也会被强制进页），而 v3.6 的 `Ctrl+F` 已提供更明确的一键直达 → 面板内直达入口收敛为一条，进入方式由四条变为**三条**（顶层入口项 / 兜底模板 / `Ctrl+F`）。
- **未改动**：`open_page` 统一回填与 `poll_page` 的 v3.3 query 保留逻辑（`Ctrl+F` 带词进页不变）、「已在文件搜索页 → 幂等」判定、栈深恒 2、扩展不可用 Toast、顶层入口项、兜底模板、扩展侧与协议（零改动）；其余键位与页面栈语义不变。
- **验证**：`fmt --all -- --check` 无差异 / `clippy --workspace --all-targets` **0 告警** / `cargo test --workspace` **447 passed / 0 failed**（基线 448 − 删除 1 个前缀专属用例 `file_drill_prefix_still_works`；`ctrl_f_query_source_rules` 改锚定「`f ` 开头原样带入」）。

### 变更（应用列表：剔除 Windows SDK / 驱动包装的调试诊断工具，2026-09-19）

- **现象**：默认「应用」列表里混入 `Application Verifier (WOW)` / `Application Verifier (X64)`、`Windows App Cert Kit`、`Debuggable Package Manager (1)`、`Developer PowerShell for VS 2022`（含 `(1)`）、`AMD Bug Report Tool`、`Windows Software Development Kit` —— 这些由 Windows SDK / 驱动包 / VS 安装器写进开始菜单，普通用户不会启动，出现在「应用」列表里只会稀释检索质量。
- **修复**：新增 `DEV_TOOL_TITLE_KEYWORDS`（与 `JUNK_TITLE_KEYWORDS` 同走 `is_junk_title` 判定，两类规则各有出处、便于日后增删），全部采用**精确短语** —— `application verifier` / `app cert kit` / `debuggable package manager` / `developer powershell` / `bug report` / `software development kit`，避免 `sdk` / `debug` 这类宽词误杀。
- **验证**：真机 **122** 条样本实测**命中 8 条、误杀 0 条**（`Visual Studio 2022` / `Visual Studio Code` / `Visual Studio Installer` / `PowerShell 7 (x64)` / `Windows 工具` / `Git Bash` 均保留）；新增单测 `dev_tool_titles_are_filtered`（8 条正例 + 6 条反例）；三关 `fmt` 无差异 / `clippy` 0 告警 / `cargo test --workspace` **436 passed**。

### 修复（模糊排序字段分层：标题命中被副标题命中压过，2026-09-19）

- **现象**：输入 `steam` 时，标题精确匹配的「Steam」排在第 5 位 —— `Dead Cells`、`Family Crush` 等 Steam 游戏都在它前面，看起来像「没有按匹配度排序」。
- **根因**：`dd-gui::fuzzy::FuzzyMatcher::score` 把 `title` / `subtitle` / `section` / `pinyin` / `tags` **混在同一池里取最高分**，而 nucleo 对**前缀连续匹配不区分 haystack 长度** —— 实测 `Steam`（标题精确，`140` 分）与 `steam://rungameid/588650`（Steam 游戏副标题，`140` 分）**完全同分**。`recompute_visible` 按分降序**稳定排序**，同分遂回落到扩展侧字母序（`apps.rs` 按标题小写排序），把标题命中项挤到后面。**排序机制本身正常**，问题在「字段无权重」。
- **修复**：**字段分层** —— `score()` 返回 `(层 << 32) | nucleo 分`：**层 1 = 标题层**（`title` 与其派生拼音索引 `pinyin`），**层 0 = 附属层**（`subtitle` / `section` / `tags`）。层不同绝不互比，层内仍按匹配质量排，同层同分继续由稳定排序保持原序。改后 `Steam` 与 `Steam Support Center`（均标题命中）恒排在所有 `steam://` 游戏之前，二者同分且字母序在前 → **`Steam` 落到第 1**。
- **验证**：新增 2 个单测（`fuzzy::tests::title_hit_outranks_subtitle_hit`、`state::tests::query_ranks_title_hit_above_subtitle_hit`）锁定「标题命中恒先于副标题命中」；三关 `fmt` 无差异 / `clippy` 0 告警 / `cargo test --workspace` **435 passed**（基线 433 + 本批 2）。

### 修复（文件搜索回落通道静默失败：`es.exe` 失败被当成「无结果」，2026-09-17）

- **现象**：IPC 不可用且 `es.exe` 也失败时，文件搜索页显示**空列表**（「无匹配结果」），**没有任何提示** —— 用户无从判断是「没搜到」还是「工具坏了」。
- **根因**：`run_es` 把 `es.exe` 的 stderr 丢弃（`Stdio::null()`）且**忽略退出码**，而 `parse_response("")` 被刻意设计为「空输出 = 无匹配结果」→ 二者叠加使「es 失败」与「真的没搜到」**无法区分**。实测失败形态：`es.exe` 以 **`rc=8`** 退出，错误只写在 stderr（`Error 8: Everything IPC not found. Please make sure Everything is running.`）。
- **修复**：新增纯函数 `interpret_es_output(status, stdout, stderr)` 收口判定 —— 仅 `rc=0` 按「无结果」处理（保持既有语义），`rc≠0` 一律转 `Err` 并**保留 es 的 stderr 文案**，由上层落成可读的 `files.error` 项（文案指向安装/运行 Everything 与 `DDRUN_ES_PATH`）；`run_es` 改为 **stdout/stderr 双管道并发消费**（避免写满管道阻塞）并带出退出码。
- **验证**：新增单测 `interpret_es_output_separates_no_result_from_failure`（5 个断言面，含「`rc=8` + 空 stdout 必须报错且带 stderr 文案与退出码」）；三关 `fmt` 无差异 / `clippy` 0 / `cargo test --workspace` **433 passed**。

### 修复（嵌套页标题显示原始 `page_id`，2026-09-17）

- **现象**：进入嵌套页后搜索框 placeholder 显示 **「在「files.results」中筛选…」** —— 把扩展内部的页 id 直接暴露给用户。
- **根因**：`open_page` 把 `page_id` **同时当作页标题**（`PageState::nested(page_id, page_id, …)`），而该标题被渲染进 placeholder。
- **修复**：标题改由**宿主侧信息**提供 —— 点击入口项进页 = **被点击项标题**；`f ` 前缀直达 = 本地化扩展名（`ext.name.filesearch`）；扩展 `GoToPage` 无来源 → 空 → placeholder 回落「筛选命令…」。同步修正 `navigation.rs` 中「标题来自 `PageInfo.title`」的过期注释（协议侧 `PageInfo` 定为**不传递**，见下）。

### 变更（协议两项定稿：`-32002` 与 `PageInfo`，2026-09-17）

- **`-32002 command_not_found`**：改为**终态口径** —— 该码**由扩展 handler 产出**（随指南发布的 Python 示例即产出）、**宿主与内置运行时不产出**（内置回 `ShowToast` 属合法实现选择）；不再是「保留码、待接线」。协议 v1.0 **零改动**（仅注记改写）。
- **`PageInfo`**：明确**维持不传递**（v1.0 预留定义）—— `GetItemsResult` 不携带页元信息，宿主页标题取自宿主侧信息；将来启用须走 §13 `MINOR`。

### 修复（`--conformance` 对 `has_fallback=false` 扩展误报红灯，2026-09-17）

- **现象**：对 `dd-ext-sample`（清单 `has_fallback: false`）跑 `--conformance`，第 `4) fallback` 步**必红** `-32601`，整体自检失败 —— 而该示例**完全合规**。
- **根因**：自检器无条件调用 `fallback_commands`；而 §6.2 的判据是「返回非空 ⟺ `has_fallback`」，扩展在 `has_fallback=false` 时**不必实现**该分发臂（宿主也不会调用）。
- **修复**：`has_fallback=false` 时**跳过**该步并明示原因；`has_fallback=true` 仍校验一致性。内置扩展（`has_fallback=true`）路径无回归，示例扩展自检由「红灯」变为 **9 步全过**。
- **顺带**：`dd-gui` 测试内的 `route_serve_line` 由手工复刻路由改为**委托生产实现** `dd_host::process::route_messages`（单一来源），消除「测试辅助与真实路由不同步」的潜在脆弱点。

### 修复（release 构建阻塞：A3 埋点 cfg 守卫漏配导致分发包无法产出，2026-09-16）

- **症状**：`cargo build --release` 在 `dd-gui` 报 `error[E0425]: cannot find value 'start' in this scope`（`crates/dd-gui/src/state.rs:395`）→ **单文件分发包自 2026-09-13 起无法产出**（`dist/` 长期停留在 `378b89d` 之前的构建，`tools/package.sh` 静默失败在宿主构建步）。
- **根因**：`378b89d`（perf：热路径去重复开销）按要求把 `recompute_visible` 的 A3 计时埋点改为 `#[cfg(debug_assertions)]`（release 免一次 `Instant::now()`/帧），但**只改了 `let start` 定义、漏改使用它的日志调用** —— release（`debug_assertions` 关闭）下该变量不存在。三关（fmt / clippy / test）与 CI 四关**均为 debug profile**，不覆盖 release 编译，故长期未暴露。
- **修复**：为日志调用补上同一 `#[cfg(debug_assertions)]` 守卫（`state.rs` +1 行），即补全 `378b89d` 已声明的设计意图；**行为不变**（该埋点本就仅 debug 构建输出，release 下不产生该次计时与日志）。
- **重新产出**：`bash tools/package.sh` → `dist/dd-run-0.1.1.exe`（8.70 MB）+ `dist/dd-run-0.1.1-portable.zip`（4.36 MB，含 `extensions.d/` sidecar 与清单）。
- **验证**：release 宿主启动正常（事件循环存活）；`dist/extensions.d/dd-ext-search.exe` 经协议驱动正常（`hint`/`empty`/`results` 三类响应齐全，IPC 往返 p50 11.27 ms）；`dd-run-cli --conformance --ext-id com.ddrun.calc` 9 步全绿；三关复跑 fmt 无差异 / clippy 0 告警 / `cargo test --workspace` 432 passed。

### 优化（窗口材质与边框：材质单选 pill + 不透明度滑杆 + 圆角/边框三选 P1–P4，2026-09-13）

- **设置卡更名「窗口材质与边框」**，参考 DeskBox「外观 → 窗口材质与边框」能力面（源码取证），原「云母/亚克力」两互斥开关行重构为四行结构；视觉契约与规格表见根目录 `cmdpal-window-material-effects.html` 效果页。
- **P1 材质单选化**：两互斥开关 → 三段 pill「无材质 / 云母 / 亚克力」（复用 F2 密度卡 pill 口径），选中值仍落单值 `backdrop`，切换防闪链路（倒计时清 DWM）不变。
- **P2 不透明度滑杆（v2 直控式，同日真机反馈修订）**：新增 0–100% 滑杆，语义 = **面板底不透明度直控** `alpha = cap(主题,材质) × pct/100`（cap = 暗·云母 0.75 / 暗·亚克力 1.0 / 亮·云母 0.25 / 亮·亚克力 0.45）——0% = 纯材质，100% = 面板最实（亮色 cap 刻意压低，延续 M1「厚白浓淡=白漆」防线）。**默认 40% 精确复现 M1 真机调定四档锚点**（0.30/0.40/0.10/0.18，默认观感零变化）。修订原因：初版 `基准 × (0.25 + 0.75×pct/100)` 在云母上有效带仅 0.075–0.30，叠在近乎不透明的 DWM 云母上感知极弱（真机反馈「调透明度效果不明显」）；参考 DeskBox 强度滑杆直扫 tint 全带（云母 0.04→0.46）改为全带直控。只走 egui 浓淡层重注册，不碰 DWM，零闪烁面；拖动即时生效不落盘、松手落盘一次（egui 0.36 为 `drag_stopped`）。P2 未发版，换算重定义无迁移。
- **P2 v3 浓淡层基色带系统强调色 + v4 亮色增强（同日显示对齐反馈）**：tint 层基色由纯面板色改为**面板色 × 系统强调色混合**（暗 8% / 亮 30%，DeskBox `BuildContentTintColor` 配方 + 亮度层补偿；复用 P4 的 `DwmGetColorizationColor` 取色，取不到回落纯面板色）。真机截图对比：纯中性白浓淡把 DWM 云母「洗」成无材质感的白平面，同桌面下 DeskBox 呈灰蓝材质面——带色基色后浓淡层在任何 alpha 下都有材质色相；v4 进一步把亮色 cap 放宽（云母 0.25→0.55 / 亚克力 0.45→0.70，默认档 0.10/0.18→0.22/0.28）：v3 带色基色已根治 M1「厚白浓淡=白漆」问题，亮色可安全加厚逼近 DeskBox 存在感（仍低于暗色一档保留采色语义）。`theme::tint_color` 单点收口，visuals 契约单测同步。
- **P3 窗口圆角三选**：圆角 / 小圆角 / 方角（`DWMWCP_ROUND / ROUNDSMALL / DONOTROUND`），默认圆角 = 现状；改选即时重设 DWM 属性（幂等），描边自动贴合新形状（DWM 自绘）。
- **P4 面板边框三选**：中性 / 强调色 / 关（`DWMWA_BORDER_COLOR`），默认中性 = 现状；强调色 = `DwmGetColorizationColor`（系统强调色等价近似，不引 WinRT；取不到回落 `Palette::accent`），跨主题恒色；描边色经 `border_color()` 单点收口，`apply_theme_pref` 同步点同源（中性随明暗、强调色不变）。
- **兼容与降级**：旧配置 `material_opacity`/`corner_pref`/`border_mode` 三键缺失 → 默认值（观感逐像素一致），越界 clamp、未知字符串回落（单测覆盖）；材质未生效（Win10 / 22621-）时滑杆与边框行置灰、圆角行恒可用。明确非目标：MicaAlt、AcrylicBase、边框粗细、材质强度第二滑杆、Win10 旧版亚克力（理由见效果页「非目标」节与 implementation.md P1–P4 小节）。
- **P2 v5 两主题统一 cap + 行填充材质适配（同日反馈）**：①cap 改为按材质定档（云母 0.75 / 亚克力 1.0，不再分主题）——亮色独立低档（0.55/0.70）压低了材质存在感（真机对比 DeskBox 仍差一档），带色基色下统一后亮色默认档 0.22/0.28 → **0.30/0.40**；②结果行填充由单一 `row_hover_glass` 改为 `theme::row_fills` 材质适配对（hover + selected 成对）——同一块 31% 白/灰玻璃在近白云母上不可辨（「不明显」）、在亚克力动态模糊上形成亮块（「太突兀」），现 hover 云母加权（亮·黑 7% / 暗·白 7.8% 玻璃）、亚克力减重（3.5%/3.9%）；**selected 材质档同走玻璃**（比 hover 重一档，回退仍实色）——原不透明 `row_selected` 实色块在半透明材质上随鼠标扫动逐行蹦跳，即真机反馈「鼠标滑动时应用栏目闪烁」，玻璃化后扫动为半透明层平滑过渡（accent 竖条不变，选中辨识度不变）；③**底部页脚同步浓淡层**——页脚原为纯 TRANSPARENT（D31 时代面板无 tint 的一致态），M1 起面板 tint 只由 CentralPanel 绘制、不覆盖页脚矩形，页脚带露出的是未 tint 的原始材质，形成底部色差带（真机反馈「底部栏未同步修复效果」），改为与面板同源 `panel_fill`，材质态页脚与面板一体（回退仍 `panel_2` 实色）；卡片/设置项等其余 hover 不变。
- **P2 v6 设置卡控件设计风格对齐（同日反馈）**：①不透明度滑杆改自绘 Fluent 规格（egui 默认 Slider 的细灰轨 + 行内百分比后缀与整套 pill/开关控件脱节）——轨 4px 圆角 2（未选段 `--border` / 已选段 accent_stroke 小面积强调口径）、钮 16px 白底 border-strong 描边（悬停/拖动加粗 2px），百分比值右对齐行头，禁用置灰不响应；②修复边框三选 pill **竖排**排版缺陷（`add_enabled_ui` 闭包内漏包 `horizontal`）；③`draw_density_pill` 增 `dim` 置灰参数（去 accent、标签 text3、无 hover 反馈），补齐禁用态视觉。
- 零新依赖（`Win32_Graphics_Dwm` 特性既有）；`cargo test --workspace` 全绿（dd-gui 187，新增 settings/theme/hover 单测 5 项），编译 0 警告；真机验收（材质/浓度/圆角/边框矩阵 + Win10 降级）待做。

### 优化（字体与材质显示：F3 拉丁主字纠偏 + M1–M4 材质浓淡/描边/圆角/禁过渡，2026-09-13）

- **F3 字体（参考 DeskBox = WinUI 3 系统排印管线）**：Proportional 族显式重排为 `[segoe, NotoEmoji-Regular, emoji-icon-font, cjk, sym, icons]`——拉丁/数字主字由 egui 内置 **Ubuntu-Light** 纠偏为 **Segoe UI**（此前英文应用名/路径/设置页全用 Ubuntu-Light，与行名 semibold 拉丁的 Segoe UI Semibold 同屏异字体混排）。重排仅动拉丁一处：CJK/emoji/Geometric Shapes（◌）/PUA glyph 回退路由逐一不变；缺文件成员自动剔除（降级安全）。semibold 链继承新序，regular/semibold 从此同为 Segoe 系。落点仅 `platform.rs`（约 20 行 + doc），无字号/密度变化（F1/F2 成果不动）。
- **M1 材质浓淡层**：材质生效时面板底由纯 `TRANSPARENT` 改为**面板色半透明浓淡层**（云母 ×0.35 / 亚克力 ×0.50，`theme.rs` 常量 `TINT_ALPHA_*` 可调）——DWM 材质仍从底下透出，面板轮廓与内容对比度不再随壁纸漂移。D31 切换防闪逻辑不变；`visuals/apply/apply_panel_tint` 签名 bool → `Option<f32>`（3 处调用点同步）。
- **M2 材质描边**：材质生效时 `DWMWA_BORDER_COLOR`（Win11 文档化属性）画 1px 外描边，取色复用 `border_strong` token（暗 `0x666666`/亮 `0xd1d1d1`），跟随 M3 圆角形状；主题切换在 `apply_theme_pref` 同步；材质关闭/Win10 恢复不描边。
- **M3 圆角显式化**：`DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`（≈8px，对齐设计 token `window_corner_radius=8`），HWND 捕获后经 `refresh_backdrop` 一次性应用——消除无边框工具窗圆角「随系统判定」的不确定性。
- **M4 禁过渡**：`DWMWA_TRANSITIONS_FORCEDISABLED`——唤起/隐藏瞬时（A1），不随系统「窗口动画」设置漂移。DeskBox 的动画开关不做：启动器无「要动画」需求面，记档直接禁用。
- 均无新设置项/依赖/配置迁移；无材质与 Win10 路径观感不变。`cargo test -p dd-gui` 181 通过、`cargo clippy` 全仓 0 警告；真机截图验收（亮暗 × 三材质矩阵）待做。
- **真机反馈修订一（同日）**：「不用加粗字体」——`theme::semibold()` 回落 **regular 字形**（函数与全部调用点保留，恢复字重只改一处）；`seguisb.ttf` / `msyhbd.ttc` 加粗字体**停载**（热替换少读 ~19MB，更快），`SEMIBOLD_FAMILY` 保留注册指向 regular 链防未知族；0.05em 标题字距作为排印规格保留。
- **真机反馈修订二（同日）**：「云母/亚克力材质没有整个面板和设置页生效」——像素取证（设置页背景 RGB 带壁纸色偏 G/B>R）确认材质**其实已激活**，被 M1 初版单值浓淡层洗白 + 设置卡实色遮挡，属感知问题非链路故障。修复：①浓淡层**分主题四档**（亮 云母 0.10 / 亚克力 0.18；暗 0.30 / 0.40——亮色厚白浓淡等于白漆）；②材质生效时**设置卡片玻璃化**（`theme::card_fill`：card 色 × alpha 暗 190 / 亮 210，材质从卡面透出）；根页行/搜索框/页脚本就透材质，不受影响。`cargo test` 182 通过、clippy 0 警告。

### 文档（文档 ↔ 代码差异清单修复：71 条差异经三路复核验真后逐条落实，2026-09-13）

- **核对与修复记录**：新增 [`docs/doc-code-diff-2026-09-13.md`](./docs/archive/doc-code-diff-2026-09-13.md)（v1.1，已登记 INDEX.md）——P/M/E/I/D/R/S 七系列 71 条差异，三路独立复核验真（修正清单自身 9 处）后按「文档对齐代码」落实约 84 处修复。规范类文档未实现契约**保留条文**、加「⚠️ 实现现状」注记；含 3 处纯注释代码修正（search.rs 超时注释、builtin.rs / aggregator.rs 陈旧注册注释），无行为变更。
- **要点**：`search.md` 移除与 A-33 红线冲突的「无需 es.exe」承诺（高严重度 S-01）；`protocol.md` §9.2 `-32002` 归因更正（兜底在各扩展 handler，Python 示例确实回该码）；`extensions.md` Python 示例补 `stdin.reconfigure(utf-8)`（中文 query 乱码隐患）；README 双语 M7–M9 关账同步；release.yml 「内嵌」表述改「in-process」。
- **遗留待人工确认**：超限消息「回 `-32600` + 关连接」是否补实现（P-01）、方法名常量层（P-18）等，见 INDEX.md §5。

### 修复（返回后搜索框失焦：嵌套页返回重新聚焦，2026-09-12 真机反馈）

- **现象**：从文件搜索页（或任意嵌套页/设置页）返回后，搜索框没有焦点——无法直接键入，须先点一下输入框。
- **修复**：新增 `PaletteApp::go_back_focused()`（出栈成功即置位 `want_focus`，由搜索框绘制统一消费），Esc、嵌套页「←」按钮、设置页返回、扩展 `GoBack` 动作四条返回路径统一走它；已在 Root 时返回 `None`（隐藏语义不变）。

### 撤销（「优先搜索文件」开关：同日加入、同日移除，用户反馈）

- **原因**：自动进页会**劫持常规搜索**——开启后输入任意关键词都被弹进文件结果页，应用/网页搜索被遮蔽，交互过强。
- **移除范围**：`Settings.prioritize_files` 字段与 JSON 读写、`file_search_drill_target` 的优先模式分支（回到纯 `f ` 前缀语义）、设置页开关行、i18n 文案。**保留**：`f ` 前缀直达文件搜索、进页回填（上一条修复）、「搜索应用」开关。配置中残留的 `prioritize_files` 字段按未知字段忽略，零迁移。

### 修复（搜索引擎配置失效：M9 in-process 后环境变量通道断裂，2026-09-12 真机反馈）

- **现象**：设置页删除引擎后（如只留 Bing），返回首屏「网络搜索」仍显示**全部 5 个引擎**。
- **根因**：M9 把内置 websearch 从 spawn 子进程改为 **in-process**（`serve_line` 直调），但引擎配置通道是进程环境变量 `DD_WEBSEARCH_ENGINES`——宿主只把它写进 manifest `entry.env`（**spawn 子进程路径才消费**），in-process 扩展在宿主进程里 `std::env::var` 永远读不到 → 配置被静默忽略、恒回落内置全表。
- **修复**：dd-ext 新增**内存注入通道** `websearch::set_configured_engines_json(Option<String>)`（`RwLock`，优先级：内存注入 → 环境变量 → 内置默认）；dd-gui 聚合线程（首启与重聚合同路径）在 collect 前注入 `Settings::search_engines_env()`。独立 exe 运行（无宿主注入）仍走环境变量，通道②保持兼容。
- **语义修正**：引擎 JSON 为**合法空数组**时原会回落内置全表（「全部关闭」关不干净）——现返回空表尊重用户意图；仅非法 JSON / 非空数组条目全非法才回落默认。
- **验证**：`cargo test -p dd-ext` 77 通过（新增 resolve 优先级 / 注入往返清除 / 空数组语义 3 条），`cargo clippy` 0 警告。

### 修复（文件搜索进页回填：根查询不再丢失，2026-09-12 真机反馈）

- **现象**：根页输入「测试」后点「文件搜索」进入 `files.results` 页，页内搜索框**为空**——根查询虽已作为 `search` 传给 `get_items`（扩展按其过滤），但页内输入框不显示，用户须重打一遍。
- **修复**：`open_page` 统一回填——带非空查询进页时把查询写入页内搜索框（`f ` 前缀 drill 原有的单独回填随之收编为同一路径）。落地后 `poll_page` 的 v3.3 保留逻辑沿用该值：页内 query 与请求 `search` 一致 → 不触发过期补偿重拉（零多余请求）。所有 `Page` 进页路径（列表点击/Enter、`f ` 前缀、页内嵌套进页）语义一致。

### 新增（搜索行为设置：「搜索应用」开关 + 默认仅 Google，2026-09-12 用户决策；「优先搜索文件」后撤销见上）

- **设置 → 搜索新增「搜索应用」开关卡**（开关行，排版同「窗口材质」卡）：
  - **搜索应用**（默认开）：关闭后「应用」类项在根页空查询首屏与关键词匹配中**均排除**（`PanelState::set_apps_hidden`，两条可见表分支都过滤；聚合落地与应用内切换两处接线）。`Settings.search_apps` 持久化；旧配置缺字段/类型损坏回落默认。
- **默认搜索引擎只开 Google**：`Settings::default().search_engines` 由预设 5 引擎改为**仅 Google**（新增 `default_search_engines()`；`preset_search_engines()` 保留为设置页「添加引擎」下拉的可添加目录）。已落盘配置（含完整引擎列表）不受影响——只改默认值。
- **验证**：`cargo test -p dd-gui` 通过（新增：apps 隐藏过滤 1 条、search_apps JSON 往返 1 条、Google 默认 1 条改写），`cargo clippy` 0 警告。

### 新增（图标与字体展示优化：参考 DeskBox 的「图标/文字大小可调」，方案 [`docs/icons-typography-plan.md`](./docs/icons-typography-plan.md)）

- **列表密度三档（F2）**：设置 → 外观新增「列表密度」卡（紧凑/标准/宽松），一档联动行高（36/40/44）、行名与标签字号（13/14/15、11/12/13）、图标格与 glyph 字号（20/24/28、16/20/24）——`theme::ListMetrics` 单点定义，标准档硬性等于既有常量（parity 单测守卫，默认观感与上一版逐像素一致）。`Settings.density` 持久化（手工 JSON 读写沿用 `label/as_str/parse` 枚举模式，旧配置缺字段/未知值回落标准档，零迁移）。**关键联动**：面板默认高度折算 `base_height_for_workarea` / `root_panel_size` / `settings_panel_size` 增 `row_h` 参数（`lifecycle.rs` 唤起与 `ui()` 页面 diff 两处调用点接线），宽/松档默认窗口不会一行显示不下；Loading 骨架行随档同源，加载完成无布局跳动。设置页三选 pill 复用 radio-card 视觉口径（选中 accent_soft + 2px accent_stroke，hover `control_hover` B7 几何判定），zh/en 文案各 5 条。
- **无图标/url 项回落占位 glyph（I1）**：结果行图标列由「空列」改为极弱色（text4）占位 glyph（U+E7C3）——对齐不变，观感从「图标缺失」变为「本来就没有」；与解码失败的 text2 占位区分层级。修订设计稿 04「空列」决策（代码注释同步记档）。
- **glyph 用色显式化（I2）**：图标格 glyph 改为显式取 `Palette::text2`。核实发现 `visuals.weak_text_color` 已被 theme.rs 显式接管为 text2（weak 档≠更弱），**行为零变化**，仅消除经由 states 辅助函数的间接引用（该函数已无调用者，删除）。
- **列表排印 token 化（F1）**：`theme.rs` 新增 `LIST_TITLE_PT/LIST_CAT_PT/LIST_ICON_GAP/LIST_ICON_CELL/LIST_GLYPH_PT`，列表绘制路径（row.rs / icons.rs）全部引用常量——初值 = 现状硬编码值，纯收拢零观感变化，为 F2 铺路。设置页/页脚字号（各有机理与单测锚点）有意不动。
- **明确不做**（同 DeskBox 的取舍，理由见方案 §3/§4）：256px Shell 图标源（48px 源对 24px 格是精确 2×）、url favicon 网络下载（M5 决策不翻案）、两行文件名（与 B5 单行 + tooltip 决策冲突）、逐组件独立字号。
- **验证**：`cargo test -p dd-gui` 178 通过（新增 density 往返/回落 1 条、ListMetrics parity 1 条、密度档高度折算 1 条），`cargo check --workspace` 干净。

### 新增（M8 扩展生态验证：非 Rust 扩展走通协议全链路，协议 v1.0 零改动）

- **扩展开发指南** [`docs/extensions.md`](./docs/extensions.md)：面向第三方开发者的「30 秒心智模型 → 10 分钟上手」路径。含**三条铁律**（stdout 只出协议消息 / UTF-8 且行内无裸换行 / 通知不回复）、**三个 Windows 陷阱**（文本模式的 `\n`→`\r\n`；stdout 默认编码可能是 cp936 而非 UTF-8，后者不容忍）、各方法按「值得实现」排序、`host/*` 反向请求（不必阻塞等应答）、自检说明、**调试手册（8 条症状→原因对照）**、崩溃语义与超时预算。只写规范里没有的东西，重复处一律引用 `protocol.md` / `manifest-schema.md` 并声明「冲突以规范为准」。
- **Python 全表面示例** [`examples/python-minimal/`](./examples/python-minimal/)：仅标准库的 Python 3 扩展，覆盖 7 个 host→ext 方法 + `items_changed` 通知 + 3 种 `host/*` 请求 + `ShowToast`/`Dismiss`/`Confirm`（含确认后重发 `invoke` 的幂等两段式）。附 Windows `.cmd` 启动器——因为清单的 `entry.args` **不支持** `${EXT_DIR}` 展开，无法表达「解释器 + 脚本」两段式命令行；实测宿主 `resolve_executable` 会为无扩展名路径补 `.cmd`，且 Rust `Command` 能直接 spawn `.cmd`。
- **`dd-run-cli --conformance`**：把扩展自检从 4 步（`--roundtrip`）扩到**全表面**——新增 `fallback_commands` 一致性（非空 ⟺ `has_fallback`）、`get_command` 可复热性（顶层 id 不得返回 `null`）、`get_items` 结构、`invoke` 返回值 ∈ §8.3 八种 `kind`、`host/*` 声明一致性、`close`。逐项输出 `✓ / ⚠ / ✗`，**任一 ✗ 退出码非 0**。新增 `--invoke`（`invoke` 有真实副作用，默认跳过）与 `--ext-id`（目录内有多个扩展时指定）。自检刻意基于**原始 JSON** 而非强类型反序列化——规范约束的是线上形状，强类型成功反而掩盖多字段/错类型问题。新增单测 3 条（兜底一致性四象限 / 8 种 `kind` 无重复 / 诊断截断按字符）。
- **实证**：`dd-run-cli --conformance --extensions-dir examples/python-minimal --invoke` → **10 项全 ✓（2386 ms）**，即**一个非 Rust 扩展走通了协议全链路**——「协议与语言无关」由声明变为事实。

### 修复（PyMin 示例：GUI 下无法启动 —— 启动器依赖 PATH，2026-09-10 真机反馈）

- **现象**：示例装进扩展目录后能在「设置 → 扩展」看到，但标为**暂时不可用**且点「重试」无效。
- **根因**：`dd-ext-pymin.cmd` 调用**裸 `python`**，依赖解释器位于 PATH 上；而 **Python 不在 Windows PATH**（`where python` 无输出）。GUI 从资源管理器启动时继承的是**系统 PATH** → cmd 报「'python' 不是内部或外部命令」→ spawn 失败 → 连续 3 次熔断。自检之所以全绿，是因为它从**终端**启动、PATH 里恰好有解释器。
- **修复**：示例改为**安装脚本生成清单**（`install.py`）——把 `sys.executable` 与脚本的**绝对路径**写进 `entry.command` / `entry.args`（宿主直接 spawn、不经 shell → **零 PATH 依赖**）。移除示例目录里的静态清单（一份固定清单对解释型扩展写不对，且正是假阳性来源）；`.cmd` 降级为**备选**，文件头写明前提与陷阱。
- **指南同步**：`docs/extensions.md` 第 4 步改为推荐绝对路径清单；§3 由「两个 Windows 陷阱」扩为**三个**（新增「解释器不在 PATH 上」，并点明 `--conformance` 全绿 ≠ GUI 能启动——两者继承的环境不同）；§8 症状表补两行（「暂时不可用 + 重试无效」、「终端全绿但 GUI 启动失败」）。
- **验证**：把 PATH 摘到只剩 `System32`（**完全无 python**）后 `dd-run-cli --conformance --ext-id com.example.pymin --invoke` → **10 项全 ✓（902 ms）**，证明清单已与 PATH 解耦。
- **真机确认（2026-09-10）**：用户点「重试」后扩展正常启动，PyMin 命令在 GUI 内可用。

### 改进（扩展失败原因可观测：Failed 原因透出到卡片与日志，2026-09-10 真机反馈驱动）

- **问题**：扩展启动失败/崩溃时用户只能看到「暂时不可用 + 重试」，**根因无处可见**——`SourceStatus::Failed.error` 字段早已存在，但设置页只读 `is_failed()` 布尔、**从未渲染过**；而崩溃路径（`refresh_health`）在丢弃进程前**没有读取已捕获的 stderr**（§2.5 本来就抓了），只打一行「已退出」。两次真机排查（解释器不在 PATH、`invoke` 响应形状）都因此绕了远路。
- **`dd-host`**：新增 `ExtensionProcess::failure_detail()`（`退出码 N / 被信号终止；stderr: <末行>`）与 `stderr_last_line()`；配套纯函数 `last_nonempty_line`（取**末条非空行**、按**字符**截断 + 省略号，中文不破字、CRLF 的 `\r` 被 trim）与 `compose_failure_detail`（两部分皆空 → `None`，不产出空壳提示）。新增单测 3 条。
- **`dd-gui` 失败信息带可操作线索**：`spawn 失败` 附**被尝试的命令路径**（PATH / 路径类问题一眼可见）；`initialize 失败` / `top_level_commands 失败` 附 stderr 末行（`（诊断：…）`，无诊断不留空括号）。`refresh_health` / `invoke` / `get_items` 三处崩溃路径**在进程 drop 前**取出摘要，并入日志与 Failed 原因。新增单测 1 条（后缀拼装）。
- **设置页扩展卡片**：新增第三行**失败原因**（11px `danger` 色，单行截断 + 悬停看全文）；`重试` 按钮的显示判据改由失败原因驱动；行数据抽成纯函数 `extension_rows` + `failed_reason`，新增单测 1 条（Failed 透出 / warm·stub 无原因 / 停用集决定开关）。
- **协议 v1.0 零改动**：纯宿主侧可观测性。**效果**：同类问题定位从「抓包两轮」降为「看一眼卡片」。

### 修复（invoke 响应形状：同一个错误被文档/类型/单测/自检器四处固化，2026-09-10 真机反馈）

- **现象**：PyMin 示例装好后，`当前时间` / `添加便签` / `清空便签` / `打开文档` 四条命令点击即报 `命令执行失败：非法 JSON-RPC 信封：missing field ``kind```（`便签列表` 是嵌套页命令、不经 `invoke`，故不受影响）。
- **根因**：§6.5 规定 `invoke` 成功的 JSON-RPC `result` 字段**就是** `CommandResult` 本体（**单层**），但有**四处**都写成了多一层的 `{"result":{"kind":...}}`——`docs/protocol.md` §6.5 与 §14 的示例、`dd-protocol` 的 `InvokeResult` 结构（`result` 域内再嵌一层）、协议一致性单测 `consistency.rs` 的 §6.5 断言、以及 M8 新写的 `docs/extensions.md` 代码示例。M2 已在**宿主侧**修掉（改为直接解析 `CommandResult`），但文档/类型/单测未同步——于是照着文档写的 Python 示例必然踩中。
- **自检器同样是错的**：`--conformance` 的 `invoke` 判据读的是 `v["result"]["kind"]`，**与错误形状恰好吻合** → 对不合规的扩展判绿。两端互相印证，形成"绿灯骗局"。
- **修复**：① 示例改为 `reply(mid, result)`（单层）；② 协议文档 §6.5 / §14 示例改单层并加显式告诫；③ **删除虚构的 `InvokeResult`**（工作区内除一致性单测外无引用），单测改为断言 `Resp<CommandResult>`；④ `--conformance` 判据改判内层 `kind`，并抽成纯函数 `command_result_kind`——**显式识别并指名「多包了一层」**；⑤ 指南 §5.4 明确「信封的 `result` 就是 CommandResult 本体」，§8 症状表补该条。
- **验证**：抓包探针确认四条 `invoke` 响应均为单层；`--conformance --invoke` **10 项全 ✓**；**反向验证**——把示例改回双层后，自检器正确报 `✗ result 被多包了一层...`（修复前对同一份扩展判绿）。新增单测 1 条（单层通过 / 双层被指名 / 缺 `kind` / 非对象）。
- **教训**：形状类契约一旦「文档 + 类型 + 单测 + 工具」四方同错，就没有任何一处会报红——**必须让至少一个环节面对真实线缆**（本次是抓包探针）。协议 v1.0 零改动（只修错误的描述与断言，不改线上契约）。
- **真机确认（2026-09-10）**：修复版脚本装入 `dist/extensions.d/` 后，用户实测四条 `invoke` 命令（`当前时间` / `添加便签` / `清空便签` / `打开文档`）在 GUI 内**全部正常执行**。

### 性能（方向 C 打磨：warm 进程空闲超时回收，协议 v1.0 零改动）

- **warm 进程空闲超时回收**：`LRU_WARM_CAPACITY`(8) 大于扩展总数（内置 5 + 官方 sidecar 1 = 6）→ LRU **永不触发驱逐**，保活集行为上"只增不减"（稳态常驻全部扩展）。`LruWarmSet` 增空闲判定：每次触达记录「最后触达时刻」，`idle_victims(ttl)` 只读返回空闲超阈值者；宿主 `warm_idle_reclaim()` **复用既有驱逐路径**（`close` + 回落 stub），下次使用走桩复热。
- 阈值 `WARM_IDLE_TTL = 120s`；**仅在面板隐藏时**回收（用户正在看列表时不回收）；在途请求（进程已 take 出保活集）跳过；隐藏期以 `WARM_IDLE_HIDDEN_TICK = 15s` 请求重绘驱动巡检（保活集清空后自动停止请求，恢复静默）。
- 新增单测 6 条：`LruWarmSet` 3 条（触达刷新不判空闲 / `ttl` 边界「≥ 即回收」/ 队尾优先 + `last_access` + `remove` 生效）、宿主 2 条（隐藏态仅回收超阈值者且源回落 Stub、可见态不回收）、兜底模板识别 1 条。

### 修复（L5：LRU 驱逐后点兜底模板必失败，协议 v1.0 零改动）

- **现象**：扩展被驱逐成 stub 后，点击其**兜底（模板）项**必然失败——报「命令已失效」，且 `invoke` 从未发出。
- **根因**：协议 §6.4 的 `get_command` **只查顶层命令**（`dd-ext/src/lib.rs`），而兜底模板 id（如 `calc.eval.query`）天然不在顶层注册表 → 桩复热链路拿到 `Ok(None)` 即判「陈旧」并短路。
- **修复**：`FallbackStore` 新增 `contains_template(ext_id, id)`；`dispatch_invoke` / `dispatch_fetch_page` 判定为模板时**跳过 §6.4 校验直接执行**（invoke 侧走 `skip_lookup`，page 侧传 `command_id=None`，等价「无对应命令点击」语义）。**常规命令保持 §6.4 陈旧校验不变**。
- 该缺陷此前不触发（`LRU=8` > 扩展数 6，永不驱逐）；本次空闲回收让驱逐真实发生，故与上一条**同批修复**。

### 新增（M9 内置扩展进程内化：单文件分发，内置不再 spawn 子进程）

- 5 个内置扩展（apps / calc / system / websearch / shell）自 M9 起改为**宿主进程内**运行：宿主直接驱动 `dd_ext::serve_line`，不再 spawn 子进程、不再内嵌扩展 exe（`dd-gui/build.rs` 内嵌空表，`assets/embed/` 仅遗留目录）。
- 发版产物收敛为**单一宿主文件** `dist/dd-run-<ver>.exe`（不再含内置 exe）；文件搜索 sidecar（`dd-ext-search.exe`）仍按 ADR-1 以子进程运行，随绿色包 `dist/extensions.d/` 携带。
- 配套：`dd-run-cli --conformance` 对内置扩展改走 in-process 分支（直驱 `serve_line`）；websearch 引擎配置改由宿主内存注入（`set_configured_engines_json`），不再依赖 spawn 子进程才生效的 `DD_WEBSEARCH_ENGINES` 环境变量（详见上文「搜索引擎配置失效」修复）。
- 协议 v1.0 零改动（in-process 与子进程走同一套 NDJSON 路由，字节等价）；`tools/package.sh` 改为 M9 单文件出包流程（见仓库 `README.md` 构建与打包一节）。

### 新增（apps 扩展垃圾项过滤：安装附属工具黑名单 + 同 exe+参数去重 + AppsFolder 伪应用拦截）

- 应用扩展结果新增**垃圾项过滤**：屏蔽安装器写入的「附属工具 / 卸载程序」等噪音项、按「同 exe + 参数」去重、拦截 `Applications` / `AppsFolder` 伪应用条目，使根视图与关键词匹配结果更干净。

### 性能（运行时内存优化 M1+M2+M3：隐藏后工作集修剪 + 字体栈 mmap 化 + 图标纹理缓存清空）

- 面板隐藏后执行**工作集修剪**；字体栈改为 `mmap` 化（按需分页，降低常驻内存）；图标纹理缓存由宿主在隐藏态清空，避免长期占用显存/内存。三项合计降低空闲期内存占用（协议 v1.0 零改动，纯宿主侧）。

### 文档

- README 双语修正**跨平台宣称口径**：原文「cross-platform (Windows / macOS / Linux)」是设计目标而非现状，改为「核心与平台无关；**当前仅发行 Windows 版**，macOS / Linux 计划 v0.2+」；对比表表头同步为「平台无关设计」，Goals 首条标注「计划中，当前仅 Windows」。

## [0.1.1] - 2026-09-10

### 新增（文件搜索 v3.3 P2：everything-ipc 主通道，es.exe 降级为回落，协议 v1.0 零改动）

- 传输层从「每次按键 spawn `es.exe` 子进程」升级为 **`everything-ipc` crate 直连 Everything IPC**（WM_COPYDATA，纯 Rust，**无外部进程、无 DLL 随包**）为**主通道**；`es.exe` 降级为**回落通道**（主通道不可用 / 查询失败 / 超时自动回落）。
- 依赖形态（`crates/dd-ext/Cargo.toml`）：`everything-ipc = { version = "=0.1.4", default-features = false }`，置于 `[target.'cfg(windows)'.dependencies]`（该 crate 无平台守卫，仅 Windows 可编译）；版本锁死 `=0.1.4` 防 0.1.x 年更 API 漂移，默认 feature 为空（不引入 tokio/pe/folder）。
- client 采用 `EverythingClient::shared()`（全局 Weak，Arc 全释放后自动重建）+ 本模块 `Arc` 缓存；**连续失败 3 次即释放缓存触发重建**，使 Everything 重启 / 实例切换后能从回落态恢复到 IPC（§9.4 A-33-10）。
- 显式 `timeout(1200ms)`（< 宿主 `get_items` 2000ms）；`RequestFlags = FileName|Path|Size|DateModified|Attributes`，目录用 `Attributes & 0x10` 精确判定、修改时间用 `FILETIME`（复用 `filetime_to_unix`）。
- 可用性探活改为 **IPC 轻量探活优先**（`FindWindowW` + `is_ipc_available` + `is_db_loaded`，不建 client / 不起线程），回落 `es.exe -get-everything-version`。
- 抽出两通道共用的纯函数 `make_entry()` / `combine_filetime()`，新增 4 条 L1 单测（FILETIME 高低位合并 / 属性判目录 / 无属性启发式 / FILETIME→Unix）。
- **真机验证**：L2 NDJSON 冒烟——`get_items("cargo")` 返回 30 条真实索引结果，stdout 全为合法 NDJSON（**无 tracing 污染**），stderr 无回落日志（即 IPC 主通道生效）。**协议 v1.0 零改动**。
- 说明：`es.exe` 仍保留为回落通道；待真机验收 A-33-06（回落）/ A-33-10（恢复）/ A-33-08（版本矩阵）通过并发布后，普通用户方可不再安装 `es.exe`。

### 新增（文件搜索 v3.3 P1：上下文菜单三动作，协议 v1.0 零改动）

- 文件结果项新增 **`more_commands` 上下文菜单**（§8.1）：右键 = 打开（默认）/ **显示所在目录**（`files.reveal.{pid}`，扩展侧 `explorer /select` 经 `raw_arg` 规避引号坑）/ **复制路径**（`files.copy.{pid}`，Toast + `host/set_clipboard`）。
- 宿主右键菜单支持渲染扩展声明的 `more_commands`（`PanelItem` 透传），激活时以 `sender=context_menu` + `context.selected_item_id` 回调 `invoke`；扩展自带动作时 GUI 静态类别映射让位（避免重复）。
- `spec()` 与 sidecar manifest（`examples/extensions.d/com.ddrun.filesearch.json`）capabilities 同步追加 `host/set_clipboard`（能力前置 §7.4：未声明将被宿主回 `-32601`），启动时一致性断言覆盖。

### 新增（文件搜索：结果项按文件类型显示图标，协议 v1.0 零改动）

- 文件结果项图标从「目录/文件两态」升级为**按扩展名 12 类映射**（`crates/dd-ext/src/bin/search.rs` 的 `file_category` + `category_glyph`）：文档 / 代码 / 压缩包 / 图片 / 音频 / 视频 / 可执行 / 配置 / 字体 / 电子书 / 文件夹 / 兜底 Page（未收录扩展名、无扩展名、dotfile 均回落 Page）。
- 引导项（`files.hint` / `files.guide` / `files.error`）与顶层 / fallback 入口统一为**搜索图标**（Segoe `Search` U+E721），文件搜索链路图标语义一致。
- 全部 13 个码位已核验在两代图标字体（Win11 `SegoeIcons.ttf` / Win10 `segmdl2.ttf`）的 cmap 中存在；映射为扩展端纯查表（零 IO、零新依赖），不影响 <100ms 搜索响应红线。**协议 v1.0 零改动**（复用 `Icon::Glyph`）。
- 新增 L1 单测 7 条：类别映射 / 大小写不敏感 / dotfile·尾点·多重点边界 / 12 类码位两两互异 / 目录优先 / 图标随类型 / 入口统一搜索图标。

### 修复

- 引导项 `files.hint` / `files.guide` / `files.error` 点击给出**专属文案** Toast（此前落回通用「未知命令」）；`files.guide` 的 subtitle 同步改为 es.exe 通道文案（移除过时的「启用 HTTP 服务器」表述）。
- 嵌套页支持**边打边搜**（200ms 去抖重拉 `get_items`）+ 过期结果补偿 + 落地保留页内 query；嵌套页跳过宿主本地二次过滤（`passthrough`），进页自动聚焦搜索框。
- 文件搜索结果右键菜单与页脚 Enter 提示的默认动作文案从「执行」改为「打开文件」（**遗漏修复**：`footer_action_text` 的 ext_id 映射表自 `com.ddrun.filesearch` 加入以来未同步，文件项被回退到 `footer.invoke`；实际 `files.open.{pid}` 走 `host/open_url` 调 ShellExecute 用系统默认程序打开，与 `apps` 扩展「打开应用」同语义）。`text.rs` 新增 `footer.open_file` i18n 条目，`footer_action_maps_five_builtin_extensions` 与 `ctx_menu_more_commands_override_static_mapping` 测试期望同步。
- 文件搜索结果按 Enter 真正「打开文件」（v3.3 P1.5 修复）：此前 `host/open_url` 对 `file://` URL 一律走 `webbrowser::open`——目录→浏览器索引页；`.txt`/`.html`/`.mp4` 等走浏览器而非关联程序；`.exe`/未知扩展名浏览器报错。改为检测 `file://` 协议后改走 `ShellExecuteW(verb="open")` 自动派发（**目录 → Explorer 窗口；文件 → 关联程序**），`http(s)://` 保持 webbrowser 默认浏览器（websearch 不受影响）。**协议 v1.0 零改动**（复用既有 `host/open_url`）—— 新增 `platform::file_url_to_path` + `platform::open_path`（仿 `run_as_admin` 风格），5 个单测覆盖基础/编码/CJK/非 file 协议/远程 file URL。

### 修复（文件搜索 v3.3 P2：`file://` URL 解析三缺陷，协议 v1.0 零改动）

源码缺陷见 [`docs/search-file.md`](./docs/search-file.md) §9.6（2026-09-10 记录），本批全部修复：

- **`%` 被无条件解码**（文件名含 `%` 打不开）：`report%20final.txt` → 旧链路 `file_url_to_path` 一律 `%XX` 解码 → `report final.txt` 打开失败。改为**候选 + 存在性优选**：`file_url_candidates` 按「原样 → 去 `?query`/`#fragment` → 上述两者的 percent-decode」产出排序候选，`resolve_file_url_to_path` 取**第一个存在**者（全不存在回退首选）。字面路径存在即命中自身，标准编码 URL（`%20`）仍落到解码版——**两种来源互不干扰**。
- **`#` / `?` 未做 fragment/query 截断**：新增 `strip_query_fragment` 作为次要候选；Windows 文件名允许 `#`（原样候选优先）、不允许 `?`（必然回落到截断候选）。
- **UNC 路径不支持**：`file://server/share/x.txt` 旧链路恒返回 `None` → 误落到 `webbrowser::open` 而失败。改为 `split_file_url` 解析 authority 并映射 UNC `\\server\share\x.txt`；**UNC 不做 `exists()` 优选**（离线 SMB 共享会让 `Path::exists()` 阻塞到网络超时），交给 `ShellExecuteW`。扩展侧 `path_to_file_url` 同步：UNC 输入走 `file://<host>/<rest>`，不再拼成必然打不开的 `file:////server/…`。
- **未做 percent-encode（`search.rs` 侧保持原样）**：刻意选择——宿主侧宽容解析已完全覆盖 `%`/`#`，改动扩展侧只会让新旧版本的扩展与宿主产生不必要的耦合（file-search 是 sidecar，版本可独立演进）。
- 验证：`fmt --all --check` 干净、`clippy --workspace --all-targets -- -D warnings` 0 告警、workspace 测试全绿；宿主侧 **8 条**单测（原 5 条改写为「候选列表」口径 + 新增 3 条，含一条真实文件系统夹具：`report%20final.txt` 存在 → 命中字面文件；删除后 → 回退 `report final.txt`），扩展侧新增 **4 条**（本地/CJK/`%`/`#` 不编码、UNC authority、退化形态、UNC invoke 端到端 URL）。
- **环境洼地记录**：本机 Bash 会话 `APPDATA` 为空（`USERPROFILE`/`TEMP` 正常），`cache_dir()` 依赖它 → 未导出时 `dd-ext-apps` 的图标抽取两条测试必失败（覆盖率 0%，因 PNG 无法落盘），`export APPDATA="C:\Users\<user>\AppData\Roaming"` 后 12/12 全过。测试前先看一眼 APPDATA，别把环境缺失误判成代码回归。

## [0.1.0] - 2026-09-06

### 新增

- **文件搜索**（扩展 `com.ddrun.filesearch`）：基于 Everything 的本地文件搜索。
  - 根视图输入 `f ` + 关键词自动进入文件结果页；同时保留顶层入口与 fallback 模板入口。
  - 每次最多返回前 30 条，按「文件名 > 路径」相关度并叠加近因加分排序；回车经 `host/open_url` 用系统默认程序打开。
  - Everything 搜索语法（`ext:` / `dm:` / `path:` / 通配符 / 正则）原样透传。
  - 以 sidecar 形式随绿色包分发（`dist/extensions.d/`），免安装。
- 用户文档：[`docs/search.md`](./docs/search.md)（Everything 配置、语法速查、故障排查）。

### 说明

- 文件搜索依赖 Everything，**仅支持 Windows**；跨平台 fd 兜底 Provider 计划于 **v0.2** 提供。
- 协议 v1.0 **冻结**：本版本未新增任何协议方法，文件结果复用协议已定义的 `get_items` 子页能力。

### 传输层（实现说明：es.exe 通道切换（`6133d96`）晚于 v0.1.0 标签最初落库（2026-09-06）；该标签 2026-09-09 经用户决策删除并重打至 es.exe 实现之后的提交，随重打首次触发 GitHub Release）

- **经 Everything 官方命令行工具 `es.exe`（IPC 通道）检索**，**无需开启 Everything HTTP 服务器**；仅需 Everything 在运行 + `es.exe` 已安装（`winget install --id=voidtools.Everything.Cli`）。
- 配置项改为 `DDRUN_ES_PATH` / `DDRUN_EVERYTHING_DIR`（默认自动定位，无 HTTP/端口概念）。
- es.exe 经管道输出为系统 OEM 代码页（中文 Windows = 936/GBK）而非 UTF-8，已由 `decode_output()`（`MultiByteToWideChar` + `GetConsoleOutputCP`，复用 `shell.rs` 惯例）按代码页转 UTF-8，单测 `decode_with_codepage_converts_gbk_filename` 覆盖——修复此前中文路径乱码导致「搜不到文件」的问题。
