#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod platform;
mod stats;
mod store;
mod tracker;
mod tray;
mod ui;

fn main() -> eframe::Result<()> {
    // Two copies in one session would double-count the user's time.
    let Some(_guard) = platform::single_instance::acquire() else {
        return Ok(());
    };

    let engine = engine::start();
    let visible = engine.settings.lock().unwrap().widget_visible;

    // No `with_position`: egui scales it by the primary monitor's DPI, which
    // misplaces a widget saved on a monitor with different scaling. The app
    // restores the saved position itself, in physical pixels.
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(ui::widget::SIZE)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_resizable(false)
        .with_taskbar(false)
        .with_visible(visible);

    eframe::run_native(
        "Sundial",
        eframe::NativeOptions {
            viewport,
            // Must be set explicitly: `default-features = false` compiles out the
            // default renderer, and the wgpu backend exits the process silently
            // on this target before the app is ever created.
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(app::SundialApp::new(cc, engine)))),
    )
}
