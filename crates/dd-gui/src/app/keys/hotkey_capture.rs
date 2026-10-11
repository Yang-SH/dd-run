//! O12（2026-10-11）拆分自 `app/keys.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：热键捕获模态（handle_hotkey_capture）与捕获应用/预设（apply_captured_hotkey / start·end_hotkey_capture / apply_hotkey_default / is_system_reserved_combo）。

use super::*;

impl PaletteApp {
    /// R-21（2026-09-30）：系统**常用/保留**组合黑名单——捕获确认时警示、
    /// 不阻止（与 PowerToys「可能错误触发检测」同语义；对话框内的「保存」
    /// 即用户确认动作）。命中即全局劫持系统行为：
    /// - `Alt+Space`：打开当前窗口菜单（RegisterHotKey 可成功，劫持一切应用）；
    /// - `Ctrl+Esc`：开始菜单；`Ctrl+Shift+Esc`：任务管理器；
    /// - `Alt+F4`：关闭窗口。
    ///
    /// Win 系（Win+D/L/Tab…）不含：基础捕获本就录不进 Win 修饰（回落提示
    /// 已另行覆盖），LL 钩子可录但系统响应优先级更高，名单随真机反馈再扩。
    pub(crate) fn is_system_reserved_combo(mods: u32, vk: u32) -> bool {
        let m = mods & dd_gui::settings::HOTKEY_MODS_MASK;
        matches!(
            (m, vk),
            (0x1, 0x20) // Alt+Space —— 窗口菜单
                | (0x2, 0x1B) // Ctrl+Esc —— 开始菜单
                | (0x6, 0x1B) // Ctrl+Shift+Esc —— 任务管理器
                | (0x1, 0x73) // Alt+F4 —— 关闭窗口
        )
    }

    /// 热键捕获（M6 批次 6.3；2026-09-29 B 方案改造）：
    /// **Windows 走 LL 键盘钩子**（系统级捕获——egui 应用层收不到 Win 键，
    /// 按下即弹开始菜单 + 失焦隐藏，真机 bug 见 `platform::capture_hook`
    /// 模块注释）；钩子安装失败回落 egui 应用层事件路径（Win 不可录）。
    /// Esc 取消；含 Ctrl/Alt/Win 的候选键生效（纯 Shift/无修饰忽略）。
    ///
    /// egui 回落路径保留逐事件检查（v4.7 真机反馈修订）：组合键的按下/释放
    /// 常在同一事件批次内完成，`i.modifiers` 快照已是释放后的状态——读
    /// `Event::Key` 自带的按下时刻 modifiers，精确可靠。
    pub(super) fn handle_hotkey_capture(&mut self, ctx: &egui::Context) {
        // 超时兜底：钩子是全局模态（吞全键盘），超时自动取消防用户键盘
        // 「失灵」（Esc 始终可取消，本值为双保险）。
        if let Some(t) = self.capture_started {
            if t.elapsed() > crate::platform::capture_hook::CAPTURE_TIMEOUT {
                log::debug!("[dd-gui] 热键捕获超时（30s），自动取消");
                self.end_hotkey_capture();
                return;
            }
        }
        // 钩子事件优先（Windows 主路径）。每帧处理至多一个事件（捕获期
        // 击键速率远低于帧率，无需批量排空）。安装**异步**（v3）：`Ready`
        // 前事件即可能达（安装通常 ms 级）；`Failed` → 清 `capture_rx` 切换
        // egui 基础路径（Ctrl/Alt 组合继续可录）+ 可见反馈。
        if let Some(rx) = self.capture_rx.as_ref() {
            let event = rx.try_recv().ok();
            match event {
                None => {} // 本帧无捕获事件
                Some(crate::platform::capture_hook::CaptureEvent::Ready) => {
                    // 钩子自验证通过 → 系统级捕获生效（Win 键可录）。
                    self.capture_failed = false;
                }
                Some(crate::platform::capture_hook::CaptureEvent::Failed(why)) => {
                    // 回落 egui 基础捕获 + 可见反馈（不再静默）。
                    log::error!("[dd-gui] 捕获钩子不可用：{why} —— 回落 egui 应用层捕获");
                    self.capture_failed = true;
                    self.capture_rx = None;
                    self.show_error_toast(
                        crate::text::t(self.lang_effective, "set.hotkey.capture_fallback")
                            .to_string(),
                    );
                }
                Some(crate::platform::capture_hook::CaptureEvent::Cancel) => {
                    self.end_hotkey_capture();
                }
                Some(crate::platform::capture_hook::CaptureEvent::Combo { mods, vk }) => {
                    // PowerToys 式（2026-09-30）：捕获**只记候选**，不写设置、
                    // 不触发注册——等用户在对话框点「保存」才走确认流。
                    self.hotkey_pending = Some((mods, vk));
                }
            }
            return;
        }
        // egui 应用层回落路径（钩子安装失败 / 非 Windows）。
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
        for ev in &events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = ev
            else {
                continue;
            };
            if *key == egui::Key::Escape {
                self.end_hotkey_capture();
                return;
            }
            let Some(vk) = Self::capture_vk(*key) else {
                continue;
            };
            let mods = ((modifiers.ctrl as u32) << 1)
                | (modifiers.alt as u32)
                | ((modifiers.shift as u32) << 2);
            if mods & 0b0011 == 0 {
                continue; // 需含 Ctrl/Alt：忽略本次按键，继续等待
            }
            // 回落路径与钩子路径同语义：只记候选，等「保存」确认（见钩子分支）。
            self.hotkey_pending = Some((mods, vk));
            return;
        }
    }

    /// 组合键落位（PowerToys 式确认流，2026-09-30）：**设置永不提前写入**——
    /// 发出带 seq 的注册请求，`ReRegistered{seq}` 成功回发后才写设置（见
    /// `poll_hotkey`）；失败/超时行内报错并保留候选，可直接重试。
    /// 旧实现（先写设置 + 单槽 `hotkey_prev` 顺序配对回滚）在重试场景会因
    /// 回滚路径多余的 `re_register(old)` 产生「假成功」事件错位配对，导致
    /// 「冲突只拦截一次、第二次假成功」——真机 2026-09-30，seq 协议根治。
    pub(crate) fn apply_captured_hotkey(&mut self, mods: u32, vk: u32) {
        // 序号发号（0 保留给启动注册结果；u32 回绕不可能达，守卫仅为完备）。
        self.hotkey_seq = self.hotkey_seq.wrapping_add(1);
        if self.hotkey_seq == dd_gui::hotkey::HOTKEY_SEQ_STARTUP {
            self.hotkey_seq = 1;
        }
        self.hotkey_confirm = Some(super::super::HotkeyConfirm {
            seq: self.hotkey_seq,
            mods,
            vk,
            started: Instant::now(),
        });
        self.hotkey_apply_failed = false;
        self.hotkey.re_register(dd_gui::hotkey::HotkeyCommand {
            seq: self.hotkey_seq,
            mods,
            vk,
        });
    }

    /// 退出捕获模式并卸载钩子（Esc / 取消 / 超时 / 隐藏 / 保存成功共用出口）。
    /// 候选一并丢弃；**在途确认保留**——注册结果仍会回发，成功照常落设置
    ///（热键已真实注册，设置必须记录），失败转 toast（对话框已关，行内不可见）。
    pub(crate) fn end_hotkey_capture(&mut self) {
        self.hotkey_capturing = false;
        // Guard Drop = 卸载 LL 钩子（清发送端 + WM_QUIT + 泵循环自检退出）。
        self.capture_hook = None;
        self.capture_rx = None;
        self.capture_started = None;
        self.capture_failed = false;
        self.hotkey_pending = None;
    }

    /// 设置页「更改热键」：进入捕获模式（下一组合键生效，Esc 取消）。
    /// **重入 = 取消**（捕获中按钮文本为「捕获中…」，再点即退出捕获态，
    /// 同时防重复装钩子——重复装会先卸旧钩子线程再装新钩子，浪费且抖动）。
    /// v3：钩子安装**全异步**（UI 零阻塞）——`Ready` 前事件即达即可用；
    /// `Failed` 由 `handle_hotkey_capture` 处理（回落 egui + 可见反馈）。
    pub(crate) fn start_hotkey_capture(&mut self) {
        if self.hotkey_capturing {
            self.end_hotkey_capture();
            return;
        }
        self.hotkey_capturing = true;
        self.capture_started = Some(Instant::now());
        self.capture_failed = false;
        self.hotkey_pending = None;
        self.hotkey_apply_failed = false;
        // B 方案：Windows 主路径 = LL 键盘钩子（Win 键可录、开始菜单不弹）。
        #[cfg(windows)]
        {
            let (tx, rx) = mpsc::channel();
            match crate::platform::capture_hook::start(tx) {
                Ok(guard) => {
                    // 守卫立即接管（任何时候 drop 都会卸钩子）；安装/自回声
                    // 验证异步进行，结果经 capture_rx 回发（Ready/Failed）。
                    self.capture_hook = Some(guard);
                    self.capture_rx = Some(rx);
                }
                Err(e) => {
                    // 仅 spawn 失败会同步 Err——回落 egui 基础捕获 + 可见反馈。
                    log::error!("[dd-gui] 捕获钩子线程创建失败：{e} —— 回落 egui 应用层捕获");
                    self.capture_failed = true;
                    self.show_error_toast(
                        crate::text::t(self.lang_effective, "set.hotkey.capture_fallback")
                            .to_string(),
                    );
                }
            }
        }
    }

    /// 设置页「恢复默认热键」（M6 批次 6.3；2026-09-30 纳入 seq 确认流）：
    /// 与「保存」同一条 `apply_captured_hotkey` 路径——注册成功才写设置，
    /// 失败 toast（对话框未开，行内提示不可见）。旧实现先写设置再注册、
    /// 失败靠单槽快照回滚，与「保存」同款错位配对风险，一并根治。
    ///（捕获 UI 不支持 Win 修饰的局限已随 LL 钩子捕获解除，注释保留历史。）
    pub(crate) fn apply_hotkey_default(&mut self) {
        self.apply_captured_hotkey(
            dd_gui::settings::HOTKEY_MODS_DEFAULT,
            dd_gui::settings::HOTKEY_VK_DEFAULT,
        );
    }
}
