use super::{chart, theme};
use crate::stats::Summary;
use crate::tracker::DerivedState;
use crate::ui::widget::dot_colour;
use chrono::Datelike;
use egui::{FontId, RichText, Vec2};
use std::time::Duration;

pub const SIZE: Vec2 = Vec2::new(320.0, 290.0);

pub fn show(ui: &mut egui::Ui, summary: &Summary, state: DerivedState, today_index: usize) {
    ui.add_space(14.0);

    // Today is the hero number - it is what you opened the panel to see.
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("TODAY").size(10.0).color(theme::MUTED));
            ui.label(
                RichText::new(theme::fmt_human(summary.today))
                    .font(FontId::proportional(30.0))
                    .color(theme::TEXT),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.add_space(16.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(rect.center(), 3.5, dot_colour(state));
        });
    });

    ui.add_space(14.0);

    // One scannable row rather than three stacked rows, which keeps the panel short.
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        stat(ui, "Yesterday", summary.yesterday);
        stat(ui, "This week", summary.this_week);
        stat(ui, "Daily avg", summary.daily_average);
    });

    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        // Leave the right margin inside the panel. Spacing added after a
        // full-width chart lands past the edge, and egui then widens the layout
        // for everything below it - clipping the right-aligned history times.
        let width = ui.available_width() - 12.0;
        ui.vertical(|ui| {
            ui.set_width(width);
            chart::show(ui, &summary.week_series, today_index);
        });
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    if summary.history.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No history yet")
                    .size(12.0)
                    .color(theme::MUTED),
            );
        });
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (date, value) in &summary.history {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!(
                        "{}  {:02} {}",
                        weekday(date),
                        date.day(),
                        month(date)
                    ))
                    .size(12.0)
                    .color(theme::MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(theme::fmt_human(*value))
                            .size(12.0)
                            .color(theme::TEXT),
                    );
                });
            });
            ui.add_space(2.0);
        }
    });
}

fn stat(ui: &mut egui::Ui, label: &str, value: Duration) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(10.0).color(theme::MUTED));
        ui.label(
            RichText::new(theme::fmt_human(value))
                .size(14.0)
                .color(theme::TEXT),
        );
    });
    ui.add_space(14.0);
}

fn weekday(d: &chrono::NaiveDate) -> &'static str {
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d.weekday().num_days_from_monday() as usize]
}

fn month(d: &chrono::NaiveDate) -> &'static str {
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][d.month0() as usize]
}
