# dd-run 修复实施方案

- **依据**：《dd-run 代码严审报告（v2，含 UI/性能/内存/架构）》，审查基线 commit `b40ced9`（v0.2.0，2026-10-07）
- **目标版本**：v0.2.1（缺陷修复为主）；F12（扩展签名）单独立项，不在本版范围
- **约定**：所有路径相对仓库根；所有"验收标准"均为可执行命令或可复现操作；每项标注**影响面与冲突**，按批次给出合并顺序以保证无冲突
- **修订 v1.1（2026-10-09）**：按代码核对（核对基线同为 `b40ced9`）修正 F1/F4/F5/F6/F7/F8/F9/F10/F11 共 7 类断言错误与行号漂移——F7 类型名 `ListState`→`PanelState`（全仓无 `ListState`）、F8 影响面扩至 icons.rs（`resolve_icons` 为 `&mut self` + owned 参数，原草稿双重编译失败）、F10 草稿 `lock()` 写法与"既有测试"表述更正、F4 MSRV 1.77→1.77.2、F11 测试更新 4→5 处、F5 已完成子项标注、F1 导入位置更正。各节内标注"2026-10-09 核对"处即本轮修正。

## 0. 全局门禁（每个批次合并前必须全绿）

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # F8-b 完成后不再带 --skip
cargo audit
cargo machete                     # 依赖零新增核实
```

发布前：`docs/implementation.md` §6.1 追加本轮台账行；`CHANGELOG.md` 增加 `[0.2.1]`；tag `v0.2.1` 与 `crates/dd-gui/Cargo.toml` version 一致（release.yml 既有校验会强制）。

---

## 批次一：独立单文件修复（可并行开发，互不相交）

### F1 — shell 兜底命令"大输出被超时误杀"（P1，正确性 bug）

**位置**：`crates/dd-ext/src/builtins/shell.rs:285-319`（`run_capture`）

**根因**：轮询循环先 `try_wait()` 后读管道。子进程 stdout 超过 OS 管道缓冲（约 4–64KB）即阻塞在 write 上永不退出，3 秒后被 kill 误报"执行超时"。第 296 行注释"进程退出后管道写端关闭，wait_with_output 不会死锁"描述的正是**死锁发生之后**的阶段，注释本身也是错的。

**改法**（删除 296-312 的轮询+`wait_with_output`，改为读线程排空）：

```rust
fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = home_dir() {
        command.current_dir(dir);
    }
    let mut child = command.spawn().map_err(|e| format!("spawn 失败：{e}"))?;

    // ① 先取管道、起读线程排空（对端写满缓冲不再反压子进程）
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);

    // ② 轮询退出 + 超时 kill（既有逻辑保留，仅删除注释中错误表述）
    let deadline = Instant::now() + Duration::from_millis(EXEC_TIMEOUT_MS);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // ③ kill 后管道写端关闭，读线程读到 EOF 自然结束
                    let _ = stdout_reader.map(|h| h.join());
                    let _ = stderr_reader.map(|h| h.join());
                    return Err(format!("执行超时（>{EXEC_TIMEOUT_MS}ms），已终止"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let _ = child.wait();
    // ④ 正常退出路径同样 join（EOF 已到，join 恒有限时完成）
    let stdout = stdout_reader.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let stderr = stderr_reader.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    /* 后续 decode_output / 合并逻辑不变，入参由 &[u8] 改为上两步所得 */
}

/// 单管道读线程：read_to_end 直到 EOF（进程退出或被 kill 均会触发）。
fn spawn_pipe_reader<R: std::io::Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf); // 读失败按空处理，与现行为一致
        buf
    })
}
```

**连带修正**：
- 重写第 296 行注释，如实描述"读线程排空防反压死锁"；
- **导入补充**（2026-10-09 核对修正）：`shell.rs` 现无 `std::io::Read` 导入（`read_to_end` 需 trait 在作用域）；`std::process::{Command, Stdio}` 位于 `#[cfg(windows)] mod sys` **内部**（80 行 use 区）而非文件头部——`use std::io::Read;` 应**新增在 `mod sys` 内 80 行 use 区**（`run_capture` 属 sys 模块），不加文件头部；`std::thread` 在草稿中为全限定路径，无需导入；
- 超时路径 kill 的仍是 `cmd.exe` 直接子进程，孙进程孤儿化**维持现状**（改动会引入 Job Object 复杂度，单独记入 §6.1 缓办台账，不与本修复捆绑）。

**影响面与冲突**：仅 `shell.rs` 一个文件 + 新测试。与 F6 同文件但不同函数区域（F6 在 235 行 Confirm 构造处），若同 PR 请分开 commit。

**新增测试**（`shell.rs` tests 模块，`#[cfg(windows)]`）：

```rust
#[test]
fn run_capture_drains_output_larger_than_pipe_buffer() {
    // ~300KB 输出（远超 64KB 管道缓冲）：修复前此命令必超时误杀
    let big = std::env::temp_dir().join("ddrun_run_capture_big.txt");
    std::fs::write(&big, "a".repeat(300 * 1024)).unwrap();
    let out = run_capture("cmd.exe", &["/C", &format!("type {}", big.display())]);
    let _ = std::fs::remove_file(&big);
    let out = out.expect("大输出命令不得误判超时");
    assert!(out.len() >= 300 * 1024, "输出应完整排空，实得 {} 字节", out.len());
}
```

**验收标准**：
1. `cargo test -p dd-ext shell::` 全绿，含上述新测试（修复前该测试必然失败，可先写测试红再改代码绿）；
2. 真机 GUI 验证：兜底执行 `cmd /C type <大文件>`，toast 显示文件内容摘要而非"执行超时"；
3. 既有 `run_capture_executes_echo`、`run_capture_runs_in_home_dir` 不回归。

**工作量**：0.5 天（含测试）。

---

### F2 — calc 大数结果显示为 i128::MAX（P2，正确性 bug）

**位置**：`crates/dd-ext/src/builtins/calc.rs:419-435`（`format_number`）

**根因**：整数浮点分支 `rounded as i128` 是 Rust 饱和转换，|v| ≥ 2¹²⁷ ≈ 1.7e38 一律显示为 `170141183460469231731687303715884105727`。

**改法**：整数分支去掉 i128 转换，直接格式化浮点（Rust 对整数值 f64 输出无小数点、无科学计数法，语义正确且无上限）：

```rust
if (v - rounded).abs() < 1e-9 {
    return format!("{rounded}");   // 4.0 → "4"，1e60 → 全数字展开
}
```

**影响面与冲突**：仅 `calc.rs`。既有测试 `format_number_trim`（4.0→"4"、2.5→"2.5"、-0.0→"0"）行为不变，无需改动。

**新增测试**：

```rust
#[test]
fn format_number_beyond_i128_range_is_exact() {
    let big = format_number(2.0f64.powi(128)); // ≈3.4e38，越过 i128 上限
    assert_eq!(big, "340282366920938463463374607431768211456");
    assert_eq!(format_number(-2.0f64.powi(128)), "-340282366920938463463374607431768211456");
    // i128 范围内行为回归
    assert_eq!(format_number(4.0), "4");
}
```

**验收标准**：`cargo test -p dd-ext calc::` 全绿含新测试；GUI 验证 `calc` 输入 `2^128` 显示 `340282366920938463463374607431768211456`。

**工作量**：0.5 小时。

---

### F3 — NDJSON 解码器 O(n²) 搬移（P3，DoS 面 + 性能）

**位置**：`crates/dd-protocol/src/framing.rs`（`Decoder::push`）

**根因**：每切一行做 `buf.iter().position(b'\n')` 从头扫描 + `buf.drain(..=pos)` 全量搬移；单 chunk 含大量小行时整体 O(n²)。

**改法**：push 内改用"消费偏移"单遍扫描，末尾一次 drain。**行切分语义必须逐字节保持现状**（含现有对残留/超限/毒化的判定顺序），仅改遍历与拷贝方式：

```rust
pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> { /* 签名不变 */
    // 前置：poisoned 丢弃入参、残留+入参超限判定 —— 与现实现完全一致，保持不动
    let mut consumed = 0usize;
    loop {
        let Some(rel) = self.buf[consumed..].iter().position(|&b| b == b'\n') else { break };
        let end = consumed + rel;               // '\n' 的绝对下标
        let line = &self.buf[consumed..end];    // 不含 '\n'（若现有实现剥离 '\r'，此处同步保留）
        // 既有逐行判定：TooLarge（按行长度，行为同现实现）/ InvalidUtf8 / Message(String::from_utf8_lossy...)
        // —— 判定代码原样挪入，仅数据来源从 drain 改为切片
        consumed = end + 1;
    }
    self.buf.drain(..consumed);                 // 每个 push 恰好一次 O(剩余) 搬移
    frames
}
```

**影响面与冲突**：`framing.rs` 单文件；`Decoder` 的公开 API 不变（`process.rs`/`dd-ext lib.rs`/CLI 均零改动）。既有 **11** 个 framing 单测（已逐一核对：多消息切分/跨 push 残留/CRLF 剥离/空行/超限/UTF-8/毒化/边界等）是本修复的安全网。

**语义保持清单**（改写时逐条对照现实现 `framing.rs:77-116`）：
- 已终止行超限 → `TooLarge` **不毒化**，继续处理后续行（93-98 行）；
- 未见换行的残留超限 → `TooLarge` 一次 + **毒化**（105-114 行）；
- CRLF 剥离（87-89 行）、空行忽略（90-92 行）、`TooLarge` 产出行内**不含** `\r\n`；
- `reset()` 语义、`buffered()` 返回值不变。

**新增测试**：

```rust
#[test]
fn push_many_small_lines_in_one_chunk_is_all_frames() {
    let mut d = Decoder::with_default_limit();
    let chunk: Vec<u8> = std::iter::repeat(b"{}\n").take(10_000).flatten().collect();
    let frames = d.push(&chunk);
    assert_eq!(frames.len(), 10_000, "单 chunk 万行应全部成帧，一行不丢");
    assert!(frames.iter().all(|f| matches!(f, Frame::Message(_))));
    assert!(d.buf.is_empty(), "无残留");
}

/// 性能回归（宽松阈值防 CI 抖动；debug 构建下也须通过）
#[test]
fn push_quadratic_regression_bound() {
    let mut d = Decoder::with_default_limit();
    let chunk: Vec<u8> = std::iter::repeat(b"x\n").take(200_000).flatten().collect();
    let t = std::time::Instant::now();
    let frames = d.push(&chunk);
    assert_eq!(frames.len(), 200_000);
    // O(n²) 旧实现 debug 下 > 数秒；O(n) 新实现 debug 下 < 1s（余量 10 倍）
    assert!(t.elapsed() < std::time::Duration::from_secs(1), "elapsed = {:?}", t.elapsed());
}
```

**验收标准**：
1. `cargo test -p dd-protocol` 全绿（既有用例零修改即通过 = 语义等价的硬证据）；
2. `cargo test -p dd-protocol --release` 中性能用例 < 100ms（记录数值入 §6.1）；
3. `cargo test -p dd-host -p dd-ext`（依赖 Decoder 的 roundtrip 测试）全绿。

**工作量**：1 天。

---

### F4 — MSRV 声明 + 清单解析拒绝 `.cmd`/`.bat`（纵深防御）

**位置**：6 个 `crates/*/Cargo.toml`；`crates/dd-host/src/manifest.rs:296-312`

**根因**：`resolve_executable` 按 PATHEXT 习惯（实为固定数组 `[".exe", ".cmd", ".bat"]` 顺序补全，manifest.rs:303，不读取 PATHEXT 环境变量）；`.cmd`/`.bat` 经 cmd.exe 承载，参数转义依赖 Rust ≥1.77.2（BatBadBut，CVE-2024-24576）修复——**修复发布于 1.77.2 而非 1.77.0，MSRV 必须锁到 1.77.2 才真正锁住该边界**；而 workspace 未声明 MSRV，该安全边界未被锁定。

**改法**：
1. 每个 `crates/*/Cargo.toml` 的 `[package]` 段加一行：`rust-version = "1.77.2"`（六个文件各一行，不用 workspace 继承，避免重构 `[workspace.package]`；已核实全仓现无任何 rust-version 字段、根 Cargo.toml 无 `[workspace.package]` 段）；
2. `manifest.rs:303` 改为：

```rust
// BatBadBut（CVE-2024-24576）纵深防御：.cmd/.bat 经 cmd.exe 承载，参数存在
// 二次解析歧义面（与 S-01 同类关切）。清单作者需要批处理时自行写
// cmd.exe /C xxx.bat 并自担转义责任；宿主不再代为补全。
for ext in [".exe"] {
```

3. `docs/manifest-schema.md` **§4 路径展开（第 68 行，已核对原文该处记载了"原样路径 → `.exe` → `.cmd` → `.bat`"补全顺序并引用 `manifest.rs:280-296`）**：删除 `.cmd`/`.bat` 补全描述、增补"为何不再补全"（引用上述注释口径），并**顺带修正该行已漂移的行号引用**（`resolve_executable` 现位于 `manifest.rs:297-313`）。

**影响面与冲突**：
- 已核实 `dd-host` 源码与测试**无** `.cmd`/`.bat` 解析用例，无测试需改；
- 行为变化：此前依赖"裸名自动补出 foo.cmd"的第三方清单会从"可用"变"扫描跳过 + 设置页可见原因"——属 fail-closed，且随包内置清单全部指向 `.exe`（已核实），不受影响；
- `docs/protocol.md` 不涉及，无需改。

**新增测试**（manifest.rs tests）：

```rust
#[test]
fn resolve_executable_no_longer_completes_cmd_bat() {
    // 目录里只有 foo.cmd / foo.bat：裸名 "foo" 不再补全 → None
    // 同目录存在 foo.exe：仍正常命中
}
```

**验收标准**：`cargo test -p dd-host` 全绿；`cargo build -p dd-protocol` 在旧工具链（<1.77.2）上因 `rust-version` 报 MSRV 错误（可选验证）；文档 diff 与注释口径一致。

**工作量**：2 小时。

---

### F5 — apps.rs 个人环境硬编码 + CI 机器相关测试（P8）

**位置**：`crates/dd-ext/src/builtins/apps.rs`（`steam_install_dirs`，742-758）；`.github/workflows/ci.yml:35`（`-- --skip steam_installed_...`）

**改法**：
1. **删除 `G:\Program Files (x86)\Steam` 字面量**（apps.rs:751；同行 752 的 `C:\Program Files (x86)\Steam` 与 744-746 的 `PROGRAMFILES(X86)` 环境变量路径重复，建议一并删除）。Steam 库目录解析保留既有 `libraryfolders.vdf` 链路（`steam_library_paths`，apps.rs:762-782 已核实），另加环境变量覆盖（与 `DDRUN_EVERYTHING_DIR` 命名一致）：

```rust
// 测试与便携安装钩子：显式覆盖优先，其次注册表/默认路径（现状逻辑）
if let Ok(dir) = std::env::var("DDRUN_STEAM_ROOT") {
    if !dir.is_empty() { out.push(PathBuf::from(dir)); }
}
```

2. **现状确认（2026-10-09 核对，本子项已完成、无需改动）**：该测试已更名为 `machine_steam_installed_shown_uninstalled_filtered_root_lnk_shown`（apps.rs:1755）且已带 `#[ignore = "机器相关：依赖本机安装 Flowframes / Dead Cells、未装 Cataclismo"]`（1754）——维持现状即可；
3. `ci.yml` 删除 `-- --skip steam_installed_shown_uninstalled_filtered_root_lnk_shown`，让 workspace 测试无例外全跑（被 ignore 的测试在 CI 自然跳过，不再污染 skip 语义）。

**影响面与冲突**：`apps.rs` 枚举逻辑函数体最小改动（只动目录来源清单）；`ci.yml` 与 release.yml 无交集。**盘符字面量核对结论（2026-10-09 再核对）**：`G:` 字面量共 3 处——生产 751（本修复删除）+ 测试 `apps.rs:1657-1658`（`exe_dedup_key` 单测把 `G:\l\launcher.exe` 用作**任意字符串夹具**，与机器无关，保留合法）；另有若干 `C:` 字面量（生产 752 建议随本修复一并删除、802 `C:\ProgramData` 为标准路径；测试 1484、1552-1583、1648-1653 均为合法夹具）——均不属机器绑定问题，不在本修复范围内。

**验收标准**：
1. `rg -n 'G:' crates/dd-ext/src/builtins/apps.rs` 仅命中 1657-1658 两行测试夹具（`steam_install_dirs` 函数体内零盘符字面量）；
2. `cargo test --workspace` 在**无 Steam 的干净 Windows runner**（即 CI）全绿——不再依赖 skip 参数；
3. 维护者真机 `cargo test -p dd-ext -- --ignored` 仍绿；
4. `DDRUN_STEAM_ROOT=<夹具目录>` 注入后枚举结果含该目录（新增单测覆盖 env 分支）。

**工作量**：0.5 天。

---

## 批次二：dd-gui 交互与平台层（文件互不相交，可并行）

### F6 — Shell 危险命令确认弹窗文案（UI-6 / P4）

**位置**：`crates/dd-ext/src/builtins/shell.rs:233-248`（`CommandResult::Confirm { .. }` 构造处；Confirm 字面量本体 235-245，完整分支到 248）

**改法**：description 文案改为明示"不拦截"语义（中英双语走既有 `tr()`）：

> 中：**"dd-run 不会拦截或沙箱化任何命令：确认后将以你的用户权限直接执行。请核对命令内容再继续。"**
> 英：**"dd-run does not block or sandbox commands: on confirm it runs with your user privileges as-is. Review the command before continuing."**

标题保留"危险命令"，确认按钮标签保留现状（critical 红底语义不变）。egui `ConfirmDialog`（`dd-gui/src/ui/confirm.rs`）按 `description` 渲染，UI 层零改动。

**新增测试**（shell.rs tests）：断言未确认分支返回的 `CommandResult::Confirm` 的 description 同时含"不会拦截"与"用户权限"（中文文案）；英文分支含 "does not block"。

**验收标准**：`cargo test -p dd-ext shell::` 全绿；真机触发 `shutdown /r` 类命令，弹窗文案与上述一致、Enter/Esc 语义不变。

**工作量**：1 小时。

---

### F7 — 列表导航补 Home / End / PgUp / PgDn（UI-1）

**位置**：`crates/dd-gui/src/state.rs:331-361`（`PanelState` 移动方法区：move_down 331-342 / move_up 345-356 / confirm 359-361）；`crates/dd-gui/src/app/keys.rs:53-59`（consume_key 消费点，ArrowDown 53 / ArrowUp 54）与 `163-170`（↑↓ 处理分支）；`crates/dd-gui/src/ui/panel.rs:434`（`draw_list` 设定页步长）

> **2026-10-09 核对更正**：原稿所写类型名 `ListState` 全仓 0 处匹配，实际类型为 **`PanelState`**（state.rs:129）；本节草稿代码中的类型名已全部更正。

**改法**：
1. `state.rs` `PanelState` 增加（字段保持该文件私有字段+方法封装风格，不引入 pub 字段；**同步更新全部 `PanelState` 构造点**——已核对 `state.rs:172-183`（`with_empty_view` 的 `Self { … }` 构造）为主要构造点，实施时 `rg -n "PanelState" state.rs` 全量清点）：

```rust
page_step: usize,   // 私有字段，默认 10（各构造点同步）

/// PgUp/PgDn 页步长：panel.rs 每帧按可视行高实测回写；未回写时默认 10。
pub fn set_page_step(&mut self, n: usize) { self.page_step = n.max(1); }

pub fn move_home(&mut self) {
    if self.visible_count() > 0 { self.selected = Selected::Some(0); }
}
pub fn move_end(&mut self) {
    let n = self.visible_count();
    if n > 0 { self.selected = Selected::Some(n - 1); }
}
pub fn move_page_down(&mut self) {
    // 不回绕（与 move_down 的环绕语义刻意不同——翻页跳转，注释写明）
    let n = self.visible_count();
    if n == 0 { return; }
    let cur = self.selected_index().unwrap_or(0);
    self.selected = Selected::Some((cur + self.page_step).min(n - 1));
}
pub fn move_page_up(&mut self) {
    let n = self.visible_count();
    if n == 0 { return; }
    let cur = self.selected_index().unwrap_or(0);
    self.selected = Selected::Some(cur.saturating_sub(self.page_step));
}
```

（2026-10-09 核对：类型名为 `PanelState`（state.rs:129，全仓无 `ListState`）；`Selected` 枚举在 106-113、`visible_count()` 在 280、`selected_index()` 在 317、`set_selected` 在 367，草稿方法直接复用。）

2. `keys.rs`：在既有 `down/up` 消费点（53-54 行 `consume_key`、163-170 行处理分支）同层增加四个 `consume_key`：

```rust
let home = i.consume_key(egui::Modifiers::NONE, egui::Key::Home);
let end  = i.consume_key(egui::Modifiers::NONE, egui::Key::End);
let pgup = i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp);
let pgdn = i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown);
```

分发位置与 ↑↓ 相同的分支（根列表 + 嵌套页共用块），`scroll_follow = true` 同步置位。

**取舍声明（写入代码注释与设计稿键位表）**：搜索框聚焦时 Home/End 被列表消费（与 ↑↓ 同策略，handle_keys 先于 TextEdit 处理输入）；单行搜索框内光标跳行首/行尾让位给列表导航——与 CmdPal 行为一致。若未来需要保留编辑光标语义，可改为 `Ctrl+Home/End`，本轮不做。

3. `panel.rs` `draw_list`（`panel.rs:434`，函数内 `panel.rs:438` 已有 `metrics = theme::ListMetrics::of(self.settings.density)`）：以 `metrics.row_h`（**密度可变三档**：紧凑 36 / 标准 40 / 宽松 44，`theme.rs:57-83` 的 `of()`；常量 `ROW_H`（theme.rs:18）仅对应标准档 40，**不能用它替代 `metrics.row_h`**）计算本帧可视行数 `n = (ScrollArea 可视高 / metrics.row_h) as usize`，回写 `page.list.set_page_step(n)`。

**影响面与冲突**：`state.rs`/`keys.rs`/`panel.rs` 三文件。**与 F8（UI-7）同触 panel.rs 的 `draw_list` 区域——合并顺序必须 F7 先、F8 rebase**（见 §批次四）。

**新增测试**（state.rs tests，仿既有 `move_down_wraps_around`）：
- `move_end_on_empty_list_is_noop` / `move_home_selects_first` / `move_page_clamps_without_wrap`（含 `page_step` 默认 10 与自定义 3 两档）。

**验收标准**：
1. `cargo test -p dd-gui state::` 全绿；
2. 键位矩阵真机走查（计入 `docs/user-session-walkthrough-checklist.md`）：Home→第一项、End→末项、PgDn→+page_step（不越界不回绕）、PgUp→−page_step、与 ↑↓/Tab 混用无选中错乱；
3. 设计稿 §2.2 键位表同步四键。

**工作量**：1 天。

---

### F8 — 面板渲染消除每帧克隆（UI-7，速度）

**位置**：`crates/dd-gui/src/ui/panel.rs`（451-531 收集/分组、537-606 行循环、609-679 写回段）；**连带 `crates/dd-gui/src/ui/icons.rs:127-131`（`resolve_icons` 签名借用化，见实施要点 0）**

**根因**：`filtered()` 已是借用迭代器（`state.rs:289`，返回 `(usize, &PanelItem)`），但 panel.rs 随即 `it.clone()` 收集成 owned Vec，分组时再 clone 一次——每帧 2×N 次深拷贝（含 String）。

**⚠️ 实施要点（核对后修正的方案）**：450 行注释"以便循环结束后可写回 hover/click 结果，避免借用冲突"揭示了克隆的**真实动机**——640-680 行的写回段含 `self.stack.current_mut()`（655）与 `self.last_hovered_index`/`self.scroll_follow` 可变写（640-647），与借用 `items` 冲突（E0502）。因此**不能只把收集改借用**，必须配合"写回后置"。

**0. `resolve_icons` 借用化 + 缓存写后置（2026-10-09 复核补充，icons.rs 连带改造）**：`resolve_icons` 现签名是 `&mut self` + **owned** 参数 `&[(usize, PanelItem)]`（icons.rs:127-131）——草稿原写法 `self.resolve_icons(ui.ctx(), &items)`（items 为借用）**双重编译失败**（E0502 借用冲突 + 类型不匹配）；现状每帧 clone 正是为绕开此借用。需连带改造：
- 函数内部只读 `item.icon`、从不拥有 `PanelItem` → 签名可安全改为 `items: &[(usize, &PanelItem)]`（内部零逻辑改动）；
- 但 `&mut self` 写缓存（`icon_cache` / `icon_failed` 插入，icons.rs:165/169）与 items 借用冲突 → 拆为**纯只读查找**（`&self` 或自由函数：命中缓存/负缓存/glyph 直接出 `IconView`；miss 项读盘解码后，把待写数据收集为局部 `pending_cache` / `pending_failed` 返回）+ **缓存写后置**（items 借用结束后随写回段统一 `&mut self` 入缓存）——与写回段后置同一手法。

1. **绘制段（450-639 行）全借用**：

```rust
let items: Vec<(usize, &PanelItem)> = page.list.filtered().collect();
let mut groups: Vec<(Option<&String>, Vec<(usize, &PanelItem)>)> = Vec::new();
for (idx, item) in &items {
    match groups.iter_mut().find(|(k, _)| *k == item.section.as_ref()) {
        Some((_, list)) => list.push((*idx, item)),
        None => groups.push((item.section.as_ref(), vec![(*idx, item)])),
    }
}
let (icon_views, pending_icon_cache) = resolve_icons_readonly(ui.ctx(), &items); // 纯只读：&self 部分抽为只读函数；缓存写随写回段后置
```

行循环（581-639）保持现状的"只收集局部变量、不改 self"模式（`hovered`/`clicked`/`select_only`/`right_clicked`/`selected_rect` 均已是局部变量，已核对）。

2. **写回段（609-679 行）整体移到 `items` 借用结束之后**：hover 簿记（`last_hovered_index`/`scroll_follow`，散布于 626-634 / 642-646 / 657-658）、右键菜单、Shift+F10 菜单、单击/双击激活全部后置；其中需要 item 数据的两处 `open_ctx_menu` 调用（656 附近右键分支与 Shift+F10 分支；`current_mut()` 实有 4 处：625/631/637/656）改为**按 index 重新查找 + 单次 clone**——clone 只在真实点击/按键时发生（每帧 0 次），不再每帧每项：

```rust
// items 借用已结束，此处 current_mut 合法
if let Some((idx, pos)) = right_clicked {
    if let Some(item) = lookup_item(idx) {           // filtered().find → clone 一次
        self.stack.current_mut().list.set_selected(idx);
        self.open_ctx_menu(idx, &item, ...);
    }
}
```

3. `lookup_item` 辅助 = `self.stack.current().list.filtered().find(|(i, _)| *i == idx).map(|(_, it)| it.clone())`；写回所需的其余数据（选中 index、锚点矩形、指针坐标）均已局部变量化，随写回段一起后置即可。

**影响面与冲突（2026-10-09 核对修正）**：`panel.rs`（draw_list）+ `icons.rs`（resolve_icons 签名借用化与缓存写拆分）**两个文件**；`filtered()` 签名不动 → 其他调用点零改动（C7 页脚实际经 `selected_item()`（panel.rs:337）间接触发 filtered()，同样零改动）。**与 F7 同文件同区域，合并顺序 F7 → F8**。风险点：借用重排漏一处即编译失败（E0502/E0499），由编译器兜底，不会静默出错；行为等价性靠验收 3 的走查保障。

**验收标准**：
1. `cargo test --workspace` 全绿；`cargo clippy -D warnings` 无新告警；
2. **量化验收**（记入 §6.1）：release 构建、500 项页面（全部应用典型规模），egui 调试层（`ctx.settings_ui` 的 frame time 面板或 `tools/screenshot` 现有口径）记录输入态每帧耗时，较基线下降 ≥30%（基线 = 修复前同场景数值，测法写入台账行）；
3. **手工回归三项**（借用重排的等价性保障）：500 项页面悬停高亮跟随、右键任意行菜单锚点正确、键盘 Shift+F10 菜单锚点正确。

**工作量**：2 天（2026-10-09 核对上调：icons.rs 连带改造 + 量化与回归）。

---

### F9 — reveal_in_folder 补路径字符门控（P6.1，一致性）

**位置**：`crates/dd-gui/src/platform.rs:589-608`（`reveal_in_folder`，589 为 `#[cfg(windows)]` 属性）

**改法**：在构造 `/select,{path}` 前加与 `dd-ext/src/bin/search.rs` `valid_reveal_path`（S-08）**同规则**的纯函数门控（双引号/控制字符含 \t\n\r 与 0x7F/`%`/`^` 一律拒绝），拒绝时返回 `Err("路径含不允许字符")` 并在调用点落 toast（沿用既有错误呈现）：

```rust
fn reveal_path_allowed(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(|c| c == '"' || c == '%' || c == '^' || c.is_control())
}
```

规则差异（与 search.rs 比对）留注释说明；后续若收敛，候选归属 `dd-protocol`（纯函数、无依赖），本轮**不**跨 crate 重构（控制冲突面）。

**新增测试**（platform.rs tests）：镜像 search.rs S-08 的合法/非法用例集（含 `C:\a"b`、含 `\n`、含 `%`、合法含空格路径放行）。

**验收标准**：`cargo test -p dd-gui platform::` 全绿；真机：文件搜索扩展"显示所在目录"对含 `%` 文件名给出错误 toast 而非 spawn explorer。

**工作量**：2 小时。

---

### F10 — 捕获钩子陈旧实例自愈（P7，竞态）

**位置**：`crates/dd-gui/src/platform/capture_hook.rs`（`CAPTURE_TX`/`THREAD_ID` static、`hook_proc`、`start`、`Guard`）

**根因**：单槽 static 复用。旧会话收尾慢于新会话 `start()` 时，`THREAD_ID` 已被覆盖，旧钩子线程的 cancelled 探测误判"仍在会话中"→ 短暂双钩子并存、旧钩子事件灌入新会话通道。

**改法**（自愈式：陈旧钩子自检自卸，不引入跨线程句柄传递）：

1. hook 线程把**自己的 HHOOK 句柄**存入 thread_local（`thread_local! { static OWN_HOOK: Cell<isize> }`）——LL 钩子回调恒在安装线程执行，TLS 可靠；
2. `hook_proc` 入口第一步（在既有 `code < 0` 判定之后）：

```rust
// 陈旧自检：THREAD_ID 恒存"最新安装会话"的线程 id。本线程若已不是
// 最新会话（start() 被快速重启），立即自卸钩子并放行事件——旧钩子
// 既不再吞键，也不再向新会话通道投递。
if *THREAD_ID.lock().unwrap_or_else(|e| e.into_inner()) != unsafe { GetCurrentThreadId() } {
    let h = OWN_HOOK.with(|c| c.get());
    if h != 0 { unhook_windows_hook(h); }   // ⚠️ 必须走既有辅助函数
    OWN_HOOK.with(|c| c.set(0));
    return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
}
```

   ⚠️ 两个核对后修正的实现约束：
   - **`THREAD_ID` 现为 `Mutex<u32>`（capture_hook.rs:51），不是 atomic**——草稿按 `lock()` 写（⚠️ 原稿 `unwrap_or(&mut 0)` 类型不匹配，已改为 `unwrap_or_else(|e| e.into_inner())`，与仓库既有 poison 恢复模式一致）；实施时可顺手改 `AtomicU32`（回调内免锁更稳），两种均可，但**必须在注释中写明所选语义**；
   - **不得直接调用 `UnhookWindowsHookEx` 符号**——该文件 91-95 行注释已记档：`windows-sys` 当前版本缺此符号，仓库已有 `unhook_windows_hook()` 辅助（98-118 行，`GetProcAddress` 动态解析），自卸必须复用它，否则链接失败；

3. `start()` 保持现状结构（**无前置断言**——2026-10-09 核实：现状即异步 spawn 钩子线程后返回单元结构体 `CaptureHookGuard`，勿凭原稿想象新增断言）；`Guard::drop`（清发送端 + `PostThreadMessageW(WM_QUIT)`）主卸载路径不变（自愈只是兜底）；
4. 非 Windows 桩不动。

**影响面与冲突**：单文件。既有 capture_hook.rs 单测仅 **2 条**纯函数用例（`mods_encoding_matches_mod_star_layout` L394、`capture_vk_allowed_set_matches_egui_path` L405），均与卸载路径无关、不受影响（原稿"五条卸载路径测试"不成立——五条卸载路径仅是 L26-27 设计注释，无对应测试）。

**测试与验收**：
1. Win32 钩子无法单测自愈路径 → 验收以**代码不变式走查**（合并时 reviewer 按 1-3 核对）+ **真机规程**：连续快速开关捕获会话 20 次（<500ms 间隔），用系统"屏幕键盘"发送按键确认第二次会话期间无双吞/漏收；30 秒空闲后任务管理器确认钩子线程数回落；
2. 既有 dd-gui 测试全绿（capture_hook.rs 2 条 + keys.rs 3 条钩子/捕获相关：r21 黑名单 / n4 warm 容量 / r22 IME）；
3. ⚠️ **工具链预警**：新增 `thread_local!` 会触发本机 clippy 1.96 的 `missing_const_for_thread_local` 误报（对任何 `thread_local!` 全报、`const { … }` 块也不例外）——本机 `clippy -D warnings` 门禁会假红；属工具链问题非仓库缺陷，**勿加 `#[allow]`**，与 CI（latest stable）实测比对或升级本机 toolchain。

**工作量**：0.5 天（含真机规程）。

---

## 批次三：跨 crate 协同修复（依赖批次一/二合入后做）

### F11 — `path_to_file_url` 百分号编码（P6.2，互操作缺陷）

**位置**：`crates/dd-ext/src/bin/search.rs:106-115`（编码侧）；`crates/dd-gui/src/platform.rs:524-543`（解码侧 `file_url_candidates`，已有 literal→decode 双候选链，无需改）

**改法**：
1. search.rs 新增极简 RFC 3986 编码（零新依赖）：

```rust
/// file URL path 段编码：保留 unreserved + '/' + ':'（盘符冒号），
/// 其余字节（空格/%/#/?/控制/非 ASCII UTF-8 序列）一律 %XX。
fn percent_encode_path(s: &str) -> String { /* ~15 行，无新依赖 */ }
```

`path_to_file_url` 的两个分支（UNC / 本地盘）路径部分改走该函数；host 部分保持原样。
2. **同步更新既有断言（共 5 处，2026-10-09 复核；本修复唯一的测试冲突点，属预期内）**：
   - `search.rs:2011`（`path_to_file_url_local_drive_keeps_third_slash`）：纯 ASCII 主断言 `file:///C:/proj/src/main.rs` 不变，但 **2017-2024 行的 `%`/`#`「不编码」断言期望必变**——`report%20final.txt` → `file:///D:/a/report%2520final.txt`、`plan#2.txt` → `file:///D:/a/plan%232.txt`；
   - `search.rs:2028`（`path_to_file_url_unc_share_uses_host_authority`）：路径均为 unreserved 字符，期望不变；
   - `search.rs:2042`（`path_to_file_url_edge_cases_fall_back_to_local_form`）：`file://///server` 路径部分无保留字符，期望不变；
   - `search.rs:2053`（`handle_invoke_open_unc_path_builds_authority_url`，函数在 2054）：期望 `file://nas/public/报告.pdf` → `file://nas/public/%E6%8A%A5%E5%91%8A.pdf`，主机名 `nas` 保持原样；
   - 解码侧联动确认：`platform.rs` 双候选链（`file_url_candidates` 524-543，literal → percent-decode 兜底）已核实可还原上述编码 URL，消费侧零改动。
   `file:///C:/proj/src/main.rs` 等纯 ASCII 无保留字符用例期望**不变**。
3. **跨 crate 契约测试**（加在 dd-gui，消费侧）：

```rust
#[test]
fn resolve_file_url_accepts_percent_encoded_from_search_ext() {
    for p in [r"C:\my files\a%b#c.txt", r"C:\报告 final.txt"] {
        let url = /* 用与 search.rs 相同编码规则构造（测试内复制 8 行编码器，两处注释互指）*/;
        assert_eq!(resolve_file_url_to_path(&url).as_deref(), Some(p), "编码 URL 必须可还原原路径");
    }
}
```

**影响面与冲突**：search.rs（编码 + 既有测试更新）、dd-gui/platform.rs（仅新增测试）。对齐报告 §9.6"缺陷 1"的关闭条件。

**验收标准**：
1. `cargo test -p dd-ext search::` 与 `cargo test -p dd-gui platform::` 全绿；
2. 真机：Everything 搜索"报告 final.txt"→ 点击打开，关联程序正确打开该文件（修复前可能解析成"报告"或失败）；
3. `docs/file-search-result-redesign.html` / `docs/search-file.md` 中 URL 约定描述同步一句。

**工作量**：0.5 天。

---

## 批次四：合并顺序与冲突矩阵

| 文件 | 涉及项 | 冲突处理 |
|---|---|---|
| `dd-ext/src/builtins/shell.rs` | F1、F6 | 同 PR 可，分 commit：F1 改 285-319，F6 改 233-248，无行交集 |
| `dd-ext/src/builtins/calc.rs` | F2 | 独占 |
| `dd-protocol/src/framing.rs` | F3 | 独占 |
| `crates/*/Cargo.toml` ×6 | F4 | 独占（各加一行） |
| `dd-host/src/manifest.rs` | F4 | 独占 |
| `dd-ext/src/builtins/apps.rs` | F5 | 独占 |
| `dd-gui/src/state.rs` | F7 | 独占 |
| `dd-gui/src/app/keys.rs` | F7 | 独占 |
| `dd-gui/src/ui/panel.rs` | F7（回写 page_step）、F8（收集/分组） | **F7 先合，F8 rebase** |
| `dd-gui/src/ui/icons.rs` | F8（resolve_icons 借用化 + 缓存写后置） | 独占（随 F8 同 PR） |
| `dd-gui/src/platform.rs` | F9（590-608 区域）、F11（仅加测试） | 无交集，任意序 |
| `dd-gui/src/platform/capture_hook.rs` | F10 | 独占 |
| `.github/workflows/ci.yml` | F5 | 独占 |

**建议 PR 序列**（每个 PR 独立可回滚）：
1. `fix(ext): run_capture 管道排空 + calc 大数显示`（F1+F2+F6，同 crate 顺手）
2. `fix(protocol): NDJSON 解码器单遍扫描`（F3）
3. `chore: MSRV 1.77 + 清单不再补全 .cmd/.bat`（F4）
4. `feat(ui): Home/End/PgUp/PgDn 列表导航`（F7）
5. `perf(ui): 面板渲染去每帧克隆`（F8，rebase 在 4 之后）
6. `fix(gui): reveal 门控 + 钩子陈旧自愈 + file URL 编码`（F9+F10+F11）
7. `chore: 去 Steam 盘符硬编码 + CI skip 收口`（F5）
8. `docs: 台账/CHANGELOG/键位表/安全审计附录回填`（收尾 PR）

**文档同步清单**（随对应 PR 或收尾 PR）：
- `CHANGELOG.md` `[0.2.1]`：F1-F11 逐条（用户可感知项在前）；
- `docs/implementation.md` §6.1：每项一行（含 F8 的帧耗时前后数值、F3 的 release 用例耗时）；
- `docs/manifest-schema.md` §7 规则 8（F4）；设计稿 §2.2 键位表（F7）；
- `docs/security-audit-2026-09-23.md` 末尾追加"2026-10 外部复审附录"：P3/P6.1/P6.2/P7 作为新发现编号入账（沿用 S-xx 编号续位），注明 F9-F11 关闭；
- `docs/user-session-walkthrough-checklist.md`：F7 键位矩阵 + F1 大输出用例。

## F12 — 长期项：扩展签名（不在 v0.2.1）

纲要（立项用，不在本轮实施）：宿主内置发行方公钥 → 清单/二进制分离签名（`minisign` 风格 detached signature）→ trust 台账从"哈希变更检测"升级为"签名验证 + 哈希双锚"→ 关闭 R-12 升级重钉的静默信任窗口。前置：签名分发渠道与密钥保管流程决策。

---

## 验收总表

| 项 | 核心验收命令/操作 | 修复前行为（可先红） |
|---|---|---|
| F1 | `cargo test -p dd-ext shell::`；真机 type 大文件 | 300KB 输出必报"超时" |
| F2 | `cargo test -p dd-ext calc::`；GUI `2^128` | 显示 i128::MAX |
| F3 | `cargo test -p dd-protocol`（既有 11 例零改动全绿）+ 性能例 | debug 下万行级 chunk 秒级卡顿 |
| F4 | `cargo test -p dd-host`；rg 检查 rust-version | 旧工具链无 MSRV 拦截 |
| F5 | 无 Steam 的 CI 全绿；apps.rs 内盘符字面量仅剩测试夹具两行 | CI 依赖 --skip；盘符硬编码 |
| F6 | `cargo test -p dd-ext shell::`；真机 shutdown 弹窗 | 文案未声明"不拦截" |
| F7 | `cargo test -p dd-gui state::`；键位矩阵走查 | 四键无响应 |
| F8 | 全绿 + 500 项页帧耗时降 ≥30%（台账记录） | 每帧 2N 次深拷贝 |
| F9 | `cargo test -p dd-gui platform::`；真机 `%` 文件名 reveal | 无门控直通 explorer |
| F10 | 代码走查 + 20 次快速重开捕获真机规程 | 竞态窗口双钩子吞键 |
| F11 | dd-ext/dd-gui 双侧测试 + 真机中文+空格文件名打开 | 非法 URL 依赖宽松解析 |

**总工作量**：约 8–9 人日（2026-10-09 核对修正：F8 因 icons.rs 连带由 1.5 天上调至 2 天）；批次一可 1 人并行推进，批次四的 panel.rs 两项按序合并后，v0.2.1 可在修复全部合入后按既有 release 流程（tag 校验 → CI → zip + sha256）发布。
