//! O12（2026-10-11）拆分自 `app/keys.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：热键捕获模态（handle_hotkey_capture）与捕获应用/预设（apply_captured_hotkey / start·end_hotkey_capture / apply_hotkey_default / is_system_reserved_combo）。

use super::*;

impl PaletteApp {
    /// R-21（2026-09-30；2026-11-23 默认改 Alt+Space 后收窄）：系统**常用/保留**
    /// 组合黑名单——捕获确认时警示、不阻止（与 PowerToys「可能错误触发检测」
    /// 同语义；对话框内的「保存」即用户确认动作）。命中即全局劫持系统行为：
    /// - `Ctrl+Esc`：开始菜单；`Ctrl+Shift+Esc`：任务管理器；
    /// - `Alt+F4`：关闭窗口。
    ///
    /// `Alt+Space`（窗口菜单）已移出黑名单：RegisterHotKey 注册成功后该组合
    /// 被系统从投递链路摘除、系统菜单自然不弹，是默认组合的已知副作用，改由
    /// [`Self::is_menu_takeover_combo`] 给中性说明（不劝阻）。
    ///
    /// Win 系（Win+D/L/Tab…）不含：基础捕获本就录不进 Win 修饰（回落提示
    /// 已另行覆盖），LL 钩子可录但系统响应优先级更高，名单随真机反馈再扩。
    pub(crate) fn is_system_reserved_combo(mods: u32, vk: u32) -> bool {
        let m = mods & crate::settings::HOTKEY_MODS_MASK;
        matches!(
            (m, vk),
            (0x2, 0x1B) // Ctrl+Esc —— 开始菜单
                | (0x6, 0x1B) // Ctrl+Shift+Esc —— 任务管理器
                | (0x1, 0x73) // Alt+F4 —— 关闭窗口
        )
    }

    /// Alt+Space 接管判定（2026-11-23）：Alt 掩码**精确等于**1 且主键 = VK_SPACE。
    /// 与 R-21 黑名单分离——该组合注册成功后系统菜单不再弹出，是默认值带来的
    /// 可控副作用，UI 给中性说明而非「建议更换」。
    pub(crate) fn is_menu_takeover_combo(mods: u32, vk: u32) -> bool {
        let m = mods & crate::settings::HOTKEY_MODS_MASK;
        m == 0x1 && vk == 0x20
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
        if let Some(t) = self.hotkeys.capture_started {
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
        if let Some(rx) = self.hotkeys.capture_rx.as_ref() {
            let event = rx.try_recv().ok();
            match event {
                None => {} // 本帧无捕获事件
                Some(crate::platform::capture_hook::CaptureEvent::Ready) => {
                    // 钩子自验证通过 → 系统级捕获生效（Win 键可录）。
                    self.hotkeys.capture_failed = false;
                }
                Some(crate::platform::capture_hook::CaptureEvent::Failed(why)) => {
                    // 回落 egui 基础捕获 + 可见反馈（不再静默）。
                    log::error!("[dd-gui] 捕获钩子不可用：{why} —— 回落 egui 应用层捕获");
                    self.hotkeys.capture_failed = true;
                    self.hotkeys.capture_rx = None;
                    self.show_error_toast(
                        crate::text::t(self.view.lang_effective, "set.hotkey.capture_fallback")
                            .to_string(),
                    );
                }
                Some(crate::platform::capture_hook::CaptureEvent::Cancel) => {
                    self.end_hotkey_capture();
                }
                Some(crate::platform::capture_hook::CaptureEvent::Combo { mods, vk }) => {
                    // PowerToys 式（2026-09-30）：捕获**只记候选**，不写设置、
                    // 不触发注册——等用户在对话框点「保存」才走确认流。
                    self.hotkeys.hotkey_pending = Some((mods, vk));
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
            self.hotkeys.hotkey_pending = Some((mods, vk));
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
        self.hotkeys.hotkey_seq = self.hotkeys.hotkey_seq.wrapping_add(1);
        if self.hotkeys.hotkey_seq == crate::hotkey::HOTKEY_SEQ_STARTUP {
            self.hotkeys.hotkey_seq = 1;
        }
        self.hotkeys.hotkey_confirm = Some(super::super::HotkeyConfirm {
            seq: self.hotkeys.hotkey_seq,
            mods,
            vk,
            started: Instant::now(),
        });
        self.hotkeys.hotkey_apply_failed = false;
        self.hotkeys
            .hotkey
            .re_register(crate::hotkey::HotkeyCommand {
                seq: self.hotkeys.hotkey_seq,
                mods,
                vk,
            });
    }

    /// N17：命令槽绑定保存（对话框「保存」路径，capture_target = CommandSlot）
    /// ——校验（关键词命中 / 组合键冲突 / 槽位上限）→ 写设置 + 落盘 + 槽位
    /// 同步注册；失败行内报错（`command_slot_err`）+ 候选保留可重试；成功关
    /// 捕获 + toast。**不走 slot 0 的 seq 确认流**：槽位注册无「失败回滚」语义
    /// （失败槽位不生效，设置卡如实呈现），结果经 CommandSlots 事件 toast。
    pub(crate) fn apply_command_slot(&mut self, mods: u32, vk: u32) {
        let hk = crate::settings::CommandHotkey {
            mods,
            vk,
            keyword: self.hotkeys.command_slot_keyword.clone(),
        };
        match self.draft.settings.validate_command_hotkey(&hk) {
            Err(e) => {
                self.hotkeys.hotkey_apply_failed = false;
                self.hotkeys.command_slot_err = Some(e);
            }
            Ok(()) => {
                self.draft.settings.command_hotkeys.push(hk);
                self.save_settings_with_feedback();
                self.sync_command_hotkey_slots();
                self.end_hotkey_capture();
                self.hotkeys.command_slot_err = None;
                self.show_toast(
                    crate::text::t(self.view.lang_effective, "toast.command_slot_saved"),
                    Some(4_000),
                );
            }
        }
    }

    /// N17：命令热键槽同步——按 `settings.command_hotkeys` 打包发线程批量重
    /// 注册（generation 自增配对；slot 0 不动）。设置卡增删/启动装配共用。
    pub fn sync_command_hotkey_slots(&mut self) {
        self.hotkeys.slots_generation = self.hotkeys.slots_generation.wrapping_add(1);
        let generation = self.hotkeys.slots_generation;
        let slots: Vec<(u32, u32)> = self
            .draft
            .settings
            .command_hotkeys
            .iter()
            .map(|h| (h.mods, h.vk))
            .collect();
        #[cfg(windows)]
        {
            let packed = crate::hotkey::pack_command_slots(&slots);
            self.hotkeys.hotkey.re_register_slots(packed, generation);
        }
        #[cfg(not(windows))]
        {
            let mut arr = [(0u32, 0u32); 3];
            for (i, (m, v)) in slots.iter().take(3).enumerate() {
                arr[i] = (*m, *v);
            }
            self.hotkeys.hotkey.re_register_slots(arr, generation);
        }
    }

    /// 退出捕获模式并卸载钩子（Esc / 取消 / 超时 / 隐藏 / 保存成功共用出口）。
    /// 候选一并丢弃；**在途确认保留**——注册结果仍会回发，成功照常落设置
    ///（热键已真实注册，设置必须记录），失败转 toast（对话框已关，行内不可见）。
    pub(crate) fn end_hotkey_capture(&mut self) {
        self.hotkeys.hotkey_capturing = false;
        // Guard Drop = 卸载 LL 钩子（清发送端 + WM_QUIT + 泵循环自检退出）。
        self.hotkeys.capture_hook = None;
        self.hotkeys.capture_rx = None;
        self.hotkeys.capture_started = None;
        self.hotkeys.capture_failed = false;
        self.hotkeys.hotkey_pending = None;
    }

    /// 设置页「更改热键」：进入捕获模式（下一组合键生效，Esc 取消）。
    /// **重入 = 取消**（捕获中按钮文本为「捕获中…」，再点即退出捕获态，
    /// 同时防重复装钩子——重复装会先卸旧钩子线程再装新钩子，浪费且抖动）。
    /// v3：钩子安装**全异步**（UI 零阻塞）——`Ready` 前事件即达即可用；
    /// `Failed` 由 `handle_hotkey_capture` 处理（回落 egui + 可见反馈）。
    pub(crate) fn start_hotkey_capture(&mut self) {
        if self.hotkeys.hotkey_capturing {
            self.end_hotkey_capture();
            return;
        }
        self.hotkeys.capture_target = crate::app::CaptureTarget::Slot0;
        self.hotkeys.command_slot_err = None;
        self.begin_capture();
    }

    /// N17：设置页「录制组合键」（命令槽绑定）——与 slot 0 捕获共用模态，
    /// `capture_target = CommandSlot`：对话框「保存」按目标路由（校验 +
    /// 槽位同步，不走 seq 确认流）；关键词在对话框内下拉选择。
    pub(crate) fn start_command_hotkey_capture(&mut self) {
        if self.hotkeys.hotkey_capturing {
            self.end_hotkey_capture();
            return;
        }
        self.hotkeys.capture_target = crate::app::CaptureTarget::CommandSlot;
        self.hotkeys.command_slot_err = None;
        self.hotkeys.command_slot_keyword = String::new();
        self.begin_capture();
    }

    /// 捕获公共装配（flags + LL 钩子异步安装；Windows 主路径 + egui 回落）。
    fn begin_capture(&mut self) {
        self.hotkeys.hotkey_capturing = true;
        self.hotkeys.capture_started = Some(Instant::now());
        self.hotkeys.capture_failed = false;
        self.hotkeys.hotkey_pending = None;
        self.hotkeys.hotkey_apply_failed = false;
        // B 方案：Windows 主路径 = LL 键盘钩子（Win 键可录、开始菜单不弹）。
        #[cfg(windows)]
        {
            let (tx, rx) = mpsc::channel();
            match crate::platform::capture_hook::start(tx) {
                Ok(guard) => {
                    // 守卫立即接管（任何时候 drop 都会卸钩子）；安装/自回声
                    // 验证异步进行，结果经 capture_rx 回发（Ready/Failed）。
                    self.hotkeys.capture_hook = Some(guard);
                    self.hotkeys.capture_rx = Some(rx);
                }
                Err(e) => {
                    // 仅 spawn 失败会同步 Err——回落 egui 基础捕获 + 可见反馈。
                    log::error!("[dd-gui] 捕获钩子线程创建失败：{e} —— 回落 egui 应用层捕获");
                    self.hotkeys.capture_failed = true;
                    self.show_error_toast(
                        crate::text::t(self.view.lang_effective, "set.hotkey.capture_fallback")
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
            crate::settings::HOTKEY_MODS_DEFAULT,
            crate::settings::HOTKEY_VK_DEFAULT,
        );
    }
}
