use crate::engine::{Command, Engine};
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

/// Clicking the widget while the panel is focused fires both "panel lost focus"
/// and "widget clicked", which would close and immediately reopen it. Ignore a
/// toggle for this long after the panel closes.
const REOPEN_GUARD: Duration = Duration::from_millis(150);

pub struct SundialApp {
    engine: Engine,
    tray: Option<Tray>,
    panel_open: bool,
    closed_at: Option<Instant>,
    widget_visible: bool,
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
        theme::apply(&cc.egui_ctx);

        let (autostart, widget_visible) = {
            let s = engine.settings.lock().unwrap();
            (s.autostart, s.widget_visible)
        };

        // The tray must be built on the thread running the winit message loop.
        // It pumps its own events on a background thread and wakes us via the
        // context, so it keeps working while the widget is hidden.
        let tray = Tray::new(autostart, cc.egui_ctx.clone());

        let hwnd = window_hwnd(cc);
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
            panel_focused_once: false,
            last_pos: None,
            hwnd,
        }
    }

    fn set_autostart(&self, enabled: bool) {
        let _ = self.engine.commands.send(Command::SetAutostart(enabled));
        if let Some(tray) = &self.tray {
            tray.autostart_item.set_checked(enabled);
        }
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
        let _ = self.engine.commands.send(Command::SetWidgetVisible(visible));
    }

    fn quit(&self, ctx: &egui::Context) {
        let _ = self.engine.commands.send(Command::Quit);
        ctx.send_viewport_cmd(ViewportCommand::Close);
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
                TrayAction::ToggleAutostart => {
                    let now = self.engine.settings.lock().unwrap().autostart;
                    self.set_autostart(!now);
                }
                TrayAction::Quit => self.quit(ctx),
            }
        }

        let snap = self.engine.snapshot.lock().unwrap().clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| match widget::show(ui, &snap) {
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
            });

        // Persist the widget position whenever the OS drag finishes.
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
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
            self.show_panel(ctx, &snap);
        }
    }
}

impl SundialApp {
    fn show_panel(&mut self, ctx: &egui::Context, snap: &crate::engine::Snapshot) {
        let widget_rect = ctx
            .input(|i| i.viewport().outer_rect)
            .unwrap_or(egui::Rect::from_min_size(egui::Pos2::ZERO, widget::SIZE));
        let pos = crate::platform::monitor::bottom_right_for(widget_rect, analytics::SIZE);
        let today_index = chrono::Local::now()
            .date_naive()
            .weekday()
            .num_days_from_monday() as usize;

        let builder = egui::ViewportBuilder::default()
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
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::NONE
                            .fill(theme::BG)
                            .corner_radius(theme::RADIUS),
                    )
                    .show(ctx, |ui| {
                        analytics::show(ui, &snap.summary, snap.state, today_index);
                    });

                escaped = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                closed = ctx.input(|i| i.viewport().close_requested());
                focused_now = ctx.input(|i| i.viewport().focused.unwrap_or(false));
            },
        );

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
