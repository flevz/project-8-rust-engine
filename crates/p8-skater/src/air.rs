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
        // Leaving the air clears SkaterState +240 (820D73C0..820D73E0).
        if self.state == State::Air && state != State::Air {
            self.grind.ollied_off_rail = false;
        }
        // +168 when the state changes (820D7220..820D7240).
        if self.state != state {
            self.state_ms = self.time_ms;
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
    /// Not translated: `SmoothSpin` (`+2752`) and the Nail the Trick checks;
    /// the model's lean display, the score's trick name and the
    /// `force_flip_bail` head check (noted at the end).
    pub fn air_rotation(&mut self, s: &Scripts, input: &InputState) {
        // 820E9644: nothing in a bail (SkaterState +128).
        if self.in_bail {
            return;
        }
        let pf = |name: &str| s.physics_float(name, false);
        let stat = |me: &Self, name: &str| s.stat(name, me.on_bike, &me.stats, me.stat_context);
        // 820E967C..820E969C: lean input (stick Y, else Up -1 / Down +1) and
        // spin input (stick X).
        let mut lean_in = input.stick_y;
        let mut spin_in = input.stick_x;
        if lean_in == 0.0 {
            if input.up {
                lean_in = -1.0;
            } else if input.down {
                lean_in = 1.0;
            }
        }
        // 820E96AC: +2721 clear -> no stick / D-pad input.
        if !self.analog_turning {
            spin_in = 0.0;
            lean_in = 0.0;
        }
        // 820E96C8..820E96F0: L1 / R1 spin (record R1's "held" check uses +224; "+128" is L1).
        let mut buttons = false;
        if input.l1 && !input.r1 {
            spin_in = -1.0;
            buttons = true;
        }
        if input.r1 && !input.l1 {
            spin_in = 1.0;
            buttons = true;
        }
        // 820E9700..820E9728: D-pad Right +1 / Left -1.
        if self.analog_turning && spin_in == 0.0 {
            if input.right {
                spin_in = 1.0;
            } else if input.left {
                spin_in = -1.0;
            }
        }
        // 820E9738..820E9748: with L2 and both inputs, the pair is
        // normalised.
        if input.l2 && lean_in != 0.0 && spin_in != 0.0 {
            let v = glam::Vec2::new(spin_in, lean_in).normalize();
            spin_in = v.x;
            lean_in = v.y;
        }
        // 820E97F8..820E9808: skaterstancepanel (82116500): nollie.
        if self.nollie {
            lean_in = -lean_in;
        }

        // 820E98CC..820E9918: a transfer, a bike or `NoSpin` cancels the vert auto-turn.
        if self.transfer.active || self.on_bike || self.no_spin {
            self.vert.auto_turn = false;
        }

        // 820E9854..820E98C4: lean only with L2 (Physics_Air_Lean_fast_stat), after Up/Down
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

        // Spin: 820E9938..820E9944 needs +2720 and not +2128.
        // 820E9924..820E992C: retail also needs SkaterState +224 clear (set
        // only by the walk-to-skate reset in the air) else the buttons-only
        // branch: not translated (audit, NOTES 44; unreachable until walking
        // is translated).
        let mut spin = 0.0;
        if self.turning_enabled && !self.no_spin {
            // 820E9960..820E9998: the rate (fast with L2), the held time.
            let rate = if input.l2 {
                stat(self, "Physics_Air_Rotation_fast_stat")
            } else {
                stat(self, "Physics_Air_Rotation_stat")
            };
            let mut held = 0.0;
            if spin_in != 0.0 {
                spin = -(rate * spin_in);
                held = if input.left { input.left_held_ms } else { input.right_held_ms } as f32;
                // 820E99CC..820E99E4: a spin held longer than `skater_autoturn_cancel_time`
                // cancels the vert auto-turn.
                if held > pf("skater_autoturn_cancel_time") {
                    self.vert.auto_turn = false;
                }
            }
            // 820E99FC..820E9A44.
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
            // 820E9A54..820E9A78.
            spin = -(stat(self, "Physics_Air_Rotation_stat") * spin_in);
        }
        // 820E9A84..820E9AB4 (IsSkaterInNailTheTrick): not translated.
        // 820E9AB8..820E9BBC: the vert auto-turn, only on frames with no spin
        // input (the spin rate is 0; any spin skips it for that frame,
        // without cancelling it).
        // While the skater's at row is more than `skater_autoturn_vert_angle`
        // from straight up it turns toward the stored facing at
        // `skater_autoturn_speed`, and stops (clearing `+120`) once the rest
        // of the turn fits in one frame; within that angle of up it stops at
        // once.
        if spin == 0.0 && self.vert.in_vert_air && self.vert.auto_turn && !self.no_spin {
            let at = self.body.at();
            let from_up = at.y.clamp(-1.0, 1.0).acos();
            // 0.017453292 is the degrees to radians constant at 82000C10.
            if from_up >= pf("skater_autoturn_vert_angle") * 0.017453292 {
                let target = crate::core_physics::signed_angle(at, self.vert.auto_turn_dir, self.body.up());
                let sign = if target < 0.0 { -1.0 } else { 1.0 };
                spin = sign * pf("skater_autoturn_speed");
                if (self.dt * spin).abs() > target.abs() {
                    spin = target / self.dt;
                    self.vert.auto_turn = false;
                }
            } else {
                self.vert.auto_turn = false;
            }
        }
        // 820E9D60..820E9DC8 (SmoothSpin, +2752..+2764, which also rescales
        // the rate): not translated.
        // 820E9DCC..820E9E34: the rotation, +2217 and trick +5360 (retail
        // adds to +5360 when the spin or the lean is non-zero; with no spin
        // it adds 0).
        if spin != 0.0 {
            let angle = self.dt * spin;
            // No +1940 here: 820E9620 does not write it and the air update
            // clears it every frame (820F24A4).
            // 820E9DD4..820E9DF8: +2217, the spin (or vert auto-turn) sign.
            self.last_spin_positive = angle > 0.0;
            self.rotate(angle);
            self.spin_degrees += angle * 57.29578;
        }

        // 820E9BBC..820E9CB4: lean angle (display), or ease it back to a
        // whole turn.
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
        // 820E9CB8..820E9D04: when the lean changed, the Model component's
        // display rotation (82260758, 82260550 with +2628 / +1912): not
        // translated (audit, NOTES 44; the lean is not drawn).
        // 820E9D08..820E9D5C: +1542 flipping.
        if changed {
            let a = (self.lean_degrees as i32 % 360).abs();
            self.flipping = (41..=319).contains(&a);
        }
        // 820E9E38..820E9ED4: Nail the Trick (ntt_add_flipper_rotation):
        // not translated.
        // 820E9ED8..820E9F90: the score's trick name (8217B3D0 with the
        // spin +5360 and the lean; in vert air only past a whole turn less
        // `spin_count_slop`): not translated (audit, NOTES 44).
        // 820E9F94..820EA0BC: `force_flip_bail`: with the model's up row
        // pointing down (82260C58) and not in vert air, a feeler of
        // `Skater_head_height_for_flips` from the position; on a hit the
        // script bails: not translated (audit, NOTES 44).
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

    /// Retail `Jump` (`820F0730`). `speed` is the script's `jump speed =
    /// ...` override; `boneless` / `no_comply` its `BonelessHeight` /
    /// `NoComply` flags (the `boneless` and `nocomply` scripts).
    ///
    /// Not translated: the rail state 4 paths (820F079C..820F0848 and
    /// 820F0E04..820F0E58, `Rail_Jump_Angle`; rails are not a `State`), the
    /// jump sounds (820F0764 `no_sound`, 82115E78 at 820F0848, 820F08F8,
    /// 820F09DC; no audio), core +1617 = 0 (820F0A0C, bike only) and the
    /// Natas spin script (820F0E64..820F0E70, 820F0550).
    pub fn jump(&mut self, s: &Scripts, speed: Option<f32>, boneless: bool, no_comply: bool) -> Vec<Event> {
        // 820F0790..820F0794: only the ground path (820F0900) records where
        // the jump started; the air path (820F0850..820F08FC, a late ollie)
        // only plays a sound.
        // 820F0900..820F091C: the ground path first runs the surface node's
        // TriggerScript, type 20 (`8228C880`), then returns at once when
        // SkaterPhysicsControl +38 is set: not translated (audit, NOTES 44;
        // no node TriggerScripts yet, +38 taken as clear).
        if self.state == State::Ground {
            // 820F0924..820F093C: +2000.
            self.jump_start = self.body.position;
            // 820F09E4..820F0A08: the vert takeoff, then SkaterState +80 is set.
            self.vert_takeoff(s);
            self.set_break_window(true);
        }
        // 820F0A1C..820F0A48.
        let max_tense = s.global_float("Skater_max_tense_time") as i64;
        self.crouch_duration_ms = self.crouch_duration_ms.min(max_tense);
        // 820F0A4C..820F0AA0: vert jump stats in vert air, or on the ground on a vert
        // surface. 0.3 is the double at 82002AD0: board pointing up =
        // launch ramp. 820F0AC4: with `BonelessHeight`, the boneless stats.
        let vert = self.vert.in_vert_air || (self.state == State::Ground && self.vert.on_vert_ground);
        let launch = self.body.at().y > 0.3;
        // 820F0AC4..820F0B8C.
        let (max_name, min_name) = if boneless {
            if vert {
                ("Physics_Boneless_Vert_Jump_Speed_Stat", "Physics_Boneless_Vert_Jump_Speed_min_Stat")
            } else if launch {
                ("Physics_Boneless_Launch_Jump_Speed_Stat", "Physics_Boneless_Launch_Jump_Speed_min_Stat")
            } else {
                ("Physics_Boneless_Jump_Speed_Stat", "Physics_Boneless_Jump_Speed_min_Stat")
            }
        } else if vert {
            ("Physics_Vert_Jump_Speed_Stat", "Physics_Vert_Jump_Speed_min_Stat")
        } else if launch {
            ("Physics_Launch_Jump_Speed_Stat", "Physics_Launch_Jump_Speed_min_Stat")
        } else {
            ("Physics_Jump_Speed_Stat", "Physics_Jump_Speed_min_Stat")
        };
        let max = s.stat(max_name, self.on_bike, &self.stats, self.stat_context);
        let mut jump = s.stat(min_name, self.on_bike, &self.stats, self.stat_context);
        // 820F0BB8.
        if max_tense != 0 {
            jump += self.crouch_duration_ms as f32 / max_tense as f32 * (max - jump);
        }
        // 820F0C04 (822128C8 "Speed").
        if let Some(speed) = speed {
            jump = speed;
        }
        // 820F0C14: SkaterState +32.
        self.uncrouch();
        // 820F0C2C..820F0C7C: +2544 (`LastWasJumpBoneless`) = BonelessHeight or NoComply.
        self.last_jump_boneless = boneless || no_comply;
        // 820F0C80..820F0D04: moving down in vert air, jump out along the eased
        // normal `+96` and leave vert air; the upward part is then 0.
        if self.vert.in_vert_air && self.body.velocity.y < 0.0 {
            self.body.velocity += self.vert.eased_normal * jump;
            self.vert.in_vert_air = false;
            // 820F0D00: and +1380.
            self.transfer.retry = false;
            jump = 0.0;
        } else if self.body.velocity.y < 0.0 {
            // 820F0D14.
            self.body.velocity.y = 0.0;
        }
        self.body.velocity.y += jump;
        // 820F0D84..820F0DF4: upside down (-0.1 at 820029A0): jump the
        // other way and step off.
        if self.body.up().y < -0.1 {
            self.body.velocity.y += jump * -1.5;
            self.body.position += self.body.up() * 0.3;
        }
        self.set_state(State::Air);
        // 820F0EA8..820F0EAC: the jump time +2540, and the SkaterJump
        // broadcast.
        self.jump_ms = self.time_ms;
        vec![Event::SkaterJump]
    }

    /// Retail air update `820F2310`, for a plain ollie: gravity, the move,
    /// and landing on a skatable surface.
    ///
    /// Not translated (absent): `820EEB38`
    /// (bikes only), bails on landing, moving platforms, the nose/tail
    /// landing feelers (`820E5250`, which only record contact), the side
    /// collision `820F2238` and the head check `820EA788` (see the end of
    /// the update), and the landing sound and triggers.
    pub fn air_update(&mut self, s: &Scripts, world: &dyn World) -> Vec<Event> {
        // 820F2340..820F235C (and 820F3A38..820F3A78, 820F3EA4..820F3EE4):
        // the +1920 velocity record (read only by walking code): not kept.
        // 820F235C..820F23A4: inline 820E53D8(0): terrain +265 = 0, the
        // "OnSkaterTerrainChange" particles (82116930, 820DE6B8, 820BDFB0)
        // and sounds: not translated (no physics effect).
        // 820F23A4: 820DE490, the Nail the Trick zero-velocity hack: not
        // translated (no Nail the Trick).
        let mut events = Vec::new();
        // Object +128: the position at the start of this frame (LIKELY: the
        // object update stores it before the physics runs).
        let old = self.old_position;
        // 820F23B0..820F23DC: the gravity vector (820D76C0).
        let g = Vec3::new(0.0, self.air_gravity(s), 0.0);
        // 820F240C..820F2420: SkaterState +72 (on vert ground) is cleared.
        self.vert.on_vert_ground = false;
        // 820F2424..820F2478: no acid drop (+200) while in vert air, a transfer, +192
        // or a bail.
        if self.vert.in_vert_air || self.transfer.active || self.transfer.flag_192 || self.in_bail {
            self.set_no_acid_drop(true);
        }
        self.standing_kick_limit = 0.0;
        self.turn_amount = 0.0;
        // 820F2488: +1548 (on a single-node rail) cleared.
        self.grind.natas = false;
        self.flag_2637 = false;
        self.bert_slide = false;
        self.kick_flag = false;
        self.last_turn = None;
        let input = self.last_input;
        self.air_rotation(s, &input);
        self.transfer_blend();
        // 820F24B4..820F24EC: the leveling is skipped in vert air, unless SkaterState
        // +144 is set or the spine button is held.
        if self.vert.over_ground || self.spine_button(&input) || !self.vert.in_vert_air {
            self.air_recover(s, world);
        }
        // 820F24F0..820F250C: bike state +152 (820DC758) / 820EEB38: not
        // translated (bikes). `820D79F8` runs here (not translated).
        // 820F2518..820F2560: outside vert air and spine transfers (and the
        // rotate component and bike state +152, not translated), the
        // sideways uprighting.
        if !self.vert.in_vert_air && !self.transfer.active {
            self.upright_sideways(s);
        }
        // 820F2564..820F2654: +2620 kept at its largest during an acid drop
        // (see `TransferState::speed`): not translated.
        // 820F2658..820F2668: 820D9F70 while +1976 != 0 (always 0 here).
        // 820F266C..820F26D8: the +2552 wallride retry countdown
        // (820EDAA8), only written by untranslated states: not translated.

        let dt = self.dt;
        // 820F26DC..820F278C: in a transfer the carry (SkaterState +272) is added to
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
        // 820F27EC..820F2830: the movable contact (820EB448, +2546): not
        // translated. 820F2830..820F28FC: the move.
        self.body.position += mv * dt + g * (dt * dt * 0.5);
        self.body.velocity += g * dt;
        // 820F2497: `+1936` = the velocity's y after the gravity.
        self.last_in_air_vy = self.body.velocity.y;
        // 820F2900..820F3014 (vert.rs).
        self.vert_air_update(s, world);
        // 820F3018..820F30A0.
        self.follow_display_matrix(s);
        // 820F30A0..820F311C: the bike nose / tail points: not translated.

        // `820EF410` (820F3144): walls ahead. When it handled the frame,
        // retail skips the landing (820F369C). 820F3148..820F3160: retail
        // returns when it changed the state to wallride / wallplant (not
        // translated).
        let handled = self.air_forward_collision(s, old, world);
        // The pitch-bail check (820F3164..820F3580, `Pitch_Bail_Feeler_Length`
        // and script `Pitch_Bail_Check`) leads into bails: not translated.
        // Without a bail it also sets +96 / +112 / +128 to the hit normal
        // and runs 820E5250 (820F3368, 820F3520): not translated either.
        // 820F3584..820F3628: in a transfer (+136) retail extends the
        // landing feeler 0.025 (820027D4) back and forward along the move:
        // not translated (audit, NOTES 44).

        // 820F3654..820F36B0: landing: feeler from last position to this
        // one, ignoring surfaces with flag 0x10 (`820E5048(16, 0)` at
        // 820F365C). On no hit or `handled`, retail goes to
        // 820F4000..820F4014: the side collision `820F2238` (not in vert
        // air; `820ED630` to each side, `Skater_side_collide_length`,
        // pushing out of walls; the position is restored when both sides
        // hit), then to the head check below: not translated (audit, NOTES
        // 44).
        let Some(hit) = world.feeler(old, self.body.position, 0x10, 0) else {
            return events;
        };
        if handled {
            return events;
        }
        // 820F36B4..820F3744: a ledge to step onto instead (`820E4DB8`) wins when
        // rising faster than 0.25 (82000BEC) or the hit is steep (normal.y
        // below 0.1), and 500 ms have passed since SkaterPhysicsControl
        // `+116` (UNKNOWN; nothing translated sets it, so always).
        let steep = hit.normal.y < 0.1;
        if self.air_snap_up(s, old, world) && (self.body.velocity.y > 0.25 || steep) {
            return events;
        }
        // 820F3748..820F3810 (820F3798: the "Ragdoll" component, no physics
        // effect, not translated).
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

    /// `820F3EF8`: the air feeler hit a surface that can't be skated
    /// (820F3EF8..820F3FE4).
    fn air_hit_wall(&mut self, s: &Scripts, n: Vec3, events: &mut Vec<Event>) {
        // 820F3F04, 820F3FE8: in a bail, "BailCollision".
        if self.in_bail {
            events.push(Event::BailCollision);
            return;
        }
        self.body.velocity = project_keep_length(self.body.velocity, n);
        self.body.matrix.z_axis = project_keep_length(self.body.matrix.z_axis, n);
        orthonormalize_keep_at(&mut self.body.matrix);
        self.matrix_32 = self.body.matrix;
        self.body.position += n * s.physics_float("Skater_Min_Distance_To_Wall", self.on_bike);
        // 820F3FC0..820F3FE4: no acid drop after this (+200). Retail then
        // runs the head / ceiling check `820EA788` (820F4018..820F404C; not
        // translated, see below) and, after the ollie trigger, the wallplant
        // check `820E80D8` (not translated).
        self.set_no_acid_drop(true);
    }

    /// The landing path of `820F2310`.
    ///
    /// Not translated: the land sound (820F3914..820F39A8, 82115B58 with
    /// |v| / `Skater_Max_Max_Speed_Stat`), the trick balance scoring
    /// (820F39AC..820F39C0, 821248A8), and the landing node TriggerScripts
    /// (820F39C4..820F3A10: 820BBE10, then `8228C880` type 264, and
    /// 0x40008 in an acid drop) (audit, NOTES 44). 820F3A14..820F3A88: when
    /// SkaterPhysicsControl +38 is set, retail sends "Landed" and returns
    /// before the velocity code; the flag is taken as clear (UNKNOWN,
    /// INFERRED walking; audit, NOTES 44).
    fn land(&mut self, s: &Scripts, n: Vec3, events: &mut Vec<Event>) {
        // 820F3814: SetState(0). 820F3824: the heading history (820DE3E8,
        // animation bookkeeping): not translated.
        self.set_state(State::Ground);
        // 820F382C..820F38A0: +2020.
        self.last_speed = self.body.velocity.length();
        // `820DBAA8(1)` runs here (820F3908), before the landing velocity
        // blend.
        self.flip_if_backwards(s, true);
        let v = self.body.velocity;
        // 820F38A4..820F38C4: landing from vert air or a transfer sets +2131
        // (`LandedFromVert`) and +2135; otherwise +2135 is cleared.
        if self.vert.in_vert_air || self.transfer.active {
            self.vert.landed_from_vert = true;
            self.vert.landing_from_vert = true;
        } else {
            self.vert.landing_from_vert = false;
        }
        // 820F38CC..820F3900: onto a bank (+2616) sets `LandedOnBank`; otherwise
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
        // 820F3A8C..820F3B10: still and pulled back (820D74B0).
        let still = v.x == 0.0 && v.z == 0.0 && !self.vert.landing_from_vert && !self.vert.landed_from_vert;
        let input = self.last_input;
        if still && self.stick_pulled_back(s, &input) {
            self.body.velocity.y = 0.0;
            self.body.velocity -= n * self.body.velocity.dot(n);
        } else if self.transfer.active {
            // 820F3B24, 820F3C44..820F3CBC (821EDD60): aim along the target's at (+2448), keeping the
            // speed, onto the landing plane; if pointing the board along it
            // would climb, the old velocity plainly projected instead. At
            // least `Physics_Acid_Drop_Min_Land_Speed`.
            let aimed = project_keep_length(self.transfer.target_at * v.length(), n);
            self.body.velocity = aimed;
            // 820F3C74..820F3C98: 820DB318 turns a stack copy along the
            // board (it reads and writes only through its pointer); the copy
            // is only tested, the velocity stays aimed.
            let speed = aimed.length();
            let copy = if speed <= 1e-6 {
                aimed
            } else {
                let at = self.body.at();
                at * speed * if (aimed / speed).dot(at) < 0.0 { -1.0 } else { 1.0 }
            };
            if copy.y > 0.0 {
                self.body.velocity = v - n * v.dot(n); // 820F3CBC (821ED798)
            }
            // 820F3CC0..820F3D64.
            let min = s.global_float("Physics_Acid_Drop_Min_Land_Speed");
            if self.body.velocity.length_squared() < min * min {
                self.body.velocity = self.body.velocity.normalize_or_zero() * min;
            }
        } else {
            // 820F3B14..820F3C40 (820F3BEC: `landing_velocity_factor`).
            let dir = v.normalize_or_zero();
            let along = project_keep_length(v, n);
            let flat = v - n * v.dot(n);
            let d = dir.dot(n).abs();
            let k = d * s.global_float("landing_velocity_factor");
            // The steeper the landing, the more of the speed is kept along
            // the ground (820F3BF0..3C3C).
            self.body.velocity = along * k + flat * (1.0 - k);
        }
        // 820F3D68..820F3DD4: 0.064516 = 0.254² (constant at 82002ADC).
        if self.body.velocity.length_squared() < 0.064516 {
            self.body.velocity = Vec3::ZERO;
        }
        // 820F3DD8..820F3E24: out of vert air (and +1380, 820F3E00); +96,
        // +112 and +128 take the normal. 820F3E2C..820F3EA0: up = n, +32,
        // "Landed".
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
    ///
    /// Not translated from `820FC990`:
    /// - 820FCA04..820FCA10: +164 = the frame dt, and dt == 0 returns
    ///   before anything; there is no such guard here (audit, NOTES 44).
    /// - 820FCA20..820FCA2C: bail state 9.
    /// - 820FCA38..820FCA84: not in a bail, 822B5A78 false and the
    ///   four-button chord 820D7B60 held: "ForcedBail" is sent and the whole
    ///   frame is skipped (audit, NOTES 44).
    /// - 820FCAB8..820FCAD8: the bike rows.
    /// - 820FCAFC..820FCB34: SkaterState +176 and +184 cleared (both
    ///   untranslated).
    /// - 820FCB4C..820FCC80: `Wall_Ride_Show_Axis` debug lines.
    /// - The other states' updates (820FCD0C..820FCD1C, 820FCD2C..820FCD6C;
    ///   state 3, 820FCD20, is the lip), state 8's snap (820FCD88..820FCD98)
    ///   and state 9 skipping the rest (820FCDA0).
    /// - 820FCD7C: object +8 bit 0 set after the state update -> return
    ///   (audit, NOTES 44).
    /// - 820FCDD8..820FCE4C: the "SkaterEnterVertAir" / "SkaterExitVertAir"
    ///   broadcasts when vert air (+2545) changes (audit, NOTES 44).
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
        // 820FCC90..820FCD04: the switch on the state (820FCCFC: +1236 after
        // an air update that landed, not kept).
        let mut events = match self.state {
            State::Ground => self.ground_update(s, input, world),
            State::Air => self.air_update(s, world),
            State::Lip => self.lip_update(s, input, world),
            State::Rail => self.rail_update(s, input, world),
            State::Stall => self.stall_update(s, input, world),
        };
        // 820F4018..820F404C: every air frame that does not land first runs
        // the head / ceiling check `820EA788` (a feeler up
        // `Skater_default_head_height`, 0.15 just after a wallplant; on a
        // downward-facing hit it slides the skater off it, drops the
        // velocity into it and sets +200): not translated (audit, NOTES 44).
        // 820F4050: the air update ends with the ollie trigger `820D7AB0`
        // on every path that does not land.
        if was_air && !events.contains(&Event::Landed) {
            if self.ollie_trigger(input) {
                events.push(Event::Ollied);
            }
            // 820F4058 / 820F405C: the wallplant check `820E80D8` (not translated).
            // 820F4060..820F40F4: then the acid drop check.
            self.air_acid_drop(s, world);
        }
        // 820FCDB4: rails and lips (`820FAAA8(0)`, lip.rs), after the state
        // update. Retail runs it after the speed allowance (820FCDA8); the
        // order here is swapped (only matters when a transfer ends on the
        // frame of a lip grab).
        if let Some(e) = self.rail_check(s, input, world) {
            events.push(e);
        }
        // 820FCDA8: the speed allowance after a transfer.
        self.post_transfer_speed(s);
        // 820FCDD4 (after the calls 820FCDBC, 820FCDC4, 820FCDCC to 820D7D78,
        // 820D7E10, 820DE110, not translated):
        // the balance meters' cheese wears off.
        {
            let (stats, ctx, on_bike, now) = (self.stats.clone(), self.stat_context, self.on_bike, self.time_ms);
            let mut no_random = |_: u32| 0;
            let c = crate::balance::BalanceCtx { s, stats: &stats, stat_context: ctx, on_bike, now_ms: now, random: &mut no_random };
            self.balance.wear_off_cheese(&c, self.dt);
        }
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
            events.extend(self.jump(s, None, false, false));
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
