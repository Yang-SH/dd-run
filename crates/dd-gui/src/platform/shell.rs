//! 系统外壳动作：UAC 提权、资源管理器定位、默认程序打开。

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

/// 路径字符门控（F9/P6.1，2026-10-10）：与 `dd-ext/src/bin/search.rs` 的
/// `valid_reveal_path`（S-08）**同规则**——拒绝双引号 / 控制字符（含 \t\n\r
/// 与 0x7F）/ `%` / `^`。explorer 参数为单 arg 直传（无 shell 中间层），但
/// 参数仍会被 explorer 二次解析（`%VAR%` 展开、`^` 转义），纵深防御与其对齐。
/// 规则收敛候选（归属 `dd-protocol` 纯函数）留待后续，本轮不跨 crate 重构。
pub(super) fn reveal_path_allowed(path: &str) -> bool {
    !path.is_empty()
        && !path
            .chars()
            .any(|c| c == '"' || c == '%' || c == '^' || c.is_control())
}

/// 在资源管理器中定位文件（`explorer /select,<path>`，10B.2）。
///
/// 若目标路径不存在，fallback 到其父目录；父目录也不存在则返回错误，避免
/// Windows 在 `/select` 失效时随机打开「文档」等默认位置。
/// F9（P6.1）：入参先过 [`reveal_path_allowed`] 字符门控（与 search 扩展
/// S-08 同规则）——含 `%`/`^`/引号/控制字符的路径拒绝并报错（调用点
/// `ctx_menu.rs` 落 `show_error_toast`），不再直通 explorer。
#[cfg(windows)]
pub(crate) fn reveal_in_folder(path: &str) -> Result<(), String> {
    use std::path::Path;

    if !reveal_path_allowed(path) {
        // 错误信息不回显原路径：路径可能含控制字符/换行，toast 呈现不可控
        return Err("路径含不允许字符（引号 / % / ^ / 控制字符）".to_string());
    }
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

/// §7.4 `host/open_url` 的宿主侧**执行策略**（S-03，2026-09-23）：只放行 `http` / `https` / `file`。
///
/// 为什么需要：`host/open_url` 由任意扩展发起（清单声明 `capabilities` 即可），
/// 加固前实现会把任意字符串交给 `ShellExecuteW` / 默认浏览器——于是 `ms-msdt:`
/// （Follina 类）、`search-ms:`、`vbscript:`、`javascript:` 以及任何自定义协议都会被
/// **静默**交给系统处理器或浏览器参数。收窄 scheme 白名单即消除这一整类调用。
///
/// 为什么**保留** `file://`：文件搜索扩展的「打开」动作正是用
/// `host/open_url` + `file://`（`bin/search.rs` 约 :1492）打开用户选中的文件；
/// 一律只放行 http(s) 会**直接打断该功能**（功能回归）。`file://` 的语义是
/// 「双击等价」（目录 → Explorer、文件 → 关联程序），属**设计如此**，故本策略只做
/// scheme 收窄、不改其行为；调用点另记扩展 id 以便溯源。
///
/// 同时拒绝空串、控制字符与裸双引号（在任何合法 URL 中都不可能出现）。
pub(crate) fn is_allowed_open_url(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() || u.chars().any(|c| c.is_control() || c == '"') {
        return false;
    }
    // ⚠️ 必须按**字节**比较前缀：不可写 `u[..p.len()]`——入参可能以多字节字符开头
    //（如 `C:\中文\文件.txt`、`中中中中`），按字节长度切 `&str` 会落在字符边界内部
    // 而 **panic**（实施时已实测复现：`end byte index 7 is not a char boundary`）。
    // `[u8]::eq_ignore_ascii_case` 对任意字节序列都成立，且 ASCII 前缀比较语义不变。
    let head = u.as_bytes();
    ["http://", "https://", "file://"]
        .iter()
        .any(|p| u.len() > p.len() && head[..p.len()].eq_ignore_ascii_case(p.as_bytes()))
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
