pub use crate::store::ThemeMode;
use egui::Color32;
use std::time::Duration;

pub const RADIUS: u8 = 10;

/// Every colour the UI draws with. Green/amber/grey keep their status meaning
/// (counting/idle/locked) in both palettes; only their shade changes for contrast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Translucent, so the widget sits lightly on the desktop.
    pub bg: Color32,
    /// Hairline border, keeping the widget's edge visible on wallpaper of a
    /// similar shade.
    pub edge: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub green: Color32,
    pub amber: Color32,
    pub grey: Color32,
}

pub const DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgba_premultiplied(26, 28, 31, 222),
    edge: Color32::from_rgba_premultiplied(22, 22, 22, 22),
    text: Color32::from_rgb(232, 234, 237),
    muted: Color32::from_rgb(154, 160, 166),
    green: Color32::from_rgb(74, 222, 128),
    amber: Color32::from_rgb(251, 191, 36),
    grey: Color32::from_rgb(107, 114, 128),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgba_premultiplied(229, 230, 231, 235),
    edge: Color32::from_black_alpha(26),
    text: Color32::from_rgb(32, 33, 36),
    muted: Color32::from_rgb(95, 99, 104),
    green: Color32::from_rgb(22, 163, 74),
    amber: Color32::from_rgb(217, 119, 6),
    grey: Color32::from_rgb(156, 163, 175),
};

/// `system` is what Windows reports for the app theme, if anything.
pub fn resolve(mode: ThemeMode, system: Option<egui::Theme>) -> &'static Palette {
    match mode {
        ThemeMode::Dark => &DARK,
        ThemeMode::Light => &LIGHT,
        ThemeMode::System => match system {
            Some(egui::Theme::Light) => &LIGHT,
            _ => &DARK,
        },
    }
}

pub fn apply(ctx: &egui::Context, p: &Palette) {
    // Pin egui's own theme to ours; left on "follow system", egui would swap in
    // its unstyled visuals whenever Windows flips, under a palette that didn't.
    ctx.set_theme(if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let mut visuals = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = p.bg;
    visuals.override_text_color = Some(p.text);
    visuals.window_stroke = egui::Stroke::NONE;
    ctx.set_visuals(visuals);
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
    fn system_mode_follows_windows() {
        assert_eq!(resolve(ThemeMode::System, Some(egui::Theme::Light)), &LIGHT);
        assert_eq!(resolve(ThemeMode::System, Some(egui::Theme::Dark)), &DARK);
    }

    #[test]
    fn system_mode_is_dark_when_windows_does_not_say() {
        assert_eq!(resolve(ThemeMode::System, None), &DARK);
    }

    #[test]
    fn explicit_modes_ignore_windows() {
        assert_eq!(resolve(ThemeMode::Dark, Some(egui::Theme::Light)), &DARK);
        assert_eq!(resolve(ThemeMode::Light, Some(egui::Theme::Dark)), &LIGHT);
    }

    /// The panel's text sits on `bg`; light mode is drawn over arbitrary
    /// wallpaper, so it needs real contrast, not just a different colour.
    #[test]
    fn text_contrasts_with_background_in_both_palettes() {
        for p in [&DARK, &LIGHT] {
            let lum = |c: Color32| {
                let [r, g, b, _] = c.to_srgba_unmultiplied();
                0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
            };
            assert!((lum(p.text) - lum(p.bg)).abs() > 150.0, "{:?}", p.dark);
        }
    }

    #[test]
    fn human_drops_hours_when_zero() {
        assert_eq!(fmt_human(Duration::from_secs(4 * 3600 + 12 * 60)), "4h 12m");
        assert_eq!(fmt_human(Duration::from_secs(12 * 60)), "12m");
    }
}
