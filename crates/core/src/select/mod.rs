//! How a slot gets selected, by direction key or by cursor angle.

pub mod angle;
pub mod held;

pub use angle::{Offset, SECTOR_RAD, slot_at};
pub use held::Held;
