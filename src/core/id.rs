//! Unique identifiers for all domain entities.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlbumId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArtistId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaylistId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentId(Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl AlbumId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl ArtistId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl PlaylistId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl WidgetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl ComponentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
