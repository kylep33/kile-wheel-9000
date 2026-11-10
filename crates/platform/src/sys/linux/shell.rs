//! The kile-wheel-9000 GNOME Shell extension, over the session bus.
//!
//! GNOME on Wayland gives an ordinary app no window list, no focus control, no global
//! pointer and no always-on-top surface. The extension does those inside the shell and
//! exports them here.

use std::sync::OnceLock;

use serde::Serialize;
use zbus::blocking::{Connection, Proxy};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{DynamicDeserialize, DynamicType};

/// The extension's objects ride on the shell's own connection.
const DESTINATION: &str = "org.gnome.Shell";
const PATH: &str = "/dev/kile/Wheel9000";
const INTERFACE: &str = "dev.kile.Wheel9000";

/// Must match `PROTOCOL_VERSION` in the extension.
const PROTOCOL_VERSION: u32 = 1;

const EXTENSION_UUID: &str = "kile-wheel-9000@kile.dev";

static PROXY: OnceLock<Result<Proxy<'static>, String>> = OnceLock::new();

pub fn proxy() -> Result<&'static Proxy<'static>, String> {
    PROXY.get_or_init(connect).as_ref().map_err(Clone::clone)
}

fn connect() -> Result<Proxy<'static>, String> {
    let connection = Connection::session().map_err(|error| {
        format!("could not reach the session bus: {error}; the wheel has to run inside a GNOME session")
    })?;

    zbus::blocking::proxy::Builder::new(&connection)
        .destination(DESTINATION)
        .and_then(|builder| builder.path(PATH))
        .and_then(|builder| builder.interface(INTERFACE))
        .map(|builder| builder.cache_properties(CacheProperties::No))
        .and_then(|builder| builder.build())
        .map_err(|error| format!("could not set up the call to GNOME Shell: {error}"))
}

/// One method call. The error names the extension, because an unreachable extension is
/// by far the likeliest cause.
pub fn call<B, R>(method: &str, body: &B) -> Result<R, String>
where
    B: Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    proxy()?
        .call(method, body)
        .map_err(|error| unreachable_extension(method, &error))
}

/// Run once at startup, so a missing or stale extension is reported up front rather than
/// as the first thing that fails.
pub fn check() -> Result<(), String> {
    let version: u32 = call("Version", &())?;
    if version != PROTOCOL_VERSION {
        return Err(format!(
            "the {EXTENSION_UUID} GNOME Shell extension speaks version {version} but this wheel needs {PROTOCOL_VERSION}; re-run install.sh, then log out and back in"
        ));
    }
    Ok(())
}

/// The body of a signal, which zbus hands over as a raw message.
pub fn signal_body<T>(message: &zbus::Message) -> Option<T>
where
    T: for<'d> DynamicDeserialize<'d>,
{
    message.body().deserialize::<T>().ok()
}

fn unreachable_extension(method: &str, error: &zbus::Error) -> String {
    format!(
        "GNOME Shell did not answer {method} ({error}); check the {EXTENSION_UUID} extension is installed and enabled with `gnome-extensions info {EXTENSION_UUID}`, then log out and back in"
    )
}
