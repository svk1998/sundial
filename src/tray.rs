use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    None,
    ToggleWidget,
    ShowAnalytics,
    ToggleAutostart,
    Quit,
}

/// The tray icon is the only way back once the widget is hidden, so it stays
/// present for the whole life of the process.
pub struct Tray {
    _icon: TrayIcon,
    toggle_id: MenuId,
    analytics_id: MenuId,
    autostart_id: MenuId,
    quit_id: MenuId,
    pub autostart_item: CheckMenuItem,
}

fn load_icon() -> Option<tray_icon::Icon> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), w, h).ok()
}

impl Tray {
    pub fn new(autostart: bool) -> Option<Tray> {
        let toggle = MenuItem::new("Show/Hide widget", true, None);
        let analytics = MenuItem::new("Analytics", true, None);
        let autostart_item = CheckMenuItem::new("Start with Windows", true, autostart, None);
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append(&toggle).ok()?;
        menu.append(&analytics).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&autostart_item).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&quit).ok()?;

        let icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Sundial")
            .with_icon(load_icon()?)
            .build()
        {
            Ok(i) => i,
            Err(e) => {
                eprintln!("sundial: tray icon unavailable: {e}");
                return None;
            }
        };

        Some(Tray {
            _icon: icon,
            toggle_id: toggle.id().clone(),
            analytics_id: analytics.id().clone(),
            autostart_id: autostart_item.id().clone(),
            quit_id: quit.id().clone(),
            autostart_item,
        })
    }

    /// Drains tray and menu events. Call once per frame.
    pub fn poll(&self) -> TrayAction {
        let mut action = TrayAction::None;

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                action = TrayAction::ToggleWidget;
            }
        }

        while let Ok(MenuEvent { id }) = MenuEvent::receiver().try_recv() {
            action = if id == self.toggle_id {
                TrayAction::ToggleWidget
            } else if id == self.analytics_id {
                TrayAction::ShowAnalytics
            } else if id == self.autostart_id {
                TrayAction::ToggleAutostart
            } else if id == self.quit_id {
                TrayAction::Quit
            } else {
                action
            };
        }

        action
    }
}
