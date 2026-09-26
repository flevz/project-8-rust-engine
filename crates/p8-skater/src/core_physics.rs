//! `SkaterCorePhysics` component: translated pieces of the ground update.
//!
//! Retail ground update `820F6978` does, among other things not yet
//! translated:
//!
//! ```text
//! if IsBraking()      { Brake(); standing_kick_limit = 0; }   // 820D9208, 820D93F0
//! else if CanKick()   { Accelerate(); }                       // 820D9830, 820D9AB8
//! ```
//!
//! [`CorePhysics::drive`] reproduces exactly that part.
use crate::body::Body;
use crate::input::InputState;
use crate::script::{Scripts, StatContext};
use crate::stats::StatLevels;
use glam::Vec3;

/// Events the translated code sends to scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `Brake` stopped the skater (retail event "Stopped").
    Stopped,
}

#[derive(Clone, Debug)]
pub struct CorePhysics {
    pub body: Body,
    /// `+164`: frame time in seconds.
    pub dt: f32,
    /// `+1540`: autokick. Set by `ForceAutokickOn/Off`; `RestoreAutoKick`
    /// copies the player's profile setting. Default on: LIKELY (consistent
    /// with holding A accelerating without any other input).
    pub autokick: bool,
    /// `+1956`: `CanKickOff` sets it, `CanKickOn` clears it. Reset: false.
    pub no_kick: bool,
    /// `+1957`: `CanBrakeOn/Off`. Reset (`820EB690`, `820FA850`): true.
    pub can_brake: bool,
    /// `+1564`: riding a bike. Bike physics is not translated; must stay false.
    pub on_bike: bool,
    /// `+2637` and `+2048`: both block kicking (`820D9830`). Meaning UNKNOWN;
    /// the crouch update clears `+2637`.
    pub flag_2637: bool,
    pub flag_2048: bool,
    /// `+2108`: standing kick speed limit. Every retail write stores 0.
    pub standing_kick_limit: f32,
    /// `+116`: compared with `Skater_max_sloped_turn_cosine` by `Brake`.
    /// Meaning UNKNOWN (a slope cosine); defaults to 1 (flat).
    pub slope_cos_116: f32,
    /// `+1568`: set by `StickPulledBack` (bike front-brake only).
    pub front_brake_1568: bool,
    /// `+1656`: last analog brake amount (0..1).
    pub brake_amount: f32,
    /// `+1980`: set while `Brake` runs.
    pub braking: bool,
    /// SkaterState `+32` (via `+2848`): crouched.
    pub crouched: bool,
    pub stats: StatLevels,
    pub stat_context: StatContext,
}

/// cos() as used by the retail helper `8262BE78` (LIKELY cosine: it takes the
/// absolute value and range-reduces; to be verified).
fn retail_cos(x: f64) -> f32 {
    x.cos() as f32
}

impl CorePhysics {
    pub fn new(scripts: &Scripts) -> Self {
        let default = scripts.global_float("Skater_Default_Stats");
        Self {
            body: Body::default(),
            dt: 1.0 / 60.0,
            autokick: true,
            no_kick: false,
            can_brake: true,
            on_bike: false,
            flag_2637: false,
            flag_2048: false,
            standing_kick_limit: 0.0,
            slope_cos_116: 1.0,
            front_brake_1568: false,
            brake_amount: 0.0,
            braking: false,
            crouched: false,
            stats: StatLevels::with_default(if default > 0.0 { default } else { 5.0 }),
            stat_context: StatContext::default(),
        }
    }

    fn speed(&self) -> f32 {
        self.body.velocity.length()
    }

    fn stat(&self, s: &Scripts, name: &str) -> f32 {
        s.stat(name, self.on_bike, &self.stats, self.stat_context)
    }

    /// Retail `820D7C08`: start crouching when the crouch input is held.
    pub fn update_crouch(&mut self, input: &InputState) {
        if !self.crouched && input.crouch {
            self.crouched = true;
            // Retail also records a timestamp at SkaterState +36.
            self.flag_2637 = false;
        }
    }

    /// Retail `820D74B0` (skater path; bike path not translated).
    pub fn stick_pulled_back(&mut self, s: &Scripts, input: &InputState) -> bool {
        self.front_brake_1568 = false;
        if s.physics_float("Use_New_Analog_Controls", self.on_bike) != 0.0 {
            let stick = input.stick_back_raw * 0.0078125;
            if stick > s.physics_float("Physics_Brake_Stick_Threshold", self.on_bike) {
                return true;
            }
        }
        input.brake_digital
    }

    /// Retail `820D9208`.
    pub fn is_braking(&mut self, s: &Scripts, input: &InputState) -> bool {
        // 1.27 is the constant at 820029BC.
        if !self.autokick && !self.on_bike && !input.kick && self.speed() < 1.27 {
            return true;
        }
        if !self.stick_pulled_back(s, input) || !self.can_brake {
            return false;
        }
        // 1.25 is the constant at 820029B8.
        if self.speed() < 1.25 {
            return true;
        }
        if !(input.brake_hold_128 || input.brake_hold_96) {
            return true;
        }
        if self.body.velocity.dot(self.body.at()) < 0.0 {
            return true;
        }
        self.on_bike
    }

    /// Retail `820D93F0` (skater path).
    pub fn brake(&mut self, s: &Scripts, input: &InputState) -> Option<Event> {
        self.braking = true;
        if self.slope_cos_116 < s.global_float("Skater_max_sloped_turn_cosine") {
            self.braking = false;
            return None;
        }
        // 1.4835299 rad = 85 degrees (double at 820029C8).
        if self.body.up().y < retail_cos(1.4835299054781597) {
            self.braking = false;
            return None;
        }
        let accel = s.physics_float("Physics_Brake_Acceleration", self.on_bike);
        let speed = self.speed();
        if speed < self.dt * accel * 2.0 {
            self.body.velocity = Vec3::ZERO;
            return Some(Event::Stopped);
        }
        let amount = if s.physics_float("Use_New_Analog_Controls", self.on_bike) == 0.0 {
            -(self.dt * accel)
        } else {
            let stick = input.stick_back_raw * 0.0078125;
            let threshold = s.physics_float("Physics_Brake_Stick_Threshold", self.on_bike);
            let mut t = ((stick - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
            if input.stick_back_raw == 0.0 && input.brake_digital {
                t = 1.0;
            }
            self.brake_amount = t;
            if t <= 0.0 {
                return None;
            }
            -(self.dt * t * accel)
        };
        self.body.velocity += self.body.velocity.normalize_or_zero() * amount;
        None
    }

    /// Retail `820D9830` (skater path; bike checks not translated).
    pub fn can_kick(&mut self, s: &Scripts, input: &InputState) -> bool {
        if self.no_kick || self.flag_2637 || self.flag_2048 {
            return false;
        }
        if !self.autokick && !input.kick && !self.on_bike {
            return false;
        }
        if self.stick_pulled_back(s, input) {
            return false;
        }
        // Board must be within 45 degrees of upright (double at 820029D0).
        let up_y = self.body.up().y;
        if up_y < retail_cos(0.7853981852531433) || up_y < 0.0 {
            return false;
        }
        let speed = self.speed();
        if self.crouched {
            speed <= self.stat(s, "Skater_Max_Crouched_Kick_Speed_Stat")
        } else {
            speed <= self.standing_kick_limit
        }
    }

    /// Retail `820D9AB8`. Retail also requires `+2184`, which the ground
    /// update path always satisfies when it calls this (to be verified).
    pub fn accelerate(&mut self, s: &Scripts) {
        let v = self.body.velocity;
        // 0.0125 is the constant at 820029D8.
        let direction = if v.length() >= 0.0125 { v.normalize() } else { self.body.at() };
        let accel = if self.crouched {
            self.stat(s, "Physics_Crouching_Acceleration_stat")
        } else {
            self.stat(s, "Physics_Standing_Acceleration_Stat")
        };
        self.body.velocity += direction * accel * self.dt;
    }

    /// The drive section of retail `820F6978`.
    pub fn drive(&mut self, s: &Scripts, input: &InputState) -> Option<Event> {
        self.update_crouch(input);
        if self.is_braking(s, input) {
            let event = self.brake(s, input);
            self.standing_kick_limit = 0.0;
            event
        } else {
            self.brake_amount = 0.0;
            self.braking = false;
            if self.can_kick(s, input) {
                self.accelerate(s);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p8_formats::qb::Value;
    use p8_formats::qb_key as k;

    /// Synthetic scripts shaped like the retail ones (values are test data).
    fn scripts() -> Scripts {
        let stat = |lo, hi, which: &str| {
            Value::Struct(vec![(0, Value::Pair(lo, hi)), (0, Value::Checksum(k(which)))])
        };
        let physics = Value::Struct(vec![
            (k("physics_standing_acceleration_stat"), stat(5.0, 5.0, "STATS_SPEED")),
            (k("physics_crouching_acceleration_stat"), stat(7.5, 7.5, "STATS_SPEED")),
            (k("skater_max_crouched_kick_speed_stat"), stat(9.5, 9.5, "STATS_SPEED")),
            (k("physics_brake_acceleration"), Value::Int(23)),
            (k("physics_brake_stick_threshold"), Value::Float(0.4)),
            (k("use_new_analog_controls"), Value::Int(1)),
        ]);
        Scripts::new(
            [
                (k("skater_physics"), physics),
                (k("STATS_SPEED"), Value::Int(3)),
                (k("Skater_Default_Stats"), Value::Float(5.0)),
                (k("skater_max_sloped_turn_cosine"), Value::Float(0.5)),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn run(p: &mut CorePhysics, s: &Scripts, input: InputState, seconds: f32) {
        for _ in 0..(seconds / p.dt) as usize {
            p.drive(s, &input);
        }
    }

    #[test]
    fn standing_skater_does_not_push_but_crouching_does_up_to_the_limit() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 2.0);
        run(&mut p, &s, InputState::default(), 1.0);
        assert_eq!(p.body.velocity.z, 2.0, "standing: no kick while moving");
        run(&mut p, &s, InputState { crouch: true, ..Default::default() }, 3.0);
        let speed = p.body.velocity.length();
        assert!(speed > 9.4 && speed <= 9.5 + 7.5 / 60.0 + 1e-4, "crouched kick limit, got {speed}");
    }

    #[test]
    fn crouch_accelerates_from_rest_along_the_board() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        run(&mut p, &s, InputState { crouch: true, ..Default::default() }, 0.5);
        assert!(p.body.velocity.z > 3.5 && p.body.velocity.x == 0.0);
    }

    #[test]
    fn pulling_back_brakes_proportionally_and_then_stops() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 10.0);
        // Stick at 0.7 of 128: (0.7 - 0.4) / 0.6 = half braking.
        let half = InputState { stick_back_raw: 0.7 * 128.0, ..Default::default() };
        p.drive(&s, &half);
        assert!((p.brake_amount - 0.5).abs() < 1e-4);
        assert!((p.body.velocity.z - (10.0 - 0.5 * 23.0 / 60.0)).abs() < 1e-4);
        let full = InputState { brake_digital: true, ..Default::default() };
        let mut stopped = false;
        for _ in 0..120 {
            stopped |= p.drive(&s, &full) == Some(Event::Stopped);
        }
        assert!(stopped && p.body.velocity == Vec3::ZERO);
    }

    #[test]
    fn no_kick_and_steep_boards_block_pushing() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        run(&mut p, &s, InputState { crouch: true, ..Default::default() }, 0.5);
        assert_eq!(p.body.velocity, Vec3::ZERO);
        let mut p = CorePhysics::new(&s);
        let tilt = glam::Mat3::from_rotation_x(1.0); // ~57 degrees
        p.body.matrix = tilt;
        run(&mut p, &s, InputState { crouch: true, ..Default::default() }, 0.5);
        assert_eq!(p.body.velocity, Vec3::ZERO);
    }
}
