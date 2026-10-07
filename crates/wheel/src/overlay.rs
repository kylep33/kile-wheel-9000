//! The overlay window. Created hidden at startup, so opening only shows it.

use anyhow::{Context, Result};
use kile_wheel_core::slot::Slot;
use kile_wheel_platform::monitor::Monitor;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, FindWindowW, HWND_TOPMOST, PostMessageW, PostQuitMessage,
    RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOREDRAW, SetWindowPos,
    ShowWindow, WM_APP, WM_DESTROY, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR};

use crate::render::Renderer;
use crate::slots::SlotRow;
use crate::theme::Metrics;

/// Also how a second launch finds the running instance.
const WINDOW_CLASS: &str = "kile-wheel-9000.overlay";

/// Asks an already running instance to open its wheel.
pub const WM_OPEN_WHEEL: u32 = WM_APP + 2;

/// The hidden topmost window and the GPU resources behind it.
#[derive(Debug)]
pub struct Overlay {
    window: HWND,
    renderer: Renderer,
    metrics: Metrics,
    centre: POINT,
}

impl Overlay {
    /// Creates the window hidden, at the size the primary monitor's DPI wants, so the
    /// first open only has to move and show it.
    pub fn create(monitor: Monitor) -> Result<Self> {
        let metrics = Metrics::for_dpi(monitor.dpi);
        let size_px = metrics.window_size_px();

        let window = create_window(size_px).context(
            "could not create the overlay window; the window class or display driver refused it",
        )?;

        let renderer = Renderer::new(window, size_px as u32, metrics).context(
            "could not set up Direct2D on DirectComposition; the display driver may not support a composition swap chain",
        )?;

        Ok(Self {
            window,
            renderer,
            metrics,
            centre: monitor.center(),
        })
    }

    pub fn window(&self) -> HWND {
        self.window
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    /// Screen position of the wheel centre, which is what cursor angles are measured
    /// from.
    pub fn centre(&self) -> POINT {
        self.centre
    }

    /// Centres the window on the monitor and matches the swap chain to its DPI.
    pub fn place_on(&mut self, monitor: Monitor) -> Result<()> {
        let metrics = Metrics::for_dpi(monitor.dpi);
        let size_px = metrics.window_size_px();
        let centre = monitor.center();

        // SAFETY: moves our own window; SWP_NOACTIVATE leaves focus where it is.
        unsafe {
            SetWindowPos(
                self.window,
                Some(HWND_TOPMOST),
                centre.x - size_px / 2,
                centre.y - size_px / 2,
                size_px,
                size_px,
                SWP_NOACTIVATE | SWP_NOREDRAW,
            )
        }
        .context("could not move the overlay window onto the target monitor")?;

        self.renderer
            .resize(size_px as u32, metrics)
            .context("could not resize the overlay swap chain for the target monitor")?;

        self.metrics = metrics;
        self.centre = centre;
        Ok(())
    }

    pub fn draw(&self, slots: &SlotRow, selected: Option<Slot>, hub_label: &str) -> Result<()> {
        self.renderer
            .draw(slots, selected, hub_label)
            .context("could not draw the wheel")
    }

    pub fn show(&self) {
        // SAFETY: SW_SHOWNOACTIVATE leaves the app underneath focused, which snippet
        // paste and window activation both depend on.
        let _ = unsafe { ShowWindow(self.window, SW_SHOWNOACTIVATE) };
    }

    pub fn hide(&self) {
        // SAFETY: hides our own window.
        let _ = unsafe { ShowWindow(self.window, SW_HIDE) };
    }
}

/// Hands a second launch over to the instance already running.
pub fn signal_existing() -> bool {
    let class = HSTRING::from(WINDOW_CLASS);

    // SAFETY: the class name stays live for the duration of the call.
    let found = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) };
    let Ok(existing) = found else {
        return false;
    };

    // SAFETY: a parameterless message carries no pointers across the process boundary.
    unsafe { PostMessageW(Some(existing), WM_OPEN_WHEEL, WPARAM(0), LPARAM(0)) }.is_ok()
}

fn create_window(size_px: i32) -> windows::core::Result<HWND> {
    let class = HSTRING::from(WINDOW_CLASS);

    // SAFETY: passing None asks for this executable's own module handle.
    let instance = unsafe { GetModuleHandleW(None) }?;

    let descriptor = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: the descriptor and its strings stay live for the duration of the call.
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        return Err(windows::core::Error::from_thread());
    }

    // SAFETY: the class is registered and the strings stay live for the call.
    unsafe {
        CreateWindowExW(
            // noredirectionbitmap is required for a composition swap chain, noactivate
            // keeps focus with the app underneath, and toolwindow keeps us out of alt-tab
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            size_px,
            size_px,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

/// The wheel reads its input from the low-level hooks, so this only has to let the
/// window be created and destroyed.
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        // SAFETY: no pointers involved.
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }

    // SAFETY: forwards the values Windows gave us to the default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
