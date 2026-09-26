//! Project 8-style chase camera: follows the direction of travel rather than
//! the board, so spins and fakie do not swing the view. The behaviour is LIKELY
//! (THPS convention). All distances are TEMPORARY TUNING VALUES in `Tuning`.
use crate::skater::{Skater, State};
use crate::tuning::Tuning;
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Horizontal direction the camera follows.
    follow: Vec3,
}

impl Camera {
    pub fn new(skater: &Skater, t: &Tuning) -> Self {
        let follow = skater.travel();
        let target = skater.position + Vec3::Y * t.camera_target_height;
        Self {
            eye: target - follow * t.camera_distance
                + Vec3::Y * (t.camera_height - t.camera_target_height),
            target,
            follow,
        }
    }

    pub fn update(&mut self, skater: &Skater, t: &Tuning, dt: f32) {
        let h = Vec3::new(skater.velocity.x, 0.0, skater.velocity.z);
        let desired = if h.length() > 1.0 && !matches!(skater.state, State::Bail { .. }) {
            h.normalize()
        } else {
            self.follow
        };
        let blend = 1.0 - (-t.camera_stiffness * dt).exp();
        self.follow = self.follow.lerp(desired, blend * 0.6).normalize_or(desired);
        let target = skater.position + Vec3::Y * t.camera_target_height;
        let eye = target - self.follow * t.camera_distance
            + Vec3::Y * (t.camera_height - t.camera_target_height);
        self.eye = self.eye.lerp(eye, blend);
        self.target = self.target.lerp(target, (blend * 2.0).min(1.0));
    }
}
