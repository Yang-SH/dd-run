//! §5.1（dd-run-改进分析.md）：使用频次（frecency）的应用侧装配——
//! 启动加载 / 激活记录 / 排序快照注入 / 设置页重置。
//!
//! 数据权威副本在 `pool.usage`（[`crate::usage::Usage`]）；`PanelState` 只持
//! `view.usage_scores` 快照参与排序。Root 列表被聚合整包替换（`aggregate.rs`
//! 的 `*root_mut() = root`）会丢快照——两处重建点之后均调 [`PaletteApp::sync_usage_index`]
//! 重新注入。

use super::*;
use crate::app::toast_state::ToastKind;

impl PaletteApp {
    /// bin 入口装配：从 `usage.json` 装载使用频次并注入排序快照。
    /// 刻意不在 [`PaletteApp::new`] 内做（保持纯注入——测试夹具不碰文件系统）。
    /// `pub`：调用方是 bin（`main.rs`），在 lib 外。
    pub fn load_usage(&mut self) {
        self.pool.usage = crate::usage::Usage::load();
        self.sync_usage_index();
    }

    /// `pool.usage` → `view.usage_scores` → Root 列表。空表注入 `None`
    ///（排序行为与本功能引入前逐字节一致——快照金图零影响的关键）；
    /// `PanelState::set_usage` 自判快照未变化时为 no-op，不触发重算。
    pub(crate) fn sync_usage_index(&mut self) {
        let scores = if self.pool.usage.is_empty() {
            None
        } else {
            Some(self.pool.usage.scores())
        };
        self.view.usage_scores = scores.clone();
        self.view.stack.root_mut().list.set_usage(scores);
    }

    /// 记录一次条目激活（`confirm_selected` 唯一调用点——Root/嵌套页、
    /// 键盘/鼠标共用）。记录即原子落盘（激活为低频人工动作）。
    pub(crate) fn record_usage(&mut self, ext_id: &str, item_id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.pool.usage.record(ext_id, item_id, now);
        // 测试不落盘（核对轮记档）：`make_app` 夹具无路径注入，`save()` 写的是
        // 真实 `%APPDATA%\dd-run\usage.json`——reset 测试会把开发者真实数据清空。
        #[cfg(not(test))]
        self.pool.usage.save();
        self.sync_usage_index();
    }

    /// 设置页「重置使用数据」：清表 + 落盘 + 快照回退 + 确认 toast。
    pub(crate) fn reset_usage_data(&mut self) {
        self.pool.usage.reset();
        // 同 [`Self::record_usage`]：测试不落盘。
        #[cfg(not(test))]
        self.pool.usage.save();
        self.sync_usage_index();
        let lang = self.view.lang_effective;
        self.show_toast_kind(
            ToastKind::Info,
            crate::text::t(lang, "toast.usage_reset"),
            Some(2_000),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::state::PanelItem;
    use crate::state::PanelState;
    use crate::test_support::make_app;
    use dd_protocol::model::CommandRef;

    /// 激活记录：confirm_selected（找不到扩展、invoke 不发）仍计数 + 快照注入。
    #[test]
    fn confirm_selected_records_usage_and_syncs_snapshot() {
        let mut app = make_app();
        let mut item = PanelItem::new("cmd1");
        item.ext_id = "ext.a".into();
        item.command = CommandRef::Invoke;
        app.view.stack.root_mut().list = PanelState::new(vec![item]);

        app.confirm_selected();

        assert_eq!(app.pool.usage.count("ext.a", "cmd1"), 1, "激活应计数");
        assert!(app.view.usage_scores.is_some(), "记录后应注入排序快照");
        // 重复激活累计
        app.confirm_selected();
        assert_eq!(app.pool.usage.count("ext.a", "cmd1"), 2);
    }

    /// 重置：清表 + 快照回退为 None（排序行为回到引入前）。
    #[test]
    fn reset_usage_data_clears_and_unsets_snapshot() {
        let mut app = make_app();
        let mut item = PanelItem::new("cmd1");
        item.ext_id = "ext.a".into();
        app.view.stack.root_mut().list = PanelState::new(vec![item]);
        app.record_usage("ext.a", "cmd1");
        assert!(app.view.usage_scores.is_some());

        app.reset_usage_data();

        assert!(app.pool.usage.is_empty(), "重置应清空记录");
        assert!(app.view.usage_scores.is_none(), "重置后快照应回 None");
        assert!(app.pool.usage.scores().is_empty());
    }
}
