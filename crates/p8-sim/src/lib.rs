//! Tony Hawk's Project 8-style skater simulation.
//!
//! This crate is deliberately independent of Bevy and of any file format. The
//! host feeds it [`Input`], a timestep and a [`World`] built from generic map
//! triangles and rails. It returns a pose and a camera. See `docs/research.md`
//! for what is known about the original game and what is still approximate.
pub mod camera;
pub mod input;
pub mod skater;
pub mod tuning;
pub mod world;

pub use camera::Camera;
pub use input::Input;
pub use skater::{Pose, Skater, State};
pub use tuning::Tuning;
pub use world::{Hit, Rail, World};

use glam::{Quat, Vec3};

/// Everything the host needs to present one simulated tick.
#[derive(Clone, Debug)]
pub struct Output {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub heading: f32,
    pub pose: Pose,
    /// Seconds since `pose` began.
    pub pose_time: f32,
    pub camera_eye: Vec3,
    pub camera_target: Vec3,
    pub fov_degrees: f32,
    /// Short status line for the HUD.
    pub hud: String,
}

/// One complete Project 8 controller instance.
pub struct Simulation {
    pub tuning: Tuning,
    pub skater: Skater,
    pub camera: Camera,
}

impl Simulation {
    pub fn new(tuning: Tuning, position: Vec3, heading: f32, velocity: Vec3) -> Self {
        let skater = Skater::new(position, heading, velocity);
        let camera = Camera::new(&skater, &tuning);
        Self {
            tuning,
            skater,
            camera,
        }
    }

    /// Replace all gameplay state, for example on activation, teleport or map change.
    pub fn reset(&mut self, position: Vec3, heading: f32, velocity: Vec3) {
        self.skater = Skater::new(position, heading, velocity);
        self.camera = Camera::new(&self.skater, &self.tuning);
    }

    pub fn step(&mut self, input: &Input, dt: f32, world: &World) {
        self.skater.step(input, dt, world, &self.tuning);
        self.camera.update(&self.skater, &self.tuning, dt);
    }

    pub fn output(&self) -> Output {
        let s = &self.skater;
        let mut hud = format!(
            "PROJECT 8  {:>4.1} m/s  {}{}",
            s.speed(),
            match s.state {
                State::Ground => "Rolling",
                State::Air => "Air",
                State::Manual { nose: false, .. } => "Manual",
                State::Manual { nose: true, .. } => "Nose manual",
                State::Grind { .. } => "Grind",
                State::Bail { .. } => "Bail",
            },
            if s.fakie { " (fakie)" } else { "" }
        );
        if let Some(b) = s.balance(&self.tuning) {
            let slot = ((b.clamp(-1.0, 1.0) + 1.0) * 5.0).round() as usize;
            let meter: String = (0..=10)
                .map(|i| if i == slot { '|' } else { '-' })
                .collect();
            hud.push_str(&format!("  [{meter}]"));
        }
        if !s.combo.is_empty() {
            hud.push_str(&format!("\n{}", s.combo.join(" + ")));
        } else if let Some(last) = &s.last_landing {
            hud.push_str(&format!("\n{last}"));
        }
        Output {
            position: s.position,
            rotation: s.rotation(),
            velocity: s.velocity,
            heading: s.heading,
            pose: s.pose(),
            pose_time: s.state_time,
            camera_eye: self.camera.eye,
            camera_target: self.camera.target,
            fov_degrees: self.tuning.camera_fov_degrees,
            hud,
        }
    }
}

#[cfg(test)]
mod tests;
