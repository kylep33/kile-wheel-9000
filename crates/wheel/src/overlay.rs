//! The wheel's window: the shared renderer drawing into the platform's overlay surface.
//!
//! The window is a square sized for the furthest a petal can reach, centred on the
//! monitor. Only the part a frame actually drew into is sent to the screen.

use std::cell::RefCell;

use anyhow::{Context, Result};
use kile_wheel_core::config::Config;
use kile_wheel_platform::geom::{ScreenPoint, ScreenRect};
use kile_wheel_platform::icons::IconImage;
use kile_wheel_platform::monitor::Monitor;
use kile_wheel_platform::overlay::{FramePixels, OverlaySurface};

use crate::geom::{Point, Rect};
use crate::render::{FormView, Frame, Region, Renderer};
use crate::theme::{Metrics, Palette};

/// The window, the renderer behind it, and where it sits on screen.
///
/// Drawing borrows the frame's text straight out of the app, so `draw` takes `&self` and
/// the mutable parts sit behind `RefCell`s rather than forcing the app to copy every label.
#[derive(Debug)]
pub struct Overlay {
    surface: RefCell<OverlaySurface>,
    renderer: RefCell<Renderer>,
    /// Screen position of the wheel centre, which is what cursor angles are measured from.
    centre: ScreenPoint,
    /// Screen position of the window's top-left corner.
    origin: ScreenPoint,
}

impl Overlay {
    /// Created hidden, so the first open only has to draw and show it.
    pub fn create(monitor: Monitor, config: &Config) -> Result<Self> {
        let metrics = metrics_for(monitor, config);
        let surface = OverlaySurface::create().context("could not create the overlay window")?;
        let renderer = Renderer::new(
            metrics.window_size_px() as f32,
            monitor.pixel_ratio,
            metrics,
            palette_for(config),
        )?;

        let mut overlay = Self {
            surface: RefCell::new(surface),
            renderer: RefCell::new(renderer),
            centre: ScreenPoint::default(),
            origin: ScreenPoint::default(),
        };
        overlay.place_on(monitor, config)?;
        Ok(overlay)
    }

    pub fn metrics(&self) -> Metrics {
        self.renderer.borrow().metrics()
    }

    pub fn centre(&self) -> ScreenPoint {
        self.centre
    }

    pub fn has_icon(&self, key: &str) -> bool {
        self.renderer.borrow().has_icon(key)
    }

    pub fn upload_icon(&mut self, key: &str, image: &IconImage) -> Result<()> {
        self.renderer.get_mut().upload_icon(key, image)
    }

    /// Centres the window on the monitor and matches the buffer to its scale.
    pub fn place_on(&mut self, monitor: Monitor, config: &Config) -> Result<()> {
        let metrics = metrics_for(monitor, config);
        let size = metrics.window_size_px();
        let centre = monitor.center();
        let origin = ScreenPoint {
            x: centre.x - size / 2,
            y: centre.y - size / 2,
        };

        let renderer = self.renderer.get_mut();
        renderer
            .resize(size as f32, monitor.pixel_ratio, metrics, palette_for(config))
            .context("could not resize the overlay buffer for the target monitor")?;

        // the window is sized for the furthest a petal can reach, which on a short monitor
        // runs off its top and bottom; petals are kept to the part you can actually see
        renderer.set_visible(visible_in_window(monitor.work, origin));

        self.centre = centre;
        self.origin = origin;
        Ok(())
    }

    pub fn draw(&self, frame: &Frame<'_>) -> Result<()> {
        let mut renderer = self.renderer.borrow_mut();
        let pixel_ratio = renderer.pixel_ratio();
        let drawn = renderer.draw(frame);
        let region = aligned(drawn.region, pixel_ratio, drawn.pixmap.width(), drawn.pixmap.height());

        let pixels = FramePixels {
            rgba: drawn.pixmap.data(),
            buffer_width: drawn.pixmap.width(),
            region_x: region.x,
            region_y: region.y,
            region_width: region.width,
            region_height: region.height,
            origin: ScreenPoint {
                x: self.origin.x + (region.x as f32 / pixel_ratio).round() as i32,
                y: self.origin.y + (region.y as f32 / pixel_ratio).round() as i32,
            },
            pixel_ratio,
        };

        self.surface
            .borrow_mut()
            .present(&pixels)
            .context("could not put the wheel on screen")
    }

    /// Which form line a screen point lands on. The point is screen-relative, so it is
    /// turned into window coordinates here, where the window's position is known.
    pub fn form_row_at(&self, view: &FormView<'_>, screen_x: i32, screen_y: i32) -> Option<usize> {
        let point = Point {
            x: (screen_x - self.origin.x) as f32,
            y: (screen_y - self.origin.y) as f32,
        };

        self.renderer.borrow().form_row_at(view, point)
    }

    pub fn show(&self) {
        self.surface.borrow_mut().show();
    }

    pub fn hide(&self) {
        self.surface.borrow_mut().hide();
    }
}

/// Snaps the region's corner to a whole screen unit, so on a 2x display the frame never
/// lands half a point off.
fn aligned(region: Region, pixel_ratio: f32, buffer_width: u32, buffer_height: u32) -> Region {
    let step = pixel_ratio.round().max(1.0) as u32;
    let x = region.x - region.x % step;
    let y = region.y - region.y % step;

    Region {
        x,
        y,
        width: (region.width + region.x - x).min(buffer_width - x),
        height: (region.height + region.y - y).min(buffer_height - y),
    }
}

/// The monitor's work area in window coordinates, given where the window's corner sits on
/// screen.
fn visible_in_window(work: ScreenRect, window_origin: ScreenPoint) -> Rect {
    Rect {
        left: (work.left - window_origin.x) as f32,
        top: (work.top - window_origin.y) as f32,
        right: (work.right - window_origin.x) as f32,
        bottom: (work.bottom - window_origin.y) as f32,
    }
}

fn metrics_for(monitor: Monitor, config: &Config) -> Metrics {
    Metrics::new(monitor.ui_scale, config.wheel.scale())
}

fn palette_for(config: &Config) -> Palette {
    Palette::new(config.wheel.accent(), config.wheel.opacity())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible_for(work: ScreenRect, centre: ScreenPoint, size: i32) -> Rect {
        visible_in_window(
            work,
            ScreenPoint {
                x: centre.x - size / 2,
                y: centre.y - size / 2,
            },
        )
    }

    /// The case that put petals off screen: a 1920x1200 monitor at 125% with the taskbar
    /// along the bottom, and a window taller than the monitor.
    #[test]
    fn a_window_taller_than_the_monitor_is_trimmed_to_the_work_area() {
        let work = ScreenRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1140,
        };
        let visible = visible_for(work, ScreenPoint { x: 960, y: 600 }, 1690);

        // the window's top edge is 245 px above the screen, so the screen starts that far in
        assert_eq!(visible.top, 245.0);
        // and the taskbar's top edge is 1140 px down the screen
        assert_eq!(visible.bottom, 245.0 + 1140.0);
        assert_eq!(visible.left, -(960.0 - 845.0));
    }

    #[test]
    fn a_window_that_fits_is_wholly_visible() {
        let work = ScreenRect {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        };
        let visible = visible_for(work, ScreenPoint { x: 1920, y: 1080 }, 1000);

        assert!(visible.left <= 0.0 && visible.top <= 0.0);
        assert!(visible.right >= 1000.0 && visible.bottom >= 1000.0);
    }

    #[test]
    fn a_region_on_a_retina_display_starts_on_a_whole_point() {
        let region = Region {
            x: 101,
            y: 51,
            width: 10,
            height: 10,
        };

        assert_eq!(
            aligned(region, 2.0, 200, 200),
            Region {
                x: 100,
                y: 50,
                width: 11,
                height: 11
            }
        );
    }

    #[test]
    fn a_region_at_one_to_one_is_left_alone() {
        let region = Region {
            x: 7,
            y: 3,
            width: 5,
            height: 5,
        };

        assert_eq!(aligned(region, 1.0, 100, 100), region);
    }
}
