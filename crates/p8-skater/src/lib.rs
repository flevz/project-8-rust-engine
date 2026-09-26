//! Tony Hawk's Project 8 skater simulation, translated from the retail game.
//!
//! Rules for this crate (see `docs/translation.md`):
//! - Each translated function cites its retail guest address, e.g. `820D9830`.
//! - Behaviour is only added once it has been read in the original code.
//!   Untranslated behaviour is absent, not approximated.
//! - Numeric settings are read from the player's own QB scripts through
//!   [`script::Scripts`]; this crate contains no values from the game except
//!   literal constants embedded in the translated code, each cited.
//! - Retail structure offsets are kept in field docs (e.g. `+1540`) so every
//!   field can be checked against the original.
pub mod air;
pub mod body;
pub mod controller;
pub mod core_physics;
pub mod ground;
pub mod input;
pub mod pad;
pub mod script;
pub mod skater;
pub mod stats;
pub mod world;

pub use body::Body;
pub use controller::{Controller, XboxPad};
pub use core_physics::CorePhysics;
pub use input::InputState;
pub use script::Scripts;
pub use skater::Skater;
pub use stats::StatLevels;
pub use world::{FlatFloor, World};
