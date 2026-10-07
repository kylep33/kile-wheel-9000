//! Direct2D drawing composited by DirectComposition.
//!
//! A layered window would need a CPU bitmap blit every frame. DirectComposition
//! composites on the GPU and handles premultiplied alpha itself, so the overlay gets
//! real per-pixel transparency with no blit.

use kile_wheel_core::select::SECTOR_RAD;
use kile_wheel_core::slot::Slot;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1DeviceContext,
    ID2D1Factory1, ID2D1Image, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
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
use windows::core::{HSTRING, Interface, Result};
use windows_numerics::Vector2;

use crate::slots::SlotRow;
use crate::theme::{self, Metrics};

/// Two buffers is the minimum the flip model accepts, and all the overlay needs.
const BUFFER_COUNT: u32 = 2;

/// Present on the next vblank. The open budget allows one frame.
const PRESENT_SYNC_INTERVAL: u32 = 1;

/// Keeps `ResizeBuffers` on the size the swap chain already has.
const KEEP_BUFFER_COUNT: u32 = 0;

const NO_MULTISAMPLING: DXGI_SAMPLE_DESC = DXGI_SAMPLE_DESC {
    Count: 1,
    Quality: 0,
};

/// The GPU side of the overlay.
#[derive(Debug)]
pub struct Renderer {
    context: ID2D1DeviceContext,
    swap_chain: IDXGISwapChain1,
    brush: ID2D1SolidColorBrush,
    slot_format: IDWriteTextFormat,
    hub_format: IDWriteTextFormat,
    size_px: u32,
    metrics: Metrics,
    /// Held so the composition tree stays alive for as long as the window does.
    _composition: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

impl Renderer {
    pub fn new(window: HWND, size_px: u32, metrics: Metrics) -> Result<Self> {
        let d3d_device = create_d3d_device()?;
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;

        // SAFETY: a single-threaded factory matches this renderer living on one thread.
        let d2d_factory: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;

        // SAFETY: both devices are live for the rest of this function.
        let context = unsafe {
            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
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
        let brush = unsafe { context.CreateSolidColorBrush(&theme::LABEL, None) }?;

        // SAFETY: the shared factory is reference counted by windows-rs.
        let text: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;

        Ok(Self {
            slot_format: centred_text_format(&text, metrics.slot_label_size_px)?,
            hub_format: centred_text_format(&text, metrics.hub_label_size_px)?,
            context,
            swap_chain,
            brush,
            size_px,
            metrics,
            _composition: composition,
            _target: target,
            _visual: visual,
        })
    }

    /// Matches the swap chain to a new window size, which happens when the wheel moves
    /// to a monitor with a different DPI.
    pub fn resize(&mut self, size_px: u32, metrics: Metrics) -> Result<()> {
        self.metrics = metrics;
        if self.size_px == size_px {
            return Ok(());
        }

        // SAFETY: the target is dropped first, because ResizeBuffers needs every
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
        Ok(())
    }

    pub fn draw(&self, slots: &SlotRow, selected: Option<Slot>, hub_label: &str) -> Result<()> {
        let surface = self.back_buffer()?;

        // SAFETY: the surface is bound, drawn to, then unbound before it is dropped.
        unsafe {
            self.context.SetTarget(Some(&surface));
            self.context.BeginDraw();
            self.context.Clear(Some(&theme::TRANSPARENT));
        }
        self.draw_wheel(slots, selected, hub_label);

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

    fn draw_wheel(&self, slots: &SlotRow, selected: Option<Slot>, hub_label: &str) {
        let centre = self.centre();

        self.fill_circle(
            centre,
            self.metrics.wheel_diameter_px / 2.0,
            &theme::BACKDROP,
        );
        self.fill_circle(centre, self.metrics.hub_radius_px, &theme::HUB_FILL);
        self.draw_label(
            hub_label,
            square_around(centre, self.metrics.hub_radius_px),
            &self.hub_format,
            &theme::LABEL,
        );

        for slot in Slot::all() {
            self.draw_slot(centre, slot, slots.get(slot), selected == Some(slot));
        }
    }

    fn draw_slot(
        &self,
        centre: Vector2,
        slot: Slot,
        content: Option<&crate::slots::SlotContent>,
        selected: bool,
    ) {
        let bounds = self.slot_bounds(centre, slot);
        let rounded = D2D1_ROUNDED_RECT {
            rect: bounds,
            radiusX: self.metrics.slot_corner_px,
            radiusY: self.metrics.slot_corner_px,
        };

        let (fill, border) = match selected {
            true => (&theme::SLOT_FILL_SELECTED, &theme::SLOT_BORDER_SELECTED),
            false => (&theme::SLOT_FILL, &theme::SLOT_BORDER),
        };

        // SAFETY: the brush and the geometry are live for the duration of the calls.
        unsafe {
            self.context
                .FillRoundedRectangle(&rounded, self.brush(fill));
            self.context.DrawRoundedRectangle(
                &rounded,
                self.brush(border),
                self.metrics.slot_border_px,
                None,
            );
        }

        let (text, colour) = match content {
            Some(content) => (content.label.as_str(), &theme::LABEL),
            None => (EMPTY_SLOT_PLACEHOLDER, &theme::LABEL_EMPTY),
        };
        self.draw_label(
            text,
            inset(bounds, self.metrics.slot_label_inset_px),
            &self.slot_format,
            colour,
        );
    }

    fn draw_label(
        &self,
        text: &str,
        bounds: D2D_RECT_F,
        format: &IDWriteTextFormat,
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
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn fill_circle(&self, centre: Vector2, radius_px: f32, colour: &D2D1_COLOR_F) {
        let ellipse = D2D1_ELLIPSE {
            point: centre,
            radiusX: radius_px,
            radiusY: radius_px,
        };

        // SAFETY: the geometry and brush are live for the duration of the call.
        unsafe { self.context.FillEllipse(&ellipse, self.brush(colour)) };
    }

    /// One brush is reused for everything, because creating a COM object per shape
    /// would allocate on every frame.
    fn brush(&self, colour: &D2D1_COLOR_F) -> &ID2D1SolidColorBrush {
        // SAFETY: setting a colour touches no pointers the caller owns.
        unsafe { self.brush.SetColor(colour) };
        &self.brush
    }

    fn slot_bounds(&self, centre: Vector2, slot: Slot) -> D2D_RECT_F {
        let bearing = SECTOR_RAD * slot.index() as f32;
        let x = centre.X + bearing.sin() * self.metrics.slot_ring_radius_px;
        let y = centre.Y - bearing.cos() * self.metrics.slot_ring_radius_px;

        D2D_RECT_F {
            left: x - self.metrics.slot_width_px / 2.0,
            top: y - self.metrics.slot_height_px / 2.0,
            right: x + self.metrics.slot_width_px / 2.0,
            bottom: y + self.metrics.slot_height_px / 2.0,
        }
    }

    fn centre(&self) -> Vector2 {
        let half = self.size_px as f32 / 2.0;
        Vector2 { X: half, Y: half }
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

/// Shown until phase 2 replaces it with the configurable quips.
const EMPTY_SLOT_PLACEHOLDER: &str = "empty";

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
        // composition swap chains accept only these two, and premultiplied alpha is
        // what gives the overlay per-pixel transparency
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

fn centred_text_format(text: &IDWriteFactory, size_px: f32) -> Result<IDWriteTextFormat> {
    let family = HSTRING::from(theme::FONT_FAMILY);
    let locale = HSTRING::from(theme::LOCALE);

    // SAFETY: both strings stay live for the duration of the call.
    let format = unsafe {
        text.CreateTextFormat(
            &family,
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size_px,
            &locale,
        )
    }?;

    // SAFETY: the format was just created and is not shared yet.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    }
    Ok(format)
}

fn square_around(centre: Vector2, half_side_px: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: centre.X - half_side_px,
        top: centre.Y - half_side_px,
        right: centre.X + half_side_px,
        bottom: centre.Y + half_side_px,
    }
}

fn inset(bounds: D2D_RECT_F, by_px: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.left + by_px,
        top: bounds.top + by_px,
        right: bounds.right - by_px,
        bottom: bounds.bottom - by_px,
    }
}
