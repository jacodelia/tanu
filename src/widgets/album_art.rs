//! Album art panel — renders a cover as a mosaic of half-block characters (two
//! vertical pixels per cell via `▀`), so it works in any terminal without image
//! protocols.
//!
//! Source priority: image files in the track's folder (a file named `cover`
//! always shown first), else the embedded tag picture. When the folder holds
//! more than one image, a `‹ i/n ›` bar lets you page left/right through them.

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::core::id::WidgetId;
use crate::events::{Event, MouseAction};
use crate::widgets::{EventResult, Widget};

/// Decodable image extensions (matches the `image` crate features enabled).
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png"];

pub struct AlbumArt {
    id: WidgetId,
    rect: Rect,
    dirty: bool,
    /// Folder image files (cover-first). Empty when using embedded art only.
    images: Vec<PathBuf>,
    /// Index into `images` currently shown.
    index: usize,
    /// The decoded image currently displayed (folder file or embedded).
    current: Option<image::DynamicImage>,
    /// Title/artist for the placeholder / caption.
    caption: String,
    /// Cache: (width, art_height, index) of the last render.
    cached_key: (u16, u16, usize),
    cached_lines: Vec<Line<'static>>,
    /// Local hit regions of the ‹ / › arrows (rebuilt on render).
    prev_btn: Option<(u16, u16, u16)>,
    next_btn: Option<(u16, u16, u16)>,
}

impl AlbumArt {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            rect: Rect::default(),
            dirty: true,
            images: Vec::new(),
            index: 0,
            current: None,
            caption: String::new(),
            cached_key: (0, 0, usize::MAX),
            cached_lines: Vec::new(),
            prev_btn: None,
            next_btn: None,
        }
    }

    /// Load the cover for a track: folder images (cover-first), else embedded.
    pub fn set_track(&mut self, path: &PathBuf) {
        self.caption = track_caption(path);
        self.images = path.parent().map(gather_dir_images).unwrap_or_default();
        self.index = 0;
        self.current = if let Some(first) = self.images.first() {
            image::open(first).ok()
        } else {
            extract_cover(path)
        };
        self.invalidate();
    }

    pub fn clear(&mut self) {
        self.images.clear();
        self.index = 0;
        self.current = None;
        self.caption.clear();
        self.invalidate();
    }

    fn invalidate(&mut self) {
        self.cached_key = (0, 0, usize::MAX);
        self.cached_lines.clear();
        self.dirty = true;
    }

    /// Page to another folder image (wraps). `delta` is +1 / -1.
    fn step(&mut self, delta: isize) {
        if self.images.len() < 2 {
            return;
        }
        let n = self.images.len() as isize;
        self.index = (((self.index as isize + delta) % n + n) % n) as usize;
        self.current = image::open(&self.images[self.index]).ok();
        self.invalidate();
    }

    /// Build half-block lines for the given size, using the cache.
    fn art_lines(&mut self, w: u16, h: u16) -> Vec<Line<'static>> {
        let key = (w, h, self.index);
        if key == self.cached_key && !self.cached_lines.is_empty() {
            return self.cached_lines.clone();
        }
        let lines = match &self.current {
            Some(img) => render_halfblocks(img, w, h),
            None => placeholder(w, h),
        };
        self.cached_key = key;
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
    fn id(&self) -> WidgetId {
        self.id
    }
    fn rect(&self) -> Rect {
        self.rect
    }
    fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }
    fn is_dirty(&self) -> bool {
        self.dirty
    }
    fn mark_dirty(&mut self) {
        self.dirty = true;
    }
    fn mark_clean(&mut self) {
        self.dirty = false;
    }
    fn is_focused(&self) -> bool {
        false
    }
    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, _event: &Event) -> EventResult {
        EventResult::NotConsumed
    }

    fn handle_mouse(&mut self, x: u16, y: u16, action: &MouseAction) -> EventResult {
        match action {
            MouseAction::Press(..) | MouseAction::DoubleClick(..) => {
                if let Some((ry, x0, x1)) = self.prev_btn {
                    if y == ry && x >= x0 && x < x1 {
                        self.step(-1);
                        return EventResult::Consumed;
                    }
                }
                if let Some((ry, x0, x1)) = self.next_btn {
                    if y == ry && x >= x0 && x < x1 {
                        self.step(1);
                        return EventResult::Consumed;
                    }
                }
                EventResult::NotConsumed
            }
            // Wheel anywhere over the cover pages through the folder images.
            MouseAction::ScrollUp(..) | MouseAction::ScrollLeft(..) => {
                self.step(-1);
                EventResult::Consumed
            }
            MouseAction::ScrollDown(..) | MouseAction::ScrollRight(..) => {
                self.step(1);
                EventResult::Consumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::theme::border()))
            .title(Span::styled(
                " ♫ Cover ",
                Style::default()
                    .fg(crate::theme::primary())
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.prev_btn = None;
        self.next_btn = None;
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Reserve a bottom row for the ‹ i/n › pager when there are >1 images.
        let has_nav = self.images.len() > 1 && inner.height >= 2;
        let art_h = if has_nav {
            inner.height - 1
        } else {
            inner.height
        };
        let lines = self.art_lines(inner.width, art_h);
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: art_h,
            },
        );

        if has_nav {
            let label = format!("‹  {}/{}  ›", self.index + 1, self.images.len());
            let lw = label.chars().count() as u16;
            let nav_x = inner.x + inner.width.saturating_sub(lw) / 2;
            let nav_y = inner.y + art_h;
            // ‹ is the first char, › the last (each 1 cell); store local hit x.
            self.prev_btn = Some((nav_y - area.y, nav_x - area.x, nav_x - area.x + 1));
            self.next_btn = Some((nav_y - area.y, nav_x - area.x + lw - 1, nav_x - area.x + lw));
            let styled = Line::from(Span::styled(
                label,
                Style::default()
                    .fg(crate::theme::primary())
                    .add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(
                Paragraph::new(styled),
                Rect {
                    x: nav_x,
                    y: nav_y,
                    width: lw,
                    height: 1,
                },
            );
        }
    }
}

/// Collect decodable image files in `dir`, `cover` first, then alphabetical.
fn gather_dir_images(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(iter) => iter
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
                        .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let stem_lower = |p: &Path| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
    };
    files.sort_by(|a, b| {
        // Rank a file named exactly "cover" first.
        let ra = (stem_lower(a) != "cover") as u8;
        let rb = (stem_lower(b) != "cover") as u8;
        ra.cmp(&rb).then(stem_lower(a).cmp(&stem_lower(b)))
    });
    files
}

/// Extract the highest-resolution embedded picture and decode it.
///
/// Files often embed both a tiny thumbnail and a full cover; the first tag
/// picture may be the thumbnail. Prefer front-cover pictures, then the one
/// with the most image data (a good proxy for resolution).
fn extract_cover(path: &PathBuf) -> Option<image::DynamicImage> {
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let best = tag.pictures().iter().max_by_key(|p| {
        // u64 so the front-cover bonus doesn't overflow usize on 32-bit targets.
        let front_bonus: u64 = if p.pic_type() == PictureType::CoverFront {
            1 << 40
        } else {
            0
        };
        front_bonus + p.data().len() as u64
    })?;
    image::load_from_memory(best.data()).ok()
}

fn track_caption(path: &Path) -> String {
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
    let rgb = img
        .resize(canvas_w, canvas_h, FilterType::Lanczos3)
        .to_rgb8();
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

    #[test]
    fn test_gather_images_cover_first() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!("tanu-art-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        for f in ["back.png", "cover.jpg", "art.png", "notes.txt"] {
            fs::write(dir.join(f), b"x").unwrap();
        }
        let imgs = gather_dir_images(&dir);
        // txt excluded; cover.jpg first; 3 images total.
        assert_eq!(imgs.len(), 3);
        assert_eq!(imgs[0].file_name().unwrap(), "cover.jpg");
        let _ = fs::remove_dir_all(&dir);
    }
}
