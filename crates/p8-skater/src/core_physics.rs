//! `SkaterCorePhysics` component: the translated ground update.
//!
//! [`CorePhysics::ground_update`] follows retail `820F6978` step by step.
//! Steps that need level collision, animation or features not yet
//! translated are listed in its docs and are absent, not approximated.
use crate::body::{Body, rotate_about_up};
use crate::input::InputState;
use crate::script::{Scripts, StatContext};
use crate::stats::StatLevels;
use glam::{Mat3, Vec3};

/// Events the translated code sends to scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Retail event "Stopped": `Brake` stopped the skater, or the ground
    /// update found the skater (nearly) still on flat ground.
    Stopped,
    /// Retail event "SteepGround" (bailing on steep ground).
    SteepGround,
    /// Retail event "Ollied": crouch released on the ground (`820D7AB0`).
    /// The skater's `ollie` script handles it by calling `Jump`.
    Ollied,
    /// Retail broadcast "SkaterJump" (end of `Jump`).
    SkaterJump,
    /// Retail event "Landed" (air update landing).
    Landed,
}

/// SkaterState `+24` (set by `SetState`, `820D71B0`). Only the states with
/// translated updates are listed; retail has ten (0..9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// 0: ground update `820F6978`.
    Ground,
    /// 1: air update `820F2310`.
    Air,
}

/// Which way the last ground turn went (`+1940`: checksum "Left"/"Right").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Left,
    Right,
}

/// Script command `OverrideLimits` (`820D5C68`) while active (`+2084` != 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverrideLimits {
    /// `+2100`: gravity while moving up (param "gravity", LIKELY).
    pub gravity: f32,
    /// `+2096`: air friction (param "friction", LIKELY).
    pub friction: f32,
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
    /// `+2637`: blocks kicking (`820D9830`). Cleared by the crouch update and
    /// while moving down. What sets it (`820F2310`) is not translated.
    pub flag_2637: bool,
    /// `+2048`: bert slide (`bertslideon`/`bertslideoff`). Blocks kicking.
    /// The bert-slide turn (`820D78B8`) is not translated; must stay false.
    pub bert_slide: bool,
    /// `+2108`: standing kick speed limit. Every retail write stores 0.
    pub standing_kick_limit: f32,
    /// `+112`: ground normal. Reset (`820D4700`): straight up.
    pub ground_normal: Vec3,
    /// `+144`: the ground normal at the start of the last ground update.
    pub previous_normal: Vec3,
    /// `+32`..`+80`: a second copy of the skater matrix, rotated together
    /// with it. Purpose UNKNOWN (reset copies the object matrix).
    pub matrix_32: Mat3,
    /// `+1568`: set by `StickPulledBack` (bike front-brake only).
    pub front_brake_1568: bool,
    /// `+1656`: last analog brake amount (0..1).
    pub brake_amount: f32,
    /// `+1980`: set while `Brake` runs.
    pub braking: bool,
    /// `+1960`: rolling friction; rewritten every rolling frame.
    pub rolling_friction: f32,
    /// `+1964`: `SetSpecialFriction` extra friction.
    pub special_friction: f32,
    /// `+2020`: speed at the previous ground update.
    pub last_speed: f32,
    /// `+2112`: set when speed jumps (a kick). Its readers (animation,
    /// LIKELY) are not translated.
    pub kick_flag: bool,
    pub override_limits: Option<OverrideLimits>,
    /// `+1388`: powersliding (`enterpowerslide`/`ExitPowerslide`).
    pub powerslide: bool,
    /// `+2184`: `LockVelocityDirection`.
    pub lock_velocity_direction: bool,
    /// `+1944`: turn amount for animation (-1..1, scaled when sharp).
    pub turn_amount: f32,
    /// `+1940` and `+2217`.
    pub last_turn: Option<Turn>,
    /// Terrain under the board (retail index `+298`), as a terrain global name.
    pub terrain: &'static str,
    /// SkaterState `+32` (via `+2848`): crouched.
    pub crouched: bool,
    /// SkaterState `+36`: game time (ms) when crouched last changed.
    pub crouch_changed_ms: i64,
    /// `+2168`: how long the crouch was held when the ollie fired (ms).
    pub crouch_duration_ms: i64,
    /// SkaterState `+24`.
    pub state: State,
    /// Game time in milliseconds (retail `8222A6C0`), advanced by `dt`.
    pub time_ms: i64,
    /// Fractional milliseconds not yet added to `time_ms`.
    pub time_frac_ms: f32,
    /// `+2000`: where the last jump started.
    pub jump_start: Vec3,
    /// `+2128`: spinning blocked (`NoSpin`; `CanSpin` clears). Reset: false.
    pub no_spin: bool,
    /// `+2720`: turning enabled (`enableturning`/`disableturning`). Reset: on.
    pub turning_enabled: bool,
    /// `+2721`: analog turning enabled (`Enable/DisableAnalogTurning`). Reset: on.
    pub analog_turning: bool,
    /// Stance panel `+29`: in nollie (`nollieon`/`nollieoff`). Inverts lean.
    pub nollie: bool,
    /// `+1912`: "lean" angle in degrees. Retail hands it to the model as a
    /// display rotation (`82260550`, about the model's own Y axis); it does
    /// not rotate the physics body.
    pub lean_degrees: f32,
    /// `+2628`: `lean_degrees` before this frame.
    pub previous_lean_degrees: f32,
    /// `+1542`: the lean angle is between 41 and 319 degrees (upside down).
    pub flipping: bool,
    /// Trick component `+5360`: degrees spun this air (for trick names).
    pub spin_degrees: f32,
    /// The input of the current frame (retail reads the Input component).
    pub last_input: InputState,
    /// `+2724`: air gravity multiplier (`AdjustGravity`); 0 = unused.
    pub gravity_multiplier: f32,
    /// SkaterState `+128`: in a bail (`IsInBail`).
    pub in_bail: bool,
    pub stats: StatLevels,
    pub stat_context: StatContext,
}

/// Retail `8262BE78`, cosine (its neighbour `8262BDA0` is sine: CONFIRMED by
/// its series coefficients; together they build rotation matrices).
fn retail_cos(x: f64) -> f32 {
    x.cos() as f32
}

/// Retail `821EDB50`: project `v` onto the plane of `n`, keeping its length.
/// If the projection vanishes, `(-n.z, n.x, -n.y)` is used as the direction.
pub(crate) fn project_keep_length(v: Vec3, n: Vec3) -> Vec3 {
    let length = v.length();
    let mut p = v - n * v.dot(n);
    if p.length() == 0.0 {
        p = Vec3::new(-n.z, n.x, -n.y);
    }
    p.normalize_or_zero() * length
}

/// Retail `821EE530`: signed angle from `a` to `b` around `axis`, after
/// projecting both onto the plane of `axis`.
fn signed_angle(a: Vec3, b: Vec3, axis: Vec3) -> f32 {
    let n = axis.normalize_or_zero();
    let a = (a - n * a.dot(n)).normalize_or_zero();
    let b = (b - n * b.dot(n)).normalize_or_zero();
    let angle = a.dot(b).clamp(-1.0, 1.0).acos();
    if n.cross(a).dot(b) < 0.0 { -angle } else { angle }
}

/// Retail's "reduce this vector by `amount` along itself, stopping at zero"
/// (`820D90E8`, `820D9F70`).
fn slow_down(v: Vec3, amount: f32) -> Vec3 {
    let d = v.normalize_or_zero() * amount;
    if d.length_squared() > v.length_squared() { Vec3::ZERO } else { v - d }
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
            bert_slide: false,
            standing_kick_limit: 0.0,
            ground_normal: Vec3::Y,
            previous_normal: Vec3::Y,
            matrix_32: Mat3::IDENTITY,
            front_brake_1568: false,
            brake_amount: 0.0,
            braking: false,
            rolling_friction: scripts.physics_float("Physics_Rolling_Friction", false),
            special_friction: 0.0,
            last_speed: 0.0,
            kick_flag: false,
            override_limits: None,
            powerslide: false,
            lock_velocity_direction: false,
            turn_amount: 0.0,
            last_turn: None,
            terrain: "terrain_default",
            crouched: false,
            crouch_changed_ms: 0,
            crouch_duration_ms: 0,
            state: State::Ground,
            time_ms: 0,
            time_frac_ms: 0.0,
            jump_start: Vec3::ZERO,
            gravity_multiplier: 0.0,
            last_input: InputState::default(),
            no_spin: false,
            turning_enabled: true,
            analog_turning: true,
            nollie: false,
            lean_degrees: 0.0,
            previous_lean_degrees: 0.0,
            flipping: false,
            spin_degrees: 0.0,
            in_bail: false,
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

    pub(crate) fn rotate(&mut self, angle: f32) {
        rotate_about_up(&mut self.body.matrix, angle);
        rotate_about_up(&mut self.matrix_32, angle);
    }

    /// Retail `820D7C08`: start crouching when the crouch input is held.
    pub fn update_crouch(&mut self, input: &InputState) {
        if !self.crouched && input.crouch {
            self.crouched = true;
            self.crouch_changed_ms = self.time_ms;
            self.flag_2637 = false;
        }
    }

    /// Clear SkaterState "crouched", stamping `+36` (retail inline pattern).
    pub(crate) fn uncrouch(&mut self) {
        if self.crouched {
            self.crouched = false;
            self.crouch_changed_ms = self.time_ms;
        }
    }

    /// Retail `820D7AB0`: releasing the crouch on the ground fires "Ollied".
    pub fn ollie_trigger(&mut self, input: &InputState) -> bool {
        if !self.crouched || input.crouch {
            return false;
        }
        self.crouch_duration_ms = self.time_ms - self.crouch_changed_ms;
        self.uncrouch();
        true
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
        // Holding left or right while pulling back turns sharply instead.
        if !(input.right || input.left) {
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
        if self.ground_normal.y < s.global_float("Skater_max_sloped_turn_cosine") {
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
        if self.no_kick || self.flag_2637 || self.bert_slide {
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

    /// The drive section of retail `820F6978` (no balance trick active; the
    /// manual and skitch branches are not translated).
    pub fn drive(&mut self, s: &Scripts, input: &InputState) -> Option<Event> {
        if self.is_braking(s, input) {
            let event = self.brake(s, input);
            self.standing_kick_limit = 0.0;
            self.kick_flag = false;
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

    /// Retail `820D90E8`: air drag, `dt * speed² * k * 60` along the motion.
    fn air_drag(&mut self, k: f32) {
        let v = self.body.velocity;
        let speed_sq = v.length_squared();
        // 1e-5 is the constant at 82000C18.
        if speed_sq < 1e-5 {
            return;
        }
        self.body.velocity = slow_down(v, self.dt * speed_sq * k * 60.0);
    }

    /// Retail `820D9C48`.
    fn air_friction(&mut self, s: &Scripts) {
        let mut crouched_k = s.physics_float("Physics_Crouched_Air_Friction", self.on_bike);
        let mut k = s.physics_float("Physics_Standing_Air_Friction", self.on_bike);
        if let Some(o) = self.override_limits {
            crouched_k = o.friction;
            k = crouched_k;
        }
        if self.crouched {
            k = crouched_k;
        }
        self.air_drag(k);
    }

    /// Retail `820D9CE8`: on slopes steeper than `Skater_max_sloped_turn_cosine`,
    /// turn the board toward the fall line. Returns whether it applied.
    fn slope_turn(&mut self, s: &Scripts) -> bool {
        if self.body.up().y < 0.0 {
            return false;
        }
        if self.ground_normal.y >= s.global_float("Skater_max_sloped_turn_cosine") {
            return false;
        }
        let v = self.body.velocity;
        // 2.5e-5 is the constant at 820027D8.
        let direction = if v.length() < 2.5e-5 { self.body.at() } else { v };
        let n = self.ground_normal;
        let down = Vec3::NEG_Y - n * Vec3::NEG_Y.dot(n);
        let angle = signed_angle(direction, down, self.body.up());
        if (angle * 57.29578).abs() > s.global_float("Skater_sloped_turn_max_angle_of_approach") {
            return false;
        }
        let rate = s.global_float("Skater_Slow_Turn_on_slopes");
        let sign = if angle < 0.0 { -1.0 } else { 1.0 };
        let mut turn = self.dt * rate * sign;
        if turn.abs() > angle.abs() {
            turn = angle;
        }
        self.rotate(turn);
        true
    }

    /// Retail `820D9F70` (not grinding).
    fn rolling_friction(&mut self, s: &Scripts) {
        let terrain = s.terrain_float(self.terrain, "SKATE_ROLL_FRICTION");
        self.rolling_friction = self.special_friction + terrain;
        // No balance trick is translated, so retail's "+2844 active" is false.
        // 0.02 is the constant at 82002968.
        if self.speed() < 0.02 {
            self.body.velocity = Vec3::ZERO;
            return;
        }
        // 60 is the constant at 82001EB4: friction is per 1/60 s.
        self.body.velocity = slow_down(self.body.velocity, self.dt * 60.0 * self.rolling_friction);
    }

    /// Retail `820E5DB8`.
    fn friction(&mut self, s: &Scripts, gravity_cancelled: bool) {
        if !self.autokick && !self.on_bike && !self.in_bail && self.special_friction == 0.0 {
            return;
        }
        if !gravity_cancelled {
            self.air_friction(s);
        }
        if !self.slope_turn(s) && !gravity_cancelled {
            self.rolling_friction(s);
        }
    }

    /// Retail `820ECEE8` (skater path): steering on the ground.
    pub fn ground_turn(&mut self, s: &Scripts, input: &InputState) {
        self.last_turn = None;
        if self.body.up().y < 0.0 {
            return;
        }
        let speed = self.speed();
        let ground_rotation = |me: &Self| s.physics_float("Physics_Ground_Rotation", me.on_bike);
        let sharp_rotation = |me: &Self| s.physics_float("Physics_Ground_Sharp_Rotation", me.on_bike);
        let mut rate = 0.0;
        if s.physics_float("Use_New_Analog_Controls", false) == 0.0 {
            // Digital controls.
            let (held, ms, sign) = if input.left {
                (true, input.left_held_ms, 1.0)
            } else if input.right {
                (true, input.right_held_ms, -1.0)
            } else {
                (false, 0, 0.0)
            };
            if held {
                if !self.stick_pulled_back(s, input) {
                    rate = sign * ground_rotation(self);
                } else {
                    rate = sign * sharp_rotation(self);
                    // Ramp up over 600 ms when nearly still (0.00166667 at 82002A84).
                    if speed < 0.25 && ms < 600 {
                        rate *= ms as f32 * 0.0016666667;
                    }
                }
            }
        } else {
            let x = input.stick_x_raw * 0.0078125;
            let y = input.stick_back_raw * 0.0078125;
            // Dead zone 0.4, rescaled by 1/0.6 (constants 82000DFC, 82000DF8).
            let mut t = if y <= 0.0 && x.abs() < 0.4 {
                0.0
            } else if x > 0.0 {
                ((x - 0.4) * 1.6666666).max(0.0)
            } else {
                ((x + 0.4) * 1.6666666).min(0.0)
            };
            t = t.clamp(-1.0, 1.0);
            let ramp_time = s.physics_float("Physics_Turn_Ramp_Time", false);
            let mut ramped = false;
            if t.abs() == 0.0 {
                // Digital fallback. Retail multiplies the ramp by t, which is 0
                // here, so a ramped turn stays 0 (kept as found).
                for (held, ms, full) in [(input.left, input.left_held_ms, -1.0), (input.right, input.right_held_ms, 1.0)] {
                    if held {
                        if speed < 0.25 && self.stick_pulled_back(s, input) && (ms as f32) < ramp_time {
                            ramped = true;
                            t *= ms as f32 / ramp_time;
                        } else {
                            t = full;
                        }
                        break;
                    }
                }
            }
            if t.abs() > 0.0 {
                let normal = ground_rotation(self);
                let mut r = normal;
                if self.stick_pulled_back(s, input) {
                    r = sharp_rotation(self);
                    if !ramped {
                        t = if t < 0.0 { -1.0 } else { 1.0 };
                    }
                }
                self.turn_amount = r / normal * t;
                rate = -(r * t);
            } else {
                self.turn_amount = t;
            }
        }
        // The bert-slide branch (+2048, `820D78B8`) is not translated.
        if rate == 0.0 {
            return;
        }
        let angle = self.dt * rate;
        self.last_turn = Some(if angle > 0.0 { Turn::Left } else { Turn::Right });
        if !self.lock_velocity_direction {
            // Rotation about world Y. The -1 at 827329F0 is filled at run time
            // (LIKELY -1: required for this to be a rotation).
            let (s, c) = (angle.sin(), angle.cos());
            let v = self.body.velocity;
            self.body.velocity = Vec3::new(c * v.x + s * v.z, v.y, c * v.z - s * v.x);
        }
        self.rotate(angle);
    }

    /// Retail `820DB318`: point the velocity exactly along the board,
    /// forwards or backwards, keeping the speed.
    fn velocity_along_board(&mut self) {
        let speed = self.speed();
        // 1e-6 is the constant at 8200297C.
        if speed <= 1e-6 {
            return;
        }
        let at = self.body.at();
        let sign = if (self.body.velocity / speed).dot(at) < 0.0 { -1.0 } else { 1.0 };
        self.body.velocity = at * speed * sign;
    }

    /// Retail ground update `820F6978`, for a skater on the ground with no
    /// balance trick, skitch, bike or moving platform.
    ///
    /// Not translated (absent): side and forward collision (`820EB9A0`,
    /// `820EBD20`), ground snapping (`820F12C0`) and the re-move loop,
    /// high-ollie checks (`820D79F8`), animation bookkeeping (`820DE230`,
    /// heading `+2016`), and the steps after steering (`820DBEF0` onward).
    /// The position is advanced by `velocity * dt` exactly as retail does
    /// before collision; keeping the board on the ground is the caller's job
    /// until ground snapping is translated.
    pub fn ground_update(&mut self, s: &Scripts, input: &InputState) -> Vec<Event> {
        let mut events = Vec::new();
        self.kick_flag = false;
        let speed = self.speed();
        if speed - self.last_speed >= s.physics_float("Physics_kick_accel_threshold", self.on_bike) {
            self.kick_flag = true;
        }
        self.last_speed = speed;

        // 0.001 is the constant at 82000D80.
        if speed > 0.001 {
            self.body.velocity = project_keep_length(self.body.velocity, self.ground_normal);
        }
        self.previous_normal = self.ground_normal;
        if self.in_bail && self.ground_normal.y < s.global_float("bail_steep_ground") {
            events.push(Event::SteepGround);
        }

        // Gravity along the ground.
        let mut g = Vec3::new(0.0, s.physics_float("Physics_Ground_Gravity", false), 0.0);
        if let Some(o) = self.override_limits
            && self.body.velocity.y > 0.0
        {
            g = Vec3::new(0.0, o.gravity, 0.0);
        }
        g -= self.ground_normal * g.dot(self.ground_normal);
        let mut gravity_cancelled = false;
        if self.body.velocity.y < 0.0 {
            if self.crouched {
                g.y -= s.physics_float("additional_downhill_gravity", false);
            }
        } else {
            // With a manual active retail uses min_uphill_manual_speed.
            let threshold = s.physics_float("min_uphill_kick_speed", false);
            if !self.powerslide && self.crouched && self.body.at().y > 0.0 && self.speed() < threshold {
                g.y = 0.0;
                gravity_cancelled = true;
            }
        }
        if !gravity_cancelled {
            self.body.velocity += g * self.dt;
        }

        // `820D7C88`: kick flag while "Up" is held, or crouched gently uphill.
        if input.up {
            self.kick_flag = true;
        } else if self.crouched {
            let dir_y = self.body.velocity.normalize_or_zero().y;
            // 0.55 is the constant at 820029A4.
            if dir_y > s.physics_float("Physics_kick_uphill_threshold", self.on_bike) && dir_y < 0.55 {
                self.kick_flag = true;
            }
        }
        // The `+2025` block is skipped: every retail write to `+2025` stores 0.

        if let Some(e) = self.drive(s, input) {
            events.push(e);
        }
        self.friction(s, gravity_cancelled);
        if self.body.velocity.y < 0.0 {
            self.flag_2637 = false;
        }
        // 0.1 and 0.9 are the constants at 82000BF4 and 82002A74.
        if self.speed() < 0.1 && self.ground_normal.y > 0.9 {
            events.push(Event::Stopped);
        }

        // Move (retail adds velocity * dt, then collides and snaps).
        self.body.position += self.body.velocity * self.dt;

        if !self.powerslide {
            self.ground_turn(s, input);
        }
        if !self.lock_velocity_direction {
            self.velocity_along_board();
        }
        // `820D7AB0` runs later in the ground update (after `820DBAA8`).
        if self.ollie_trigger(input) {
            events.push(Event::Ollied);
        }
        events
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
        let f = |name: &str, v: f32| (k(name), Value::Float(v));
        let physics = Value::Struct(vec![
            (k("physics_standing_acceleration_stat"), stat(5.0, 5.0, "STATS_SPEED")),
            (k("physics_crouching_acceleration_stat"), stat(7.5, 7.5, "STATS_SPEED")),
            (k("skater_max_crouched_kick_speed_stat"), stat(9.5, 9.5, "STATS_SPEED")),
            (k("physics_brake_acceleration"), Value::Int(23)),
            f("physics_brake_stick_threshold", 0.4),
            (k("use_new_analog_controls"), Value::Int(1)),
            f("physics_ground_gravity", -9.8),
            f("physics_standing_air_friction", 3e-6),
            f("physics_crouched_air_friction", 1.2e-6),
            f("physics_rolling_friction", 0.1),
            (k("additional_downhill_gravity"), Value::Int(10)),
            f("min_uphill_kick_speed", 8.5),
            f("physics_ground_rotation", 1.8),
            f("physics_ground_sharp_rotation", 3.6),
            (k("physics_turn_ramp_time"), Value::Int(150)),
            f("physics_kick_accel_threshold", 0.25),
            f("physics_kick_uphill_threshold", 0.15),
            f("physics_air_gravity", -21.6),
            (k("physics_jump_speed_stat"), stat(7.6, 7.6, "STATS_AIR")),
            (k("physics_jump_speed_min_stat"), stat(7.0, 7.6, "STATS_AIR")),
            (k("physics_air_rotation_stat"), stat(6.85, 7.75, "STATS_SPIN")),
            (k("physics_air_no_rotate_time"), Value::Int(150)),
            (k("physics_air_ramp_rotate_time"), Value::Int(50)),
            (k("physics_air_no_lean_time"), Value::Int(200)),
            (k("physics_air_ramp_lean_time"), Value::Int(200)),
        ]);
        let terrain = Value::Struct(vec![(
            k("physicsactions"),
            Value::Struct(vec![(k("skate_roll_friction"), Value::Checksum(k("default_friction")))]),
        )]);
        Scripts::new(
            [
                (k("skater_physics"), physics),
                (k("STATS_SPEED"), Value::Int(3)),
                (k("STATS_AIR"), Value::Int(0)),
                (k("STATS_SPIN"), Value::Int(4)),
                f("physics_air_hang_stat", 0.9),
                (k("skater_max_tense_time"), Value::Int(200)),
                f("landing_velocity_factor", 0.35),
                f("Skater_Default_Stats", 5.0),
                f("skater_max_sloped_turn_cosine", 0.5),
                f("default_friction", 0.025),
                (k("terrain_default"), Value::Checksum(k("standard_terrain_default"))),
                (k("standard_terrain_default"), terrain),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn run(p: &mut CorePhysics, s: &Scripts, input: InputState, seconds: f32) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..(seconds / p.dt).round() as usize {
            events.extend(p.step(s, &input, &crate::world::FlatFloor::default()));
        }
        events
    }

    const CROUCH: InputState = InputState {
        crouch: true,
        kick: false,
        up: false,
        brake_digital: false,
        down: false,
        l1: false,
        r1: false,
        l2: false,
        up_held_ms: 0,
        down_held_ms: 0,
        left: false,
        right: false,
        left_held_ms: 0,
        right_held_ms: 0,
        stick_x_raw: 0.0,
        stick_back_raw: 0.0,
        stick_x: 0.0,
        stick_y: 0.0,
    };

    #[test]
    fn terrain_friction_follows_names_to_default_friction() {
        let s = scripts();
        assert_eq!(s.terrain_float("terrain_default", "SKATE_ROLL_FRICTION"), 0.025);
        // Unknown terrain falls back to TERRAIN_DEFAULT.
        assert_eq!(s.terrain_float("missing", "SKATE_ROLL_FRICTION"), 0.025);
        assert_eq!(s.terrain_float("terrain_default", "SKATE_GRIND_FRICTION"), 0.0);
    }

    #[test]
    fn coasting_slows_by_rolling_friction() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        run(&mut p, &s, InputState::default(), 1.0);
        // 0.025 per 1/60 s = 1.5 m/s², plus a little air drag.
        let v = p.body.velocity.z;
        assert!(v < 3.5 && v > 3.45, "got {v}");
        assert!((p.body.position.z - 4.25).abs() < 0.05);
    }

    #[test]
    fn standing_skater_does_not_push_but_crouching_holds_the_kick_limit() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        run(&mut p, &s, InputState::default(), 1.0);
        assert_eq!(p.body.velocity, Vec3::ZERO);
        run(&mut p, &s, CROUCH, 4.0);
        let speed = p.body.velocity.length();
        assert!(speed > 9.3 && speed <= 9.5 + 7.5 / 60.0, "got {speed}");
        assert!(p.body.velocity.x == 0.0 && p.body.velocity.z > 0.0);
    }

    #[test]
    fn full_stick_right_turns_at_ground_rotation_and_velocity_follows() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        let right = InputState { stick_x_raw: 127.0, ..Default::default() };
        run(&mut p, &s, right, 0.5);
        assert_eq!(p.last_turn, Some(Turn::Right));
        // Right is away from row 0 (+X at identity). Stick 127/128 past the
        // 0.4 dead zone gives t = (127/128 - 0.4) / 0.6; 1.8 rad/s for 0.5 s.
        let t = (127.0 / 128.0 - 0.4) / 0.6;
        let at = p.body.at();
        assert!((at.x.atan2(at.z) + 0.9 * t).abs() < 1e-3, "at {at}");
        let dir = p.body.velocity.normalize();
        assert!(dir.dot(at) > 0.9999);
    }

    #[test]
    fn stick_inside_the_dead_zone_does_not_turn() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        run(&mut p, &s, InputState { stick_x_raw: 0.39 * 128.0, ..Default::default() }, 0.5);
        assert_eq!(p.body.at(), Vec3::Z);
    }

    #[test]
    fn rolls_down_a_slope_and_crouching_adds_downhill_gravity() {
        let s = scripts();
        let slope = |crouch: bool| {
            let mut p = CorePhysics::new(&s);
            // 20 degree slope falling toward +Z; board faces down it.
            let a = 20f32.to_radians();
            p.ground_normal = Vec3::new(0.0, a.cos(), a.sin());
            p.body.matrix = Mat3::from_rotation_x(a);
            let input = InputState { crouch, ..Default::default() };
            // Crouching also kicks; block that to isolate gravity.
            p.no_kick = true;
            run(&mut p, &s, input, 1.0);
            p.body.velocity.length()
        };
        let standing = slope(false);
        let crouched = slope(true);
        assert!(standing > 1.5 && crouched > standing, "{standing} {crouched}");
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
        let events = run(&mut p, &s, full, 2.0);
        assert!(events.contains(&Event::Stopped) && p.body.velocity == Vec3::ZERO);
    }

    #[test]
    fn no_kick_and_steep_boards_block_pushing() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        run(&mut p, &s, CROUCH, 0.5);
        assert_eq!(p.body.velocity, Vec3::ZERO);
        let mut p = CorePhysics::new(&s);
        p.update_crouch(&CROUCH);
        p.body.matrix = Mat3::from_rotation_x(1.0); // ~57 degrees
        for _ in 0..30 {
            p.drive(&s, &CROUCH);
        }
        assert_eq!(p.body.velocity, Vec3::ZERO);
    }

    /// Crouch for `hold` seconds, release, and fly until landing.
    fn ollie(p: &mut CorePhysics, s: &Scripts, hold: f32) -> (f32, f32, Vec<Event>) {
        run(p, s, CROUCH, hold);
        let mut events = Vec::new();
        let (mut apex, mut air_time) = (0.0f32, 0.0);
        for _ in 0..300 {
            events.extend(p.step(s, &InputState::default(), &crate::world::FlatFloor::default()));
            if p.state == State::Air {
                air_time += p.dt;
            }
            apex = apex.max(p.body.position.y);
            if events.contains(&Event::Landed) {
                break;
            }
        }
        (apex, air_time, events)
    }

    #[test]
    fn a_quick_ollie_uses_the_min_jump_speed_and_lands() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        let (apex, air_time, events) = ollie(&mut p, &s, 1.0 / 60.0);
        assert!(events.contains(&Event::Ollied) && events.contains(&Event::Landed));
        // Stat level 5 of min (7.0, 7.6): 7.3 m/s up; tense 17 ms of 200.
        let v = 7.3 + 17.0 / 200.0 * 0.3;
        let g = 21.6 / 0.9;
        assert!((apex - v * v / (2.0 * g)).abs() < 0.03, "apex {apex}");
        assert!((air_time - 2.0 * v / g).abs() < 0.04, "air {air_time}");
        assert_eq!(p.state, State::Ground);
        assert!(p.body.position.y.abs() < 0.01);
        // Straight down onto flat ground: the keep-length projection has no
        // direction, so retail 821EDB50 falls back to (-n.z, n.x, -n.y) =
        // -Z, keeping 0.35 of the landing speed there (to be compared with
        // the original game).
        let landing = v; // symmetric flight
        assert!((p.body.velocity.z + 0.35 * landing).abs() < 0.1, "vel {}", p.body.velocity);
    }

    #[test]
    fn holding_the_crouch_200ms_gives_the_full_jump() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        let (apex, _, _) = ollie(&mut p, &s, 0.5);
        let g = 21.6 / 0.9;
        assert!((apex - 7.6 * 7.6 / (2.0 * g)).abs() < 0.03, "apex {apex}");
    }

    #[test]
    fn landing_turns_part_of_the_fall_into_forward_speed() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        p.last_speed = 5.0;
        let (_, _, events) = ollie(&mut p, &s, 0.5);
        assert!(events.contains(&Event::Landed));
        let speed = p.body.velocity.length();
        // Friction takes the 5 m/s down to about 4.25 while crouching; the
        // landing then adds part of the 7.6 m/s fall.
        assert!(speed > 5.0 && p.body.velocity.y.abs() < 1e-4, "speed {speed}");
    }

    fn heading(p: &CorePhysics) -> f32 {
        let at = p.body.at();
        at.x.atan2(at.z)
    }

    #[test]
    fn stick_spins_only_after_the_no_rotate_time_then_at_the_rotation_stat() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.set_state(State::Air);
        p.body.position.y = 50.0;
        let floor = crate::world::FlatFloor::default();
        let mut input = InputState { stick_x: 1.0, right: true, ..Default::default() };
        // Held 100 ms: inside Physics_Air_No_Rotate_Time (150).
        input.right_held_ms = 100;
        p.step(&s, &input, &floor);
        assert_eq!(heading(&p), 0.0);
        // Held 300 ms: full rate, stat level 5 of (6.85, 7.75) = 7.3 rad/s.
        input.right_held_ms = 300;
        for _ in 0..6 {
            p.step(&s, &input, &floor);
        }
        assert!((heading(&p) + 7.3 * 0.1).abs() < 1e-3, "heading {}", heading(&p));
        assert!((p.spin_degrees + 7.3 * 0.1 * 57.29578).abs() < 0.1);
    }

    #[test]
    fn shoulder_buttons_spin_at_once() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.set_state(State::Air);
        p.body.position.y = 50.0;
        let input = InputState { l1: true, ..Default::default() };
        p.step(&s, &input, &crate::world::FlatFloor::default());
        // L1 = spin input -1 -> positive angle ("Left").
        assert!((heading(&p) - 7.3 / 60.0).abs() < 1e-4);
        assert_eq!(p.last_turn, Some(Turn::Left));
    }

    #[test]
    fn lean_needs_l2_and_eases_back_when_released() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.set_state(State::Air);
        p.body.position.y = 50.0;
        let floor = crate::world::FlatFloor::default();
        let up = InputState { stick_y: -1.0, up: true, up_held_ms: 500, ..Default::default() };
        p.step(&s, &up, &floor);
        assert_eq!(p.lean_degrees, 0.0, "no L2, no lean");
        p.lean_degrees = 30.0;
        p.step(&s, &InputState::default(), &floor);
        assert!((p.lean_degrees - 27.0).abs() < 1e-4, "eases 10% toward 0");
    }
}
