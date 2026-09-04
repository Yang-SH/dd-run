//! dd-ext-apps —— 内置「应用启动」扩展（`com.ddrun.apps`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：Apps 枚举真实应用）：
//! - 顶层命令 = 枚举的本地应用列表（进程内 `OnceLock` 缓存一次）；
//! - **Windows**（对齐 PowerToys CmdPal「All Apps」的行为）：
//!   - 主源：`shell:AppsFolder`（`FOLDERID_AppsFolder`）Shell 枚举——拿到的是
//!     **应用本体**（UWP/打包应用 + 桌面应用），显示名与应用图标均由 Shell 解析，
//!     无 PATH exe 噪音、无「快捷方式」字样；
//!   - 兜底：`%APPDATA%` / `%ProgramData%` 的「开始菜单\Programs」递归 `*.lnk`
//!     （AppsFolder 缺席的桌面快捷方式，按显示名小写去重后补充）；经
//!     `IShellLinkW::GetPath` 解析目标，**仅收录目标为 `.exe` 的项**（过滤
//!     帮助文档/安装器等非应用链接），图标与副标题取目标 exe（无「快捷方式
//!     箭头」覆盖图标）；
//!   - 图标：`IShellItemImageFactory::GetImage(48, SIIGBF_ICONONLY)`（UWP 与
//!     .lnk 目标应用统一走此链路；.lnk 回退 `SHGetFileInfoW` 32px），alpha 按
//!     per-pixel / AND 掩码正确生成（参考 ueli 的高质量图标显示）；
//! - invoke：AppsFolder 项 → `explorer shell:AppsFolder\<parsing>`；`.lnk` →
//!   `cmd /c start "" <lnk>` → `Dismiss`。
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

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.apps",
        display_name: "Apps",
        description: "枚举并启动本地应用（AppsFolder 应用本体 + 开始菜单 .lnk 兜底）",
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
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, HBITMAP};
    use windows_sys::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows_sys::Win32::UI::Shell::{
        BHID_SFObject, FOLDERID_AppsFolder, SHCONTF_FOLDERS, SHCONTF_INCLUDEHIDDEN,
        SHCONTF_NONFOLDERS, SHCreateItemFromIDList, SHCreateItemFromParsingName,
        SHCreateItemWithParent, SHGetKnownFolderIDList, SIGDN_DESKTOPABSOLUTEPARSING,
        SIGDN_NORMALDISPLAY, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    /// 结果列表上限：避免异常环境产生超大首屏（截断并记日志）。
    const MAX_APPS: usize = 400;
    /// 目标图标边长（px）。48px 来自 `SHIL_EXTRALARGE` 档，20px 展示格下采样后
    /// 边缘平滑（ueli 同档位）；GetImage 允许返回更大图（BIGGERSIZEOK）。
    const ICON_SIZE: u32 = 48;
    /// 图标回落占位 glyph（Segoe MDL2 "Apps" U+E7C4）。
    const FALLBACK_GLYPH: &str = "\u{E7C4}";

    // ────────────────────────────────────────────────────────────────
    // 手绘 COM vtable（windows-sys 不生成接口结构与 IID；IID 为 shobjidl_core /
    // thumbcache 中稳定公开值）。COM 对象以 *mut c_void 携带，lpVtbl 位于偏移 0。
    // ────────────────────────────────────────────────────────────────

    // COM vtable 函数指针必须为 `extern "system"` ABI（Rust 默认 ABI 不保证与 C 兼容）。
    type FnQueryInterface =
        unsafe extern "system" fn(*mut core::ffi::c_void, *const GUID, *mut *mut core::ffi::c_void) -> HRESULT;
    type FnAddRef = unsafe extern "system" fn(*mut core::ffi::c_void) -> u32;
    type FnRelease = unsafe extern "system" fn(*mut core::ffi::c_void) -> u32;

    /// 所有 COM 接口共享的前三槽（QI/AddRef/Release）——按偏移 0 直接解释。
    #[repr(C)]
    struct ComUnknownVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
    }

    /// `IShellItem`（shobjidl_core）。
    #[repr(C)]
    struct IShellItemVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        bind_to_handler: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void, // IBindCtx*
            *const GUID,            // bhid
            *const GUID,            // riid
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        get_parent:
            unsafe extern "system" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> HRESULT,
        get_display_name:
            unsafe extern "system" fn(*mut core::ffi::c_void, i32, *mut *mut u16) -> HRESULT,
        get_attributes:
            unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut u32) -> HRESULT,
        compare: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            u32,
            *mut i32,
        ) -> HRESULT,
    }

    /// `IShellItemImageFactory`（thumbcache）。
    #[repr(C)]
    struct IShellItemImageFactoryVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        get_image: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            SIZE,
            i32,
            *mut HBITMAP,
        ) -> HRESULT,
    }

    /// `IShellFolder`（shobjidl_core）。仅使用 EnumObjects（槽 4）；
    /// 未调用的槽按指针占位（布局仅要求槽位大小一致）。
    #[repr(C)]
    struct IShellFolderVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        parse_display_name: *const core::ffi::c_void,
        enum_objects: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            windows_sys::Win32::Foundation::HWND,
            u32, // SHCONTF
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        bind_to_object: *const core::ffi::c_void,
        get_display_name_of: *const core::ffi::c_void,
        get_attributes_of: *const core::ffi::c_void,
        get_ui_object_of: *const core::ffi::c_void,
        create_view_object: *const core::ffi::c_void,
        set_name_of: *const core::ffi::c_void,
    }

    /// `IEnumIDList`（shtypes/shobjidl_core）。
    #[repr(C)]
    struct IEnumIDListVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        next: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            u32,
            *mut *mut ITEMIDLIST,
            *mut u32,
        ) -> HRESULT,
        skip: *const core::ffi::c_void,
        reset: *const core::ffi::c_void,
        clone: *const core::ffi::c_void,
    }

    /// `IShellLinkW`（shobjidl_core）。仅使用 GetPath（槽 3），其余槽不访问。
    #[repr(C)]
    struct IShellLinkWVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        get_path: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut u16,               // pszFile
            i32,                    // cch
            *mut core::ffi::c_void, // WIN32_FIND_DATAW*
            u32,                    // fFlags（SLGP_*）
        ) -> HRESULT,
    }

    /// `IPersistFile`（objidl）。仅使用 Load（槽 5），其余槽不访问。
    #[repr(C)]
    struct IPersistFileVtbl {
        query_interface: FnQueryInterface,
        add_ref: FnAddRef,
        release: FnRelease,
        get_class_id: *const core::ffi::c_void,
        is_dirty: *const core::ffi::c_void,
        load: unsafe extern "system" fn(*mut core::ffi::c_void, *const u16, u32) -> HRESULT,
    }

    /// IID_IShellItem {43826D1E-E718-42EE-BC55-A1E261C37BFE}
    const IID_ISHELL_ITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);
    /// IID_IShellFolder {000214E6-0000-0000-C000-000000000046}
    const IID_ISHELL_FOLDER: GUID = GUID::from_u128(0x000214e6_0000_0000_c000_000000000046);
    /// IID_IShellItemImageFactory {BCC18B79-BA16-442F-80C4-8A59C30C463B}
    const IID_ISHELL_ITEM_IMAGE_FACTORY: GUID =
        GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);
    /// IID_IShellLinkW {000214F9-0000-0000-C000-000000000046}
    const IID_ISHELL_LINKW: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
    /// IID_IPersistFile {0000010B-0000-0000-C000-000000000046}
    const IID_IPERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);
    /// CLSID_ShellLink {00021401-0000-0000-C000-000000000046}
    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);

    /// QI：任意 COM 对象按前三槽调用 QueryInterface。
    unsafe fn com_qi(unk: *mut core::ffi::c_void, iid: &GUID) -> Option<*mut core::ffi::c_void> {
        let vt: *const ComUnknownVtbl = *(unk as *const *const ComUnknownVtbl);
        let mut out: *mut core::ffi::c_void = std::ptr::null_mut();
        if ((*vt).query_interface)(unk, iid, &mut out) == 0 && !out.is_null() {
            Some(out)
        } else {
            None
        }
    }

    /// Release（引用计数减一，返回值无意义）。
    unsafe fn com_release(unk: *mut core::ffi::c_void) {
        let vt: *const ComUnknownVtbl = *(unk as *const *const ComUnknownVtbl);
        ((*vt).release)(unk);
    }

    // ────────────────────────────────────────────────────────────────
    // 应用项模型
    // ────────────────────────────────────────────────────────────────

    /// 启动方式（AppsFolder 项为应用本体；.lnk 为兜底）。
    enum Launch {
        /// `explorer shell:AppsFolder\<parsing>`：UWP/打包应用与 AppsFolder 注册的桌面应用。
        AppsFolder(String),
        /// 经典 .lnk：`cmd /c start`（CreateProcess 不解析 .lnk）。
        Lnk(PathBuf),
    }

    /// 一条应用项（图标在枚举期即解析完成，落盘 PNG 或回落 glyph）。
    struct App {
        /// 显示名（不含 .lnk/.exe 后缀）
        title: String,
        /// 副标题：AppsFolder 应用留空（PowerToys 同款干净列表）；.lnk 显示其路径
        subtitle: Option<String>,
        /// 已解析图标
        icon: Icon,
        launch: Launch,
    }

    static APP_CACHE: OnceLock<Vec<App>> = OnceLock::new();

    fn app_list() -> &'static Vec<App> {
        APP_CACHE.get_or_init(|| {
            // Shell COM（SHCreateItem*/IShellItem）需要 COM 初始化；S_OK/S_FALSE 均可继续。
            unsafe {
                CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            }

            let mut seen: HashSet<String> = HashSet::new();
            let mut apps: Vec<App> = Vec::new();

            // ① shell:AppsFolder：应用本体（UWP + 桌面应用），对齐 PowerToys CmdPal
            match collect_apps_folder(&mut apps, &mut seen) {
                Ok(n) => eprintln!("[dd-ext-apps] AppsFolder 枚举到 {n} 个应用"),
                Err(e) => eprintln!(
                    "[dd-ext-apps] AppsFolder 枚举失败（仅用开始菜单 .lnk 兜底）：{e}"
                ),
            }

            // ② 开始菜单 .lnk 兜底（两个根，递归；按显示名去重，仅补 AppsFolder 缺席项）
            for root in start_menu_roots() {
                let mut stack = vec![root];
                while let Some(dir) = stack.pop() {
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
                        if path.is_dir() {
                            stack.push(path);
                        } else if path.extension().and_then(|s| s.to_str()) == Some("lnk") {
                            let Some(title) = file_stem(&path) else {
                                continue;
                            };
                            // 仅收录目标为 .exe 的快捷方式（应用本体）：过滤 .chm/.inf/
                            // 卸载器/帮助文档等非应用链接（真机反馈：TAP adapter 等混入）。
                            // 图标与副标题取目标 exe——消除「快捷方式箭头」覆盖图标
                            // （真机反馈：7-Zip 仍显示 lnk 箭头图标）。
                            let Some(target) = lnk_target(&path) else {
                                continue;
                            };
                            if !is_exe(&target) {
                                continue;
                            }
                            if seen.insert(title.to_lowercase()) {
                                apps.push(App {
                                    title,
                                    subtitle: Some(target.display().to_string()),
                                    icon: icon_or_glyph(unsafe { file_icon_png(&target) }),
                                    launch: Launch::Lnk(path),
                                });
                            }
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

    /// 枚举 `shell:AppsFolder`（应用本体），逐项产出 App 并按显示名去重。
    ///
    /// 链路（经典 Shell 枚举路径，规避 `BHID_EnumItems` 在部分系统上的
    /// `E_NOINTERFACE` 问题——实测 Win11 上该 handler 不可用）：
    /// `SHGetKnownFolderIDList(FOLDERID_AppsFolder)` → `SHCreateItemFromIDList`
    /// → `BindToHandler(BHID_SFObject, IID_IShellFolder)` → `EnumObjects`
    /// → `IEnumIDList::Next` → `SHCreateItemWithParent` 重建每项的 `IShellItem`。
    /// 每项：`GetDisplayName` 取显示名与 parsing name（= AppUserModelID），
    /// 图标经 `IShellItemImageFactory`（QI 自活动 item）抽 48px。
    fn collect_apps_folder(
        apps: &mut Vec<App>,
        seen: &mut HashSet<String>,
    ) -> Result<usize, String> {
        unsafe {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            if SHGetKnownFolderIDList(&FOLDERID_AppsFolder, 0, std::ptr::null_mut(), &mut pidl)
                != 0
            {
                return Err("SHGetKnownFolderIDList(FOLDERID_AppsFolder) 失败".to_string());
            }
            let mut folder: *mut core::ffi::c_void = std::ptr::null_mut();
            let hr = SHCreateItemFromIDList(pidl, &IID_ISHELL_ITEM, &mut folder);
            if hr != 0 {
                CoTaskMemFree(pidl as *const core::ffi::c_void);
                return Err(format!("SHCreateItemFromIDList: 0x{hr:08X}"));
            }

            let ivt: *const IShellItemVtbl = *(folder as *const *const IShellItemVtbl);
            let mut psf: *mut core::ffi::c_void = std::ptr::null_mut();
            let hr = ((*ivt).bind_to_handler)(
                folder,
                std::ptr::null_mut(),
                &BHID_SFObject,
                &IID_ISHELL_FOLDER,
                &mut psf,
            );
            if hr != 0 {
                com_release(folder);
                CoTaskMemFree(pidl as *const core::ffi::c_void);
                return Err(format!("BindToHandler(BHID_SFObject): 0x{hr:08X}"));
            }

            let fvt: *const IShellFolderVtbl = *(psf as *const *const IShellFolderVtbl);
            let mut enum_list: *mut core::ffi::c_void = std::ptr::null_mut();
            // SHCONTF：应用均为非文件夹项；带上 FOLDERS/INCLUDEHIDDEN 以防遗漏
            let grf = (SHCONTF_FOLDERS | SHCONTF_NONFOLDERS | SHCONTF_INCLUDEHIDDEN) as u32;
            let hr = ((*fvt).enum_objects)(psf, std::ptr::null_mut(), grf, &mut enum_list);
            if hr != 0 {
                com_release(psf);
                com_release(folder);
                CoTaskMemFree(pidl as *const core::ffi::c_void);
                return Err(format!("IShellFolder::EnumObjects: 0x{hr:08X}"));
            }

            let evt: *const IEnumIDListVtbl = *(enum_list as *const *const IEnumIDListVtbl);
            let mut pushed = 0usize;
            let mut batch: [*mut ITEMIDLIST; 32] = [std::ptr::null_mut(); 32];
            loop {
                let mut fetched: u32 = 0;
                // S_OK=0 继续取；S_FALSE=1 本批已取完（fetched 仍有效）；其余失败。
                let hr = ((*evt).next)(enum_list, 32, batch.as_mut_ptr(), &mut fetched);
                if hr != 0 && hr != 1 {
                    break;
                }
                for &child in &batch[..fetched as usize] {
                    if apps.len() < MAX_APPS {
                        let mut item: *mut core::ffi::c_void = std::ptr::null_mut();
                        if SHCreateItemWithParent(
                            pidl,
                            psf,
                            child,
                            &IID_ISHELL_ITEM,
                            &mut item,
                        ) == 0
                            && !item.is_null()
                        {
                            if let (Some(title), Some(parsing)) = (
                                shell_item_display_name(item, SIGDN_NORMALDISPLAY),
                                shell_item_display_name(item, SIGDN_DESKTOPABSOLUTEPARSING),
                            ) {
                                // 过滤非应用项（真机反馈）：Applications 文件夹里混有
                                // ① 以文件路径为 parsing name 的快捷方式/文档（.txt/.url）；
                                // ② `Microsoft.AutoGenerated.*`——Shell 从 .lnk 自动注册的
                                //    伪应用 AUMID（非安装应用，目标多为脚本/文档）。
                                //    正式应用的 parsing name 是 AUMID 或短名（无路径分隔符）
                                if !title.is_empty()
                                    && !parsing.contains('\\')
                                    && !parsing.contains('/')
                                    && !parsing.starts_with("Microsoft.AutoGenerated.")
                                    && seen.insert(title.to_lowercase())
                                {
                                    apps.push(App {
                                        title,
                                        subtitle: None,
                                        icon: icon_or_glyph(shell_item_icon_png(
                                            item,
                                            &format!("appsfolder:{parsing}"),
                                        )),
                                        launch: Launch::AppsFolder(parsing),
                                    });
                                    pushed += 1;
                                }
                            }
                            com_release(item);
                        }
                    }
                    CoTaskMemFree(child as *const core::ffi::c_void);
                }
                if hr == 1 || fetched == 0 {
                    break;
                }
            }
            com_release(enum_list);
            com_release(psf);
            com_release(folder);
            CoTaskMemFree(pidl as *const core::ffi::c_void);
            Ok(pushed)
        }
    }

    /// `IShellItem::GetDisplayName` → UTF-8 字符串（CoTaskMemFree 释放 PWSTR）。
    unsafe fn shell_item_display_name(
        item: *mut core::ffi::c_void,
        sigdn: i32,
    ) -> Option<String> {
        let vt: *const IShellItemVtbl = *(item as *const *const IShellItemVtbl);
        let mut pw: *mut u16 = std::ptr::null_mut();
        if ((*vt).get_display_name)(item, sigdn, &mut pw) != 0 || pw.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *pw.add(len) != 0 {
            len += 1;
        }
        let s = std::ffi::OsString::from_wide(std::slice::from_raw_parts(pw, len))
            .to_string_lossy()
            .into_owned();
        CoTaskMemFree(pw as *const core::ffi::c_void);
        Some(s)
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

    /// 解析 `.lnk` 的目标路径（`CoCreateInstance(CLSID_ShellLink)` +
    /// `IPersistFile::Load` + `IShellLinkW::GetPath(SLGP_RAWPATH)`）。
    /// 目标文件不存在时返回 None。
    fn lnk_target(path: &Path) -> Option<PathBuf> {
        unsafe {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut link: *mut core::ffi::c_void = std::ptr::null_mut();
            if CoCreateInstance(
                &CLSID_SHELL_LINK,
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_ISHELL_LINKW,
                &mut link,
            ) != 0
                || link.is_null()
            {
                return None;
            }

            // IPersistFile::Load 装载 .lnk（STGM_READ = 0）
            let mut hr = match com_qi(link, &IID_IPERSIST_FILE) {
                Some(persist) => {
                    let pvt: *const IPersistFileVtbl =
                        *(persist as *const *const IPersistFileVtbl);
                    let r = ((*pvt).load)(persist, wide.as_ptr(), 0);
                    com_release(persist);
                    r
                }
                None => -1, // 未实现错误码，仅作失败标记
            };
            let mut buf = [0u16; 1024];
            if hr == 0 {
                let lvt: *const IShellLinkWVtbl = *(link as *const *const IShellLinkWVtbl);
                hr = ((*lvt).get_path)(link, buf.as_mut_ptr(), 1024, std::ptr::null_mut(), 4); // SLGP_RAWPATH
            }
            com_release(link);
            if hr != 0 {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
            if len == 0 {
                return None;
            }
            let target = PathBuf::from(std::ffi::OsString::from_wide(&buf[..len]));
            if target.is_file() {
                Some(target)
            } else {
                None
            }
        }
    }

    /// 是否为 `.exe` 目标（大小写不敏感）。
    fn is_exe(p: &Path) -> bool {
        p.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
    }

    // ────────────────────────────────────────────────────────────────
    // 图标抽取（48px，落盘 PNG 缓存；alpha 正确生成）
    // ────────────────────────────────────────────────────────────────

    /// 图标缓存基目录：`%APPDATA%\dd-run\cache\apps-icons\`（与 `dd-host::manifest::cache_dir()` 同源）。
    fn icon_cache_dir() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|p| {
            std::path::PathBuf::from(p)
                .join("dd-run")
                .join("cache")
                .join("apps-icons")
        })
    }

    /// 稳定哈希缓存键（.lnk 绝对路径 / `appsfolder:<parsing>`）→ 16-hex 文件名。
    fn icon_cache_key(key: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    /// 若 cache 已有该 key 的 PNG（且**内容合法**），返回该路径（不重抽）。
    ///
    /// 自愈：若落盘文件不是合法 PNG（魔数缺失，如上次写入因 IO/中断失败），
    /// 返回 None，让上层重抽覆盖。
    fn cached_icon_path(dir: &Path, key: &str, size: u32) -> Option<PathBuf> {
        let p = dir.join(format!("apps-{}-{}.png", icon_cache_key(key), size));
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

    /// 抽 .lnk / .exe 的真实应用图标，写为 PNG 到 cache 目录，返回 PNG 路径。
    ///
    /// 优先 `SHCreateItemFromParsingName` → `IShellItemImageFactory::GetImage`
    /// （.lnk 自动解析到目标应用图标，48px）；失败回退 `SHGetFileInfoW` 32px 老链路。
    unsafe fn file_icon_png(path: &Path) -> Option<PathBuf> {
        let key = path.to_string_lossy();
        let out_dir = icon_cache_dir()?;
        if let Some(p) = cached_icon_path(&out_dir, &key, ICON_SIZE) {
            return Some(p);
        }
        std::fs::create_dir_all(&out_dir).ok()?;

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut factory: *mut core::ffi::c_void = std::ptr::null_mut();
        let hr = SHCreateItemFromParsingName(
            wide.as_ptr(),
            std::ptr::null_mut(),
            &IID_ISHELL_ITEM_IMAGE_FACTORY,
            &mut factory,
        );
        let png = if hr == 0 && !factory.is_null() {
            let p = factory_get_image_png(factory);
            com_release(factory);
            p
        } else {
            None
        };
        // 回退：SHGetFileInfoW 32px（系统缓存图标，老链路保底）
        let png = png.or_else(|| shfileinfo_png(path))?;
        let out_path = out_dir.join(format!("apps-{}-{}.png", icon_cache_key(&key), ICON_SIZE));
        if !out_path.exists() {
            std::fs::write(&out_path, &png).ok()?;
        }
        Some(out_path)
    }

    /// 从活动 IShellItem（QI `IShellItemImageFactory`）抽 48px 图标 PNG。
    unsafe fn shell_item_icon_png(item: *mut core::ffi::c_void, cache_key: &str) -> Option<PathBuf> {
        let out_dir = icon_cache_dir()?;
        if let Some(p) = cached_icon_path(&out_dir, cache_key, ICON_SIZE) {
            return Some(p);
        }
        std::fs::create_dir_all(&out_dir).ok()?;
        let factory = com_qi(item, &IID_ISHELL_ITEM_IMAGE_FACTORY)?;
        let png = factory_get_image_png(factory);
        com_release(factory);
        let png = png?;
        let out_path =
            out_dir.join(format!("apps-{}-{}.png", icon_cache_key(cache_key), ICON_SIZE));
        if !out_path.exists() {
            std::fs::write(&out_path, &png).ok()?;
        }
        Some(out_path)
    }

    /// `IShellItemImageFactory::GetImage(48, ICONONLY|BIGGERSIZEOK)` → PNG bytes。
    ///
    /// 返回句柄类型实测不统一（部分系统/项返回 HICON，部分返回 32bpp HBITMAP），
    /// 按两种解释依次尝试：`GetIconInfo` 成功走 HICON 链路（含 AND 掩码 alpha）；
    /// 否则 `GetDIBits` 直接读位图（走纯 alpha，全零回退不透明）。两种释方式
    /// （DestroyIcon / DeleteObject）对错误句柄类型均无害失败。
    unsafe fn factory_get_image_png(factory: *mut core::ffi::c_void) -> Option<Vec<u8>> {
        let fvt: *const IShellItemImageFactoryVtbl =
            *(factory as *const *const IShellItemImageFactoryVtbl);
        let mut hb: HBITMAP = std::ptr::null_mut();
        let hr = ((*fvt).get_image)(
            factory,
            SIZE {
                cx: ICON_SIZE as i32,
                cy: ICON_SIZE as i32,
            },
            SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            &mut hb,
        );
        if hr != 0 || hb.is_null() {
            return None;
        }
        if let Some(png) = hicon_to_png(hb) {
            DestroyIcon(hb);
            return Some(png);
        }
        if let Some(png) = bitmap_to_png(hb) {
            DeleteObject(hb);
            return Some(png);
        }
        DeleteObject(hb);
        None
    }

    /// 32bpp GDI HBITMAP → PNG bytes（直接 GetDIBits；alpha 保留，全零回退不透明）。
    unsafe fn bitmap_to_png(hbm: HBITMAP) -> Option<Vec<u8>> {
        use image::ImageEncoder;
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleDC, GetDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
        };

        let hdc = CreateCompatibleDC(std::ptr::null_mut());
        if hdc.is_null() {
            return None;
        }
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        let ok = GetDIBits(hdc, hbm, 0, 0, std::ptr::null_mut(), &mut bmi, DIB_RGB_COLORS);
        let w = bmi.bmiHeader.biWidth;
        let h = bmi.bmiHeader.biHeight.unsigned_abs() as i32;
        if ok == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
            DeleteDC(hdc);
            return None;
        }
        let stride = w as usize * 4;
        let mut buf: Vec<u8> = vec![0; stride * h as usize];
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB
        let n = GetDIBits(
            hdc,
            hbm,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        DeleteDC(hdc);
        if n == 0 {
            return None;
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // B↔R
        }
        let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
        if !has_alpha {
            for px in buf.chunks_exact_mut(4) {
                px[3] = 0xff;
            }
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
        let mut out = Vec::with_capacity(16 * 1024);
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(img.as_raw(), w as u32, h as u32, image::ExtendedColorType::Rgba8)
            .ok()?;
        Some(out)
    }

    /// 回退链路：`SHGetFileInfoW` 取 32×32 HICON → PNG bytes（不落盘，调用方负责）。
    unsafe fn shfileinfo_png(path: &Path) -> Option<Vec<u8>> {
        use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};

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
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if hr == 0 || fi.hIcon.is_null() {
            return None;
        }
        let png = hicon_to_png(fi.hIcon);
        DestroyIcon(fi.hIcon);
        png
    }

    /// HICON → PNG bytes。实际尺寸由位图决定（GetImage BIGGERSIZEOK 可能给出
    /// 大于请求的图）。失败 None（**不** DestroyIcon：调用方负责）。
    ///
    /// alpha 生成策略（修复旧实现"强制 alpha=255"导致的黑角/锯齿）：
    /// - 色位图含真实 per-pixel alpha → 原样保留；
    /// - alpha 全 0（掩码型图标）→ 读 `hbmMask` 上半部 AND 掩码生成 alpha
    ///   （掩码位 1 = 透明）；掩码读取失败 → 整图 alpha=255 兜底。
    unsafe fn hicon_to_png(hicon: *mut core::ffi::c_void) -> Option<Vec<u8>> {
        use image::ImageEncoder;
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleDC, GetDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
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

        let hdc = CreateCompatibleDC(std::ptr::null_mut());
        if hdc.is_null() {
            DeleteObject(hbm_color);
            if !hbm_mask.is_null() {
                DeleteObject(hbm_mask);
            }
            return None;
        }

        // ① 查询色位图实际尺寸（lpvBits=NULL 的 GetDIBits 会回填 bmiHeader）
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        let ok = GetDIBits(hdc, hbm_color, 0, 0, std::ptr::null_mut(), &mut bmi, DIB_RGB_COLORS);
        let w = bmi.bmiHeader.biWidth;
        let h = bmi.bmiHeader.biHeight.unsigned_abs() as i32;
        if ok == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
            DeleteObject(hbm_color);
            if !hbm_mask.is_null() {
                DeleteObject(hbm_mask);
            }
            DeleteDC(hdc);
            return None;
        }

        // ② 32bpp top-down 读色位图
        let stride = w as usize * 4;
        let mut buf: Vec<u8> = vec![0; stride * h as usize];
        bmi.bmiHeader.biHeight = -h; // 负高 = top-down
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB
        let n = GetDIBits(
            hdc,
            hbm_color,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // ③ 掩码型图标：读 AND 掩码上半部（1bpp，行 DWORD 对齐；位 1 = 透明）。
        //    必须在 DeleteObject(hbm_mask) 之前读。
        let mask = if !hbm_mask.is_null() {
            read_mask_bits(hdc, hbm_mask, w as usize, h as usize)
        } else {
            None
        };

        DeleteObject(hbm_color);
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask);
        }
        DeleteDC(hdc);
        if n == 0 {
            return None;
        }

        // BGRA → RGBA
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // B↔R
        }
        // alpha：有真实 per-pixel alpha 就保留；全 0 → 掩码生成 / 兜底不透明
        let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
        if !has_alpha {
            match mask {
                Some((mask_bits, mask_stride)) => {
                    for (y, row) in buf.chunks_exact_mut(stride).enumerate() {
                        for (x, px) in row.chunks_exact_mut(4).enumerate() {
                            let byte = mask_bits[y * mask_stride + x / 8];
                            // AND 掩码位 0 = 不透明
                            let opaque = (byte >> (7 - (x % 8))) & 1 == 0;
                            px[3] = if opaque { 0xff } else { 0 };
                        }
                    }
                }
                None => {
                    for px in buf.chunks_exact_mut(4) {
                        px[3] = 0xff;
                    }
                }
            }
        }

        let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
        let mut out = Vec::with_capacity(16 * 1024);
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(
                img.as_raw(),
                w as u32,
                h as u32,
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
        Some(out)
    }

    /// 读 AND 掩码（1bpp）上半部 `h` 行，返回 (bits, stride)。失败 None。
    ///
    /// ICONINFO 的 hbmMask 高度为色位图高度 ×2（上 AND 下 XOR）；图标 alpha 全 0
    /// 时只需 AND 掩码。行按 DWORD 对齐（GetDIBits 规则）。
    unsafe fn read_mask_bits(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        hbm_mask: HBITMAP,
        w: usize,
        h: usize,
    ) -> Option<(Vec<u8>, usize)> {
        use windows_sys::Win32::Graphics::Gdi::{GetDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS};

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        let ok = GetDIBits(hdc, hbm_mask, 0, 0, std::ptr::null_mut(), &mut bmi, DIB_RGB_COLORS);
        let mw = bmi.bmiHeader.biWidth as usize;
        let mh = bmi.bmiHeader.biHeight.unsigned_abs() as usize;
        if ok == 0 || mw < w || mh < h || mw > 512 {
            return None;
        }
        let stride = mw.div_ceil(32) * 4; // 掩码行宽：每 32 像素一 DWORD
        let mut buf: Vec<u8> = vec![0; stride * h]; // 只取上半部（AND 掩码）
        bmi.bmiHeader.biBitCount = 1;
        bmi.bmiHeader.biCompression = 0;
        let n = GetDIBits(
            hdc,
            hbm_mask,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        if n == 0 {
            None
        } else {
            Some((buf, stride))
        }
    }

    /// 图标统一出口：PNG 路径 → `IconKind::Path`；失败 → 占位 glyph `U+E7C4`。
    fn icon_or_glyph(png: Option<PathBuf>) -> Icon {
        match png.and_then(|p| p.to_str().map(|s| s.to_string())) {
            Some(value) => Icon {
                kind: IconKind::Path,
                value,
            },
            None => Icon {
                kind: IconKind::Glyph,
                value: FALLBACK_GLYPH.to_string(),
            },
        }
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        app_list()
            .iter()
            .enumerate()
            .map(|(i, app)| CommandItem {
                id: format!("apps.run.{i}"),
                title: app.title.clone(),
                subtitle: app.subtitle.clone(),
                icon: Some(app.icon.clone()),
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

        let spawned = match &app.launch {
            Launch::AppsFolder(parsing) => {
                // explorer 负责解析 shell:AppsFolder\<AUMID>（UWP 与桌面应用通用，
                // PowerToys CmdPal 同款启动方式）
                std::process::Command::new("explorer.exe")
                    .arg(format!("shell:AppsFolder\\{parsing}"))
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Launch::Lnk(path) => launch_shortcut(path),
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use dd_protocol::model::Sender;

        #[test]
        fn windows_app_list_is_non_empty_and_unique() {
            // 真机/CI 的 Windows 环境必有 AppsFolder 应用与开始菜单；列表非空且标题唯一（小写不重复）
            let apps = app_list();
            assert!(!apps.is_empty(), "Windows 上应至少枚举到一个应用");
            let mut lower: Vec<String> = apps.iter().map(|a| a.title.to_lowercase()).collect();
            lower.sort();
            lower.dedup();
            assert_eq!(lower.len(), apps.len(), "应用标题不应重复");
            assert!(apps.len() <= MAX_APPS);
        }

        #[test]
        fn appsfolder_enumeration_present() {
            // 主源守卫：AppsFolder 枚举链路必须产出应用本体（Launch::AppsFolder）。
            // 全为空说明 SHGetKnownFolderIDList/BindToHandler 链路坏了。
            let apps = app_list();
            assert!(
                apps.iter()
                    .any(|a| matches!(a.launch, Launch::AppsFolder(_))),
                "AppsFolder 应至少枚举到一个应用本体"
            );
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
        /// 阈值取 90%：个别 app 因目标被卸载/路径失效等拿不到图标属正常；但整体
        /// 大面积回落说明抽取链路（GetImage / SHGetFileInfoW / PNG 落盘）坏了。
        #[test]
        fn real_icon_covers_most_apps() {
            let apps = app_list();
            assert!(!apps.is_empty(), "Windows 上应至少枚举到一个应用");
            let path_n = apps
                .iter()
                .filter(|a| matches!(a.icon.kind, IconKind::Path))
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
                if app.icon.kind == IconKind::Path {
                    let p = std::path::Path::new(&app.icon.value);
                    assert!(
                        p.is_file(),
                        "Path 图标应已落盘：{}（app={}）",
                        app.icon.value,
                        app.title
                    );
                    let head = std::fs::read(p).unwrap();
                    assert_eq!(
                        &head[..8],
                        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
                        "落盘文件应为 PNG：{}",
                        app.icon.value
                    );
                }
            }
        }

        #[test]
        fn file_icon_png_succeeds_for_real_exe() {
            // 抽取一个真实 .exe（IShellItemImageFactory 全链路，含回退 SHGetFileInfoW）
            // 验证：①返回 Some(path)；②路径在 cache 目录里；③文件存在且为合法 PNG
            let exe = std::env::var("ComSpec")
                .unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_string());
            let path = std::path::PathBuf::from(&exe);
            if !path.is_file() {
                eprintln!("[file_icon_png_succeeds_for_real_exe] skip: {exe:?} 不存在");
                return;
            }
            let out = unsafe { file_icon_png(&path) }
                .unwrap_or_else(|| panic!("抽 {exe:?} 图标应成功"));
            assert!(out.is_file(), "落盘 PNG 应存在：{}", out.display());
            let head = std::fs::read(&out).unwrap();
            assert!(
                head.len() > 8,
                "PNG 文件应非空：{}",
                out.display()
            );
            assert_eq!(
                &head[..8],
                &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
            );
        }

        /// AppsFolder 应用不应携带「快捷方式」类副标题（应用本体，干净列表）。
        #[test]
        fn appsfolder_apps_have_no_shortcut_subtitle() {
            for app in app_list().iter() {
                if matches!(app.launch, Launch::AppsFolder(_)) {
                    assert!(
                        app.subtitle.is_none(),
                        "AppsFolder 应用不应有副标题：{}（{:?}）",
                        app.title,
                        app.subtitle
                    );
                }
            }
        }

        /// `.lnk` 兜底项守卫：副标题必须指向 `.exe` 目标（IShellLinkW 解析 +
        /// 非应用链接过滤生效），图标取自目标 exe（无「快捷方式箭头」覆盖）。
        #[test]
        fn lnk_items_point_to_exe_targets() {
            for app in app_list().iter() {
                if matches!(app.launch, Launch::Lnk(_)) {
                    let sub = app.subtitle.as_deref().unwrap_or("");
                    assert!(
                        sub.to_lowercase().ends_with(".exe"),
                        "Lnk 项副标题应为目标 exe 路径：{}（{sub}）",
                        app.title
                    );
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
