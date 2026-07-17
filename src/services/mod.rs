//! Service layer — integrations and background tasks.
//!
//! Services include: file watcher, auto-save, scrobbling,
//! and any long-running background operations.

/// Placeholder for service module.
pub struct Services;

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}

impl Services {
    pub fn new() -> Self {
        Self {}
    }
}
