//! Album art panel — renders the current track's embedded cover as a mosaic
//! of half-block characters (two vertical pixels per cell via `▀`), so it
//! works in any terminal without image protocols.

use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::Event;
use crate::widgets::{EventResult, Widget};

pub struct AlbumArt {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    /// Decoded cover image, if the current track had embedded art.
    image: Option<image::DynamicImage>,
    /// Title/artist for the placeholder / caption.
    caption: String,
    /// Cache: last rendered size so we only resize on change.
    cached_size: (u16, u16),
    cached_lines: Vec<Line<'static>>,
}

impl AlbumArt {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            image: None,
            caption: String::new(),
            cached_size: (0, 0),
            cached_lines: Vec::new(),
        }
    }

    /// Load embedded cover + caption for a track path.
    pub fn set_track(&mut self, path: &PathBuf) {
        self.image = extract_cover(path);
        self.caption = track_caption(path);
        self.cached_size = (0, 0);
        self.cached_lines.clear();
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        self.image = None;
        self.caption.clear();
        self.cached_size = (0, 0);
        self.cached_lines.clear();
        self.dirty = true;
    }

    /// Build half-block lines for the given inner size, using the cache.
    fn art_lines(&mut self, w: u16, h: u16) -> Vec<Line<'static>> {
        if (w, h) == self.cached_size && !self.cached_lines.is_empty() {
            return self.cached_lines.clone();
        }
        let lines = match &self.image {
            Some(img) => render_halfblocks(img, w, h),
            None => placeholder(w, h),
        };
        self.cached_size = (w, h);
        self.cached_lines = lines.clone();
        lines
    }
}

impl Default for AlbumArt {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for AlbumArt {
    fn id(&self) -> WidgetId { self.id }
    fn rect(&self) -> Rect { self.rect }
    fn set_rect(&mut self, rect: Rect) { self.rect = rect; }
    fn is_dirty(&self) -> bool { self.dirty }
    fn mark_dirty(&mut self) { self.dirty = true; }
    fn mark_clean(&mut self) { self.dirty = false; }
    fn is_focused(&self) -> bool { false }
    fn is_focusable(&self) -> bool { false }

    fn handle_event(&mut self, _event: &Event) -> EventResult {
        EventResult::NotConsumed
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(69, 71, 90)))
            .title(Span::styled(" ♫ Cover ", Style::default().fg(Color::Rgb(203, 166, 247)).add_modifier(Modifier::BOLD)));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let lines = self.art_lines(inner.width, inner.height);
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Extract the first embedded picture from a file's tags and decode it.
fn extract_cover(path: &PathBuf) -> Option<image::DynamicImage> {
    use lofty::file::TaggedFileExt;
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let pic = tag.pictures().first()?;
    image::load_from_memory(pic.data()).ok()
}

fn track_caption(path: &PathBuf) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Render an image using `▀` half-blocks (top pixel = fg, bottom = bg), giving
/// two vertical pixels per cell. Aspect ratio is preserved (the cover is
/// centered, letterboxed) and downscaled with Lanczos3 for a crisp result.
fn render_halfblocks(img: &image::DynamicImage, w: u16, h: u16) -> Vec<Line<'static>> {
    use image::imageops::FilterType;
    let canvas_w = w as u32;
    let canvas_h = (h as u32) * 2; // two pixels per text row
    if canvas_w == 0 || canvas_h == 0 {
        return Vec::new();
    }
    // resize() keeps aspect ratio, fitting inside the canvas.
    let rgb = img.resize(canvas_w, canvas_h, FilterType::Lanczos3).to_rgb8();
    let (iw, ih) = rgb.dimensions();
    let ox = (canvas_w.saturating_sub(iw)) / 2;
    let oy = (canvas_h.saturating_sub(ih)) / 2;
    let bg = [24u8, 24, 37];
    let pixel = |cx: u32, cy: u32| -> [u8; 3] {
        if cx >= ox && cx < ox + iw && cy >= oy && cy < oy + ih {
            rgb.get_pixel(cx - ox, cy - oy).0
        } else {
            bg
        }
    };

    let mut lines = Vec::with_capacity(h as usize);
    for row in 0..h as u32 {
        let mut spans = Vec::with_capacity(w as usize);
        for col in 0..w as u32 {
            let top = pixel(col, row * 2);
            let bottom = pixel(col, row * 2 + 1);
            spans.push(Span::styled(
                "▀",
                Style::default()
                    .fg(Color::Rgb(top[0], top[1], top[2]))
                    .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// A centered music-note placeholder when there's no embedded art.
fn placeholder(w: u16, h: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(h as usize);
    let mid = h / 2;
    for row in 0..h {
        if row == mid {
            let note = "♪ ♫ ♪";
            let pad = (w as usize).saturating_sub(note.chars().count()) / 2;
            lines.push(Line::from(Span::styled(
                format!("{}{}", " ".repeat(pad), note),
                Style::default().fg(Color::Rgb(108, 112, 134)),
            )));
        } else {
            lines.push(Line::from(""));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_size() {
        let lines = placeholder(20, 8);
        assert_eq!(lines.len(), 8);
    }
}
