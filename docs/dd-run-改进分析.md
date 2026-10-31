# dd-run-改进分析

# dd-run 改进分析

> 对象：`Yang-SH/dd-run` @ `ff7817f`（v0.2.1，214 commits，2026-10-24 拉取）
> 方法：全量静态检索 107 个 Rust 文件（48,159 行）与全部 docs，精读核心路径（app / aggregator / ext\_client / process / trust / settings / ui / platform），并在本机实测构建与测试。**未逐字通读全部 107 个文件**，结论以可复核的命令输出与 file:line 为准
> 结论可信度：所有"证据"列均为仓库内可定位的 file:line 或本机命令输出

---

## 0. TL;DR（如果只读五行）

1. **这个项目的代码质量明显高于同类个人项目**——协议冻结、信任链、崩溃熔断、有界队列、原子写盘都做到了，不存在"能跑就行"的迹象。真正的问题不在代码写得好不好，而在**结构熵**和**验证盲区**。核对时还实测出一个被 CI 掩盖的硬伤：非 Windows 构建的字体集**必然为空**（§4.9，已跑探针验证）。
2. **最大的隐性成本是文档税**：26 份 docs ≈ 1.0 MB，CHANGELOG 201 KB，README 29 KB——**文档体量与 48k 行代码基本相当**；而且 24 处"实现现状"注已有一处确证漂移。docscan 只查链接与行数，查不出语义。
3. **最强的杠杆是 UI 快照测试**：7,316 行绘制层只有 30 个纯函数单测、**0 张渲染快照**；三平台主要靠人工"真机走查"。这是限制迭代速度的真正瓶颈，不是缺功能。
4. **§3.4 里性价比最高的是 4 件小事**：`rust-toolchain.toml`、workspace 版本继承、CI clippy 扩到 Linux/macOS、发布产物加 SHA256 + 构建出处证明。半天工作量，换来可复现与可验证。（注意：§3.1/§3.2 两项 P0 反而最贵，见各自的工作量行）
5. **不建议**在这个阶段做的：WASM 沙箱、async runtime、拆 crate、SQLite、第三种语言。见 §7。

---

## 1. 实测基线（可复现）

本机环境：Linux x86\_64 / rustc 1.95.0（按 `rust-version = "1.95"` 安装）。

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 编译 | `CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu cargo check --workspace --all-targets` | ✅ 通过，**约 32 条 warning**（`dd-gui` 16 / `dd-ext` 14 / `dd-host` 1 / `dd-ext-search` 1）——全部是 Windows 专属代码在 unix 下变成死代码/未用导入（`tray_tooltip`、`combine_filetime`、`EXEC_TIMEOUT_MS`、`MAX_SUMMARY` 等） |
| 测试 | 同上 `cargo test --workspace --no-fail-fast` | ⚠️ **561 passed / 3 failed**。失败全部同一根因：用 `chmod` 注入写盘失败，root 无视权限位（`dd-gui` 2 例 + `dd-host` 1 例，见 §4.3）。**注意**：不加 `--no-fail-fast` 时 cargo 在 dd-gui 失败处即中止，只能看到 `415 passed / 2 failed`，会误以为其余 crate 全绿 |
| 规模 | — | 107 个 `.rs` / 48,159 行；注释行占比 18.9%（占非空行）；`PaletteApp` **95 个字段**；依赖树 395 包 |
| 文档 | — | `docs/*.md` 26 份 ≈1.0 MB（981 KiB）；CHANGELOG 201 KB / 963 行；README 中英 29 KB |
| 测试分布 | — | 631 个 `#[test]`（**静态计数**：大量 `cfg(windows)` 用例在非 Windows 上根本不编译，故三平台门禁不能互相替代）；`ui/` 7,316 行仅 30 个；`platform/` 1,614 行仅 17 个 |
| 工程卫生 | — | `clippy` 只在 windows-gnu 跑（`ci.yml:40`）；无 `rust-toolchain.toml`；无 fuzz；无 bench；无覆盖率 |

> 这 32 条 warning 说明一件重要的事：**非 Windows 分支的代码长期不在 clippy 视野内**。它们不是致命 bug，但会持续积累。

---

## 2. 结构性判断

用一句话概括：**dd-run 用"文档 + 台账 + 验收清单"承担了本该由"测试 + 类型 + 自动化"承担的职责**。

这套流程在 M0–M9 阶段非常有效（因为它强制作者把每个决策想清楚），但代价现在开始显现：

- 每改一处实现，可能要同步 README / implementation.md / INDEX 行数 / CHANGELOG / 专题方案——**120 commits/30 天**的节奏下，这套同步是线性叠加的。
- 文档里的"实现现状 ⚠️"是**散文**，不是断言。`docs/manifest-schema.md:35` 现在就与代码矛盾（见 §3.3）。
- UI 的正确性靠人眼，于是"真机走查"成了发布前的固定成本，且不可并行、不可回归。

**方向不是"少写文档"，而是把知识搬到可执行的位置**：能断言的变成测试，能生成的变成脚本，剩下的才是散文。

---

## 3. P0：不修会持续烧时间（4 项）

### 3.1 `PaletteApp` 是 95 字段的上帝对象

| 项 | 内容 |
| --- | --- |
| 证据 | `crates/dd-gui/src/app/mod.rs` 的 `PaletteApp` 结构体，95 个 `pub(crate)` 字段；同文件 1,368 行；`app/` 目录已有 15 个子模块（含 `keys/` 子目录），但状态仍全部平铺在一个结构体上 |
| 影响 | 每个新功能都要往这个结构体加字段；任何一个子模块的方法都能读写全部状态，**借用检查器无法帮你发现越界访问**；`size_tests` 甚至在为结构体体积写断言（`app/mod.rs` 的 `size_tests`），说明作者也感到它在膨胀 |
| 改法 | 按「谁拥有谁」切**六个**子结构（分组不是拍脑袋：按 95 个字段逐条归类，并集 = 全集、两两互斥）：`WindowState` **26**（visible / want\_focus / hwnd / backdrop\_\* / native\_resize\* / hide\_\* / drag\_candidate / scroll\_follow / tray\_\* …）`ExtPool` **22**（processes / exts / trust / lru / inproc\_specs / crash\_guards / fallback\_\* / cache / inflight …）`SettingsDraft` **18**（settings / engines\_dirty / 各类 `*_buf` 与 `*_err` / settings\_category …）`UiViewState` **13**（stack / icon\_cache / icon\_failed / backdrop\_cache / clipboard\_\* / lang\_effective / e2e\_\* …）`HotkeyState` **11**（hotkey / capture\_\* / hotkey\_\*）`OverlayState` **5**（toast / confirm / ctx\_menu / ctx\_row\_rects …）合计 26+22+18+13+11+5 = **95**，无遗漏、无重叠 |
| 验收 | ① `PaletteApp` 直接字段 ≤ 8（六个子结构 + ≤2 个跨域句柄）；② `cargo test --workspace` 全绿且测试数不减；③ 被移动函数体逐字节相同（`git diff -w` 抽查）；④ 不新增 `pub(crate)` 可见性（越界访问由编译器挡住） |
| 工作量 | **3–5 天，不是 1–2 天**（上一版低估了）。真实成本不在搬字段，而在 **944 处 **`self.<field>`** / **`app.<field>`** 引用要逐一改写**（实测：lifecycle.rs 132、keys/settings\_actions.rs 114、app/mod.rs 100、ui/panel.rs 67…）。**口径注记**：944 为**字段引用**口径；全量 `self.` / `app.` 出现（含方法调用）共 **1,613** 处，摸排工作量时以此兜底。机械但量大，必须按组分批：`OverlayState`(5) → `HotkeyState`(11) → `SettingsDraft`(18) → `UiViewState`(13) → `ExtPool`(22) → `WindowState`(26)。每组独立可编译、独立可提交，任一批出问题只回退一批 |

> 注意顺序：**先做这项，再做 §4.1 流式聚合**。否则流式聚合会再给这个结构体加 3–5 个字段，等于给债务付利息。
> 但反过来：944 处引用改写虽然机械，**必须等批次 B（金图 + 契约测试）落地后再动**，否则改错了没有护栏。

### 3.2 绘制层零快照测试，三平台靠人眼

| 项 | 内容 |
| --- | --- |
| 证据 | `crates/dd-gui/src/ui/` 共 7,316 行，30 个 `#[test]`，全部是几何/解码等纯函数断言（`ui/row.rs` 4 个、`ui/chrome.rs` 6 个…）；仓库内搜 `snapshot` / `insta` / `golden` 三个关键词**均无命中**；`ui/panel.rs`（851 行）、`ui/settings_view/appearance.rs`（843 行）**零测试** |
| 影响 | 任何一次主题/间距/图标改动都无法自动验证；`docs/user-session-walkthrough-checklist.md` 这类人工清单是唯一的回归手段，且要求真机 + 三平台 |
| 改法 | 引入 `egui_kittest = "0.36.2"`（与 egui 0.36 同号、MSRV 同为 1.95），开 `wgpu + snapshot + eframe`。**可行性已核实**：`HarnessBuilder::build_eframe` 确实存在（`FnOnce(&mut CreationContext) -> State where State: App`），`CreationContext` 的 `pub egui_ctx: egui::Context` 字段也在（eframe 0.36 的 `CreationContext::egui_ctx`），`PaletteApp` 实现了 `eframe::App`，三者齐备；`PaletteApp::new` 是**纯注入式**（8 个参数：HotkeyThread / Receiver&lt;TrayEvent&gt; / Arc&lt;AtomicBool&gt; / Receiver&lt;AggregatePayload&gt; / ColdStartTimer / Option&lt;FrozenCache&gt; / Settings / Arc&lt;AtomicBool&gt;），构造时**不注册全局热键、不扫扩展**，唯一副作用是 `clipboard_worker::spawn()` 起一个剪贴板线程（有失败降级）。热键依赖也无需先抽 trait：直接 `HotkeyThread::spawn(ctx.clone(), 0, 0)`，注册失败/线程创建失败都会返回降级实例且**不 panic**（`hotkey.rs` 的 `HotkeyThread::spawn` 降级实例段），CI 上安全。测试里传空 channel + 直接铺 `stack` 即可出图。 |
| 关键设计 | 金图必须注入固定测试字体，不能吃 runner 的系统字体。 ① 项目字体栈全取自 Windows（`msyh.ttc` / `segoeui.ttf` / `SegoeIcons.ttf`），非 Windows 上字形不同甚至无字体（§4.9）；② 即使固定在 Windows runner，GitHub 镜像更新也会换字体版本 → 金图无端漂移。做法：在 `build_eframe` 闭包里 `cc.egui_ctx.set_fonts(测试字体)`，内嵌一份 Noto Sans SC 子集约 1–2 MB，**只进 dev-dependency / 测试资产，不进产物**。**同时冻结全部动态量**：`ui_animations = false`、`icon_cache` 预置固定纹理或留空（走字形）、`lang_effective` 固定、`cold` 打桩、避开 `Instant::now()` 分支。先覆盖 **6 张**：空查询暗色 / 空查询亮色 / 有结果 + 选中 / 嵌套页 / 设置-外观 / 右键菜单。**分工要清楚**：金图只负责「布局 / 间距 / 配色 / 层级」的回归，**不负责真实字体栈**——真实字体是否有字、缺字码位、Windows msyh/segoeui 回退链，由 §4.9 的单测（断言 `font_data` 非空且含预期族）覆盖。两者互补，不要用金图兼职字体测试。 |
| 验收 | ① 6 张金图入库（每张 ≤ 300 KB；650×440 暗色面板 PNG 实测多在 100–300 KB，上一版写 150 KB 偏紧）；② 故意把行高 +1px，测试必须失败（自证有效）；③ 换机器 / 清 `~/.cargo` 重跑，金图不变（证明字体不依赖环境）；④ CI `snapshot` job ≤ 8 min（含 wgpu 编译）；⑤ 后续 UI PR 不再需要人工走查即可合并 |
| 工作量 | 2–3 天。**唯一不确定点**：CI 的 wgpu 适配器——GitHub runner 无 GPU，Windows 走 WARP（DX12 软件适配器）、Linux 走 lavapipe，都能跑但要在 `wgpu_setup` 里显式指定；若某平台调不通，退路是**只在能跑通的单一平台出金图**（其余平台跑交互断言），不影响方案主体 |

> 这是整份报告里**回报最高**的一项。它把"改 UI 要三平台手测"变成"改 UI 看 diff"。

### 3.3 文档语义漂移已经发生，docscan 拦不住

| 项 | 内容 |
| --- | --- |
| 证据 | **两处已确证的漂移**：① `docs/manifest-schema.md:35` 写「单个清单读取失败的条目被**静默丢弃**」（原契约锚 manifest.rs 498–516 行），实际 `crates/dd-host/src/manifest.rs` 的 `outcome.skipped.push(SkippedExtension{..})`（R-20 起已上报 UI），且 498-516 现在是 `from_command()`；② `docs/protocol.md:295` 写「落盘门禁读取清单 `frozen`，见原 `aggregator.rs` 锚 394、411–435 行」，实际那三处分别是 `}`、`pub fn sidecar_extensions_dir`、`first`；真实位置是 **662 与 685 行**。**规模**：「实现现状」注 **24 处**（protocol.md 16 / manifest-schema.md 5 / extensions.md 2 / search-file.md 1）；而**行号锚全 docs 共 93 处**，其中对扩展作者可见的契约文档占 **16 处**（protocol 11 / manifest-schema 3 / extensions 2）。所有现状注都带「核对至 2026-09-13」——作者已经在人工对日期，说明知道它会烂 |
| 影响 | 读者（尤其扩展作者）会按过期文档做判断；规则文档的可信度是扩展生态的地基，一处漂移的伤害大于十个 bug |
| 改法 | 分三步，**按腐烂速度排序**，不要一次全上：**① 先杀行号锚（收益最大、最机械）**：契约文档的 16 处行号改为「文件 + 符号名」（如 `aggregator.rs` 的 `load_one`）。`docscan` 的检查必须**分批放开**，否则 93 处旧锚会让 CI 立刻全红：第一版只对 `protocol.md` / `manifest-schema.md` / `extensions.md` 三份生效，剩余 77 处清完后（可另开 issue 按模块做）再把扫描面扩到全 docs。**② 再把行为类现状注变成契约测试**：新建 crates/dd-host/tests/doc_contract.rs（拟新建），每条写一个断言，测试名即锚点（`fn manifest_skip_is_reported_not_silent()`），失败信息写「请同步 docs/manifest-schema.md §2」。注意这批注分两类——✅「已实现」类断言正向行为；⚠️「未实现/有偏差」类要断言**当前确实没有该行为**（例如 `timeouts.*` 宿主不读取、未知 `page_id` 回 `-32005`），这类测试的价值是「一旦有人补上实现就红」，提醒同步文档。**③ INDEX 行数改脚本生成**：`tools/docscan.mjs` 现在强制「INDEX 登记行数 = 实际行数」，每次编辑文档都要手改 INDEX；改成 `docscan --write-index` 自动回填，人只维护状态与读者列 |
| 验收 | ① 契约文档（protocol / manifest-schema / extensions）行号锚 **16 处 → 0**；② `docscan` 对这三份文档启用「出现 `.rs:NNN` 即 fail」，并进 CI（**只对这三份**，其余 77 处留给第二阶段，否则 CI 无法转绿）；③ 24 处现状注至少覆盖 ✅/⚠️ 两类各 70%（少数「代码无对应枚举」这类无法断言的，允许标注 `no-assert: 原因`）；④ 改任一文档不再需要手改 INDEX |
| 工作量 | **4 天**（上一版写 2 天偏乐观：93 处行号锚清理 + 24 条断言，其中若干要先让代码可测，比如「渲染侧忽略 toast state」需要能单独调渲染函数） |

### 3.4 构建与发布不可复现 / 不可验证

| 项 | 内容 |
| --- | --- |
| 证据 | ① 无 `rust-toolchain.toml`，本地靠人装（我这次就因为默认 1.93 直接报 "requires rustc 1.95"）；CI 用浮动的 `stable`（`ci.yml:21,92,130`）；② 版本号在 **5 个产品 crate** 里各自重复写 `0.2.1`（`dd-protocol` 是独立版本线 `1.0.0`），而 `release.yml` 的 tag 校验（三平台 job 各一处：`:36` / `:115` / `:151`）**都只读 **`crates/dd-gui/Cargo.toml`，其余 4 个产品 crate 漂了不会被发现；③ 发布产物无 `SHA256SUMS`、无 build provenance，可执行文件本身未做 Authenticode 签名（只有扩展 `.sig`） |
| 影响 | ① 新贡献者第一小时就卡在工具链；② 版本漂移会污染 `HOST_VERSION` 相关语义（`min_host_version` 判定、sidecar 钉扎）；③ 用户下载 8.4 MB exe 无法校验来源，SmartScreen 警告也无法解释 |
| 改法 | ① 提交 `rust-toolchain.toml`：`[toolchain] channel = "1.95.0"` + `components = ["clippy","rustfmt"]`，CI 去掉手写 `rustup toolchain install`；② workspace 继承：根 `Cargo.toml` 加 `[workspace.package] version/license/edition/rust-version`，5 个产品 crate 改 `version.workspace = true`（`dd-protocol` 保持独立 `1.0.0` 版本线，用 `# 协议版本独立于产品版本` 注释说明），版本提升只改一处；③ release job 增加两步：`sha256sum` 产出 `SHA256SUMS.txt` 一起上传；`actions/attest-build-provenance` 生成 SBOM 级出处证明（免费，无需证书） |
| 验收 | ① 干净机器 `git clone && cargo test` 零手动步骤；② 5 个产品 crate 版本一致（由 workspace 继承天然保证）+ 脚本校验协议版本独立；③ Release 页面有 `SHA256SUMS.txt` 且能被 `sha256sum -c` 验证 |
| 工作量 | 1 天（含清 32 条 warning；纯配置部分只要半天） |

---

## 4. P1：影响体验或长期可维护性（9 项）

### 4.1 首屏是"全量等待"而非"先到先渲染"

- **证据**：`aggregator::collect_top_level` 用 `thread::scope` 起 N 个线程后**按 spawn 顺序 **`join`** 全部**（`crates/dd-gui/src/aggregator/spawn.rs` 的 `collect_top_level`（批次 B 目录化后落点））；`spawn_aggregation` 只在全部完成后发**一次** `AggregatePayload`（今 `aggregate_job.rs` 的 `spawn_aggregation`，`tx.send` 仅一处）。单扩展最坏耗时 = `initialize` 5 s（`process.rs` 超时常量）+ `top_level_commands` 3 s（同文件常量）。
- **影响**：一个走 spawn 路径的扩展慢 2.5 s，首屏就空 2.5 s——哪怕内置 5 个扩展（in-process）早已就绪。A2"数据就绪 \< 200 ms"是在"只有内置扩展"的前提下成立的口径。
- **成立前提（重要，别夸大）**：`frozen=true` **且已有磁盘桩**的扩展首屏读桩、不 spawn，不阻塞（`aggregator/mod.rs` 模块文档 + `load_one` 的桩分流段）。真正阻塞的是：**首次运行**（无桩）、`frozen=false`** 的扩展**（含声明了 fallback 而被宿主降级者）、以及 in-process 调用卡满 5 s 的内置扩展。所以这不是"所有场景都慢"，而是"新装扩展 / 首启 / fresh 扩展会把首屏拖住"。
- **改法**：`AggregatePayload` 拆成三段式事件流——① `Meta { exts, trust, ledger_state, inproc_specs, skipped }`：扫描阶段就能拿到，**最先发**，让设置页与信任表立即就绪；② `Ext { id, outcome }`：`collect_top_level` 每 `join` 完一个扩展就发一个（由父线程边 join 边发最简单，且天然保序）；③ `Done { agg_ms }`：仅收尾（停加载态、落 `cold` 计时）。UI 侧每收到 `Ext` 就并入 `per_ext` 后**重新调用一次 **`flatten(&arrived)` 再替换列表——`flatten` 是纯函数、扩展数只有个位数，重算是微秒级，**不要写增量合并算法**（那会把 20 行的改动做成 200 行，且引入排序稳定性的坑）。
- **验收**：构造"1 个 `frozen=false` 的 2.5 s 慢扩展 + 5 个 10 ms 快扩展"，首项可交互时间从 \~2.5 s 降到 \< 300 ms；慢扩展到达后列表**增量追加而非重排**（选中项不跳）；再跑一遍"已有桩的 frozen 扩展"回归，确认行为零变化（仍不 spawn）。

### 4.2 非 Windows 分支没有 clippy 门禁

- **证据**：`ci.yml:40` 的 `cargo clippy ... -D warnings` 只在 windows-gnu job 里；linux/macOS job（`ci.yml:77` 注释明说"clippy/fmt 仍以 Windows 门禁为准"）只 build + test。实测 Linux 侧约 32 条 warning（`dd-gui` 16 / `dd-ext` 14 / `dd-host` 1 / `dd-ext-search` 1），全部为 Windows 专属路径在 unix 下变成死代码。
- **影响**：`cfg(unix)`/`cfg(not(windows))` 代码的死代码、未用导入长期堆积；跨平台重构（比如把 `platform/` 再抽象一层）时噪音会掩盖真问题。
- **改法**：linux job 加 clippy（`-D warnings`），先把这 \~32 条清掉（基本都是 `#[cfg(windows)]` 邻近的常量/函数没加同名 cfg）。若担心差异噪音，至少加 `cargo clippy --target x86_64-unknown-linux-gnu` 作为 **non-blocking** 步骤先观察一周，再转阻塞。
- **验收**：三平台 clippy 全绿；新增 `cfg(unix)` 死代码会被 CI 拦下。

### 4.3 三个测试在 root 容器里必挂（环境耦合）

- **证据**：3 个用例用 `chmod 0o555` 注入写盘失败——`settings/tests.rs` 的 r02 与 r17 用例、`dd-host/src/lib.rs` 的同名 r02 用例。root 无视权限位 → 写盘成功 → 断言失败。实测 `561 passed / 3 failed`，**这 3 个就是全部失败**。
- **附带发现**：`atomic_write` 在 `dd-host` 与 dd-gui `settings.rs` 是**两份逐字相同的实现**（注释自述「方案 R-02 口径：两 crate 各一」），测试也相应地写了两份——所以这个缺陷天然会重复出现，修的时候必须两处都改。若想顺带消重，把 `dd-host` 那份从 `pub(crate)` 提为 `pub` 即可（代价：公共 API 面 +1）。
- **影响**：任何用容器/devcontainer 开发的贡献者（以及未来 GitHub Actions 若切到容器 runner）都会看到"红"，然后学会忽略测试——这是最坏的习惯养成路径。
- **改法**：注入点下沉。两处 `atomic_write` 都改为 `atomic_write_with(path, bytes, writer: impl Fn(&Path,&[u8])->io::Result<()>)`，测试传恒返回 `Err` 的 writer；生产路径传真实实现。`save_to` 同理。这样失败注入不再依赖 OS 权限语义，Windows 的 `share_mode(0)` 分支与 Unix 的 `chmod` 分支可以一起删掉。
- **验收**：`cargo test --workspace --no-fail-fast` 在 root 与非 root 下均为 `564 passed / 0 failed`；失败注入不依赖 OS 权限语义（`chmod` / `share_mode` 分支已删除）。

### 4.4 协议解码边界没有 fuzz

- **证据**：`crates/dd-protocol/src/framing.rs` 的 `Decoder` 直接消费**不受信第三方扩展**的 stdout（单帧上限 `DEFAULT_MAX_MESSAGE_BYTES` 1 MiB）；全仓无 `fuzz/` 目录、无 `cargo-fuzz` 配置。
- **影响**：这类代码的 bug 类型是"某段畸形输入导致 panic/超长/状态机错位"，单测只能覆盖想得到的形状。宿主被杀掉 = 用户失去整个启动器。
- **改法**：`cargo fuzz init` + 两个 target：`decoder`（任意 bytes → 只允许 `Ok/Vec<Frame>/Err` 三态，永不 panic）、`envelope`（任意帧 → 解析不 panic）。每天夜间跑 10 分钟，语料入库。
- **验收**：连续 7 天夜间 fuzz 零 crash；`Decoder` 在 ≥ 10^7 次执行后无 panic（OOM 与 `TooLarge` 路径除外）。

### 4.5 没有任何性能门槛

- **证据**：A2 冷启动、A3 过滤耗时都只有 `#[cfg(debug_assertions)]` 的 `log::debug!` 埋点（`state.rs` 的 debug 埋点、今 `aggregate_job.rs` 的计时埋点），无 bench、无 CI 阈值。
- **影响**：性能是 dd-run 的核心卖点（"Fast" 写在 Goals #3），却只能靠人偶尔看日志；一次不经意的 `clone()` 或每帧 `Vec` 分配可以静默吃掉 30%。
- **改法**：`criterion` 三个 bench：`fuzzy::FuzzyMatcher::score`（2000 项 × 8 字符）、`recompute_visible`、`Decoder::push`（1 KiB 行）。CI 只做**趋势回归**：与 main 的基线比 \> 20% 才红（用 `criterion-compare` 或存 JSON 比对），避免机器抖动误报。
- **验收**：3 个 bench 有基线数据；回归超阈值时 PR 上有注释提示；首屏 bench 覆盖 `flatten` + `FuzzyMatcher` 热路径。
- **✅ 落地（2026-10-25）**：`dd-gui/benches/hot_paths.rs`（fuzzy_score_2000 / set_query_2000 / flatten_5ext）+ `dd-protocol/benches/decoder.rs`，本机基线已记录；CI `bench` job（PR-only 观察模式）base vs head 比中位数、>20% 红（`tools/bench_compare.mjs` 绿/红双路自测）。**偏差记档**：`recompute_visible` 为私有方法，公共入口多个（`set_query`/`set_empty_view`/`set_apps_hidden`/`set_steam_hidden`/`reset`），bench 取击键驱动的 `set_query` 承接完整路径；`FuzzyMatcher` 放宽 `pub`（bench target 是独立 crate 触不到 `pub(crate)`）；criterion 0.5.1 + 18 个新包名共 19 包进 lock 仅 dev 面。详见 CHANGELOG「P2-4.5」与 implementation.md 台账同日行。

### 4.6 错误类型是 `String`，用户可见信息无法分类

- **证据**：94 个 `-> Result<…>` 中 **48 个**用 `String` 作错误（排除注释行后精确计数；含注释为 50）（`ext_client::open`、`spawn_and_initialize`、`aggregator::load_one` 等），而 `dd-host` 已经有结构化的 `ProtocolError`。
- **影响**：设置页/Toast 只能显示"某某失败：\<拼接字符串\>"，无法区分"超时 / 崩溃 / 签名无效 / 版本不兼容"，也就无法给出可操作建议（重试 / 去批准 / 升级扩展）。扩展生态越大，这个损失越明显。
- **改法**：只在**三个边界**引入 `thiserror` 枚举（别全仓铺开）：`dd_host::LoadError`（清单/信任/spawn）、`ExtClient::CallError`（超时/进程退出/RPC 错误码）、`SettingsError`（写盘/解析）。UI 层加一个 `fn user_message(&self, lang) -> String` 统一出文案。**供应链代价已核实为零**：`Cargo.lock` 里 `thiserror` 已存在（`1.0.69` 与 `2.0.20` 两个版本都在树内、均为传递依赖），直接依赖 `thiserror = 2` **不新增任何包**，符合本项目「lock 零新增」的既有纪律。
- **验收**：UI 上超时与崩溃显示不同文案与不同操作按钮；`grep -c "Result<.*, String>"` 从 48 降到 ≤ 24。

### 4.7 冻结契约里存在死字段

- **证据**：清单里的 `Manifest.icon`（`manifest.rs` 字段）**无任何读取方**——全仓只有结构体定义与构造时的 `icon: None`，`dd-gui` 从不访问 `manifest.icon`（注意别与 `CommandItem.icon` 混淆：后者是协议命令项图标，`aggregator/items.rs` 的 `flatten` 透传、渲染层正常使用）；`GetItemsResult.has_more_items` 在 `crates/dd-gui/` 中**零命中**（仅 CLI 检查字段存在性，内置扩展恒发 `false`）。
- **影响**：v1.0 是**冻结**契约。冻结面上留死字段，比"没有这个字段"更贵——实现者会照着实现，然后发现宿主不理它。
- **改法**：二选一，别拖。要么实现（`icon` 用于扩展管理卡片，成本很低，`ui/icons.rs` 已有解码链），要么在 protocol v1.1 的演进草案里显式标注 `deprecated`，并在 schema 表加"未实现，勿依赖"的强提示（当前提示藏在字段行尾，容易被略过）。
- **验收**：两个字段要么有消费者 + 测试，要么在文档中被标 `deprecated` 且 `doc_contract` 测试断言这一点。

### 4.8 设置文件没有 schema 版本，降级会静默丢配置

- **证据**：`Settings::parse_json` 逐字段读已知键；`to_json_string` 只写已知键。**未知字段在"读→写"一轮后消失**。也没有 `schema_version`。
- **影响**：用户装了新版（写入新字段）→ 回退旧版 → 旧版保存一次 → 新字段永久丢失；跨版本排查问题时会看到"配置莫名回滚"。
- **改法**：① 加 `"schema_version": 1`；读到 `> 当前` 时进入**只读模式**（设置页顶部黄条"配置来自更高版本，已禁用写入"）；② `parse_json` 保留原始 `serde_json::Map` 于 `Settings.unknown: Map`，`to_json_string` 先写 unknown 再覆盖已知键（未知字段保真）。
- **验收**：写入 `{"future_field":1}` → 保存 → 文件仍含该字段；高版本配置下保存被拒且有 UI 提示。

### 4.9 非 Windows 构建的字体集**必然为空**（核对中得到实证）

- **证据链（前两步已实测，末步待真机）**：
① `crates/dd-gui/Cargo.toml` 的 `eframe` 用 `default-features = false`，注释写明"O5-a：default\_fonts 移除——不再嵌入 egui 内置字体三件套"，并注明"`FontDefinitions::default()` 在无该特性时返回空集"；
② `platform/fonts.rs` 只从 **Windows 系统路径**加载 `msyh.ttc` / `segoeui.ttf` / `seguisym.ttf` / `SegoeIcons.ttf`；全仓 `grep "/usr/share/fonts|/System/Library|Library/Fonts|fontconfig"` **零命中**；
③ `setup_cjk_fonts` 在 `!any_loaded` 时只记一条 `debug` 日志然后 `return None`，调用方沿用空的默认字体集（`platform/fonts.rs` 的 `setup_cjk_fonts` 空集回落段）。
④ **实测补充（本轮新增）**：在本机跑探针 `egui::FontDefinitions::default().font_data.len()`，结果 **0**（families 只有空壳 `[Proportional, Monospace]`）；且 `setup_cjk_fonts` 第 67 行 `ctx.set_fonts(fonts)` **无条件执行**——就算默认集非空也会被这个空集覆盖。前三条 + 实测 ⇒ **非 Windows 上字体集必然为空**，这一步已经不是「很可能」而是「确定」。仍未验证的只剩「egui 在空字体集下的具体视觉表现」（空白 / 方块 / 不显示文字），需要真机窗口。
- **影响**：README 把 macOS/Linux 描述为"CI-gated previews（同代码库、降级运行）"，读者会理解为"功能少但有界面"。字体集为空意味着**界面文字不可读**——这不是 preview，是不可用。CI 也发现不了：`cargo test` 不渲染，`build` 不显字。（`cargo test` 里那个探针也不会失败，因为没人断言过字体非空——这正是缺一条断言测试的典型后果。）
- **改法**：① 非 Windows 走系统字体发现（Linux：fontconfig / 扫 `/usr/share/fonts` 取 Noto Sans CJK；macOS：`/System/Library/Fonts/PingFang.ttc`）；② 找不到时回落到内嵌 subset（拉丁 + 常用 CJK，1–2 MB，**只进非 Windows 产物**）；③ 把 `any_loaded == false` 从 `debug` 提为 `warn`，并让设置页「关于」显示当前字体来源。**与既有决策 O5-a 不冲突**：O5-a 移除的是三件套全量字体（Windows 产物 −1.35 MiB，Windows 有 msyh/segoeui 兜底）；这里只在**非 Windows 产物**里内嵌 subset，两者体积预算互不影响。若坚持「任何平台都不内嵌」，那 Linux/macOS 就必须依赖用户机器装有 CJK 字体——可作为备选，但要写进 README 的运行时依赖。
- **验收**：① 单测断言「`FontDefinitions` 经 `setup_cjk_fonts` 后含 ≥1 个字体」（当前必然失败，可作为红灯起点，写法与本次探针相同）；② Linux 真机启动面板，中文标题与命令名可读（截图存 `docs/`）；③ 若选择内嵌子集，非 Windows 产物体积增量记录在 README。
- **工作量**：1 天（系统字体发现）或 2 天（含内嵌子集兜底）

---

## 5. P2：值得排期但不紧急（7 项）

| # | 项 | 证据 | 建议与验收 |
| --- | --- | --- | --- |
| 5.1 | **无使用频次（frecency）** | 全仓无 frecency/usage 记录；`recompute_visible` 只按模糊分排序（`state.rs` 的 `recompute_visible`） | 启动器的第一体验杠杆。`usage.json` 存 `{id: {count, last_ts}}`，空查询按 `count` 降序、有查询时作同分 tiebreak。验收：连续使用一周后常用项在空查询 Top5 命中率 ≥ 80%；提供"重置使用数据"。**✅ 落地（2026-10-25）**：`usage.rs` + `PanelState::set_usage` 快照注入 + `confirm_selected` 记录 + 设置页重置卡；排序判据仅 count（`last_ts` 留作 recency 衰减观测）；记录点 = 激活中枢（Root/嵌套页 × 键盘/鼠标）；`usage=None` 与引入前逐字节一致（金图 6/6 不变）；验收「一周 Top5 ≥80%」由结构测试四条承接、真机腿留给日常使用——详见 CHANGELOG「P2-5.1」与 implementation.md 台账同日行 |
| 5.2 | **无自动更新/版本检查** | 便携 zip 分发，无更新链路；用户只能自己去 Releases 看 | 设置页加"检查更新"（读 GitHub Releases API，可关）。**不要做自动安装**——便携版静默替换 exe 风险大于收益。验收：点击后 3 s 内显示"已是最新/有 vX.Y.Z"。**✅ 落地（2026-10-25）**：`update_check.rs`（GitHub Releases `latest` 比对）+ 系统 curl（零新依赖）+ 设置页「关于」卡（版本 + 按钮 + 「启动时检查」开关默认关——"可关"落实记档）；预发布后缀比较忽略（近似记档）；真机腿达成：本机 curl 实测 1.05 s 返回 v0.2.1 → UpToDate，`--max-time 3` 硬约束背书「3 s 内显示」；详见 CHANGELOG「P2-5.2」与台账同日行 |
| 5.3 | **日志不落盘、无诊断包** | 日志后端恒写 stderr（`dd-protocol/src/logging.rs` 的日志后端），而 release 版是 `windows_subsystem = "windows"`（无控制台）→ 用户双击启动时日志**既看不见也不留存**；只有 `panic.log` 落盘 | ① 默认落盘 `%APPDATA%\dd-run\logs\dd-run.log`（滚动 5×1 MB，落盘级别默认降到 `Info`——当前默认是 `Debug`，直接落盘会涨得很快）；② 设置页加「打开日志目录」；③「导出诊断包」：设置（脱敏）+ 扩展清单与信任判定 + 最近 200 行 → 单个 zip。扩展作者提 issue 直接附这个文件，省掉一半来回 —— **✅ 落地（2026-10-24，①②③ 见附录 C；③ zip → 单 JSON 偏差记档）** |
| 5.4 | **扩展开发无脚手架** | `examples/python-minimal` 要手工拷贝改 | `dd-run-cli ext new <id>` 生成 manifest + 最小实现 + `--conformance` 断言；再加一个可复制的 GitHub Action 模板（跑 conformance）。**注意现有 CLI 是手写 flag 解析**（dd-run-cli `main.rs` 的 `match arg.as_str()`，非 clap），加子命令要扩这段 `match`（约 30–50 行），**不要为它引入 clap**——会破坏本项目的 lock 零新增纪律。验收：零基础用户 10 分钟内产出通过 conformance 的扩展。**✅ 落地（2026-10-25）**：flag 式 `--ext-new <ID>`（偏差记档：CLI 全 flag 解析器，子命令语法不适配既有风格）；生成物 7 件含 install.py 绝对路径安装器与 conformance.yml CI 模板；**关键发现**：F4 口径 Windows 只补 .exe 不补 .cmd（首版实跑 os error 193 暴露）——Windows 宿主启动走 install.py（pymin 既有模式）；单测 +3 含「生成物经 dd_host 真实扫描器加载」；conformance 端到端实跑待真 Python 环境（本机仅 Store stub），「10 分钟」验收随腿 |
| 5.5 | **缺社区基建** | `.github/` 只有 `dependabot.yml` + 两个 workflow；无 CONTRIBUTING / SECURITY.md / issue 模板 / CODEOWNERS | 协议是公开契约、README 面向扩展作者，却缺"怎么贡献"和"漏洞怎么报"。半天可补齐 |
| 5.6 | **依赖许可证无门禁** | CI 只有 `cargo audit`（漏洞）+ `cargo machete`（死依赖），没有 `cargo deny`；项目全仓声称 MIT，但三方依赖的许可证从未被机器校验过 | 加 `cargo deny check licenses bans`（`deny.toml` 允许 MIT/Apache-2.0/BSD/ISC/Unicode-3.0，其余逐条审批）。验收：`cargo deny check` 进 CI 阻塞门禁；许可证清单落入 `docs/` |
| 5.7 | `.cargo/config.toml`** 硬编码 windows-gnu** | `config.toml:15` `target = x86_64-pc-windows-gnu`；CI 两处用 `CARGO_BUILD_TARGET` 覆盖（`ci.yml:87,125`） | 改成不 pin 目标，另给 `[alias] build-win = "build --target x86_64-pc-windows-gnu"`。收益：非 Windows 贡献者 `cargo test` 开箱即用；代价：Windows 本地开发要记一个别名（可写进 CONTRIBUTING）。**✅ 落地（2026-10-25）**：去 pin 完成，CI 三处 `CARGO_BUILD_TARGET` 保留为显式声明（注释更新）；**偏差**：alias 未加——1.95.0-msvc 交叉 windows-gnu 缺 dlltool 必挂（B2 记档），Windows 本机机制 = `RUSTUP_TOOLCHAIN=stable-gnu` 工具链覆盖（与 CI Windows job 同款，CONTRIBUTING 已改写）；package.sh Windows 分支产物路径实证不变。CI 覆盖处数记档更正：落地时为 **3 处**（linux/macos/bench，A3 后增加）而非本表所写 2 处 |

另有两条**先测再决定**，不要凭感觉做：

- **列表虚拟化**：`ui/panel.rs` 用 `ScrollArea::show`,逐行 `draw_item_row`，无 `show_rows`. 但真实上限是 400 应用 + 30 文件结果（dd-ext `search.rs` 的 `RESULT_LIMIT`），当前规模大概率不是瓶颈。**先按 §4.5 加 bench，超过 8 ms/帧再上虚拟化**，否则是为了不存在的问题付出复杂度。**→ 判定（2026-10-29）：虚拟化永不立项、正式关闭**——`hot_paths.rs` 三组实测（430 行夹具，kittest 单帧）：逐行绘制成本（布局分解组，零渲染 = 虚拟化 `show_rows` 可节省上界）median **3.88 ms ≤ 8 ms**；硬件判定组全帧 9.48 ms（本机独显 Radeon RX 580；超阈段 ≈5.6 ms = tessellation + 可见像素 GPU 光栅化 + kittest 回读——虚拟化无关的固定段）；软件组全帧 39.94 ms（kittest `default_wgpu_setup` 全机器显式优先 CPU 软件适配器，快照确定性设计——初稿「本机 GPU」前提据此修正）。实测数字与适配器记档 [implementation.md](./implementation.md) §3.1 末行。
- **i18n 双形态统一**（原「内联 393 处」表述已修正）：dd-gui **已是 key 制**——`app/mod.rs` 的 `fn tr(&self, key: &'static str)` + `text.rs` 约 194 条 `(key, zh, en)` 键表（调用点仅 59 处）；真正的内联双语 `tr(zh, en)` 只在 **dd-ext**（`dd-ext/src/i18n.rs` 定义，107 处调用）。加第三种语言的实际改动面 = dd-gui 键表 194 条 + dd-ext 内联 107 处。**等第三种语言立项时再统一**：dd-ext 抽成 key 表（直接复用 dd-gui 形态），届时用 `include_str!` 的 JSON 表 + 一个 tools/i18n_check.mjs（拟新建）查漏。两语言现状下不动作。

---

## 6. 落地路线（四批，每批都可独立合并）

### 批次 A：地基（约 3 天，零行为变更）——✅ 已落地（2026-10-24，见附录 C）

| 步骤 | 内容 | 验收 | 状态 |
| --- | --- | --- | --- |
| A1 | `rust-toolchain.toml` + workspace 版本/许可证继承 | 干净机器 `cargo test` 零手动；5 个产品 crate 版本一致 | ✅ |
| A2 | release 加 `SHA256SUMS.txt` + build provenance | Release 产物可校验 | ✅（CI 行为待下次 tag 验证） |
| A3 | CI：linux clippy 转阻塞（先清 \~32 条 warning） | 三平台 clippy 全绿 | ✅（实清 41 条；linux + macOS 双目标实测 0） |
| A4 | `atomic_write_with` 注入点改造 | root/非 root 测试结果一致 | ✅ |
| A5 | CONTRIBUTING + SECURITY + issue 模板 | — | ✅（CODEOWNERS 单维护者仓不做） |

### 批次 B：治盲区（约 8 天，最高回报）——✅ B1/B2/B3/B4 全部落地（2026-10-24，见附录 C）

| 步骤 | 内容 | 验收 | 状态 |
| --- | --- | --- | --- |
| B1 | `egui_kittest` 6 张金图（**注入固定测试字体**，任意平台可复现）+ CI snapshot job | 故意改行高必须失败；换机器重跑不变 | ✅（6/6 张全落地：暗/亮空查询、有结果+选中、嵌套页、设置-外观、右键菜单；两轮比对全过。字体方案改为 dev 面重开 eframe 内置字体——无 CJK 资产可内嵌，中文方块但逐字节确定；CI job 观察模式待首绿转阻塞；**自证实验已做（2026-10-24）**：`ROW_H` 40→41 后 5/6 金图即红（含列表行的全部命中；settings_appearance 无列表行、通过是正确行为），还原后 6/6 绿——金图对 1px 级布局回归的检出力实证） |
| B3 | `docscan --write-index` 自动回填 | 改文档不再手改 INDEX | ✅ |
| B4 | CHANGELOG 交给 `git-cliff`（提交已是 Conventional Commits），保留人手写的 Release Highlights | CHANGELOG 由 CI 生成，人只写 5 条亮点 | ✅（**范围记档**：生成物进 Release body（亮点人工 + 提交明细自动）；仓库内 CHANGELOG.md 是人工详账、与 implementation.md 联动，**不**被生成物覆盖——与分析文档原意不同，原案未考虑本仓 CHANGELOG 兼作台账） |
| B2 | `doc_contract.rs`：24 处现状注断言化 + 去行号锚 | 现状注 100% 有机检；`grep "\.rs:[0-9]" docs/` 归零 | ✅（2026-10-24 收尾）：① 行号锚**全清**——契约文档 16 处全部改「文件 + 符号名」（含 manifest-schema「静默丢弃」过时表述一并校正），docscan 新增第④项机检「契约文档出现 `.rs:行号` 即 fail」（仅三份契约文档生效，其余 ~77 处第二阶段清理后扩面）进 CI 门禁；② **断言化两批共 22 条**（首批 dd-host 3 条 + 第二批 19 条，分布：dd-protocol 5 / dd-ext 3 / dd-host 6 / dd-gui 8）——✅ 类 5/5、⚠️ 类 16/19 有运行时断言，余 3 处标注原因的 `no-assert`（负向结构性断言无载体），验收「两类各 ≥ 70%」达成；锚点就近私有函数不提可见性；protocol.md §7.2 注经核实仍准确，未发现新漂移 |

### 批次 C：结构（约 8 天，必须排在 B 之后）——✅ 全部落地（2026-10-24，见附录 C）

| 步骤 | 内容 | 验收 | 状态 |
| --- | --- | --- | --- |
| C1 | `PaletteApp` **六片**拆分，按 5→11→18→13→22→26 分批提交（944 处引用改写） | 直接字段 ≤ 8；每批独立可编译；测试数不减 | ✅（2026-10-24）：六批全落地（Overlay5 / Hotkey11 / Draft18 / UiView13 / ExtPool22 / Window26，并集=全集 95；C2 起 ExtPool +1 = `arrived` 流式在途集 → 23，总 96）；**计数更新（2026-10-25 复核轮）**：P2-5.1/5.2 又给 UiViewState 加 `usage_scores`/`update_rx`（→15）、给 ExtPool 加 `usage`（→24），六片合计 **99**——直接字段仍 = 6，拆片纪律未破；650 passed / 0 failed（数不减）；`git diff -w` 抽查 = 纯前缀插入；金图 6/6 逐字节不变（像素级零行为证明）；未新增 `pub(crate)`；同名字段误伤三处（FallbackStore.exts / ExtRow.trust / PanelState.visible）均被编译器当场拦下——「越界访问由编译器挡」实证 |
| C2 | 流式聚合（`AggregateEvent`） | 慢扩展场景首项 \< 300 ms | ✅（2026-10-24）：三段式 Meta→Ext×N→Done 落地；Ext 完成序回传（快扩展先回先渲染）；加载骨架只挡零项期；选中按 (ext_id,id) 身份恢复；旧轮进程在首个结果到达时替换（无空窗）；结构化测试 3 条 + 金图 6/6 逐字节不变 + frozen 桩锚点回归全绿；「<300ms」由机制保证 + 结构断言承接（不引慢扩展夹具进 CI） |
| C3 | 三边界 `thiserror` 错误类型 | 超时/崩溃文案与操作分离 | ✅（2026-10-24）：`LoadError`（Display 逐字一致、open 链路换型、测试结构化匹配）+ `ProtocolErrorKind`（分类在产生点，invoke toast 按类分流：超时可重试 / 崩溃回落 stub）+ `SettingsImportError`；thiserror 2 已在树 → lock 零新增；只动三边界（48→42，余量非边界路径） |
| C4 | settings `schema_version` + 未知字段保真 | 降级不丢配置 | ✅（2026-10-24）：写盘戳 `schema_version:1`、更高版本 → 只读模式（保存拒 + toast）；`Settings.unknown`（JSON 编码文本）读→写保真（类型还原、往返稳定）；验收测试 4 条 |
| C5 | 协议死字段收口（实现或废弃） | icon / has\_more\_items 各有结论 | ✅（2026-10-24）：`icon` → **deprecated**（§5 强提示 + doc_contract 锚：原样保留不展开，启用消费即红）；`has_more_items` → **保持**（协议语义 + CLI conformance 校验，宿主不消费是 §6.3 既定口径，非死字段） |

### 批次 D：跨平台补正（约 1 天，与 A/B 并行）——✅ 已落地（2026-10-24，见附录 C）

| 步骤 | 内容 | 验收 | 状态 |
| --- | --- | --- | --- |
| D1 | 非 Windows 系统字体发现 + 空字体时 `warn` | 单测断言 `font_data` 非空；Linux 面板中文可读 | ✅（回归锚单测已加；Linux 真机截图待用户腿） |
| D2 | README 平台口径修正：preview 到底"少功能"还是"不可用"要说清 | README 三平台能力表与实际一致 | ✅（补字体运行时依赖说明） |

> **总量**：A 3 + B 8 + C 8 + D 1 ≈ **20 个工作日**（单人、含自测，不含评审与回归）。这个数字比「四个批次」听起来大，但它对应一次性清掉 §3 + §4 这 13 项结构性问题（P2 的 7 项按需插队）——按项目现在 120 commits/月的节奏，不还的话，这些债会以「每次改 UI 都要手测三平台」「每次改协议都要人工对文档」的形式持续扣费。
>
> 顺序很重要：**B 在 C 之前**。没有金图和契约测试就做结构重构，等于在没护栏的路上换轮胎。
> D 可以随时插队——§4.9 的「字体集为空」已由本轮探针**实测确定**（不再是推测），建议直接按 P0 处理：一个「跨平台」项目的主打卖点在两个平台上无法显示文字，优先级高于任何重构。唯一待验的只是视觉表现形态（空白 / 方块）。

---

## 7. 不建议做（避免过度工程）

| 不做 | 理由 |
| --- | --- |
| 拆更多 **crate** | 6 个 crate 的边界已经合理（protocol/host/ext/gui/cli/sample）。再拆只会增加 `Cargo.toml` 与可见性税（注意：§3.1 拆的是 **struct**，不是 crate，两者不冲突） |
| 引入 async runtime（tokio） | 当前"线程 + mpsc + 超时"模型与 egui 的同步帧循环天然契合，19 处 `thread::spawn` 全部有明确生命周期。tokio 会换来 Send/Sync 边界与更大二进制，收益不明 |
| WASM 沙箱 | 已是显式 non-goal。真要做安全隔离，先做 §4.6（错误分类）+ 子进程资源限额，性价比更高 |
| SQLite 存设置/缓存 | 单文件 JSON + 原子 rename 足够，且便于用户手改与备份。SQLite 会引入迁移与并发问题 |
| 第三种语言 | 第二语言的内联 `tr()` 已经能跑；在多语言需求被真实用户提出前，目录化是净负债 |
| 全仓 `Result` 类型重构 | 只在三个边界做（§4.6），全仓铺开是高风险低回报 |

---

## 8. 一句话总结

dd-run 现在不缺工程能力，缺的是**把"人肉纪律"换成"机器纪律"**：文档 → 契约测试，走查 → 快照，人工计时 → bench，口头版本一致 → workspace 继承。做完这几件，120 commits/月 的节奏才可持续。

---

### 附：本次分析可直接复现的命令

```bash
git clone https://github.com/Yang-SH/dd-run.git && cd dd-run
rustup toolchain install 1.95.0 --profile minimal
rustup component add --toolchain 1.95.0 clippy rustfmt

# 编译与告警（非 Windows 视角）
CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu cargo +1.95.0 check --workspace --all-targets

# 测试（root 下会有 2 例因权限注入失效而失败，见 §4.3）
CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu cargo +1.95.0 test --workspace
```

---

## 附录 B：核对记录（累计三轮）

四轮核对累计修正 19 处 + 口径注记 1 处，全部有仓库内证据；下表只列「改了什么、为什么」（第四轮见文末）。

| # | 位置 | 原写法 | 修正为 | 依据 |
| --- | --- | --- | --- | --- |
| 1 | §1 表 | 50/94 个 `Result` 用 `String` | 48/94 | 原 grep 把注释行算入；排除后精确 48 |
| 2 | §4.5 | 埋点落点 | 今 `aggregate_job.rs` 计时埋点 | 原锚 32 行是注释，`Instant::now()` 在 57 行 |
| 3 | §4.1 | `initialize` 5 s | 今 `process.rs` 超时常量 | 原锚 36 行是节标题注释、37 行为常量 |
| 4 | §3.4 | 6 个 Cargo.toml 重复版本 | 5 个产品 crate（+ 独立 1.0.0 协议线） | `dd-protocol` 不随产品版本走 |
| 5 | §1 表 | docs 981 KB | 981 KiB ≈ 1.0 MB | `du -b` = 1,004,658 B |
| 6 | §3.2 | 金图跑 Linux（lavapipe） | 金图注入固定测试字体，任意平台 | 字体全为 Windows 系统字体，Linux 无字形（§4.9） |
| 7 | §3.2 | 金图 ≤150 KB | ≤300 KB | 650×440 暗色面板 PNG 实测区间 100–300 KB |
| 8 | §3.1 | 四片拆分 / 1–2 天 / 字段 ≤12 | 六片（26+22+18+13+11+5=95）/ 3–5 天 / ≤8 | 实测 **944 处** `self./app.` 引用需改写 |
| 9 | §4.1 | 「一个扩展慢 2.5s 首屏就空 2.5s」 | 补前提：frozen+有桩不 spawn；阻塞仅首启/无桩/frozen=false | `load_one` 桩分流段（原 660–672 行） |
| 10 | §4.1 | 单扩展最坏 3 s | 5 s + 3 s ≈ 8 s | `TIMEOUT_INITIALIZE` 5000 + `TIMEOUT_TOP_LEVEL` 3000 |
| 11 | §6 | 批次 B 约 4 天 / C 约 5 天 | B 约 8 天 / C 约 8 天 | 与各子项工作量求和一致（原值自相矛盾）；B 后续又因 §3.3 从 2 天改为 4 天而加到 8 天 |
| 12 | §5.7 | CI 三处覆盖 target | 两处（`:87` linux / `:125` macOS） | `grep -c` = 2 |
| 13 | §1 | 「通读 107 个文件」 | 「全量静态检索 + 精读核心路径」 | 避免夸大工作量表述 |
| 14 | §1 / §4.3 | 283 passed / 2 failed，「其余 crate 全绿」 | **561 / 3**（`--no-fail-fast`） | 不加该参数时 cargo 在 dd-gui 首个失败即中止，dd-host / protocol / cli 根本没跑；第 3 个失败在 `dd-host` 的同名 r02 用例，根因相同 |
| 15 | §3.3 | 「1 处漂移 / 24 处注 / 2 天 / 全 docs 行号归零」 | 2 处确证漂移；行号锚 **93 处**（契约文档 16）；**4 天**；验收改为「契约文档 16→0 + docscan 禁行号」 | `grep -c '\.rs:[0-9]' docs/*.md` = 93；`protocol.md:295` 引用的 394/411-435 已失效，真实在 662/685 |
| 16 | §4.9 | 「静态推导，很可能」 | 「**默认字体集实测为 0**，空集必然」 | 本机探针 `FontDefinitions::default().font_data.len() == 0`；且 `setup_cjk_fonts` 第 67 行 `set_fonts` 无条件执行 |
| 17 | §3.2 | 「build\_eframe 存在」 | 补 `CreationContext.egui_ctx` 实证 | eframe 0.36 的 `CreationContext::egui_ctx` |

**第四轮（2026-10-24，Windows 本机逐项复核）修正 2 处 + 注记 1 处**：

1. **§5 i18n 前提重写**——「393 处 `tr("中文","English")` 内联」失实：dd-gui 已是 key 制（`app/mod.rs` 的 `fn tr(&self, key)` + `text.rs` 键表约 194 条 `(key, zh, en)`，调用点 59 处），内联双语 `tr(zh, en)` 仅在 dd-ext（`dd-ext/src/i18n.rs`，107 处调用）；393 ≈ 194×2 的字符串数，不是调用点数。
2. **§4.9** `ctx.set_fonts(fonts)` 行号 66 → **67**（「无条件执行」结论不变）。
3. **§3.1 口径注记**——944 为字段引用口径；全量 `self.`/`app.` 出现 1,613 处（含方法调用）。

本轮复核确认成立（可放心引用）：107 文件 / 48,159 行、95 字段（含 3 个跨行声明）、631 `#[test]`、395 包、行号锚 93 处（排除本文档自身）/ 契约锚 16 / 现状注 24、`Result<_, String>` 非注释 48 处、两份 `atomic_write` 除可见性外逐字相同、`PaletteApp::new` 恰 8 参数、`release.yml:36/115/151` 三处均只读 dd-gui 版本、§4.1 / §4.3 / §5 全部行号锚。

**第五轮（2026-10-25，批次 A–D + P2 全部落地后的全量复核）修正 1 处 + 澄清 2 处**：

1. **§6 C1 字段计数更新**——P2-5.1/5.2 落地后 UiViewState +2（`usage_scores`/`update_rx` → 15）、ExtPool 再 +1（`usage` → 24），六片合计 **99**（原记 96 为 C2 时点口径）；直接字段仍 = 6，拆片纪律未破（已在 C1 行就地更正）。
2. **澄清：B2「22 条」口径成立**——本轮复核一度误报「实测 21 vs 声明 22」，实为把 C5 的 icon deprecated 锚（1 条）误计入 B2、又漏数 dd-gui 在 `aggregator.rs` 就近的 2 条；真实构成 = B2 锚 22（protocol 5 / ext 3 / host 6 / gui 8，gui 含 aggregator 就近 2）+ C5 锚 1 = **23 个测试**，与附录 C 各行记档及各 crate 文件实数逐一相符，无需改数。
3. **澄清：dd-host `tests/doc_contract.rs` 头注释**——「覆盖 6 条」指 B2② 现状注锚，文件实际含 7 个测试（多出的是 C5 icon 锚，头注释 no-assert 台账本已记）——已在头注释补歧义澄清，并顺手补记 dd-gui「2 条就近放 aggregator」的指引。

门禁复测（本轮）：`cargo test --workspace --no-fail-fast` **679 passed / 0 failed / 7 ignored**；clippy `-D warnings` 零告警；fmt 零差异；docscan 四项全绿。随轮修文档缺口：README 中英新增「用户数据集中一处」条目（config.json / usage.json / extensions.d/ / cache/ / logs/ 三平台路径，与 `dd_run_dir` 实现逐字对照）、package.sh README-dist 模板同步（usage.json 入列、logs/ 描述补运行日志——P2-5.3 后不再只有 panic 取证）。

**第二轮新增 4 项**（原报告遗漏）：

1. **§4.9 非 Windows 零字体**——三条静态证据 + 本轮实测（`FontDefinitions::default().font_data.len() == 0`），已从「很可能」升级为「确定」；建议按 P0 处理。
2. **§5.6 无许可证门禁**——只有 audit + machete，`cargo deny` 缺位。
3. **§5.3 日志不落盘**——stderr 后端 + 无控制台的 release 构建 = 用户机器上日志既看不见也不留存，原报告只提了「诊断包」没提根因。
4. **§3.1 死字段排查**——曾怀疑 `native_resize_since` / `drag_candidate` 是死字段，经 `grep` 证实由 `app.<field>` 访问（`ui/chrome.rs` 访问段），**不是死字段**，未写入报告（记录在此，避免后续重复怀疑）。

**核对时确认成立的（可放心引用）**：95 字段（含跨行的 `capture_rx`）、944 处引用、24 处「实现现状」注、631 tests（静态）/ 564 可执行（Linux）、`ui/` 7,316 行 / 30 tests、32 条 Linux warning、`manifest.rs` `skipped.push` 与 manifest-schema 旧「静默丢弃」表述的矛盾、`build_eframe` API 存在性、`PaletteApp::new` 的 8 参数纯注入签名、`thiserror` 已在 lock。

**仍未核实（需要 Windows 或图形环境）**：产物字节数、Windows 专属用例真实通过率、DWM 材质与托盘真机行为、§4.9 空字体集下的**具体视觉表现**（字体集为空已实测确定，只剩「看起来是空白还是方块」未验）。

---

## 附录 C：落地记录（批次 A + B + C + D，2026-10-24）

批次 A（A1–A5）、B（B1–B4）、C（C1–C5）、D（D1/D2）按本报告顺序全部落地，均在本机（Windows）工作副本完成，**未提交**（遵循「AI 只在工作副本改」约定）。批次 B1/B2/B3/B4 与 C1–C5 的落地行见下表末尾（B2 第二批与 C 批为 2026-10-24 同日续批）。

| 项 | 落地要点 | 与原案的偏差 |
| --- | --- | --- |
| A1 | `rust-toolchain.toml`（channel 1.95.0 + clippy/rustfmt）；根 `Cargo.toml` `[workspace.package]` + 5 产品 crate `*.workspace = true`（`dd-protocol` 独立 1.0.0 + 注释）；**`release.yml` 三处 tag 校验改读根 `Cargo.toml`**（原案未覆盖的连带点：继承后 crate 内清单不含字面 version，原 grep 会取到 `workspace` 字样） | CI **未**「去掉手写 rustup install」：`rust-toolchain.toml` 优先级高于 `rustup default`，直接采信原案会把 Windows job 切到 msvc 宿主、失去 gnu self-contained 链接器——改为三 job 用 `RUSTUP_TOOLCHAIN` 显式覆盖，工具链选择零变化 |
| A2 | `release.yml` 新增 `release-sums` job：needs 三平台构建 job → 下载全部 Release 资产 → 统一 `SHA256SUMS.txt`（`sha256sum -c` 可验）→ `actions/attest-build-provenance@977bb373…`（v3，按 commit SHA 钉扎，与仓内 action 纪律一致）→ 附着 | 集中在一个汇总 job 而非各构建 job 内散算——保证单一 SHA256SUMS.txt 覆盖全部资产 |
| A3 | 实清 **41 条**（分析估算 ~32 条偏少）：Windows 专属项补 `#[cfg(windows)]`；被普通 `#[cfg(test)]` 测试引用项用 `#[cfg(any(windows, test))]`（bookmarks 解析四件套 / `combine_filetime` 等）；`MAX_SUMMARY` 一度误判为 `any(windows, test)`——其测试嵌套在 windows 门控块内，实归 `#[cfg(windows)]`；`hotkey.thread_id` 字段用 `cfg_attr(not(windows), allow(dead_code))`（构造点过多，逐点门控不划算）。`check-linux` / `check-macos` 均加 `clippy -D warnings`（两目标跨平台实测 0） | macOS 也加了门禁（原案只提 linux；aarch64-apple-darwin 实测 0 告警后一并收） |
| A4 | dd-host / dd-gui 各拆 `atomic_write_with(path, bytes, writer)`，生产路径 `atomic_write` = `atomic_writer`（`std::fs::write`）包装；r02/r17 三个测试改恒 `Err` writer 桩并断言「writer 收到 `.tmp` 路径 / 旧内容完整 / 无残留」；`share_mode(0)` / `chmod` 分支删除 | `save_to_with` 标 `#[cfg(test)]`（纯测试缝，生产 lib 不编译） |
| A5 | CONTRIBUTING.md（工具链 / 三关逐字命令 / lock 零新增 / CRLF 纪律）、SECURITY.md（GitHub 私密报告 + 信任模型简述 + 支持版本）、3 个 issue 模板 | CODEOWNERS 不做（单维护者仓无意义） |
| D1 | `fonts.rs` 新增非 Windows 系统字体发现（发行版/用户目录递归 ≤512 文件；CJK 文件名命中优先至多 3 个；无 CJK 回落任意系统字体 + warn；空集 warn）+ semibold 语义族显式注册；空字体降级日志 debug → warn；新增回归锚单测 `d1_font_definitions_nonempty_on_all_platforms`（Windows 实测过） | 内嵌字体子集兜底**未做**（无可用字体资产；「若选择内嵌，体积增量记录在 README」分支随之搁置）——Linux/macOS 依赖系统字体，已写入 README 与 README-dist |
| D2 | README 中英 §2 段 + 非 Windows 包表格行 + `package.sh` README-dist 模板：补「界面文字用系统字体、中文需 CJK 字体（建议 fonts-noto-cjk）」 | — |
| B1 | dd-gui dev-deps `egui_kittest 0.36`（wgpu+snapshot+eframe，lock **+93 包全在 dev 面**）+ eframe 重开 `default_fonts`；快照模块 `src/snapshot_tests.rs`：确定性三前提 = egui 内置字体（不走 `setup_cjk_fonts` 系统栈——仅 bin 入口调用，直构 PaletteApp 不经过）/ `ui_animations=false` / 聚合通道预发完整 payload（免 Loading 逐帧变化）；首批 2 张金图（暗/亮空查询 650×440）本机出图后两轮比对全过；CI `snapshot` job（windows、`-- --ignored`、**continue-on-error 观察模式**）；快照测试标 `#[ignore]` 不进常规三关 | 与原案三处偏差：① 字体方案——原案「内嵌 Noto Sans SC 子集」无可获取资产，改为 dev 面重开 eframe 内置字体（确定性更强，代价 = 金图中 CJK 呈方块）；② 快照 `#[ignore]` + 专用 job——原案未考虑 linux runner 无 GPU 时常规 `cargo test` 会 panic；③ CI 观察模式——WARP 与本地 GPU 像素一致性未实测，首绿后转阻塞。**实现踩坑记档**：夹具两行重复 widget ID 会把 egui debug 红字告警画进金图；面板构造点 `visible=false` 须显式置真（否则纯黑全图） |
| B3 | `tools/docscan.mjs` 新增 `--write-index`：行数漂移自动回填 INDEX 行数列后重校验（「文件不存在」仍 exit 1）；CI 门禁不带参数（严格模式不变）；CONTRIBUTING 补提示 | 与原案一致；自测时误用 `git checkout --` 回退临时行、连带抹掉本会话 CHANGELOG 条目——当场逐字节重写恢复，事故记档 implementation.md §7 |
| B1 余量 | 补齐 4 张金图：results_selected（5 项 + `state_mut` 首帧后摆选中——**payload 首帧重置 Root 列表，首帧前摆的会被覆盖**，首版两图逐字节相同即踩中）/ nested_page / settings_appearance / ctx_menu（直构 `CtxMenuState`，anchor 固定窗口坐标）；六图两轮比对全过 | 快照模块重构出 `payload_items` + `configure` 双参夹具；时序陷阱记入模块文档 |
| B4 | 根 `cliff.toml`（Conventional Commits → 七分组 + 其他，Tera 模板）+ release.yml：Release body 由静态 `body:` 改为 `body_path` 生成文件——人写亮点 heredoc + `git-cliff --unreleased --strip all` 提交明细（失败不阻塞发版）；checkout 加 `fetch-depth: 0`（浅克隆无 tag 历史，明细必空——原案未覆盖的连带点） | **范围偏差**：生成物只进 Release body，仓库 CHANGELOG.md 人工详账不覆盖 |
| B2 第二批 | 现状注断言化收尾：19 条新锚点分布四 crate（dd-protocol 5 / dd-ext 3 / dd-host +3 / dd-gui 8——dd-gui 侧 6 条入新建 `src/doc_contract_tests.rs`，2 条就近放 aggregator 测试模块）；`test_support` 增 `exiting_ext/exiting_process`（退出码可参数化，§11 两支口径共用）；三份 doc_contract 文件头部各带「已覆盖 / no-assert」台账（测试名 ↔ 文档条目双向可查） | 与原案偏差：① 原案只设想 dd-host 一处 `doc_contract.rs`，实际按「谁的行为谁断言」分四 crate（协议形状断言放 dd-protocol、运行时行为放 dd-ext、GUI 编排放 dd-gui）；② 私有函数（`load_one`）锚点**就近**同文件测试模块而非提 `pub(crate)`（与 C1 纪律一致）；③ 环境记档：本机钉链 `1.95.0`（msvc 宿主）交叉 windows-gnu 缺 `dlltool`，测试一律 `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`（与 CI Windows job 同款覆盖） |
| B1 自证 | `ROW_H` 40→41（仅实验，跑完还原）→ 含列表行的 5/6 金图即红（settings_appearance 无列表行、通过是正确行为）→ 还原 6/6 绿——验收「改行高必须失败」实证 | 专门过滤 snapshot 测试隔离验证金图门禁本身（常规单测含 D8 行高断言但不跑渲染） |
| C1 | 六批依次落地（5→11→18→13→22→26，并集=全集 95）；每批「定义迁移 + 构造器嵌套字面量 + 全仓前缀改写（perl 单行 + mlfix 脚本收跨行链式 17 处）」；直接字段 95→6；`git diff -w` 抽查 = 纯前缀插入；**金图 6/6 逐字节不变**（像素级零行为证明）；未新增 `pub(crate)` | 三处同名字段误伤（`FallbackStore.exts` / `ExtRow.trust` / `PanelState.visible`）被编译器 E0609 当场拦下逐一回退——「越界访问由编译器挡」实证；`settings` 移入 draft 后构造器 `lru` 行 use-after-move——提前取 `warm_capacity` 局部 |
| C2 | 三段式 `AggregateEvent`（Meta→Ext×N 完成序→Done）；`collect_top_level_streaming` mpsc 回传（快扩展先回先渲染）；加载骨架只挡零项期；选中按 `(ext_id,id)` 身份恢复（新 `ListState::select_by_identity`）；旧轮进程在首个结果到达时替换（无空窗）；`Full` 变体仅测试夹具（旧单包语义逐字保留——金图像素零变化的关键） | 「首项 <300ms」由机制保证 + 结构断言承接，**不引 2.5s 慢扩展夹具进 CI**（时序夹具在慢 runner 必抖）；附带 `spawn_aggregation` 去 `custom_commands` 参数（UI 侧重建时追加） |
| C3 | 三边界 thiserror（thiserror 2 已在树 → **lock 零新增**）：`LoadError`（4 变体，Display 与原拼接文案**逐字一致**，`ext_client::open` 链路换型、测试改结构化匹配）；`ProtocolErrorKind`（分类在产生点，invoke toast 按类分流：超时可重试 / 崩溃回落 stub / 其余透传）；`SettingsImportError` | 只动三边界、不全仓铺开（§7 纪律）：`Result<_,String>` 48→42，余量为非边界路径 |
| C4 | 写盘戳 `schema_version:1`；读到更高版本 → 只读模式（保存拒 + toast，`save_settings_with_feedback` 统一拦截）；`Settings.unknown`（JSON 编码文本）读→写保真（类型还原、往返稳定）；验收测试 4 条 | `exported_from`（N5 导出标记）归已知键不进保真集（写盘方是 config.json 非备份文件） |
| C5 | `Manifest.icon` → **deprecated**（§5 强提示改写 + doc_contract 锚点断言「原样保留不展开、与 entry.command 展开对照」——启用消费即红）；`has_more_items` → **保持**（协议语义 + CLI conformance 校验 + §6.3 既定口径，非死字段） | 二选一按「deprecated + 锚点测试」路线（实现路线涉及 UI 设计，留给启用时） |
| P2-5.3 | §5.3 三件全做：① 日志落盘（`file_log.rs` Tee 双后端——stderr 逐字不变 + `logs_dir()/dd-run.log` 默认 Info 滚动 5×1MB；仅 bin 入口装配、dd-protocol 零改动；目录不可建降级 stderr-only）；② 设置 → 常规「诊断」卡（打开日志目录，缺失先建）；③ 导出诊断报告（单 JSON：meta + 设置 + 扩展/信任/源状态 + 解析失败清单 + 日志尾 200 行）；+5 测试 → 663 passed / 0 failed / 7 ignored | ③ **zip → 单 JSON**（zip 需新 crate 违背 lock 零新增；单文件可直接阅读/附件）；**5.6 cargo deny 记入遗留**（本机无 cargo-deny，deny.toml 无法本地验证，不做 CI-only 盲改）；**待真机腿**：启动后确认 `logs/dd-run.log` 生成且 Info 级 |
| P2-4.5 | §4.5 全做：① criterion 三组 bench（`hot_paths.rs`：fuzzy_score_2000 / set_query_2000 / flatten_5ext；`decoder.rs`：decoder_push_64x1kib）+ 本机基线记录（decoder ≈40.4 µs / fuzzy ≈294 µs / set_query ≈184 µs / flatten ≈218 µs）；② CI `bench` job（PR-only，continue-on-error 观察模式）——base/head worktree 各跑 fast bench + 共享 target 目录，`tools/bench_compare.mjs` 比中位数 >20% 红；③ `FuzzyMatcher` 放宽 `pub`；criterion 0.5.1 + 18 新包名共 19 包进 lock 仅 dev 面 | `recompute_visible` 私有且公共入口多个 → bench 取击键驱动的 `set_query`（matcher 构建 + 打分 + 排序完整路径）；环境记档（同日核对轮修正归因）：criterion dev 链（is-terminal/winapi-util → windows-sys **0.61.2**）要求该包以新 feature 组合全新编译 → raw-dylib 需 dlltool，本机 rustc 1.98.0 查找失败（dlltool 在工具链 `bin/self-contained/` 子目录），本地构建须加 PATH（实测即解）；HEAD lock 已含 0.61.2 且 CI 全绿，CI 侧加 `GITHUB_PATH` 防御步兜底，首编待下次 push 实证；**待验**：runner 噪音分布（观察期后转阻塞） |
| P2-5.1 | §5.1 全做：① `usage.rs`（`usage.json` = `{id:{count,last_ts}}`，键 `ext_id\u{1f}item_id`；损坏/高版本 → 空表 + warn；写盘复用 `atomic_write`）；② `PanelState::set_usage` 快照注入——空查询 count 降序稳定排序、同分 tiebreak、passthrough 不变、`None` 回退与引入前逐字节一致（金图 6/6 不变）；③ 记录点 = `confirm_selected`（记录先于分派，invoke 失败仍计数）；④ bin 入口 `load_usage`（`PaletteApp::new` 保持 8 参纯注入）+ 聚合重建点重同步；⑤ 设置页「使用数据」卡 + 重置；+9 测试 → 672 passed / 0 failed / 7 ignored | 排序判据仅 `count`（`last_ts` 留作 recency 衰减观测，分析文档未定义衰减公式，不擅自发明）；dd-gui 直加 `serde` derive（已在树 → lock 零新增）；`atomic_write` 提 `pub(crate)`（crate 内复用）；**核对轮同日**：发现并修复测试污染缺陷（`save()` 无条件执行 → reset 测试清空开发者真实 usage.json——落盘改 `#[cfg(not(test))]` 门控，md5 前后比对实证）；激活后 Root 即时重排记档为预期行为；**验收「一周 Top5 ≥80%」短期不可实测**——结构测试四条承接，真机腿留给日常使用 |
| P2-5.2 | §5.2 全做：`update_check.rs`（GitHub latest 比对 + 系统 curl）+ 后台线程/mpsc 轮询转 toast + 设置页「关于」卡（版本 + 按钮 + 启动检查开关）+ settings `check_updates` 键；单测 +4 → 679 passed；真机腿达成（curl 1.05 s 实测返回 v0.2.1 → UpToDate） | **偏差**：① "可关"落实为「启动时检查更新」开关且**默认关**（隐私优先；手动按钮常驻）；② HTTP 方案 = 系统 curl 零新依赖（原案未指定；ureq/WinHTTP 弃选记档）；③ 预发布后缀比较忽略、展示保留（近似记档）；不做自动安装 = 原案口径 |
| P2-5.4 | §5.4 全做（语法除外）：`--ext-new <ID>` 生成 7 件套（清单 / main.py 必选表面最小实现 / start·start.cmd / install.py / conformance.yml / README）；单测 +3（id 校验 / dd_host 扫描器真实加载 / 覆盖保护）→ 675 passed；本机实跑生成 + 扫描加载 + 渲染检视 | **偏差**：flag 式非子命令（CLI 全 flag 解析器）；**关键发现（实跑 os error 193）**：F4 补全口径 = 仅 .exe 不补 .cmd 且 entry.args 不展开 `${EXT_DIR}` → 「宿主补全 .cmd」不可行，Windows 宿主启动走 install.py 绝对路径清单（pymin 模式）；**待真机腿**：conformance 端到端需真 Python（本机仅 Store stub）；「零基础 10 分钟」验收随腿 |
| P2-5.7 | §5.7 全做（alias 除外）：`[build] target` 去 pin；CI 三处 `CARGO_BUILD_TARGET` 保留为显式声明；rust-toolchain.toml channel 1.95.0 不动（MSRV 保留）；CONTRIBUTING 环境节改写（Linux/macOS 开箱即用提为第 2 条）；package.sh / release.yml 注释同步 | **偏差**：`[alias] build-win` 未加——1.95.0-msvc 交叉 windows-gnu 缺 dlltool 必挂（B2 记档），Windows 本机机制 = `RUSTUP_TOOLCHAIN=stable-gnu` 覆盖（与 CI Windows job 同款）；本机实证：bare 解析 = 1.95.0-msvc / 文档化流程构建通过 / package.sh Windows 分支产物路径不变（TOOLCHAIN gnu 宿主 = 原 pin 值）；**待验**：CI 真实行为（env 均保留理论零变化） |

**门禁实测（2026-10-24）**：`cargo test --workspace` **628 passed / 0 failed**（627 + D1 新增 1；r02/r17 替换不增减）；`cargo fmt --all --check` 零差异；host `clippy -D warnings` 零告警；`--target x86_64-unknown-linux-gnu` 与 `--target aarch64-apple-darwin` 跨目标 clippy 均零告警；`node tools/docscan.mjs` 0 broken / 0 drift / 87 路径 0 missing；`bash -n tools/package.sh` 通过。**未验**：`release-sums` job 与 workspace 继承的 CI 真实行为待下次 tag 构建。

**门禁实测（2026-10-24，B2 收口后）**：`cargo test --workspace --no-fail-fast` **650 passed / 0 failed / 7 ignored**（631 + B2 第二批 19 条契约锚点；ignored = 6 张金图 + 1 个 dd-ext R-09 机器相关测试——apps.rs Flowframes/Dead Cells 本机安装断言，一直存在）。

**门禁实测（2026-10-24，C 批收口 + 核对修正后，最终）**：`cargo test --workspace --no-fail-fast` **658 passed / 0 failed / 7 ignored**（658 = 650 + C2 结构断言 3 + C4 验收 4 + C5 锚 1；ignored 同上——金图 `#[ignore]` 不进常规三关，另有 snapshot 专用 job 单跑 6/6 过）；clippy `-D warnings` 零告警；`fmt --check` 零差异；docscan 四项全绿；`bash -n tools/package.sh` 通过。核对轮修正台账 3 处数字误差（✅/⚠️ 分类计数、ExtPool 23 注记、ignored 7 非 6）——零代码影响。**待验**（与前同）：CI snapshot job WARP 一致性、`release-sums` job。

**门禁实测（2026-10-25，P2-4.5 后）**：`cargo test --workspace --no-fail-fast` **663 passed / 0 failed / 7 ignored**（数不减——bench 不进常规三关）；clippy `--all-targets -D warnings` 零告警，且 `x86_64-unknown-linux-gnu` / `aarch64-apple-darwin` 交叉目标零告警（bench 纳入 lint 面）；`fmt --check` 零差异；`tools/bench_compare.mjs` 绿/红双路自测通过；本机 bench 基线见台账 P2-4.5 行。**核对轮（同日）修正 3 处**：① dlltool 归因——失败 crate 实为 windows-sys **0.61.2**（is-terminal/winapi-util 要求的新 feature 组合全新编译），非「criterion 引入的 0.59」（0.59/0.52 均在 HEAD lock，egui_kittest/eframe 链）；② 「set_query 是 recompute_visible 唯一公共入口」失实——公共入口 5 个，bench 取击键驱动的 set_query；③ lock 新增精确计数 = criterion 本体 + 18 新包名共 19 包。CI 侧加 `check`/`snapshot` 两 Windows job 的 `GITHUB_PATH` dlltool 防御步。**待验**（与前同 + 新增）：CI bench job runner 噪音分布（观察期后转阻塞）、dlltool 防御步与新 feature 组合的 CI 首编。

**门禁实测（2026-10-25，P2-5.2 后）**：`cargo test --workspace --no-fail-fast` **679 passed / 0 failed / 7 ignored**（+4：update_check parse/classify/normalize 3 + app poll 状态机 1；真实 curl 不进测试）；clippy 三目标零告警；fmt 零差异；金图 6/6（关于卡在 General 分类）；**真机腿达成**：本机 curl 实测 `--max-time 3` 下 **1.05 s** 返回 `v0.2.1` → 分类 UpToDate。至此 **§5 的 P2 七项全部收口**（5.5 并入 A5、5.6 cargo-deny 与 §4.4 fuzz 维持"工具不可本地验证"记档遗留）。

**门禁实测（2026-10-25，P2-5.4 后）**：`cargo test --workspace --no-fail-fast` **675 passed / 0 failed / 7 ignored**（+3：scaffold id 校验 / dd_host 扫描器真实加载 / 覆盖保护）；clippy 三目标零告警；fmt 零差异；本机实跑生成 + 扫描加载 + 生成物渲染检视；**核对轮（同日）**：CI 模板缺 Rust 安装步骤（runner 不预装 Rust）→ 补 setup-rust-toolchain + js-yaml 校验；extensions.md §1 补脚手架捷径；`entry.args` 不展开 / stub exit 49 / 扫描平铺 / conformance 分支条件均 file:line 实证；**待真机腿**：conformance 端到端实跑需真 Python（本机仅 WindowsApps Store stub，`python`/`python3` 均 exit 49），CI 模板 ubuntu 自带 Python 首用即验。

**门禁实测（2026-10-25，P2-5.7 后）**：`cargo test --workspace --no-fail-fast` **672 passed / 0 failed / 7 ignored**；clippy 三目标零告警；fmt 零差异；`rustup show` bare 解析 = `1.95.0-x86_64-pc-windows-msvc`（Windows 需工具链覆盖的事实不变）；package.sh Windows 分支 `cargo +stable-gnu build --release` 产物路径 `target/x86_64-pc-windows-gnu/release` 不变（宿主三元组 = 原 pin 值）；ci.yml / release.yml / package.sh 语法校验过。**核对轮（同日）**：残留陈旧表述两处清零（rust-toolchain.toml 注释、README 中英「覆盖仓库默认 windows-gnu」）——教训与 B2 锚点治理同款：**改配置类事实必须全仓 grep 其表述面**，不能只改配置文件本身；portable_zip.ps1 无硬编码 target 路径不受影响。修正后全量测试与 docscan 复绿。**待验**：CI 真实行为（env 均保留，理论零变化，下次 push 实证）。

**门禁实测（2026-10-25，P2-5.1 后）**：`cargo test --workspace --no-fail-fast` **672 passed / 0 failed / 7 ignored**（+9：usage 3 + state 4 + app 2）；clippy `--all-targets -D warnings` 三目标零告警；fmt 零差异；金图 6/6 逐字节不变（`usage=None` 默认 → 排序路径与引入前一致）；docscan 全绿。**核对轮（同日）**：① 发现并修复测试污染缺陷（测试路径 `save()` 清空真实 usage.json → `#[cfg(not(test))]` 门控，修复后 md5 前后比对不变）；② 逐项复核成立：记录点唯一、聚合两处重建点均同步（`keys.rs` 的 root 重建为测试夹具）、lock 零新增（serde 系均在 HEAD lock）。
