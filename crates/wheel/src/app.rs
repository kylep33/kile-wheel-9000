//! The running wheel: owns the window, the model and the message loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use kile_wheel_core::binding::{Action, BindingTable, KeyCode, MouseButton, default_hotkeys, keys};
use kile_wheel_core::select::{self, Offset, latch};
use kile_wheel_platform::activate;
use kile_wheel_platform::apps::WindowModel;
use kile_wheel_platform::hooks::{self, HookThread, InputEvent, SharedInput};
use kile_wheel_platform::monitor::{self, Monitor, MonitorChoice};
use kile_wheel_platform::watcher::{Watcher, WindowEvent};
use tracing::{info, warn};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, SetCursorPos};

use crate::overlay::{self, Overlay};
use crate::slots::{SlotContent, SlotRow};
use crate::state::WheelState;

/// Phase 1 settings. The config file takes these over in phase 2.
const MONITOR_CHOICE: MonitorChoice = MonitorChoice::UnderCursor;
const CENTRE_CURSOR_ON_OPEN: bool = true;
const OPEN_HOTKEY_CLOSES: bool = false;
const MINIMIZE_IF_FOCUSED: bool = true;
const SUPPRESS_IN_FULLSCREEN: bool = true;

/// Phase 3 replaces this with the tab number, name, and the date and time.
const HUB_LABEL: &str = "1";

/// Hotkey press to wheel visible. Spec requirement, not a nice to have.
const OPEN_BUDGET: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct App {
    /// Declared first so the hooks come down before the window they post to.
    _hooks: HookThread,
    shared: Arc<SharedInput>,
    overlay: Overlay,
    model: WindowModel,
    watcher: Watcher,
    state: WheelState,
    slots: SlotRow,
    redraw: bool,
}

impl App {
    pub fn start() -> Result<Self> {
        let shared = Arc::new(SharedInput::default());
        shared.replace_bindings(BindingTable::new(default_hotkeys()));

        let overlay = Overlay::create(monitor::monitor_for(MONITOR_CHOICE))?;
        shared.set_overlay(overlay.window());

        let hooks = hooks::install(Arc::clone(&shared))
            .context("could not install the input hooks, so no hotkey would work")?;
        let watcher = Watcher::install().context(
            "could not subscribe to window events, so the running app list would go stale",
        )?;

        let mut model = WindowModel::default();
        model
            .rebuild()
            .context("could not enumerate the open windows")?;
        info!(apps = model.groups().len(), "window model built");

        Ok(Self {
            _hooks: hooks,
            shared,
            overlay,
            model,
            watcher,
            state: WheelState::default(),
            slots: SlotRow::default(),
            redraw: false,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let mut message = MSG::default();

        // SAFETY: the pointer targets a live local; the loop ends on WM_QUIT or error.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            if message.message == overlay::WM_OPEN_WHEEL {
                let opened = self.open(None);
                log_failure(opened);
            }

            // SAFETY: the message was just filled in by GetMessageW.
            unsafe { DispatchMessageW(&message) };

            self.absorb_window_events();
            self.absorb_input();
        }

        info!("stopping");
        Ok(())
    }

    fn absorb_window_events(&mut self) {
        for event in self.watcher.drain() {
            match event {
                WindowEvent::Changed(window) => self.model.refresh(window),
                WindowEvent::Vanished(window) => self.model.remove(window),
                WindowEvent::Focused(window) => self.on_focus_changed(window),
            }
        }
    }

    /// The fullscreen check runs here rather than in the hook callback, so the hook
    /// stays cheap enough that Windows never tears it down.
    fn on_focus_changed(&mut self, window: HWND) {
        self.model.set_focused(window);

        if SUPPRESS_IN_FULLSCREEN {
            self.shared
                .set_hotkeys_suppressed(monitor::foreground_is_fullscreen());
        }
    }

    fn absorb_input(&mut self) {
        for event in self.shared.drain() {
            self.handle(event);
        }
        if !self.redraw {
            return;
        }

        self.redraw = false;
        log_failure(
            self.overlay
                .draw(&self.slots, self.state.selected(), HUB_LABEL),
        );
    }

    fn handle(&mut self, event: InputEvent) {
        match event {
            InputEvent::Hotkey { action, seen_at } => self.handle_hotkey(action, seen_at),
            InputEvent::Key {
                code,
                pressed: true,
                ..
            } => self.handle_key(code),
            InputEvent::MouseMoved { x, y } => self.aim(x, y),
            InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            } => {
                let activated = self.activate();
                log_failure(activated);
            }
            InputEvent::Key { .. } | InputEvent::MouseButton { .. } => {}
        }
    }

    fn handle_hotkey(&mut self, action: Action, seen_at: Instant) {
        match (action, self.state.is_open()) {
            (Action::Open | Action::Toggle, false) => {
                let opened = self.open(Some(seen_at));
                log_failure(opened);
            }
            // the open hotkey only doubles as a close when the setting says so
            (Action::Open, true) if OPEN_HOTKEY_CLOSES => self.close(),
            (Action::Close | Action::Toggle, true) => self.close(),
            (Action::Open, true) | (Action::Close, false) => {}
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        if !self.state.is_open() {
            return;
        }

        // backspace backs out one level, and the top level is the wheel itself
        if code == keys::ESC || code == keys::BACKSPACE {
            self.close();
            return;
        }
        if code == keys::ENTER {
            let activated = self.activate();
            log_failure(activated);
            return;
        }

        let Some(cardinal) = latch::cardinal_for(code) else {
            return;
        };
        self.state.press_direction(cardinal);
        self.redraw = true;
    }

    fn aim(&mut self, x: i32, y: i32) {
        let centre = self.overlay.centre();
        let offset = Offset::new((x - centre.x) as f32, (y - centre.y) as f32);

        self.state.aim_at(select::slot_at(
            offset,
            self.overlay.metrics().hub_radius_px,
        ));
        self.redraw = true;
    }

    fn open(&mut self, hotkey_seen_at: Option<Instant>) -> Result<()> {
        if self.state.is_open() {
            return Ok(());
        }

        let monitor = monitor::monitor_for(MONITOR_CHOICE);
        self.overlay.place_on(monitor)?;

        self.slots = SlotRow::from_running(&self.model);
        self.state.open();
        self.shared.set_wheel_open(true);

        // drawn before it is shown, so the first frame can never be a stale one
        self.overlay.draw(&self.slots, None, HUB_LABEL)?;
        self.overlay.show();
        self.redraw = false;

        if CENTRE_CURSOR_ON_OPEN {
            centre_cursor(monitor);
        }

        if let Some(seen_at) = hotkey_seen_at {
            report_open_latency(seen_at.elapsed());
        }
        Ok(())
    }

    fn close(&mut self) {
        if !self.state.is_open() {
            return;
        }

        self.shared.set_wheel_open(false);
        self.state.close();
        self.overlay.hide();
        self.redraw = false;
    }

    /// Hides the wheel before acting, so the target window lands in front of nothing
    /// of ours.
    fn activate(&mut self) -> Result<()> {
        let Some(slot) = self.state.selected() else {
            return Ok(());
        };
        let Some(content) = self.slots.get(slot).cloned() else {
            return Ok(());
        };

        self.close();
        activate_app(&content)
    }
}

/// Mirrors the taskbar: focus the window, or minimize it if it already has focus.
///
/// An app with more than one window gets a sub-wheel in phase 2; for now the oldest
/// window wins.
fn activate_app(content: &SlotContent) -> Result<()> {
    let Some(window) = content.windows.first().copied() else {
        return Ok(());
    };

    if MINIMIZE_IF_FOCUSED && activate::is_foreground(window) {
        activate::minimize(window);
        return Ok(());
    }

    activate::focus(window)
        .with_context(|| format!("could not bring \"{}\" forward", content.label))
}

/// The open budget is the whole point of the design, so every open is measured and a
/// miss is loud.
fn report_open_latency(elapsed: Duration) {
    if elapsed > OPEN_BUDGET {
        warn!(
            micros = elapsed.as_micros(),
            budget_ms = OPEN_BUDGET.as_millis(),
            "wheel open missed its budget"
        );
        return;
    }
    info!(micros = elapsed.as_micros(), "wheel open");
}

fn centre_cursor(monitor: Monitor) {
    let centre = monitor.center();

    // SAFETY: moves the cursor only. The move it generates is flagged injected, so our
    // own mouse hook ignores it and the selection does not jump.
    let _ = unsafe { SetCursorPos(centre.x, centre.y) };
}

/// Keeps the message loop alive. A failed draw or activation is worth a log line, not
/// the whole launcher going away.
fn log_failure(outcome: Result<()>) {
    if let Err(error) = outcome {
        warn!("{error:#}");
    }
}
