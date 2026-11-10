//! A StatusNotifierItem, which Ubuntu's AppIndicator extension shows in the top bar.

use image::RgbaImage;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{MenuItem, StandardItem};

use crate::event_loop::{Notice, Waker};
use crate::tray::{Command, MENU, TrayError, tooltip};

/// The item's id on the bus, which the top bar uses to remember its position.
const ITEM_ID: &str = "kile-wheel-9000";

struct Model {
    name: String,
    enabled: bool,
    icon: ksni::Icon,
    waker: Waker,
}

impl ksni::Tray for Model {
    fn id(&self) -> String {
        ITEM_ID.to_owned()
    }

    fn title(&self) -> String {
        self.name.clone()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: tooltip(&self.name, self.enabled),
            ..ksni::ToolTip::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.waker.send(Notice::Tray(Command::Settings));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        MENU.into_iter()
            .map(|entry| match entry {
                Some(command) => StandardItem {
                    label: command.label(self.enabled).to_owned(),
                    enabled: self.enabled || !command.needs_enabled(),
                    activate: Box::new(move |model: &mut Self| {
                        model.waker.send(Notice::Tray(command));
                    }),
                    ..StandardItem::default()
                }
                .into(),
                None => MenuItem::Separator,
            })
            .collect()
    }
}

pub struct Tray {
    handle: Handle<Model>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tray").finish_non_exhaustive()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

impl Tray {
    pub fn create(name: &str, icon: RgbaImage, waker: Waker) -> Result<Self, TrayError> {
        let model = Model {
            name: name.to_owned(),
            enabled: true,
            icon: argb_icon(icon),
            waker,
        };
        let handle = model.spawn().map_err(|error| {
            TrayError(format!(
                "the top bar refused the tray icon: {error}; Ubuntu shows it through the AppIndicator extension, which has to be enabled"
            ))
        })?;
        Ok(Self { handle })
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.handle.update(|model| model.enabled = enabled);
    }
}

/// StatusNotifierItem pixmaps are ARGB in network byte order.
fn argb_icon(icon: RgbaImage) -> ksni::Icon {
    let (width, height) = icon.dimensions();
    let data = icon
        .pixels()
        .flat_map(|pixel| {
            let [red, green, blue, alpha] = pixel.0;
            [alpha, red, green, blue]
        })
        .collect();

    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_moves_to_the_front() {
        let icon = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]).expect("one pixel");

        assert_eq!(argb_icon(icon).data, [4, 1, 2, 3]);
    }
}
