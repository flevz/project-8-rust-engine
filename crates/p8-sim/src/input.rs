//! Project 8 controller input, decoupled from any device API.
//!
//! Layout follows the Xbox 360 release (the host maps its pad and keyboard onto
//! this). Only the physical layout is CONFIRMED from Project8Recomp's scripted
//! input layer. The button *roles* follow the long-standing THPS convention
//! and are LIKELY, not verified against Project 8 scripts:
//! A ollie, X flip, B grab, Y grind, LB/RB spin, left stick steer and balance.
use glam::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// Left stick. +x is right, +y is forward/up. Each axis is in -1..1.
    pub stick: Vec2,
    pub ollie: bool,
    pub flip: bool,
    pub grab: bool,
    pub grind: bool,
    pub spin_left: bool,
    pub spin_right: bool,
}

/// Recognises the THPS manual gesture: stick up then down quickly (manual),
/// or down then up (nose manual). The last request is buffered briefly so that
/// it can be entered in the air before landing.
#[derive(Clone, Debug, Default)]
pub(crate) struct ManualGesture {
    last_up: Option<f32>,
    last_down: Option<f32>,
    was_up: bool,
    was_down: bool,
    pub requested: Option<(bool, f32)>,
}

const WINDOW: f32 = 0.3;
const BUFFER: f32 = 0.45;

impl ManualGesture {
    /// Feed one tick of stick input. `clock` is simulation time in seconds.
    pub fn update(&mut self, stick_y: f32, clock: f32) {
        let up = stick_y > 0.7;
        let down = stick_y < -0.7;
        if up && !self.was_up {
            if self.last_down.is_some_and(|t| clock - t <= WINDOW) {
                self.requested = Some((true, clock));
            }
            self.last_up = Some(clock);
        }
        if down && !self.was_down {
            if self.last_up.is_some_and(|t| clock - t <= WINDOW) {
                self.requested = Some((false, clock));
            }
            self.last_down = Some(clock);
        }
        self.was_up = up;
        self.was_down = down;
        if self.requested.is_some_and(|(_, t)| clock - t > BUFFER) {
            self.requested = None;
        }
    }

    /// Take a pending request. Returns `Some(nose)`.
    pub fn take(&mut self) -> Option<bool> {
        self.requested.take().map(|(nose, _)| nose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_then_down_is_a_manual_and_down_then_up_a_nose_manual() {
        let mut g = ManualGesture::default();
        g.update(1.0, 0.0);
        g.update(0.0, 0.05);
        g.update(-1.0, 0.1);
        assert_eq!(g.take(), Some(false));
        g.update(0.0, 0.9);
        g.update(-1.0, 1.0);
        g.update(1.0, 1.1);
        assert_eq!(g.take(), Some(true));
    }

    #[test]
    fn slow_gestures_and_stale_requests_are_ignored() {
        let mut g = ManualGesture::default();
        g.update(1.0, 0.0);
        g.update(0.0, 0.2);
        g.update(-1.0, 0.6);
        assert_eq!(g.take(), None);
        g.update(0.0, 1.9);
        g.update(1.0, 2.0);
        g.update(-1.0, 2.1);
        g.update(0.0, 3.0);
        assert_eq!(g.take(), None, "buffer expired");
    }
}
