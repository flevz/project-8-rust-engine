//! What the translated physics asks of the level: retail "feeler" line
//! checks (`820E5048`, which runs the collision query `8221BC60` from
//! physics `+1088` to `+1104` and stores the hit at `+192`).
use glam::Vec3;

/// A feeler hit (physics `+352` point, `+368` normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    /// Physics `+1184`: the surface can be skated (from the collision flags
    /// and `820E0958`). Only walkable/skatable surfaces start a landing.
    pub skatable: bool,
}

pub trait World {
    /// The first surface on the line from `start` to `end`, if any.
    fn feeler(&self, start: Vec3, end: Vec3) -> Option<Hit>;
}

/// A level that is one infinite skatable floor at `height`, facing up: a
/// line hits it only when going down onto it (from above or on it to below
/// or on it). This is a real (trivial) level, not a stand-in for game logic.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatFloor {
    pub height: f32,
}

impl World for FlatFloor {
    fn feeler(&self, start: Vec3, end: Vec3) -> Option<Hit> {
        let (a, b) = (start.y - self.height, end.y - self.height);
        if a < 0.0 || b > 0.0 || a <= b {
            return None;
        }
        let t = a / (a - b);
        Some(Hit { point: start + (end - start) * t, normal: Vec3::Y, skatable: true })
    }
}
