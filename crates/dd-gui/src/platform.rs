//! 系统副作用：字体加载、窗口居中定位、UAC 提权、资源管理器定位。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1），方法体逐字未改。

/// 热键捕获的低级键盘钩子（2026-09-29 B 方案：Win 键可录 + 开始菜单不弹）。
pub mod capture_hook;

mod autostart;
mod cursor;
mod dwm;
mod fonts;
mod lang;
mod paths;
mod shell;
mod window;

pub use autostart::set_autostart;
pub use cursor::{MouseHideScope, CURSOR_IDLE_HIDE};
pub use dwm::{
    apply_system_backdrop, apply_window_chrome, is_window_visible, set_immersive_dark,
    set_window_border, system_accent_color, SystemBackdrop,
};
pub use fonts::{setup_cjk_fonts, trim_working_set};
pub use lang::system_ui_lang;

pub(crate) use paths::resolve_file_url_to_path;
pub(crate) use shell::{is_allowed_open_url, open_path, reveal_in_folder, run_as_admin};

#[cfg(test)]
use cursor::cursor_should_hide;
#[cfg(test)]
use paths::file_url_candidates;
#[cfg(test)]
use shell::reveal_path_allowed;

#[cfg(test)]
mod tests {
    use super::{
        cursor_should_hide, file_url_candidates, is_allowed_open_url, reveal_path_allowed,
        MouseHideScope, CURSOR_IDLE_HIDE,
    };
    use std::time::{Duration, Instant};

    /// F11/P6.2 跨 crate 契约：search 扩展（`dd-ext/src/bin/search.rs`
    /// `percent_encode_path`）编码的 file URL 必须可由本侧 `file_url_candidates`
    /// 无损还原原路径。测试内复制其编码器（保留集 = unreserved + `/` + `:`），
    /// 两侧改动须互相同步（注释互指）。
    #[test]
    fn file_url_candidates_restore_percent_encoded_from_search_ext() {
        fn encode_like_search_ext(s: &str) -> String {
            let mut out = String::with_capacity(s.len());
            for &b in s.as_bytes() {
                match b {
                    b'A'..=b'Z'
                    | b'a'..=b'z'
                    | b'0'..=b'9'
                    | b'-'
                    | b'.'
                    | b'_'
                    | b'~'
                    | b'/'
                    | b':' => out.push(b as char),
                    _ => out.push_str(&format!("%{b:02X}")),
                }
            }
            out
        }
        // 含空格/中文/`%`/`#` 的路径：字面候选不存在 → decode 候选命中
        for p in [r"C:\my files\报告 final.txt", r"C:\a%b#c.txt"] {
            let url = format!("file:///{}", encode_like_search_ext(&p.replace('\\', "/")));
            let restored = file_url_candidates(&url);
            assert!(
                restored
                    .iter()
                    .any(|c| c.replace('\\', "/") == p.replace('\\', "/")),
                "编码 URL 必须可还原原路径：{url} → {restored:?}"
            );
        }
    }

    /// F9/P6.1：reveal 门控与 search 扩展 S-08（`valid_reveal_path`）同规则的
    /// 镜像用例集——引号/控制字符/%/^ 拒绝；合法含空格/中文路径放行。
    #[test]
    #[cfg(windows)]
    fn reveal_path_allowed_mirrors_search_s08_rules() {
        // 合法放行
        assert!(
            reveal_path_allowed(r"C:\my files\报告 final.txt"),
            "空格与中文"
        );
        assert!(reveal_path_allowed(r"C:\a\b\c.exe"), "常规路径");
        // 非法拒绝（镜像 search.rs S-08 用例集）
        assert!(!reveal_path_allowed(r#"C:\a"b"#), "双引号");
        assert!(!reveal_path_allowed("C:\\a\nb"), "换行（控制字符）");
        assert!(!reveal_path_allowed("C:\\a\tb"), "制表符（控制字符）");
        assert!(
            !reveal_path_allowed(r"C:\a%b"),
            "百分号（explorer 变量展开）"
        );
        assert!(!reveal_path_allowed(r"C:\a^b"), "脱字符（explorer 转义）");
        assert!(!reveal_path_allowed("C:\u{7f}b"), "DEL（控制字符）");
        assert!(!reveal_path_allowed(""), "空路径");
    }

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
    #[cfg(windows)]
    fn resolve_file_url_to_path_prefers_existing_literal_file() {
        // 存在性优选：两个同名候选都在时，先看字面 `%` 文件，再看解码后文件。
        // 本函数语义 = Windows explorer 打开链路（§9.6，P4 口径：候选为反斜杠
        // UNC/盘符形态）——跨平台 file:// 语义属 O6 后续批次。
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

    // ── S-03（2026-09-23）：host/open_url 的 scheme 白名单 ──────────────

    /// 放行：`http` / `https` / `file`（大小写不敏感）。这三者是当前全部**合法消费者**：
    /// WebSearch 开网页（http/https）、文件搜索的「打开」动作（file）。
    #[test]
    fn open_url_allows_http_https_file_only() {
        for ok in [
            "https://github.com/search?q=dd-run",
            "http://localhost:8080/x",
            "HTTPS://Example.com/Path",
            "File:///C:/proj/src/main.rs",
            "file://server/share/%E6%8A%A5%E5%91%8A.pdf",
            "file:///G:/AI/dd-run",
        ] {
            assert!(is_allowed_open_url(ok), "合法目标被误拒：{ok}");
        }
    }

    /// 拒绝：可被滥用的协议（`ms-msdt:` 是 Follina 类攻击链的经典入口）与
    /// 任何自定义 scheme、空串、控制字符、裸引号。
    #[test]
    fn open_url_rejects_untrusted_schemes() {
        for bad in [
            "",
            "   ",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=secret",
            "vbscript:msgbox(1)",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "steam://rungameid/1",
            "shell:AppsFolder\\x",
            "about:blank",
            "C:\\Windows\\System32\\calc.exe",
            "https://a/b\r\nX",
            "https://a/\"&calc&\"",
            "https:",
        ] {
            assert!(!is_allowed_open_url(bad), "危险目标被放行：{bad:?}");
        }
    }

    /// **首要判据：不 panic**。非 ASCII 开头的输入（Windows 中文路径、CJK 串、全角
    /// 伪装 scheme）曾因「按字节长度切 `&str`」而 panic ——
    /// `end byte index 7 is not a char boundary; it is inside '中'`。
    /// 现改为**按字节**比较前缀（`[u8]::eq_ignore_ascii_case`），本用例把该回归锁死。
    #[test]
    fn open_url_handles_non_ascii_without_panicking() {
        for s in [
            "中中中中",          // 12 字节：切片点 7 落在字符内部（旧实现必 panic）
            r"C:\中文\文件.txt", // 真实中文路径形态（旧实现必 panic）
            r"C:\报告\2026 年度.docx",
            "ｈｔｔｐ://example.com", // 全角伪装 scheme → 必须拒绝
            "文件:///C:/x",
            "https://例え.jp/パス", // 合法 https + 非 ASCII 正文 → 放行
        ] {
            let _ = is_allowed_open_url(s); // 走到这里即证明未 panic
        }
        assert!(!is_allowed_open_url("中中中中"));
        assert!(!is_allowed_open_url(r"C:\中文\文件.txt"));
        assert!(!is_allowed_open_url("ｈｔｔｐ://example.com"));
        assert!(!is_allowed_open_url("文件:///C:/x"));
        assert!(is_allowed_open_url("https://例え.jp/パス"));
    }
}
