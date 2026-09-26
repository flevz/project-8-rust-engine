//! Readers for Tony Hawk's Project 8 (Xbox 360) data, for use on the player's
//! own installed copy. Every structure is labelled CONFIRMED, LIKELY or
//! UNKNOWN in `docs/formats.md`.
pub mod checksum;
pub mod pak;
pub mod qb;

pub use checksum::qb_key;
