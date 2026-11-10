//! Monitors, the pointer and fullscreen, as the shell sees them, in stage pixels.

use tracing::{debug, warn};

use super::shell;
use crate::geom::{ScreenPoint, ScreenRect};
use crate::monitor::{Monitor, MonitorChoice};

/// Bounds xywh, work area xywh, UI scale, primary.
type MonitorRow = (i32, i32, i32, i32, i32, i32, i32, i32, f64, bool);

/// The stage is laid out in the pixels the wheel draws in, so no further ratio applies.
const STAGE_PIXELS: f32 = 1.0;

/// Only used when the shell cannot be asked at all, so the wheel still has somewhere to
/// draw; the warning says why it looks wrong.
const NOMINAL: Monitor = Monitor {
    bounds: ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    },
    work: ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    },
    ui_scale: 1.0,
    pixel_ratio: STAGE_PIXELS,
};

pub fn monitor_for(choice: MonitorChoice) -> Monitor {
    let rows: Vec<MonitorRow> = match shell::call("Monitors", &()) {
        Ok(rows) => rows,
        Err(error) => {
            warn!("placing the wheel on a nominal 1080p screen: {error}");
            return NOMINAL;
        }
    };

    let wanted = match choice {
        MonitorChoice::UnderCursor => shell::call::<_, i32>("CurrentMonitor", &()).ok(),
        MonitorChoice::Primary => None,
    };
    let chosen = wanted
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| rows.get(index))
        .or_else(|| rows.iter().find(|row| row.9))
        .or_else(|| rows.first());

    chosen.map(|row| monitor_of(*row)).unwrap_or(NOMINAL)
}

pub fn cursor_position() -> Option<ScreenPoint> {
    let (x, y): (i32, i32) = shell::call("Pointer", &()).ok()?;
    Some(ScreenPoint { x, y })
}

pub fn move_cursor(to: ScreenPoint) {
    if let Err(error) = shell::call::<_, ()>("WarpPointer", &(to.x, to.y)) {
        debug!("could not centre the pointer: {error}");
    }
}

pub fn foreground_is_fullscreen() -> bool {
    shell::call("FocusIsFullscreen", &()).unwrap_or(false)
}

fn monitor_of((x, y, width, height, work_x, work_y, work_width, work_height, scale, _): MonitorRow) -> Monitor {
    Monitor {
        bounds: rect(x, y, width, height),
        work: rect(work_x, work_y, work_width, work_height),
        ui_scale: scale as f32,
        pixel_ratio: STAGE_PIXELS,
    }
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> ScreenRect {
    ScreenRect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_becomes_bounds_and_a_work_area() {
        let monitor = monitor_of((1920, 0, 2560, 1440, 1920, 32, 2560, 1408, 2.0, false));

        assert_eq!(monitor.bounds.right, 4480);
        assert_eq!(monitor.work.top, 32);
        assert_eq!(monitor.ui_scale, 2.0);
    }
}
