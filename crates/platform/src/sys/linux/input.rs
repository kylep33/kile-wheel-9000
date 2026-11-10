//! Every keyboard and mouse, grabbed through evdev and replayed through one virtual device.
//!
//! Wayland lets no app see global input, so the wheel reads the devices underneath the
//! compositor. Grabbing them is what makes swallowing possible: the compositor only sees
//! what this thread passes on. Touchpads, tablets and touchscreens are left alone,
//! because their absolute axes cannot be replayed through a relative device.
//!
//! The pointer position comes from the shell instead, because evdev only knows how far a
//! mouse moved, not where the cursor is.

use std::collections::HashMap;
use std::os::fd::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use evdev::{Device, EventSummary, EventType, InputEvent, KeyCode, RelativeAxisCode};
use kile_wheel_core::binding::{Modifiers, MouseButton};
use tracing::{debug, info, warn};

use super::{keymap, shell, uinput};
use crate::input::{self, InstallError, SharedInput};

/// New devices are looked for this often, so a mouse plugged in later still works.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// Win32 reports one scroll notch as 120, which is what the wheel expects.
const NOTCH: i16 = 120;

const KEY_RELEASED: i32 = 0;

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The input threads run for the process lifetime; the kernel drops the grabs on exit.
#[derive(Debug)]
pub struct Capture {
    _private: (),
}

impl Capture {
    pub fn install(shared: Arc<SharedInput>) -> Result<Self, InstallError> {
        if INSTALLED.swap(true, Ordering::AcqRel) {
            return Err(InstallError::AlreadyInstalled);
        }
        shell::check().map_err(InstallError::Refused)?;
        // created now so a missing udev rule is reported at startup, not at first key
        drop(uinput::device().map_err(InstallError::Refused)?);

        let devices = Devices::scan();
        if devices.is_empty() {
            return Err(InstallError::Refused(
                "no keyboard or mouse under /dev/input could be opened; add yourself to the input group with `sudo usermod -aG input $USER`, then log out and back in"
                    .to_owned(),
            ));
        }

        let keys = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("input-evdev".to_owned())
            .spawn(move || run(devices, &keys))
            .map_err(|_| InstallError::ThreadGone)?;

        follow_pointer(shared)?;
        Ok(Self { _private: () })
    }
}

/// Every key passes through this thread, so nothing is held that it did not see. A key
/// already down when the wheel started is the one gap, and a release fixes it.
pub fn os_modifiers() -> Modifiers {
    Modifiers::default()
}

fn follow_pointer(shared: Arc<SharedInput>) -> Result<(), InstallError> {
    let moves = shell::proxy()
        .and_then(|proxy| {
            proxy
                .receive_signal("PointerMoved")
                .map_err(|error| format!("could not follow the pointer: {error}"))
        })
        .map_err(InstallError::Refused)?;

    std::thread::Builder::new()
        .name("input-pointer".to_owned())
        .spawn(move || {
            for message in moves {
                if let Some((x, y)) = shell::signal_body::<(i32, i32)>(&message) {
                    input::on_move(&shared, x, y);
                }
            }
        })
        .map(drop)
        .map_err(|_| InstallError::ThreadGone)
}

/// The grabbed devices, by path.
struct Devices {
    open: HashMap<PathBuf, Device>,
}

impl Devices {
    fn scan() -> Self {
        let mut devices = Self {
            open: HashMap::new(),
        };
        devices.add_new();
        devices
    }

    fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    fn add_new(&mut self) {
        for (path, mut device) in evdev::enumerate() {
            if self.open.contains_key(&path) || !wanted(&device) {
                continue;
            }
            if let Err(error) = device.grab() {
                warn!("could not grab {}: {error}", path.display());
                continue;
            }
            info!(
                device = device.name().unwrap_or("unnamed"),
                path = %path.display(),
                "grabbed input device"
            );
            self.open.insert(path, device);
        }
    }
}

/// Keyboards and plain mice only, and never our own virtual device.
fn wanted(device: &Device) -> bool {
    if device.name() == Some(uinput::DEVICE_NAME) {
        return false;
    }
    let absolute = device
        .supported_absolute_axes()
        .is_some_and(|axes| axes.iter().next().is_some());
    if absolute {
        return false;
    }

    let keys = device.supported_keys();
    let keyboard = keys.is_some_and(|keys| keys.contains(KeyCode::KEY_A));
    let mouse = keys.is_some_and(|keys| keys.contains(KeyCode::BTN_LEFT))
        && device
            .supported_relative_axes()
            .is_some_and(|axes| axes.contains(RelativeAxisCode::REL_X));
    keyboard || mouse
}

fn run(mut devices: Devices, shared: &SharedInput) {
    let mut rescanned = Instant::now();
    loop {
        if rescanned.elapsed() >= RESCAN_INTERVAL {
            devices.add_new();
            rescanned = Instant::now();
        }

        let ready = wait_for_input(&devices);
        for path in ready {
            let Some(device) = devices.open.get_mut(&path) else {
                continue;
            };
            if let Err(error) = pump(device, shared) {
                // unplugged, most likely; a replug is picked up by the next rescan
                debug!("dropping {}: {error}", path.display());
                devices.open.remove(&path);
            }
        }
    }
}

/// The devices with events waiting, or none once the rescan interval is up.
fn wait_for_input(devices: &Devices) -> Vec<PathBuf> {
    let mut entries: Vec<(PathBuf, RawFd)> = devices
        .open
        .iter()
        .map(|(path, device)| (path.clone(), device.as_raw_fd()))
        .collect();
    let mut polled: Vec<libc::pollfd> = entries
        .iter()
        .map(|(_, fd)| libc::pollfd {
            fd: *fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();

    let timeout_ms = RESCAN_INTERVAL.as_millis() as libc::c_int;
    // SAFETY: the slice is live for the call and its length is passed with it.
    let ready = unsafe { libc::poll(polled.as_mut_ptr(), polled.len() as libc::nfds_t, timeout_ms) };
    if ready <= 0 {
        if ready < 0 {
            // with nothing grabbed poll has nothing to wait on, so pace the rescans
            std::thread::sleep(RESCAN_INTERVAL);
        }
        return Vec::new();
    }

    entries
        .drain(..)
        .zip(polled)
        .filter(|(_, poll)| poll.revents != 0)
        .map(|((path, _), _)| path)
        .collect()
}

/// Reads one batch, decides on each event, and replays what was not swallowed.
fn pump(device: &mut Device, shared: &SharedInput) -> std::io::Result<()> {
    let mut passed = Vec::new();
    for event in device.fetch_events()? {
        if !swallows(event, shared) && event.event_type() != EventType::SYNCHRONIZATION {
            passed.push(event);
        }
    }

    if passed.is_empty() {
        return Ok(());
    }
    if let Err(error) = uinput::emit(&passed) {
        warn!("{error}");
    }
    Ok(())
}

fn swallows(event: InputEvent, shared: &SharedInput) -> bool {
    match event.destructure() {
        EventSummary::Key(_, code, value) => on_key(code, value, shared),
        EventSummary::RelativeAxis(_, axis, value) => on_relative(axis, value, shared),
        _ => false,
    }
}

fn on_key(code: KeyCode, value: i32, shared: &SharedInput) -> bool {
    // auto-repeat counts as a press, as it does on Windows
    let pressed = value != KEY_RELEASED;

    if let Some(button) = mouse_button(code) {
        return input::on_button(shared, button, pressed);
    }
    match keymap::to_vk(code.code()) {
        Some(key) => input::on_key(shared, key, pressed),
        None => false,
    }
}

fn on_relative(axis: RelativeAxisCode, value: i32, shared: &SharedInput) -> bool {
    match axis {
        RelativeAxisCode::REL_WHEEL => input::on_scroll(shared, (value as i16).saturating_mul(NOTCH)),
        // the high-resolution twin and sideways scrolling go wherever the notches go
        RelativeAxisCode::REL_WHEEL_HI_RES
        | RelativeAxisCode::REL_HWHEEL
        | RelativeAxisCode::REL_HWHEEL_HI_RES => shared.is_wheel_open(),
        _ => false,
    }
}

fn mouse_button(code: KeyCode) -> Option<MouseButton> {
    let button = match code {
        KeyCode::BTN_LEFT => MouseButton::Left,
        KeyCode::BTN_RIGHT => MouseButton::Right,
        KeyCode::BTN_MIDDLE => MouseButton::Middle,
        KeyCode::BTN_SIDE => MouseButton::X1,
        KeyCode::BTN_EXTRA => MouseButton::X2,
        _ => return None,
    };
    Some(button)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_buttons_are_x1_and_x2() {
        assert_eq!(mouse_button(KeyCode::BTN_SIDE), Some(MouseButton::X1));
        assert_eq!(mouse_button(KeyCode::BTN_EXTRA), Some(MouseButton::X2));
        assert_eq!(mouse_button(KeyCode::KEY_A), None);
    }

    #[test]
    fn scrolling_only_stops_while_the_wheel_is_open() {
        let shared = SharedInput::default();
        assert!(!on_relative(RelativeAxisCode::REL_WHEEL, 1, &shared));
        assert!(!on_relative(RelativeAxisCode::REL_X, 5, &shared));

        shared.set_wheel_open(true);
        assert!(on_relative(RelativeAxisCode::REL_WHEEL, 1, &shared));
        assert!(on_relative(RelativeAxisCode::REL_WHEEL_HI_RES, 120, &shared));
        assert!(!on_relative(RelativeAxisCode::REL_X, 5, &shared));
    }
}
