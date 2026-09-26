//! Skater stat levels (0..10).
//!
//! Retail: `821980B0` returns a skater's level for a `STATS_*` index. A global
//! override (manager `+188`) wins when non-zero; otherwise the skater's own
//! array is used, and an unset entry (-1) falls back to the script global
//! `Skater_Default_Stats`. Split-screen and network adjustments are not
//! translated yet.

/// Stat indices. CONFIRMED: script globals `STATS_AIR` = 0 … `STATS_SPECIAL` = 11.
pub mod index {
    pub const AIR: usize = 0;
    pub const RUN: usize = 1;
    pub const OLLIE: usize = 2;
    pub const SPEED: usize = 3;
    pub const SPIN: usize = 4;
    pub const FLIPSPEED: usize = 5;
    pub const SWITCH: usize = 6;
    pub const RAILBALANCE: usize = 7;
    pub const LIPBALANCE: usize = 8;
    pub const MANUAL: usize = 9;
    pub const WALL: usize = 10;
    pub const SPECIAL: usize = 11;
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatLevels {
    /// Per-stat levels; `None` means "unset" (-1 in retail).
    pub levels: [Option<f32>; 12],
    /// Retail manager `+188`: when non-zero, overrides every stat.
    pub override_all: f32,
    /// Script global `Skater_Default_Stats`.
    pub default: f32,
}

impl StatLevels {
    pub fn with_default(default: f32) -> Self {
        Self { levels: [None; 12], override_all: 0.0, default }
    }

    /// Retail `821980B0` (single-player path).
    pub fn level(&self, stat: usize) -> f32 {
        if self.override_all != 0.0 {
            return self.override_all;
        }
        self.levels.get(stat).copied().flatten().unwrap_or(self.default)
    }
}
