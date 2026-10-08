//! Direct2D drawing composited by DirectComposition.
//!
//! A layered window would need a CPU bitmap blit every frame. DirectComposition
//! composites on the GPU and handles premultiplied alpha itself, so the overlay gets
//! real per-pixel transparency with no blit.

mod geometry;
mod icons;
mod text;

use std::collections::HashMap;

use kile_wheel_core::slot::Slot;
use kile_wheel_platform::icons::IconImage;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1CreateFactory, ID2D1Bitmap1,
    ID2D1DeviceContext, ID2D1Factory1, ID2D1Image, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2,
    IDXGISurface, IDXGISwapChain1,
};
use windows::core::{Interface, Result};
use windows_numerics::Vector2;

use crate::theme::{self, Metrics, Palette};
use geometry::{Wedges, on_circle, slot_bearing_rad, slot_edge_rad};
use text::Fonts;

/// Two buffers is the minimum the flip model accepts, and all the overlay needs.
const BUFFER_COUNT: u32 = 2;

/// Present on the next vblank. The open budget allows one frame.
const PRESENT_SYNC_INTERVAL: u32 = 1;

/// Keeps `ResizeBuffers` on the size the swap chain already has.
const KEEP_BUFFER_COUNT: u32 = 0;

/// Pill under a running app's icon, as a fraction of the wedge's label box.
const RUNNING_PILL_WIDTH_RATIO: f32 = 0.22;
const RUNNING_PILL_FOCUSED_RATIO: f32 = 0.42;
const RUNNING_PILL_HEIGHT_PX: f32 = 2.5;
const RUNNING_PILL_GAP_PX: f32 = 5.0;
const MULTI_WINDOW_PILL_GAP_PX: f32 = 3.0;

const NO_MULTISAMPLING: DXGI_SAMPLE_DESC = DXGI_SAMPLE_DESC {
    Count: 1,
    Quality: 0,
};

/// How a slot should be drawn.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlotView<'a> {
    pub label: &'a str,
    /// Cache key of an already uploaded icon.
    pub icon_key: Option<&'a str>,
    pub running: bool,
    pub focused: bool,
    pub window_count: usize,
    pub pinned: bool,
    pub filled: bool,
}

/// What the middle of the wheel shows.
#[derive(Debug, Default, Clone, Copy)]
pub struct HubView<'a> {
    pub tab_line: &'a str,
    pub clock: Option<&'a str>,
    pub date: Option<&'a str>,
}

/// Everything one frame needs.
#[derive(Debug)]
pub struct Frame<'a> {
    pub slots: [SlotView<'a>; Slot::COUNT],
    pub selected: Option<Slot>,
    pub hub: HubView<'a>,
    pub edit_mode: bool,
    pub dragging: Option<Slot>,
}

/// The GPU side of the overlay.
#[derive(Debug)]
pub struct Renderer {
    factory: ID2D1Factory1,
    context: ID2D1DeviceContext,
    swap_chain: IDXGISwapChain1,
    brush: ID2D1SolidColorBrush,
    fonts: Fonts,
    wedges: Wedges,
    icons: HashMap<String, ID2D1Bitmap1>,
    size_px: u32,
    metrics: Metrics,
    palette: Palette,
    /// Held so the composition tree stays alive for as long as the window does.
    _composition: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

impl Renderer {
    pub fn new(window: HWND, size_px: u32, metrics: Metrics, palette: Palette) -> Result<Self> {
        let d3d_device = create_d3d_device()?;
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;

        // SAFETY: a single-threaded factory matches this renderer living on one thread.
        let factory: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;

        // SAFETY: both devices are live for the rest of this function.
        let context = unsafe {
            let d2d_device = factory.CreateDevice(&dxgi_device)?;
            d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?
        };

        let swap_chain = create_swap_chain(&d3d_device, &dxgi_device, size_px)?;

        // SAFETY: the DXGI device is live, and the tree is kept alive by this struct.
        let (composition, target, visual) = unsafe {
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let target = composition.CreateTargetForHwnd(window, true)?;
            let visual = composition.CreateVisual()?;

            visual.SetContent(&swap_chain)?;
            target.SetRoot(&visual)?;
            composition.Commit()?;
            (composition, target, visual)
        };

        // SAFETY: the colour is copied; the brush is retargeted before every use.
        let brush = unsafe { context.CreateSolidColorBrush(&palette.label, None) }?;

        let wedges = Wedges::build(&factory, centre_of(size_px), metrics)?;

        Ok(Self {
            fonts: Fonts::new(metrics)?,
            factory,
            context,
            swap_chain,
            brush,
            wedges,
            icons: HashMap::new(),
            size_px,
            metrics,
            palette,
            _composition: composition,
            _target: target,
            _visual: visual,
        })
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    pub fn has_icon(&self, key: &str) -> bool {
        self.icons.contains_key(key)
    }

    /// Uploads an icon once. Kept on the GPU for the process lifetime, because there are
    /// only ever as many as there are pinned and running apps.
    pub fn upload_icon(&mut self, key: &str, image: &IconImage) -> Result<()> {
        let bitmap = icons::upload(&self.context, image)?;
        self.icons.insert(key.to_owned(), bitmap);
        Ok(())
    }

    /// Matches the swap chain and the wedge outlines to a new size, which happens when
    /// the wheel moves to a monitor with a different DPI.
    pub fn resize(&mut self, size_px: u32, metrics: Metrics, palette: Palette) -> Result<()> {
        self.palette = palette;
        if self.size_px == size_px {
            self.metrics = metrics;
            return Ok(());
        }

        // SAFETY: the target is unbound first, because ResizeBuffers needs every
        // reference to the old buffers released.
        unsafe {
            self.context.SetTarget(None);
            self.swap_chain.ResizeBuffers(
                KEEP_BUFFER_COUNT,
                size_px,
                size_px,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }

        self.size_px = size_px;
        self.metrics = metrics;
        self.fonts = Fonts::new(metrics)?;
        self.wedges = Wedges::build(&self.factory, centre_of(size_px), metrics)?;
        Ok(())
    }

    pub fn draw(&self, frame: &Frame<'_>) -> Result<()> {
        let surface = self.back_buffer()?;

        // SAFETY: the surface is bound, drawn to, then unbound before it is dropped.
        unsafe {
            self.context.SetTarget(Some(&surface));
            self.context.BeginDraw();
            self.context.Clear(Some(&self.palette.transparent));
        }

        self.draw_wheel(frame);

        // SAFETY: matches the BeginDraw above.
        unsafe {
            self.context.EndDraw(None, None)?;
            self.context.SetTarget(None);
            self.swap_chain
                .Present(PRESENT_SYNC_INTERVAL, DXGI_PRESENT(0))
                .ok()?;
        }
        Ok(())
    }

    fn draw_wheel(&self, frame: &Frame<'_>) {
        let centre = centre_of(self.size_px);

        self.fill_circle(centre, self.metrics.radius_px, &self.palette.backdrop);
        self.fill_wedges(frame);
        self.draw_dividers(centre);
        self.stroke_circle(
            centre,
            self.metrics.radius_px,
            &self.palette.ring,
            self.metrics.ring_stroke_px,
        );

        for slot in Slot::all() {
            self.draw_slot(centre, slot, frame);
        }

        self.draw_hub(centre, frame);
    }

    fn fill_wedges(&self, frame: &Frame<'_>) {
        for slot in Slot::all() {
            let Some(wedge) = self.wedges.get(slot) else {
                continue;
            };
            let selected = frame.selected == Some(slot);
            let fill = match selected {
                true => &self.palette.wedge_selected,
                false => &self.palette.wedge,
            };

            // SAFETY: the geometry and brush are live for the duration of the call.
            unsafe { self.context.FillGeometry(wedge, self.brush(fill), None) };

            if !selected && !frame.edit_mode {
                continue;
            }

            let outline = match (selected, frame.dragging == Some(slot)) {
                (_, true) => &self.palette.running,
                (true, false) => &self.palette.accent,
                (false, false) => &self.palette.accent_dim,
            };
            let stroke = match selected {
                true => self.metrics.selected_stroke_px,
                false => self.metrics.edit_stroke_px,
            };

            // SAFETY: as above.
            unsafe {
                self.context
                    .DrawGeometry(wedge, self.brush(outline), stroke, None)
            };
        }
    }

    fn draw_dividers(&self, centre: Vector2) {
        for slot in Slot::all() {
            let bearing = slot_edge_rad(slot);
            let from = on_circle(centre, self.metrics.hub_radius_px, bearing);
            let to = on_circle(centre, self.metrics.radius_px, bearing);

            // SAFETY: the brush is live for the duration of the call.
            unsafe {
                self.context.DrawLine(
                    from,
                    to,
                    self.brush(&self.palette.divider),
                    self.metrics.divider_stroke_px,
                    None,
                )
            };
        }
    }

    fn draw_slot(&self, centre: Vector2, slot: Slot, frame: &Frame<'_>) {
        let view = frame.slots[slot.index()];
        let selected = frame.selected == Some(slot);
        let bearing = slot_bearing_rad(slot);

        self.draw_slot_number(centre, slot, bearing, selected);

        if let Some(key) = view.icon_key {
            self.draw_icon(centre, bearing, key);
        }

        let label_colour = match (view.filled, selected) {
            (false, _) => &self.palette.label_empty,
            (true, true) => &self.palette.label_selected,
            (true, false) => &self.palette.label,
        };
        let label_centre = on_circle(centre, self.metrics.label_radius_px, bearing);
        self.draw_text(
            &view.label.to_uppercase(),
            box_around(
                label_centre,
                self.metrics.label_box_px,
                self.metrics.label_size_px * 3.0,
            ),
            self.fonts.label(),
            label_colour,
        );

        self.draw_indicators(centre, bearing, label_centre, view);
    }

    fn draw_slot_number(&self, centre: Vector2, slot: Slot, bearing: f32, selected: bool) {
        let colour = match selected {
            true => &self.palette.accent,
            false => &self.palette.number,
        };
        let at = on_circle(centre, self.metrics.number_radius_px, bearing);

        self.draw_text(
            &(slot.index() + 1).to_string(),
            box_around(
                at,
                self.metrics.number_size_px * 2.0,
                self.metrics.number_size_px * 2.0,
            ),
            self.fonts.number(),
            colour,
        );
    }

    fn draw_icon(&self, centre: Vector2, bearing: f32, key: &str) {
        let Some(bitmap) = self.icons.get(key) else {
            return;
        };

        let at = on_circle(centre, self.metrics.icon_radius_px, bearing);
        let half = self.metrics.icon_size_px / 2.0;
        let bounds = D2D_RECT_F {
            left: at.X - half,
            top: at.Y - half,
            right: at.X + half,
            bottom: at.Y + half,
        };

        // SAFETY: the bitmap and bounds are live for the duration of the call.
        unsafe {
            self.context.DrawBitmap(
                bitmap,
                Some(&bounds),
                1.0,
                D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
                None,
                None,
            )
        };
    }

    /// Mirrors the taskbar: a pill under a running app, a longer brighter one when it has
    /// focus, a second pill for more than one window, and an accent pin for a pinned slot.
    ///
    /// The pin sits on the wedge's own bisector between the number and the icon, so it
    /// cannot collide with a neighbour's label however long the names get.
    fn draw_indicators(
        &self,
        centre: Vector2,
        bearing: f32,
        label_centre: Vector2,
        view: SlotView<'_>,
    ) {
        if view.pinned {
            let at = on_circle(centre, self.metrics.pin_radius_px, bearing);
            let box_px = self.metrics.number_size_px * 2.0;
            self.draw_text(
                theme::PIN_GLYPH,
                box_around(at, box_px, box_px),
                self.fonts.glyph(),
                &self.palette.accent,
            );
        }

        if !view.running {
            return;
        }

        // the focused app gets the longer, brighter pill, as on the taskbar
        let (width, colour) = match view.focused {
            true => (
                self.metrics.label_box_px * RUNNING_PILL_FOCUSED_RATIO,
                &self.palette.accent,
            ),
            false => (
                self.metrics.label_box_px * RUNNING_PILL_WIDTH_RATIO,
                &self.palette.running,
            ),
        };
        let top = label_centre.Y + self.metrics.label_size_px + RUNNING_PILL_GAP_PX;
        self.fill_pill(label_centre.X, top, width, colour);

        if view.window_count > 1 {
            self.fill_pill(
                label_centre.X,
                top + RUNNING_PILL_HEIGHT_PX + MULTI_WINDOW_PILL_GAP_PX,
                self.metrics.label_box_px * RUNNING_PILL_WIDTH_RATIO,
                colour,
            );
        }
    }

    fn fill_pill(&self, centre_x: f32, top: f32, width: f32, colour: &D2D1_COLOR_F) {
        let bounds = D2D_RECT_F {
            left: centre_x - width / 2.0,
            top,
            right: centre_x + width / 2.0,
            bottom: top + RUNNING_PILL_HEIGHT_PX,
        };

        // SAFETY: the bounds and brush are live for the duration of the call.
        unsafe { self.context.FillRectangle(&bounds, self.brush(colour)) };
    }

    fn draw_hub(&self, centre: Vector2, frame: &Frame<'_>) {
        self.fill_circle(centre, self.metrics.hub_radius_px, &self.palette.hub);
        self.stroke_circle(
            centre,
            self.metrics.hub_radius_px,
            &self.palette.accent,
            self.metrics.hub_stroke_px,
        );

        let tab_line = match frame.edit_mode {
            true => theme::EDIT_LABEL,
            false => frame.hub.tab_line,
        };
        let tab_colour = match frame.edit_mode {
            true => &self.palette.accent,
            false => &self.palette.number,
        };
        self.draw_hub_line(
            centre,
            self.metrics.hub_tab_offset_px(),
            tab_line,
            self.fonts.hub_tab(),
            tab_colour,
        );

        if let Some(clock) = frame.hub.clock {
            self.draw_hub_line(
                centre,
                0.0,
                clock,
                self.fonts.hub_clock(),
                &self.palette.label_selected,
            );
        }
        if let Some(date) = frame.hub.date {
            self.draw_hub_line(
                centre,
                self.metrics.hub_date_offset_px(),
                date,
                self.fonts.hub_date(),
                &self.palette.number,
            );
        }

        self.draw_hub_line(
            centre,
            self.metrics.cog_offset_px(),
            theme::COG_GLYPH,
            self.fonts.cog(),
            &self.palette.accent_dim,
        );
    }

    fn draw_hub_line(
        &self,
        centre: Vector2,
        offset_y: f32,
        text: &str,
        format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
        colour: &D2D1_COLOR_F,
    ) {
        let at = Vector2 {
            X: centre.X,
            Y: centre.Y + offset_y,
        };
        let height = self.metrics.hub_radius_px;
        self.draw_text(
            text,
            box_around(at, self.metrics.hub_radius_px * 2.0, height),
            format,
            colour,
        );
    }

    fn draw_text(
        &self,
        text: &str,
        bounds: D2D_RECT_F,
        format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
        colour: &D2D1_COLOR_F,
    ) {
        let wide: Vec<u16> = text.encode_utf16().collect();

        // SAFETY: the buffer, format, bounds and brush all outlive the call.
        unsafe {
            self.context.DrawText(
                &wide,
                format,
                &bounds,
                self.brush(colour),
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
                windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn fill_circle(&self, centre: Vector2, radius_px: f32, colour: &D2D1_COLOR_F) {
        // SAFETY: the geometry and brush are live for the duration of the call.
        unsafe {
            self.context
                .FillEllipse(&circle(centre, radius_px), self.brush(colour))
        };
    }

    fn stroke_circle(
        &self,
        centre: Vector2,
        radius_px: f32,
        colour: &D2D1_COLOR_F,
        stroke_px: f32,
    ) {
        // SAFETY: the geometry and brush are live for the duration of the call.
        unsafe {
            self.context.DrawEllipse(
                &circle(centre, radius_px),
                self.brush(colour),
                stroke_px,
                None,
            )
        };
    }

    /// One brush is reused for everything, because creating a COM object per shape would
    /// allocate on every frame.
    fn brush(&self, colour: &D2D1_COLOR_F) -> &ID2D1SolidColorBrush {
        // SAFETY: setting a colour touches no pointer the caller owns.
        unsafe { self.brush.SetColor(colour) };
        &self.brush
    }

    /// Re-acquired every frame rather than cached, so a rotated flip-model buffer can
    /// never be drawn into by mistake.
    fn back_buffer(&self) -> Result<ID2D1Image> {
        // SAFETY: buffer 0 is the only one a flip-model chain exposes.
        let surface: IDXGISurface = unsafe { self.swap_chain.GetBuffer(0) }?;

        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };

        // SAFETY: the surface and properties are live for the duration of the call.
        let bitmap = unsafe {
            self.context
                .CreateBitmapFromDxgiSurface(&surface, Some(&properties))
        }?;

        bitmap.cast()
    }
}

fn centre_of(size_px: u32) -> Vector2 {
    let half = size_px as f32 / 2.0;
    Vector2 { X: half, Y: half }
}

fn circle(centre: Vector2, radius_px: f32) -> D2D1_ELLIPSE {
    D2D1_ELLIPSE {
        point: centre,
        radiusX: radius_px,
        radiusY: radius_px,
    }
}

fn box_around(centre: Vector2, width_px: f32, height_px: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: centre.X - width_px / 2.0,
        top: centre.Y - height_px / 2.0,
        right: centre.X + width_px / 2.0,
        bottom: centre.Y + height_px / 2.0,
    }
}

fn create_d3d_device() -> Result<ID3D11Device> {
    let mut device = None;

    // SAFETY: the out-pointer targets a live local, and BGRA support is what Direct2D
    // needs from the device.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            None,
        )
    }?;

    device.ok_or_else(windows::core::Error::from_thread)
}

fn create_swap_chain(
    d3d_device: &ID3D11Device,
    dxgi_device: &IDXGIDevice,
    size_px: u32,
) -> Result<IDXGISwapChain1> {
    let description = DXGI_SWAP_CHAIN_DESC1 {
        Width: size_px,
        Height: size_px,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: NO_MULTISAMPLING,
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: BUFFER_COUNT,
        // composition swap chains accept only these two, and premultiplied alpha is what
        // gives the overlay per-pixel transparency
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        ..Default::default()
    };

    // SAFETY: every argument is live for the duration of the calls.
    unsafe {
        let adapter = dxgi_device.GetAdapter()?;
        let factory: IDXGIFactory2 = adapter.GetParent()?;
        factory.CreateSwapChainForComposition(d3d_device, &description, None)
    }
}
