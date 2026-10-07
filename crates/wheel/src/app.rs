//! The running wheel: owns the window, the model, and the message loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Local;
use kile_wheel_core::binding::{Action, KeyCode, MouseButton, keys};
use kile_wheel_core::config::{Config, ConfigDocument, MonitorTarget};
use kile_wheel_core::item::{Item, ItemKind, Position};
use kile_wheel_core::layout::SlotMemory;
use kile_wheel_core::names::{ConfigPaths, SETTINGS_EXE};
use kile_wheel_core::select::{self, Offset, latch};
use kile_wheel_core::slot::{Slot, Tab};
use kile_wheel_platform::apps::{AppKey, WindowModel};
use kile_wheel_platform::hooks::{self, HookThread, InputEvent, SharedInput};
use kile_wheel_platform::icons::IconCache;
use kile_wheel_platform::monitor::{self, Monitor, MonitorChoice};
use kile_wheel_platform::watcher::{Watcher, WindowEvent};
use kile_wheel_platform::{activate, launch, shell};
use tracing::{info, warn};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SetCursorPos, WM_TIMER,
};

use crate::overlay::{self, Overlay};
use crate::render::{Frame, HubView, SlotView};
use crate::slots::{Layout, MORE_LABEL, SlotEntry, SlotTarget};
use crate::state::{Back, Mode, WheelState, WindowChoice};

/// Hotkey press to wheel visible. Spec requirement, not a nice to have.
const OPEN_BUDGET: Duration = Duration::from_millis(50);

/// Hold this long on a slot or the cog to toggle edit mode.
const EDIT_HOLD_MS: u32 = 400;

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
    state: WheelState,
    memory: SlotMemory<AppKey>,
    layout: Layout,
    icons: IconCache,
    paths: ConfigPaths,
    config: Config,
    press: Option<Press>,
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

        let mut app = Self {
            _hooks: hooks,
            shared,
            overlay,
            model,
            watcher,
            state: WheelState::default(),
            memory: SlotMemory::default(),
            layout: Layout::default(),
            icons: IconCache::default(),
            paths,
            config,
            press: None,
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
            self.hold_elapsed();
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
                pressed,
            } => self.handle_left_button(pressed),
            InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: true,
            } => self.launch_new_instance(),
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
            (Action::Open, true) if self.config.wheel.open_hotkey_closes => self.close(),
            (Action::Close | Action::Toggle, true) => self.close(),
            (Action::Open, true) | (Action::Close, false) => {}
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        if !self.state.is_open() {
            return;
        }

        // esc always closes everything, wherever it is pressed
        if code == keys::ESC {
            self.close();
            return;
        }
        if code == keys::BACKSPACE {
            self.go_back();
            return;
        }
        if code == keys::ENTER {
            let activated = self.activate();
            log_failure(activated);
            return;
        }

        if let Some(number) = tab_number(code) {
            self.switch_tab(number);
            return;
        }

        let Some(cardinal) = latch::cardinal_for(code) else {
            return;
        };
        self.state.press_direction(cardinal);
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

        if slot.is_some() || on_cog {
            self.overlay.start_hold_timer(EDIT_HOLD_MS);
        }
        if let Some(slot) = slot.filter(|_| self.state.edit_mode()) {
            self.state.begin_drag(slot);
            self.redraw = true;
        }
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

    fn finish_drag(&mut self) {
        let Some(from_slot) = self.state.end_drag() else {
            return;
        };
        self.redraw = true;

        let Some(to_slot) = self.state.selected() else {
            return;
        };
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

        // only a pinned item has a config entry to move; a running app has nowhere to
        // write its slot to, so dragging one pins it first
        if !document.move_item(from, to) {
            let Some(entry) = self.layout.get(from.tab, from.slot) else {
                return Ok(());
            };
            let Some(item) = pin_as_item(entry, to) else {
                return Ok(());
            };
            document.upsert_item(&item)?;
        }

        document.save().context("could not save the moved item")?;
        self.reload_config()
    }

    fn open_settings(&mut self) -> Result<()> {
        self.close();

        let exe = std::env::current_exe()
            .context("could not find our own folder to look for the settings window in")?
            .with_file_name(SETTINGS_EXE);

        shell::execute(&shell::Command::new(exe.to_string_lossy().into_owned()))
            .context("could not start the settings window")
    }

    fn aim(&mut self, x: i32, y: i32) {
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
        self.state.open(self.layout.tab_count());
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
        Ok(())
    }

    fn close(&mut self) {
        if !self.state.is_open() {
            return;
        }

        self.overlay.stop_hold_timer();
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

    fn draw_wheel(&self) -> Result<()> {
        let tab = self.state.tab();
        let entries: Vec<Option<&SlotEntry>> =
            Slot::all().map(|slot| self.layout.get(tab, slot)).collect();
        let quip_for = |slot: Slot| quip(&self.config.wheel.empty_quips, tab, slot);

        let slots = std::array::from_fn(|index| {
            let slot = Slot::new(index).expect("index comes from Slot::all");
            match entries[index] {
                Some(entry) => SlotView {
                    label: &entry.label,
                    icon_key: entry.icon_source.as_deref(),
                    running: entry.is_running(),
                    focused: entry.focused,
                    window_count: entry.windows.len(),
                    pinned: entry.pinned,
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

        self.overlay.draw(&Frame {
            slots,
            selected: self.state.selected(),
            hub: HubView {
                tab_line: &self.tab_line(tab),
                clock: show_clock.then_some(clock.as_str()),
                date: show_clock.then_some(date.as_str()),
            },
            edit_mode: self.state.edit_mode(),
            dragging: self.state.dragging(),
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

        self.overlay.draw(&Frame {
            slots,
            selected: self.state.selected(),
            hub: HubView {
                tab_line: "WINDOWS",
                clock: None,
                date: None,
            },
            edit_mode: false,
            dragging: None,
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
        if matches!(entry.target, SlotTarget::More) {
            // the overflow sub-wheel lands in phase 3's follow-up; closing beats a no-op
            self.close();
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
                    .unwrap_or_else(|| MORE_LABEL.to_owned()),
            })
            .collect()
    }

    fn run_item(&mut self, item: &Item) -> Result<()> {
        if let ItemKind::Snippet { text, mode } = &item.kind {
            // snippets land in the window that had focus, which needs the paste path
            warn!(
                name = %item.name,
                mode = ?mode,
                length = text.len(),
                "snippet delivery is not wired up yet, so nothing was pasted"
            );
            return Ok(());
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

/// Turns a running app into a pinned entry, which is what dragging one does.
fn pin_as_item(entry: &SlotEntry, to: Position) -> Option<Item> {
    let SlotTarget::Running(AppKey::Executable(path)) = &entry.target else {
        return None;
    };

    Some(Item {
        name: entry.label.clone(),
        icon: None,
        tab: to.tab.number(),
        slot: to.slot.index(),
        kind: ItemKind::App {
            target: path.to_string_lossy().into_owned(),
        },
    })
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

fn tab_number(code: KeyCode) -> Option<usize> {
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

    #[test]
    fn zero_and_letters_are_not_tab_keys() {
        assert_eq!(tab_number(KeyCode(b'0' as u16)), None);
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
}
