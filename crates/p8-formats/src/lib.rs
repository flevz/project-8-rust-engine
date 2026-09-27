//! Readers for Tony Hawk's Project 8 (Xbox 360) data, for use on the player's
//! own installed copy. Every structure is labelled CONFIRMED, LIKELY or
//! UNKNOWN in `docs/formats.md`.
pub mod anim;
pub mod checksum;
pub mod havok;
pub mod pak;
pub mod qb;
pub mod scene;
pub mod skeleton;
pub mod texture;
pub mod zone;

pub use checksum::qb_key;
