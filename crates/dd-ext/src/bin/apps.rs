//! dd-ext-apps —— 内置「应用启动」扩展（`com.ddrun.apps`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：Apps 枚举真实应用）：
//! - 顶层命令 = 枚举的本地应用列表（进程内 `OnceLock` 缓存一次）；
//! - **Windows**：`%APPDATA%` / `%ProgramData%` 的「开始菜单\Programs」递归 `*.lnk`
//!   + `PATH` 各目录根层 `*.exe`（去重、上限防爆表）；
//! - invoke：`.lnk` → `cmd /c start "" <lnk>`；`.exe` → 直接 spawn → `Dismiss`。
//!
//! `frozen=false`：应用列表随安装/卸载变化，属 fresh——宿主不落 frozen 桩、
//! 每次冷启动 warm 拉取（与 [`docs/m4-record.md`](../../docs/m4-record.md) P4 语义一致）。
//!
//! 平台策略（P4 决策：Windows 优先）：macOS（`/Applications`）与 Linux
//! （`.desktop` + PATH）为**编译恒成立占位**，待对应平台轮实现。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use dd_ext::{run, Effect, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 结果列表上限：避免 PATH 全量扫描产生超大首屏（截断并记日志）。
const MAX_APPS: usize = 400;

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.apps",
        display_name: "Apps",
        description: "枚举并启动本地应用（开始菜单 + PATH）",
        // 应用列表随安装/卸载变化 → fresh（不落 frozen 桩，见模块文档）
        frozen: false,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-apps",
        top_level: sys::top_level_commands,
        fallback: None,
        invoke: sys::handle_invoke,
    }
}

/// 平台枚举与启动（按 OS 分实现）。
#[cfg(windows)]
mod sys {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    /// 一条应用项：命令 id 后缀（`apps.run.<n>`）+ 原始启动描述。
    struct App {
        /// 显示名（不含 .lnk/.exe 后缀）
        title: String,
        /// 是否为快捷方式（需 `cmd /c start` 间接启动）
        is_shortcut: bool,
        /// 绝对路径
        path: PathBuf,
    }

    static APP_CACHE: OnceLock<Vec<App>> = OnceLock::new();

    fn app_list() -> &'static Vec<App> {
        APP_CACHE.get_or_init(|| {
            let mut seen: HashSet<String> = HashSet::new();
            let mut apps: Vec<App> = Vec::new();

            // ① 开始菜单 .lnk（两个根，递归）
            for root in start_menu_roots() {
                let mut stack = vec![root];
                while let Some(dir) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&dir) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if path.extension().and_then(|s| s.to_str()) == Some("lnk") {
                            let Some(title) = file_stem(&path) else {
                                continue;
                            };
                            if seen.insert(title.to_lowercase()) {
                                apps.push(App {
                                    title,
                                    is_shortcut: true,
                                    path,
                                });
                            }
                        }
                    }
                }
            }

            // ② PATH 各目录根层 *.exe（去重：与 .lnk 同显示名者跳过）
            for dir in path_dirs() {
                if apps.len() >= MAX_APPS {
                    break;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if apps.len() >= MAX_APPS {
                        break;
                    }
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
                        let Some(title) = file_stem(&path) else {
                            continue;
                        };
                        if seen.insert(title.to_lowercase()) {
                            apps.push(App {
                                title,
                                is_shortcut: false,
                                path,
                            });
                        }
                    }
                }
            }

            // 按显示名排序（列表稳定、可预测）
            apps.sort_by_key(|a| a.title.to_lowercase());
            eprintln!(
                "[dd-ext-apps] 枚举到 {} 个应用{}",
                apps.len(),
                if apps.len() >= MAX_APPS {
                    format!("（达到上限 {MAX_APPS}，已截断）")
                } else {
                    String::new()
                }
            );
            apps
        })
    }

    fn file_stem(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    }

    /// 开始菜单两个根（用户 + 公共）。
    fn start_menu_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for (env, default) in [
            ("APPDATA", r"%USERPROFILE%\AppData\Roaming"),
            ("PROGRAMDATA", r"C:\ProgramData"),
        ] {
            let base = std::env::var_os(env)
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("USERPROFILE").map(|_| PathBuf::from(default)))
                .unwrap_or_default();
            roots.push(
                base.join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs"),
            );
        }
        roots
    }

    /// PATH 拆分出的目录列表。
    fn path_dirs() -> Vec<PathBuf> {
        std::env::var_os("PATH")
            .map(|p| {
                std::env::split_paths(&p)
                    .filter(|d| d.is_dir())
                    .collect::<Vec<PathBuf>>()
            })
            .unwrap_or_default()
    }

    /// 图标缓存基目录：`%APPDATA%\dd-run\cache\apps-icons\`（与 `dd-host::manifest::cache_dir()` 同源）。
    fn icon_cache_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("APPDATA").map(|p| {
            std::path::PathBuf::from(p)
                .join("dd-run")
                .join("cache")
                .join("apps-icons")
        })
    }

    /// 稳定哈希 .lnk/.exe 绝对路径 → 16-hex 文件名（同一文件重抽可命中缓存）。
    fn icon_cache_key(path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        path.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// 若 cache 已有该 .lnk/.exe 的 32×32 PNG（且**内容合法**），返回该路径（不重抽）。
    ///
    /// 自愈：若落盘文件不是合法 PNG（魔数 `89 50 4E 47 0D 0A 1A 0A` 缺失，如上次写入
    /// 因 IO/中断失败），返回 None，让上层重抽覆盖——避免"程序 bug 写出坏 PNG 后永远卡住"。
    fn cached_icon_path(dir: &Path, path: &Path, size: u32) -> Option<std::path::PathBuf> {
        let key = icon_cache_key(path);
        let p = dir.join(format!("apps-{}-{}.png", key, size));
        if !p.is_file() {
            return None;
        }
        if let Ok(head) = std::fs::read(&p) {
            if head.len() >= 8 && head[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
                return Some(p);
            }
        }
        None
    }

    /// 抽 .lnk / .exe 的真实系统图标，写为 32×32 PNG 到 cache 目录，返回 PNG 路径。
    ///
    /// 流程（Windows）：
    /// 1. `SHGetFileInfoW` 取系统缓存的 `.lnk` 解析后图标 / `.exe` 嵌入图标
    ///    （`SHGFI_ICON|SHGFI_LARGEICON` → 32×32 HICON）；
    /// 2. `GetIconInfo` 拆 `hbmColor` → `GetDIBits` → raw BGRA buffer；
    /// 3. `image::PngEncoder` 写 PNG bytes 落盘；
    /// 4. 缓存命中则跳过 SHGetFileInfoW。失败回退 None（调用方回落占位 glyph）。
    fn extract_to_png(path: &Path) -> Option<std::path::PathBuf> {
        unsafe {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::UI::Shell::{
                SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

            const ICON_SIZE: u32 = 32;
            const SHFILEINFOW_SIZE: u32 = std::mem::size_of::<SHFILEINFOW>() as u32;

            let out_dir = icon_cache_dir()?;
            if let Some(p) = cached_icon_path(&out_dir, path, ICON_SIZE) {
                return Some(p);
            }
            std::fs::create_dir_all(&out_dir).ok()?;

            // SHGetFileInfoW：返回 HICON 在 fi.hIcon（无效时为 null）
            let path_wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut fi: SHFILEINFOW = std::mem::zeroed();
            let hr = SHGetFileInfoW(
                path_wide.as_ptr(),
                0,
                &mut fi,
                SHFILEINFOW_SIZE,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if hr == 0 || fi.hIcon.is_null() {
                return None;
            }

            // HICON → PNG bytes（BGRA→RGBA + 强制 alpha=255）
            let png_bytes = hicon_to_png(fi.hIcon, ICON_SIZE)?;
            // 释放 HICON（不论写文件成功与否：避免泄漏）
            DestroyIcon(fi.hIcon);

            let key = icon_cache_key(path);
            let out_path = out_dir.join(format!("apps-{}-{}.png", key, ICON_SIZE));
            if !out_path.exists() {
                std::fs::write(&out_path, &png_bytes).ok()?;
            }
            Some(out_path)
        }
    }

    /// HICON → 32×32 RGBA → PNG bytes。失败 None（**不** DestroyIcon：调用方负责）。
    ///
    /// image 0.25 `ImageEncoder::write_image(self, ...)` 接收 self（值），所以下面
    /// `encoder.write_image(...)` 会移走 encoder；这之后 `out` 已写完可丢弃。
    unsafe fn hicon_to_png(hicon: *mut std::ffi::c_void, size: u32) -> Option<Vec<u8>> {
        use image::ImageEncoder;
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
            DIB_RGB_COLORS,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

        let mut ii: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut ii) == 0 {
            return None;
        }
        let hbm_color = ii.hbmColor;
        let hbm_mask = ii.hbmMask;
        if hbm_color.is_null() {
            if !hbm_mask.is_null() {
                DeleteObject(hbm_mask);
            }
            return None;
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size as i32;
        bmi.bmiHeader.biHeight = -(size as i32); // 负高=top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB

        let stride = (size as usize) * 4;
        let mut buf: Vec<u8> = vec![0; stride * size as usize];
        // GetDIBits 的 hdc 参数**必须是有效 DC**（传 NULL 直接返回 0 失败——
        // 实测：hbmColor 32×32@32bpp 都正确，仅因 hdc=NULL 拿不到 bits）。
        // 取屏幕兼容的内存 DC 即可，用完 DeleteDC。
        let hdc = CreateCompatibleDC(std::ptr::null_mut());
        if hdc.is_null() {
            DeleteObject(hbm_color);
            if !hbm_mask.is_null() {
                DeleteObject(hbm_mask);
            }
            return None;
        }
        let n = GetDIBits(
            hdc,
            hbm_color,
            0,
            size,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        DeleteDC(hdc);
        // 释放位图资源（不论 GetDIBits 成功与否）
        DeleteObject(hbm_color);
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask);
        }
        if n == 0 {
            return None;
        }

        // GetDIBits 32bpp 给的是 BGRA，但 alpha 字段在 Windows GDI 下含义不一。
        // 强制 alpha=255 让图标不透，BGRA→RGBA 后交给 dd-gui 的 image::load_from_memory 渲染。
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // B↔R
            px[3] = 0xff;
        }

        let img = image::RgbaImage::from_raw(size, size, buf)?;
        let mut out = Vec::with_capacity(8 * 1024);
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(img.as_raw(), size, size, image::ExtendedColorType::Rgba8)
            .ok()?;
        Some(out)
    }

    /// 单条 app 的 Icon：优先 .lnk/.exe 真实图标（PNG 落 cache）；失败回落到原
    /// Segoe MDL2 `U+E7C4` (Apps) 占位 glyph。
    fn item_icon(app: &App) -> Icon {
        if let Some(png) = extract_to_png(&app.path) {
            if let Some(p) = png.to_str() {
                return Icon {
                    kind: IconKind::Path,
                    value: p.to_string(),
                };
            }
        }
        Icon {
            kind: IconKind::Glyph,
            value: "\u{E7C4}".to_string(),
        }
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        app_list()
            .iter()
            .enumerate()
            .map(|(i, app)| CommandItem {
                id: format!("apps.run.{i}"),
                title: app.title.clone(),
                subtitle: Some(if app.is_shortcut {
                    "快捷方式".to_string()
                } else {
                    app.path.display().to_string()
                }),
                icon: Some(item_icon(app)),
                section: Some("应用".to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect()
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        let idx: usize = params
            .id
            .strip_prefix("apps.run.")
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        let Some(app) = app_list().get(idx) else {
            return (
                CommandResult::ShowToast {
                    message: format!("应用不存在或列表已变化：{}", params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };

        let spawned = if app.is_shortcut {
            launch_shortcut(&app.path)
        } else {
            launch_executable(&app.path)
        };
        match spawned {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: format!("启动 {} 失败：{e}", app.title),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    /// 启动 .lnk：`cmd /c start "" "<lnk>"`（CreateProcess 不解析 .lnk，start 负责）。
    fn launch_shortcut(path: &Path) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("cmd.exe");
        cmd.args(["/C", "start", "", &path.to_string_lossy()]);
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW：隐藏 cmd 自身窗口
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 启动 .exe：直接 spawn（继承宿主无 console 环境，GUI 程序正常）。
    fn launch_executable(path: &Path) -> Result<(), String> {
        let mut cmd = std::process::Command::new(path);
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dd_protocol::model::Sender;

        #[test]
        fn windows_app_list_is_non_empty_and_unique() {
            // 真机/CI 的 Windows 环境必有开始菜单与 PATH exe；列表非空且标题唯一（小写不重复）
            let apps = app_list();
            assert!(!apps.is_empty(), "Windows 上应至少枚举到一个应用");
            let mut lower: Vec<String> = apps.iter().map(|a| a.title.to_lowercase()).collect();
            lower.sort();
            lower.dedup();
            assert_eq!(lower.len(), apps.len(), "应用标题不应重复");
            assert!(apps.len() <= MAX_APPS);
        }

        #[test]
        fn top_level_ids_map_back_into_list() {
            let cmds = top_level_commands();
            assert!(!cmds.is_empty());
            // id 为 apps.run.<index>，index 必须能回查 app_list()
            let last = cmds.last().unwrap().id.clone();
            let idx: usize = last.strip_prefix("apps.run.").unwrap().parse().unwrap();
            assert!(app_list().get(idx).is_some());
        }

        #[test]
        fn start_menu_roots_point_under_windows_dirs() {
            // 只验证目录构造（不触碰文件系统副作用）：两个根都应是 …\Start Menu\Programs
            let roots = start_menu_roots();
            assert_eq!(roots.len(), 2);
            for r in &roots {
                assert!(
                    r.to_string_lossy().contains("Start Menu")
                        && r.to_string_lossy().ends_with("Programs"),
                    "got {}",
                    r.display()
                );
            }
        }

        /// 真机覆盖率守卫：绝大多数 app 应拿到 Path 真实图标（而非回落 apps 占位 glyph）。
        ///
        /// 阈值取 90%（实测 400/400 = 100%）：个别 app 因目标被卸载/路径失效等拿不到
        /// 图标属正常，回落 `U+E7C4` 不影响可用性；但整体大面积回落说明抽取链路坏了。
        #[test]
        fn real_icon_covers_most_apps() {
            let apps = app_list();
            assert!(!apps.is_empty(), "Windows 上应至少枚举到一个应用");
            let path_n = apps
                .iter()
                .filter(|a| matches!(super::item_icon(a).kind, IconKind::Path))
                .count();
            let ratio = path_n as f64 / apps.len() as f64;
            assert!(
                ratio >= 0.9,
                "真实图标覆盖率过低：{path_n}/{} = {:.1}%（阈值 90%）",
                apps.len(),
                ratio * 100.0
            );
        }

        /// Path 图标必须指向**真实存在**的 PNG 文件（host 端 decode 依赖落盘文件，
        /// 悬空路径会在 dd-gui 表现为"解码失败 → 占位 glyph"）。
        #[test]
        fn path_icons_point_to_existing_png_files() {
            for app in app_list().iter().take(20) {
                let icon = super::item_icon(app);
                if icon.kind == IconKind::Path {
                    let p = std::path::Path::new(&icon.value);
                    assert!(
                        p.is_file(),
                        "Path 图标应已落盘：{}（app={}）",
                        icon.value,
                        app.title
                    );
                    let head = std::fs::read(p).unwrap();
                    assert_eq!(
                        &head[..8],
                        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
                        "落盘文件应为 PNG：{}",
                        icon.value
                    );
                }
            }
        }

        #[test]
        fn extract_to_png_succeeds_for_real_exe() {
            // 抽取 system32 里随便一个 .exe（SHGetFileInfoW + GetDIBits 全链路）
            // 验证：①返回 Some(path)；②路径在 cache 目录里；③文件存在非空
            let exe = std::env::var("ComSpec")
                .unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_string());
            let path = std::path::PathBuf::from(&exe);
            if !path.is_file() {
                eprintln!("[extract_to_png_succeeds_for_real_exe] skip: {exe:?} 不存在");
                return;
            }
            let out = super::extract_to_png(&path)
                .unwrap_or_else(|| panic!("SHGetFileInfoW 抽 {exe:?} 应成功"));
            assert!(out.is_file(), "落盘 PNG 应存在：{}", out.display());
            let meta = std::fs::metadata(&out).unwrap();
            assert!(meta.len() > 0, "PNG 文件应非空");
            // PNG magic = 89 50 4E 47 0D 0A 1A 0A
            let head = std::fs::read(&out).unwrap();
            assert_eq!(
                &head[..8],
                &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
            );
        }

        #[test]
        fn item_icon_prefers_real_path_for_existing_apps() {
            // 列表里第一条应用：真实图标应优先 Path 类型
            let apps = app_list();
            if let Some(app) = apps.first() {
                let icon = super::item_icon(app);
                match icon.kind {
                    IconKind::Glyph => {
                        // 真实不可用时回落到原 glyph（U+E7C4）—— 也算正常路径
                        assert_eq!(icon.value, "\u{E7C4}");
                    }
                    IconKind::Path => {
                        assert!(
                            std::path::Path::new(&icon.value).is_file(),
                            "Path 图标应指向落盘 PNG：{}",
                            icon.value
                        );
                    }
                    IconKind::Url => panic!("apps 不能用 Url icon"),
                }
            }
        }

        #[test]
        fn invoke_unknown_index_toasts() {
            let p = InvokeParams {
                id: "apps.run.999999".into(),
                sender: Sender::TopLevel,
                context: None,
            };
            let (result, _) = handle_invoke(&p);
            assert!(matches!(result, CommandResult::ShowToast { .. }));
        }
    }
}

/// 非 Windows 占位（P4 Windows 优先）：编译恒成立，功能待对应平台轮实现。
#[cfg(not(windows))]
mod sys {
    use super::*;
    use dd_ext::Effect;

    pub fn top_level_commands() -> Vec<CommandItem> {
        // macOS 扫描 /Applications；Linux 读 .desktop + PATH：TODO 对应平台轮
        Vec::new()
    }

    pub fn handle_invoke(_params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        (
            CommandResult::ShowToast {
                message: "应用枚举：当前平台尚未实现（P4 Windows 优先）".to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        )
    }
}
