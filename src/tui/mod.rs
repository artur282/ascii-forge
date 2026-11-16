//! Interactive terminal preview.
//!
//! The preview renders a scene with `ratatui` and `crossterm`. `frame_to_buffer`
//! is the pure conversion from a composed frame to a ratatui buffer; `run_preview`
//! owns the terminal lifecycle and falls back to printing one frame when stdout
//! is not a terminal.

mod app;
mod buffer;
mod color;

use std::io::{IsTerminal, Write};

use thiserror::Error;

use crate::render::raster;
use crate::scene::Scene;
use crate::scene::compose::{ComposeError, compose_frame};

pub use buffer::frame_to_buffer;
pub use color::{ColorMode, color_mode_for, detect_color_mode};

/// Errors produced while previewing a scene.
#[derive(Debug, Error)]
pub enum PreviewError {
    /// Writing the fallback frame to stdout failed.
    #[error("preview failed to write output: {0}")]
    Io(#[from] std::io::Error),

    /// A frame could not be composed.
    #[error(transparent)]
    Compose(#[from] ComposeError),

    /// The terminal could not be initialized, driven or restored.
    #[error("preview terminal error: {0}")]
    Terminal(String),
}

/// Preview a scene in the terminal, starting at `start_frame`.
///
/// When stdout is not a terminal the requested frame is printed once and the
/// function returns without touching terminal modes. `no_color` forces plain
/// text in that fallback and `ColorMode::NoColor` in the interactive preview.
pub fn run_preview(scene: &Scene, start_frame: u32, no_color: bool) -> Result<(), PreviewError> {
    let mode = detect_color_mode(no_color);

    if !std::io::stdout().is_terminal() {
        let frame = compose_frame(scene, start_frame)?;
        let text = match mode {
            ColorMode::NoColor => raster::frame_to_text(&frame),
            ColorMode::TrueColor | ColorMode::Ansi256 | ColorMode::Ansi16 => {
                raster::frame_to_ansi(&frame, true)
            }
        };
        let mut stdout = std::io::stdout();
        stdout.write_all(text.as_bytes())?;
        stdout.flush()?;
        return Ok(());
    }

    app::run_interactive(scene, start_frame, mode)
}
