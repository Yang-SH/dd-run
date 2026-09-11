//! dd-ext 内置扩展集合（M9：集中 5 个 `ExtensionSpec`，运行期按生效语言构造）。
//!
//! 原各 `bin/*.rs` 的 `spec()` 已上移至此（见 `apps.rs` / `calc.rs` / `system.rs` /
//! `websearch.rs` / `shell.rs`）；各 `bin/*.rs` 退化为 `dd_ext::run(&spec())` 薄壳。
//! 宿主（dd-gui）将经 [`builtin_specs`] 拿到规格做 in-process 调用（B2）。

use crate::ExtensionSpec;

pub mod apps;
pub mod calc;
pub mod shell;
pub mod system;
pub mod websearch;

/// 运行期构造全部内置扩展规格（按生效语言 `DDRUN_LANG` 选文案）。
pub fn builtin_specs() -> Vec<ExtensionSpec> {
    vec![
        apps::spec(),
        calc::spec(),
        system::spec(),
        websearch::spec(),
        shell::spec(),
    ]
}
