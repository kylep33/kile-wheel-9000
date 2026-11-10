//! The wheel's pixels go to the shell through a file in the runtime folder, which is in
//! memory, and the shell draws them above every window. A Wayland client window could
//! not be placed, kept on top, or kept from taking focus, so the shell has to do it.

use std::io::Write;
use std::path::PathBuf;

use super::shell;
use crate::geom::ScreenPoint;
use crate::overlay::{FramePixels, OverlayError};

const FRAME_FILE: &str = "kile-wheel-9000-frame.rgba";

/// What the shell needs to show the last frame written.
#[derive(Debug, Clone, Copy)]
struct Placed {
    origin: ScreenPoint,
    size_units: (i32, i32),
    size_px: (u32, u32),
}

#[derive(Debug)]
pub struct Surface {
    path: PathBuf,
    last: Option<Placed>,
    visible: bool,
}

impl Surface {
    pub fn create() -> Result<Self, OverlayError> {
        shell::check().map_err(OverlayError)?;
        let folder = dirs::runtime_dir().ok_or_else(|| {
            OverlayError(
                "XDG_RUNTIME_DIR is not set, so there is nowhere in memory to hand frames to the shell; start the wheel from the desktop session"
                    .to_owned(),
            )
        })?;

        Ok(Self {
            path: folder.join(FRAME_FILE),
            last: None,
            visible: false,
        })
    }

    /// Written while hidden too, so the first show already has the right frame.
    pub fn present(&mut self, frame: &FramePixels<'_>) -> Result<(), OverlayError> {
        self.write(frame).map_err(|error| {
            OverlayError(format!("could not write the frame to {}: {error}", self.path.display()))
        })?;

        self.last = Some(Placed {
            origin: frame.origin,
            size_units: frame.size_units(),
            size_px: (frame.region_width, frame.region_height),
        });
        match self.visible {
            true => self.send(),
            false => Ok(()),
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        if let Err(error) = self.send() {
            tracing::warn!("{error}");
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        if let Err(error) = shell::call::<_, ()>("HideFrame", &()) {
            tracing::warn!("could not hide the wheel: {error}");
        }
    }

    fn write(&self, frame: &FramePixels<'_>) -> std::io::Result<()> {
        let mut file = std::io::BufWriter::new(std::fs::File::create(&self.path)?);
        for row in 0..frame.region_height {
            file.write_all(frame.row(row))?;
        }
        file.flush()
    }

    fn send(&self) -> Result<(), OverlayError> {
        let Some(placed) = self.last else {
            return Ok(());
        };
        let path = self.path.to_string_lossy();

        shell::call::<_, ()>(
            "ShowFrame",
            &(
                path.as_ref(),
                placed.origin.x,
                placed.origin.y,
                placed.size_units.0,
                placed.size_units.1,
                placed.size_px.0 as i32,
                placed.size_px.1 as i32,
            ),
        )
        .map_err(OverlayError)
    }
}
