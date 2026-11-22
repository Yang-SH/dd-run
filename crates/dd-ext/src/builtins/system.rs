//! dd-ext-system —— 内置「系统」扩展（`com.ddrun.system`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：System 锁屏等系统命令；N7 扩表）：
//! - 顶层固定命令（frozen）：锁屏 / 睡眠 / 关机 / 重启 / 注销（M4 五件套）；
//!   **N7 新增**：Windows 设置页（嵌套页 52 条，`get_items` 供给）/
//!   清空回收站（`SHEmptyRecycleBinW`，危险项走 Confirm）/
//!   音量与媒体键（`SendInput` 注入虚拟键 ×7）；
//! - 关机 / 重启 / 注销 / 清空回收站为**危险操作** → invoke 先回
//!   `Confirm{is_critical:true}`，宿主带 `context.confirmed=true` 重发后才真正
//!   执行（§8.3）；
//! - 执行成功 → `Dismiss`（关闭面板）；执行失败 → `ShowToast` 报错。
//!
//! 平台策略（P4 决策：Windows 优先）：Windows 实现 `rundll32` / `shutdown` /
//! `explorer.exe ms-settings:` / `SHEmptyRecycleBinW` / `SendInput`（本机真机
//! 验证）；macOS / Linux 自 S3 起为**真实现**——macOS `pmset`/`osascript`
//! （System Events）、Linux `systemctl`/`loginctl`（见 not(windows) `sys`
//! 分支）；执行语义（Confirm 门禁 / 危险项）跨平台同构。N7 的设置页与音量/
//! 媒体/回收站为 Windows 专属面（unix 分支不编译、不出现对应命令）。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use crate::{i18n::tr, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandResult};
#[cfg(windows)]
use dd_protocol::model::{CommandRef, Icon, IconKind};

pub fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.system",
        display_name: tr("系统", "System"),
        description: tr(
            "锁屏 / 睡眠 / 关机 / 重启 / 注销 / Windows 设置页 / 清空回收站 / 音量与媒体",
            "Lock / Sleep / Shut down / Restart / Sign out / Windows Settings / Empty Recycle Bin / Volume & media",
        ),
        frozen: true,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-system",
        pages: sys::pages_handler(),
        top_level: sys::top_level_commands,
        fallback: None,
        invoke: sys::handle_invoke,
    }
}

/// 平台命令表与执行逻辑（按 OS 分实现）。
#[cfg(windows)]
mod sys {
    use super::*;
    use crate::Effect;
    use dd_protocol::messages::{GetItemsParams, GetItemsResult};

    /// Windows 设置页 page_id（N7）。
    pub const WIN_SETTINGS_PAGE: &str = "system.winsettings";

    /// 执行方式（N7）：spawn 子进程（既有五件套）/ 清空回收站（`SHEmptyRecycleBinW`
    /// 直调，零子进程）/ 注入虚拟键（`SendInput` down+up，音量与媒体）/ 进入
    /// Windows 设置嵌套页。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Exec {
        Spawn,
        EmptyBin,
        SendVk(u16),
        OpenSettings,
    }

    /// 一条系统命令：id / 标题 / 副标题 / 确认描述 / 是否危险 / 程序 / 参数 /
    /// 分组 / 执行方式。
    #[derive(Debug, Clone, Copy)]
    pub struct SystemCommand {
        pub id: &'static str,
        pub title: &'static str,
        pub subtitle: &'static str,
        /// Confirm 对话框描述（仅危险项使用）
        pub confirm_description: &'static str,
        /// 危险操作（关机/重启/注销/清空回收站）需 `Confirm` 二次确认
        pub dangerous: bool,
        pub program: &'static str,
        pub args: &'static [&'static str],
        /// 首屏分组（tr 解析后的 `&'static str`，`tr` 返回静态串）
        pub section: &'static str,
        pub exec: Exec,
    }

    /// 系统命令表（N7：五件套 + Windows 设置页 + 清空回收站 + 音量/媒体键 ×7）。
    ///
    /// **每次调用重建、不做 `OnceLock` 缓存**：`tr` 输出随进程语言可重设
    /// （i18n `AtomicU8` + `set_lang` 语义），OnceLock 会把标题冻结在首次语言
    /// ——与 M9 语言切换语义相悖，本表去缓存顺带修正（原五件套同患）。
    pub fn commands() -> Vec<SystemCommand> {
        let sec_sys = tr("系统", "System");
        let sec_media = tr("音量与媒体", "Volume & media");
        vec![
            SystemCommand {
                id: "system.settings",
                title: tr("Windows 设置", "Windows Settings"),
                subtitle: tr(
                    "打开 Windows 设置页列表（显示/声音/网络/更新…52 条）",
                    "Open the Windows Settings page list (display, sound, network, update… 52 items)",
                ),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_sys,
                exec: Exec::OpenSettings,
            },
            SystemCommand {
                id: "system.lock",
                title: tr("锁屏", "Lock Screen"),
                subtitle: tr(
                    "立即锁定工作站（rundll32 LockWorkStation）",
                    "Lock the workstation immediately (rundll32 LockWorkStation)",
                ),
                confirm_description: "",
                dangerous: false,
                program: "rundll32.exe",
                args: &["user32.dll,LockWorkStation"],
                section: sec_sys,
                exec: Exec::Spawn,
            },
            SystemCommand {
                id: "system.sleep",
                title: tr("睡眠", "Sleep"),
                subtitle: tr(
                    "使计算机进入睡眠（可唤醒）",
                    "Put the computer to sleep (wakeable)",
                ),
                confirm_description: "",
                dangerous: false,
                program: "rundll32.exe",
                args: &["powrprof.dll,SetSuspendState", "0,1,0"],
                section: sec_sys,
                exec: Exec::Spawn,
            },
            SystemCommand {
                id: "system.shutdown",
                title: tr("关机", "Shut Down"),
                subtitle: tr(
                    "关闭计算机（⚠ 危险，需二次确认）",
                    "Shut down the computer (⚠ dangerous, needs confirmation)",
                ),
                confirm_description: tr(
                    "计算机会立即关闭，未保存的工作将丢失。",
                    "The computer will shut down immediately; unsaved work will be lost.",
                ),
                dangerous: true,
                program: "shutdown.exe",
                args: &["/s", "/t", "0"],
                section: sec_sys,
                exec: Exec::Spawn,
            },
            SystemCommand {
                id: "system.restart",
                title: tr("重启", "Restart"),
                subtitle: tr(
                    "重启计算机（⚠ 危险，需二次确认）",
                    "Restart the computer (⚠ dangerous, needs confirmation)",
                ),
                confirm_description: tr(
                    "计算机会立即重启，未保存的工作将丢失。",
                    "The computer will restart immediately; unsaved work will be lost.",
                ),
                dangerous: true,
                program: "shutdown.exe",
                args: &["/r", "/t", "0"],
                section: sec_sys,
                exec: Exec::Spawn,
            },
            SystemCommand {
                id: "system.logoff",
                title: tr("注销", "Sign Out"),
                subtitle: tr(
                    "注销当前用户（⚠ 危险，需二次确认）",
                    "Sign out the current user (⚠ dangerous, needs confirmation)",
                ),
                confirm_description: tr(
                    "当前会话将结束，未保存的工作可能丢失。",
                    "The current session will end; unsaved work may be lost.",
                ),
                dangerous: true,
                program: "shutdown.exe",
                args: &["/l"],
                section: sec_sys,
                exec: Exec::Spawn,
            },
            SystemCommand {
                id: "system.emptybin",
                title: tr("清空回收站", "Empty Recycle Bin"),
                subtitle: tr(
                    "清空所有驱动器的回收站（⚠ 不可恢复，需二次确认）",
                    "Empty the Recycle Bin on all drives (⚠ irreversible, needs confirmation)",
                ),
                confirm_description: tr(
                    "回收站中的所有项目将被永久删除，无法恢复。",
                    "All items in the Recycle Bin will be permanently deleted and cannot be recovered.",
                ),
                dangerous: true,
                program: "",
                args: &[],
                section: sec_sys,
                exec: Exec::EmptyBin,
            },
            SystemCommand {
                id: "system.volume.mute",
                title: tr("静音切换", "Toggle Mute"),
                subtitle: tr("切换系统静音（虚拟键）", "Toggle system mute (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_VOLUME_MUTE),
            },
            SystemCommand {
                id: "system.volume.down",
                title: tr("音量减", "Volume Down"),
                subtitle: tr("降低系统音量（虚拟键）", "Lower system volume (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_VOLUME_DOWN),
            },
            SystemCommand {
                id: "system.volume.up",
                title: tr("音量加", "Volume Up"),
                subtitle: tr("提高系统音量（虚拟键）", "Raise system volume (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_VOLUME_UP),
            },
            SystemCommand {
                id: "system.media.playpause",
                title: tr("播放/暂停", "Play / Pause"),
                subtitle: tr("切换媒体播放状态（虚拟键）", "Toggle media playback (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_MEDIA_PLAY_PAUSE),
            },
            SystemCommand {
                id: "system.media.next",
                title: tr("下一曲", "Next Track"),
                subtitle: tr("跳到下一曲目（虚拟键）", "Skip to the next track (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_MEDIA_NEXT_TRACK),
            },
            SystemCommand {
                id: "system.media.prev",
                title: tr("上一曲", "Previous Track"),
                subtitle: tr("跳到上一曲目（虚拟键）", "Skip to the previous track (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_MEDIA_PREV_TRACK),
            },
            SystemCommand {
                id: "system.media.stop",
                title: tr("停止播放", "Stop"),
                subtitle: tr("停止媒体播放（虚拟键）", "Stop media playback (virtual key)"),
                confirm_description: "",
                dangerous: false,
                program: "",
                args: &[],
                section: sec_media,
                exec: Exec::SendVk(vk::VK_MEDIA_STOP),
            },
        ]
    }

    /// `VK_*` 虚拟键常量别名（windows-sys `KeyboardAndMouse` 模块；取值
    /// 0xB0–0xB3 / 0xAD–0xAF，user32 标准定义）。
    mod vk {
        pub const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
        pub const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
        pub const VK_MEDIA_STOP: u16 = 0xB2;
        pub const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;
        pub const VK_VOLUME_MUTE: u16 = 0xAD;
        pub const VK_VOLUME_DOWN: u16 = 0xAE;
        pub const VK_VOLUME_UP: u16 = 0xAF;
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        commands()
            .iter()
            .map(|c| CommandItem {
                id: c.id.to_string(),
                title: c.title.to_string(),
                subtitle: Some(c.subtitle.to_string()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: "\u{E7E8}".to_string(), // PowerButton（N7 不扩 glyph，图标细化属 icons 渠道）
                }),
                section: Some(c.section.to_string()),
                tags: Some(if c.dangerous {
                    vec!["system".to_string(), "danger".to_string()]
                } else {
                    vec!["system".to_string()]
                }),
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect()
    }

    /// Windows 设置页条目表（N7 附录 B；52 条，真机验收逐条收缩——单条打不开
    /// 即从表删除并记档）。元组 = (slug, zh, en)。
    const WIN_SETTINGS: &[(&str, &str, &str)] = &[
        ("display", "显示", "Display"),
        ("sound", "声音", "Sound"),
        ("notifications", "通知", "Notifications"),
        ("quiethours", "专注助手", "Focus assist"),
        ("powersleep", "电源和睡眠", "Power & sleep"),
        ("batterysaver", "电池", "Battery"),
        ("storagesense", "存储", "Storage"),
        ("multitasking", "多任务处理", "Multitasking"),
        ("project", "投影", "Projecting"),
        ("about", "关于", "About"),
        ("bluetooth", "蓝牙和设备", "Bluetooth & devices"),
        ("network", "网络和 Internet", "Network & internet"),
        ("wifi", "WLAN", "Wi-Fi"),
        ("network-ethernet", "以太网", "Ethernet"),
        ("network-vpn", "VPN", "VPN"),
        ("network-proxy", "代理", "Proxy"),
        ("mobilehotspot", "移动热点", "Mobile hotspot"),
        ("datausage", "数据使用量", "Data usage"),
        ("personalization", "个性化", "Personalization"),
        ("personalization-background", "背景", "Background"),
        ("colors", "颜色", "Colors"),
        ("lockscreen", "锁屏界面", "Lock screen"),
        ("themes", "主题", "Themes"),
        ("taskbar", "任务栏", "Taskbar"),
        ("personalization-start", "开始", "Start"),
        ("appsfeatures", "应用", "Apps"),
        ("defaultapps", "默认应用", "Default apps"),
        ("optionalfeatures", "可选功能", "Optional features"),
        ("startupapps", "启动应用", "Startup apps"),
        ("yourinfo", "账户信息", "Your info"),
        ("signinoptions", "登录选项", "Sign-in options"),
        ("dateandtime", "日期和时间", "Date & time"),
        ("regionlanguage", "语言和区域", "Language & region"),
        ("typing", "键入", "Typing"),
        ("gaming-gamebar", "游戏", "Gaming"),
        ("easeofaccess", "辅助功能", "Accessibility"),
        ("privacy", "隐私和安全性", "Privacy & security"),
        ("privacy-location", "位置", "Location"),
        ("privacy-microphone", "麦克风", "Microphone"),
        ("privacy-webcam", "相机", "Camera"),
        ("windowsdefender", "Windows 安全中心", "Windows Security"),
        ("windowsupdate", "Windows 更新", "Windows Update"),
        ("recovery", "恢复", "Recovery"),
        ("troubleshoot", "疑难解答", "Troubleshoot"),
        ("activation", "激活", "Activation"),
        ("findmydevice", "查找我的设备", "Find my device"),
        ("developers", "开发者选项", "For developers"),
        ("remotedesktop", "远程桌面", "Remote Desktop"),
        ("fonts", "字体", "Fonts"),
        ("autoplay", "自动播放", "AutoPlay"),
        ("backup", "备份", "Backup"),
        ("delivery-optimization", "传递优化", "Delivery Optimization"),
    ];

    /// 单条设置页条目（invoke 经 `explorer.exe ms-settings:<slug>` 打开）。
    fn win_setting_item(slug: &str, zh: &'static str, en: &'static str) -> CommandItem {
        let title = tr(zh, en);
        CommandItem {
            id: format!("system.ms.{slug}"),
            title: title.to_string(),
            subtitle: Some(tr("打开「{}」设置页", "Open Windows setting: {}").replace("{}", title)),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E7E8}".to_string(),
            }),
            section: Some(tr("Windows 设置", "Windows Settings").to_string()),
            tags: Some(vec!["settings".to_string(), slug.to_string()]),
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }
    }

    /// §6.3 `get_items` 处理器：仅认 [`WIN_SETTINGS_PAGE`]；`search_text` 非空时
    /// 按标题/slug **大小写不敏感包含**过滤（嵌套页搜索由扩展侧过滤，宿主不代滤
    /// ——fallback_flow 既有口径）；未知 page_id 回空页（不报错，宿主按空列表渲染）。
    pub fn handle_get_items(params: &GetItemsParams) -> GetItemsResult {
        if params.page_id != WIN_SETTINGS_PAGE {
            return GetItemsResult {
                items: Vec::new(),
                has_more_items: false,
                is_loading: false,
            };
        }
        let needle = params
            .search_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let items = WIN_SETTINGS
            .iter()
            .copied()
            .filter(|&(slug, zh, en)| match &needle {
                None => true,
                Some(q) => {
                    tr(zh, en).to_lowercase().contains(q)
                        || en.to_lowercase().contains(q)
                        || slug.to_lowercase().contains(q)
                }
            })
            .map(|(slug, zh, en)| win_setting_item(slug, zh, en))
            .collect();
        GetItemsResult {
            items,
            has_more_items: false,
            is_loading: false,
        }
    }

    /// 子页声明：Windows 挂设置页；unix 无（N7 为 Windows 专属面）。
    pub fn pages_handler() -> Option<crate::PageHandler> {
        Some(handle_get_items)
    }

    /// 纯决策层（无副作用，可安全单测）：
    /// 未知 id → `Err(ShowToast)`；危险命令未经确认 → `Err(Confirm{is_critical:true})`；
    /// 否则 `Ok(cmd)` 交由执行层（EmptyBin / SendVk 仅真机轮验证）。
    fn decide(params: &InvokeParams) -> Result<SystemCommand, CommandResult> {
        let confirmed = params
            .context
            .as_ref()
            .and_then(|c| c.confirmed)
            .unwrap_or(false);
        let Some(cmd) = commands().into_iter().find(|c| c.id == params.id) else {
            return Err(CommandResult::ShowToast {
                message: tr("未知系统命令：{}", "Unknown system command: {}")
                    .replace("{}", &params.id),
                duration_ms: Some(2_500),
            });
        };
        // §8.3：确认后宿主带 confirmed=true 重发 invoke
        if cmd.dangerous && !confirmed {
            return Err(CommandResult::Confirm {
                title: tr("确认{}？", "Confirm {}?").replace("{}", cmd.title),
                description: tr(
                    "{}该操作无法撤销，确认要执行吗？",
                    "{} This action cannot be undone. Continue?",
                )
                .replace("{}", cmd.confirm_description),
                confirm_label: tr("执行{}", "Execute {}").replace("{}", cmd.title),
                is_critical: true,
            });
        }
        Ok(cmd)
    }

    /// 设置页条目 id → slug（**白名单校验**：slug 必须在 [`WIN_SETTINGS`] 表内
    /// ——id 回环自宿主（源自本扩展自身产出），白名单防御纵深 + 纯函数可单测）。
    fn win_setting_slug(id: &str) -> Option<&'static str> {
        let slug = id.strip_prefix("system.ms.")?;
        WIN_SETTINGS
            .iter()
            .find(|(s, _, _)| *s == slug)
            .map(|(s, _, _)| *s)
    }

    /// 设置页条目 URI（纯函数，单测锁定形态）。
    fn ms_settings_uri(slug: &str) -> String {
        format!("ms-settings:{slug}")
    }

    /// 设置页条目执行：`explorer.exe ms-settings:<slug>`（非破坏性，无 Confirm；
    /// Spawn 通道复用 [`launch`] 的 CREATE_NO_WINDOW）。
    fn launch_ms_settings(slug: &str) -> (CommandResult, Vec<Effect>) {
        let uri = ms_settings_uri(slug);
        match launch("explorer.exe", &[uri.as_str()]) {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: tr("打开设置页失败：{e}", "Failed to open setting: {e}")
                        .replace("{e}", &e),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        // N7：设置页条目（`system.ms.<slug>`）——先于 `decide` 拦截（这些 id 不在
        // 命令表内，落入 decide 会误报「未知系统命令」；slug 白名单校验在先）。
        if let Some(slug) = win_setting_slug(&params.id) {
            return launch_ms_settings(slug);
        }
        let cmd = match decide(params) {
            Ok(cmd) => cmd,
            Err(result) => return (result, Vec::new()),
        };
        let outcome = match cmd.exec {
            // 「Windows 设置」命令 → 进入嵌套页（宿主 `GoToPage` → 页栈 push）
            Exec::OpenSettings => {
                return (
                    CommandResult::GoToPage {
                        page_id: WIN_SETTINGS_PAGE.to_string(),
                    },
                    Vec::new(),
                );
            }
            Exec::SendVk(v) => send_vk(v),
            Exec::EmptyBin => empty_recycle_bin(),
            Exec::Spawn => launch(cmd.program, cmd.args),
        };
        match outcome {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: tr("执行 {} 失败：{e}", "Failed to run {}: {e}")
                        .replace("{e}", &e)
                        .replace("{}", cmd.title),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    /// 启动系统命令（隐藏窗口，避免控制台闪现）。
    fn launch(program: &str, args: &[&str]) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        // CREATE_NO_WINDOW = 0x08000000
        cmd.creation_flags(0x0800_0000);
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 清空所有驱动器回收站（N7）：`SHEmptyRecycleBinW` 直调（shell32，
    /// `Win32_UI_Shell` 特性已在树）——跳过 shell 自带确认框/音效（应用层
    /// Confirm 门禁已把关，§8.3），进度 UI 交由系统。根路径传 null = 所有驱动器。
    fn empty_recycle_bin() -> Result<(), String> {
        use windows_sys::Win32::UI::Shell::{
            SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
        };
        let flags = SHERB_NOCONFIRMATION | SHERB_NOSOUND | SHERB_NOPROGRESSUI;
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
        if hr == 0 {
            Ok(())
        } else {
            let hex = format!("{hr:#x}");
            Err(tr(
                "SHEmptyRecycleBinW HRESULT={hr}",
                "SHEmptyRecycleBinW HRESULT={hr}",
            )
            .replace("{hr}", &hex))
        }
    }

    /// 注入虚拟键（N7 音量/媒体）：`SendInput` down + up 两事件（user32，
    /// `Win32_UI_Input_KeyboardAndMouse` 特性——本批新增，无新 crate）。
    /// 注入目标是系统级音量/媒体虚拟键，前台应用是否消费由系统决定。
    fn send_vk(vk: u16) -> Result<(), String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        };
        let mk = |flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [mk(0), mk(KEYEVENTF_KEYUP)];
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent == inputs.len() as u32 {
            Ok(())
        } else {
            Err(tr(
                "SendInput 被系统拒绝（sent={sent}）",
                "SendInput rejected by the system (sent={sent})",
            )
            .replace("{sent}", &sent.to_string()))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dd_protocol::model::Sender;

        fn invoke(id: &str, confirmed: bool) -> InvokeParams {
            InvokeParams {
                id: id.into(),
                sender: Sender::TopLevel,
                context: Some(dd_protocol::messages::InvokeContext {
                    query: None,
                    selected_item_id: None,
                    form_data: None,
                    confirmed: Some(confirmed),
                }),
            }
        }

        #[test]
        fn windows_table_shape() {
            let cmds = commands();
            // 五件套 + Windows 设置页 + 清空回收站 + 音量/媒体 ×7 = 14
            assert_eq!(cmds.len(), 14);
            let ids: Vec<&str> = cmds.iter().map(|c| c.id).collect();
            assert_eq!(
                ids,
                vec![
                    "system.settings",
                    "system.lock",
                    "system.sleep",
                    "system.shutdown",
                    "system.restart",
                    "system.logoff",
                    "system.emptybin",
                    "system.volume.mute",
                    "system.volume.down",
                    "system.volume.up",
                    "system.media.playpause",
                    "system.media.next",
                    "system.media.prev",
                    "system.media.stop",
                ]
            );
            // id 全局唯一
            let mut sorted = ids.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), ids.len(), "命令 id 必须唯一");
            // 危险项 = 关机/重启/注销/清空回收站 四项
            assert_eq!(cmds.iter().filter(|c| c.dangerous).count(), 4);
            // 音量/媒体段 7 条且全部 SendVk、非危险
            let media: Vec<&SystemCommand> = cmds
                .iter()
                .filter(|c| c.id.starts_with("system.media.") || c.id.starts_with("system.volume."))
                .collect();
            assert_eq!(media.len(), 7);
            assert!(media.iter().all(|c| !c.dangerous));
            assert!(media.iter().all(|c| matches!(c.exec, Exec::SendVk(_))));
        }

        #[test]
        fn settings_invoke_goes_to_page() {
            let (result, effects) = handle_invoke(&invoke("system.settings", false));
            assert!(effects.is_empty());
            match result {
                CommandResult::GoToPage { page_id } => {
                    assert_eq!(page_id, WIN_SETTINGS_PAGE);
                }
                other => panic!("settings 应回 GoToPage，实得 {other:?}"),
            }
        }

        #[test]
        fn settings_page_slug_lookup_and_uri() {
            assert_eq!(win_setting_slug("system.ms.display"), Some("display"));
            assert_eq!(
                win_setting_slug("system.ms.delivery-optimization"),
                Some("delivery-optimization")
            );
            assert_eq!(win_setting_slug("system.ms.nope"), None, "表外 slug 拒绝");
            assert_eq!(
                win_setting_slug("system.lock"),
                None,
                "非 ms.* 前缀不进设置页路径"
            );
            assert_eq!(ms_settings_uri("display"), "ms-settings:display");
        }

        #[test]
        fn settings_page_item_ids_route_before_decide() {
            // 拦截层存在性：表内 slug 经白名单先行（真实 spawn 不进单测，真机腿
            // 验证）；表外 `system.ms.*` 落 decide 的未知 toast（防御纵深双保险）
            assert!(win_setting_slug("system.ms.display").is_some());
            let result = decide(&invoke("system.ms.nope", false));
            assert!(matches!(result, Err(CommandResult::ShowToast { .. })));
        }

        #[test]
        fn destructive_new_command_requires_confirm() {
            // 清空回收站与关机同门禁：首次 invoke 必回 Confirm{is_critical}
            for id in ["system.shutdown", "system.emptybin"] {
                let result = decide(&invoke(id, false));
                assert!(
                    matches!(
                        result,
                        Err(CommandResult::Confirm {
                            is_critical: true,
                            ..
                        })
                    ),
                    "{id} 首次 invoke 应回 Confirm"
                );
            }
        }

        #[test]
        fn destructive_confirmed_then_passes_gate() {
            for id in ["system.shutdown", "system.emptybin"] {
                let cmd = decide(&invoke(id, true)).expect("confirmed=true 应绕过确认门禁");
                assert_eq!(cmd.id, id);
            }
        }

        #[test]
        fn safe_commands_pass_gate_without_confirm() {
            for id in ["system.lock", "system.volume.mute", "system.media.next"] {
                let cmd = decide(&invoke(id, false)).expect("安全命令无需确认");
                assert_eq!(cmd.id, id);
            }
        }

        #[test]
        fn media_vk_values_match_user32() {
            // user32 标准虚拟键取值（windows-sys KeyboardAndMouse 同值）
            let expect: &[(&str, u16)] = &[
                ("system.volume.mute", 0xAD),
                ("system.volume.down", 0xAE),
                ("system.volume.up", 0xAF),
                ("system.media.next", 0xB0),
                ("system.media.prev", 0xB1),
                ("system.media.stop", 0xB2),
                ("system.media.playpause", 0xB3),
            ];
            for (id, code) in expect {
                let cmd = commands().into_iter().find(|c| c.id == *id).unwrap();
                assert_eq!(cmd.exec, Exec::SendVk(*code), "{id} 虚拟键取值");
            }
        }

        #[test]
        fn settings_page_table_shape() {
            assert_eq!(WIN_SETTINGS.len(), 52);
            let mut slugs: Vec<&str> = WIN_SETTINGS.iter().map(|(s, _, _)| *s).collect();
            slugs.sort();
            let unique = slugs.len();
            slugs.dedup();
            assert_eq!(unique, slugs.len(), "设置页 slug 必须唯一");
            assert!(WIN_SETTINGS
                .iter()
                .all(|(_, zh, en)| !zh.is_empty() && !en.is_empty()));
        }

        #[test]
        fn settings_page_get_items_full_and_filtered() {
            // 全量 52 条，id 前缀与 slug 一致
            let full = handle_get_items(&GetItemsParams {
                page_id: WIN_SETTINGS_PAGE.into(),
                search_text: None,
            });
            assert_eq!(full.items.len(), 52);
            assert!(full.items.iter().all(|it| it.id.starts_with("system.ms.")));
            assert!(!full.has_more_items && !full.is_loading);
            // 过滤：英文大小写不敏感 + slug 命中
            let filtered = handle_get_items(&GetItemsParams {
                page_id: WIN_SETTINGS_PAGE.into(),
                search_text: Some("  WIFI ".into()),
            });
            assert_eq!(filtered.items.len(), 1);
            assert_eq!(filtered.items[0].id, "system.ms.wifi");
            let by_title = handle_get_items(&GetItemsParams {
                page_id: WIN_SETTINGS_PAGE.into(),
                search_text: Some("显示".into()),
            });
            assert!(by_title.items.iter().any(|it| it.id == "system.ms.display"));
            // 空白搜索词 = 不过滤
            let blank = handle_get_items(&GetItemsParams {
                page_id: WIN_SETTINGS_PAGE.into(),
                search_text: Some("   ".into()),
            });
            assert_eq!(blank.items.len(), 52);
        }

        #[test]
        fn unknown_page_returns_empty() {
            let empty = handle_get_items(&GetItemsParams {
                page_id: "system.nope".into(),
                search_text: None,
            });
            assert!(empty.items.is_empty());
        }

        #[test]
        fn unknown_id_toasts() {
            let result = decide(&invoke("system.nope", false));
            assert!(matches!(result, Err(CommandResult::ShowToast { .. })));
        }
    }
}

/// 非 Windows 实现（S3，2026-10-31）：macOS 走 `pmset` / `osascript`（System
/// Events，锁屏快捷键路径需辅助功能权限——真机腿 O6 ④ 复验）；Linux 走
/// systemd 通道（`systemctl` suspend/poweroff/reboot + `loginctl`
/// lock-session/terminate-session，均为无 root 用户命令）。决策层（Confirm
/// 门禁 / 危险项三项）与 Windows 分支同构同语义；执行走直接 spawn（无控制台
/// 窗口问题）。N7 的设置页 / 回收站 / 音量媒体为 Windows 专属，unix 分支不出现。
#[cfg(not(windows))]
mod sys {
    use super::*;
    use crate::Effect;
    use dd_protocol::model::{CommandRef, Icon, IconKind};

    /// 一条系统命令：id / 标题 / 副标题 / 确认描述 / 是否危险 / 程序 / 参数。
    #[derive(Debug, Clone, Copy)]
    pub struct SystemCommand {
        pub id: &'static str,
        pub title: &'static str,
        pub subtitle: &'static str,
        /// Confirm 对话框描述（仅危险项使用）
        pub confirm_description: &'static str,
        /// 危险操作（关机/重启/注销）需 `Confirm` 二次确认
        pub dangerous: bool,
        pub program: &'static str,
        pub args: &'static [&'static str],
    }

    /// 系统命令表（与 Windows 表五件套同构：5 项 / 危险项 3；文案经 `tr`；
    /// 每次调用重建、不缓存 tr 输出——语言可重设语义与 Windows 分支一致）。
    pub fn commands() -> Vec<SystemCommand> {
        if cfg!(target_os = "macos") {
            vec![
                SystemCommand {
                    id: "system.lock",
                    title: tr("锁屏", "Lock Screen"),
                    subtitle: tr(
                        "锁定屏幕（System Events；首次使用需授予辅助功能权限）",
                        "Lock the screen (System Events; Accessibility permission required on first use)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "osascript",
                    args: &[
                        "-e",
                        "tell application \"System Events\" to keystroke \"q\" using {command down, control down}",
                    ],
                },
                SystemCommand {
                    id: "system.sleep",
                    title: tr("睡眠", "Sleep"),
                    subtitle: tr(
                        "使计算机进入睡眠（可唤醒，pmset）",
                        "Put the computer to sleep (wakeable, pmset)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "pmset",
                    args: &["sleepnow"],
                },
                SystemCommand {
                    id: "system.shutdown",
                    title: tr("关机", "Shut Down"),
                    subtitle: tr(
                        "关闭计算机（⚠ 危险，需二次确认）",
                        "Shut down the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即关闭，未保存的工作将丢失。",
                        "The computer will shut down immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "osascript",
                    args: &["-e", "tell application \"System Events\" to shut down"],
                },
                SystemCommand {
                    id: "system.restart",
                    title: tr("重启", "Restart"),
                    subtitle: tr(
                        "重启计算机（⚠ 危险，需二次确认）",
                        "Restart the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即重启，未保存的工作将丢失。",
                        "The computer will restart immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "osascript",
                    args: &["-e", "tell application \"System Events\" to restart"],
                },
                SystemCommand {
                    id: "system.logoff",
                    title: tr("注销", "Sign Out"),
                    subtitle: tr(
                        "注销当前用户（⚠ 危险，需二次确认）",
                        "Sign out the current user (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "当前会话将结束，未保存的工作可能丢失。",
                        "The current session will end; unsaved work may be lost.",
                    ),
                    dangerous: true,
                    program: "osascript",
                    args: &["-e", "tell application \"System Events\" to log out"],
                },
            ]
        } else {
            vec![
                SystemCommand {
                    id: "system.lock",
                    title: tr("锁屏", "Lock Screen"),
                    subtitle: tr(
                        "锁定当前会话（loginctl，systemd）",
                        "Lock the current session (loginctl, systemd)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "loginctl",
                    args: &["lock-session"],
                },
                SystemCommand {
                    id: "system.sleep",
                    title: tr("睡眠", "Sleep"),
                    subtitle: tr(
                        "使计算机进入睡眠（可唤醒，systemctl suspend）",
                        "Put the computer to sleep (wakeable, systemctl suspend)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "systemctl",
                    args: &["suspend"],
                },
                SystemCommand {
                    id: "system.shutdown",
                    title: tr("关机", "Shut Down"),
                    subtitle: tr(
                        "关闭计算机（⚠ 危险，需二次确认）",
                        "Shut down the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即关闭，未保存的工作将丢失。",
                        "The computer will shut down immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "systemctl",
                    args: &["poweroff"],
                },
                SystemCommand {
                    id: "system.restart",
                    title: tr("重启", "Restart"),
                    subtitle: tr(
                        "重启计算机（⚠ 危险，需二次确认）",
                        "Restart the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即重启，未保存的工作将丢失。",
                        "The computer will restart immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "systemctl",
                    args: &["reboot"],
                },
                SystemCommand {
                    id: "system.logoff",
                    title: tr("注销", "Sign Out"),
                    subtitle: tr(
                        "结束当前会话（⚠ 危险，需二次确认）",
                        "End the current session (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "当前会话将结束，未保存的工作可能丢失。",
                        "The current session will end; unsaved work may be lost.",
                    ),
                    dangerous: true,
                    program: "loginctl",
                    args: &["terminate-session"],
                },
            ]
        }
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        commands()
            .iter()
            .map(|c| CommandItem {
                id: c.id.to_string(),
                title: c.title.to_string(),
                subtitle: Some(c.subtitle.to_string()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: "\u{E7E8}".to_string(), // PowerButton
                }),
                section: Some(tr("系统", "System").to_string()),
                tags: Some(if c.dangerous {
                    vec!["system".to_string(), "danger".to_string()]
                } else {
                    vec!["system".to_string()]
                }),
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect()
    }

    /// 子页声明：unix 无设置页（N7 Windows 专属面）。
    pub fn pages_handler() -> Option<crate::PageHandler> {
        None
    }

    /// 纯决策层（无副作用，可安全单测）：与 Windows 分支同语义——未知 id →
    /// `Err(ShowToast)`；危险命令未经确认 → `Err(Confirm{is_critical:true})`；
    /// 否则 `Ok(cmd)` 交由 `launch` 执行。
    fn decide(params: &InvokeParams) -> Result<SystemCommand, CommandResult> {
        let confirmed = params
            .context
            .as_ref()
            .and_then(|c| c.confirmed)
            .unwrap_or(false);
        let Some(cmd) = commands().into_iter().find(|c| c.id == params.id) else {
            return Err(CommandResult::ShowToast {
                message: tr("未知系统命令：{}", "Unknown system command: {}")
                    .replace("{}", &params.id),
                duration_ms: Some(2_500),
            });
        };
        if cmd.dangerous && !confirmed {
            return Err(CommandResult::Confirm {
                title: tr("确认{}？", "Confirm {}?").replace("{}", cmd.title),
                description: tr(
                    "{}该操作无法撤销，确认要执行吗？",
                    "{} This action cannot be undone. Continue?",
                )
                .replace("{}", cmd.confirm_description),
                confirm_label: tr("执行{}", "Execute {}").replace("{}", cmd.title),
                is_critical: true,
            });
        }
        Ok(cmd)
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        let cmd = match decide(params) {
            Ok(cmd) => cmd,
            Err(result) => return (result, Vec::new()),
        };
        match launch(cmd.program, cmd.args) {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: tr("执行 {} 失败：{e}", "Failed to run {}: {e}")
                        .replace("{e}", &e.to_string())
                        .replace("{}", cmd.program),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    /// 启动系统命令（unix：直接 spawn，无控制台窗口问题）。
    fn launch(program: &str, args: &[&str]) -> Result<(), String> {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn catalog_has_five_known_commands() {
            assert_eq!(commands().len(), 5);
            let ids: Vec<&str> = commands().iter().map(|c| c.id).collect();
            assert_eq!(
                ids,
                vec![
                    "system.lock",
                    "system.sleep",
                    "system.shutdown",
                    "system.restart",
                    "system.logoff"
                ]
            );
            // 危险项恰好三项（与 Windows 表同构）
            assert_eq!(commands().iter().filter(|c| c.dangerous).count(), 3);
        }

        #[test]
        fn command_programs_follow_platform() {
            if cfg!(target_os = "macos") {
                assert!(commands()
                    .iter()
                    .all(|c| c.program == "osascript" || c.program == "pmset"));
            } else {
                assert!(commands()
                    .iter()
                    .all(|c| c.program == "systemctl" || c.program == "loginctl"));
            }
        }

        #[test]
        fn dangerous_command_first_invoke_confirms() {
            let p = dd_protocol::messages::InvokeParams {
                id: "system.shutdown".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let result = decide(&p);
            assert!(matches!(
                result,
                Err(CommandResult::Confirm {
                    is_critical: true,
                    ..
                })
            ));
        }

        #[test]
        fn dangerous_command_confirmed_then_passes_gate() {
            let ctx = dd_protocol::messages::InvokeContext {
                query: None,
                selected_item_id: None,
                form_data: None,
                confirmed: Some(true),
            };
            let p = dd_protocol::messages::InvokeParams {
                id: "system.shutdown".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: Some(ctx),
            };
            let cmd = decide(&p).expect("confirmed=true 应绕过确认门禁");
            assert_eq!(cmd.id, "system.shutdown");
        }

        #[test]
        fn safe_command_passes_gate_without_confirm() {
            let p = dd_protocol::messages::InvokeParams {
                id: "system.lock".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let cmd = decide(&p).expect("安全命令无需确认");
            assert_eq!(cmd.id, "system.lock");
        }

        #[test]
        fn unknown_id_toasts() {
            let p = dd_protocol::messages::InvokeParams {
                id: "system.nope".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let result = decide(&p);
            assert!(matches!(result, Err(CommandResult::ShowToast { .. })));
        }
    }
}
