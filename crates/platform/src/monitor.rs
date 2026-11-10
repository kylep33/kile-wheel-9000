//! Monitors, the cursor, and the fullscreen check that suppresses hotkeys.

use crate::geom::{ScreenPoint, ScreenRect};
use crate::sys;

/// Which monitor the wheel opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorChoice {
    UnderCursor,
    Primary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub bounds: ScreenRect,
    /// The bounds less the taskbar, dock or top bar, which is the part a panel can be
    /// read in.
    pub work: ScreenRect,
    /// The desktop's scaling setting for this monitor, 1.0 at 100%. Sizes in window
    /// units are multiplied by it.
    pub ui_scale: f32,
    /// Buffer pixels per window unit: 1 where windows are placed in physical pixels, 2 on
    /// a Retina display where they are placed in points.
    pub pixel_ratio: f32,
}

impl Monitor {
    pub fn center(self) -> ScreenPoint {
        self.bounds.center()
    }
}

/// The monitor the wheel should open on, falling back to the primary one when the
/// cursor cannot be read.
pub fn monitor_for(choice: MonitorChoice) -> Monitor {
    sys::monitor::monitor_for(choice)
}

pub fn cursor_position() -> Option<ScreenPoint> {
    sys::monitor::cursor_position()
}

/// The move is not reported back as input, so the selection does not jump.
pub fn move_cursor(to: ScreenPoint) {
    sys::monitor::move_cursor(to);
}

/// Whether the focused window covers its whole monitor, which is how games and video
/// present.
pub fn foreground_is_fullscreen() -> bool {
    sys::monitor::foreground_is_fullscreen()
}
