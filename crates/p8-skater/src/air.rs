//! Jumping, the air update and landing, plus the per-frame state dispatch.
use crate::core_physics::{CorePhysics, Event, State};
use crate::input::InputState;
use crate::script::Scripts;
use crate::world::World;
use glam::{Mat3, Vec3};

/// Retail `821E9B58(m, 1)`: rebuild the other rows around the up row.
/// The row more perpendicular to up is kept as the reference.
pub fn orthonormalize_keep_up(m: &mut Mat3) {
    let up = m.y_axis;
    if m.x_axis.dot(up).abs() < up.dot(m.z_axis).abs() {
        m.z_axis = m.x_axis.cross(up).normalize_or_zero();
        m.x_axis = up.cross(m.z_axis).normalize_or_zero();
    } else {
        m.x_axis = up.cross(m.z_axis).normalize_or_zero();
        m.z_axis = m.x_axis.cross(up).normalize_or_zero();
    }
}

impl CorePhysics {
    /// Retail `820D71B0` (SetState): only the parts that affect the
    /// translated physics (timestamps and trigger bookkeeping omitted).
    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// Retail `820D76C0`: air gravity, `Physics_Air_Gravity / Physics_Air_hang_Stat`
    /// (the vert-air hang stat applies on vert, not translated), times the
    /// `AdjustGravity` multiplier when set. The moon cheat is not translated.
    pub fn air_gravity(&self, s: &Scripts) -> f32 {
        let mut g = s.physics_float("Physics_Air_Gravity", self.on_bike) / s.global_float("Physics_Air_hang_Stat");
        if self.gravity_multiplier != 0.0 {
            g *= self.gravity_multiplier;
        }
        g
    }

    /// Retail `Jump` (`820F0730`), ground path, without script parameters
    /// (the `ollie` script calls it plainly unless it passes `speed`).
    /// `speed` is the script's `jump speed = ...` override.
    pub fn jump(&mut self, s: &Scripts, speed: Option<f32>) -> Vec<Event> {
        self.jump_start = self.body.position;
        // `820DA3D0` (vert push-out and quick-exit checks) is not translated.
        let max_tense = s.global_float("Skater_max_tense_time") as i64;
        self.crouch_duration_ms = self.crouch_duration_ms.min(max_tense);
        // Retail picks vert jump stats when on vert (SkaterState +56/+72) or
        // not on the ground; those states are not translated.
        // 0.3 is the double at 82002AD0: board pointing up = launch ramp.
        let launch = self.body.at().y > 0.3;
        let (max_name, min_name) = if launch {
            ("Physics_Launch_Jump_Speed_Stat", "Physics_Launch_Jump_Speed_min_Stat")
        } else {
            ("Physics_Jump_Speed_Stat", "Physics_Jump_Speed_min_Stat")
        };
        let max = s.stat(max_name, self.on_bike, &self.stats, self.stat_context);
        let mut jump = s.stat(min_name, self.on_bike, &self.stats, self.stat_context);
        if max_tense != 0 {
            jump += self.crouch_duration_ms as f32 / max_tense as f32 * (max - jump);
        }
        if let Some(speed) = speed {
            jump = speed;
        }
        self.uncrouch();
        let v = &mut self.body.velocity;
        if v.y < 0.0 {
            v.y = 0.0;
        }
        v.y += jump;
        // Upside down (-0.1 at 820029A0): jump the other way and step off.
        if self.body.up().y < -0.1 {
            self.body.velocity.y += jump * -1.5;
            self.body.position += self.body.up() * 0.3;
        }
        self.set_state(State::Air);
        vec![Event::SkaterJump]
    }

    /// Retail air update `820F2310`, for a plain ollie: gravity, the move,
    /// and landing on a skatable surface.
    ///
    /// Not translated (absent): air rotation and spins (`820E9620`), board
    /// and body lean (`820EA0D0`, `820E4AD0`, `820EEB38`), vert air and lip
    /// checks, wall collision (`820EF410`), bails on landing, moving
    /// platforms, and the nose/tail landing feelers (`820E5250`, which only
    /// record contact).
    pub fn air_update(&mut self, s: &Scripts, world: &dyn World) -> Vec<Event> {
        let mut events = Vec::new();
        // Object +128: the position at the start of this frame (LIKELY: the
        // object update stores it before the physics runs).
        let old = self.body.position;
        let g = Vec3::new(0.0, self.air_gravity(s), 0.0);
        self.standing_kick_limit = 0.0;
        self.turn_amount = 0.0;
        self.flag_2637 = false;
        self.bert_slide = false;
        self.kick_flag = false;
        self.last_turn = None;

        let dt = self.dt;
        self.body.position += self.body.velocity * dt + g * (dt * dt * 0.5);
        self.body.velocity += g * dt;

        // Landing: feeler from last position to this one.
        let Some(hit) = world.feeler(old, self.body.position) else {
            return events;
        };
        if !hit.skatable {
            return events;
        }
        // Hitting a ceiling (-0.01 at 82001BA8): back off and fall.
        if hit.normal.y < -0.01 {
            self.body.position = old;
            self.body.velocity.y = -0.254;
            return events;
        }
        // 0.0025 is the constant at 820027D0.
        self.body.position = hit.point + hit.normal * 0.0025;
        self.land(s, hit.normal, &mut events);
        events
    }

    /// The landing path of `820F2310` (not from vert).
    fn land(&mut self, s: &Scripts, n: Vec3, events: &mut Vec<Event>) {
        self.set_state(State::Ground);
        self.last_speed = self.body.velocity.length();
        let v = self.body.velocity;
        // Retail also checks vert-landing flags +2131/+2135 (not translated).
        let still = v.x == 0.0 && v.z == 0.0;
        let input = self.last_input;
        if still && self.stick_pulled_back(s, &input) {
            self.body.velocity.y = 0.0;
            self.body.velocity -= n * self.body.velocity.dot(n);
        } else {
            let dir = v.normalize_or_zero();
            let along = project_keep_length(v, n);
            let flat = v - n * v.dot(n);
            let d = dir.dot(n).abs();
            let k = d * s.global_float("landing_velocity_factor");
            // The steeper the landing, the more of the speed is kept along
            // the ground (820F3BF0..3C3C).
            self.body.velocity = along * k + flat * (1.0 - k);
        }
        // 0.064516 = 0.254² (constant at 82002ADC).
        if self.body.velocity.length_squared() < 0.064516 {
            self.body.velocity = Vec3::ZERO;
        }
        self.ground_normal = n;
        self.previous_normal = n;
        self.body.matrix.y_axis = n;
        orthonormalize_keep_up(&mut self.body.matrix);
        self.matrix_32 = self.body.matrix;
        events.push(Event::Landed);
    }

    /// One physics frame, as retail's component update `820FC990` runs it:
    /// the crouch update, then the update for the current state. The
    /// "Ollied" event is handled the way the `ollie` script does: `Jump`.
    pub fn step(&mut self, s: &Scripts, input: &InputState, world: &dyn World) -> Vec<Event> {
        self.time_frac_ms += self.dt * 1000.0;
        let whole = self.time_frac_ms.floor();
        self.time_ms += whole as i64;
        self.time_frac_ms -= whole;
        self.last_input = *input;
        self.update_crouch(input);
        let mut events = match self.state {
            State::Ground => self.ground_update(s, input),
            State::Air => self.air_update(s, world),
        };
        if events.contains(&Event::Ollied) {
            events.extend(self.jump(s, None));
        }
        events
    }
}

/// Retail `821EDB50` (shared with the ground update).
fn project_keep_length(v: Vec3, n: Vec3) -> Vec3 {
    crate::core_physics::project_keep_length(v, n)
}
