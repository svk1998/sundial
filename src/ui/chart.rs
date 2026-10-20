use super::theme::{self, Palette};
use egui::{Align2, FontId, Rect, Vec2};
use std::time::Duration;

const BAR_AREA_HEIGHT: f32 = 64.0;
const LABEL_HEIGHT: f32 = 16.0;
const WEEKDAY_INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// Monday-first 7-bar chart. `None` entries are future days and render as
/// nothing; `Some(ZERO)` is a past day with no recorded time and renders as a
/// minimal stub bar. `today_index` is highlighted.
pub fn show(ui: &mut egui::Ui, series: &[Option<Duration>; 7], today_index: usize, p: &Palette) {
    let total_height = BAR_AREA_HEIGHT + LABEL_HEIGHT + 4.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), total_height),
        egui::Sense::hover(),
    );

    let peak = series
        .iter()
        .filter_map(|v| *v)
        .map(|d| d.as_secs_f32())
        .fold(1.0_f32, f32::max);

    let slot = rect.width() / 7.0;
    let bar_width = (slot * 0.44).min(18.0);

    for (i, value) in series.iter().enumerate() {
        let centre_x = rect.left() + slot * (i as f32 + 0.5);
        let baseline = rect.top() + BAR_AREA_HEIGHT;

        if let Some(v) = value {
            let frac = (v.as_secs_f32() / peak).clamp(0.0, 1.0);
            let height = (frac * BAR_AREA_HEIGHT).max(2.0);
            let bar = Rect::from_min_max(
                egui::pos2(centre_x - bar_width / 2.0, baseline - height),
                egui::pos2(centre_x + bar_width / 2.0, baseline),
            );
            let colour = if i == today_index { p.green } else { p.grey };
            ui.painter().rect_filled(bar, 3, colour);

            let response = ui.interact(bar, ui.id().with(("bar", i)), egui::Sense::hover());
            if response.hovered() {
                response.on_hover_text(theme::fmt_human(*v));
            }
        }

        let label_colour = if i == today_index { p.text } else { p.muted };
        ui.painter().text(
            egui::pos2(centre_x, baseline + LABEL_HEIGHT),
            Align2::CENTER_BOTTOM,
            WEEKDAY_INITIALS[i],
            FontId::proportional(10.0),
            label_colour,
        );
    }
}
