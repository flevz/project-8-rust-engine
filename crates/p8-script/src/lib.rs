//! Tony Hawk's Project 8 script VM, translated from the retail `CScript`
//! code (functions `82207ED8`..`82211DE8` and the script commands that act
//! on scripts). It runs the player's own scripts from `qb.pak.xen`; the
//! game object running a script supplies the other commands ([`Host`]).
pub mod code;
pub mod params;
pub mod vm;

pub use params::Params;
pub use vm::{Handler, Host, Script, Status, TRUE, equal};
