//! Plain 2D types for layout and drawing, in window units.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }

    pub fn is_empty(self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Right and bottom edges are exclusive, so neighbouring rows cannot both claim a click.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    /// The smallest rect covering both. An empty side is ignored, so a fresh union can
    /// start from `Rect::default()`.
    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Rect {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    pub fn intersect(self, other: Rect) -> Rect {
        Rect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    /// Grows the rect on every side, for strokes that reach past the shape they outline.
    pub fn outset(self, by: f32) -> Rect {
        Rect {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 10.0,
        bottom: 10.0,
    };

    #[test]
    fn the_far_edges_are_outside() {
        assert!(UNIT.contains(Point { x: 0.0, y: 0.0 }));
        assert!(!UNIT.contains(Point { x: 10.0, y: 5.0 }));
        assert!(!UNIT.contains(Point { x: 5.0, y: 10.0 }));
    }

    #[test]
    fn a_union_with_nothing_is_the_other_side() {
        assert_eq!(Rect::default().union(UNIT), UNIT);
        assert_eq!(UNIT.union(Rect::default()), UNIT);
    }

    #[test]
    fn a_union_covers_both() {
        let other = Rect {
            left: 5.0,
            top: -5.0,
            right: 20.0,
            bottom: 8.0,
        };

        assert_eq!(
            UNIT.union(other),
            Rect {
                left: 0.0,
                top: -5.0,
                right: 20.0,
                bottom: 10.0
            }
        );
    }
}
