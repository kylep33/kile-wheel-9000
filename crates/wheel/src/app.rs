//! The running wheel: owns the window, the model, and the message loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Local;
use kile_wheel_core::binding::{Action, KeyCode, Modifiers, MouseButton, keys};
use kile_wheel_core::config::{Config, ConfigDocument, MonitorTarget};
use kile_wheel_core::item::{Item, ItemKind, Position};
use kile_wheel_core::layout::SlotMemory;
use kile_wheel_core::names::{ConfigPaths, SETTINGS_EXE};
use kile_wheel_core::select::{self, Offset, held};
use kile_wheel_core::slot::{Slot, Tab};
use kile_wheel_platform::apps::{AppKey, WindowModel};
use kile_wheel_platform::clipboard;
use kile_wheel_platform::config_watch::ConfigWatcher;
use kile_wheel_platform::hooks::{self, HookThread, InputEvent, SharedInput};
use kile_wheel_platform::icons::IconCache;
use kile_wheel_platform::monitor::{self, Monitor, MonitorChoice};
use kile_wheel_platform::watcher::{Watcher, WindowEvent};
use kile_wheel_platform::{activate, launch, shell, snippet};
use tracing::{info, warn};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, DispatchMessageW, GetMessageW, MSG, SetCursorPos, WM_TIMER,
};

use crate::form::{Form, Outcome};
use crate::keyboard::printable;
use crate::overlay::{self, Overlay};
use crate::render::{FormView, Frame, HubView, SlotView};
use crate::slots::{Layout, SlotEntry, SlotTarget};
use crate::state::{Back, Mode, WheelState, WindowChoice};
use crate::theme;
use crate::tray::{self, Tray};

/// Hotkey press to wheel visible. Spec requirement, not a nice to have.
const OPEN_BUDGET: Duration = Duration::from_millis(50);

/// What the notification area shows on hover.
const TRAY_TOOLTIP: &str = "kile-wheel-9000";

/// Hold this long on a slot or the cog to toggle edit mode.
const EDIT_HOLD_MS: u32 = 400;

/// Roughly one frame at 60Hz, for driving the form's open animation.
const FRAME_MS: u32 = 16;

const CLOCK_FORMAT: &str = "%H:%M";
const DATE_FORMAT: &str = "%a %d %b";

/// A left button still down, and what it landed on.
#[derive(Debug, Clone, Copy)]
struct Press {
    on_cog: bool,
    /// Set once the hold has turned into an edit-mode toggle, so the release does nothing.
    consumed: bool,
}

#[derive(Debug)]
pub struct App {
    /// Declared first so the hooks come down before the window they post to.
    _hooks: HookThread,
    shared: Arc<SharedInput>,
    overlay: Overlay,
    model: WindowModel,
    watcher: Watcher,
    config_watch: Option<ConfigWatcher>,
    state: WheelState,
    memory: SlotMemory<AppKey>,
    layout: Layout,
    icons: IconCache,
    paths: ConfigPaths,
    config: Config,
    press: Option<Press>,
    /// The open petal form, which takes every key while it is up.
    form: Option<Form>,
    /// Held so the icon is removed on the way out. Dropped after the overlay, which is
    /// what its menu posts to.
    _tray: Option<Tray>,
    redraw: bool,
}

impl App {
    pub fn start(paths: ConfigPaths, config: Config) -> Result<Self> {
        let shared = Arc::new(SharedInput::default());
        shared.replace_bindings(
            config
                .binding_table()
                .context("the configured hotkeys could not be compiled")?,
        );

        let overlay = Overlay::create(monitor::monitor_for(choice(&config)), &config)?;
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

        // a watch that fails to start costs live reload, not the whole launcher
        let config_watch = match ConfigWatcher::start(
            &paths.config_file(),
            overlay.window(),
            overlay::WM_CONFIG_CHANGED,
        ) {
            Ok(watch) => Some(watch),
            Err(error) => {
                warn!("config changes will need a restart to apply: {error}");
                None
            }
        };

        // a tray icon that fails to appear costs the menu, not the launcher
        let tray = match Tray::create(overlay.window(), TRAY_TOOLTIP) {
            Ok(tray) => Some(tray),
            Err(error) => {
                warn!("no tray icon, so use the hotkey or the settings window: {error:#}");
                None
            }
        };

        let mut app = Self {
            _hooks: hooks,
            shared,
            overlay,
            model,
            watcher,
            config_watch,
            state: WheelState::default(),
            memory: SlotMemory::default(),
            layout: Layout::default(),
            icons: IconCache::default(),
            paths,
            config,
            press: None,
            form: None,
            _tray: tray,
            redraw: false,
        };

        app.warm_up();
        Ok(app)
    }

    /// Draws one throwaway frame into the still-hidden window.
    ///
    /// The first draw pays for shader compilation and the first buffer allocation, which
    /// measured 64 ms. Spending it at startup keeps it out of the first open.
    fn warm_up(&mut self) {
        self.layout = Layout::build(&self.config, &self.model, &mut self.memory);
        self.load_icons();
        log_failure(self.draw_wheel());
        self.layout = Layout::default();
        self.memory = SlotMemory::default();
    }

    pub fn run(mut self) -> Result<()> {
        let mut message = MSG::default();

        // SAFETY: the pointer targets a live local; the loop ends on WM_QUIT or an error.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            self.handle_message(&message);

            // SAFETY: the message was just filled in by GetMessageW.
            unsafe { DispatchMessageW(&message) };

            self.absorb_window_events();
            self.absorb_input();
        }

        info!("stopping");
        Ok(())
    }

    fn handle_message(&mut self, message: &MSG) {
        if message.message == overlay::WM_OPEN_WHEEL {
            let opened = self.open(None);
            log_failure(opened);
            return;
        }
        if message.message == WM_TIMER {
            match message.wParam.0 {
                // the tick re-checks the clock so the last frame of the growth also
                // stops the timer, instead of redrawing a settled petal forever
                overlay::FRAME_TIMER => {
                    self.redraw = true;
                    self.drive_animation();
                }
                _ => self.hold_elapsed(),
            }
            return;
        }
        if message.message == overlay::WM_CONFIG_CHANGED {
            self.absorb_config_change();
            return;
        }
        if message.message == tray::WM_TRAY_COMMAND {
            self.handle_tray_command(tray::Command::from_id(message.wParam.0));
        }
    }

    /// A menu choice arrives here rather than running inside the menu's own message loop,
    /// which would re-enter the app while the menu still owns the thread.
    fn handle_tray_command(&mut self, command: Option<tray::Command>) {
        let Some(command) = command else {
            return;
        };
        info!(?command, "tray command");

        match command {
            tray::Command::OpenWheel => log_failure(self.open(None)),
            tray::Command::Settings => log_failure(self.open_settings()),
            tray::Command::ReloadConfig => match self.reload_config() {
                Ok(()) => info!("reloaded the config"),
                Err(error) => warn!("keeping the last good config: {error:#}"),
            },
            // the overlay's WM_DESTROY is what posts WM_QUIT, so the loop ends cleanly and
            // the tray icon is removed on the way out
            tray::Command::Quit => self.quit(),
        }
    }

    fn quit(&mut self) {
        self.close();

        // SAFETY: destroys our own window, which posts WM_QUIT from its procedure.
        let _ = unsafe { DestroyWindow(self.overlay.window()) };
    }

    /// A bad edit keeps the config already in memory, so a typo cannot take the wheel out.
    fn absorb_config_change(&mut self) {
        let Some(watch) = &self.config_watch else {
            return;
        };
        if !watch.take_change() {
            return;
        }

        match self.reload_config() {
            Ok(()) => info!("reloaded the config"),
            Err(error) => warn!("keeping the last good config: {error:#}"),
        }
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

    /// The fullscreen check runs here rather than in the hook callback, so the hook stays
    /// cheap enough that Windows never tears it down.
    fn on_focus_changed(&mut self, window: HWND) {
        self.model.set_focused(window);

        if self.config.general.disable_in_fullscreen {
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
        let drawn = self.draw();
        log_failure(drawn);
        self.drive_animation();
    }

    fn handle(&mut self, event: InputEvent) {
        match event {
            InputEvent::Hotkey { action, seen_at } => self.handle_hotkey(action, seen_at),
            InputEvent::Key {
                code,
                modifiers,
                pressed,
            } => match pressed {
                true => self.handle_key(code, modifiers, printable(code, modifiers)),
                false => self.handle_key_release(code),
            },
            InputEvent::MouseMoved { x, y } => self.aim(x, y),
            InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
            } => self.handle_left_button(pressed),
            InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: true,
            } => self.launch_new_instance(),
            InputEvent::MouseButton { .. } => {}
        }
    }

    fn handle_hotkey(&mut self, action: Action, seen_at: Instant) {
        match (action, self.state.is_open()) {
            (Action::Open | Action::Toggle, false) => {
                let opened = self.open(Some(seen_at));
                log_failure(opened);
            }
            // the open hotkey only doubles as a close when the setting says so
            (Action::Open, true) if self.config.wheel.open_hotkey_closes => self.close(),
            (Action::Close | Action::Toggle, true) => self.close(),
            (Action::Open, true) | (Action::Close, false) => {}
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: Modifiers, typed: Option<char>) {
        // Esc is handled before the open check on purpose: it is the way out if the state
        // and the hook ever disagree about whether the wheel is up, and the hook is what
        // decides whether your keys reach anything.
        if code == keys::ESC {
            match self.form.is_some() {
                true => self.close_form(),
                false => self.close(),
            }
            return;
        }
        if !self.state.is_open() {
            return;
        }

        // a form takes every other key, so Backspace edits a field instead of backing out
        if self.form.is_some() {
            self.handle_form_key(code, modifiers, typed);
            return;
        }

        if code == keys::BACKSPACE {
            self.go_back();
            return;
        }
        if code == keys::ENTER {
            // Shift+Enter is a second instance rather than a focus, matching the taskbar
            match modifiers.shift {
                true => self.launch_new_instance(),
                false => log_failure(self.activate()),
            }
            return;
        }

        if let Some(number) = tab_number(code) {
            self.switch_tab(number);
            return;
        }

        let Some(cardinal) = held::cardinal_for(code) else {
            return;
        };
        self.state.press_direction(cardinal);
        self.redraw = true;
    }

    /// Routes a key into the open form and acts on what it decided.
    fn handle_form_key(&mut self, code: KeyCode, modifiers: Modifiers, typed: Option<char>) {
        let Some(form) = &mut self.form else {
            return;
        };

        // the clipboard is only read when a paste is actually asked for
        let clipboard = match modifiers.ctrl && code == keys::V {
            true => clipboard::read_text(),
            false => None,
        };

        let outcome = form.press(code, modifiers, typed, clipboard.as_deref());
        self.apply_form_outcome(outcome);
    }

    /// Routes a click that landed inside the form. Shared with the key path so a click and
    /// a keypress can never drift on what Save or Delete means.
    fn handle_form_click(&mut self, x: i32, y: i32) {
        let lines = self.form_lines();
        let Some(view) = self.form_view(&lines) else {
            return;
        };
        let Some(index) = self.overlay.form_row_at(&view, x, y) else {
            return;
        };

        let Some(form) = &mut self.form else {
            return;
        };
        let outcome = form.click(index);
        self.apply_form_outcome(outcome);
    }

    fn apply_form_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Ignored => {}
            Outcome::Redraw => self.redraw = true,
            Outcome::Cancel => self.close_form(),
            Outcome::Copy(text) => {
                // a failed copy must not close the form, so it is only reported
                log_failure(clipboard::write_text(&text).map_err(anyhow::Error::from));
                self.redraw = true;
            }
            Outcome::Delete => {
                let cleared = self.clear_slot();
                log_failure(cleared);
                self.close_form();
            }
            Outcome::Commit(item) => {
                let saved = self.save_item(&item);
                log_failure(saved);
                self.close_form();
            }
        }
    }

    /// Opens the form on whatever the selected slot holds, or on the type picker when it
    /// holds nothing.
    fn open_form(&mut self) {
        let Some(slot) = self.state.selected() else {
            return;
        };
        let tab = self.state.tab();

        self.form = Some(match self.layout.get(tab, slot) {
            Some(entry) => match &entry.target {
                SlotTarget::Pinned(item) => Form::editing(tab, slot, item),
                // a running app or an empty unpinned slot is still an unpinned slot entry
                _ => Form::editing(tab, slot, &unpinned_item(tab, slot)),
            },
            None => Form::picking(tab, slot),
        });
        self.redraw = true;
        self.drive_animation();
    }

    fn close_form(&mut self) {
        self.form = None;
        self.redraw = true;
        // the wheel's own transitions may still be running, so the timer is re-evaluated
        // rather than simply stopped
        self.drive_animation();
    }

    /// Keeps frames coming while anything is still moving, then stops. Without this the
    /// transitions would only advance when some other input happened to force a redraw.
    fn drive_animation(&mut self) {
        let duration_ms = self.animation_ms();
        let form_running = self
            .form
            .as_ref()
            .is_some_and(|form| form.animating(duration_ms));
        let animating = form_running || (self.state.is_open() && self.state.animating(duration_ms));

        match animating {
            true => self.overlay.start_frame_timer(FRAME_MS),
            false => self.overlay.stop_frame_timer(),
        }
    }

    fn save_item(&mut self, item: &Item) -> Result<()> {
        let mut document = ConfigDocument::load(&self.paths.config_file())
            .context("could not open the config to save the slot")?;

        document.upsert_item(item)?;
        document.save().context("could not save the slot")?;
        info!(name = %item.name, kind = item.kind.type_name(), "saved a slot");
        self.reload_config()
    }

    fn clear_slot(&mut self) -> Result<()> {
        let Some(form) = &self.form else {
            return Ok(());
        };
        let position = Position::new(form.tab(), form.slot());

        let mut document = ConfigDocument::load(&self.paths.config_file())
            .context("could not open the config to clear the slot")?;

        if !document.remove_item_at(position) {
            info!(at = %describe(position), "that slot was already empty");
            return Ok(());
        }

        document.save().context("could not save the cleared slot")?;
        info!(at = %describe(position), "cleared a slot");
        self.reload_config()
    }

    /// Only direction keys do anything on release; everything else acts on the press.
    fn handle_key_release(&mut self, code: KeyCode) {
        if !self.state.is_open() {
            return;
        }

        let Some(cardinal) = held::cardinal_for(code) else {
            return;
        };
        self.state.release_direction(cardinal);
        self.redraw = true;
    }

    /// While dragging, a number key moves the item rather than just changing the view.
    fn switch_tab(&mut self, number: usize) {
        let dragged_from = self
            .state
            .dragging()
            .map(|slot| Position::new(self.state.tab(), slot));

        if !self.state.select_tab(number) {
            return;
        }
        self.redraw = true;

        let Some(from) = dragged_from else {
            return;
        };
        let moved = self.move_item(from, Position::new(self.state.tab(), from.slot));
        log_failure(moved);
    }

    fn handle_left_button(&mut self, pressed: bool) {
        if !self.state.is_open() {
            return;
        }

        // a form takes the click itself: the wheel underneath must not select or drag while
        // the petal is covering it
        if self.form.is_some() {
            if let (true, Some(cursor)) = (pressed, monitor::cursor_position()) {
                self.handle_form_click(cursor.x, cursor.y);
            }
            return;
        }

        match pressed {
            true => self.begin_press(),
            false => self.end_press(),
        }
    }

    fn begin_press(&mut self) {
        let on_cog = self.cursor_on_cog();
        let slot = self.state.selected();

        self.press = Some(Press {
            on_cog,
            consumed: false,
        });

        // in edit mode a press on a slot starts a drag, and the hold clock must not run
        // alongside it: a deliberate drag takes longer than the hold, so the hold would
        // leave edit mode and swallow the drop. The hold still works on the hub and on
        // empty space, and Backspace always leaves edit mode.
        if let Some(slot) = slot.filter(|_| self.state.edit_mode()) {
            self.state.begin_drag(slot);
            self.redraw = true;
            return;
        }

        self.overlay.start_hold_timer(EDIT_HOLD_MS);
    }

    fn end_press(&mut self) {
        self.overlay.stop_hold_timer();

        let Some(press) = self.press.take() else {
            return;
        };
        if press.consumed {
            return;
        }

        if press.on_cog {
            // the hub holds no item, so there is nothing for the petal form to edit here
            log_failure(self.open_settings());
            return;
        }
        if self.state.edit_mode() {
            self.finish_drag();
            return;
        }

        let activated = self.activate();
        log_failure(activated);
    }

    /// The 400 ms hold turned into an edit-mode toggle, so the release is swallowed.
    fn hold_elapsed(&mut self) {
        self.overlay.stop_hold_timer();

        let Some(press) = self.press.as_mut() else {
            return;
        };
        press.consumed = true;

        let on = self.state.toggle_edit();
        info!(edit_mode = on, "hold toggled edit mode");
        self.redraw = true;
    }

    /// A press and release on the same slot is a click, which opens the form. Landing
    /// somewhere else is a drag, which moves the slot.
    fn finish_drag(&mut self) {
        let Some(from_slot) = self.state.end_drag() else {
            self.open_form();
            return;
        };
        self.redraw = true;

        let Some(to_slot) = self.state.selected() else {
            return;
        };
        if from_slot == to_slot {
            self.open_form();
            return;
        }

        let tab = self.state.tab();
        let moved = self.move_item(Position::new(tab, from_slot), Position::new(tab, to_slot));
        log_failure(moved);
    }

    /// Writes the move straight to the config, so an edit survives a crash.
    fn move_item(&mut self, from: Position, to: Position) -> Result<()> {
        if from == to {
            return Ok(());
        }

        let mut document = ConfigDocument::load(&self.paths.config_file())
            .context("could not open the config to record the move")?;

        if !document.move_item(from, to) {
            self.pin_dropped_app(&mut document, from, to)?;
        }

        document.save().context("could not save the moved item")?;
        info!(
            from = %describe(from),
            to = %describe(to),
            "moved a slot"
        );
        self.reload_config()
    }

    /// Nothing is pinned at `from`, so this is a running app being dropped somewhere.
    /// Pinning it is what records the move; anything already pinned at `to` trades places
    /// with it rather than being overwritten.
    fn pin_dropped_app(
        &self,
        document: &mut ConfigDocument,
        from: Position,
        to: Position,
    ) -> Result<()> {
        let Some(entry) = self.layout.get(from.tab, from.slot) else {
            info!(from = %describe(from), "that slot is empty, so there was nothing to move");
            return Ok(());
        };
        let Some(item) = pin_as_item(entry, to) else {
            warn!(
                label = %entry.label,
                "that slot cannot be pinned, so the drop was dropped"
            );
            return Ok(());
        };

        document.move_item(to, from);
        document.upsert_item(&item)?;
        info!(label = %item.name, to = %describe(to), "pinned a running app");
        Ok(())
    }

    fn open_settings(&mut self) -> Result<()> {
        self.close();

        let exe = std::env::current_exe()
            .context("could not find our own folder to look for the settings window in")?
            .with_file_name(SETTINGS_EXE);

        if !exe.exists() {
            anyhow::bail!(
                "{} is not beside the wheel; re-run install.ps1 to put it there",
                exe.display()
            );
        }

        shell::execute(&shell::Command::new(exe.to_string_lossy().into_owned()))
            .context("could not start the settings window")
    }

    fn aim(&mut self, x: i32, y: i32) {
        // the petal covers the wheel, so moving the mouse over it must not drag the
        // highlight around underneath
        if self.form.is_some() {
            return;
        }

        let centre = self.overlay.centre();
        let offset = Offset::new((x - centre.x) as f32, (y - centre.y) as f32);

        self.state.aim_at(select::slot_at(
            offset,
            self.overlay.metrics().dead_zone_px(),
        ));
        self.redraw = true;
    }

    fn cursor_on_cog(&self) -> bool {
        let Some(cursor) = monitor::cursor_position() else {
            return false;
        };
        let centre = self.overlay.centre();
        let metrics = self.overlay.metrics();

        let dx = (cursor.x - centre.x) as f32;
        let dy = (cursor.y - centre.y) as f32 - metrics.cog_offset_px();
        dx.hypot(dy) <= metrics.cog_hit_radius_px()
    }

    fn open(&mut self, hotkey_seen_at: Option<Instant>) -> Result<()> {
        if self.state.is_open() {
            return Ok(());
        }

        let monitor = monitor::monitor_for(choice(&self.config));
        self.overlay.place_on(monitor, &self.config)?;

        self.layout = Layout::build(&self.config, &self.model, &mut self.memory);
        // every tab exists whether the config mentions it or not, so an unused one is a
        // blank wheel you can fill rather than a key that does nothing
        self.state.open(Tab::MAX_COUNT);
        self.shared.set_wheel_open(true);
        self.load_icons();

        // drawn before it is shown, so the first frame can never be a stale one
        self.draw()?;
        self.overlay.show();
        self.redraw = false;

        if self.config.wheel.center_cursor {
            centre_cursor(monitor);
        }

        if let Some(seen_at) = hotkey_seen_at {
            report_open_latency(seen_at.elapsed());
        }

        // measured after the latency report, so the timer setup is never counted against
        // the open budget
        self.drive_animation();
        Ok(())
    }

    /// Unconditional on purpose. A guard here once let a state desync leave the hook
    /// believing the wheel was open, which swallowed every key until the next open.
    fn close(&mut self) {
        self.overlay.stop_hold_timer();
        self.overlay.stop_frame_timer();
        self.form = None;
        self.press = None;
        self.shared.set_wheel_open(false);
        self.state.close();
        self.overlay.hide();
        self.redraw = false;
    }

    fn go_back(&mut self) {
        match self.state.back() {
            Back::WentUp => self.redraw = true,
            Back::Closed => self.close(),
        }
    }

    /// Uploads any icon the GPU has not seen yet. Extraction is cached, so this is a
    /// lookup on every open after the first.
    fn load_icons(&mut self) {
        let wanted = self.overlay.metrics().icon_size_px.ceil() as u32;
        let sources = self.icon_sources();

        for source in sources {
            if self.overlay.has_icon(&source) {
                continue;
            }
            let Some(image) = self.icons.get(&source, wanted) else {
                continue;
            };
            let uploaded = self.overlay.upload_icon(&source, image);
            log_failure(uploaded.context("could not upload an icon to the GPU"));
        }
    }

    fn icon_sources(&self) -> Vec<String> {
        let mut sources = Vec::new();
        for tab in (1..=self.layout.tab_count()).filter_map(Tab::from_number) {
            for slot in Slot::all() {
                let Some(entry) = self.layout.get(tab, slot) else {
                    continue;
                };
                if let Some(source) = &entry.icon_source {
                    sources.push(self.resolve_icon(source));
                }
            }
        }
        sources
    }

    /// A configured icon path is relative to the config folder; an extracted one is
    /// already absolute.
    fn resolve_icon(&self, source: &str) -> String {
        let path = std::path::Path::new(source);
        if path.is_absolute() {
            return source.to_owned();
        }
        self.paths.resolve_icon(path).to_string_lossy().into_owned()
    }

    fn draw(&self) -> Result<()> {
        match self.state.mode() {
            Mode::Wheel => self.draw_wheel(),
            Mode::SubWheel { windows } => self.draw_sub_wheel(windows),
        }
    }

    /// The lines of the open form, kept alive by the caller so the view can borrow them.
    fn form_lines(&self) -> Vec<crate::form::Line> {
        self.form.as_ref().map(Form::lines).unwrap_or_default()
    }

    fn form_view<'a>(&'a self, lines: &'a [crate::form::Line]) -> Option<FormView<'a>> {
        let form = self.form.as_ref()?;

        Some(FormView {
            slot: form.slot(),
            title: form.title(),
            lines,
            progress: form.progress(self.config.wheel.animation.duration_ms()),
        })
    }

    fn draw_wheel(&self) -> Result<()> {
        let lines = self.form_lines();
        let tab = self.state.tab();
        let entries: Vec<Option<&SlotEntry>> =
            Slot::all().map(|slot| self.layout.get(tab, slot)).collect();
        let quip_for = |slot: Slot| quip(&self.config.wheel.empty_quips, tab, slot);

        let slots = std::array::from_fn(|index| {
            let slot = Slot::new(index).expect("index comes from Slot::all");
            match entries[index] {
                // an unpinned slot with nothing in it reads as empty, but keeps its glyph
                Some(entry) if entry.is_waiting() => SlotView {
                    label: quip_for(slot),
                    type_glyph: Some(glyph_for(&entry.kind)),
                    ..SlotView::default()
                },
                Some(entry) => SlotView {
                    label: &entry.label,
                    icon_key: entry.icon_source.as_deref(),
                    type_glyph: Some(glyph_for(&entry.kind)),
                    running: entry.is_running(),
                    focused: entry.focused,
                    window_count: entry.windows.len(),
                    filled: true,
                },
                None => SlotView {
                    label: quip_for(slot),
                    ..SlotView::default()
                },
            }
        });

        let now = Local::now();
        let clock = now.format(CLOCK_FORMAT).to_string();
        let date = now.format(DATE_FORMAT).to_string();
        let show_clock = self.config.general.show_clock;
        let overflow_note = overflow_note(self.layout.overflow());

        self.overlay.draw(&Frame {
            slots,
            selected: self.state.selected(),
            highlights: self.highlights(),
            appear: self.state.appear_progress(self.animation_ms()),
            hub: HubView {
                tab_line: &self.tab_line(tab),
                clock: show_clock.then_some(clock.as_str()),
                date: show_clock.then_some(date.as_str()),
                overflow_note: overflow_note.as_deref(),
            },
            edit_mode: self.state.edit_mode(),
            dragging: self.state.dragging(),
            overflow: self.layout.overflow(),
            form: self.form_view(&lines),
        })
    }

    fn animation_ms(&self) -> u64 {
        self.config.wheel.animation.duration_ms()
    }

    /// How lit each wedge is this frame, which is what makes the selection slide rather
    /// than jump between wedges.
    fn highlights(&self) -> [f32; Slot::COUNT] {
        let hover = self.state.hover();
        let duration_ms = self.animation_ms();

        std::array::from_fn(|index| {
            let slot = Slot::new(index).expect("index comes from the array length");
            hover.highlight(slot, duration_ms)
        })
    }

    fn draw_sub_wheel(&self, windows: &[WindowChoice]) -> Result<()> {
        let slots = std::array::from_fn(|index| match windows.get(index) {
            Some(choice) => SlotView {
                label: &choice.title,
                running: true,
                filled: true,
                window_count: 1,
                ..SlotView::default()
            },
            None => SlotView::default(),
        });

        // the wheel has eight slots, so a ninth window has nowhere to go; say so rather
        // than quietly dropping it
        let hidden = windows.len().saturating_sub(Slot::COUNT);
        let note = hidden_windows_note(hidden);

        self.overlay.draw(&Frame {
            slots,
            selected: self.state.selected(),
            highlights: self.highlights(),
            appear: self.state.appear_progress(self.animation_ms()),
            hub: HubView {
                tab_line: "WINDOWS",
                clock: None,
                date: None,
                overflow_note: note.as_deref(),
            },
            edit_mode: false,
            dragging: None,
            overflow: hidden,
            form: None,
        })
    }

    fn tab_line(&self, tab: Tab) -> String {
        match self
            .config
            .tab(tab)
            .and_then(|settings| settings.name.clone())
        {
            Some(name) => format!("{}  {}", tab.number(), name.to_uppercase()),
            None => format!("TAB {}", tab.number()),
        }
    }

    fn activate(&mut self) -> Result<()> {
        let Some(slot) = self.state.selected() else {
            return Ok(());
        };

        if let Mode::SubWheel { windows } = self.state.mode() {
            let chosen = windows.get(slot.index()).map(|choice| choice.handle);
            self.close();
            return match chosen {
                Some(window) => self.focus_or_minimize(window),
                None => Ok(()),
            };
        }

        let Some(entry) = self.layout.get(self.state.tab(), slot).cloned() else {
            return Ok(());
        };
        self.activate_entry(&entry)
    }

    fn activate_entry(&mut self, entry: &SlotEntry) -> Result<()> {
        // an unpinned slot with nothing in it has nothing to do
        if matches!(entry.target, SlotTarget::Waiting) {
            return Ok(());
        }

        // more than one window means choose which, exactly as the taskbar does
        if entry.windows.len() > 1 {
            self.state.enter_sub_wheel(self.window_choices(entry));
            self.redraw = true;
            return Ok(());
        }

        if let Some(window) = entry.windows.first().copied() {
            self.close();
            return self.focus_or_minimize(window);
        }

        let SlotTarget::Pinned(item) = &entry.target else {
            return Ok(());
        };
        let item = item.clone();
        self.close();
        self.run_item(&item)
    }

    fn window_choices(&self, entry: &SlotEntry) -> Vec<WindowChoice> {
        let titles = self.model.groups();
        entry
            .windows
            .iter()
            .map(|handle| WindowChoice {
                handle: *handle,
                title: titles
                    .iter()
                    .flat_map(|group| group.windows.iter())
                    .find(|window| window.handle == *handle)
                    .map(|window| window.title.to_owned())
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn run_item(&mut self, item: &Item) -> Result<()> {
        if let ItemKind::Snippet { text, mode } = &item.kind {
            return snippet::deliver(text, *mode)
                .with_context(|| format!("could not paste \"{}\"", item.name));
        }

        launch::run(&item.kind).with_context(|| format!("could not run \"{}\"", item.name))
    }

    fn focus_or_minimize(&self, window: HWND) -> Result<()> {
        if self.config.general.minimize_if_focused && activate::is_foreground(window) {
            activate::minimize(window);
            return Ok(());
        }

        activate::focus(window).context("could not bring the window forward")
    }

    fn launch_new_instance(&mut self) {
        let Some(slot) = self.state.selected() else {
            return;
        };
        let Some(entry) = self.layout.get(self.state.tab(), slot).cloned() else {
            return;
        };
        let SlotTarget::Pinned(item) = &entry.target else {
            return;
        };

        let item = item.clone();
        self.close();
        log_failure(self.run_item(&item));
    }

    fn reload_config(&mut self) -> Result<()> {
        let reloaded = kile_wheel_core::config::load(&self.paths.config_file())
            .context("the saved config could not be read back")?;

        self.shared.replace_bindings(reloaded.binding_table()?);
        self.config = reloaded;
        self.layout = Layout::build(&self.config, &self.model, &mut self.memory);
        self.redraw = true;
        Ok(())
    }
}

/// A slot the config marked unpinned, rebuilt so the form can edit it.
fn unpinned_item(tab: Tab, slot: Slot) -> Item {
    Item {
        name: "Open".to_owned(),
        icon: None,
        tab: tab.number(),
        slot: slot.index(),
        kind: ItemKind::UnpinnedApp,
    }
}

/// The mini glyph that marks what a slice is.
fn glyph_for(kind: &ItemKind) -> &'static str {
    theme::TYPE_GLYPHS[kind.type_index()]
}

/// Turns a running app into a pinned entry, which is what dragging one does.
///
/// Packaged apps are pinned through the Apps folder, because their executable path is not
/// what the shell launches them by.
fn pin_as_item(entry: &SlotEntry, to: Position) -> Option<Item> {
    let SlotTarget::Running(key) = &entry.target else {
        return None;
    };

    let target = match key {
        AppKey::Executable(path) => path.to_string_lossy().into_owned(),
        AppKey::UserModelId(id) => format!(r"shell:AppsFolder\{id}"),
    };

    Some(Item {
        name: entry.label.clone(),
        icon: None,
        tab: to.tab.number(),
        slot: to.slot.index(),
        kind: ItemKind::App { target },
    })
}

/// For log lines, so a move reads as the tab and slot you dragged between.
fn describe(position: Position) -> String {
    format!(
        "tab {} slot {}",
        position.tab.number(),
        position.slot.index()
    )
}

/// An empty slot shows the same quip for as long as the wheel stays on that tab, so the
/// text does not flicker while you aim.
fn quip(quips: &[String], tab: Tab, slot: Slot) -> &str {
    if quips.is_empty() {
        return "";
    }

    let spread = tab.number() * Slot::COUNT + slot.index();
    quips[spread % quips.len()].as_str()
}

/// Says how many of an app's windows did not fit on the sub-wheel, so a ninth window is
/// never dropped without a word. `None` when they all fit.
fn hidden_windows_note(hidden: usize) -> Option<String> {
    if hidden == 0 {
        return None;
    }

    let plural = match hidden {
        1 => "window",
        _ => "windows",
    };
    Some(format!(
        "{hidden} more {plural} open. Alt-Tab reaches the rest."
    ))
}

/// Says what the overflow glyph means, since a glyph on its own does not explain itself.
/// `None` when nothing overflowed, which is also when the glyph is not drawn.
fn overflow_note(overflow: usize) -> Option<String> {
    if overflow == 0 {
        return None;
    }

    let plural = match overflow {
        1 => "app has",
        _ => "apps have",
    };
    Some(format!(
        "{overflow} {plural} nowhere to land. Add unpinned app slots, or close a few."
    ))
}

/// Maps a number key to its tab. The top row reads 1 through 9 then 0, so 0 is the
/// tenth tab rather than a zeroth one.
fn tab_number(code: KeyCode) -> Option<usize> {
    if code == keys::DIGIT_0 {
        return Some(Tab::MAX_COUNT);
    }

    let offset = code.0.checked_sub(keys::DIGIT_1.0)?;
    let highest = keys::DIGIT_9.0 - keys::DIGIT_1.0;

    (offset <= highest).then_some(offset as usize + 1)
}

fn choice(config: &Config) -> MonitorChoice {
    match config.wheel.monitor {
        MonitorTarget::Cursor => MonitorChoice::UnderCursor,
        MonitorTarget::Primary => MonitorChoice::Primary,
    }
}

fn centre_cursor(monitor: Monitor) {
    let centre: POINT = monitor.center();

    // SAFETY: moves the cursor only. The move it generates is flagged injected, so our
    // own mouse hook ignores it and the selection does not jump.
    let _ = unsafe { SetCursorPos(centre.x, centre.y) };
}

/// The open budget is the whole point of the design, so every open is measured and a miss
/// is loud.
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

/// Keeps the message loop alive. A failed draw, launch or save is worth a log line, not
/// the whole launcher going away.
fn log_failure(outcome: Result<()>) {
    if let Err(error) = outcome {
        warn!("{error:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_keys_one_to_nine_map_to_tabs() {
        assert_eq!(tab_number(keys::DIGIT_1), Some(1));
        assert_eq!(tab_number(keys::DIGIT_9), Some(9));
    }

    /// The top row reads 1 through 9 then 0, so 0 is the tenth tab, not a zeroth one.
    #[test]
    fn zero_maps_to_the_last_tab() {
        assert_eq!(tab_number(keys::DIGIT_0), Some(Tab::MAX_COUNT));
        assert_eq!(Tab::MAX_COUNT, 10);
    }

    #[test]
    fn every_number_key_reaches_a_real_tab() {
        for code in [keys::DIGIT_0, keys::DIGIT_1, keys::DIGIT_9] {
            let number = tab_number(code).expect("a number key maps to a tab");
            assert!(Tab::from_number(number).is_some(), "tab {number}");
        }
    }

    #[test]
    fn letters_and_enter_are_not_tab_keys() {
        assert_eq!(tab_number(keys::W), None);
        assert_eq!(tab_number(keys::ENTER), None);
    }

    #[test]
    fn a_quip_is_stable_for_a_given_slot() {
        let quips: Vec<String> = ["a", "b", "c"].into_iter().map(str::to_owned).collect();
        let slot = Slot::new(3).expect("slot 3");

        assert_eq!(
            quip(&quips, Tab::FIRST, slot),
            quip(&quips, Tab::FIRST, slot)
        );
    }

    #[test]
    fn neighbouring_slots_get_different_quips() {
        let quips: Vec<String> = ["a", "b", "c"].into_iter().map(str::to_owned).collect();
        let first = quip(&quips, Tab::FIRST, Slot::new(0).expect("slot 0"));
        let second = quip(&quips, Tab::FIRST, Slot::new(1).expect("slot 1"));

        assert_ne!(first, second);
    }

    #[test]
    fn an_empty_quip_list_draws_nothing_rather_than_panicking() {
        assert_eq!(quip(&[], Tab::FIRST, Slot::new(0).expect("slot 0")), "");
    }

    #[test]
    fn nothing_overflowing_says_nothing() {
        assert_eq!(overflow_note(0), None);
    }

    #[test]
    fn one_overflowing_app_reads_as_singular() {
        let note = overflow_note(1).expect("one app overflowed");
        assert!(note.starts_with("1 app has"), "{note}");
    }

    #[test]
    fn several_overflowing_apps_read_as_plural() {
        let note = overflow_note(3).expect("three apps overflowed");
        assert!(note.starts_with("3 apps have"), "{note}");
    }

    #[test]
    fn the_overflow_note_says_what_to_do_about_it() {
        let note = overflow_note(2).expect("two apps overflowed");
        assert!(note.contains("unpinned app slots"), "{note}");
    }

    #[test]
    fn windows_that_all_fit_say_nothing() {
        assert_eq!(hidden_windows_note(0), None);
    }

    #[test]
    fn one_hidden_window_reads_as_singular() {
        let note = hidden_windows_note(1).expect("one window hidden");
        assert!(note.starts_with("1 more window "), "{note}");
    }

    #[test]
    fn several_hidden_windows_read_as_plural() {
        let note = hidden_windows_note(4).expect("four windows hidden");
        assert!(note.starts_with("4 more windows "), "{note}");
    }
}
