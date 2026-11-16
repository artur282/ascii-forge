//! Interactive terminal loop: setup, playback and teardown.

use std::io::Write;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::scene::compose::compose_frame;
use crate::scene::{Frame, Scene};
use crate::tui::PreviewError;
use crate::tui::buffer::frame_to_buffer;
use crate::tui::color::ColorMode;

/// How long a paused preview waits for input before redrawing.
const IDLE_POLL: Duration = Duration::from_millis(250);

type PreviewTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

/// Run the interactive preview until the user quits.
pub(crate) fn run_interactive(
    scene: &Scene,
    start_frame: u32,
    mode: ColorMode,
) -> Result<(), PreviewError> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|error| {
        PreviewError::Terminal(format!("could not initialize the terminal: {error}"))
    })?;
    let mut playback = Playback {
        current: start_frame,
        playing: false,
    };
    event_loop(&mut terminal, scene, mode, &mut playback)
}

fn event_loop(
    terminal: &mut PreviewTerminal,
    scene: &Scene,
    mode: ColorMode,
    playback: &mut Playback,
) -> Result<(), PreviewError> {
    let fps = scene.timeline.fps.max(1);
    let frame_duration = Duration::from_secs_f64(1.0 / f64::from(fps));
    let mut progress = Duration::ZERO;
    let mut last_tick = Instant::now();
    let mut dirty = true;

    loop {
        let now = Instant::now();
        if playback.playing {
            progress += now.saturating_duration_since(last_tick);
        }
        last_tick = now;

        if advance_playback(playback, scene, &mut progress, frame_duration) {
            dirty = true;
        }

        if dirty {
            let frame = compose_frame(scene, playback.current)?;
            draw(terminal, scene, &frame, mode, playback)?;
            dirty = false;
        }

        let timeout = if playback.playing {
            frame_duration.saturating_sub(progress)
        } else {
            IDLE_POLL
        };
        if !event::poll(timeout).map_err(PreviewError::Io)? {
            continue;
        }
        match event::read().map_err(PreviewError::Io)? {
            Event::Key(key) if is_key_press(key) => match handle_key(key, scene, playback) {
                KeyAction::Quit => break,
                KeyAction::Redraw => {
                    progress = Duration::ZERO;
                    last_tick = Instant::now();
                    dirty = true;
                }
                KeyAction::Ignore => {}
            },
            Event::Resize(_, _) => dirty = true,
            _ => {}
        }
    }

    Ok(())
}

/// Advance the playback clock; returns true when a new frame must be drawn.
fn advance_playback(
    playback: &mut Playback,
    scene: &Scene,
    progress: &mut Duration,
    frame_duration: Duration,
) -> bool {
    if !playback.playing {
        return false;
    }
    let mut advanced = false;
    while playback.playing && *progress >= frame_duration {
        *progress -= frame_duration;
        playback.advance(scene);
        advanced = true;
    }
    advanced
}

fn draw(
    terminal: &mut PreviewTerminal,
    scene: &Scene,
    frame: &Frame,
    mode: ColorMode,
    playback: &Playback,
) -> Result<(), PreviewError> {
    let status = Status { scene, playback };
    terminal
        .draw(|ui| {
            let area = ui.area();
            let status_height = u16::from(area.height > 0);
            let canvas = Rect::new(
                area.x,
                area.y,
                area.width,
                area.height.saturating_sub(status_height),
            );
            let content = frame_to_buffer(frame, canvas, mode);
            blit(&content, ui.buffer_mut());
            if status_height > 0 {
                let status_area =
                    Rect::new(area.x, area.y + canvas.height, area.width, status_height);
                ui.render_widget(Paragraph::new(status.text(frame, canvas)), status_area);
            }
        })
        .map_err(PreviewError::Io)?;
    Ok(())
}

/// Copy `source` into `target`, skipping positions outside the target.
fn blit(source: &Buffer, target: &mut Buffer) {
    for y in source.area.top()..source.area.bottom() {
        for x in source.area.left()..source.area.right() {
            let Some(cell) = source.cell((x, y)) else {
                continue;
            };
            if let Some(destination) = target.cell_mut((x, y)) {
                *destination = cell.clone();
            }
        }
    }
}

/// Frame number, timing and play state shown under the canvas.
struct Status<'a> {
    scene: &'a Scene,
    playback: &'a Playback,
}

impl Status<'_> {
    fn text(&self, frame: &Frame, canvas: Rect) -> String {
        let timeline = &self.scene.timeline;
        let fps = timeline.fps.max(1);
        let duration = f64::from(timeline.frames) / f64::from(fps);
        let state = if self.playback.playing {
            "playing"
        } else {
            "paused"
        };
        let current = self.playback.current.saturating_add(1);
        let total = timeline.frames;
        let mut text = format!("frame {current}/{total} | {fps} fps | {duration:.2}s | {state}");
        if canvas.width < frame.grid.cols || canvas.height < frame.grid.rows {
            let (cols, rows) = (frame.grid.cols, frame.grid.rows);
            let (width, height) = (canvas.width, canvas.height);
            text.push_str(&format!(
                " | canvas {cols}x{rows} clipped to {width}x{height}"
            ));
        }
        text
    }
}

/// Playback position and play/pause state.
struct Playback {
    current: u32,
    playing: bool,
}

impl Playback {
    fn advance(&mut self, scene: &Scene) {
        if self.current >= last_frame(scene) {
            if scene.timeline.loop_ {
                self.current = 0;
            } else {
                self.playing = false;
            }
        } else {
            self.current += 1;
        }
    }

    fn toggle(&mut self, scene: &Scene) {
        self.playing = !self.playing;
        // Pressing play on the final frame without looping restarts from the top.
        if self.playing && !scene.timeline.loop_ && self.current >= last_frame(scene) {
            self.current = 0;
        }
    }

    fn step_back(&mut self) {
        self.current = self.current.saturating_sub(1);
    }

    fn step_forward(&mut self, scene: &Scene) {
        self.current = self.current.saturating_add(1).min(last_frame(scene));
    }

    fn jump_to_first(&mut self) {
        self.current = 0;
    }

    fn jump_to_last(&mut self, scene: &Scene) {
        self.current = last_frame(scene);
    }
}

fn last_frame(scene: &Scene) -> u32 {
    scene.timeline.frames.saturating_sub(1)
}

enum KeyAction {
    Ignore,
    Redraw,
    Quit,
}

fn is_key_press(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn handle_key(key: KeyEvent, scene: &Scene, playback: &mut Playback) -> KeyAction {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => KeyAction::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        KeyCode::Char(' ') => {
            playback.toggle(scene);
            KeyAction::Redraw
        }
        KeyCode::Left => {
            playback.step_back();
            KeyAction::Redraw
        }
        KeyCode::Right => {
            playback.step_forward(scene);
            KeyAction::Redraw
        }
        KeyCode::Home => {
            playback.jump_to_first();
            KeyAction::Redraw
        }
        KeyCode::End => {
            playback.jump_to_last(scene);
            KeyAction::Redraw
        }
        _ => KeyAction::Ignore,
    }
}

/// Restores the terminal no matter how the preview exits, including errors.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, PreviewError> {
        enable_raw_mode().map_err(|error| {
            PreviewError::Terminal(format!("could not enable raw mode: {error}"))
        })?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(PreviewError::Terminal(format!(
                "could not enter the alternate screen: {error}"
            )));
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show);
        let _ = stdout.flush();
    }
}
