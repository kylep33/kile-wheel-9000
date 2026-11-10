//! Screens, the pointer and fullscreen. AppKit measures from the bottom left of the main
//! screen, the event system from the top left, so everything here is turned top-left.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWorkspace};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGAssociateMouseAndMouseCursorPosition, CGEvent, CGWarpMouseCursorPosition};
use objc2_foundation::NSRect;
use tracing::warn;

use super::ax;
use crate::geom::{ScreenPoint, ScreenRect};
use crate::monitor::{Monitor, MonitorChoice};

/// Windows are placed in points, so sizes need no UI scale of their own.
const POINTS: f32 = 1.0;

/// Only used off the main thread, which AppKit does not answer on.
const NOMINAL: Monitor = Monitor {
    bounds: ScreenRect {
        left: 0,
        top: 0,
        right: 1440,
        bottom: 900,
    },
    work: ScreenRect {
        left: 0,
        top: 0,
        right: 1440,
        bottom: 900,
    },
    ui_scale: POINTS,
    pixel_ratio: 2.0,
};

pub fn monitor_for(choice: MonitorChoice) -> Monitor {
    let Some(mtm) = MainThreadMarker::new() else {
        warn!("screens can only be read on the main thread, so the wheel uses a nominal one");
        return NOMINAL;
    };
    let screens: Vec<Monitor> = NSScreen::screens(mtm)
        .iter()
        .map(|screen| describe(&screen, primary_height(mtm)))
        .collect();

    let wanted = match choice {
        MonitorChoice::UnderCursor => cursor_position()
            .and_then(|cursor| screens.iter().find(|screen| screen.bounds.contains(cursor))),
        MonitorChoice::Primary => None,
    };
    wanted.or(screens.first()).copied().unwrap_or(NOMINAL)
}

pub fn cursor_position() -> Option<ScreenPoint> {
    let event = CGEvent::new(None)?;
    let at = CGEvent::location(Some(&event));
    Some(ScreenPoint {
        x: at.x as i32,
        y: at.y as i32,
    })
}

pub fn move_cursor(to: ScreenPoint) {
    let _ = CGWarpMouseCursorPosition(CGPoint::new(f64::from(to.x), f64::from(to.y)));
    // a warp leaves the pointer frozen for a moment unless it is reattached
    let _ = CGAssociateMouseAndMouseCursorPosition(true);
}

pub fn foreground_is_fullscreen() -> bool {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return false;
    };
    ax::element(&ax::application(app.processIdentifier()), ax::FOCUSED_WINDOW)
        .and_then(|window| ax::boolean(&window, ax::FULL_SCREEN))
        .unwrap_or(false)
}

/// The main screen is first, and its frame is the one AppKit's origin hangs off.
pub fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|screen| screen.frame().size.height)
        .unwrap_or(0.0)
}

fn describe(screen: &NSScreen, primary_height: f64) -> Monitor {
    Monitor {
        bounds: top_left(screen.frame(), primary_height),
        work: top_left(screen.visibleFrame(), primary_height),
        ui_scale: POINTS,
        pixel_ratio: screen.backingScaleFactor() as f32,
    }
}

fn top_left(rect: NSRect, primary_height: f64) -> ScreenRect {
    let top = primary_height - (rect.origin.y + rect.size.height);
    ScreenRect {
        left: rect.origin.x.round() as i32,
        top: top.round() as i32,
        right: (rect.origin.x + rect.size.width).round() as i32,
        bottom: (top + rect.size.height).round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSPoint, NSSize};

    #[test]
    fn a_screen_above_the_main_one_has_a_negative_top() {
        let above = NSRect::new(NSPoint::new(0.0, 900.0), NSSize::new(1440.0, 900.0));

        assert_eq!(top_left(above, 900.0).top, -900);
    }
}
