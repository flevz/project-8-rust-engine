//! The skater object's transform and motion (retail object at physics `+12`).
use glam::{Mat3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Object `+112`.
    pub position: Vec3,
    /// Object `+144`..`+176`, stored as glam columns: row 0 (retail "right",
    /// X), row 1 (up, Y) and row 2 (at, Z, forward). A positive ground turn
    /// rotates `at` toward row 0 and retail reports it as "Left" (`820ECEE8`),
    /// so row 0 points to the skater's left. Reset to identity by `820D4700`.
    pub matrix: Mat3,
    /// Object `+208`.
    pub velocity: Vec3,
}

impl Default for Body {
    fn default() -> Self {
        Self { position: Vec3::ZERO, matrix: Mat3::IDENTITY, velocity: Vec3::ZERO }
    }
}

/// Retail `820B9ED0` (with `821E9168`): `M = RotY(angle) * M` in row-vector
/// form, i.e. a turn about the matrix's own up row.
pub fn rotate_about_up(m: &mut Mat3, angle: f32) {
    let (s, c) = (angle.sin(), angle.cos());
    let (x, z) = (m.x_axis, m.z_axis);
    m.x_axis = x * c - z * s;
    m.z_axis = x * s + z * c;
}

impl Body {
    /// Object `+144`.
    pub fn row0(&self) -> Vec3 {
        self.matrix.x_axis
    }
    /// Object `+160`.
    pub fn up(&self) -> Vec3 {
        self.matrix.y_axis
    }
    /// Object `+176`.
    pub fn at(&self) -> Vec3 {
        self.matrix.z_axis
    }
}
