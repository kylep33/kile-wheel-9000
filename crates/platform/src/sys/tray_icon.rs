//! The tray on Windows and macOS, through the `tray-icon` crate, which drives the
//! notification area and the menu bar the same way.
//!
//! Both need the main thread's event loop running, which `EventLoop::wait` provides.

use image::RgbaImage;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::event_loop::{Notice, Waker};
use crate::tray::{Command, MENU, TrayError, tooltip};

pub struct Tray {
    icon: TrayIcon,
    /// Kept so the toggle and the greyed items can change with the app's state.
    items: Vec<(Command, MenuItem)>,
    name: String,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Tray")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Tray {
    pub fn create(name: &str, icon: RgbaImage, waker: Waker) -> Result<Self, TrayError> {
        let (width, height) = icon.dimensions();
        let icon = Icon::from_rgba(icon.into_raw(), width, height)
            .map_err(|error| TrayError(format!("the tray icon image was rejected: {error}")))?;

        let menu = Menu::new();
        let mut items = Vec::new();
        for entry in MENU {
            let appended = match entry {
                Some(command) => {
                    let item = MenuItem::with_id(command.id(), command.label(true), true, None);
                    let appended = menu.append(&item);
                    items.push((command, item));
                    appended
                }
                None => menu.append(&PredefinedMenuItem::separator()),
            };
            appended.map_err(|error| TrayError(format!("could not build the tray menu: {error}")))?;
        }

        forward_events(waker);

        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(name)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| TrayError(format!("the desktop refused the tray icon: {error}")))?;

        Ok(Self {
            icon,
            items,
            name: name.to_owned(),
        })
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        for (command, item) in &self.items {
            item.set_text(command.label(enabled));
            item.set_enabled(enabled || !command.needs_enabled());
        }
        let _ = self.icon.set_tooltip(Some(tooltip(&self.name, enabled)));
    }
}

/// The crate's handlers are global, which is fine: there is one tray per process.
fn forward_events(waker: Waker) {
    let menu_waker = waker.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(command) = Command::from_id(event.id().as_ref()) {
            menu_waker.send(Notice::Tray(command));
        }
    }));

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            waker.send(Notice::Tray(Command::Settings));
        }
    }));
}
