//! §5.2（dd-run-改进分析.md）：检查更新——读 GitHub Releases 的 `latest`，
//! 与当前版本比较；**不做自动安装**（便携版静默替换 exe 风险大于收益，原案口径）。
//!
//! HTTP 方案（本批决策记档）：**不引 HTTP crate**（运行时面 lock 新增 + 二进制
//! 体积税），走系统 `curl`（Windows 10 1803+ 自带 `System32\curl.exe`，POSIX
//! 普遍可用；Git Bash 亦有）。`--max-time` 由调用方给 3 s（验收「点击后 3 s
//! 内显示」由 curl 超时硬约束承接）。GitHub API 要求 User-Agent；匿名限额
//! 60 次/时/IP，手动检查频度远低于此。
//!
//! 版本比较口径：取 `tag_name` 剥离前导 `v`，按 `MAJOR.MINOR.PATCH` 三段
//! 数值比较——**预发布后缀（如 `-beta`）被忽略**（`v0.3.0-beta` 视为 0.3.0，
//! 会提示更新；近似记档，语义化预发布排序不在 v1 范围）。解析失败（API 出错
//! 返回非 JSON / 字段缺失）→ `Failed`，UI 提示而非静默。

use serde_json::Value;

/// 当前产品版本（dd-gui = 产品版本，workspace 继承）。
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 本仓库 latest release 的 GitHub API 端点。
pub const RELEASES_API: &str = "https://api.github.com/repos/Yang-SH/dd-run/releases/latest";

/// 检查结果（纯数据，UI 层转 toast 文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// 已是最新（远端 ≤ 当前）。
    UpToDate,
    /// 有新版本（剥 `v` 后的语义版本字符串，如 `"0.3.0"`）。
    Available { version: String },
    /// 检查失败（网络 / 超时 / 响应异常），附可读原因。
    Failed(String),
}

/// 从 GitHub releases/latest 响应体提取 `tag_name`（缺失/非法 JSON → None）。
pub fn parse_tag_name(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    v.get("tag_name")?.as_str().map(str::to_string)
}

/// 剥离 tag 的前导 `v`/`V`（`v0.2.2` → `0.2.2`）。
pub fn normalize_tag(tag: &str) -> &str {
    tag.strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag)
}

/// 三段数值版本解析（`0.2.1` → (0,2,1)；预发布后缀截断；失败 → None）。
fn parse_triple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 比较：远端 tag 与当前版本谁新（解析失败的一侧 → `Failed`）。
pub fn classify(current: &str, tag: &str) -> UpdateStatus {
    let Some(remote) = parse_triple(normalize_tag(tag)) else {
        return UpdateStatus::Failed(format!("远端版本号无法解析：{tag}"));
    };
    let Some(local) = parse_triple(current) else {
        return UpdateStatus::Failed(format!("本地版本号无法解析：{current}"));
    };
    if remote > local {
        UpdateStatus::Available {
            version: normalize_tag(tag).to_string(),
        }
    } else {
        UpdateStatus::UpToDate
    }
}

/// 经系统 `curl` 拉取 latest release 的 `tag_name`（网络/超时/解析失败 → Err）。
pub fn fetch_latest_tag(timeout_secs: u64) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL", // fail 静默 + 重定向跟随；-S 把错误送到 stderr
            "--max-time",
            &timeout_secs.to_string(),
            "-A",
            &format!("dd-run/{}", CURRENT_VERSION),
            RELEASES_API,
        ])
        .output()
        .map_err(|e| format!("无法启动 curl（{e}）"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl 退出码 {:?}{}",
            output.status.code(),
            detail
                .trim()
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!("：{}", detail.trim()))
        ));
    }
    parse_tag_name(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "响应缺少 tag_name".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_name_extracts_field() {
        assert_eq!(
            parse_tag_name(r#"{"tag_name":"v0.3.0","name":"x"}"#).as_deref(),
            Some("v0.3.0")
        );
        assert_eq!(parse_tag_name(r#"{"name":"x"}"#), None, "缺 tag_name");
        assert_eq!(parse_tag_name("not json"), None, "非法 JSON");
        assert_eq!(parse_tag_name(r#"{"tag_name":123}"#), None, "类型损坏");
    }

    #[test]
    fn classify_compares_triples_and_ignores_prerelease_suffix() {
        assert_eq!(
            classify("0.2.1", "v0.2.2"),
            UpdateStatus::Available {
                version: "0.2.2".into()
            }
        );
        assert_eq!(classify("0.2.1", "v0.2.1"), UpdateStatus::UpToDate);
        assert_eq!(classify("0.3.0", "v0.2.9"), UpdateStatus::UpToDate);
        assert_eq!(
            classify("0.2.1", "v1.0.0"),
            UpdateStatus::Available {
                version: "1.0.0".into()
            },
            "远端主版本更高 → Available"
        );
        assert_eq!(
            classify("0.2.1", "v0.3.0-beta"),
            UpdateStatus::Available {
                version: "0.3.0-beta".into()
            },
            "比较忽略预发布后缀（0.3.0-beta > 0.2.1）；展示保留完整 tag（近似记档）"
        );
        assert!(
            matches!(classify("0.2.1", "weird"), UpdateStatus::Failed(_)),
            "远端版本号无法解析 → Failed"
        );
    }

    #[test]
    fn normalize_strips_leading_v() {
        assert_eq!(normalize_tag("v0.2.2"), "0.2.2");
        assert_eq!(normalize_tag("0.2.2"), "0.2.2");
        assert_eq!(normalize_tag("V1.0"), "1.0");
    }
}
