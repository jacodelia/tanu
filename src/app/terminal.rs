//! Terminal initialization and restoration using crossterm.
//!
//! Handles raw mode, alternate screen, mouse capture,
//! and provides a draw wrapper around ratatui's `Terminal`.

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Frame;

/// Wraps a ratatui `Terminal` with convenience methods for
/// initialization, rendering, and teardown.
pub struct Terminal {
    inner: ratatui::Terminal<CrosstermBackend<Stdout>>,
}

impl Terminal {
    /// Creates a new terminal backend from stdout.
    pub fn new() -> anyhow::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let inner = ratatui::Terminal::new(backend)?;
        Ok(Self { inner })
    }

    /// Enables raw mode, alternate screen, and mouse capture.
    /// Must be called exactly once before entering the render loop.
    pub fn setup() -> anyhow::Result<()> {
        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            cursor::Hide
        )?;
        Ok(())
    }

    /// Restores terminal state: leaves alternate screen,
    /// disables mouse capture, restores raw mode, shows cursor.
    /// Call on normal exit and in panic hooks.
    pub fn teardown() -> anyhow::Result<()> {
        execute!(
            io::stdout(),
            cursor::Show,
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        disable_raw_mode()?;
        Ok(())
    }

    /// Draws a frame using the provided render closure.
    pub fn draw<F>(&mut self, render: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.inner.draw(render)?;
        Ok(())
    }

    /// Clears the terminal screen.
    pub fn clear(&mut self) -> anyhow::Result<()> {
        self.inner.clear()?;
        Ok(())
    }

    /// Returns the terminal size in (cols, rows).
    pub fn size(&self) -> anyhow::Result<(u16, u16)> {
        let rect = self.inner.size()?;
        Ok((rect.width, rect.height))
    }
}

/// Reads a single crossterm event, blocking until one arrives.
pub fn read_event() -> anyhow::Result<crossterm::event::Event> {
    Ok(crossterm::event::read()?)
}

/// Checks if an event is available, waiting at most `timeout`.
pub fn poll_event(timeout: Duration) -> anyhow::Result<bool> {
    Ok(crossterm::event::poll(timeout)?)
}
