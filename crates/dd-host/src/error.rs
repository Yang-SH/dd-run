//! 宿主加载扩展的错误（C3，dd-run-改进分析.md §4.6 三边界之一）。
//!
//! 覆盖「清单/信任/spawn」边界：把此前以 `String` 拼接的错误（信任拒绝 /
//! spawn 失败 / initialize 失败）收拢为结构化枚举——`Display` 与拼接版
//! **逐字一致**（日志、设置页失败行、既有测试零变化）；调用方可按变体
//! 分流用户文案（UI 侧 `user_message`），不再对字符串做 contains 判型。

/// 扩展加载失败（信任门禁 / spawn / initialize 握手）。
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// S-05 信任门禁拒绝（未获信任者一律不拉起；两道门禁的第二道）。
    /// `label`：待批准 / 已被阻止。
    #[error("扩展未获信任（{label}）：{id} —— 请在设置页「扩展」中允许该扩展")]
    Untrusted { id: String, label: &'static str },
    /// 扩展既非内置注册也无可用执行文件（`ext_client::open` 的输入不一致）。
    #[error("扩展无注册信息（既非内置也无可执行文件）")]
    NoRegistration,
    /// spawn 失败（路径 / PATH / 权限类；附被尝试的命令路径——可操作线索）。
    /// 字段名用 `reason` 而非 `source`（thiserror 会把 `source` 当错误链源）。
    #[error("spawn 失败：{reason}（命令：{command}）")]
    Spawn { reason: String, command: String },
    /// initialize 握手失败（超时 / 版本不兼容 / EOF 等；`detail` 为诊断后缀）。
    #[error("initialize 失败：{reason}{detail}")]
    Initialize { reason: String, detail: String },
}
