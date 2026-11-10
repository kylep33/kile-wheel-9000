//! Desktop coordinates, in the units the OS positions windows in: physical pixels on
//! Windows, points on macOS, stage pixels on GNOME.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn center(self) -> ScreenPoint {
        ScreenPoint {
            x: self.left + self.width() / 2,
            y: self.top + self.height() / 2,
        }
    }

    pub fn contains(self, point: ScreenPoint) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    pub fn covers(self, other: ScreenRect) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn the_centre_is_halfway_across_and_down() {
        assert_eq!(SCREEN.center(), ScreenPoint { x: 960, y: 540 });
    }

    #[test]
    fn a_rect_covers_itself_but_not_a_bigger_one() {
        let bigger = ScreenRect {
            right: 2000,
            ..SCREEN
        };

        assert!(SCREEN.covers(SCREEN));
        assert!(!SCREEN.covers(bigger));
        assert!(bigger.covers(SCREEN));
    }
}
