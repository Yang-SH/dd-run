//! M9 集成测试：`i18n::set_lang` 的可重设语义。
//!
//! 独立测试二进制 = 独立进程 → 翻转进程级生效语言不会与 `dd-ext` 单测
//! （同一 crate 的 unit test 进程）竞争，也不会污染其 `tr()` 文案断言。
//!
//! 背景：内置扩展 M9 起 in-process 运行于宿主进程，宿主需在设置页切换语言后
//! 重设生效语言；原先 `OnceLock` 缓存一次的实现无法反映这种切换。

use dd_ext::i18n::{current_lang, set_lang, tr, Lang};

#[test]
fn set_lang_overrides_cached_language() {
    // 首次访问 = env 解析（测试环境通常未设 DDRUN_LANG → 中文）
    assert_eq!(current_lang(), Lang::ZhCn, "未设 env 时应回落到中文");
    assert_eq!(tr("中文", "English"), "中文");

    // 显式设为英文 → tr 立即反映（这是 OnceLock → AtomicU8 的核心收益）
    set_lang(Lang::EnUs);
    assert_eq!(current_lang(), Lang::EnUs);
    assert_eq!(tr("中文", "English"), "English");

    // 可反复重设（模拟宿主每次重聚合按当前设置页语言重设）
    set_lang(Lang::ZhCn);
    assert_eq!(current_lang(), Lang::ZhCn);
    assert_eq!(tr("中文", "English"), "中文");

    set_lang(Lang::EnUs);
    assert_eq!(tr("中文", "English"), "English");
}
