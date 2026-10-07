//! How a slot gets selected, by direction key or by cursor angle.

pub mod angle;
pub mod latch;

pub use angle::{Offset, slot_at};
pub use latch::Latch;
