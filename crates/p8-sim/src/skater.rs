//! The Project 8 skater state machine. It owns all skater gameplay state.
//!
//! Conventions: metres and seconds, +Y up. `heading` is radians about +Y, and
//! heading 0 faces +Z (the engine-wide convention). The board-forward vector
//! is `(sin h, 0, cos h)`, so increasing heading turns left.
//! `position` is the board/ground contact point.
use crate::input::{Input, ManualGesture};
use crate::tuning::Tuning;
use crate::world::World;
use glam::{Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Ground,
    Air,
    Manual {
        nose: bool,
        balance: f32,
        rate: f32,
    },
    Grind {
        rail: usize,
        t: f32,
        direction: f32,
        balance: f32,
        rate: f32,
    },
    Bail {
        timer: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirTrick {
    None,
    Flip,
    Grab,
}

/// Animation intent published for the host's presentation layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Ride,
    Push,
    Crouch,
    Air,
    Flip,
    Grab,
    Manual,
    NoseManual,
    Grind,
    Bail,
}

#[derive(Clone, Debug)]
pub struct Skater {
    pub position: Vec3,
    /// World velocity, in m/s.
    pub velocity: Vec3,
    /// Board-forward heading, in radians.
    pub heading: f32,
    /// True while rolling backwards relative to the board.
    pub fakie: bool,
    pub ground_normal: Vec3,
    pub state: State,
    pub air_trick: AirTrick,
    /// Seconds since the current trick or state began, for animation.
    pub state_time: f32,
    crouch: f32,
    pushing: bool,
    was_ollie: bool,
    was_flip: bool,
    air_time: f32,
    air_rotation: f32,
    trick_timer: f32,
    clock: f32,
    manual: ManualGesture,
    /// Recent trick names for the HUD, oldest first.
    pub combo: Vec<String>,
    pub last_landing: Option<String>,
}

fn forward(heading: f32) -> Vec3 {
    Vec3::new(heading.sin(), 0.0, heading.cos())
}

fn heading_of(v: Vec3) -> f32 {
    v.x.atan2(v.z)
}

fn wrap(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// Project `v` onto the plane with normal `n`, preserving `v`'s length.
fn on_plane(v: Vec3, n: Vec3) -> Vec3 {
    let p = v - n * v.dot(n);
    if p.length_squared() < 1e-8 {
        v
    } else {
        p.normalize() * v.length()
    }
}

impl Skater {
    pub fn new(position: Vec3, heading: f32, velocity: Vec3) -> Self {
        let state = if velocity.y > 0.5 {
            State::Air
        } else {
            State::Ground
        };
        let mut s = Self {
            position,
            velocity,
            heading,
            fakie: false,
            ground_normal: Vec3::Y,
            state,
            air_trick: AirTrick::None,
            state_time: 0.0,
            crouch: 0.0,
            pushing: false,
            was_ollie: false,
            was_flip: false,
            air_time: 0.0,
            air_rotation: 0.0,
            trick_timer: 0.0,
            clock: 0.0,
            manual: ManualGesture::default(),
            combo: Vec::new(),
            last_landing: None,
        };
        let h = Vec3::new(velocity.x, 0.0, velocity.z);
        if h.length() > 0.5 && forward(heading).dot(h) < 0.0 {
            s.fakie = true;
        }
        s
    }

    /// Travel direction on the ground: board forward, or backwards when fakie.
    pub fn travel(&self) -> Vec3 {
        forward(self.heading) * if self.fakie { -1.0 } else { 1.0 }
    }

    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }

    fn set_state(&mut self, state: State) {
        if std::mem::discriminant(&state) != std::mem::discriminant(&self.state) {
            self.state_time = 0.0;
        }
        self.state = state;
    }

    /// Board orientation for rendering. The board follows the ground normal when
    /// grounded and stays upright in the air.
    pub fn rotation(&self) -> Quat {
        let yaw = Quat::from_rotation_y(self.heading);
        let up = match self.state {
            State::Ground | State::Manual { .. } => self.ground_normal,
            _ => Vec3::Y,
        };
        Quat::from_rotation_arc(Vec3::Y, up) * yaw
    }

    pub fn pose(&self) -> Pose {
        match self.state {
            State::Ground if self.crouch > 0.0 => Pose::Crouch,
            State::Ground if self.pushing => Pose::Push,
            State::Ground => Pose::Ride,
            State::Air => match self.air_trick {
                AirTrick::Flip => Pose::Flip,
                AirTrick::Grab => Pose::Grab,
                AirTrick::None => Pose::Air,
            },
            State::Manual { nose: false, .. } => Pose::Manual,
            State::Manual { nose: true, .. } => Pose::NoseManual,
            State::Grind { .. } => Pose::Grind,
            State::Bail { .. } => Pose::Bail,
        }
    }

    /// Balance for the HUD meter, from -1 to 1, while manualling or grinding.
    pub fn balance(&self, t: &Tuning) -> Option<f32> {
        match self.state {
            State::Manual { balance, .. } => Some(balance / t.manual_limit),
            State::Grind { balance, .. } => Some(balance / t.grind_limit),
            _ => None,
        }
    }

    pub fn step(&mut self, input: &Input, dt: f32, world: &World, t: &Tuning) {
        if dt.is_nan() || dt <= 0.0 {
            return;
        }
        self.clock += dt;
        self.state_time += dt;
        self.manual.update(input.stick.y, self.clock);
        let ollie_released = self.was_ollie && !input.ollie;
        let flip_pressed = input.flip && !self.was_flip;
        match self.state {
            State::Ground => self.ground(input, ollie_released, dt, world, t),
            State::Air => self.air(input, flip_pressed, dt, world, t),
            State::Manual { .. } => self.manualling(input, ollie_released, dt, world, t),
            State::Grind { .. } => self.grinding(input, ollie_released, dt, world, t),
            State::Bail { .. } => self.bailing(dt, world, t),
        }
        self.was_ollie = input.ollie;
        self.was_flip = input.flip;
        if !self.position.is_finite() || !self.velocity.is_finite() {
            // A degenerate collision must never poison the renderer.
            self.velocity = Vec3::ZERO;
            self.position = if self.position.is_finite() {
                self.position
            } else {
                Vec3::ZERO
            };
        }
    }

    // ---- ground ------------------------------------------------------------

    fn ground(&mut self, input: &Input, ollie_released: bool, dt: f32, world: &World, t: &Tuning) {
        let mut speed = self.velocity.length();
        let turn = t.turn_rate_slow
            + (t.turn_rate_fast - t.turn_rate_slow) * (speed / t.max_push_speed).min(1.0);
        self.heading = wrap(self.heading - input.stick.x * turn * dt);
        let travel = on_plane(self.travel(), self.ground_normal).normalize_or_zero();

        self.pushing = input.stick.y > 0.3 && !input.ollie && speed < t.max_push_speed;
        if self.pushing {
            speed += t.push_acceleration * input.stick.y * dt;
        }
        if input.stick.y < -0.5 {
            speed = (speed - t.brake_deceleration * dt).max(0.0);
        }
        speed = (speed - t.rolling_friction * dt).max(0.0);
        // Gravity along the slope. A negative result rolls the skater backwards.
        speed += -t.gravity * t.slope_gravity_scale * travel.y * dt;
        if speed < 0.0 {
            self.fakie = !self.fakie;
            speed = -speed;
        }
        speed = speed.min(t.max_speed);
        let travel = on_plane(self.travel(), self.ground_normal).normalize_or_zero();
        self.velocity = travel * speed;

        if input.ollie {
            self.crouch += dt;
        }
        if ollie_released {
            let bonus = t.ollie_crouch_bonus * (self.crouch / t.ollie_crouch_time).min(1.0);
            self.crouch = 0.0;
            self.velocity += self.ground_normal * (t.ollie_speed + bonus);
            self.launch("Ollie");
            return;
        }
        if !input.ollie {
            self.crouch = 0.0;
        }
        if let Some(nose) = self.manual.take() {
            self.set_state(State::Manual {
                nose,
                balance: self.initial_balance(),
                rate: 0.0,
            });
        }
        self.slide(world, t, dt);
        self.follow_ground(world, t);
    }

    /// Deterministic small starting lean so that every manual needs correcting.
    fn initial_balance(&self) -> f32 {
        if (self.clock * 10.0) as i64 % 2 == 0 {
            0.04
        } else {
            -0.04
        }
    }

    /// Integrate position with horizontal wall collision.
    fn slide(&mut self, world: &World, t: &Tuning, dt: f32) {
        let step = self.velocity * dt;
        let horizontal = Vec3::new(step.x, 0.0, step.z);
        let length = horizontal.length();
        if length > 1e-6 {
            let dir = horizontal / length;
            // Probe at knee height so that kerbs are climbed, not treated as walls.
            let origin = self.position + Vec3::Y * (t.ground_snap + 0.05);
            if let Some(hit) = world.raycast(origin, dir, length + t.body_radius)
                && hit.normal.y < t.min_ground_normal_y
            {
                let n = Vec3::new(hit.normal.x, 0.0, hit.normal.z).normalize_or_zero();
                let into = self.velocity.dot(n);
                if into < 0.0 {
                    let head_on = (-into / self.velocity.length().max(1e-3)).clamp(0.0, 1.0);
                    self.velocity -= n * into;
                    self.velocity *= 1.0 - head_on * (1.0 - t.wall_speed_retention);
                    let h = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
                    if matches!(self.state, State::Ground | State::Manual { .. })
                        && h.length() > 0.2
                    {
                        let travel = h.normalize();
                        self.heading = heading_of(if self.fakie { -travel } else { travel });
                    }
                }
                let allowed = (hit.distance - t.body_radius).max(0.0);
                self.position += dir * allowed.min(length);
                self.position.y += step.y;
                return;
            }
        }
        self.position += step;
    }

    /// Snap to the ground under the board, or leave it (ramps, ledges).
    fn follow_ground(&mut self, world: &World, t: &Tuning) {
        let origin = self.position + Vec3::Y * t.ground_snap;
        match world.raycast(origin, Vec3::NEG_Y, t.ground_snap * 2.0) {
            Some(hit) if hit.normal.y >= t.min_ground_normal_y => {
                self.position = hit.point;
                self.ground_normal = self.ground_normal.lerp(hit.normal, 0.5).normalize();
                // Keep the velocity tangent to the new surface.
                self.velocity = on_plane(self.velocity, self.ground_normal);
            }
            _ => {
                // Rolled off an edge or a ramp lip: keep momentum, including its
                // upward component, as a ballistic launch.
                self.launch("");
            }
        }
    }

    fn launch(&mut self, name: &str) {
        self.set_state(State::Air);
        self.air_time = 0.0;
        self.air_rotation = 0.0;
        self.air_trick = AirTrick::None;
        self.trick_timer = 0.0;
        self.crouch = 0.0;
        self.pushing = false;
        if !name.is_empty() {
            self.combo.push(name.into());
        }
    }

    // ---- air ---------------------------------------------------------------

    fn air(&mut self, input: &Input, flip_pressed: bool, dt: f32, world: &World, t: &Tuning) {
        self.air_time += dt;
        self.velocity.y -= t.gravity * dt;
        let spin = -input.stick.x + if input.spin_left { 1.0 } else { 0.0 }
            - if input.spin_right { 1.0 } else { 0.0 };
        let spin = spin.clamp(-1.0, 1.0) * t.air_spin_rate * dt;
        self.heading = wrap(self.heading + spin);
        self.air_rotation += spin;
        // Slight lateral drift. THPS air control is weak but present.
        let h = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
        if h.length() > 0.5 {
            let left = Vec3::new(h.z, 0.0, -h.x).normalize();
            self.velocity += left * (-input.stick.x) * t.air_control * dt;
        }

        if self.trick_timer > 0.0 {
            self.trick_timer -= dt;
            if self.trick_timer <= 0.0 && self.air_trick == AirTrick::Flip {
                self.air_trick = AirTrick::None;
            }
        }
        if flip_pressed && self.air_trick == AirTrick::None {
            self.air_trick = AirTrick::Flip;
            self.trick_timer = t.flip_duration;
            self.state_time = 0.0;
            self.combo.push("Kickflip".into());
        } else if input.grab && self.air_trick == AirTrick::None {
            self.air_trick = AirTrick::Grab;
            self.state_time = 0.0;
            self.combo.push("Melon".into());
        } else if !input.grab && self.air_trick == AirTrick::Grab {
            self.air_trick = AirTrick::None;
        }

        if input.grind
            && self.air_time > 0.05
            && let Some(p) = world.nearest_rail(self.position, t.grind_snap_distance)
        {
            let rail = world.rails()[p.rail];
            let axis = (rail.end - rail.start).normalize_or_zero();
            let along = self.velocity.dot(axis);
            if along.abs() >= t.grind_min_speed * 0.5 || self.velocity.length() >= t.grind_min_speed
            {
                let direction = if along >= 0.0 { 1.0 } else { -1.0 };
                self.position = p.point;
                self.velocity = axis * direction * along.abs().max(t.grind_min_speed);
                self.heading = heading_of(axis * direction);
                self.fakie = false;
                self.finish_air_combo();
                self.combo.push("50-50".into());
                self.set_state(State::Grind {
                    rail: p.rail,
                    t: p.t,
                    direction,
                    balance: self.initial_balance(),
                    rate: 0.0,
                });
                return;
            }
        }

        let falling = self.velocity.y <= 0.0;
        self.slide(world, t, dt);
        if falling && self.air_time >= t.min_air_time {
            let probe = t.ground_snap.max(-self.velocity.y * dt + 0.05);
            let origin = self.position + Vec3::Y * probe;
            if let Some(hit) = world.raycast(origin, Vec3::NEG_Y, probe + 0.05)
                && hit.normal.y >= t.min_ground_normal_y
            {
                self.position = hit.point;
                self.ground_normal = hit.normal;
                self.land(t);
            }
        }
    }

    fn finish_air_combo(&mut self) {
        let turns = (self.air_rotation.abs() / std::f32::consts::PI).round() as i32;
        if turns > 0 {
            self.combo.push(format!("{} Spin", turns * 180));
        }
        self.air_rotation = 0.0;
    }

    fn land(&mut self, t: &Tuning) {
        // Drop the component into the ground: falling speed is absorbed, not
        // converted into rolling speed.
        let n = self.ground_normal;
        let v = self.velocity - n * self.velocity.dot(n);
        let horizontal = Vec3::new(v.x, 0.0, v.z);
        let board = forward(self.heading);
        let incomplete_flip = self.air_trick == AirTrick::Flip && self.trick_timer > 0.0;
        let clean_forward =
            horizontal.length() < 1.5 || board.angle_between(horizontal) <= t.landing_tolerance;
        let clean_fakie =
            horizontal.length() >= 1.5 && (-board).angle_between(horizontal) <= t.landing_tolerance;
        self.finish_air_combo();
        if incomplete_flip || !(clean_forward || clean_fakie) || self.air_trick == AirTrick::Grab {
            self.bail();
            return;
        }
        if horizontal.length() >= 1.5 {
            self.fakie = !clean_forward;
            let travel = horizontal.normalize();
            // Snap the board exactly onto the landing direction.
            self.heading = heading_of(if self.fakie { -travel } else { travel });
        }
        self.velocity = v;
        self.air_trick = AirTrick::None;
        let manual = self.manual.take();
        self.last_landing = (!self.combo.is_empty()).then(|| self.combo.join(" + "));
        if let Some(nose) = manual {
            self.set_state(State::Manual {
                nose,
                balance: self.initial_balance(),
                rate: 0.0,
            });
        } else {
            self.combo.clear();
            self.set_state(State::Ground);
        }
    }

    fn bail(&mut self) {
        self.combo.clear();
        self.air_trick = AirTrick::None;
        self.last_landing = Some("Bail!".into());
        self.set_state(State::Bail { timer: 0.0 });
    }

    // ---- manual ------------------------------------------------------------

    fn manualling(
        &mut self,
        input: &Input,
        ollie_released: bool,
        dt: f32,
        world: &World,
        t: &Tuning,
    ) {
        let State::Manual {
            nose,
            mut balance,
            mut rate,
        } = self.state
        else {
            return;
        };
        // Manual balance lives on the stick's forward/back axis.
        let correction = input.stick.y * if nose { 1.0 } else { -1.0 };
        rate += (balance.signum() * t.manual_instability + correction * t.manual_correction) * dt;
        balance += rate * dt;
        if balance.abs() > t.manual_limit {
            self.bail();
            return;
        }
        let mut speed = self.velocity.length();
        let turn = t.turn_rate_fast * 0.6;
        self.heading = wrap(self.heading - input.stick.x * turn * dt);
        speed = (speed - t.rolling_friction * dt).max(0.0);
        let travel = on_plane(self.travel(), self.ground_normal).normalize_or_zero();
        speed += -t.gravity * t.slope_gravity_scale * travel.y * dt;
        self.velocity = travel * speed.clamp(0.0, t.max_speed);
        if ollie_released {
            if !self.combo.iter().any(|c| c.contains("Manual")) {
                self.combo.push(if nose {
                    "Nose Manual".into()
                } else {
                    "Manual".into()
                });
            }
            self.velocity += self.ground_normal * t.ollie_speed;
            self.launch("Ollie");
            return;
        }
        self.state = State::Manual {
            nose,
            balance,
            rate,
        };
        self.slide(world, t, dt);
        self.follow_ground(world, t);
        if self.state == State::Air {
            // Rolled off a ledge in a manual: the combo continues in the air.
            self.combo.push(if nose {
                "Nose Manual".into()
            } else {
                "Manual".into()
            });
        }
    }

    // ---- grind -------------------------------------------------------------

    fn grinding(
        &mut self,
        input: &Input,
        ollie_released: bool,
        dt: f32,
        world: &World,
        t: &Tuning,
    ) {
        let State::Grind {
            rail,
            t: mut u,
            direction,
            mut balance,
            mut rate,
        } = self.state
        else {
            return;
        };
        let Some(&r) = world.rails().get(rail) else {
            self.launch("");
            return;
        };
        let span = r.end - r.start;
        let length = span.length().max(1e-3);
        let axis = span / length * direction;
        let mut speed = self.velocity.length();
        speed += -t.gravity * axis.y * dt - t.grind_friction * dt;
        speed = speed.clamp(t.grind_min_speed, t.max_speed);
        rate += (balance.signum() * t.grind_instability - input.stick.x * t.grind_correction) * dt;
        balance += rate * dt;
        if balance.abs() > t.grind_limit {
            self.velocity = axis * speed;
            self.bail();
            return;
        }
        u += direction * speed * dt / length;
        self.velocity = axis * speed;
        if ollie_released {
            self.velocity += Vec3::Y * t.ollie_speed;
            self.launch("Ollie");
            return;
        }
        if !(0.0..=1.0).contains(&u) {
            // Rolled off the end of the rail.
            self.position = r.start + span * u.clamp(0.0, 1.0);
            self.launch("");
            return;
        }
        self.position = r.start + span * u;
        self.heading = heading_of(axis);
        self.state = State::Grind {
            rail,
            t: u,
            direction,
            balance,
            rate,
        };
    }

    // ---- bail --------------------------------------------------------------

    fn bailing(&mut self, dt: f32, world: &World, t: &Tuning) {
        let State::Bail { mut timer } = self.state else {
            return;
        };
        timer += dt;
        let grounded = world
            .raycast(
                self.position + Vec3::Y * 0.3,
                Vec3::NEG_Y,
                0.35 + (-self.velocity.y * dt).max(0.0),
            )
            .filter(|h| h.normal.y >= t.min_ground_normal_y);
        match grounded {
            Some(hit) => {
                self.position = hit.point;
                self.ground_normal = hit.normal;
                let h = Vec3::new(self.velocity.x, 0.0, self.velocity.z);
                let slowed = (h.length() - 12.0 * dt).max(0.0);
                self.velocity = h.normalize_or_zero() * slowed;
            }
            None => self.velocity.y -= t.gravity * dt,
        }
        self.slide(world, t, dt);
        if timer >= t.bail_duration && grounded.is_some() {
            self.velocity = Vec3::ZERO;
            self.fakie = false;
            self.set_state(State::Ground);
        } else {
            self.state = State::Bail { timer };
        }
    }
}
