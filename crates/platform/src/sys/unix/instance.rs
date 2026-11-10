//! Single instance through a Unix socket: whoever binds it is the running instance, and a
//! second launch connects to it and asks for the wheel.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use tracing::warn;

use crate::event_loop::{Notice, Waker};
use crate::instance::{Claim, InstanceError, InstanceGuard};

const OPEN_REQUEST: &str = "open";

#[derive(Debug)]
pub struct Guard {
    listener: Option<UnixListener>,
    path: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Guard {
    pub fn listen(&mut self, waker: Waker) -> Result<(), InstanceError> {
        let Some(listener) = self.listener.take() else {
            return Err(InstanceError("already listening for a second launch".to_owned()));
        };

        std::thread::Builder::new()
            .name("instance".to_owned())
            .spawn(move || accept(&listener, &waker))
            .map(drop)
            .map_err(|error| {
                InstanceError(format!("could not start the hand-over thread: {error}"))
            })
    }
}

pub fn claim(app_name: &str) -> Result<Claim, InstanceError> {
    let path = socket_path(app_name);
    if UnixStream::connect(&path).is_ok() {
        return Ok(Claim::AlreadyRunning);
    }

    // nobody answered, so whatever is there was left by a crash
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).map_err(|error| {
        InstanceError(format!(
            "could not create {}: {error}; check the folder is writable",
            path.display()
        ))
    })?;

    Ok(Claim::Acquired(InstanceGuard::from_sys(Guard {
        listener: Some(listener),
        path,
    })))
}

pub fn signal_existing(app_name: &str) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path(app_name)) else {
        return false;
    };
    writeln!(stream, "{OPEN_REQUEST}").is_ok()
}

/// Per user: the runtime dir on Linux, the per-user temp dir on macOS.
fn socket_path(app_name: &str) -> PathBuf {
    let folder = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
    folder.join(format!("{app_name}.sock"))
}

fn accept(listener: &UnixListener, waker: &Waker) {
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                warn!("a second launch could not hand over: {error}");
                continue;
            }
        };
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == OPEN_REQUEST {
            waker.send(Notice::OpenWheel);
        }
    }
}
