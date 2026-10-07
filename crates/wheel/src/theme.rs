//! Dark theme sizes and colours. One place to retune the look.

use kile_wheel_platform::monitor::USER_DEFAULT_DPI;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// Sizes below are written for 100% scaling and multiplied up from there.
const WHEEL_DIAMETER_PX: f32 = 560.0;
const HUB_RADIUS_PX: f32 = 82.0;
const SLOT_RING_RADIUS_PX: f32 = 192.0;
const SLOT_WIDTH_PX: f32 = 158.0;
const SLOT_HEIGHT_PX: f32 = 62.0;
const SLOT_CORNER_PX: f32 = 10.0;
const SLOT_BORDER_PX: f32 = 1.5;
const SLOT_LABEL_SIZE_PX: f32 = 14.0;
const SLOT_LABEL_INSET_PX: f32 = 10.0;
const HUB_LABEL_SIZE_PX: f32 = 22.0;

const FULL_CHANNEL: f32 = 255.0;

pub const FONT_FAMILY: &str = "Segoe UI";
pub const LOCALE: &str = "en-us";

pub const BACKDROP: D2D1_COLOR_F = rgba(0x10, 0x12, 0x16, 0.80);
pub const HUB_FILL: D2D1_COLOR_F = rgba(0x18, 0x1B, 0x21, 0.94);
pub const SLOT_FILL: D2D1_COLOR_F = rgba(0x1E, 0x22, 0x2A, 0.92);
pub const SLOT_BORDER: D2D1_COLOR_F = rgba(0x2E, 0x34, 0x40, 0.95);
pub const SLOT_FILL_SELECTED: D2D1_COLOR_F = rgba(0x2B, 0x6C, 0xB0, 0.95);
pub const SLOT_BORDER_SELECTED: D2D1_COLOR_F = rgba(0x5A, 0xA8, 0xF0, 1.0);
pub const LABEL: D2D1_COLOR_F = rgba(0xE4, 0xE8, 0xEF, 1.0);
pub const LABEL_EMPTY: D2D1_COLOR_F = rgba(0x6A, 0x72, 0x80, 1.0);
pub const TRANSPARENT: D2D1_COLOR_F = rgba(0x00, 0x00, 0x00, 0.0);

const fn rgba(red: u8, green: u8, blue: u8, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: red as f32 / FULL_CHANNEL,
        g: green as f32 / FULL_CHANNEL,
        b: blue as f32 / FULL_CHANNEL,
        a: alpha,
    }
}

/// Theme sizes scaled for one monitor's DPI.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub wheel_diameter_px: f32,
    pub hub_radius_px: f32,
    pub slot_ring_radius_px: f32,
    pub slot_width_px: f32,
    pub slot_height_px: f32,
    pub slot_corner_px: f32,
    pub slot_border_px: f32,
    pub slot_label_size_px: f32,
    pub slot_label_inset_px: f32,
    pub hub_label_size_px: f32,
}

impl Metrics {
    pub fn for_dpi(dpi: u32) -> Self {
        let scale = dpi as f32 / USER_DEFAULT_DPI as f32;

        Self {
            wheel_diameter_px: WHEEL_DIAMETER_PX * scale,
            hub_radius_px: HUB_RADIUS_PX * scale,
            slot_ring_radius_px: SLOT_RING_RADIUS_PX * scale,
            slot_width_px: SLOT_WIDTH_PX * scale,
            slot_height_px: SLOT_HEIGHT_PX * scale,
            slot_corner_px: SLOT_CORNER_PX * scale,
            slot_border_px: SLOT_BORDER_PX * scale,
            slot_label_size_px: SLOT_LABEL_SIZE_PX * scale,
            slot_label_inset_px: SLOT_LABEL_INSET_PX * scale,
            hub_label_size_px: HUB_LABEL_SIZE_PX * scale,
        }
    }

    /// Window side length in pixels. Square, so the wheel can sit anywhere in it.
    pub fn window_size_px(self) -> i32 {
        self.wheel_diameter_px.ceil() as i32
    }
}
