//! Delivering a snippet into the window that had focus. The overlay never takes focus, so
//! by the time the wheel has hidden itself that window is already the foreground one and
//! no activation call is needed.

use kile_wheel_core::item::SnippetMode;
use std::thread;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

use crate::clipboard::{self, ClipboardError};
use crate::send::{self, SendError};

/// How long the paste target gets to read the clipboard before the old contents go back.
/// A paste is asynchronous, so restoring immediately would race the reader.
const RESTORE_DELAY_MS: u64 = 220;

#[derive(Debug, Error)]
pub enum SnippetError {
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),

    #[error(transparent)]
    Send(#[from] SendError),
}

/// Typing is the fallback-free path: it needs no clipboard and no delay, but it is slow
/// for long text and loses anything a target maps to a literal keystroke.
pub fn deliver(text: &str, mode: SnippetMode) -> Result<(), SnippetError> {
    if text.is_empty() {
        return Ok(());
    }

    match mode {
        SnippetMode::Type => Ok(send::text(text)?),
        SnippetMode::Clipboard => paste(text),
    }
}

fn paste(text: &str) -> Result<(), SnippetError> {
    let previous = clipboard::read_text();

    clipboard::write_text(text)?;
    send::paste()?;

    restore_later(text.to_owned(), previous);
    Ok(())
}

/// Restores on a thread rather than inline, so the wheel's message loop stays responsive
/// for the next hotkey instead of blocking out the delay.
fn restore_later(snippet: String, previous: Option<String>) {
    let restore = thread::Builder::new()
        .name("clipboard-restore".to_owned())
        .spawn(move || {
            thread::sleep(Duration::from_millis(RESTORE_DELAY_MS));
            restore(&snippet, previous.as_deref());
        });

    if let Err(error) = restore {
        warn!("the clipboard keeps the snippet, no thread to restore it: {error}");
    }
}

/// Only puts the old contents back if the snippet is still there. Anything else means the
/// user copied something in the meantime, and that is theirs to keep.
fn restore(snippet: &str, previous: Option<&str>) {
    if clipboard::read_text().as_deref() != Some(snippet) {
        debug!("the clipboard moved on, so the snippet was left alone");
        return;
    }

    let Some(previous) = previous else {
        debug!("there was nothing on the clipboard before, so the snippet stays");
        return;
    };

    if let Err(error) = clipboard::write_text(previous) {
        warn!("could not put the clipboard back: {error:#}");
    }
}
