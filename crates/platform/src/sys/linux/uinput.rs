//! The one virtual input device: every key and pointer event the real devices produce
//! goes back out through it unless the wheel swallowed it, and synthesised keystrokes go
//! out through it too.

use std::sync::{Mutex, MutexGuard, OnceLock};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, InputEvent, KeyCode, RelativeAxisCode};

/// How the input thread tells its own device apart from the real ones.
pub const DEVICE_NAME: &str = "kile-wheel-9000 virtual input";

/// Keyboard keys, mouse buttons, and the extra keys past the buttons. The joystick and
/// digitiser ranges are left out, because libinput would take the device for a tablet.
const KEY_RANGES: [(u16, u16); 3] = [(1, 0xFF), (0x110, 0x117), (0x160, 0x2FF)];

const RELATIVE_AXES: [RelativeAxisCode; 6] = [
    RelativeAxisCode::REL_X,
    RelativeAxisCode::REL_Y,
    RelativeAxisCode::REL_WHEEL,
    RelativeAxisCode::REL_HWHEEL,
    RelativeAxisCode::REL_WHEEL_HI_RES,
    RelativeAxisCode::REL_HWHEEL_HI_RES,
];

static DEVICE: OnceLock<Result<Mutex<VirtualDevice>, String>> = OnceLock::new();

/// Creates the device on first use.
pub fn device() -> Result<MutexGuard<'static, VirtualDevice>, String> {
    let device = DEVICE.get_or_init(|| create().map(Mutex::new));
    match device {
        Ok(device) => Ok(device.lock().unwrap_or_else(|poisoned| poisoned.into_inner())),
        Err(error) => Err(error.clone()),
    }
}

/// One batch, closed with a sync report so it lands as one moment.
pub fn emit(events: &[InputEvent]) -> Result<(), String> {
    device()?
        .emit(events)
        .map_err(|error| format!("the virtual input device refused events: {error}"))
}

fn create() -> Result<VirtualDevice, String> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for (first, last) in KEY_RANGES {
        for code in first..=last {
            keys.insert(KeyCode::new(code));
        }
    }
    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    for axis in RELATIVE_AXES {
        axes.insert(axis);
    }

    VirtualDevice::builder()
        .and_then(|builder| builder.name(DEVICE_NAME).with_keys(&keys))
        .and_then(|builder| builder.with_relative_axes(&axes))
        .and_then(|builder| builder.build())
        .map_err(|error| {
            format!(
                "could not create a virtual input device through /dev/uinput: {error}; install.sh adds a udev rule that lets the input group use it, so re-run it and log out and back in"
            )
        })
}
