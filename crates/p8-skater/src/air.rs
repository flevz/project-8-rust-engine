//! Jumping, the air update and landing, plus the per-frame state dispatch.
use crate::core_physics::{CorePhysics, Event, State};
use crate::input::InputState;
use crate::script::Scripts;
use crate::world::World;
use glam::{Mat3, Vec3};

/// Retail `821E9B58(m, 2)`: rebuild the other rows around the at row
/// (same rule as [`orthonormalize_keep_up`], rows rotated).
pub fn orthonormalize_keep_at(m: &mut Mat3) {
    let at = m.z_axis;
    if m.y_axis.dot(at).abs() < at.dot(m.x_axis).abs() {
        m.x_axis = m.y_axis.cross(at).normalize_or_zero();
        m.y_axis = at.cross(m.x_axis).normalize_or_zero();
    } else {
        m.y_axis = at.cross(m.x_axis).normalize_or_zero();
        m.x_axis = m.y_axis.cross(at).normalize_or_zero();
    }
}

/// Retail `820E0958`: can this hit surface be skated? Flag 0x1 yes, 0x2 no,
/// 0x8 yes, 0x4 no; otherwise only if it is flatter than
/// `Wall_Non_Skatable_Angle` degrees from vertical.
pub fn surface_skatable(s: &Scripts, hit: &crate::world::Hit) -> bool {
    let f = hit.flags;
    if f & 0x1 != 0 {
        return true;
    }
    if f & 0x2 != 0 {
        return false;
    }
    if f & 0x8 != 0 {
        return true;
    }
    if f & 0x4 != 0 {
        return false;
    }
    // 0.0174533 = degrees to radians (82000C10); sine is 8262BDA0.
    hit.normal.y >= (s.global_float("Wall_Non_Skatable_Angle") * 0.017453292).sin()
}

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
        // Leaving the lip: back to where the lip started (+1248), which is
        // then cleared.
        if self.state == State::Lip && state != State::Lip && self.lip_pos != Vec3::ZERO {
            self.body.position = self.lip_pos;
            self.old_position = self.lip_pos;
        }
        if state != State::Lip {
            self.lip_pos = Vec3::ZERO;
        }
        // Entering the air clears the trick component's spin count (820D7438).
        if state == State::Air && self.state != State::Air {
            self.spin_degrees = 0.0;
            // 820D7430: the time the skater left the ground (+2176).
            self.air_start_ms = self.time_ms;
        }
        self.state = state;
    }

    /// Retail `820E9620`: spinning and "lean" in the air.
    ///
    /// Not translated: the vert auto-turn (needs vert air), `SmoothSpin`
    /// (`+2752`), and the Nail the Trick checks.
    pub fn air_rotation(&mut self, s: &Scripts, input: &InputState) {
        if self.in_bail {
            return;
        }
        let pf = |name: &str| s.physics_float(name, false);
        let stat = |me: &Self, name: &str| s.stat(name, me.on_bike, &me.stats, me.stat_context);
        // Lean input (stick Y, else Up -1 / Down +1) and spin input (stick X).
        let mut lean_in = input.stick_y;
        let mut spin_in = input.stick_x;
        if lean_in == 0.0 {
            if input.up {
                lean_in = -1.0;
            } else if input.down {
                lean_in = 1.0;
            }
        }
        if !self.analog_turning {
            spin_in = 0.0;
            lean_in = 0.0;
        }
        // L1 / R1 spin (record R1's "held" check uses +224; "+128" is L1).
        let mut buttons = false;
        if input.l1 && !input.r1 {
            spin_in = -1.0;
            buttons = true;
        }
        if input.r1 && !input.l1 {
            spin_in = 1.0;
            buttons = true;
        }
        if self.analog_turning && spin_in == 0.0 {
            if input.right {
                spin_in = 1.0;
            } else if input.left {
                spin_in = -1.0;
            }
        }
        // With L2 and both inputs, the pair is normalised.
        if input.l2 && lean_in != 0.0 && spin_in != 0.0 {
            let v = glam::Vec2::new(spin_in, lean_in).normalize();
            spin_in = v.x;
            lean_in = v.y;
        }
        if self.nollie {
            lean_in = -lean_in;
        }

        // Lean: only with L2 (Physics_Air_Lean_fast_stat), after Up/Down
        // have been held Physics_Air_No_Lean_Time, ramped over
        // Physics_Air_Ramp_Lean_Time.
        let no_lean = pf("Physics_Air_No_Lean_Time");
        let ramp_lean = pf("Physics_Air_Ramp_Lean_Time");
        let lean_rate = if input.l2 { stat(self, "Physics_Air_Lean_fast_stat") } else { 0.0 };
        let mut lean = 0.0;
        let mut lean_held = 0.0;
        if lean_in != 0.0 {
            lean = -(lean_rate * lean_in);
            lean_held = if input.up { input.up_held_ms } else { input.down_held_ms } as f32;
        }
        if lean_held <= no_lean {
            lean = 0.0;
        }
        if lean_held < ramp_lean {
            lean *= (lean_held - no_lean) / ramp_lean;
        }

        // Spin.
        let mut spin = 0.0;
        if self.turning_enabled && !self.no_spin {
            let rate = if input.l2 {
                stat(self, "Physics_Air_Rotation_fast_stat")
            } else {
                stat(self, "Physics_Air_Rotation_stat")
            };
            let mut held = 0.0;
            if spin_in != 0.0 {
                spin = -(rate * spin_in);
                held = if input.left { input.left_held_ms } else { input.right_held_ms } as f32;
            }
            if !buttons {
                let no_rotate = pf("Physics_Air_No_Rotate_Time");
                let ramp_rotate = pf("Physics_Air_Ramp_Rotate_Time");
                if held <= no_rotate {
                    spin = 0.0;
                } else if held - no_rotate < ramp_rotate {
                    spin *= (held - no_rotate) / ramp_rotate;
                }
            }
        } else if buttons && !self.no_spin {
            spin = -(stat(self, "Physics_Air_Rotation_stat") * spin_in);
        }
        if spin != 0.0 {
            let angle = self.dt * spin;
            self.last_turn = Some(if angle > 0.0 { crate::core_physics::Turn::Left } else { crate::core_physics::Turn::Right });
            self.rotate(angle);
            self.spin_degrees += angle * 57.29578;
        }

        // Lean angle (display), or ease it back to a whole turn.
        self.previous_lean_degrees = self.lean_degrees;
        let mut changed = false;
        if lean != 0.0 {
            self.lean_degrees += self.dt * lean * 57.29578;
            changed = true;
        } else {
            let x = (self.lean_degrees as i32 % 360) as f32;
            if x != 0.0 {
                let correction = if x > 0.0 {
                    if x > 180.0 { (360.0 - x) * 0.1 } else { x * -0.1 }
                } else if x < -180.0 {
                    (-360.0 - x) * 0.1
                } else {
                    x * -0.1
                };
                self.lean_degrees += correction;
                changed = true;
            }
        }
        if changed {
            let a = (self.lean_degrees as i32 % 360).abs();
            self.flipping = (41..=319).contains(&a);
        }
    }

    /// Retail `820E4AD0`: level the board out in the air when there is
    /// ground below. Called every air frame outside vert air (`820F24B4`).
    ///
    /// A feeler looks 12.7 m straight down (82002A4C); if it finds a surface
    /// without flag 0x8 whose normal.y is at least 0.2 (82000D34), and the
    /// board's up.y is at most 0.975 (82002A60), the matrix turns about the
    /// horizontal axis `h x Y` by `dt * Physics_recover_rate_stat`, `h` being
    /// the flat direction up leans toward: up moves toward straight up.
    ///
    /// Skipped as retail skips it: in a spine transfer (SkaterState `+136`,
    /// read by `IsInSpineTransfer`), when the
    /// ground normal is upside down (`+116` < -0.1), and when moving up
    /// while on a movable object (SkaterPhysicsControl `+2852`->`+24`,
    /// the pointer `HasMovableContact` tests; the level has none). Once it
    /// finds that ground it leaves vert air and sets SkaterState `+144`.
    pub fn air_recover(&mut self, s: &Scripts, world: &dyn World) {
        // -0.1 is the constant at 820029A0.
        if self.transfer.active || self.ground_normal.y < -0.1 {
            return;
        }
        let pos = self.body.position;
        let Some(hit) = world.feeler(pos, pos - Vec3::Y * 12.7, 0x10, 0) else {
            return;
        };
        if hit.flags & 0x8 != 0 || hit.normal.y < 0.2 {
            return;
        }
        // 820E4BE0: out of vert air (and +1380), and +144.
        self.vert.in_vert_air = false;
        self.transfer.retry = false;
        self.vert.over_ground = true;
        if self.body.up().y > 0.975 {
            return;
        }
        let rate = s.stat("Physics_recover_rate_stat", self.on_bike, &self.stats, self.stat_context);
        let up = self.body.up();
        let h = Vec3::new(up.x, 0.0, up.z).normalize_or_zero();
        let axis = Vec3::new(-h.z, 0.0, h.x);
        // `820D7F00`: every matrix row rotated about the axis (`820A8A38`,
        // right-handed), then copied to `+32`.
        let r = glam::Quat::from_axis_angle(axis, self.dt * rate);
        let m = &mut self.body.matrix;
        m.x_axis = r * m.x_axis;
        m.y_axis = r * m.y_axis;
        m.z_axis = r * m.z_axis;
        self.matrix_32 = self.body.matrix;
    }

    /// Retail `820D76C0`: air gravity, `Physics_Air_Gravity / Physics_Air_hang_Stat`
    /// (`Physics_Vert_hang_Stat` in vert air or a spine transfer, SkaterState
    /// `+56` or `+136`), times the `AdjustGravity` multiplier when set. The
    /// moon cheat is not translated.
    pub fn air_gravity(&self, s: &Scripts) -> f32 {
        self.air_gravity_hang(s, self.transfer.active)
    }

    /// [`CorePhysics::air_gravity`] with `transfer` standing for SkaterState
    /// `+136` (the transfer code sets it around its own calls).
    pub(crate) fn air_gravity_hang(&self, s: &Scripts, transfer: bool) -> f32 {
        let vert = self.vert.in_vert_air || transfer;
        let hang = if vert { "Physics_Vert_hang_Stat" } else { "Physics_Air_hang_Stat" };
        let mut g = s.physics_float("Physics_Air_Gravity", self.on_bike) / s.global_float(hang);
        if self.gravity_multiplier != 0.0 {
            g *= self.gravity_multiplier;
        }
        g
    }

    /// Retail `Jump` (`820F0730`), ground path, without script parameters
    /// (the `ollie` script calls it plainly unless it passes `speed`).
    /// `speed` is the script's `jump speed = ...` override.
    pub fn jump(&mut self, s: &Scripts, speed: Option<f32>) -> Vec<Event> {
        // Only the ground path (820F0900) records where the jump started;
        // the air path (820F0850, a late ollie) only plays a sound.
        if self.state == State::Ground {
            self.jump_start = self.body.position;
            // 820F09E4: the vert takeoff, then SkaterState +80 is set.
            self.vert_takeoff(s);
            self.set_break_window(true);
        }
        let max_tense = s.global_float("Skater_max_tense_time") as i64;
        self.crouch_duration_ms = self.crouch_duration_ms.min(max_tense);
        // 820F0A4C: vert jump stats in vert air, or on the ground on a vert
        // surface. 0.3 is the double at 82002AD0: board pointing up =
        // launch ramp. (The `BonelessHeight` variants are not translated.)
        let vert = self.vert.in_vert_air || (self.state == State::Ground && self.vert.on_vert_ground);
        let launch = self.body.at().y > 0.3;
        let (max_name, min_name) = if vert {
            ("Physics_Vert_Jump_Speed_Stat", "Physics_Vert_Jump_Speed_min_Stat")
        } else if launch {
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
        // 820F0C80: moving down in vert air, jump out along the eased
        // normal `+96` and leave vert air; the upward part is then 0.
        if self.vert.in_vert_air && self.body.velocity.y < 0.0 {
            self.body.velocity += self.vert.eased_normal * jump;
            self.vert.in_vert_air = false;
            // 820F0D00: and +1380.
            self.transfer.retry = false;
            jump = 0.0;
        } else if self.body.velocity.y < 0.0 {
            self.body.velocity.y = 0.0;
        }
        self.body.velocity.y += jump;
        // Upside down (-0.1 at 820029A0): jump the other way and step off.
        if self.body.up().y < -0.1 {
            self.body.velocity.y += jump * -1.5;
            self.body.position += self.body.up() * 0.3;
        }
        self.set_state(State::Air);
        // 820F0EA8: the jump time +2540.
        self.jump_ms = self.time_ms;
        vec![Event::SkaterJump]
    }

    /// Retail air update `820F2310`, for a plain ollie: gravity, the move,
    /// and landing on a skatable surface.
    ///
    /// Not translated (absent): `820EEB38`
    /// (bikes only), lip checks, bails on landing, moving platforms, and the
    /// nose/tail landing feelers (`820E5250`, which only record contact).
    pub fn air_update(&mut self, s: &Scripts, world: &dyn World) -> Vec<Event> {
        let mut events = Vec::new();
        // Object +128: the position at the start of this frame (LIKELY: the
        // object update stores it before the physics runs).
        let old = self.old_position;
        let g = Vec3::new(0.0, self.air_gravity(s), 0.0);
        // 820F2410: SkaterState +72 (on vert ground) is cleared.
        self.vert.on_vert_ground = false;
        // 820F2424: no acid drop (+200) while in vert air, a transfer, +192
        // or a bail.
        if self.vert.in_vert_air || self.transfer.active || self.transfer.flag_192 || self.in_bail {
            self.set_no_acid_drop(true);
        }
        self.standing_kick_limit = 0.0;
        self.turn_amount = 0.0;
        self.flag_2637 = false;
        self.bert_slide = false;
        self.kick_flag = false;
        self.last_turn = None;
        let input = self.last_input;
        self.air_rotation(s, &input);
        self.transfer_blend();
        // 820F24B4: the leveling is skipped in vert air, unless SkaterState
        // +144 is set or the spine button is held.
        if self.vert.over_ground || self.spine_button(&input) || !self.vert.in_vert_air {
            self.air_recover(s, world);
        }
        // `820D79F8` runs here (not translated). 820F2518: outside vert air
        // and spine transfers (and the rotate component and bike state
        // +152, not translated), the sideways uprighting.
        if !self.vert.in_vert_air && !self.transfer.active {
            self.upright_sideways(s);
        }

        let dt = self.dt;
        // 820F26DC: in a transfer the carry (SkaterState +272) is added to
        // the move, except when falling below +2172: then it shrinks to
        // length 0.1 (82000BF4) and is not added.
        let mut mv = self.body.velocity;
        if self.transfer.active {
            if self.body.velocity.y < 0.0 && self.body.position.y < self.transfer.ref_height {
                self.transfer.carry = self.transfer.carry.normalize_or_zero() * 0.1;
            } else {
                mv += self.transfer.carry;
            }
        }
        self.body.position += mv * dt + g * (dt * dt * 0.5);
        self.body.velocity += g * dt;
        self.vert_air_update(s, world);
        // 820F3018: in vert air the normal easing runs; otherwise the
        // second matrix copy follows the matrix.
        if self.vert.in_vert_air {
            self.ease_normal(s);
        } else {
            self.matrix_32 = self.body.matrix;
        }

        // `820EF410`: walls ahead. When it handled the frame, retail skips
        // the landing (820F369C).
        let handled = self.air_forward_collision(s, old, world);
        // The pitch-bail check (820F31E4, `Pitch_Bail_Feeler_Length` and
        // script `Pitch_Bail_Check`) leads into bails: not translated.

        // Landing: feeler from last position to this one, ignoring surfaces
        // with flag 0x10 (`820E5048(16, 0)` at 820F365C).
        let Some(hit) = world.feeler(old, self.body.position, 0x10, 0) else {
            return events;
        };
        if handled {
            return events;
        }
        // 820F36B4: a ledge to step onto instead (`820E4DB8`) wins when
        // rising faster than 0.25 (82000BEC) or the hit is steep (normal.y
        // below 0.1), and 500 ms have passed since SkaterPhysicsControl
        // `+116` (UNKNOWN; nothing translated sets it, so always).
        let steep = hit.normal.y < 0.1;
        if self.air_snap_up(s, old, world) && (self.body.velocity.y > 0.25 || steep) {
            return events;
        }
        let skatable = surface_skatable(s, &hit);
        if skatable && hit.normal.y < -0.01 {
            // Hitting a ceiling (-0.01 at 82001BA8): back off and fall.
            self.body.position = old;
            self.body.velocity.y = -0.254;
            return events;
        }
        // 0.0025 is the constant at 820027D0.
        self.body.position = hit.point + hit.normal * 0.0025;
        if !skatable {
            self.air_hit_wall(s, hit.normal, &mut events);
            return events;
        }
        self.terrain = hit.terrain;
        self.land(s, hit.normal, &mut events);
        events
    }

    /// Retail `820E4DB8`: step up onto a ledge top the skater has just
    /// clipped. `from` is where the skater was (object `+128`). A feeler
    /// straight down at the skater's position, from `Physics_Air_Snap_Up`
    /// above the higher of the two heights to the lower one, must find a
    /// surface with normal.y above 0.5 (82000BE8); then the line at
    /// snap-up height from `from` to that point (lifted 0.025 off it,
    /// 820027D4) must be clear. The skater is put there, 0.00254 higher
    /// (82002A64). Returns whether it stepped up.
    pub fn air_snap_up(&mut self, s: &Scripts, from: Vec3, world: &dyn World) -> bool {
        if self.in_bail {
            return false;
        }
        let snap = s.physics_float("Physics_Air_Snap_Up", self.on_bike);
        let pos = self.body.position;
        let top = Vec3::new(pos.x, pos.y.max(from.y) + snap, pos.z);
        let bottom = Vec3::new(pos.x, pos.y.min(from.y), pos.z);
        let Some(hit) = world.feeler(top, bottom, 0x10, 0) else {
            return false;
        };
        if hit.normal.y <= 0.5 {
            return false;
        }
        let q = hit.point + hit.normal * 0.025;
        let lift = Vec3::Y * snap;
        if world.feeler(from + lift, q + lift, 0x10, 0).is_some() {
            return false;
        }
        self.body.position = q + Vec3::Y * 0.00254;
        true
    }

    /// Retail `820EF410`: in the air, a feeler ahead of the move at
    /// `Skater_First_Forward_Collision_Height`, reaching
    /// `..._Length` past it. Returns true when it dealt with the frame
    /// (stepped up onto a ledge), which skips the landing.
    ///
    /// Not translated: wallrides and wallplants
    /// (`820EDAA8`, `820E8618`, which may take over first), trigger 512
    /// scripts, the bonk sound, SkaterState `+200` bookkeeping, the wall
    /// normal kept for `GetWallNormal` (`+2592`) and moving objects.
    pub fn air_forward_collision(&mut self, s: &Scripts, from: Vec3, world: &dyn World) -> bool {
        let pos = self.body.position;
        let dir = (pos - from).normalize_or_zero();
        let h = s.physics_float("Skater_First_Forward_Collision_Height", self.on_bike);
        let l = s.physics_float("Skater_First_Forward_Collision_Length", self.on_bike);
        let up = self.body.up();
        let start = from + up * h;
        let end = pos + up * h + dir * l;
        let Some(hit) = world.feeler(start, end, 0x10, 0) else {
            return false;
        };
        let n = hit.normal;
        // Skatable and ground-like (0.8 at 82002964, 0.5 at 82000BE8): the
        // landing deals with it.
        if surface_skatable(s, &hit) && (n.dot(self.ground_normal) >= 0.8 || n.y >= 0.5) {
            return false;
        }
        // 0.1 is the constant at 82000BF4.
        let steep = n.y < 0.1;
        self.body.position = pos + dir * l;
        // 0.254 is the constant at 82002AC0.
        if self.air_snap_up(s, from, world) && (self.body.velocity.y > 0.254 || steep) {
            return true;
        }
        self.body.position = pos;

        // Rising, upright, into a wall (0.5, 0.01 at 82000D7C): look for
        // the lowest clear height over it, `Physics_Air_Snap_Up` down in
        // steps of 0.05 (82054BE8) to 0.1, and lift the skater by it.
        if self.body.up().y > 0.5 && self.body.velocity.y > 0.0 && n.y < 0.01 {
            let mut lift = s.physics_float("Physics_Air_Snap_Up", self.on_bike);
            let clear = |d: f32| world.feeler(start + Vec3::Y * d, end + Vec3::Y * d, 0x10, 0).is_none();
            if clear(lift) {
                while lift > 0.1 {
                    if !clear(lift - 0.05) {
                        break;
                    }
                    lift -= 0.05;
                }
                self.body.position.y += lift;
                return true;
            }
        }

        // 820EF8A4: in vert air, sit against the wall at
        // `Skater_Min_Distance_To_Wall` and drop the velocity into it.
        if self.vert.in_vert_air {
            let min = s.physics_float("Skater_Min_Distance_To_Wall", self.on_bike);
            self.body.position = hit.point - up * h + n * min;
            let v = self.body.velocity;
            // 820EF918: in a transfer the length is kept (821EDB50).
            self.body.velocity = if self.transfer.active { project_keep_length(v, n) } else { v - n * v.dot(n) };
            return false;
        }

        // 820EF948: no acid drop after this (+200).
        self.set_no_acid_drop(true);
        // Slide along the wall: drop the velocity into the wall, keeping
        // the vertical part unless the wall faces down (-0.1 at 820029A0),
        // then push off it by a tenth of the speed (0.1 at 82000BF4).
        let v = &mut self.body.velocity;
        let mut vertical = 0.0;
        if n.y > -0.1 {
            vertical = v.y;
            v.y = 0.0;
        }
        *v -= n * v.dot(n);
        let speed = v.length();
        *v += n * speed * 0.1;
        if n.y > -0.1 {
            v.y = vertical;
        }
        // Turn a little away from the wall (0.05 at 82054BE8), keeping at.
        let m = &mut self.body.matrix;
        m.z_axis = (m.z_axis + n * 0.05).normalize_or_zero();
        m.x_axis = m.y_axis.cross(m.z_axis).normalize_or_zero();
        m.y_axis = m.z_axis.cross(m.x_axis).normalize_or_zero();
        false
    }

    /// `820F3EF8`: the air feeler hit a surface that can't be skated.
    fn air_hit_wall(&mut self, s: &Scripts, n: Vec3, events: &mut Vec<Event>) {
        if self.in_bail {
            events.push(Event::BailCollision);
            return;
        }
        self.body.velocity = project_keep_length(self.body.velocity, n);
        self.body.matrix.z_axis = project_keep_length(self.body.matrix.z_axis, n);
        orthonormalize_keep_at(&mut self.body.matrix);
        self.matrix_32 = self.body.matrix;
        self.body.position += n * s.physics_float("Skater_Min_Distance_To_Wall", self.on_bike);
        // 820F3FC0: no acid drop after this (+200). Retail then runs the lip
        // and wall-ride checks (`820EA788`, `820E80D8`, ...), not translated.
        self.set_no_acid_drop(true);
    }

    /// The landing path of `820F2310`.
    fn land(&mut self, s: &Scripts, n: Vec3, events: &mut Vec<Event>) {
        self.set_state(State::Ground);
        self.last_speed = self.body.velocity.length();
        // `820DBAA8(1)` runs here, before the landing velocity blend.
        self.flip_if_backwards(s);
        let v = self.body.velocity;
        // 820F38A4: landing from vert air or a transfer sets +2131
        // (`LandedFromVert`) and +2135; otherwise +2135 is cleared.
        if self.vert.in_vert_air || self.transfer.active {
            self.vert.landed_from_vert = true;
            self.vert.landing_from_vert = true;
        } else {
            self.vert.landing_from_vert = false;
        }
        // 820F38CC: onto a bank (+2616) sets `LandedOnBank`; otherwise
        // `LandedFromSpine` takes +136. Then +2616 = 0, +2133 = +1618.
        let tr = &mut self.transfer;
        if tr.bank {
            tr.landed_on_bank = true;
            tr.landed_from_spine = false;
        } else {
            tr.landed_on_bank = false;
            tr.landed_from_spine = tr.active;
        }
        tr.bank = false;
        tr.landed_from_tiretap = tr.auto_drop;
        let still = v.x == 0.0 && v.z == 0.0 && !self.vert.landing_from_vert && !self.vert.landed_from_vert;
        let input = self.last_input;
        if still && self.stick_pulled_back(s, &input) {
            self.body.velocity.y = 0.0;
            self.body.velocity -= n * self.body.velocity.dot(n);
        } else if self.transfer.active {
            // 820F3C44: aim along the target's at (+2448), keeping the
            // speed, onto the landing plane; if pointing the board along it
            // would climb, the old velocity plainly projected instead. At
            // least `Physics_Acid_Drop_Min_Land_Speed`.
            let aimed = project_keep_length(self.transfer.target_at * v.length(), n);
            self.body.velocity = aimed;
            self.velocity_along_board();
            if aimed.y > 0.0 {
                self.body.velocity = v - n * v.dot(n);
            }
            let min = s.global_float("Physics_Acid_Drop_Min_Land_Speed");
            if self.body.velocity.length_squared() < min * min {
                self.body.velocity = self.body.velocity.normalize_or_zero() * min;
            }
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
        // 820F3DD8: out of vert air (and +1380, 820F3E00); +96, +112 and
        // +128 take the normal.
        self.vert.in_vert_air = false;
        self.transfer.retry = false;
        self.vert.eased_normal = n;
        self.vert.ease_from = n;
        self.ground_normal = n;
        self.previous_normal = n;
        self.body.matrix.y_axis = n;
        orthonormalize_keep_up(&mut self.body.matrix);
        self.matrix_32 = self.body.matrix;
        events.push(Event::Landed);
    }

    /// Air time in milliseconds (retail `820D4CA8` while in the air).
    pub fn air_time_ms(&self) -> i64 {
        self.time_ms - self.air_start_ms
    }

    /// One physics frame, as retail's component update `820FC990` runs it:
    /// the crouch update, the speed limits, then the update for the current
    /// state.
    ///
    /// Without the scripts (`scripted` false), their event handlers are
    /// stood in for here, only as far as they are read: on the ground
    /// "Ollied" runs `ollie`, which calls `Jump`; in the air it does so only
    /// while [`CorePhysics::late_ollie`] is set (see there). With the
    /// scripts, `skater.rs` delivers the events to them instead.
    pub fn step(&mut self, s: &Scripts, input: &InputState, world: &dyn World) -> Vec<Event> {
        self.time_frac_ms += self.dt * 1000.0;
        let whole = self.time_frac_ms.floor();
        self.time_ms += whole as i64;
        self.time_frac_ms -= whole;
        self.last_input = *input;
        self.old_position = self.body.position;
        // `WaitAnimWhilstCheckingLateOllie` (one check per game frame; its
        // order against the physics in a frame is LIKELY, not read).
        if self.late_ollie
            && self.state == State::Air
            && self.air_time_ms() as f32 > s.global_float("skater_late_jump_slop")
        {
            self.late_ollie = false;
        }
        self.update_crouch(input);
        self.speed_limits(s);
        // 820FCAB4: +1237 = SkaterState +136 before the state update.
        self.transfer.was_active = self.transfer.active;
        let was_air = self.state == State::Air;
        let mut events = match self.state {
            State::Ground => self.ground_update(s, input, world),
            State::Air => self.air_update(s, world),
            State::Lip => self.lip_update(s, input, world),
        };
        // 820F4050: the air update ends with the ollie trigger `820D7AB0`
        // on every path that does not land.
        if was_air && !events.contains(&Event::Landed) {
            if self.ollie_trigger(input) {
                events.push(Event::Ollied);
            }
            // 820F4060: then the acid drop check.
            self.air_acid_drop(s, world);
        }
        // 820FCD8C: rails, after the state update.
        self.rail_check(s, input, world);
        // 820FCDA8: the speed allowance after a transfer.
        self.post_transfer_speed(s);
        if self.scripted {
            return events;
        }
        if events.contains(&Event::GroundGone) {
            // Script `groundgone`: `SetException ex = ollied scr = ollie`.
            self.late_ollie = true;
        }
        if events.contains(&Event::Landed) {
            self.late_ollie = false;
        }
        if events.contains(&Event::Ollied) && (!was_air || self.late_ollie) {
            events.extend(self.jump(s, None));
            // `ollie` runs `InAirExceptions`, which drops the handler.
            self.late_ollie = false;
        }
        events
    }
}

/// Retail `821EDB50` (shared with the ground update).
fn project_keep_length(v: Vec3, n: Vec3) -> Vec3 {
    crate::core_physics::project_keep_length(v, n)
}
