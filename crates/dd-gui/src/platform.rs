//! 系统副作用：字体加载、窗口居中定位、UAC 提权、资源管理器定位。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1），方法体逐字未改。

use crate::app::PaletteApp;
use eframe::egui;

/// 内存优化 M1（docs/memory-optimization-plan.md §3.M1）：隐藏后修剪进程
/// 工作集——提示 OS 把当前物理页移出工作集（`(-1,-1)` = EmptyWorkingSet 语义）。
/// 页按需软故障回（µs 级），唤起首帧无感；**私有提交不变**，收益 = 后台常驻
/// 物理内存与 Task Manager「内存」数字的真实下降（启动器隐藏期是常驻态）。
/// 调用点：`ui()` 隐藏帧绘制收尾（`paint_hide_frame` 消费后）+ 隐藏期
/// `warm_idle_reclaim` 驱逐之后（`health.rs`）。
#[cfg(windows)]
pub fn trim_working_set() {
    use windows_sys::Win32::System::Memory::SetProcessWorkingSetSizeEx;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        // 伪句柄（GetCurrentProcess），min/max = (SIZE_T)-1 + Flags=0 =
        // EmptyWorkingSet 语义（等价旧 API SetProcessWorkingSetSize(-1,-1)）
        SetProcessWorkingSetSizeEx(GetCurrentProcess(), usize::MAX, usize::MAX, 0);
    }
}

/// 非 Windows 平台无对应语义，空实现（调用点恒安全）。
#[cfg(not(windows))]
pub fn trim_working_set() {}

/// 加载本地字体栈：CJK 主字（msyh / SimHei / Deng）、Segoe UI Symbol 符号后援、
/// Segoe UI 拉丁/符号扩展后援、Segoe Fluent/MDL2 图标字体（§8.6 glyph 图标，M5 UI 批次 2）。
///
/// msyh.ttc 覆盖 CJK 与 ✓/✗（Dingbats 区），但**缺** Geometric Shapes 的 ◌ (U+25CC)
/// ——M3 桩态页脚会渲染成方框。seguisym.ttf（Win 7+ 必装）补 Geometric Shapes /
/// Misc Symbols，把 ◌/○/· 等符号路由到它去渲染。
///
/// **segoeui.ttf（v4.12 真机修复 2026-09-06）**：AMD Software 等应用的
/// AppsFolder 显示名含 U+A78B（MODIFIER LETTER COLON「꞉」，AMD 用它规避
/// Windows 文件名非法字符 `:`）——msyh/seguisym 的 cmap 均无此码位（已用
/// fonttools 核验），egui 渲染为缺字方块「AMD Software□ Adrenalin」。
/// Segoe UI 覆盖该码位（开始菜单即用 Segoe UI 渲染正常），补为拉丁后援；
/// 其 cmap **零 PUA 码位**（已核验），插在图标字体之前不会抢 §8.6 glyph 字形。
///
/// 图标字体按两代兼容顺序加载：Win11 的 `SegoeIcons.ttf`（Segoe Fluent Icons）优先，
/// Win10 无此文件时回落 `segmdl2.ttf`（Segoe MDL2 Assets）——码位（U+E700–U+E8FF
/// 一带）两代基本兼容；两个都不存在时 glyph 图标显示为方块（记录日志，不致命）。
/// 追加在字体族**末尾**：egui 字形回退按族内顺序查，CJK/符号字体缺的 PUA 码位
/// 自然落到图标字体（PUA 区 U+E000+ 两字体均无覆盖，无抢字形问题）。
///
/// **M6 批次 6.2（L10）：后台线程加载**——22MB 级字体（msyh.ttc ~19.7MB +
/// seguisym 2.5MB）的读盘 + 解析是冷启动 GUI 初始化的最大瓶颈（A2 实测
/// total ~2.8s，其中数据就绪仅 ~2ms）。本函数**立即返回**：首帧用 egui 默认
/// 字体渲染，字体在后台线程就绪后 `ctx.set_fonts` 原子热替换并请求重绘。
/// 已知取舍（记档）：若用户在字体就绪前（约 2.5s 内）唤起面板，CJK 文本
/// 短暂显示方块后自动恢复（字体热替换为原子操作，无半新半旧帧）。
pub fn setup_cjk_fonts(ctx: &egui::Context) {
    // B1 语义字重：先**同步**注册 semibold 族（映射到 egui 默认 Proportional
    // 链）——字体后台热替换（~2.5s）完成前，首帧若引用未知族 egui 无法回退；
    // 注册后前期以默认字体渲染，热替换后自动获得真实 semibold 字形。
    let mut fonts = egui::FontDefinitions::default();
    let prop = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts.families.insert(
        egui::FontFamily::Name(crate::theme::SEMIBOLD_FAMILY.into()),
        prop,
    );
    ctx.set_fonts(fonts);

    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("cjk-fonts".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            match load_cjk_font_definitions() {
                Some(fonts) => {
                    ctx.set_fonts(fonts);
                    ctx.request_repaint();
                    eprintln!(
                        "[dd-gui] CJK 字体后台加载完成（{} ms），已热替换；首帧为默认字体",
                        started.elapsed().as_millis()
                    );
                }
                None => eprintln!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块"),
            }
        })
        .expect("spawn cjk-fonts thread");
}

/// 读单个字体文件为 egui 字体数据（内存优化 M2，docs/memory-optimization-plan.md
/// §3.M2）：**只读内存映射**接入——文件页不计私有提交、可被系统随时回收重读
/// （字体「大而偶用」的理想形态）。上游事实（epaint 0.36.1 已核）：
/// `FontData::from_static(&'static [u8])` + skrifa `FontRef` 纯借用解析 ⇒ mmap
/// 切片贯穿 FontData → skrifa → set_fonts 热替换重解析全链路，零私有拷贝。
/// mmap 常驻进程全程（`Box::leak` 有意为之——字体生命周期即进程生命周期）；
/// 映射失败回落整读 `from_owned`（行为与 M6 批次 6.2 完全一致）。
fn load_font_file(path: &str) -> Option<egui::FontData> {
    match std::fs::File::open(path) {
        Ok(file) => match unsafe { memmap2::Mmap::map(&file) } {
            Ok(mmap) => {
                let len = mmap.len();
                let leaked: &'static [u8] = &*Box::leak(Box::new(mmap));
                eprintln!("[dd-gui] 字体 mmap 化：{path}（{len} B，文件页，不计私有提交）");
                Some(egui::FontData::from_static(leaked))
            }
            Err(e) => {
                let bytes = std::fs::read(path).ok()?;
                eprintln!(
                    "[dd-gui] 字体 mmap 失败（{e}），回落整读：{path}（{} B）",
                    bytes.len()
                );
                Some(egui::FontData::from_owned(bytes))
            }
        },
        Err(_) => None,
    }
}

/// 读盘并构建字体定义（纯函数，供 [`setup_cjk_fonts`] 的后台线程调用）；
/// `None` = 无任何 CJK 字体可用（维持 egui 默认字体）。
fn load_cjk_font_definitions() -> Option<egui::FontDefinitions> {
    let cjk_candidates = [
        // 优先 msyh.ttc（YaHei，Win7+ 必装且完整含 U+2713 ✓ 与 CJK）
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
    ];
    let sym_candidate = r"C:\Windows\Fonts\seguisym.ttf";
    // M5 批次 2：glyph 图标字体（按 Win11→Win10 顺序尝试，两代码位兼容）
    let icon_candidates = [
        r"C:\Windows\Fonts\SegoeIcons.ttf", // Win11（Segoe Fluent Icons）
        r"C:\Windows\Fonts\segmdl2.ttf",    // Win10 回退（Segoe MDL2 Assets）
    ];

    let mut fonts = egui::FontDefinitions::default();
    let mut any_loaded = false;

    if let Some(path) = cjk_candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
    {
        if let Some(data) = load_font_file(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), std::sync::Arc::new(data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".to_owned());
            any_loaded = true;
        } else {
            eprintln!("[dd-gui] 读 CJK 字体 {path} 失败");
        }
    }
    if let Some(data) = load_font_file(sym_candidate) {
        // 符号后援：append 在 cjk 之后，egui 字形回退按字体族顺序查找，
        // cjk 缺的 Geometric Shapes/Misc Symbols 落到 seguisym。
        fonts
            .font_data
            .insert("sym".to_owned(), std::sync::Arc::new(data));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("sym".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("sym".to_owned());
        any_loaded = true;
    } else {
        eprintln!("[dd-gui] 未找到 {sym_candidate}（符号字体）；M3 桩态 ◌ 等符号可能仍显示为方块");
    }
    // Segoe UI 拉丁/符号扩展后援（v4.12 真机修复）：U+A78B 等 msyh/seguisym
    // 缺失的拉丁修饰符码位落到这里（见函数 doc 注释的取证记录）。零 PUA
    // 码位，插在图标字体之前无抢字形风险；缺文件（< Vista）仅记日志。
    let latin_candidate = r"C:\Windows\Fonts\segoeui.ttf";
    if let Some(data) = load_font_file(latin_candidate) {
        fonts
            .font_data
            .insert("segoe".to_owned(), std::sync::Arc::new(data));
        // 插在 sym 之后、icons 之前：普通拉丁/符号优先用 Segoe UI，
        // PUA 图标码位继续落到后面的图标字体。
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("segoe".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("segoe".to_owned());
        eprintln!("[dd-gui] 已加载拉丁后援字体：{latin_candidate}");
    }
    if let Some(path) = icon_candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
    {
        if let Some(data) = load_font_file(path) {
            fonts
                .font_data
                .insert("icons".to_owned(), std::sync::Arc::new(data));
            // 追加在族末（cjk/sym 之后）：PUA 码位（§8.6 glyph 值）落到图标字体。
            // 加入 Proportional + Monospace 两个族（列表副标题/键位提示同源显示）。
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("icons".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("icons".to_owned());
            any_loaded = true;
            eprintln!("[dd-gui] 已加载图标字体：{path}");
        } else {
            eprintln!("[dd-gui] 读图标字体 {path} 失败");
        }
    } else {
        eprintln!("[dd-gui] 未找到图标字体（SegoeIcons/segmdl2）；glyph 图标将显示为方块");
    }

    // B1 语义字重：semibold 族主字（拉丁 = Segoe UI Semibold，Win7+ 必装；
    // CJK = 微软雅黑 Bold msyhbd.ttc）。任一缺失仅记日志、跳过——族内缺主字
    // 时该脚本回落 regular 观感，不影响启动。mmap 载入策略与 msyh.ttc 相同
    // （msyhbd.ttc ~19MB，文件页常驻不计私有提交）。
    let semibold_candidates = [
        (r"C:\Windows\Fonts\seguisb.ttf", "seguisb"),
        (r"C:\Windows\Fonts\msyhbd.ttc", "cjkbd"),
    ];
    let mut semibold_chain: Vec<String> = Vec::new();
    for (path, key) in semibold_candidates {
        if let Some(data) = load_font_file(path) {
            fonts
                .font_data
                .insert(key.to_owned(), std::sync::Arc::new(data));
            semibold_chain.push(key.to_owned());
        } else {
            eprintln!("[dd-gui] 未找到 semibold 字体 {path}（对应脚本以 regular 字重渲染）");
        }
    }
    // 后援链与 Proportional 一致（◌ U+25CC 等缺字形继续落 sym/segoe；
    // Latin/CJK 已由 seguisb/cjkbd 覆盖，排在后面的 regular 主字不会截胡）。
    if let Some(prop) = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
    {
        semibold_chain.extend(prop);
    }
    fonts.families.insert(
        egui::FontFamily::Name(crate::theme::SEMIBOLD_FAMILY.into()),
        semibold_chain,
    );

    if !any_loaded {
        eprintln!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块");
        return None;
    }
    Some(fonts)
}

impl PaletteApp {
    /// 取"光标所在显示器工作区"的**逻辑尺寸**（物理像素 ÷ `pixels_per_point`），
    /// 供 v4.12 D37 唤起尺寸自适应（`root_panel_size`/`settings_panel_size`）clamp
    /// 使用；返回 `None` = 取不到（指针/显示器信息失败），调用方按无 clamp 路径回落。
    ///
    /// 与 [`center_on_cursor`] 拆分的原因：原 `send_center_on_cursor` 在发
    /// `InnerSize` **之前**读取 `inner_rect` 作为居中窗口尺寸——但该尺寸是上一次
    /// 可见态的遗留值（例如设置页 650×640），而本次 `show()` 实际要显示的是
    /// 复位后的根页 650×440。用陈旧尺寸居中 + 实际小尺寸渲染 → 面板相对屏心
    /// 偏移 `(大-小)/2`，表现为"切换设置项后关闭重开面板位置变化"。拆分后
    /// 先取工作区算目标尺寸，再用目标尺寸精确居中，消除该跳动。
    #[cfg(windows)]
    pub(crate) fn cursor_work_area(&self, ctx: &egui::Context) -> Option<(f32, f32)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } == 0 {
            eprintln!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return None;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            eprintln!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return None;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            eprintln!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
            return None;
        }
        let work = info.rcWork; // 工作区（物理像素，不含任务栏）
        let ppp = ctx.pixels_per_point().max(0.5);
        Some((
            (work.right - work.left) as f32 / ppp,
            (work.bottom - work.top) as f32 / ppp,
        ))
    }

    /// 把 `target`（逻辑点，即本次 `show()` 将要显示的窗口尺寸）居中到
    /// 光标所在显示器工作区，发送 `OuterPosition`。
    ///
    /// 坐标换算：显示器（`rcWork`）与 winit 窗口位置都是**物理像素**，而
    /// egui `OuterPosition` 期望**逻辑点**（egui-winit 内部按窗口 scale factor
    /// 再换算回物理）。这里用 `ctx.pixels_per_point()` 作换算率（即当前窗口缩放）。
    /// 多 DPI 混合屏上目标屏缩放与窗口当前缩放不同时会有几像素偏差——验收标准
    /// （grills A1："每次唤起居中、无位置跳动"）不要求像素级精确，单屏/同 DPI
    /// 场景完全居中。
    ///
    /// 任一步失败直接返回（指针/显示器信息取不到）：窗口仍正常显示在上一次
    /// 位置，仅不居中——不让定位失败阻断唤起。
    #[cfg(windows)]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, target: (f32, f32)) {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } == 0 {
            eprintln!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            eprintln!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            eprintln!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
            return;
        }
        let work = info.rcWork;
        let ppp = ctx.pixels_per_point().max(0.5);
        // 用**目标尺寸**而非 `inner_rect` 的遗留尺寸（见 `cursor_work_area` 注释）。
        let win_w = target.0 * ppp;
        let win_h = target.1 * ppp;
        let cx = work.left as f32 + ((work.right - work.left) as f32 - win_w) * 0.5;
        let cy = work.top as f32 + ((work.bottom - work.top) as f32 - win_h) * 0.5;
        eprintln!(
            "[dd-gui] 唤起居中：光标屏工作区=({},{} {}x{}) 目标={}x{} → ({}, {})",
            work.left,
            work.top,
            work.right - work.left,
            work.bottom - work.top,
            target.0,
            target.1,
            cx,
            cy
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            cx / ppp,
            cy / ppp,
        )));
    }

    /// 非 Windows 占位：取不到工作区，返回 `None`（唤起尺寸不做 clamp）。
    #[cfg(not(windows))]
    pub(crate) fn cursor_work_area(&self, _ctx: &egui::Context) -> Option<(f32, f32)> {
        None
    }

    /// 非 Windows 占位：退化为 egui 自带"按窗口当前所在屏居中"（dd-run 当前
    /// Windows 宿主不走此分支；多屏语义在此平台未定义）。
    #[cfg(not(windows))]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, _target: (f32, f32)) {
        if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
            ctx.send_viewport_cmd(cmd);
        }
    }
}

/// 以管理员身份运行（UAC 提权）：`ShellExecuteW` verb=runas（10B.2）。
/// 用户在 UAC 弹窗取消时返回 Err（返回值 ≤ 32）。
#[cfg(windows)]
pub(crate) fn run_as_admin(path: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let verb = wide("runas");
    let file = wide(path);
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if h as isize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW = {}", h as isize))
    }
}

#[cfg(not(windows))]
pub(crate) fn run_as_admin(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持提权运行".to_string())
}

/// 拆分 `file://` URL 为 `(authority, path)`。
///
/// - `file:///<path>` → `(None, path)`（空 authority，本机路径）
/// - `file://<host>/<path>` → `(Some(host), path)`（UNC 网络共享，v3.3 P2 §9.6 支持）
/// - 非 `file://` 协议、或 `file://<host>` 后无路径 → None
///
/// 注：host 不做 percent-decode（`file://` 的 authority 极少出现非 ASCII/IPv6
/// 字面量，Everything 索引结果亦不会产生），已作为已知边界记档。
fn split_file_url(url: &str) -> Option<(Option<&str>, &str)> {
    let rest = url.strip_prefix("file://")?;
    match rest.strip_prefix('/') {
        Some(p) => Some((None, p)),
        None => {
            let slash = rest.find('/')?;
            Some((Some(&rest[..slash]), &rest[slash + 1..]))
        }
    }
}

/// `(authority, path)` → Windows 路径：`/` → `\`；有 host 时拼 UNC `\\host\share\…`。
fn to_windows_path(host: Option<&str>, path: &str) -> String {
    let p = path.replace('/', "\\");
    match host {
        Some(h) => format!("\\\\{h}\\{p}"),
        None => p,
    }
}

/// 去掉 `?query` / `#fragment`（Windows 文件名不允许 `?`，`#` 则合法，故仅作
/// 次要候选——见 `file_url_candidates` 的排序）。
fn strip_query_fragment(s: &str) -> &str {
    match s.find(['?', '#']) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// 最小 `%XX` → byte 解码（**按字节处理**，避免把 UTF-8 多字节序列拆成多个
/// Latin-1 codepoint）。非法/不完整转义按字面保留。
fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_byte(bytes[i + 1]), hex_byte(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// `file://` URL → **按可能性排序**的本地路径候选（纯函数，无 IO）。
///
/// 排序（v3.3 P2 修复 §9.6 缺陷 1/2：`%` `#` `?` 未处理）：
/// 1. 原样保留（`%XX` 按字面）——覆盖「文件名真的含 `%`」，如 `report%20final.txt`；
/// 2. 去 `?query`/`#fragment`——覆盖 URL 语义分量（`?` 在 Windows 文件名非法）；
/// 3. 上述两者的 percent-decode 版——覆盖标准编码 URL，如 `%20` → 空格。
///
/// 伴随宿主的存在性优选（`resolve_file_url_to_path`）后，两种来源互不干扰：
/// 字面路径存在即命中自身，不存在才落到解码解释。
pub(crate) fn file_url_candidates(url: &str) -> Vec<String> {
    let (host, path) = match split_file_url(url) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut variants: Vec<&str> = vec![path];
    let trimmed = strip_query_fragment(path);
    if trimmed != path {
        variants.push(trimmed);
    }

    let mut out: Vec<String> = Vec::with_capacity(4);
    for v in &variants {
        push_unique(&mut out, to_windows_path(host, v));
    }
    for v in &variants {
        push_unique(&mut out, to_windows_path(host, &to_decoded_str(v)));
    }
    out
}

fn to_decoded_str(s: &str) -> String {
    String::from_utf8_lossy(&percent_decode(s.as_bytes())).into_owned()
}

fn push_unique(out: &mut Vec<String>, s: String) {
    if !out.contains(&s) {
        out.push(s);
    }
}

/// `file://` URL → 最可能存在 Windows 路径（v3.3 P2：`file_url_candidates` +
/// 存在性优选）。
///
/// IO 策略：**UNC 路径不做 `exists()`**（离线 SMB 共享会让 `Path::exists()`
/// 阻塞到网络超时），直接返回由 `ShellExecuteW` 处理；本地路径按候选顺序取
/// 第一个存在者；全部不存在时回退首选（原样保留版），便于上层报错。
pub(crate) fn resolve_file_url_to_path(url: &str) -> Option<String> {
    let mut cands = file_url_candidates(url);
    if let Some(hit) = cands
        .iter()
        .position(|c| !is_unc_path(c) && std::path::Path::new(c).exists())
    {
        return Some(cands.swap_remove(hit));
    }
    cands.into_iter().next()
}

fn is_unc_path(p: &str) -> bool {
    p.starts_with("\\\\")
}

fn hex_byte(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 在资源管理器中定位文件（`explorer /select,<path>`，10B.2）。
///
/// 若目标路径不存在，fallback 到其父目录；父目录也不存在则返回错误，避免
/// Windows 在 `/select` 失效时随机打开「文档」等默认位置。
#[cfg(windows)]
pub(crate) fn reveal_in_folder(path: &str) -> Result<(), String> {
    use std::path::Path;

    let p = Path::new(path);
    let arg = if p.exists() {
        format!("/select,{path}")
    } else if let Some(parent) = p.parent().filter(|d| d.exists()) {
        // 目标已不存在/被卸载：至少打开其原本所在目录（定位到目录本身）。
        format!("/open,{}", parent.display())
    } else {
        return Err(format!("路径不存在：{path}"));
    };

    std::process::Command::new("explorer")
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub(crate) fn reveal_in_folder(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持资源管理器定位".to_string())
}

/// 用系统默认程序打开路径（v3.3 P1.5 修复 host/open_url file:// 派发）。
///
/// 仿 `run_as_admin` 风格：`ShellExecuteW(verb="open", file=path)`。
/// Windows 自动派发语义：目录 → Explorer 窗口；文件 → 关联程序。
/// 返回值 ≤ 32 视为失败（ShellExecuteW 错误约定）。
#[cfg(windows)]
pub(crate) fn open_path(path: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let verb = wide("open");
    let file = wide(path);
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if h as isize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW = {}", h as isize))
    }
}

#[cfg(not(windows))]
pub(crate) fn open_path(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持 ShellExecute open".to_string())
}

/// 窗口系统背景材质（v4.7 D31：Win11 22H2+ `DWMWA_SYSTEMBACKDROP_TYPE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemBackdrop {
    /// 无材质（`DWMSBT_NONE`，不透明面板）。
    None,
    /// 云母（`DWMSBT_MAINWINDOW`）。
    Mica,
    /// 亚克力（`DWMSBT_TRANSIENTWINDOW`）。
    Acrylic,
}

/// 应用系统背景材质到主面板窗口（v4.7 D31，设计稿 8.1「材质效果」行）。
///
/// 返回 `false` = API 不支持或调用失败（Win10 / Win11 22621 以下返回错误码），
/// 调用方必须回退不透明面板——降级不阻断（区别热键 fail-fast）。
#[cfg(windows)]
pub fn apply_system_backdrop(hwnd: isize, backdrop: SystemBackdrop) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
        DWMWA_SYSTEMBACKDROP_TYPE,
    };
    let value: i32 = match backdrop {
        SystemBackdrop::None => DWMSBT_NONE,
        SystemBackdrop::Mica => DWMSBT_MAINWINDOW,
        SystemBackdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
    };
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            (&value) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        eprintln!(
            "[dd-gui] DWMWA_SYSTEMBACKDROP_TYPE 应用失败（hr=0x{:x}）→ 回退不透明面板",
            hr
        );
        return false;
    }
    true
}

#[cfg(not(windows))]
pub fn apply_system_backdrop(_hwnd: isize, _backdrop: SystemBackdrop) -> bool {
    false
}

/// 材质/窗口明暗染色跟随主题（`DWMWA_USE_IMMERSIVE_DARK_MODE` = 20，D31）。
/// best-effort：失败仅记日志（材质染色回退系统默认，不影响功能）。
#[cfg(windows)]
pub fn set_immersive_dark(hwnd: isize, dark: bool) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let value: i32 = dark as i32;
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&value) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        eprintln!(
            "[dd-gui] DWMWA_USE_IMMERSIVE_DARK_MODE 应用失败（hr=0x{:x}）",
            hr
        );
        return false;
    }
    true
}

#[cfg(not(windows))]
pub fn set_immersive_dark(_hwnd: isize, _dark: bool) -> bool {
    false
}

/// 窗口是否 OS 层可见（`IsWindowVisible`）。
///
/// v4.16 真机修复（拖拽后面板空白）判据：应用态 `visible=false` 但 OS 可见
/// ⇒ `Visible(false)` 曾在原生拖拽/缩放模态循环内被 Windows 静默忽略
/// （SC_MOVE 循环内 ShowWindow 无效），应用态与 OS 态脱钩——宿主据此自愈。
#[cfg(windows)]
pub fn is_window_visible(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    unsafe { IsWindowVisible(hwnd as _) != 0 }
}

#[cfg(not(windows))]
pub fn is_window_visible(_hwnd: isize) -> bool {
    true
}

/// UTF-16 NUL 结尾宽字符串（windows-sys 0.61 无 wide_string! 宏，本地辅助）。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 开机自启（M6 批次 6.3）：HKCU `...\CurrentVersion\Run` 写/删 `dd-run` 值。
/// 值 = 带引号的当前 exe 路径（含空格安全）。返回 Err = 注册表操作失败。
#[cfg(windows)]
pub fn set_autostart(enable: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name = wide("dd-run");
    let exe = std::env::current_exe()
        .map_err(|e| format!("current_exe 失败：{e}"))?
        .to_string_lossy()
        .to_string();
    let data: Vec<u16> = wide(&format!("\"{exe}\""));

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let open = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if open != 0 {
            return Err(format!("RegOpenKeyExW = {open}"));
        }
        let result = if enable {
            let hr = RegSetValueExW(
                hkey,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            if hr == 0 {
                Ok(())
            } else {
                Err(format!("RegSetValueExW = {hr}"))
            }
        } else {
            // 删除不存在的值（ERROR_FILE_NOT_FOUND = 2）属幂等成功
            let hr = RegDeleteValueW(hkey, value_name.as_ptr());
            if hr == 0 || hr == 2 {
                Ok(())
            } else {
                Err(format!("RegDeleteValueW = {hr}"))
            }
        };
        RegCloseKey(hkey);
        result
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_enable: bool) -> Result<(), String> {
    Err("仅 Windows 支持开机自启".to_string())
}

/// 系统 UI 语言探测（v4.13 D38）：`GetUserDefaultUILanguage` 的主语言 ID
/// （LANGID 低 10 位）为中文（0x04）→ [`Lang::ZhCn`]，否则 → [`Lang::EnUs`]。
/// 供 `settings.lang == FollowSystem` 时解析生效语言（`PaletteApp::resolve_lang`）。
#[cfg(windows)]
pub fn system_ui_lang() -> dd_gui::settings::Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // SAFETY：无参数、仅读进程默认 UI 语言，线程安全。
    let langid = unsafe { GetUserDefaultUILanguage() };
    if langid & 0x03FF == 0x0004 {
        dd_gui::settings::Lang::ZhCn
    } else {
        dd_gui::settings::Lang::EnUs
    }
}

/// 非 Windows 开发兜底：文案表以中文为主开发语言，跟随系统 → ZhCn。
#[cfg(not(windows))]
pub fn system_ui_lang() -> dd_gui::settings::Lang {
    dd_gui::settings::Lang::ZhCn
}

/// 指针静止多久后重新隐藏光标（亚克力面板：不挡视线）。
///
/// 命令面板是短驻留工具（唤起→输入→回车），阈值取 1.5s：键盘流中鼠标一动
/// 就恢复、静止片刻再隐藏，不会频繁闪烁。
pub const CURSOR_IDLE_HIDE: std::time::Duration = std::time::Duration::from_millis(1500);

/// 面板显示期间的鼠标闲置隐藏守卫（v4.17 亚克力体验优化）。
///
/// **语义（v4.17a 真机修复）**：静止隐藏、移动即恢复——不是"面板期间一直隐藏"。
/// - 面板唤起后鼠标尚未动过 → 隐藏（满足"启动面板默认隐藏鼠标、第一项高亮"）；
/// - 鼠标一动 → **立即恢复** `Default`（用户主动用鼠标时看得见指针）；
/// - 静止超过 [`CURSOR_IDLE_HIDE`] → 重新隐藏（不挡亚克力背景）。
///
/// 设计：
/// - `new()` 仅标记 active（不立即改 egui——chrome_end 每帧尾会按缩放热区
///   重设 `cursor_icon`，直接设一次会被覆盖）。
/// - `apply(ctx)`：**每帧 ui() 早期调用**（chrome_begin 之前），内部采样指针
///   位置判定"本帧是否活动"，再决定光标图标。
///
/// 为什么需要 per-frame apply：
/// - egui 0.36 没有 `set_cursor_visible`，只有 `set_cursor_icon`。
/// - `chrome_end` 每帧尾根据缩放热区重设光标（`ResizeNs` 等），会覆盖本帧
///   设置 → 必须每帧重设。覆盖优先级正确：缩放热区显 Resize（拖拽反馈优先），
///   主体区域才隐藏。
///
/// 安全性：即便守卫意外泄漏，egui 每帧由 chrome_end 兜底，不存在
/// Win32 `ShowCursor` 计数器那种"永久丢失鼠标"的坑。
///
/// 用法：
/// - PaletteApp 持有 `Option<MouseHideScope>`；
///   - `show()` 时 `Some(MouseHideScope::new())`
///   - `ui()` 早期：`if let Some(scope) = self.mouse_hide.as_mut() { scope.apply(&ctx); }`
///   - `hide()` 时 `take()` 并立即 `set_cursor_icon(Default)`。
pub struct MouseHideScope {
    active: bool,
    /// 上次检测到指针活动（移动/滚动/按下）的时刻；`None` = 唤起后尚未动过。
    last_activity: Option<std::time::Instant>,
    /// 上一帧指针位置（`None` = 尚未采样；首帧只采样不判活动，避免"唤起面板"
    /// 这一动作本身被误判成用户移动鼠标）。
    last_pos: Option<egui::Pos2>,
}

impl MouseHideScope {
    /// 进入面板：标记 active，下一帧起每帧 `apply()` 按闲置时长决定光标。
    pub fn new() -> Self {
        Self {
            active: true,
            last_activity: None,
            last_pos: None,
        }
    }

    /// 每帧 ui() 早期调用（chrome_begin 之前）：采样指针活动 → 设置光标图标。
    ///
    /// 非 active 时 no-op（守卫已释放，光标由 `hide()` 立即恢复，不依赖本帧）。
    pub fn apply(&mut self, ctx: &egui::Context) {
        if !self.active {
            return;
        }
        let mut moved = false;
        ctx.input(|i| {
            // `hover_pos` = 指针在窗口内；窗口外（None）时保持上一帧基准，
            // 不误判为移动（指针移出再移回会正常触发）。
            let pos = i.pointer.hover_pos().or_else(|| i.pointer.latest_pos());
            match (self.last_pos, pos) {
                (Some(prev), Some(cur)) => {
                    if (prev - cur).length() > 0.5 {
                        moved = true;
                    }
                }
                (None, Some(_)) => {} // 首帧：只采样，不算活动
                (_, None) => {}
            }
            if let Some(cur) = pos {
                self.last_pos = Some(cur);
            }
            // 滚动与按下也算活动（键盘/滚轮浏览时不隐藏光标）。
            if i.smooth_scroll_delta.length() > 0.0 || i.pointer.any_pressed() {
                moved = true;
            }
        });
        if moved {
            self.last_activity = Some(std::time::Instant::now());
        }
        let hide = cursor_should_hide(self.last_activity, std::time::Instant::now());
        ctx.set_cursor_icon(if hide {
            egui::CursorIcon::None
        } else {
            egui::CursorIcon::Default
        });
    }

    /// 指针自面板唤起后**是否活动过**（移动/滚动/按下）。
    ///
    /// 供列表 hover 抑制使用：鼠标未动过时，指针可能恰好停在结果行上（唤起前
    /// 的残留位置），此时不应显示 hover 高亮、也不应让 hover 行抢走键盘选中
    /// ——否则用户看到的是"鼠标所在行被选中"而非第一项。
    pub fn pointer_engaged(&self) -> bool {
        self.last_activity.is_some()
    }

    /// 主动退出面板：active=false（后续 `apply()` 变 no-op）。
    pub fn release(mut self) {
        self.active = false;
    }
}

/// 纯判定（便于单测）：`None` = 从未活动 → 隐藏；否则静止满 [`CURSOR_IDLE_HIDE`] → 隐藏。
fn cursor_should_hide(last_activity: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    match last_activity {
        None => true,
        Some(t) => now.saturating_duration_since(t) >= CURSOR_IDLE_HIDE,
    }
}

impl Default for MouseHideScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{cursor_should_hide, file_url_candidates, MouseHideScope, CURSOR_IDLE_HIDE};
    use std::time::{Duration, Instant};

    /// 面板唤起后鼠标未动 → 隐藏；刚动过 → 显示（v4.17a 修复的正是这条：
    /// 旧实现无条件隐藏，导致"鼠标动起来反而不见了"）。
    #[test]
    fn cursor_hidden_only_while_pointer_idle() {
        let now = Instant::now();
        assert!(
            cursor_should_hide(None, now),
            "鼠标从未动过（刚唤起面板）→ 隐藏"
        );
        assert!(
            !cursor_should_hide(Some(now), now),
            "本帧刚动过 → 必须恢复可见"
        );
        let just_moved = now.checked_sub(CURSOR_IDLE_HIDE / 2).unwrap_or(now);
        assert!(
            !cursor_should_hide(Some(just_moved), now),
            "静止未超时 → 保持可见"
        );
        let long_idle = now
            .checked_sub(CURSOR_IDLE_HIDE + Duration::from_millis(1))
            .unwrap_or(now);
        assert!(
            cursor_should_hide(Some(long_idle), now),
            "静止超过阈值 → 重新隐藏"
        );
    }

    /// 守卫语义：新建时未 engaged（供列表抑制残留指针位置的 hover）。
    #[test]
    fn new_scope_is_not_engaged() {
        let scope = MouseHideScope::new();
        assert!(
            !scope.pointer_engaged(),
            "刚创建的守卫：指针尚未活动 → hover 应被抑制"
        );
    }

    // ── v3.3 P1.5：file_url_to_path（host/open_url file:// 派发前置） ───

    #[test]
    fn file_url_candidates_basic_windows_path() {
        assert_eq!(
            file_url_candidates("file:///G:/AI/dd-run")[0],
            "G:\\AI\\dd-run",
            "盘符 + 路径：file:///G:/x → G:\\x"
        );
        assert_eq!(
            file_url_candidates("file:///C:/Windows/System32/notepad.exe")[0],
            "C:\\Windows\\System32\\notepad.exe"
        );
    }

    #[test]
    fn file_url_candidates_literal_percent_ranks_before_decoded() {
        // v3.3 P2 §9.6 缺陷 1：文件名真的含 `%` 时必须优先保留字面量
        let cands = file_url_candidates("file:///C:/a/report%20final.txt");
        assert_eq!(
            cands[0], "C:\\a\\report%20final.txt",
            "首选 = 原样（% 作为文件名字符，不可被无条件解码）"
        );
        assert!(
            cands.contains(&"C:\\a\\report final.txt".to_string()),
            "解码版作为兜底候选存在：{cands:?}"
        );
    }

    #[test]
    fn file_url_candidates_offers_query_and_fragment_trimmed_variant() {
        // §9.6 缺陷 2：`?`/`#` 在 URL 语义里是 query/fragment 起点
        let cands = file_url_candidates("file:///C:/a/b.txt?v=1#frag");
        assert_eq!(cands[0], "C:\\a\\b.txt?v=1#frag", "首选仍是完整分量");
        assert!(
            cands.iter().any(|c| c == "C:\\a\\b.txt"),
            "应提供去掉 query/fragment 的候选：{cands:?}"
        );
    }

    #[test]
    fn file_url_candidates_decodes_percent_encoded_chars() {
        // 标准编码 URL：`%20` = 空格、`%E4%BD%A0…` = 你好（UTF-8 三字节）
        assert!(file_url_candidates("file:///C:/Program%20Files/test.exe")
            .contains(&"C:\\Program Files\\test.exe".to_string()));
        assert!(file_url_candidates("file:///D:/%E4%BD%A0%E5%A5%BD.txt")
            .contains(&"D:\\你好.txt".to_string()));
    }

    #[test]
    fn file_url_candidates_preserves_unencoded_cjk() {
        // CJK 字符按字面保留（search.rs 直接拼 file:// + 路径，未做百分号编码）
        assert_eq!(
            file_url_candidates("file:///D:/文档/test.txt")[0],
            "D:\\文档\\test.txt"
        );
    }

    #[test]
    fn file_url_candidates_is_empty_for_non_file_protocol() {
        // websearch 的 http/https 不进 file:// 分支 → 走 webbrowser（行为不变）
        assert!(file_url_candidates("https://example.com/").is_empty());
        assert!(file_url_candidates("http://localhost/x").is_empty());
        assert!(file_url_candidates("about:blank").is_empty());
    }

    #[test]
    fn file_url_candidates_maps_file_host_to_unc() {
        // §9.6 缺陷 3：`file://host/share` → UNC `\\host\share`（此前恒 None，
        // 会误落到 webbrowser 打开而失败）
        assert_eq!(
            file_url_candidates("file://server/share/x.txt")[0],
            "\\\\server\\share\\x.txt"
        );
        assert!(
            file_url_candidates("file://server").is_empty(),
            "无路径部分的 authority-only URL 不构成 UNC 路径"
        );
    }

    #[test]
    fn resolve_file_url_to_path_prefers_existing_literal_file() {
        // 存在性优选：两个同名候选都在时，先看字面 `%` 文件，再看解码后文件
        let dir = std::env::temp_dir().join("dd-run-url-fixture");
        std::fs::create_dir_all(&dir).expect("建夹具目录");
        let literal = dir.join("report%20final.txt");
        let decoded = dir.join("report final.txt");
        for f in [&literal, &decoded] {
            std::fs::write(f, b"x").expect("写夹具文件");
        }

        let url = format!(
            "file:///{}",
            literal.display().to_string().replace('\\', "/")
        );
        assert_eq!(
            super::resolve_file_url_to_path(&url).as_deref(),
            Some(literal.to_string_lossy().as_ref()),
            "字面文件存在 → 命名字面路径，不被解码成「report final.txt」"
        );

        // 删掉字面文件后应回退到解码解释（而非返回不存在的路径）
        std::fs::remove_file(&literal).expect("清理夹具");
        assert_eq!(
            super::resolve_file_url_to_path(&url).as_deref(),
            Some(decoded.to_string_lossy().as_ref()),
            "字面文件不存在 → 回退 decode 候选"
        );

        let _ = std::fs::remove_file(&decoded);
        let _ = std::fs::remove_dir(&dir);
    }
}
