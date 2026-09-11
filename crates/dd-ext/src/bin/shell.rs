//! dd-ext-shell —— 薄壳：`dd-ext` 内置 `spec()` 经 `run` 启动（进程模式；dev / 第三方兼容）。
//!
//! in-process 调用由宿主（dd-gui）经 `dd_ext::builtins::shell::spec()` 走，不 spawn 此进程。

use dd_ext::run;

fn main() {
    run(&dd_ext::builtins::shell::spec());
}
