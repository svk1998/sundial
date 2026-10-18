use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ToggleWidget,
    ShowAnalytics,
    ToggleAutostart,
    Quit,
}

/// The tray icon is the only way back once the widget is hidden, so it stays
/// present for the whole life of the process.
pub struct Tray {
    _icon: TrayIcon,
    menu: Menu,
    pending: Arc<Mutex<Vec<TrayAction>>>,
    pub autostart_item: CheckMenuItem,
}

fn load_icon() -> Option<tray_icon::Icon> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), w, h).ok()
}

struct Ids {
    toggle: MenuId,
    analytics: MenuId,
    autostart: MenuId,
    quit: MenuId,
}

impl Tray {
    pub fn new(autostart: bool, ctx: egui::Context) -> Option<Tray> {
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
            .with_menu(Box::new(menu.clone()))
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

        let ids = Ids {
            toggle: toggle.id().clone(),
            analytics: analytics.id().clone(),
            autostart: autostart_item.id().clone(),
            quit: quit.id().clone(),
        };

        let pending = Arc::new(Mutex::new(Vec::new()));
        spawn_pump(ids, Arc::clone(&pending), ctx);

        Some(Tray {
            _icon: icon,
            menu,
            pending,
            autostart_item,
        })
    }

    /// Takes everything the pump has queued since the last call.
    pub fn take_actions(&self) -> Vec<TrayAction> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }

    /// Subclasses the widget's window so the popup's `WM_COMMAND` is translated
    /// into a `MenuEvent`. Without this the menu opens and draws correctly, but
    /// the command goes to winit's window procedure and is silently dropped -
    /// every item appears dead. Call once, before `show_context_menu`.
    pub fn attach_to(&self, hwnd: isize) {
        unsafe {
            use tray_icon::menu::ContextMenu;
            self.menu.attach_menu_subclass_for_hwnd(hwnd);
        }
    }

    /// Pops the same menu as a native Win32 context menu at the cursor.
    ///
    /// The widget cannot use an egui context menu: egui draws menus inside the
    /// window that owns them, and the widget's window is 140x40 px, so the menu
    /// would be clipped to nothing. A native popup is not bounded by the window.
    /// Selections arrive through the same global `MenuEvent` channel the tray
    /// pump already drains, so there is one menu definition and one handler.
    pub fn show_context_menu(&self, hwnd: isize) {
        use tray_icon::menu::ContextMenu;
        unsafe {
            self.menu.show_context_menu_for_hwnd(hwnd, None);
        }
    }
}

/// Tray events are pumped on their own thread rather than from the egui update
/// loop. A hidden widget never repaints, so polling from `update` would stop the
/// moment the widget was hidden - leaving the tray icon, the only way to bring it
/// back, permanently dead.
fn spawn_pump(ids: Ids, pending: Arc<Mutex<Vec<TrayAction>>>, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("sundial-tray".into())
        .spawn(move || {
            loop {
            let mut queued = Vec::new();

            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                // Left click toggles. A double click arrives as two Up events and
                // therefore toggles twice, which is the honest interpretation.
                if let TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = event
                {
                    queued.push(TrayAction::ToggleWidget);
                }
            }

            while let Ok(MenuEvent { id }) = MenuEvent::receiver().try_recv() {
                if id == ids.toggle {
                    queued.push(TrayAction::ToggleWidget);
                } else if id == ids.analytics {
                    queued.push(TrayAction::ShowAnalytics);
                } else if id == ids.autostart {
                    queued.push(TrayAction::ToggleAutostart);
                } else if id == ids.quit {
                    queued.push(TrayAction::Quit);
                }
            }

            if !queued.is_empty() {
                pending.lock().unwrap().extend(queued);
                // Wake the event loop: it may be idle with nothing to draw.
                ctx.request_repaint();
            }

                std::thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("spawn tray pump thread");
}
