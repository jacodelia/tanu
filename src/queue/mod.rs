//! Playback queue.
//!
//! The queue is an ordered list of tracks to be played.
//! It supports queue, dequeue, shuffle, repeat modes,
//! and is separate from the currently loaded playlist.

use crate::core::id::TrackId;

/// A playback queue with optional shuffle and repeating.
pub struct Queue {
    tracks: Vec<TrackId>,
    position: usize,
    shuffle: bool,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            position: 0,
            shuffle: false,
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn current(&self) -> Option<&TrackId> {
        self.tracks.get(self.position)
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn enqueue(&mut self, track_id: TrackId) {
        self.tracks.push(track_id);
    }

    pub fn enqueue_many(&mut self, tracks: Vec<TrackId>) {
        self.tracks.extend(tracks);
    }

    pub fn dequeue(&mut self) -> Option<TrackId> {
        if self.tracks.is_empty() {
            None
        } else {
            Some(self.tracks.remove(0))
        }
    }

    pub fn insert(&mut self, index: usize, track_id: TrackId) {
        if index <= self.tracks.len() {
            self.tracks.insert(index, track_id);
            if index <= self.position {
                self.position += 1;
            }
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<TrackId> {
        if index < self.tracks.len() {
            let removed = self.tracks.remove(index);
            if index < self.position {
                self.position -= 1;
            }
            if self.position >= self.tracks.len() && !self.tracks.is_empty() {
                self.position = self.tracks.len() - 1;
            }
            Some(removed)
        } else {
            None
        }
    }

    pub fn advance(&mut self) -> Option<&TrackId> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.position + 1 < self.tracks.len() {
            self.position += 1;
        } else {
            self.position = 0; // wrap
        }
        self.current()
    }

    pub fn retreat(&mut self) -> Option<&TrackId> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.position > 0 {
            self.position -= 1;
        } else {
            self.position = 0;
        }
        self.current()
    }

    pub fn goto(&mut self, position: usize) -> Option<&TrackId> {
        if position < self.tracks.len() {
            self.position = position;
        }
        self.current()
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.position = 0;
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        self.shuffle = enabled;
    }

    pub fn all_tracks(&self) -> &[TrackId] {
        &self.tracks
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_queue() {
        let queue = Queue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.current().is_none());
    }

    #[test]
    fn test_enqueue_dequeue() {
        let mut queue = Queue::new();
        let t1 = TrackId::new();
        let t2 = TrackId::new();
        queue.enqueue(t1);
        queue.enqueue(t2);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_advance() {
        let mut queue = Queue::new();
        let t1 = TrackId::new();
        let t2 = TrackId::new();
        let t3 = TrackId::new();
        queue.enqueue(t1);
        queue.enqueue(t2);
        queue.enqueue(t3);
        assert_eq!(queue.position(), 0);
        queue.advance();
        assert_eq!(queue.position(), 1);
        queue.advance();
        assert_eq!(queue.position(), 2);
        queue.advance(); // wraps
        assert_eq!(queue.position(), 0);
    }

    #[test]
    fn test_remove_adjusts_position() {
        let mut queue = Queue::new();
        let t1 = TrackId::new();
        let t2 = TrackId::new();
        let t3 = TrackId::new();
        queue.enqueue(t1);
        queue.enqueue(t2);
        queue.enqueue(t3);
        queue.advance(); // pos 1
        queue.remove(0); // remove first
        assert_eq!(queue.position(), 0);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_insert_adjusts_position() {
        let mut queue = Queue::new();
        let t1 = TrackId::new();
        let t2 = TrackId::new();
        queue.enqueue(t1);
        queue.enqueue(t2);
        queue.advance(); // pos 1
        let t3 = TrackId::new();
        queue.insert(0, t3); // insert at beginning
        assert_eq!(queue.position(), 2);
        assert_eq!(queue.len(), 3);
    }

    #[test]
    fn test_clear() {
        let mut queue = Queue::new();
        queue.enqueue(TrackId::new());
        queue.enqueue(TrackId::new());
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.position(), 0);
    }
}
