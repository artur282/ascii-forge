//! Minimal vendored FIGlet renderer.
//!
//! Based on `figlet-rs` 1.0.0 (Apache-2.0), trimmed to the FIGlet font path
//! used by ASCII Forge. The vulnerable `zip` dependency and the unused Toilet
//! font support were removed; zipped fonts are rejected explicitly.

mod figlet;
mod shared;

pub use figlet::FIGlet;
pub use shared::{FIGcharacter, FIGure, HeaderLine};
