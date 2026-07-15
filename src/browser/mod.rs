//! File browser component.
//!
//! Navigate the filesystem within the TUI. Used for adding
//! directories, browsing files, and selecting music files.

use std::path::{Path, PathBuf};

/// Represents a directory in the browser tree.
pub struct Directory {
    pub path: PathBuf,
    pub name: String,
    pub children: Vec<Directory>,
    pub expanded: bool,
}

/// File system browser state.
pub struct FileBrowser {
    current_directory: PathBuf,
    entries: Vec<PathBuf>,
    selected_index: usize,
}

impl FileBrowser {
    pub fn new(root: PathBuf) -> Self {
        Self {
            current_directory: root,
            entries: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn current_dir(&self) -> &Path {
        &self.current_directory
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() && self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.entries.get(self.selected_index)
    }

    pub fn navigate_to(&mut self, path: PathBuf) -> anyhow::Result<()> {
        if path.is_dir() {
            self.current_directory = path;
            self.refresh_entries()?;
        }
        Ok(())
    }

    pub fn refresh_entries(&mut self) -> anyhow::Result<()> {
        self.entries.clear();
        for entry in std::fs::read_dir(&self.current_directory)? {
            let entry = entry?;
            self.entries.push(entry.path());
        }
        self.entries.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            if a_is_dir != b_is_dir {
                b_is_dir.cmp(&a_is_dir)
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });
        self.selected_index = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_browser_navigation() {
        let dir = TempDir::new().unwrap();
        let mut browser = FileBrowser::new(dir.path().to_path_buf());
        browser.refresh_entries().unwrap();
        assert!(browser.entries().is_empty()); // empty temp dir
    }

    #[test]
    fn test_browser_selection() {
        let dir = TempDir::new().unwrap();
        let mut browser = FileBrowser::new(dir.path().to_path_buf());
        browser.select_next();
        assert_eq!(browser.selected_index(), 0); // empty, no change
        browser.select_previous();
        assert_eq!(browser.selected_index(), 0); // no underflow
    }
}
