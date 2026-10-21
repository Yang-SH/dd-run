//! 字体加载与内存工作集：CJK / 符号 / 图标字体栈装配。

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
    // 链）——字体后台热替换（~2.5s）完成前，首帧若引用未知族 egui 无法回退。
    // 2026-09-13 真机反馈「不用加粗字体」撤销加粗：该族自此恒指 regular 字形
    // （seguisb/msyhbd 停载，见 load_cjk_font_definitions 末尾注册）。
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
    // R-05：线程创建失败降级为默认字体（中文可能显示为方块），不 panic。
    if let Err(e) = std::thread::Builder::new()
        .name("cjk-fonts".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            match load_cjk_font_definitions() {
                Some(fonts) => {
                    ctx.set_fonts(fonts);
                    ctx.request_repaint();
                    log::debug!(
                        "[dd-gui] CJK 字体后台加载完成（{} ms），已热替换；首帧为默认字体",
                        started.elapsed().as_millis()
                    );
                }
                None => log::debug!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块"),
            }
        })
    {
        log::error!(
            "[dd-gui] CJK 字体加载线程创建失败：{e} —— 以默认字体继续（中文可能显示为方块）"
        );
    }
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
                log::debug!("[dd-gui] 字体 mmap 化：{path}（{len} B，文件页，不计私有提交）");
                Some(egui::FontData::from_static(leaked))
            }
            Err(e) => {
                let bytes = std::fs::read(path).ok()?;
                log::debug!(
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
///
/// **F3 拉丁主字纠偏（2026-09-13）**：最终 Proportional 族序由下方**显式重排**
/// 为 `[segoe, NotoEmoji-Regular, emoji-icon-font, cjk, sym, icons]`——拉丁/
/// 数字主字 = Segoe UI（Windows 原生排印；参考 DeskBox = WinUI 3 系统管线的
/// 默认观感），并消除「semibold 拉丁 = Segoe UI Semibold / regular 拉丁 =
/// Ubuntu-Light」的同屏异字体混排。重排原则**仅拉丁一处变化**：CJK/emoji/
/// Geometric Shapes/PUA 的回退路由与重排前逐一相同；缺文件的成员被剔除
/// （降级安全，如无 segoe → 回到旧观感）。Ubuntu-Light 移出 Proportional
/// （Hack 仍留在 Monospace 族——该族全仓无调用者，零行为变化）。
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
            log::warn!("[dd-gui] 读 CJK 字体 {path} 失败");
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
        log::debug!(
            "[dd-gui] 未找到 {sym_candidate}（符号字体）；M3 桩态 ◌ 等符号可能仍显示为方块"
        );
    }
    // Segoe UI 拉丁/符号扩展后援（v4.12 真机修复）：U+A78B 等 msyh/seguisym
    // 缺失的拉丁修饰符码位落到这里（见函数 doc 注释的取证记录）。零 PUA
    // 码位，插在图标字体之前无抢字形风险；缺文件（< Vista）仅记日志。
    let latin_candidate = r"C:\Windows\Fonts\segoeui.ttf";
    if let Some(data) = load_font_file(latin_candidate) {
        fonts
            .font_data
            .insert("segoe".to_owned(), std::sync::Arc::new(data));
        // 载入顺序仅决定 font_data 注册；最终族序由下方 F3 重排统一确定
        // （segoe 置顶 = 拉丁主字，图标字体仍居末位不抢 PUA 码位）。
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
        log::info!("[dd-gui] 已加载拉丁后援字体：{latin_candidate}");
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
            log::info!("[dd-gui] 已加载图标字体：{path}");
        } else {
            log::warn!("[dd-gui] 读图标字体 {path} 失败");
        }
    } else {
        log::debug!("[dd-gui] 未找到图标字体（SegoeIcons/segmdl2）；glyph 图标将显示为方块");
    }

    // F3 拉丁主字纠偏（2026-09-13）：显式重排 Proportional 族序——只做两个
    // 动作：`segoe` 提到最前（拉丁/数字主字 = Segoe UI，见函数 doc）、
    // `Ubuntu-Light` 移出，其余成员相对次序一字不动 ⇒ 除拉丁外零路由变化：
    //   旧：[Ubuntu-Light, NotoEmoji, emoji-icon, cjk, sym, segoe, icons]
    //   新：[segoe,       NotoEmoji, emoji-icon, cjk, sym,        icons]
    // CJK 汉字/全角标点 → msyh（segoe/NotoEmoji/emoji-icon 均无 CJK 不截胡）；
    // emoji → NotoEmoji（相对 sym 次序不变）；◌ → seguisym；PUA → 图标字体。
    // U+A78B（AMD 显示名）由族首 segoe 直接命中（v4.12 修复语义不变）。
    // 缺文件成员剔除：retain 按 font_data 实际注册过滤，降级安全。
    // semibold 族（见下方注册）同样指向本链——2026-09-13 撤销加粗后恒为
    // regular 字形，regular/semibold 名义族共用一套字形，无混排问题。
    let mut prop_order: Vec<String> = [
        "segoe",
        "NotoEmoji-Regular",
        "emoji-icon-font",
        "cjk",
        "sym",
        "icons",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    prop_order.retain(|k| fonts.font_data.contains_key(k.as_str()));
    fonts
        .families
        .insert(egui::FontFamily::Proportional, prop_order);

    // B1 语义字重（2026-09-13 真机反馈「不用加粗字体」撤销）：seguisb.ttf /
    // msyhbd.ttc **不再加载**——theme::semibold 已回落 regular 字形，加粗面
    // 零引用（少两份 mmap 读盘 + 解析，热替换更快）。族名保留注册并指向
    // regular 链：热替换会整体重建 FontDefinitions，不注册则该族在替换后
    // 消失；显式兜底，防任何历史 FontId 引用未知族。
    fonts.families.insert(
        egui::FontFamily::Name(crate::theme::SEMIBOLD_FAMILY.into()),
        fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );

    if !any_loaded {
        log::debug!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块");
        return None;
    }
    Some(fonts)
}
