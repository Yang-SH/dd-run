//! 开机自启：HKCU `Run` 键写/删。

#[cfg(windows)]
use super::dwm::wide;

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
