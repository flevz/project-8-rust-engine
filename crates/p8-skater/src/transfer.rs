//! Spine transfers, acid drops and bank drops.
//!
//! - `820E68A8`: in vert air with the spine button, look over the coping
//!   for another ramp (`820E0AC8`) and fly to it.
//! - `820E1600` / `820DA7B0`: rolling off an edge (or just after) with the
//!   spine button, look ahead for a vert or bank surface below and drop in.
//! - `820EA0D0`: while either move runs, turn the skater smoothly from the
//!   take-off orientation to the target's.
//! - `820DAFF0`: after landing, the speed may stay above the normal cap for
//!   a while.
//!
//! Both moves set SkaterState `+136` (`IsInSpineTransfer`), which changes
//! the gravity, the air move, the speed limits and the landing elsewhere.
//!
//! Not translated: walking and bike branches of `820E1600` (neither is
//! translated), the local collision caches the searches build (INFERRED
//! to only speed the feelers up), moving objects (`+2546` is always false
//! because level objects are not loaded), the acid-drop matrix copy at
//! `+2240` (no reader found), and SkaterState `+120`.
use crate::body::rotate_about_up;
use crate::core_physics::{CorePhysics, signed_angle};
use crate::script::Scripts;
use crate::world::{Hit, World};
use glam::{Mat3, Quat, Vec3};
use p8_formats::qb::Value;
use p8_formats::qb_key;
use p8_script::Params;

/// Something the physics asks of the skater's scripts. Retail does these
/// at once, inside the physics; here `skater.rs` does them right after the
/// physics step, in order (APPROXIMATE timing).
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptAction {
    /// Retail `822265F8`: create the script on the skater and update it
    /// once now (`82210860`, `8220F8F0`); it is deleted if it finished.
    Run(u32, Params),
    /// Retail `82225278` -> `8220D180`: `ClearEventHandler` on the
    /// skater's script.
    ClearHandler(u32),
}

/// The fields both moves use: SkaterState flags and physics fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    /// SkaterState `+136` (stamp `+140`): in a spine transfer or acid drop.
    pub active: bool,
    pub active_ms: i64,
    /// `+1237`: `+136` as it was before this frame's state update
    /// (`820FCAB4`).
    pub was_active: bool,
    /// SkaterState `+192` (stamp `+196`): set by the acid drop. Read by the
    /// rail grab (`820F8454`, not translated) and the air update start.
    pub flag_192: bool,
    pub flag_192_ms: i64,
    /// SkaterState `+200` (stamp `+204`): acid drops are not allowed
    /// (INFERRED name; `DisallowAcidDrops` sets it).
    pub no_acid_drop: bool,
    pub no_acid_drop_ms: i64,
    /// SkaterState `+272`: horizontal carry added to the air move.
    pub carry: Vec3,
    /// `+1380`: the transfer search found a target it could not take yet;
    /// the air update tries again next frame while this is set.
    pub retry: bool,
    /// `+1618`: `auto_drop`. Only untranslated code sets it (`820F01E8`
    /// after the ollie trigger, `enterwheeliefromair`), so it stays false.
    pub auto_drop: bool,
    /// `+2130` `LandedFromSpine`, `+2133` `landedfromtiretap`, `+2134`
    /// `LandedOnBank`.
    pub landed_from_spine: bool,
    pub landed_from_tiretap: bool,
    pub landed_on_bank: bool,
    /// `+2172`: the height the carry stops below when falling.
    pub ref_height: f32,
    /// `+2224`: the post-transfer speed factor (1 = none). `820FA850`
    /// sets 1.
    pub speed_factor: f32,
    /// `+2304` blend start, `+2368` target, `+2432` seconds elapsed,
    /// `+2436` duration, `+2464` the last blended matrix.
    pub from: Mat3,
    pub to: Mat3,
    pub elapsed: f32,
    pub duration: f32,
    pub last: Mat3,
    /// `+2448`: the target's "at" direction; the landing aims along it.
    pub target_at: Vec3,
    /// `+2546`: the target is on a moving object.
    pub on_object: bool,
    /// `+2616`: the target is a bank (surface flag 0x100 without 0x8).
    pub bank: bool,
    /// `+2620`: the speed at the start of an acid drop.
    pub speed: f32,
    /// `+2768`: a copy of the acid drop's carry (read by the rail grab,
    /// not translated).
    pub acid_carry: Vec3,
    /// Script work queued by these functions.
    pub actions: Vec<ScriptAction>,
}

impl Default for Transfer {
    fn default() -> Self {
        Self {
            active: false,
            active_ms: 0,
            was_active: false,
            flag_192: false,
            flag_192_ms: 0,
            no_acid_drop: false,
            no_acid_drop_ms: 0,
            carry: Vec3::ZERO,
            retry: false,
            auto_drop: false,
            landed_from_spine: false,
            landed_from_tiretap: false,
            landed_on_bank: false,
            ref_height: 0.0,
            speed_factor: 1.0,
            from: Mat3::IDENTITY,
            to: Mat3::IDENTITY,
            elapsed: 0.0,
            duration: 0.0,
            last: Mat3::IDENTITY,
            target_at: Vec3::ZERO,
            on_object: false,
            bank: false,
            speed: 0.0,
            acid_carry: Vec3::ZERO,
            actions: Vec::new(),
        }
    }
}

/// What `820E0AC8` found.
struct Target {
    point: Vec3,
    normal: Vec3,
    /// b85: not back to back with the ramp the skater is on (a hip).
    hip: bool,
    /// b84: a bank (flag 0x100 without 0x8).
    bank: bool,
    /// b86: on a named object (`8221AFA0`); level objects are not loaded.
    on_object: bool,
}

/// What `820E1600` found (its `out` struct).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcidDrop {
    /// `+0`: the landing point (raised in 0.6 steps if the path was
    /// blocked).
    pub point: Vec3,
    /// `+16`: the surface normal there.
    pub normal: Vec3,
    /// `+32`: the height of the surface as first found.
    pub hit_y: f32,
    /// `+36`: on a moving object.
    pub on_object: bool,
}

/// `x` with y set to 0.
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Retail `820BAEC0`: the matrix `t` of the way from `a` to `b`, through
/// quaternions (`820BA2E0`) and a slerp that takes the short way
/// (`820A8BC0`, the second quaternion negated when the dot is below 0).
/// Its near-parallel branch (1e-6, 820A8C50) is glam's, not read.
fn matrix_slerp(a: &Mat3, b: &Mat3, t: f32) -> Mat3 {
    let qa = Quat::from_mat3(a);
    let qb = Quat::from_mat3(b);
    Mat3::from_quat(qa.slerp(qb, t))
}

/// Set a stamped SkaterState flag: the stamp only changes with the value.
fn stamp(flag: &mut bool, ms: &mut i64, on: bool, now: i64) {
    if *flag != on {
        *flag = on;
        *ms = now;
    }
}

impl CorePhysics {
    /// Set or clear SkaterState `+136`.
    pub(crate) fn set_transfer(&mut self, on: bool) {
        let now = self.time_ms;
        stamp(&mut self.transfer.active, &mut self.transfer.active_ms, on, now);
    }

    pub(crate) fn set_flag_192(&mut self, on: bool) {
        let now = self.time_ms;
        stamp(&mut self.transfer.flag_192, &mut self.transfer.flag_192_ms, on, now);
    }

    pub(crate) fn set_no_acid_drop(&mut self, on: bool) {
        let now = self.time_ms;
        stamp(&mut self.transfer.no_acid_drop, &mut self.transfer.no_acid_drop_ms, on, now);
    }

    /// Retail `820D77F0` as the transfer code calls it, with SkaterState
    /// `+136` set around the call (so `820D76C0` uses the vert hang): the
    /// time to go from `y_start` to `y_target` starting at `vy`. 1e-5
    /// (82000C18) when it is never reached.
    fn flight_time(&self, s: &Scripts, y_target: f32, y_start: f32, vy: f32) -> f32 {
        let big_g = -self.air_gravity_hang(s, true);
        let root = vy * vy + 2.0 * big_g * (y_start - y_target);
        if root < 0.0 {
            return 1e-5;
        }
        (vy + root.sqrt()) / big_g
    }

    /// Retail `820E68A8`: the spine transfer. Called from the break-vert
    /// (`820EC7B0`) with the spine button held or `+1380` set. Returns
    /// true when it handled the break (including "try again next frame").
    pub(crate) fn spine_transfer(&mut self, s: &Scripts, world: &dyn World) -> bool {
        let pos = self.body.position;
        let v = self.body.velocity;
        let input = self.last_input;
        // 820E6914..820E6C2C: `u` = flat up, `side` = u turned flat by 90
        // degrees, `w` = back along the velocity; R = w turned 90 degrees
        // (half-angle 0.785398, 820029D0) about `side`, right-handed
        // (quaternion q w q*, CONFIRMED from the permute constants).
        let u = flat(self.body.up()).normalize_or_zero();
        let side = Vec3::new(-u.z, 0.0, u.x);
        let w = (-v).normalize_or_zero();
        let axis = side.normalize_or_zero();
        #[allow(clippy::approx_constant)] // the image's own constant
        let half_angle = 0.785_398_f64 as f32;
        let r = Quat::from_axis_angle(axis, 2.0 * half_angle) * w;

        // Probe 1 (820E6C30): straight down 100 (82000D8C) from 0.0125
        // (820029D8) along R.
        let start = pos + r * 0.0125;
        let Some(ground) = world.feeler(start, start - Vec3::Y * 100.0, 0x10, 0) else {
            return false;
        };
        let own = flat(ground.normal).normalize_or_zero();

        // 820E6CF8: straight over the coping, then (820E6DD4) diagonally
        // with exactly one of Left / Right.
        let mut target = transfer_target(world, pos, -r, own);
        if target.is_none() {
            let t = Vec3::new(-own.z, 0.0, own.x);
            let dir = if input.left && !input.right {
                Some((t - r).normalize_or_zero())
            } else if input.right && !input.left {
                Some((-t - r).normalize_or_zero())
            } else {
                None
            };
            target = dir.and_then(|d| transfer_target(world, pos, d, own));
        }
        let Some(target) = target else {
            return false;
        };

        // 820E6FA8: D from probe 1's point to the target, flat.
        let saved_v = v;
        let d_len = flat(target.point - ground.point).length();
        let c = u.dot(flat(target.normal).normalize_or_zero());
        let target_y = target.point.y;
        let mut goal = target.point;
        goal.y = pos.y;
        // 0.9 (82002A74), 0.6 (820024BC): unless the target is close and
        // (anti)parallel, all the speed goes straight up.
        if c.abs() < 0.9 || d_len > 0.6 {
            self.body.velocity = Vec3::new(0.0, v.length(), 0.0);
        }
        let v = self.body.velocity;
        let mut t = self.flight_time(s, goal.y, pos.y, v.y);

        // 820E7318: a target above the skater: try again next frame.
        if target_y > pos.y {
            self.transfer.retry = true;
            return true;
        }
        // 0.1 (82000BF4), 0.0025 (820027D0).
        t = t.max(0.1);
        let reach = d_len + 0.0025;
        if reach > 0.6 && (reach / t) * (reach / t) > v.length_squared() {
            return false;
        }
        // Probe 3 (820E7370): the straight line at the skater's height.
        if world.feeler(pos, goal, 0x10, 0).is_some() {
            self.transfer.retry = true;
            return true;
        }
        self.transfer.retry = false;

        // 820E73DC: the target orientation.
        let n = target.normal;
        let dx = goal.x - pos.x;
        let dz = goal.z - pos.z;
        let mut d = if !target.hip {
            Vec3::new(dx, -(n.x * dx + n.z * dz) / n.y, dz).normalize_or_zero()
        } else if (goal - pos).normalize_or_zero().dot(flat(n).normalize_or_zero()) < 0.0 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        };
        // 821ED798: onto the target plane.
        d = (d - n * d.dot(n)).normalize_or_zero();
        let mut to = Mat3::from_cols(n.cross(d), n, d);
        self.transfer.target_at = d;
        // 821EE530 / 820B9ED0: keep the skater's heading offset from its
        // velocity when it is at least 30 degrees (0.523599, 82002A78).
        let mut a = signed_angle(self.body.at(), saved_v, self.body.up());
        #[allow(clippy::approx_constant)] // the image's own constant
        let min_turn = 0.523_599;
        if a.abs() < min_turn {
            a = 0.0;
        }
        rotate_about_up(&mut to, -a);

        // 820E7710: the blend. 0.9 is 82002A74.
        let tr = &mut self.transfer;
        tr.from = self.body.matrix;
        tr.to = to;
        tr.elapsed = 0.0;
        tr.duration = t.max(0.9);
        tr.last = self.body.matrix;
        // 820E77E0: upside down halfway: start and target swap (purpose
        // UNKNOWN; CONFIRMED in the code).
        if matrix_slerp(&tr.from, &tr.to, 0.5).y_axis.y < 0.0 {
            std::mem::swap(&mut tr.from, &mut tr.to);
        }

        // 820E7848: the carry reaches the target in `t`.
        tr.ref_height = pos.y;
        tr.carry = Vec3::new((goal.x - pos.x) / t, 0.0, (goal.z - pos.z) / t);
        let script = if target.hip {
            "SkaterAwardHipTransfer"
        } else if target.bank {
            "SkaterAwardTransferToBank"
        } else {
            "SkaterAwardTransfer"
        };
        tr.actions.push(ScriptAction::Run(qb_key(script), Params::new()));
        tr.bank = target.bank;
        tr.actions.push(ScriptAction::ClearHandler(qb_key("Ollied")));
        self.set_transfer(true);
        self.set_flag_192(false);
        self.vert.tracking = false;
        self.set_break_window(false);
        self.transfer.on_object = target.on_object;
        true
    }

    /// Retail `820E1600`, skating only: look ahead for a surface to drop
    /// into. `pop` (`b_pop`): the skater rolled off an edge rather than
    /// jumped. On success the velocity keeps the changes made here.
    pub(crate) fn acid_drop_search(&mut self, s: &Scripts, world: &dyn World, pop: bool) -> Option<AcidDrop> {
        let pos = self.body.position;
        let old = self.old_position;
        // 820E1728: along the flat velocity; 0.00025 is 820027DC.
        let fv = flat(self.body.velocity);
        if fv.length() < 0.00025 {
            return None;
        }
        let dir = fv.normalize();
        // 820E181C: from the old position at the higher of the two
        // heights; 12.7 (82002A4C) ahead, feeling from 0 (h) above to 100
        // (82000D8C) below.
        let start = Vec3::new(old.x, pos.y.max(old.y), old.z);
        let range = 12.7;
        let unflagged = s.global_float("unflagged_bank_transfers") != 0.0;
        let mut found: Option<Hit> = None;
        // 0.00025 start (820027DC), 0.125 steps (82002414), plus 0.6
        // (820024BC) past 2.5 (82000C20).
        let mut k = 0.00025f32;
        while k < range {
            let p = start + dir * k;
            let hit = world.feeler(p, p - Vec3::Y * 100.0, 0x10, 0);
            // 820E22F4: which surfaces count; 0.907 is 8200298C. A bank
            // (0x100) sets +2616 even if the hit is then refused.
            let mut ok = false;
            if unflagged {
                self.transfer.bank = true;
                ok = true;
            } else if let Some(h) = hit {
                if h.flags & 0x8 != 0 && h.normal.y.abs() < 0.907 {
                    ok = true;
                } else if h.flags & 0x100 != 0 {
                    self.transfer.bank = true;
                    ok = true;
                }
            }
            if let (Some(h), true) = (hit, ok) {
                // 820E2398: not the ground just left (L1 distance to +144
                // above 0.1, 82001BA0) and facing the way the skater goes
                // (0.05, 82054BE8).
                let dn = h.normal - self.previous_normal;
                let l1 = dn.x.abs() + dn.y.abs() + dn.z.abs();
                if l1 > 0.1 && flat(h.normal).normalize_or_zero().dot(dir) >= 0.05 {
                    found = Some(h);
                    break;
                }
            }
            if k > 2.5 {
                k += 0.6;
            }
            k += 0.125;
        }
        let hit = found?;
        let mut goal = hit.point;
        let hit_y = goal.y;

        // 820E25A4: the flat way to the hit.
        let to_goal = flat(goal - pos);
        let mut d_len = to_goal.length();
        if d_len < 0.00025 {
            return None;
        }
        if to_goal.dot(dir) < 0.0 {
            d_len = -d_len;
        }
        let dir2 = to_goal / d_len;
        let saved_v = self.body.velocity;
        let vxz = flat(self.body.velocity).length();
        // 820E27B8: the pop (`Physics_Acid_Drop_Pop_Speed`), capped at
        // twice it (2 is 82000D78).
        let pop_speed = s.physics_float("Physics_Acid_Drop_Pop_Speed", self.on_bike);
        if pop {
            self.body.velocity.y = self.body.velocity.y.max(pop_speed);
        }
        self.body.velocity.y = self.body.velocity.y.min(2.0 * pop_speed);
        let vy = self.body.velocity.y;

        // 820E28C0: the height on arrival over the hit (3e-06 is 82002A50,
        // 0.5 is 82000BE8).
        let g = self.air_gravity_hang(s, true);
        let arrive_y = if d_len > 0.0 && vxz > 3e-6 {
            let th = d_len / vxz;
            pos.y + th * (vy + th * g * 0.5)
        } else {
            pos.y
        };
        let fail = |me: &mut Self| {
            me.body.velocity = saved_v;
            None
        };
        if arrive_y < goal.y {
            return fail(self);
        }
        // 820E2A1C: long enough in the air.
        let t = self.flight_time(s, goal.y, pos.y, vy);
        if t < s.global_float("Physics_Acid_Drop_Min_Air_Time") {
            return fail(self);
        }
        if goal.y >= arrive_y {
            return fail(self);
        }
        // 820E2AB0: the path, as two lines through the height reached at
        // half the fall time; blocked: aim 0.6 higher (820024BC) while
        // below the arrival height.
        loop {
            let big_g = -g;
            let disc = vy * vy + big_g * (pos.y - goal.y) * 2.0;
            // 1e-05 is 82000C18.
            let tf = if disc < 0.0 { 1e-5 } else { (disc.sqrt() + vy) / big_g };
            let th = tf * 0.5;
            let half = d_len * 0.5;
            let mid = Vec3::new(pos.x + dir2.x * half, pos.y + vy * th + th * th * g * 0.5, pos.z + dir2.z * half);
            // 0.0025 is 820027D0.
            let end = goal + Vec3::Y * 0.0025;
            let blocked = world.feeler(pos, mid, 0x10, 0).is_some() || world.feeler(mid, end, 0x10, 0).is_some();
            if !blocked {
                break;
            }
            goal.y += 0.6;
            if goal.y >= arrive_y {
                return fail(self);
            }
        }
        Some(AcidDrop { point: goal, normal: hit.normal, hit_y, on_object: false })
    }

    /// Retail `820DA7B0`: start the acid (or bank) drop `820E1600` found.
    pub(crate) fn acid_drop_start(&mut self, s: &Scripts, drop: &AcidDrop) {
        let pos = self.body.position;
        let v = self.body.velocity;
        self.transfer.speed = v.length();
        let to_goal = flat(drop.point - pos);
        let mut d_len = to_goal.length();
        if to_goal.dot(v) < 0.0 {
            d_len = -d_len;
        }
        let u = to_goal / d_len;
        let t = self.flight_time(s, drop.point.y, pos.y, v.y);
        let carry = u * d_len / t;
        self.transfer.carry = Vec3::new(carry.x, 0.0, carry.z);
        self.transfer.acid_carry = self.transfer.carry;
        self.transfer.ref_height = drop.point.y;
        self.set_transfer(true);
        self.vert.in_vert_air = true;
        self.set_flag_192(true);
        self.vert.tracking = false;
        self.transfer.on_object = drop.on_object;
        self.body.velocity.x = 0.0;
        self.body.velocity.z = 0.0;

        // 820DAB60: the target orientation, down the surface the way the
        // skater goes.
        let n = drop.normal;
        let mut d = Vec3::new(u.x, -(n.x * u.x + n.z * u.z) / n.y, u.z).normalize_or_zero();
        d = (d - n * d.dot(n)).normalize_or_zero();
        self.transfer.target_at = d;
        let mut to = Mat3::from_cols(n.cross(d), n, d);
        // 820DAD90: keep the heading offset (no 30 degree threshold here).
        let a = signed_angle(flat(self.body.at()), u, self.body.up());
        rotate_about_up(&mut to, -a);
        let tr = &mut self.transfer;
        tr.from = self.body.matrix;
        tr.to = to;
        tr.elapsed = 0.0;
        tr.duration = t;
        tr.last = self.body.matrix;

        // 820DAE88: SkaterAcidDropTriggered with DropHeight and the flags.
        // (`drop_backwards`, +1619, is bike-only and never set here.)
        let mut params = Params::new();
        params.add(qb_key("DropHeight"), Value::Float(pos.y - drop.hit_y));
        if tr.bank {
            params.add(0, Value::Checksum(qb_key("bank_drop")));
        }
        if tr.auto_drop {
            params.add(0, Value::Checksum(qb_key("auto_drop")));
        }
        tr.actions.push(ScriptAction::Run(qb_key("SkaterAcidDropTriggered"), params));
        self.set_no_acid_drop(true);
        self.transfer.auto_drop = false;
        self.transfer.actions.push(ScriptAction::ClearHandler(qb_key("Ollied")));
    }

    /// The acid drop check at the end of the air update (820F4060):
    /// allowed (`+200` clear), with the spine button or `+1618`; `pop` when
    /// the air began after the last jump and less than 250 ms ago.
    pub(crate) fn air_acid_drop(&mut self, s: &Scripts, world: &dyn World) {
        if self.transfer.no_acid_drop {
            return;
        }
        let input = self.last_input;
        if !self.spine_button(&input) && !self.transfer.auto_drop {
            return;
        }
        let pop = self.air_start_ms > self.jump_ms && self.time_ms - self.air_start_ms < 250;
        if let Some(drop) = self.acid_drop_search(s, world, pop) {
            self.acid_drop_start(s, &drop);
        }
    }

    /// Retail `820EA0D0`: in a transfer or drop, turn the skater from the
    /// take-off orientation toward the target over the flight
    /// (smoothstep), as a change applied to the live matrix so spins done
    /// meanwhile are kept. (SkaterState `+152`, the other gate, is bike
    /// state and never set.)
    pub(crate) fn transfer_blend(&mut self) {
        if !self.transfer.active {
            return;
        }
        let tr = &mut self.transfer;
        tr.elapsed += self.dt;
        let s = (tr.elapsed / tr.duration).min(1.0);
        // 820EA138: (s - 1.5) * s * s * -2 = 3s^2 - 2s^3.
        let f = (s - 1.5) * s * s * -2.0;
        let mut m = matrix_slerp(&tr.from, &tr.to, f);
        crate::air::orthonormalize_keep_up(&mut m);
        // 820EA2D4: the change since the last blend, `last^-1 * m` in
        // retail's row order (glam columns are retail rows, so the
        // product reverses). The order is INFERRED: it is the one that
        // lands exactly on `m` when nothing else turned the skater.
        let mut delta = m * tr.last.transpose();
        crate::air::orthonormalize_keep_up(&mut delta);
        let new = delta * self.body.matrix;
        // 820EA6B4: written only if |up.y| <= 10 (820564C4).
        if new.y_axis.y.abs() <= 10.0 {
            self.body.matrix = new;
        }
        self.matrix_32 = self.body.matrix;
        self.transfer.last = m;
    }

    /// Retail `820DAFF0` (from `820FC990` after the state update): after a
    /// transfer ends, a landing faster than 1.25 (820029B8) times
    /// `Skater_Max_Speed_Stat` keeps a raised cap that falls back at
    /// `Physics_Transfer_Speed_Limit_Override_Drop_Rate` a second.
    pub(crate) fn post_transfer_speed(&mut self, s: &Scripts) {
        let stat_max = |me: &Self| s.stat("Skater_Max_Speed_Stat", me.on_bike, &me.stats, me.stat_context);
        // `+2084`..`+2100` as they stand (the OverrideLimits fields).
        let mut o = self.override_base();
        if self.transfer.was_active && !self.transfer.active && !self.in_bail {
            let max = stat_max(self);
            let speed = self.body.velocity.length();
            if speed < max * 1.25 {
                return;
            }
            let over = s.physics_float("Physics_Transfer_Speed_Limit_Override_Max", self.on_bike);
            self.transfer.speed_factor = (speed / max).min(over);
            // 1e20 is 820029E4.
            o.max = 1e20;
            o.max_max = 1e20;
        } else if self.transfer.speed_factor == 1.0 {
            return;
        }
        // 820DB144: no time limit while it runs (-1).
        o.timer = -1.0;
        self.set_override(o);
        let rate = s.physics_float("Physics_Transfer_Speed_Limit_Override_Drop_Rate", self.on_bike);
        self.transfer.speed_factor -= self.dt * rate;
        // 820DB17C: back to normal: factor 1 and the override off (its
        // timer was just set to -1, so the -2 test never keeps it).
        let end = |me: &mut Self| {
            me.transfer.speed_factor = 1.0;
            me.set_override(crate::core_physics::OverrideLimits { timer: 0.0, ..o });
        };
        if self.transfer.speed_factor < 1.0 {
            end(self);
            return;
        }
        let max = stat_max(self);
        let limit = self.transfer.speed_factor * max;
        // 820DB1E4: in the air (SkaterState +24 == 1) the cap may also not
        // be above 1.1 (820029E0) times the current speed.
        let mut cap = limit.min(o.max);
        if self.state == crate::core_physics::State::Air {
            cap = cap.min(self.body.velocity.length() * 1.1);
        }
        if cap < max {
            end(self);
            return;
        }
        // 820DB2A8: +2088 = cap, +2092 = cap / max * Skater_Max_Speed_Stat
        // (read again; the same value). A global counter at 82731500 is
        // also stepped (purpose UNKNOWN).
        o.max = cap;
        o.max_max = cap / max * stat_max(self);
        self.set_override(o);
    }
}

/// Retail `820E0AC8`: step along `dir` from `pos` (0.25, 82000BEC, to
/// 12.7, 82002A4C, in 0.15 steps, 820029DC), feeling from 10 (820564C4)
/// above to 100 below, for the first vert (0x8) or 0x100 surface that is
/// steep (|n.y| < 0.907) and not facing the way `own` (the ramp the skater
/// is on) does (flat dot at most 0.95, 82002A00).
fn transfer_target(world: &dyn World, pos: Vec3, dir: Vec3, own: Vec3) -> Option<Target> {
    let mut k = 0.25f32;
    while k < 12.7 {
        let p = pos + dir * k;
        if let Some(h) = world.feeler(p + Vec3::Y * 10.0, p - Vec3::Y * 100.0, 0x10, 0)
            && h.flags & (0x8 | 0x100) != 0
            && h.normal.y.abs() < 0.907
        {
            let fn_ = flat(h.normal).normalize_or_zero();
            let c = fn_.dot(own);
            if c <= 0.95 {
                return Some(Target {
                    point: h.point,
                    normal: h.normal,
                    // -0.866 is 82002A44.
                    hip: c > -0.866,
                    bank: h.flags & 0x8 == 0,
                    on_object: false,
                });
            }
        }
        k += 0.15;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::ScriptAction;
    use crate::core_physics::{CorePhysics, State};
    use crate::script::Scripts;
    use crate::world::{Hit, World, filter_allows};
    use glam::{Mat3, Vec3};
    use p8_formats::qb::Value;
    use p8_formats::qb_key as k;

    fn scripts() -> Scripts {
        let f = |name: &str, v: f32| (k(name), Value::Float(v));
        Scripts::new(
            [
                (k("skater_physics"), Value::Struct(vec![])),
                f("Physics_Air_Gravity", -21.6),
                f("Physics_Air_hang_Stat", 0.9),
                f("Physics_Vert_hang_Stat", 1.0),
                f("Physics_Acid_Drop_Pop_Speed", 5.0),
                f("Physics_Acid_Drop_Min_Air_Time", 0.25),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// Test surfaces: planes limited to an x range (and |z| < 5).
    struct Planes(Vec<(Vec3, Vec3, f32, f32, u16)>);

    impl World for Planes {
        fn feeler(&self, a: Vec3, b: Vec3, ignore_1: u16, ignore_0: u16) -> Option<Hit> {
            let mut best: Option<(f32, Hit)> = None;
            for &(p, n, x0, x1, flags) in &self.0 {
                if !filter_allows(flags, ignore_1, ignore_0) {
                    continue;
                }
                let (da, db) = ((a - p).dot(n), (b - p).dot(n));
                if (da > 0.0) == (db > 0.0) {
                    continue;
                }
                let t = da / (da - db);
                let point = a + (b - a) * t;
                if point.x < x0 || point.x > x1 || point.z.abs() > 5.0 {
                    continue;
                }
                if best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                    best = Some((t, Hit { point, normal: n, flags, terrain: 0 }));
                }
            }
            best.map(|(_, h)| h)
        }
    }

    /// Two ramps back to back, tops at y = 3: A for x <= 0 facing -x,
    /// B for x >= 0.3 facing +x (both 0.8 / 0.6 steep, vert flag), and a
    /// floor at y = 0.
    fn spine(with_b: bool) -> Planes {
        let a = (Vec3::new(0.0, 3.0, 0.0), Vec3::new(-0.8, 0.6, 0.0), -2.25, 0.0, 0x8);
        let b = (Vec3::new(0.3, 3.0, 0.0), Vec3::new(0.8, 0.6, 0.0), 0.3, 2.55, 0x8);
        let floor = (Vec3::ZERO, Vec3::Y, -100.0, 100.0, 0x1);
        Planes(if with_b { vec![a, b, floor] } else { vec![a, floor] })
    }

    fn in_vert_air_over_a(s: &Scripts) -> CorePhysics {
        let mut p = CorePhysics::new(s);
        p.state = State::Air;
        p.vert.in_vert_air = true;
        p.body.position = Vec3::new(-0.2, 3.3, 0.0);
        // Up out of ramp A, at straight up.
        p.body.matrix = Mat3::from_cols(Vec3::Z, Vec3::NEG_X, Vec3::Y);
        p.body.velocity = Vec3::new(0.0, 6.0, 0.0);
        p
    }

    #[test]
    fn spine_transfer_aims_over_the_spine_and_down_the_far_ramp() {
        let s = scripts();
        let mut p = in_vert_air_over_a(&s);
        assert!(p.spine_transfer(&s, &spine(true)));
        assert!(p.transfer.active && !p.transfer.retry);
        // The far ramp is found at x = 0.35 (k = 0.55); time up and down
        // at vert hang: 2 * 6 / 21.6.
        let t = 2.0 * 6.0 / 21.6;
        assert!((p.transfer.carry - Vec3::new(0.55 / t, 0.0, 0.0)).length() < 1e-3);
        assert!((p.transfer.duration - 0.9).abs() < 1e-6);
        assert!((p.transfer.target_at - Vec3::new(0.6, -0.8, 0.0)).length() < 1e-4);
        assert_eq!(p.transfer.actions[0], ScriptAction::Run(k("SkaterAwardTransfer"), Default::default()));
        assert_eq!(p.transfer.actions[1], ScriptAction::ClearHandler(k("Ollied")));
        // The blend ends facing out of ramp B.
        for _ in 0..60 {
            p.transfer_blend();
        }
        assert!((p.body.up() - Vec3::new(0.8, 0.6, 0.0)).length() < 1e-3);
    }

    #[test]
    fn no_far_ramp_no_transfer() {
        let s = scripts();
        let mut p = in_vert_air_over_a(&s);
        assert!(!p.spine_transfer(&s, &spine(false)));
        assert!(!p.transfer.active && p.transfer.actions.is_empty());
    }

    #[test]
    fn a_blocked_line_is_retried() {
        let s = scripts();
        let mut p = in_vert_air_over_a(&s);
        // The straight line to the far ramp now runs into ramp A.
        p.body.position.y = 2.95;
        assert!(p.spine_transfer(&s, &spine(true)));
        assert!(p.transfer.retry && !p.transfer.active);
    }

    #[test]
    fn acid_drop_off_a_deck_into_the_ramp_below() {
        let s = scripts();
        // A deck at y = 3 for x < 0, then ramp B.
        let mut w = spine(true);
        w.0[0] = (Vec3::new(0.0, 3.0, 0.0), Vec3::Y, -100.0, 0.0, 0x1);
        let mut p = CorePhysics::new(&s);
        p.state = State::Air;
        p.body.position = Vec3::new(-0.1, 3.0025, 0.0);
        p.old_position = Vec3::new(-0.2, 3.0025, 0.0);
        p.body.velocity = Vec3::new(6.0, 0.0, 0.0);
        let drop = p.acid_drop_search(&s, &w, true).expect("a drop");
        // Popped to 5 up; the ramp found at x = 0.30025 (k = 0.50025).
        assert_eq!(p.body.velocity, Vec3::new(6.0, 5.0, 0.0));
        assert!((drop.point.x - 0.30025).abs() < 1e-4);
        p.acid_drop_start(&s, &drop);
        assert!(p.transfer.active && p.vert.in_vert_air && p.transfer.flag_192 && p.transfer.no_acid_drop);
        assert_eq!(p.body.velocity, Vec3::new(0.0, 5.0, 0.0));
        let t = p.transfer.duration;
        assert!((p.transfer.carry.x - 0.40025 / t).abs() < 1e-3);
        assert!((p.transfer.target_at - Vec3::new(0.6, -0.8, 0.0)).length() < 1e-4);
        let ScriptAction::Run(name, params) = &p.transfer.actions[0] else { panic!() };
        assert_eq!(*name, k("SkaterAcidDropTriggered"));
        assert!((params.float(k("DropHeight")).unwrap() - (3.0025 - drop.hit_y)).abs() < 1e-5);
    }

    #[test]
    fn blend_keeps_a_spin_made_meanwhile() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.transfer.active = true;
        p.transfer.from = Mat3::IDENTITY;
        p.transfer.last = Mat3::IDENTITY;
        p.transfer.to = Mat3::from_rotation_z(1.0);
        p.transfer.duration = 0.5;
        p.transfer_blend();
        crate::body::rotate_about_up(&mut p.body.matrix, 0.3);
        for _ in 0..40 {
            p.transfer_blend();
        }
        // The target, then the spin about its own up.
        let mut want = Mat3::from_rotation_z(1.0);
        crate::body::rotate_about_up(&mut want, 0.3);
        assert!((p.body.matrix.y_axis - want.y_axis).length() < 1e-4);
        assert!((p.body.matrix.z_axis - want.z_axis).length() < 1e-4);
    }

    #[test]
    fn flight_time_uses_the_vert_hang() {
        let s = scripts();
        let p = CorePhysics::new(&s);
        assert!((p.flight_time(&s, 0.0, 0.0, 6.0) - 12.0 / 21.6).abs() < 1e-5);
        // Never reached: 1e-5.
        assert_eq!(p.flight_time(&s, 10.0, 0.0, 1.0), 1e-5);
    }
}
