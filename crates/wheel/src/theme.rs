//! Sizes and colours for the wheel. One place to retune the look.
//!
//! Every radius is a fraction of the wheel radius, so the `size` setting and the
//! monitor DPI both scale the whole thing by one multiplier.

use kile_wheel_core::config::Rgb;
use kile_wheel_platform::monitor::USER_DEFAULT_DPI;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// Wheel diameter at 100% scale on a 96 dpi monitor.
const BASE_DIAMETER_PX: f32 = 620.0;

/// Room around the wheel for the ring stroke and the selected wedge's glow.
const WINDOW_MARGIN_PX: f32 = 12.0;

const HUB_RADIUS_RATIO: f32 = 0.185;
const NUMBER_RADIUS_RATIO: f32 = 0.255;
const ICON_RADIUS_RATIO: f32 = 0.455;
const LABEL_RADIUS_RATIO: f32 = 0.635;
const LABEL_BOX_RATIO: f32 = 0.52;
const ICON_SIZE_RATIO: f32 = 0.105;

const RING_STROKE_PX: f32 = 1.6;
const DIVIDER_STROKE_PX: f32 = 1.2;
const HUB_STROKE_PX: f32 = 1.6;
const SELECTED_STROKE_PX: f32 = 2.0;
const EDIT_STROKE_PX: f32 = 1.6;

const LABEL_SIZE_PX: f32 = 15.0;
const NUMBER_SIZE_PX: f32 = 12.0;
const HUB_TAB_SIZE_PX: f32 = 13.0;
const HUB_CLOCK_SIZE_PX: f32 = 26.0;
const HUB_DATE_SIZE_PX: f32 = 11.0;
const COG_SIZE_PX: f32 = 15.0;

/// Positions inside the hub, as a fraction of the hub radius from the wheel centre.
const TAB_Y_RATIO: f32 = -0.52;
const DATE_Y_RATIO: f32 = 0.30;
const COG_Y_RATIO: f32 = 0.66;
const COG_HIT_RATIO: f32 = 1.35;

const FULL_CHANNEL: f32 = 255.0;

pub const LABEL_FONT: &str = "Segoe UI";
pub const ICON_FONT: &str = "Segoe MDL2 Assets";
pub const LOCALE: &str = "en-us";

/// Settings gear in Segoe MDL2 Assets.
pub const COG_GLYPH: &str = "\u{E713}";

/// Pin glyph for a slot that is pinned rather than merely running.
pub const PIN_GLYPH: &str = "\u{E718}";

/// Shown on the hub while edit mode is on.
pub const EDIT_LABEL: &str = "EDIT";

/// Theme sizes resolved for one monitor and one size setting.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub radius_px: f32,
    pub hub_radius_px: f32,
    pub number_radius_px: f32,
    pub icon_radius_px: f32,
    pub label_radius_px: f32,
    pub label_box_px: f32,
    pub icon_size_px: f32,
    pub ring_stroke_px: f32,
    pub divider_stroke_px: f32,
    pub hub_stroke_px: f32,
    pub selected_stroke_px: f32,
    pub edit_stroke_px: f32,
    pub label_size_px: f32,
    pub number_size_px: f32,
    pub hub_tab_size_px: f32,
    pub hub_clock_size_px: f32,
    pub hub_date_size_px: f32,
    pub cog_size_px: f32,
}

impl Metrics {
    pub fn new(dpi: u32, size_scale: f32) -> Self {
        let scale = (dpi as f32 / USER_DEFAULT_DPI as f32) * size_scale;
        let radius_px = BASE_DIAMETER_PX * scale / 2.0;

        Self {
            radius_px,
            hub_radius_px: radius_px * HUB_RADIUS_RATIO,
            number_radius_px: radius_px * NUMBER_RADIUS_RATIO,
            icon_radius_px: radius_px * ICON_RADIUS_RATIO,
            label_radius_px: radius_px * LABEL_RADIUS_RATIO,
            label_box_px: radius_px * LABEL_BOX_RATIO,
            icon_size_px: radius_px * ICON_SIZE_RATIO,
            ring_stroke_px: RING_STROKE_PX * scale,
            divider_stroke_px: DIVIDER_STROKE_PX * scale,
            hub_stroke_px: HUB_STROKE_PX * scale,
            selected_stroke_px: SELECTED_STROKE_PX * scale,
            edit_stroke_px: EDIT_STROKE_PX * scale,
            label_size_px: LABEL_SIZE_PX * scale,
            number_size_px: NUMBER_SIZE_PX * scale,
            hub_tab_size_px: HUB_TAB_SIZE_PX * scale,
            hub_clock_size_px: HUB_CLOCK_SIZE_PX * scale,
            hub_date_size_px: HUB_DATE_SIZE_PX * scale,
            cog_size_px: COG_SIZE_PX * scale,
        }
    }

    /// Square window side in pixels, leaving room for the outer stroke.
    pub fn window_size_px(self) -> i32 {
        ((self.radius_px + WINDOW_MARGIN_PX) * 2.0).ceil() as i32
    }

    /// Distance from the wheel centre at which selection starts, matching the hub.
    pub fn dead_zone_px(self) -> f32 {
        self.hub_radius_px
    }

    /// How far below the wheel centre the cog sits.
    pub fn cog_offset_px(self) -> f32 {
        self.hub_radius_px * COG_Y_RATIO
    }

    /// Click radius around the cog. Generous, because the hub takes no other clicks.
    pub fn cog_hit_radius_px(self) -> f32 {
        self.cog_size_px * COG_HIT_RATIO
    }

    pub fn hub_tab_offset_px(self) -> f32 {
        self.hub_radius_px * TAB_Y_RATIO
    }

    pub fn hub_date_offset_px(self) -> f32 {
        self.hub_radius_px * DATE_Y_RATIO
    }
}

/// Every colour the wheel draws with, tinted by the configured accent.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub backdrop: D2D1_COLOR_F,
    pub ring: D2D1_COLOR_F,
    pub divider: D2D1_COLOR_F,
    pub wedge: D2D1_COLOR_F,
    pub wedge_selected: D2D1_COLOR_F,
    pub hub: D2D1_COLOR_F,
    pub label: D2D1_COLOR_F,
    pub label_selected: D2D1_COLOR_F,
    pub label_empty: D2D1_COLOR_F,
    pub number: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub accent_dim: D2D1_COLOR_F,
    pub running: D2D1_COLOR_F,
    pub transparent: D2D1_COLOR_F,
}

impl Palette {
    pub fn new(accent: Rgb, opacity: f32) -> Self {
        let accent = from_rgb(accent, 1.0);

        Self {
            backdrop: grey(0x0C, 0.84),
            ring: grey(0x8A, 0.55),
            divider: grey(0x6E, 0.40),
            wedge: grey(0x16, 0.55),
            wedge_selected: grey(0x30, 0.80),
            hub: grey(0x12, 0.96),
            label: grey(0xB4, 0.95),
            label_selected: grey(0xFF, 1.0),
            label_empty: grey(0x55, 0.85),
            number: grey(0x78, 0.85),
            accent,
            accent_dim: fade(accent, 0.45),
            running: grey(0xE8, 0.95),
            transparent: grey(0x00, 0.0),
        }
        .with_opacity(opacity)
    }

    /// The window is already per-pixel alpha, so the opacity setting just scales every
    /// colour's alpha rather than needing a separate layer.
    fn with_opacity(mut self, opacity: f32) -> Self {
        for colour in self.each_mut() {
            colour.a *= opacity;
        }
        self.transparent.a = 0.0;
        self
    }

    fn each_mut(&mut self) -> [&mut D2D1_COLOR_F; 13] {
        [
            &mut self.backdrop,
            &mut self.ring,
            &mut self.divider,
            &mut self.wedge,
            &mut self.wedge_selected,
            &mut self.hub,
            &mut self.label,
            &mut self.label_selected,
            &mut self.label_empty,
            &mut self.number,
            &mut self.accent,
            &mut self.accent_dim,
            &mut self.running,
        ]
    }
}

fn grey(level: u8, alpha: f32) -> D2D1_COLOR_F {
    from_rgb(
        Rgb {
            red: level,
            green: level,
            blue: level,
        },
        alpha,
    )
}

fn from_rgb(colour: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(colour.red) / FULL_CHANNEL,
        g: f32::from(colour.green) / FULL_CHANNEL,
        b: f32::from(colour.blue) / FULL_CHANNEL,
        a: alpha,
    }
}

fn fade(colour: D2D1_COLOR_F, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: alpha, ..colour }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOUBLE_DPI: u32 = USER_DEFAULT_DPI * 2;

    #[test]
    fn doubling_the_dpi_doubles_every_size() {
        let normal = Metrics::new(USER_DEFAULT_DPI, 1.0);
        let doubled = Metrics::new(DOUBLE_DPI, 1.0);

        assert_eq!(doubled.radius_px, normal.radius_px * 2.0);
        assert_eq!(doubled.hub_radius_px, normal.hub_radius_px * 2.0);
        assert_eq!(doubled.label_size_px, normal.label_size_px * 2.0);
    }

    #[test]
    fn the_size_setting_scales_the_same_way_as_dpi() {
        let scaled = Metrics::new(USER_DEFAULT_DPI, 2.0);
        let doubled_dpi = Metrics::new(DOUBLE_DPI, 1.0);

        assert_eq!(scaled.radius_px, doubled_dpi.radius_px);
    }

    #[test]
    fn radii_stay_in_the_order_they_are_drawn() {
        let metrics = Metrics::new(USER_DEFAULT_DPI, 1.0);

        assert!(metrics.hub_radius_px < metrics.number_radius_px);
        assert!(metrics.number_radius_px < metrics.icon_radius_px);
        assert!(metrics.icon_radius_px < metrics.label_radius_px);
        assert!(metrics.label_radius_px < metrics.radius_px);
    }

    #[test]
    fn the_window_leaves_room_for_the_outer_stroke() {
        let metrics = Metrics::new(USER_DEFAULT_DPI, 1.0);

        assert!(metrics.window_size_px() as f32 > metrics.radius_px * 2.0);
    }

    #[test]
    fn the_dead_zone_matches_the_hub() {
        let metrics = Metrics::new(USER_DEFAULT_DPI, 1.0);

        assert_eq!(metrics.dead_zone_px(), metrics.hub_radius_px);
    }

    #[test]
    fn the_opacity_setting_scales_alpha_but_leaves_transparent_clear() {
        let accent = Rgb {
            red: 0xD8,
            green: 0xB3,
            blue: 0x6A,
        };
        let solid = Palette::new(accent, 1.0);
        let half = Palette::new(accent, 0.5);

        assert_eq!(half.backdrop.a, solid.backdrop.a * 0.5);
        assert_eq!(half.transparent.a, 0.0);
    }

    #[test]
    fn the_accent_colour_reaches_the_palette() {
        let accent = Rgb {
            red: 0xFF,
            green: 0x00,
            blue: 0x80,
        };
        let palette = Palette::new(accent, 1.0);

        assert_eq!(palette.accent.r, 1.0);
        assert_eq!(palette.accent.g, 0.0);
    }
}
