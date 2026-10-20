use super::theme::{self, Palette};
use crate::tracker::DerivedState;
use egui::{Align2, Color32, FontId, Sense, Vec2};
use std::time::Duration;

/// The widget at `WidgetSize::Small`; other sizes scale everything uniformly.
const BASE: Vec2 = Vec2::new(140.0, 40.0);

pub fn size(scale: f32) -> Vec2 {
    BASE * scale
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetAction {
    None,
    TogglePanel,
    StartDrag,
    ShowMenu,
}

/// Green while counting, amber while idle, grey while locked. With idle-pause in
/// play, a stopped timer and a running one are otherwise indistinguishable.
pub fn dot_colour(state: DerivedState, p: &Palette) -> Color32 {
    match state {
        DerivedState::Counting => p.green,
        DerivedState::IdlePaused => p.amber,
        DerivedState::Locked => p.grey,
    }
}

pub fn show(
    ui: &mut egui::Ui,
    today: Duration,
    state: DerivedState,
    p: &Palette,
    scale: f32,
) -> WidgetAction {
    let rect = ui.max_rect();
    let radius = (f32::from(theme::RADIUS) * scale).round() as u8;

    ui.painter().rect(
        rect,
        radius,
        p.bg,
        egui::Stroke::new(1.0_f32, p.edge),
        egui::StrokeKind::Inside,
    );

    let dot_centre = egui::pos2(rect.left() + 16.0 * scale, rect.center().y);
    ui.painter()
        .circle_filled(dot_centre, 3.5 * scale, dot_colour(state, p));

    ui.painter().text(
        egui::pos2(rect.left() + 32.0 * scale, rect.center().y),
        Align2::LEFT_CENTER,
        theme::fmt_clock(today),
        FontId::monospace(16.0 * scale),
        p.text,
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
