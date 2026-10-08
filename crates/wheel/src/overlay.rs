//! The overlay window. Created hidden at startup, so opening only shows it.

use anyhow::{Context, Result};
use kile_wheel_core::config::Config;
use kile_wheel_platform::icons::IconImage;
use kile_wheel_platform::monitor::Monitor;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, FindWindowW, HWND_TOPMOST, KillTimer, PostMessageW,
    PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOREDRAW,
    SetTimer, SetWindowPos, ShowWindow, WM_APP, WM_DESTROY, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR};
use windows_numerics::Vector2;

use crate::render::{FormView, Frame, Renderer};
use crate::theme::{Metrics, Palette};

/// Also how a second launch finds the running instance.
const WINDOW_CLASS: &str = "kile-wheel-9000.overlay";

/// Asks an already running instance to open its wheel.
pub const WM_OPEN_WHEEL: u32 = WM_APP + 2;

/// Posted by the config watcher when the file on disk changed.
pub const WM_CONFIG_CHANGED: u32 = WM_APP + 3;

/// The one timer this window uses for the click-and-hold that toggles edit mode.
const HOLD_TIMER: usize = 1;

/// Drives the form's open animation, which has no input to ride along with. Public so the
/// message loop can tell this timer from the hold one without a second copy of the id.
pub const FRAME_TIMER: usize = 2;

/// The hidden topmost window and the GPU resources behind it.
#[derive(Debug)]
pub struct Overlay {
    window: HWND,
    renderer: Renderer,
    centre: POINT,
}

impl Overlay {
    /// Creates the window hidden, so the first open only has to move and show it.
    pub fn create(monitor: Monitor, config: &Config) -> Result<Self> {
        let metrics = metrics_for(monitor, config);
        let size_px = metrics.window_size_px();

        let window = create_window(size_px).context(
            "could not create the overlay window; the window class or display driver refused it",
        )?;

        let renderer = Renderer::new(window, size_px as u32, metrics, palette_for(config)).context(
            "could not set up Direct2D on DirectComposition; the display driver may not support a composition swap chain",
        )?;

        Ok(Self {
            window,
            renderer,
            centre: monitor.center(),
        })
    }

    pub fn window(&self) -> HWND {
        self.window
    }

    pub fn metrics(&self) -> Metrics {
        self.renderer.metrics()
    }

    /// Screen position of the wheel centre, which is what cursor angles are measured from.
    pub fn centre(&self) -> POINT {
        self.centre
    }

    pub fn has_icon(&self, key: &str) -> bool {
        self.renderer.has_icon(key)
    }

    pub fn upload_icon(&mut self, key: &str, image: &IconImage) -> Result<()> {
        self.renderer
            .upload_icon(key, image)
            .context("Direct2D refused the icon bitmap")
    }

    /// Centres the window on the monitor and matches the swap chain to its DPI.
    pub fn place_on(&mut self, monitor: Monitor, config: &Config) -> Result<()> {
        let metrics = metrics_for(monitor, config);
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
            .resize(size_px as u32, metrics, palette_for(config))
            .context("could not resize the overlay swap chain for the target monitor")?;

        self.centre = centre;
        Ok(())
    }

    pub fn draw(&self, frame: &Frame<'_>) -> Result<()> {
        self.renderer
            .draw(frame)
            .context("could not draw the wheel")
    }

    /// Which form line a screen point lands on. The point is screen-relative, so it is
    /// turned into window coordinates here, where the window's position is known.
    pub fn form_row_at(&self, view: &FormView<'_>, screen_x: i32, screen_y: i32) -> Option<usize> {
        let half_px = self.renderer.metrics().window_size_px() as f32 / 2.0;
        let point = Vector2 {
            X: screen_x as f32 - self.centre.x as f32 + half_px,
            Y: screen_y as f32 - self.centre.y as f32 + half_px,
        };

        self.renderer.form_row_at(view, point)
    }

    pub fn show(&self) {
        // SAFETY: SW_SHOWNOACTIVATE leaves the app underneath focused, which snippet paste
        // and window activation both depend on.
        let _ = unsafe { ShowWindow(self.window, SW_SHOWNOACTIVATE) };
    }

    pub fn hide(&self) {
        // SAFETY: hides our own window.
        let _ = unsafe { ShowWindow(self.window, SW_HIDE) };
    }

    /// Starts the click-and-hold clock. A timer is used rather than measuring on release,
    /// so edit mode arrives while the button is still down.
    pub fn start_hold_timer(&self, after_ms: u32) {
        // SAFETY: targets our own window with no callback, so the message loop gets it.
        unsafe { SetTimer(Some(self.window), HOLD_TIMER, after_ms, None) };
    }

    pub fn stop_hold_timer(&self) {
        // SAFETY: killing a timer that is not running just fails.
        let _ = unsafe { KillTimer(Some(self.window), HOLD_TIMER) };
    }

    /// Ticks while something is animating. Without it an animation would stall, because the
    /// message loop only wakes for input.
    pub fn start_frame_timer(&self, every_ms: u32) {
        // SAFETY: targets our own window with no callback, so the message loop gets it.
        unsafe { SetTimer(Some(self.window), FRAME_TIMER, every_ms, None) };
    }

    pub fn stop_frame_timer(&self) {
        // SAFETY: killing a timer that is not running just fails.
        let _ = unsafe { KillTimer(Some(self.window), FRAME_TIMER) };
    }
}

fn metrics_for(monitor: Monitor, config: &Config) -> Metrics {
    Metrics::new(monitor.dpi, config.wheel.scale())
}

fn palette_for(config: &Config) -> Palette {
    Palette::new(config.wheel.accent(), config.wheel.opacity())
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

/// The wheel reads its input from the low-level hooks, so this only has to let the window
/// be created and destroyed. The timer and wake messages are handled in the main loop.
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
