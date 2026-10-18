use egui::Color32;
use std::time::Duration;

pub const BG: Color32 = Color32::from_rgba_premultiplied(26, 28, 31, 222);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 237);
pub const MUTED: Color32 = Color32::from_rgb(154, 160, 166);
pub const GREEN: Color32 = Color32::from_rgb(74, 222, 128);
pub const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
pub const GREY: Color32 = Color32::from_rgb(107, 114, 128);
pub const RADIUS: u8 = 10;

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = BG;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.window_stroke = egui::Stroke::NONE;
    ctx.set_style(style);
}

/// `HH:MM:SS` for the widget.
pub fn fmt_clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// `4h 12m` for the panel. Under an hour, `12m`.
pub fn fmt_human(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_pads_every_field() {
        assert_eq!(
            fmt_clock(Duration::from_secs(9 * 3600 + 5 * 60 + 3)),
            "09:05:03"
        );
        assert_eq!(fmt_clock(Duration::ZERO), "00:00:00");
    }

    #[test]
    fn human_drops_hours_when_zero() {
        assert_eq!(fmt_human(Duration::from_secs(4 * 3600 + 12 * 60)), "4h 12m");
        assert_eq!(fmt_human(Duration::from_secs(12 * 60)), "12m");
    }
}
