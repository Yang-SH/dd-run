//! 鼠标闲置隐藏守卫。

use eframe::egui;

/// 指针静止多久后重新隐藏光标（亚克力面板：不挡视线）。
///
/// 命令面板是短驻留工具（唤起→输入→回车），阈值取 1.5s：键盘流中鼠标一动
/// 就恢复、静止片刻再隐藏，不会频繁闪烁。
pub const CURSOR_IDLE_HIDE: std::time::Duration = std::time::Duration::from_millis(1500);

/// 面板显示期间的鼠标闲置隐藏守卫（v4.17 亚克力体验优化）。
///
/// **语义（v4.17a 真机修复）**：静止隐藏、移动即恢复——不是"面板期间一直隐藏"。
/// - 面板唤起后鼠标尚未动过 → 隐藏（满足"启动面板默认隐藏鼠标、第一项高亮"）；
/// - 鼠标一动 → **立即恢复** `Default`（用户主动用鼠标时看得见指针）；
/// - 静止超过 [`CURSOR_IDLE_HIDE`] → 重新隐藏（不挡亚克力背景）。
///
/// 设计：
/// - `new()` 仅标记 active（不立即改 egui——chrome_end 每帧尾会按缩放热区
///   重设 `cursor_icon`，直接设一次会被覆盖）。
/// - `apply(ctx)`：**每帧 ui() 早期调用**（chrome_begin 之前），内部采样指针
///   位置判定"本帧是否活动"，再决定光标图标。
///
/// 为什么需要 per-frame apply：
/// - egui 0.36 没有 `set_cursor_visible`，只有 `set_cursor_icon`。
/// - `chrome_end` 每帧尾根据缩放热区重设光标（`ResizeNs` 等），会覆盖本帧
///   设置 → 必须每帧重设。覆盖优先级正确：缩放热区显 Resize（拖拽反馈优先），
///   主体区域才隐藏。
///
/// 安全性：即便守卫意外泄漏，egui 每帧由 chrome_end 兜底，不存在
/// Win32 `ShowCursor` 计数器那种"永久丢失鼠标"的坑。
///
/// 用法：
/// - PaletteApp 持有 `Option<MouseHideScope>`；
///   - `show()` 时 `Some(MouseHideScope::new())`
///   - `ui()` 早期：`if let Some(scope) = self.mouse_hide.as_mut() { scope.apply(&ctx); }`
///   - `hide()` 时 `take()` 并立即 `set_cursor_icon(Default)`。
pub struct MouseHideScope {
    active: bool,
    /// 上次检测到指针活动（移动/滚动/按下）的时刻；`None` = 唤起后尚未动过。
    last_activity: Option<std::time::Instant>,
    /// 上一帧指针位置（`None` = 尚未采样；首帧只采样不判活动，避免"唤起面板"
    /// 这一动作本身被误判成用户移动鼠标）。
    last_pos: Option<egui::Pos2>,
}

impl MouseHideScope {
    /// 进入面板：标记 active，下一帧起每帧 `apply()` 按闲置时长决定光标。
    pub fn new() -> Self {
        Self {
            active: true,
            last_activity: None,
            last_pos: None,
        }
    }

    /// 每帧 ui() 早期调用（chrome_begin 之前）：采样指针活动 → 设置光标图标。
    ///
    /// 非 active 时 no-op（守卫已释放，光标由 `hide()` 立即恢复，不依赖本帧）。
    pub fn apply(&mut self, ctx: &egui::Context) {
        if !self.active {
            return;
        }
        let mut moved = false;
        ctx.input(|i| {
            // `hover_pos` = 指针在窗口内；窗口外（None）时保持上一帧基准，
            // 不误判为移动（指针移出再移回会正常触发）。
            let pos = i.pointer.hover_pos().or_else(|| i.pointer.latest_pos());
            match (self.last_pos, pos) {
                (Some(prev), Some(cur)) => {
                    if (prev - cur).length() > 0.5 {
                        moved = true;
                    }
                }
                (None, Some(_)) => {} // 首帧：只采样，不算活动
                (_, None) => {}
            }
            if let Some(cur) = pos {
                self.last_pos = Some(cur);
            }
            // 滚动与按下也算活动（键盘/滚轮浏览时不隐藏光标）。
            if i.smooth_scroll_delta.length() > 0.0 || i.pointer.any_pressed() {
                moved = true;
            }
        });
        if moved {
            self.last_activity = Some(std::time::Instant::now());
        }
        let hide = cursor_should_hide(self.last_activity, std::time::Instant::now());
        ctx.set_cursor_icon(if hide {
            egui::CursorIcon::None
        } else {
            egui::CursorIcon::Default
        });
    }

    /// 指针自面板唤起后**是否活动过**（移动/滚动/按下）。
    ///
    /// 供列表 hover 抑制使用：鼠标未动过时，指针可能恰好停在结果行上（唤起前
    /// 的残留位置），此时不应显示 hover 高亮、也不应让 hover 行抢走键盘选中
    /// ——否则用户看到的是"鼠标所在行被选中"而非第一项。
    pub fn pointer_engaged(&self) -> bool {
        self.last_activity.is_some()
    }

    /// 主动退出面板：active=false（后续 `apply()` 变 no-op）。
    pub fn release(mut self) {
        self.active = false;
    }
}

/// 纯判定（便于单测）：`None` = 从未活动 → 隐藏；否则静止满 [`CURSOR_IDLE_HIDE`] → 隐藏。
pub(super) fn cursor_should_hide(
    last_activity: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    match last_activity {
        None => true,
        Some(t) => now.saturating_duration_since(t) >= CURSOR_IDLE_HIDE,
    }
}

impl Default for MouseHideScope {
    fn default() -> Self {
        Self::new()
    }
}
