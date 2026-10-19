use super::theme;
use crate::tracker::DerivedState;
use egui::{Align2, Color32, FontId, Sense, Vec2};
use std::time::Duration;

pub const SIZE: Vec2 = Vec2::new(140.0, 40.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetAction {
    None,
    TogglePanel,
    StartDrag,
    ShowMenu,
}

/// Green while counting, amber while idle, grey while locked. With idle-pause in
/// play, a stopped timer and a running one are otherwise indistinguishable.
pub fn dot_colour(state: DerivedState) -> Color32 {
    match state {
        DerivedState::Counting => theme::GREEN,
        DerivedState::IdlePaused => theme::AMBER,
        DerivedState::Locked => theme::GREY,
    }
}

pub fn show(ui: &mut egui::Ui, today: Duration, state: DerivedState) -> WidgetAction {
    let rect = ui.max_rect();

    ui.painter().rect_filled(rect, theme::RADIUS, theme::BG);

    let dot_centre = egui::pos2(rect.left() + 16.0, rect.center().y);
    ui.painter()
        .circle_filled(dot_centre, 3.5, dot_colour(state));

    ui.painter().text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        theme::fmt_clock(today),
        FontId::monospace(16.0),
        theme::TEXT,
    );

    let response = ui.interact(rect, ui.id().with("widget"), Sense::click_and_drag());

    // Primary button only. egui senses drags from any button, so reacting to a
    // plain `drag_started()` would start an OS drag loop on right-press and
    // swallow the menu.
    //
    // egui's built-in drag threshold separates a click from a drag, so a small
    // wobble while clicking still opens the panel rather than moving the widget.
    if response.drag_started_by(egui::PointerButton::Primary) {
        WidgetAction::StartDrag
    } else if response.clicked() {
        WidgetAction::TogglePanel
    } else if response.secondary_clicked() {
        // Handled by the caller as a native popup - see Tray::show_context_menu.
        WidgetAction::ShowMenu
    } else {
        WidgetAction::None
    }
}
