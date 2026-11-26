//! dd-gui 宿主面板（bin 入口）：egui 窗口骨架 + 页面栈渲染 + 命令执行链路。
//!
//! M1–M3 职责（见 [`docs/implementation.md`](../../docs/implementation.md)）：
//! 1. **键盘焦点**：FilterBox 有焦点时，`↑↓/Tab/Enter/Esc` 仍能经
//!    `ctx.input_mut(|i| i.consume_key(...))` 在应用层可靠拦截（A11）；
//! 2. **窗口行为**：无边框、置顶、初始隐藏、失焦自动隐藏、热键唤起（A1）；
//! 3. **Root View（界面 01）**：FilterBox + 分组列表（Section）+ tags chip + 页脚键位提示；
//!    数据来自首屏聚合（扫描扩展 → 并行拉取 `top_level_commands`），失败扩展不阻塞整体；
//! 4. **页面栈**（M2）：`CommandRef::Page` 命令进入嵌套页（后台 `get_items`），
//!    `Esc` 非 Root 先返回、Root 再隐藏；`GoBack`/`GoHome` 由结果类型驱动（A5）；
//! 5. **命令执行**（M2）：Enter → 后台 `invoke` → 8 种 `CommandResultKind` 裁决为
//!    宿主动作（关闭/隐藏/回首页/返回/保持/跳页/Toast/确认，A4）；`Confirm` 弹确认框，
//!    确认后带 `context.confirmed = true` 重发（协议 §8.3 注）；
//! 6. **列表刷新**（M2）：`items_changed` 通知 → 100ms 合并 → 全量重拉 `get_items`
//!    （协议 §6.3 + 验收 A9：协议层不做增量推送）；
//! 7. **缓存与懒加载**（M3）：冷启动按 `frozen` 分流——磁盘桩命中直接渲染、**不拉起进程**
//!    （A6，桩缓存由 `dd-host::cache::FrozenCache` 落盘）；点击桩项走**复热链路**
//!    （spawn → initialize → `get_command` → 执行，协议 §6.4）；warm 进程经
//!    `LruWarmSet` 保活、超容驱逐回落 stub（A7）；`ColdStartTimer` 埋点供 A2 实测。
//!
//! eframe 0.36 的 `App` trait 为 `ui()` + `logic()` 两个回调：
//! - `logic`：窗口**隐藏时也会被调用**（经 `request_repaint` 唤醒）→ 热键与失焦；
//! - `ui`：窗口可见/需重绘时调用 → 后台结果轮询、键盘导航与绘制。

// 发布版（release）不弹命令行控制台（GUI 程序，日志随 stderr 丢弃）；
// debug / 测试构建保留控制台，便于看 eprintln 探针与冷启动计时。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//!
//! 2026-09-05 分层重构（方案 1，docs/refactor-layering-plan.md）：实现已拆分至
//! dd_gui::{app, ui, platform, text}（业务/绘制/系统副作用/纯函数），
//! 本文件仅保留进程入口 main()。

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use eframe::egui;

use dd_gui::app::{spawn_aggregation, PaletteApp, APP_H, APP_W, OFFSCREEN_X, OFFSCREEN_Y};
use dd_gui::crashlog;
use dd_gui::hotkey::HotkeyThread;
use dd_gui::platform::setup_cjk_fonts;
use dd_gui::theme;
use dd_gui::tray::TrayThread;
use dd_host::cache::{ColdStartTimer, FrozenCache};
use dd_host::manifest;

fn main() -> eframe::Result {
    // R-25：panic 取证 hook（进程入口最早挂上——后续任何阶段的未知崩溃都
    // 追加写 `%APPDATA%\dd-run\logs\panic.log`；hook 内失败静默放弃，默认
    // panic 流程不变，见 crashlog 模块文档）。
    crashlog::install();

    // O4 + P2-5.3：装配日志后端。stderr 口径不变（级别取 `DDRUN_LOG`，未设时 =
    // debug，与接入前等价）；P2-5.3 起增加文件腿 `%APPDATA%\dd-run\logs\dd-run.log`
    // （默认 Info，滚动 5×1 MB）——release GUI 子系统无控制台，日志自此**留存**。
    // 排障仍可终端重定向 stderr：`dd-run.exe 2> dd-run.log`。
    dd_gui::file_log::init();

    // R-26：单实例互斥（方案：O4 init 之后、eframe 创建之前）——双开第二实例
    // 弹「已在运行」提示后退出，见函数文档。
    enforce_single_instance();

    // A2 冷启动计时起点：**进程进入 main 即开始**，覆盖 eframe 窗口创建 +
    // 聚合全过程（M6 批次 6.2 起 CJK 字体改为后台线程加载，不在主路径上——
    // 旧注释"覆盖字体加载 22MB"已随异步化失效，首帧用 egui 默认字体）。
    let mut cold = ColdStartTimer::new();
    cold.mark_spawn_start();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([APP_W, APP_H])
        // v4.7 D31：透明视觉恒创建——材质生效时 DWM 系统材质从面板半透明
        // 浓淡层下透出（M1 2026-09-13）；未生效（Win10 / 22621 以下回退）时
        // 面板不透明填充完整覆盖，视觉与既往一致。
        .with_transparent(true)
        // 启动静默双保险之二（其一为 with_visible(false)）：初始位置屏幕外 +
        // 初始不活跃。屏幕外位置保证 eframe 首帧强制 set_visible(true) 时
        // 用户看不到任何窗口；with_active(false) 防止强制显示瞬间夺取焦点
        // （焦点要等用户热键唤起时才给，设计稿 03 验收行）。
        .with_position([OFFSCREEN_X, OFFSCREEN_Y])
        .with_active(false)
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        // v4.10 D36：程序化允许缩放（原生边框仍隐藏，缩放由 ui/chrome.rs 的
        // 8 方向边缘热区接管，BeginResize 原生循环）；下限防缩破布局。
        .with_resizable(true)
        .with_min_inner_size([460.0, 400.0])
        .with_visible(false); // 初始隐藏，热键唤起
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "dd-run",
        options,
        Box::new(|cc| {
            setup_cjk_fonts(&cc.egui_ctx);
            // M5 批次 3：注册亮/暗双视觉 + 主题偏好（批次 4.0 起偏好来自
            // 持久化配置，缺省跟随系统；须在任何绘制前应用，egui 会按
            // 系统亮暗自动在两套 Style 间 re-resolve）。
            let settings = dd_gui::settings::Settings::load();
            // v4.13 D38：托盘菜单语言与生效语言同步（菜单每次右键即席创建，
            // 读共享原子量；此处设初值，设置页切换经 apply_lang 更新）。
            let resolved_lang = match settings.lang {
                dd_gui::settings::Lang::FollowSystem => dd_gui::platform::system_ui_lang(),
                l => l,
            };
            dd_gui::tray::set_tray_lang(resolved_lang);
            // R-19：托盘 tooltip / 菜单尾缀跟随当前热键组合（托盘线程 spawn 前
            // 写入初值；改绑注册成功后由 poll_hotkey 同步）。
            dd_gui::tray::set_tray_hotkey_label(dd_gui::settings::hotkey_combo_label(
                settings.hotkey_mods,
                settings.hotkey_vk,
            ));
            // v4.7 D31：启动先按不透明注册（HWND 未捕获、材质成败未知）；
            // 首个 ui 帧捕获 HWND 后由 refresh_backdrop 按结果切换浓淡层（M1）。
            theme::apply(
                &cc.egui_ctx,
                theme::theme_preference(settings.theme),
                None,
                theme::Colorization::from_settings(&settings),
            );
            // 初始隐藏双保险：`with_visible(false)` 之外显式发 `Visible(false)`，
            // 规避 eframe/egui 0.36 在 Windows 上 `with_visible` 偶发不生效的情况。
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
            let hotkey = HotkeyThread::spawn(
                cc.egui_ctx.clone(),
                settings.hotkey_mods,
                settings.hotkey_vk,
            );
            // 系统托盘（设计稿 10C：常驻图标 + 左键 toggle + 右键原生菜单 +
            // 唯一退出入口；失败线程内降级，不影响面板）。click_flag = Toggle
            // 点击在途旗标（修复失焦隐藏与 Toggle 的「闪黑又展示」竞态）。
            let tray_click_flag = Arc::new(AtomicBool::new(false));
            let tray = TrayThread::spawn(cc.egui_ctx.clone(), Arc::clone(&tray_click_flag));
            // v4.16 真机修复（拖拽后面板空白）重绘看门狗：面板打开期间每秒
            // 强制重绘一次——自愈「GL surface 在原生拖拽/缩放模态循环内
            // present 失败遗留的空白表面」（循环内无输入事件时 egui 不会自行
            // 重绘，空白会一直停留）。隐藏期间（panel_open=false）零唤醒。
            let panel_open = Arc::new(AtomicBool::new(false));
            {
                let ctx = cc.egui_ctx.clone();
                let open = Arc::clone(&panel_open);
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if open.load(std::sync::atomic::Ordering::Relaxed) {
                        ctx.request_repaint();
                    }
                });
            }
            // M3 磁盘桩缓存（读桩不拉起进程 A6；目录 = 数据根目录/cache）
            // PaletteApp 持有一份副本：设置页搜索引擎变更触发重聚合时复用。
            let cache = manifest::cache_dir().map(FrozenCache::new);
            // M3 A2 冷启动计时起点：进程就绪即开始（聚合线程随后启动）
            let mut cold = ColdStartTimer::new();
            cold.mark_spawn_start();
            // 可配置搜索引擎（2026-09-05）：spawn 前把引擎表注入 websearch
            // 扩展环境（DD_WEBSEARCH_ENGINES），面板「网络搜索」按配置渲染。
            let engines_env = settings.search_engines_env();
            let disabled = settings.disabled_extensions.clone();
            let ext_settings = settings.ext_settings.clone();
            let (agg_tx, agg_rx) = mpsc::channel();
            spawn_aggregation(
                agg_tx,
                cache.clone(),
                engines_env,
                disabled,
                resolved_lang,
                ext_settings,
            );
            let mut app = PaletteApp::new(
                hotkey,
                tray.events,
                tray_click_flag,
                agg_rx,
                cold,
                cache,
                settings,
                panel_open,
            );
            // §5.1：装载使用频次（usage.json → pool.usage → Root 排序快照）。
            // 保持 PaletteApp::new 纯注入（8 参数不变），文件系统副作用在此显式装配。
            app.load_usage();
            // N20：装载搜索历史（history.json → 内存表；R-02 读写惯例同上）。
            app.load_query_history();
            // 加固②：启动骤降检测（config 命令数较备份环最新档骤降 → toast
            // 指引设置页恢复；备份缺失/为空/损坏静默不虚警）。
            app.check_startup_config_drop();
            // N17：命令热键槽启动同步（settings.command_hotkeys → 线程批量
            // 注册；slot 0 已在 spawn 时按 settings 注册）。
            app.sync_command_hotkey_slots();
            // §5.2：启动时检查更新（settings.check_updates，默认关；结果落地为
            // toast——面板隐藏期间暂存，下次唤起可见）。
            app.maybe_autostart_update_check();
            Ok(Box::new(app))
        }),
    )
}

/// R-26：单实例互斥（启动早期调用——O4 init 之后、eframe 创建之前）。
///
/// 双开的实际危害（方案 §3 R-26）：① 第二实例热键注册失败 → R-15「可能被
/// 其他程序占用」toast，**误导排障方向**（真凶是自己）；② 托盘双图标；
/// ③ config.json 并发写丢设置——R-02 原子写只保证文件不损坏，不解决
/// last-writer-wins。
///
/// **互斥体命名**：`Local\dd-run-single-instance`——`Local\` 前缀 = 本登录
/// 会话命名空间，不同终端会话（不同用户）各自独立，与面板 per-user 数据
/// 目录（`%APPDATA%\dd-run`）口径一致。
///
/// **句柄生命周期**：取得句柄后**有意不 CloseHandle**——命名互斥体随进程
/// 退出由内核释放，「句柄存活 = 实例存活」正是单实例语义；提前关闭会让
/// 后续双开误判「无实例运行」。第二实例命中 `ERROR_ALREADY_EXISTS` 后弹
/// `MessageBoxW`（阻塞至用户确认）再 `process::exit(0)`，句柄同样随进程
/// 释放。开机自启（HKCU Run 键执行 dd-run.exe）复启时命中的就是同一条
/// 第二实例路径。
///
/// 提示语中英并列（此阶段 GUI 语言尚未解析、i18n 未装配）；创建失败（极少）
/// 降级为不互斥照常启动——互斥是防误排障的护栏，不应成为启动失败的新单点。
#[cfg(windows)]
fn enforce_single_instance() {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
    };

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let name = to_wide("Local\\dd-run-single-instance");
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        log::warn!("[dd-gui] 单实例互斥体创建失败，降级为不互斥启动（照常运行）");
        return;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log::warn!("[dd-gui] 已有 dd-run 实例在运行（单实例互斥命中），弹窗提示后退出");
        let text = to_wide(
            "dd-run 已在运行（请用托盘图标或热键唤起面板），无需重复启动。\n\
             dd-run is already running (use the tray icon or the hotkey) — no need to start it again.",
        );
        let caption = to_wide("dd-run");
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
            );
        }
        std::process::exit(0);
    }
    // 首实例：持句柄至进程退出（不 CloseHandle，见函数文档），互斥体保持占用。
}

#[cfg(any(unix, test))]
fn same_file_identity(left: Option<(u64, u64)>, right: Option<(u64, u64)>) -> bool {
    matches!((left, right), (Some(a), Some(b)) if a == b)
}

#[cfg(any(unix, test))]
fn runtime_dir_is_private(metadata: Option<(bool, u32, u32)>, euid: u32) -> bool {
    matches!(metadata, Some((true, uid, 0o700)) if uid == euid)
}

#[cfg(any(unix, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketCleanup {
    Active,
    Stale,
    Foreign,
    NotSocket,
    Inconclusive,
    Missing,
}

#[cfg(any(unix, test))]
fn classify_socket_candidate(
    metadata: Option<(bool, u32)>,
    connect: Result<(), std::io::ErrorKind>,
    euid: u32,
) -> SocketCleanup {
    match connect {
        Ok(()) => SocketCleanup::Active,
        Err(std::io::ErrorKind::NotFound) => SocketCleanup::Missing,
        Err(error) => match metadata {
            Some((false, _)) => SocketCleanup::NotSocket,
            Some((true, uid)) if uid != euid => SocketCleanup::Foreign,
            Some((true, _)) if error == std::io::ErrorKind::ConnectionRefused => {
                SocketCleanup::Stale
            }
            _ => SocketCleanup::Inconclusive,
        },
    }
}

/// 非 Windows 单实例互斥（O6 批次三，2026-10-11；P1-15 加固）。
///
/// **路径**：优先当前用户 0700 的 `$XDG_RUNTIME_DIR/dd-run.sock`，否则
/// `$TMPDIR/dd-run-{euid}/dd-run.sock`。先 `bind`，只有 `AddrInUse` 才在跨进程
/// 限时 `flock` 保护下复查；候选先原子 rename 到私有隔离路径，再按同一 inode
/// 复查类型、属主与 `ConnectionRefused`，确认后才 unlink。普通文件、symlink、
/// 他人 socket 或证据不足的路径不被删除。
///
/// listener 有意泄漏至进程退出；清理锁文件保留，`flock` 随进程退出自动释放。
#[cfg(unix)]
#[derive(Debug)]
enum UnixAcquireError {
    AlreadyRunning,
    UnsafePath(String),
    Io(std::io::Error),
}

#[cfg(unix)]
unsafe extern "C" {
    fn geteuid() -> u32;
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(unix)]
fn lock_socket_cleanup(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    #[cfg(target_os = "linux")]
    const O_NOFOLLOW: i32 = 0o400000;
    #[cfg(target_os = "macos")]
    const O_NOFOLLOW: i32 = 0o400;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    const O_NOFOLLOW: i32 = 0;

    let mut lock_name = path.as_os_str().to_os_string();
    lock_name.push(".lock");
    let lock_path = std::path::PathBuf::from(lock_name);
    let euid = unsafe { geteuid() };
    if let Ok(metadata) = std::fs::symlink_metadata(&lock_path) {
        if !metadata.file_type().is_file() || metadata.uid() != euid || metadata.mode() & 0o077 != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "清理锁文件不是当前用户的私有普通文件",
            ));
        }
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(lock_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != euid || metadata.mode() & 0o077 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "清理锁文件属主或权限不安全",
        ));
    }

    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
            return Ok(file);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock || std::time::Instant::now() >= deadline {
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn unix_runtime_dir() -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    let euid = unsafe { geteuid() };
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            let path = std::path::PathBuf::from(dir);
            let metadata = std::fs::symlink_metadata(&path)
                .ok()
                .map(|m| (m.file_type().is_dir(), m.uid(), m.mode() & 0o777));
            if runtime_dir_is_private(metadata, euid) {
                return Ok(path);
            }
        }
    }

    let path = std::env::temp_dir().join(format!("dd-run-{euid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    if !runtime_dir_is_private(
        Some((
            metadata.file_type().is_dir(),
            metadata.uid(),
            metadata.mode() & 0o777,
        )),
        euid,
    ) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "运行时目录不是当前用户的 0700 目录",
        ));
    }
    Ok(path)
}

#[cfg(unix)]
fn bind_and_keep_alive(path: &std::path::Path) -> Result<(), UnixAcquireError> {
    let listener = std::os::unix::net::UnixListener::bind(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            UnixAcquireError::AlreadyRunning
        } else {
            UnixAcquireError::Io(e)
        }
    })?;
    listener
        .set_nonblocking(true)
        .map_err(UnixAcquireError::Io)?;
    std::thread::Builder::new()
        .name("dd-run-single-instance".into())
        .spawn(move || loop {
            match listener.accept() {
                Ok((stream, _)) => drop(stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => return,
            }
        })
        .map_err(|e| UnixAcquireError::Io(e))?;
    Ok(())
}

#[cfg(unix)]
fn rename_socket_noreplace(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let old = CString::new(old.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let new = CString::new(new.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        fn renameat2(oldfd: i32, old: *const i8, new: *const i8, flags: u32) -> i32;
    }
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn renamex_np(old: *const i8, new: *const i8, flags: u32) -> i32;
    }

    #[cfg(target_os = "linux")]
    let result = unsafe {
        renameat2(
            -100,
            old.as_ptr(),
            new.as_ptr(),
            1, // RENAME_NOREPLACE
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe {
        renamex_np(old.as_ptr(), new.as_ptr(), 4) // RENAME_EXCL
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (old, new);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "当前 Unix 平台不支持不可覆盖重命名",
        ));
    }
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn path_file_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

#[cfg(unix)]
fn socket_candidate_metadata(path: &std::path::Path) -> Option<(bool, u32, u64, u64)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.file_type().is_socket(), m.uid(), m.dev(), m.ino()))
}

#[cfg(unix)]
fn quarantine_socket_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!(
            ".ds-{:x}-{:08x}-{id}",
            std::process::id(),
            nanos as u32
        ))
}

#[cfg(unix)]
fn restore_quarantined_socket(
    quarantine: &std::path::Path,
    path: &std::path::Path,
) -> Result<(), UnixAcquireError> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            rename_socket_noreplace(quarantine, path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    UnixAcquireError::UnsafePath(format!(
                        "清理期间原路径被占用，隔离文件保留在 {}",
                        quarantine.display()
                    ))
                } else {
                    UnixAcquireError::Io(e)
                }
            })
        }
        Ok(_) => Err(UnixAcquireError::UnsafePath(format!(
            "清理期间原路径被占用，隔离文件保留在 {}",
            quarantine.display()
        ))),
        Err(e) => Err(UnixAcquireError::Io(e)),
    }
}

#[cfg(unix)]
fn reclaim_stale_socket(path: &std::path::Path) -> Result<(), UnixAcquireError> {
    use std::os::unix::net::UnixStream;

    let Some(original_identity) = path_file_identity(path) else {
        return Err(UnixAcquireError::UnsafePath(
            "隔离前无法读取原路径 inode".to_string(),
        ));
    };
    let mut quarantine = None;
    for _ in 0..16 {
        let candidate = quarantine_socket_path(path);
        match rename_socket_noreplace(path, &candidate) {
            Ok(()) => {
                quarantine = Some(candidate);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return bind_and_keep_alive(path),
            Err(e) => return Err(UnixAcquireError::Io(e)),
        }
    }
    let quarantine = quarantine.ok_or_else(|| {
        UnixAcquireError::UnsafePath("无法分配不可覆盖的 socket 隔离路径".to_string())
    })?;
    if !same_file_identity(Some(original_identity), path_file_identity(&quarantine)) {
        restore_quarantined_socket(&quarantine, path)?;
        return Err(UnixAcquireError::UnsafePath(
            "隔离前后 dev+inode 不一致".to_string(),
        ));
    }

    let euid = unsafe { geteuid() };
    let connect = UnixStream::connect(&quarantine)
        .map(|_| ())
        .map_err(|e| e.kind());
    let metadata = socket_candidate_metadata(&quarantine);
    let decision = classify_socket_candidate(
        metadata.map(|(is_socket, uid, _, _)| (is_socket, uid)),
        connect,
        euid,
    );
    match decision {
        SocketCleanup::Active | SocketCleanup::Foreign => {
            restore_quarantined_socket(&quarantine, path)?;
            Err(UnixAcquireError::AlreadyRunning)
        }
        SocketCleanup::Stale => {
            let identity = path_file_identity(&quarantine);
            let connect = UnixStream::connect(&quarantine)
                .map(|_| ())
                .map_err(|e| e.kind());
            let metadata = socket_candidate_metadata(&quarantine);
            let rechecked = classify_socket_candidate(
                metadata.map(|(is_socket, uid, _, _)| (is_socket, uid)),
                connect,
                euid,
            );
            if !same_file_identity(Some(original_identity), identity)
                || rechecked != SocketCleanup::Stale
            {
                restore_quarantined_socket(&quarantine, path)?;
                return Err(UnixAcquireError::UnsafePath(
                    "删除前 inode 或存活状态复查失败".to_string(),
                ));
            }
            match std::fs::remove_file(&quarantine) {
                Ok(()) => bind_and_keep_alive(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => bind_and_keep_alive(path),
                Err(e) => Err(UnixAcquireError::Io(e)),
            }
        }
        _ => {
            restore_quarantined_socket(&quarantine, path)?;
            Err(UnixAcquireError::UnsafePath(
                "隔离后复查确认不是可清理的当前用户死 socket".to_string(),
            ))
        }
    }
}

#[cfg(unix)]
fn reclaim_and_bind(path: &std::path::Path) -> Result<(), UnixAcquireError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::os::unix::net::UnixStream;

    let _cleanup_lock = lock_socket_cleanup(path).map_err(|e| {
        UnixAcquireError::UnsafePath(format!("清理锁不可用，拒绝降级为多实例：{e}"))
    })?;
    let connect = UnixStream::connect(path).map(|_| ()).map_err(|e| e.kind());
    let metadata = std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.file_type().is_socket(), m.uid()));
    let euid = unsafe { geteuid() };
    match classify_socket_candidate(metadata, connect, euid) {
        SocketCleanup::Active | SocketCleanup::Foreign => Err(UnixAcquireError::AlreadyRunning),
        SocketCleanup::Stale => reclaim_stale_socket(path),
        SocketCleanup::Missing => bind_and_keep_alive(path),
        SocketCleanup::NotSocket => Err(UnixAcquireError::UnsafePath(
            "单实例路径已存在但不是 socket，拒绝删除".to_string(),
        )),
        SocketCleanup::Inconclusive => Err(UnixAcquireError::UnsafePath(
            "无法确认单实例路径所有权与存活状态，拒绝删除".to_string(),
        )),
    }
}

#[cfg(unix)]
fn acquire_unix_socket(path: &std::path::Path) -> Result<(), UnixAcquireError> {
    match bind_and_keep_alive(path) {
        Ok(()) => Ok(()),
        Err(UnixAcquireError::AlreadyRunning) => reclaim_and_bind(path),
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn enforce_single_instance() {
    const ALREADY_RUNNING: &str = "dd-run 已在运行（请用托盘图标或热键唤起面板），无需重复启动。\n\
         dd-run is already running (use the tray icon or the hotkey) — no need to start it again.";

    let path = match unix_runtime_dir() {
        Ok(dir) => dir.join("dd-run.sock"),
        Err(e) => {
            log::warn!("[dd-gui] 单实例运行时目录不安全（{e}），停止启动");
            eprintln!("dd-run 单实例运行时目录不安全：{e}；已停止启动。");
            std::process::exit(1);
        }
    };

    match acquire_unix_socket(&path) {
        Ok(()) => {}
        Err(UnixAcquireError::AlreadyRunning) => {
            log::warn!("[dd-gui] 已有 dd-run 实例在运行，提示后退出");
            eprintln!("{ALREADY_RUNNING}");
            std::process::exit(0);
        }
        Err(UnixAcquireError::UnsafePath(reason)) => {
            log::warn!("[dd-gui] 单实例路径不安全：{reason}，停止启动");
            eprintln!("dd-run 单实例路径不安全：{reason}；已停止启动。");
            std::process::exit(1);
        }
        Err(UnixAcquireError::Io(e)) => {
            log::warn!("[dd-gui] 单实例 socket 失败（{e}），降级为不互斥启动（照常运行）");
        }
    }
}

/// 非 Windows 且非 Unix 的其余目标占位（与 hotkey/tray 的跨平台占位同口径）。
#[cfg(not(any(windows, unix)))]
fn enforce_single_instance() {}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{
        acquire_unix_socket, quarantine_socket_path, rename_socket_noreplace, UnixAcquireError,
    };
    use super::{
        classify_socket_candidate, runtime_dir_is_private, same_file_identity, SocketCleanup,
    };
    #[cfg(unix)]
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::sync::atomic::{AtomicU32, Ordering};
    #[cfg(unix)]
    use std::sync::{mpsc, Arc, Barrier};

    #[test]
    fn p1_15_file_identity_requires_same_device_and_inode() {
        assert!(same_file_identity(Some((1, 2)), Some((1, 2))));
        assert!(!same_file_identity(Some((1, 2)), Some((1, 3))));
        assert!(!same_file_identity(Some((1, 2)), Some((2, 2))));
        assert!(!same_file_identity(None, Some((1, 2))));
    }

    #[test]
    fn p1_15_runtime_directory_must_be_private_and_owned() {
        assert!(runtime_dir_is_private(Some((true, 7, 0o700)), 7));
        assert!(!runtime_dir_is_private(Some((true, 7, 0o755)), 7));
        assert!(!runtime_dir_is_private(Some((true, 8, 0o700)), 7));
        assert!(!runtime_dir_is_private(Some((false, 7, 0o700)), 7));
        assert!(!runtime_dir_is_private(None, 7));
    }

    #[test]
    fn p1_15_socket_cleanup_classifier_never_reclaims_unsafe_paths() {
        use std::io::ErrorKind;

        assert_eq!(
            classify_socket_candidate(Some((true, 7)), Ok(()), 7),
            SocketCleanup::Active
        );
        assert_eq!(
            classify_socket_candidate(Some((true, 7)), Err(ErrorKind::ConnectionRefused), 7),
            SocketCleanup::Stale
        );
        assert_eq!(
            classify_socket_candidate(Some((true, 8)), Err(ErrorKind::ConnectionRefused), 7),
            SocketCleanup::Foreign
        );
        assert_eq!(
            classify_socket_candidate(Some((false, 7)), Err(ErrorKind::ConnectionRefused), 7),
            SocketCleanup::NotSocket
        );
        assert_eq!(
            classify_socket_candidate(None, Err(ErrorKind::PermissionDenied), 7),
            SocketCleanup::Inconclusive
        );
        assert_eq!(
            classify_socket_candidate(None, Err(ErrorKind::NotFound), 7),
            SocketCleanup::Missing
        );
    }

    #[cfg(all(test, unix))]
    fn temp_socket_path(tag: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("dd-run-single-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建 socket 测试目录");
        dir.join("dd-run.sock")
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_live_socket_is_not_removed() {
        use std::os::unix::net::{UnixListener, UnixStream};

        let path = temp_socket_path("live");
        let listener = UnixListener::bind(&path).expect("绑定活 socket");
        assert!(matches!(
            acquire_unix_socket(&path),
            Err(UnixAcquireError::AlreadyRunning)
        ));
        assert!(UnixStream::connect(&path).is_ok(), "活 socket 必须保留");
        drop(listener);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_stale_socket_is_reclaimed() {
        use std::os::unix::net::{UnixListener, UnixStream};

        let path = temp_socket_path("stale");
        let listener = UnixListener::bind(&path).expect("绑定 socket");
        drop(listener);
        assert!(path.exists(), "listener drop 后路径应残留");
        acquire_unix_socket(&path).expect("崩溃残留应自愈");
        assert!(UnixStream::connect(&path).is_ok(), "新 listener 应可连接");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_quarantine_name_stays_short() {
        let path = temp_socket_path("short-quarantine");
        let quarantine = quarantine_socket_path(&path);
        assert_eq!(quarantine.parent(), path.parent());
        assert!(quarantine.file_name().unwrap().len() <= 32);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_socket_path_symlink_is_not_followed_or_removed() {
        use std::os::unix::fs::symlink;

        let path = temp_socket_path("socket-symlink");
        let target = path.parent().unwrap().join("keep.txt");
        std::fs::write(&target, b"keep me").expect("写普通文件");
        symlink(&target, &path).expect("建 socket 路径 symlink");
        assert!(matches!(
            acquire_unix_socket(&path),
            Err(UnixAcquireError::UnsafePath(_))
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
        assert!(std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_quarantine_rename_never_overwrites_existing_path() {
        let socket_path = temp_socket_path("rename-noreplace");
        let dir = socket_path.parent().unwrap();
        let source = dir.join("source");
        let target = dir.join("target");
        std::fs::write(&source, b"source").expect("写源文件");
        std::fs::write(&target, b"target").expect("写目标文件");
        assert!(matches!(
            rename_socket_noreplace(&source, &target),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists
        ));
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
        assert_eq!(std::fs::read(&target).unwrap(), b"target");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_symlink_lock_file_is_rejected() {
        use std::os::unix::fs::symlink;

        let path = temp_socket_path("lock-symlink");
        let target = path.parent().unwrap().join("keep.txt");
        std::fs::write(&target, b"keep me").expect("写锁目标");
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        symlink(&target, std::path::PathBuf::from(lock_name)).expect("建锁 symlink");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("绑定残留 socket");
        drop(listener);
        assert!(matches!(
            acquire_unix_socket(&path),
            Err(UnixAcquireError::UnsafePath(_))
        ));
        assert_eq!(std::fs::read(&target).expect("锁目标仍在"), b"keep me");
        assert!(path.exists(), "socket 不得被不安全的锁路径清理");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_regular_file_is_never_deleted() {
        let path = temp_socket_path("regular");
        std::fs::write(&path, b"keep me").expect("写普通文件");
        assert!(matches!(
            acquire_unix_socket(&path),
            Err(UnixAcquireError::UnsafePath(_))
        ));
        assert_eq!(std::fs::read(&path).expect("文件仍在"), b"keep me");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_cross_process_acquirers_allow_exactly_one() {
        const N: usize = 4;
        let path = temp_socket_path("cross-process");
        let mut children = Vec::new();
        for _ in 0..N {
            children.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "tests::p1_15_child_acquire", "--ignored"])
                    .env("DDRUN_TEST_SOCKET", &path)
                    .spawn()
                    .expect("启动子进程"),
            );
        }
        let mut acquired = 0;
        let mut already_running = 0;
        for child in children {
            let status = child.wait_with_output().expect("等待子进程");
            match status.status.code() {
                Some(0) => acquired += 1,
                Some(2) => already_running += 1,
                code => panic!("子进程异常退出：{code:?}"),
            }
        }
        assert_eq!(acquired, 1);
        assert_eq!(already_running, N - 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(all(test, unix))]
    #[ignore]
    #[test]
    fn p1_15_child_acquire() {
        let path = std::env::var_os("DDRUN_TEST_SOCKET").expect("测试 socket 路径");
        match acquire_unix_socket(std::path::Path::new(&path)) {
            Ok(()) => {
                std::thread::sleep(std::time::Duration::from_secs(10));
                std::process::exit(0);
            }
            Err(UnixAcquireError::AlreadyRunning) => std::process::exit(2),
            Err(_) => std::process::exit(3),
        }
    }

    #[cfg(all(test, unix))]
    #[test]
    fn p1_15_concurrent_acquirers_allow_exactly_one() {
        const N: usize = 8;
        let path = Arc::new(temp_socket_path("concurrent"));
        let barrier = Arc::new(Barrier::new(N));
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            for _ in 0..N {
                let tx = tx.clone();
                let path = Arc::clone(&path);
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    tx.send(acquire_unix_socket(&path)).expect("发送结果");
                });
            }
        });
        drop(tx);

        let mut acquired = 0;
        let mut already_running = 0;
        for _ in 0..N {
            match rx.recv().expect("收到并发结果") {
                Ok(()) => acquired += 1,
                Err(UnixAcquireError::AlreadyRunning) => already_running += 1,
                other => panic!("不应出现不安全路径或 IO 降级：{other:?}"),
            }
        }
        assert_eq!(acquired, 1, "并发只放行一个实例");
        assert_eq!(already_running, N - 1);
    }
}
