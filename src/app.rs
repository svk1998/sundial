use crate::engine::{Command, Engine};
use crate::platform::monitor;
use crate::store::{ThemeMode, WidgetSize};
use crate::tray::{Tray, TrayAction};
use crate::ui::{analytics, theme, widget};
use chrono::Datelike;
use egui::ViewportCommand;
use std::time::{Duration, Instant};

fn window_hwnd(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// Title of the analytics viewport, used to find its native window.
const PANEL_TITLE: &str = "Sundial Analytics";

/// Clicking the widget while the panel is focused fires both "panel lost focus"
/// and "widget clicked", which would close and immediately reopen it. Ignore a
/// toggle for this long after the panel closes.
const REOPEN_GUARD: Duration = Duration::from_millis(150);

/// How long after startup or a size change the widget is held on-screen.
const KEEP_ON_SCREEN_FOR: Duration = Duration::from_secs(2);

pub struct SundialApp {
    engine: Engine,
    tray: Option<Tray>,
    panel_open: bool,
    closed_at: Option<Instant>,
    widget_visible: bool,
    autostart: bool,
    theme_mode: ThemeMode,
    widget_size: WidgetSize,
    /// The palette last pushed into egui's style; re-applied only on change.
    applied: Option<theme::Palette>,
    /// Until then, keep the widget inside its monitor's work area. Resizes land
    /// asynchronously, so this runs for a moment after one rather than once;
    /// it is time-boxed so it never fights a drag across monitors.
    keep_on_screen_until: Option<Instant>,
    /// The panel closes on focus loss, but it does not hold focus on the frame
    /// it is created - the widget still does. Without this latch it would
    /// dismiss itself immediately and could never be seen.
    panel_focused_once: bool,
    last_pos: Option<egui::Pos2>,
    /// The widget's native window, needed to pop a Win32 context menu.
    hwnd: Option<isize>,
}

impl SundialApp {
    pub fn new(cc: &eframe::CreationContext<'_>, engine: Engine) -> Self {
        let (autostart, widget_visible) = (engine.initial_autostart, engine.initial_visible);
        let (theme_mode, widget_size) = (engine.initial_theme, engine.initial_size);

        // The tray must be built on the thread running the winit message loop.
        // It pumps its own events on a background thread and wakes us via the
        // context, so it keeps working while the widget is hidden.
        let hwnd = window_hwnd(cc);
        if let (Some(hwnd), Some(p)) = (hwnd, engine.initial_widget_pos) {
            let pos = egui::pos2(p.x, p.y);
            if monitor::is_on_screen(egui::Rect::from_min_size(
                pos,
                widget::size(widget_size.scale()),
            )) {
                monitor::move_window(hwnd, pos);
            }
        }
        let tray = Tray::new(
            autostart,
            theme_mode,
            widget_size,
            cc.egui_ctx.clone(),
            hwnd,
        );

        match (&tray, hwnd) {
            (Some(tray), Some(hwnd)) => tray.attach_to(hwnd),
            _ => eprintln!(
                "sundial: no Win32 window handle; the widget's right-click menu is unavailable"
            ),
        }

        Self {
            engine,
            tray,
            panel_open: false,
            closed_at: None,
            widget_visible,
            autostart,
            theme_mode,
            widget_size,
            applied: None,
            keep_on_screen_until: Some(Instant::now() + KEEP_ON_SCREEN_FOR),
            panel_focused_once: false,
            last_pos: None,
            hwnd,
        }
    }

    fn set_autostart(&mut self, enabled: bool) {
        self.autostart = enabled;
        let _ = self.engine.commands.send(Command::SetAutostart(enabled));
        if let Some(tray) = &self.tray {
            tray.autostart_item.set_checked(enabled);
        }
    }

    fn set_theme(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        let _ = self.engine.commands.send(Command::SetTheme(mode));
        self.show_appearance();
    }

    fn set_widget_size(&mut self, ctx: &egui::Context, size: WidgetSize) {
        self.widget_size = size;
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(widget::size(size.scale())));
        self.keep_on_screen_until = Some(Instant::now() + KEEP_ON_SCREEN_FOR);
        let _ = self.engine.commands.send(Command::SetWidgetSize(size));
        self.show_appearance();
    }

    fn show_appearance(&self) {
        if let Some(tray) = &self.tray {
            tray.show_appearance(self.theme_mode, self.widget_size);
        }
    }

    /// Resolves the palette every frame, so "Match Windows" follows a theme
    /// change made while Sundial is running.
    fn palette(&mut self, ctx: &egui::Context) -> theme::Palette {
        let p = *theme::resolve(self.theme_mode, ctx.system_theme());
        if self.applied != Some(p) {
            theme::apply(ctx, &p);
            self.applied = Some(p);
        }
        p
    }

    fn close_panel(&mut self) {
        self.panel_open = false;
        self.closed_at = Some(Instant::now());
    }

    fn open_panel(&mut self) {
        self.panel_open = true;
        self.panel_focused_once = false;
    }

    fn toggle_panel(&mut self) {
        if self.panel_open {
            self.close_panel();
        } else if self.closed_at.is_none_or(|t| t.elapsed() >= REOPEN_GUARD) {
            self.open_panel();
        }
    }

    fn set_widget_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.widget_visible = visible;
        ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
        let _ = self
            .engine
            .commands
            .send(Command::SetWidgetVisible(visible));
    }

    /// The engine saves and ends the process. The tray icon is dropped first,
    /// on this thread: exiting skips destructors, and an icon left registered
    /// lingers in the notification area until hovered.
    fn quit(&mut self) {
        self.tray = None;
        let _ = self.engine.commands.send(Command::Quit);
    }
}

impl eframe::App for SundialApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The clock must keep ticking visually even when nothing is interacted with.
        ctx.request_repaint_after(Duration::from_millis(500));

        let tray_actions = self
            .tray
            .as_ref()
            .map(|t| t.take_actions())
            .unwrap_or_default();
        for action in tray_actions {
            match action {
                TrayAction::ToggleWidget => {
                    let next = !self.widget_visible;
                    self.set_widget_visible(ctx, next);
                }
                TrayAction::ShowAnalytics => self.open_panel(),
                TrayAction::ToggleAutostart => self.set_autostart(!self.autostart),
                TrayAction::SetTheme(mode) => self.set_theme(mode),
                TrayAction::SetWidgetSize(size) => self.set_widget_size(ctx, size),
                TrayAction::Quit => self.quit(),
            }
        }

        // The tray pump shows a hidden window to get this frame to run at all.
        // The analytics panel is drawn from inside this window's frame, so the
        // window must stay up while the panel is open; otherwise put it back.
        // The pump showed it via Win32, so winit still thinks it is hidden and
        // would ignore `Visible(false)`, leaving a frozen widget on screen that
        // no longer runs `update`; hide it the same way it was shown.
        if !self.widget_visible && !self.panel_open {
            match self.hwnd {
                Some(hwnd) => monitor::hide_window(hwnd),
                None => ctx.send_viewport_cmd(ViewportCommand::Visible(false)),
            }
        }

        let palette = self.palette(ctx);
        let scale = self.widget_size.scale();
        // Copy only what the widget draws; the panel's full summary (which
        // includes every day of history) is cloned only while it is open.
        let (today, state) = {
            let snap = self.engine.snapshot.lock().unwrap();
            (snap.summary.today, snap.state)
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                match widget::show(ui, today, state, &palette, scale) {
                    widget::WidgetAction::StartDrag => {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag)
                    }
                    widget::WidgetAction::TogglePanel => self.toggle_panel(),
                    widget::WidgetAction::ShowMenu => {
                        if let (Some(tray), Some(hwnd)) = (&self.tray, self.hwnd) {
                            tray.show_context_menu(hwnd);
                        }
                    }
                    widget::WidgetAction::None => {}
                }
            });

        if let (Some(until), Some(hwnd)) = (self.keep_on_screen_until, self.hwnd) {
            if Instant::now() < until {
                monitor::keep_on_work_area(hwnd);
            } else {
                self.keep_on_screen_until = None;
            }
        }

        // Persist the widget position whenever the OS drag finishes, in
        // physical pixels - see `monitor::window_rect`.
        if let Some(rect) = self.hwnd.and_then(monitor::window_rect) {
            let pos = rect.min;
            if self.last_pos.is_none_or(|p| p != pos) {
                self.last_pos = Some(pos);
                let _ = self
                    .engine
                    .commands
                    .send(Command::SetWidgetPos(pos.x, pos.y));
            }
        }

        if self.panel_open {
            self.show_panel(ctx, &palette);
        }
    }
}

impl SundialApp {
    fn show_panel(&mut self, ctx: &egui::Context, palette: &theme::Palette) {
        let snap = self.engine.snapshot.lock().unwrap().clone();
        let widget_rect =
            self.hwnd
                .and_then(monitor::window_rect)
                .unwrap_or(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    widget::size(self.widget_size.scale()),
                ));
        // Physical pixels in, but egui scales `with_position` by this.
        let ppp = ctx.input(|i| i.viewport().native_pixels_per_point.unwrap_or(1.0));
        let pos = monitor::bottom_right_for(widget_rect, analytics::SIZE) / ppp;
        let today_index = chrono::Local::now()
            .date_naive()
            .weekday()
            .num_days_from_monday() as usize;

        let builder = egui::ViewportBuilder::default()
            .with_title(PANEL_TITLE)
            .with_inner_size(analytics::SIZE)
            .with_position([pos.x, pos.y])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_resizable(false)
            .with_taskbar(false)
            // Ask for focus, so the panel can take it and the focus-loss
            // dismissal below becomes meaningful rather than instant.
            .with_active(true);

        let mut escaped = false;
        let mut closed = false;
        let mut focused_now = false;

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("analytics"),
            builder,
            |ctx, _class| {
                // Painted over the whole window rather than as the panel's
                // frame: a frame wraps the content, and when the content runs a
                // little taller than the window its rounded bottom corners are
                // drawn off-screen, leaving square ones.
                ctx.layer_painter(egui::LayerId::background()).rect(
                    ctx.screen_rect(),
                    theme::RADIUS,
                    palette.bg,
                    egui::Stroke::new(1.0_f32, palette.edge),
                    egui::StrokeKind::Inside,
                );
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        analytics::show(ui, &snap.summary, snap.state, today_index, palette);
                    });

                escaped = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                closed = ctx.input(|i| i.viewport().close_requested());
                focused_now = ctx.input(|i| i.viewport().focused.unwrap_or(false));
            },
        );

        // egui sizes and positions new viewports using the primary monitor's
        // scale, so the position above is only a first guess whenever the
        // widget sits on a monitor with different scaling. Correct it from the
        // panel's real physical size once its window exists.
        if let Some(panel) = monitor::find_window(PANEL_TITLE) {
            if let Some(rect) = monitor::window_rect(panel) {
                let target = monitor::bottom_right_for(widget_rect, rect.size());
                if rect.min != target {
                    monitor::move_window(panel, target);
                }
            }
        }

        if focused_now {
            self.panel_focused_once = true;
        }

        // Only honour focus loss once the panel has actually held focus.
        let lost_focus = self.panel_focused_once && !focused_now;
        if escaped || closed || lost_focus {
            self.close_panel();
        }
    }
}
