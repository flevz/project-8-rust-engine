//! The skater object's transform and motion (retail object at physics `+12`).
use glam::{Mat3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Object `+112`.
    pub position: Vec3,
    /// Object `+144`..`+176`: rows right (X), up (Y) and at (Z, forward).
    /// Reset to identity by `820D4700`.
    pub matrix: Mat3,
    /// Object `+208`.
    pub velocity: Vec3,
}

impl Default for Body {
    fn default() -> Self {
        Self { position: Vec3::ZERO, matrix: Mat3::IDENTITY, velocity: Vec3::ZERO }
    }
}

impl Body {
    /// Object `+160`.
    pub fn up(&self) -> Vec3 {
        self.matrix.y_axis
    }
    /// Object `+176`.
    pub fn at(&self) -> Vec3 {
        self.matrix.z_axis
    }
}
