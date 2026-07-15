//! Audio system module.
//!
//! Decodes audio using symphonia, outputs via rodio.
//! The audio backend is abstracted behind a trait for future swappability.

pub mod backend;
pub mod decoder;
pub mod replaygain;
