//! Vert: leaving a vert wall (quarter pipe) into vert air, following the
//! wall below while in the air, breaking out of vert over the lip, and the
//! eased normal these use. Surfaces with collision flag 0x8 are vert
//! (`820E5048` copies it to `+1185`, the snap to SkaterState `+72`).
//!
//! Spine transfers are in `transfer.rs`. Not translated: the skater-rotate
//! component's flags (`+2836`->`+40`/`+80`, never set), moving platforms
//! and bikes.
use crate::air::orthonormalize_keep_at;
use crate::core_physics::{CorePhysics, project_keep_length};
use crate::input::InputState;
use crate::script::Scripts;
use crate::world::World;
use glam::{Mat3, Vec3};

/// 0.0174533: degrees to radians (82000C10).
const DEG: f32 = 0.017453292;

/// Retail `820BE1B0` (matrix from `821E8EE8`): `M = RotX(angle) * M` in
/// row-vector form, a turn about the matrix's own row 0.
pub(crate) fn rotate_about_row0(m: &mut Mat3, angle: f32) {
    let (s, c) = (angle.sin(), angle.cos());
    let (y, z) = (m.y_axis, m.z_axis);
    m.y_axis = y * c + z * s;
    m.z_axis = z * c - y * s;
}

/// Retail `820BF0E8` (matrix from `821E93E8`): `M = RotZ(angle) * M`, a
/// turn about the matrix's own at row.
pub(crate) fn rotate_about_at(m: &mut Mat3, angle: f32) {
    let (s, c) = (angle.sin(), angle.cos());
    let (x, y) = (m.x_axis, m.y_axis);
    m.x_axis = x * c + y * s;
    m.y_axis = y * c - x * s;
}

impl CorePhysics {
    /// Retail `820D53F8`: the spine-transfer button ("R2") is held, unless
    /// in Nail the Trick (script `IsSkaterInNailTheTrick`; not translated,
    /// so never).
    pub(crate) fn spine_button(&self, input: &InputState) -> bool {
        input.r2
    }

    /// Set a SkaterState flag the retail way (`820D4...` inline pattern):
    /// the time stamp only changes when the value does.
    pub(crate) fn set_break_window(&mut self, on: bool) {
        if self.vert.break_window != on {
            self.vert.break_window = on;
            self.vert.break_window_ms = self.time_ms;
        }
    }

    /// Retail `820DA1A8`: `+96` eases from `+128` to the ground normal
    /// `+112` as `+160` runs from 1 to 0 at `Normal_Lerp_Speed` (x1.5 when
    /// the normal points down). Then, whether or not it eased this frame,
    /// the display matrix `+32` follows: when `+96` differs from its up
    /// (`+48`), up takes `+96`, the matrix is rebuilt around it
    /// (`821E9B58`, row 1) and its x and z rows are then overwritten with
    /// the object's (`+144`, `+176`). The skater matrix queries
    /// (`PitchGreaterThan`, `RollGreaterThan`, ...) read that display
    /// matrix, so without this a skater in vert air keeps a stale matrix
    /// while the landing normal is steep. A camera value (the global at
    /// `+5372`, stored to `+5368`) is not translated.
    pub(crate) fn ease_normal(&mut self, s: &Scripts) {
        let target = self.ground_normal;
        let mut speed = s.global_float("Normal_Lerp_Speed");
        if target.y < 0.0 {
            // 1.5 is the constant at 82000DD8.
            speed *= 1.5;
        }
        let v = &mut self.vert;
        if v.ease_left != 0.0 {
            if v.ease_from == target {
                v.ease_left = 0.0;
            } else {
                // 60 is the constant at 82001EB4.
                v.ease_left -= self.dt * speed * 60.0;
                if v.ease_left <= 0.0 {
                    v.ease_left = 0.0;
                    v.eased_normal = target;
                } else {
                    v.eased_normal = (target + (v.ease_from - target) * v.ease_left).normalize_or_zero();
                }
            }
        }
        if self.vert.eased_normal != self.matrix_32.y_axis {
            self.matrix_32.y_axis = self.vert.eased_normal;
            crate::air::orthonormalize_keep_up(&mut self.matrix_32);
            self.matrix_32.x_axis = self.body.matrix.x_axis;
            self.matrix_32.z_axis = self.body.matrix.z_axis;
        }
    }

    /// Retail 820F3018..820F30A0 (air update): the normal easing (and
    /// with it the display matrix `+32`) runs in vert air (`+56`) only when
    /// there is no spine transfer (`+136`) (and the skaterrotate
    /// component's `+40` / `+80` are 0, never set here); otherwise the
    /// display matrix is a copy of the object's. In a transfer the eased
    /// normal stays the take-off wall's, so easing would keep the matrix the
    /// landing checks read on the wall while the object turns to the landing
    /// slope (every spine transfer landing bailed).
    pub(crate) fn follow_display_matrix(&mut self, s: &Scripts) {
        if self.vert.in_vert_air && !self.transfer.active {
            self.ease_normal(s);
        } else {
            self.matrix_32 = self.body.matrix;
        }
    }

    /// Retail `820DA3D0`: leaving the ground. On a vert wall, keep only the
    /// speed along the wall, face the skater's up out from the wall, push
    /// out by `Physics_Vert_Push_Out` and enter vert air.
    pub(crate) fn vert_takeoff(&mut self, s: &Scripts) {
        // Upside down (-0.1 at 820029A0): out of vert, step off the
        // ceiling by 0.025 (820027D4). Retail then runs script `ForcedBail`
        // with `allow_quick_exit`; bails are not translated.
        if self.ground_normal.y < -0.1 {
            self.vert.in_vert_air = false;
            self.body.position -= self.ground_normal * 0.025;
            return;
        }
        if !self.vert.on_vert_ground {
            self.vert.in_vert_air = false;
            return;
        }
        self.vert.normal = self.ground_normal;
        // Object `+128`, lowered by 0.0025 (820027D0).
        self.vert.point = self.old_position - Vec3::Y * 0.0025;
        let flat = Vec3::new(self.ground_normal.x, 0.0, self.ground_normal.z);
        // 0.001 is the constant at 82000D80.
        if flat.length() <= 0.001 {
            self.vert.in_vert_air = false;
            return;
        }
        let n = flat.normalize();
        self.body.velocity = project_keep_length(self.body.velocity, n);
        self.orient_to_ground(n);
        // 820DA3D0: the at row with y negated goes to `+1296`, and
        // SkaterState `+120` is set: the vert auto-turn (`air_rotation`)
        // turns the skater round to that facing on the way down.
        let at = self.body.at();
        self.vert.auto_turn_dir = Vec3::new(at.x, -at.y, at.z);
        self.vert.auto_turn = true;
        self.body.position += n * s.global_float("Physics_Vert_Push_Out");
        self.vert.in_vert_air = true;
        self.vert.tracking = true;
        // 0.15 is the constant at 820029DC.
        self.vert.lift = 0.15;
    }

    /// Retail `820EC7B0`: break out of vert air, over the lip. Happens when
    /// "Up" has been held longer than `Skater_vert_push_time` (and not
    /// Left, Right, Square or Circle), with the spine button or `+1380`
    /// (a transfer to retry), or when `force` is set (`forcebreakvert`).
    /// `world` is `None` only when a script runs before the level is
    /// given; the spine search is then skipped.
    pub(crate) fn break_vert(&mut self, s: &Scripts, world: Option<&dyn World>, force: bool) {
        let input = self.last_input;
        let push_time = s.global_float("Skater_vert_push_time") as i32;
        let pushing = !self.on_bike
            && input.up
            && input.up_held_ms > push_time
            && !input.left
            && !input.right
            && !input.kick
            && !input.circle;
        let spine = self.spine_button(&input) || self.transfer.retry;
        if !pushing && !spine && !force {
            return;
        }
        let n = self.vert.eased_normal;
        if spine {
            // 820ECC84: first the spine transfer `820E68A8`; when it took
            // over, only the matrix copy (820ECC38).
            if let Some(world) = world
                && self.spine_transfer(s, world)
            {
                self.matrix_32 = self.body.matrix;
                return;
            }
            // No target: move 0.6 (820024BC) toward the wall's far side.
            self.body.velocity.x -= n.x * 0.6;
            self.body.velocity.z -= n.z * 0.6;
            let tilt = s.global_float("Skater_Break_Vert_forward_tilt");
            rotate_about_row0(&mut self.body.matrix, tilt * DEG);
            self.vert.in_vert_air = false;
            self.vert.tracking = false;
            self.set_break_window(false);
            self.vert.over_ground = true;
            self.face_velocity();
            return;
        }
        let k = self.body.velocity.length() * s.global_float("physics_break_air_speed_scale");
        self.body.velocity.x -= n.x * k;
        self.body.velocity.z -= n.z * k;
        self.body.velocity.y *= s.global_float("physics_break_air_up_scale");
        let tilt = s.global_float("Skater_Break_Vert_forward_tilt");
        rotate_about_row0(&mut self.body.matrix, tilt * DEG);
        self.vert.in_vert_air = false;
        self.vert.tracking = false;
        self.set_break_window(false);
        // 820ECB18: acid drops allowed again (+200).
        self.set_no_acid_drop(false);
        self.face_velocity();
        self.matrix_32 = self.body.matrix;
    }

    /// The end of `820EC7B0`: the at row takes the flat direction of the
    /// velocity (keeping its own y), then the matrix is rebuilt around it.
    fn face_velocity(&mut self) {
        let dir = self.body.velocity.normalize_or_zero();
        let m = &mut self.body.matrix;
        m.z_axis.x = dir.x;
        m.z_axis.z = dir.z;
        m.z_axis = m.z_axis.normalize_or_zero();
        orthonormalize_keep_at(m);
    }

    /// Retail `820DC5D0`: outside vert air, roll the skater upright about
    /// the at row at `skater_upright_sideways_speed` degrees a second while
    /// up leans sideways by more than `1.2 * dt`, unless at points nearly
    /// straight up or down (0.95).
    pub(crate) fn upright_sideways(&mut self, s: &Scripts) {
        let at = self.body.at();
        // 0.95 is the constant at 82002A00.
        if at.y.abs() > 0.95 {
            return;
        }
        let side = Vec3::new(-at.z, 0.0, at.x);
        let lean = self.body.up().dot(side);
        // 1.2 and -1.2 are the constants at 820029FC and 820029F8.
        let rate = s.global_float("skater_upright_sideways_speed") * DEG * self.dt;
        let angle = if lean > self.dt * 1.2 {
            rate
        } else if lean < self.dt * -1.2 {
            -rate
        } else {
            return;
        };
        rotate_about_at(&mut self.body.matrix, angle);
        self.matrix_32 = self.body.matrix;
    }

    /// The vert block of the air update (`820F2900`..`820F3014`), after the
    /// move: 820F2900..820F2A70 the break-vert window, 820F2A74..820F2AC8
    /// the spine gate, 820F2B08..820F3014 following the wall.
    pub(crate) fn vert_air_update(&mut self, s: &Scripts, world: &dyn World) {
        let input = self.last_input;
        if self.vert.break_window {
            if self.vert.tracking && self.vert.in_vert_air {
                // Nothing under the board (0.0025 above to 0.58 below,
                // 820027D0 and 82002AE8): the break-vert check.
                let up = self.body.up();
                let pos = self.body.position;
                if world.feeler(pos + up * 0.0025, pos + up * -0.58, 0x10, 0).is_none() {
                    self.break_vert(s, Some(world), false);
                    let t = s.global_float("Skater_vert_active_up_time") as i32;
                    if input.up_released_ms > t && input.up_held_ms > t {
                        self.set_break_window(false);
                    }
                }
            }
            let allow = s.global_float("Skater_Vert_Allow_break_Time") as i64;
            if self.time_ms - self.vert.break_window_ms > allow {
                self.set_break_window(false);
            }
        } else if self.vert.in_vert_air
            && (self.spine_button(&input) || self.transfer.retry)
            && !self.transfer.active
            && self.body.velocity.y > 0.0
        {
            // 820F2A74: the spine gate.
            self.break_vert(s, Some(world), false);
        }
        // 820F2ACC..820F2B00: with a movable contact (820CD810) retail
        // skips the wall follow: not translated (no moving platforms).
        if self.vert.tracking && self.vert.in_vert_air {
            self.follow_vert_wall(s, world);
        }
    }

    /// `820F2B28`..`820F3014`: look across the vert wall's plane (0.76 each
    /// side, 82002AE4) at the height followed so far for a vert surface,
    /// first `+1312` higher, then lower in 0.075 steps (82002960), up to 10
    /// times. If found and still facing the same way, move over it, take its
    /// normal and push out; otherwise stop following.
    fn follow_vert_wall(&mut self, s: &Scripts, world: &dyn World) {
        let pos = self.body.position;
        let n = self.vert.normal;
        let base = Vec3::new(pos.x, self.vert.point.y, pos.z);
        let mut a = base + n * 0.76;
        let mut b = base - n * 0.76;
        let vert_hit = |a: Vec3, b: Vec3| world.feeler(a, b, 0, 0).filter(|h| h.flags & 0x8 != 0);
        let mut hit = None;
        // 0.0125 is the constant at 820029D8.
        if self.vert.lift > 0.0125 {
            let lift = Vec3::Y * self.vert.lift;
            hit = vert_hit(a + lift, b + lift);
            if hit.is_none() {
                // 0.5 is f30 (82000BE8).
                self.vert.lift *= 0.5;
            }
        }
        if hit.is_none() {
            hit = vert_hit(a, b);
        }
        if hit.is_none() {
            for _ in 0..10 {
                a.y -= 0.075;
                b.y -= 0.075;
                hit = vert_hit(a, b);
                if let Some(h) = hit {
                    self.vert.point.y = h.point.y;
                    break;
                }
            }
        }
        // sqrt(|hit.n . n| over x and z); 0.02 is the constant at 82002968.
        let Some(h) = hit.filter(|h| (h.normal.x * n.x + n.z * h.normal.z).abs().sqrt() > 0.02) else {
            // 820F2FF4..820F3014: +64 cleared (retail also stamps +68, not
            // kept; no reader known).
            self.vert.tracking = false;
            return;
        };
        let mut point = h.point;
        if self.vert.point.y > point.y {
            point.y = self.vert.point.y;
        }
        self.vert.point = point;
        self.body.position.x = point.x;
        self.body.position.z = point.z;
        self.vert.normal = h.normal;
        self.body.position += h.normal * s.global_float("Physics_Vert_Push_Out");
        let flat = Vec3::new(h.normal.x, 0.0, h.normal.z).normalize_or_zero();
        self.orient_to_ground(flat);
        self.body.velocity = project_keep_length(self.body.velocity, flat);
    }
}

#[cfg(test)]
mod tests {
    use crate::core_physics::{CorePhysics, State};
    use crate::input::InputState;
    use crate::script::Scripts;
    use crate::world::{Hit, World, filter_allows};
    use glam::Vec3;
    use p8_formats::qb::Value;
    use p8_formats::qb_key as k;

    fn scripts() -> Scripts {
        let f = |name: &str, v: f32| (k(name), Value::Float(v));
        Scripts::new(
            [
                (
                    k("skater_physics"),
                    Value::Struct(vec![f("skater_autoturn_vert_angle", 5.0), f("skater_autoturn_speed", 3.0), f("skater_autoturn_cancel_time", 300.0)]),
                ),
                f("Physics_Vert_Push_Out", 0.075),
                f("Normal_Lerp_Speed", 0.1),
                f("Skater_vert_push_time", 130.0),
                f("Skater_vert_active_up_time", 250.0),
                f("Skater_Vert_Allow_break_Time", 200.0),
                f("physics_break_air_speed_scale", 0.75),
                f("physics_break_air_up_scale", 0.75),
                f("Skater_Break_Vert_forward_tilt", 45.0),
                f("skater_upright_sideways_speed", -60.0),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// A vert wall in the plane z = 0 facing -z, from y = 0 up to `top`.
    struct Wall {
        top: f32,
    }

    impl World for Wall {
        fn feeler(&self, start: Vec3, end: Vec3, ignore_1: u16, ignore_0: u16) -> Option<Hit> {
            if !filter_allows(0x8, ignore_1, ignore_0) || start.z >= 0.0 || end.z < 0.0 {
                return None;
            }
            let t = -start.z / (end.z - start.z);
            let point = start + (end - start) * t;
            (point.y >= 0.0 && point.y <= self.top).then_some(Hit {
                point,
                normal: Vec3::NEG_Z,
                flags: 0x8,
                terrain: 0,
            })
        }
    }

    #[test]
    fn takeoff_from_vert_keeps_speed_along_the_wall_and_faces_out() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.vert.on_vert_ground = true;
        p.ground_normal = Vec3::new(0.0, 0.05, -1.0).normalize();
        p.body.velocity = Vec3::new(0.5, 8.0, 0.4);
        let speed = p.body.velocity.length();
        p.body.position = Vec3::new(0.0, 3.0, -0.1);
        p.vert_takeoff(&s);
        assert!(p.vert.in_vert_air && p.vert.tracking);
        assert!(p.body.velocity.z.abs() < 1e-5);
        assert!((p.body.velocity.length() - speed).abs() < 1e-4);
        assert!((p.body.up() - Vec3::NEG_Z).length() < 1e-5);
        assert!((p.body.position.z - (-0.175)).abs() < 1e-5);
    }

    #[test]
    fn takeoff_from_a_non_vert_surface_is_not_vert_air() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.vert.in_vert_air = true;
        p.vert_takeoff(&s);
        assert!(!p.vert.in_vert_air);
    }

    #[test]
    fn vert_air_follows_the_wall_below() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.state = State::Air;
        p.vert.in_vert_air = true;
        p.vert.tracking = true;
        p.vert.point = Vec3::new(0.0, 1.0, 0.0);
        p.vert.normal = Vec3::NEG_Z;
        p.vert.lift = 0.15;
        p.body.position = Vec3::new(2.0, 2.0, -0.3);
        p.body.velocity = Vec3::new(1.0, 0.0, -0.5);
        p.vert_air_update(&s, &Wall { top: 3.0 });
        assert!(p.vert.tracking);
        assert!((p.body.position - Vec3::new(2.0, 2.0, -0.075)).length() < 1e-5);
        assert!((p.vert.point.y - 1.15).abs() < 1e-5);
        assert!(p.body.velocity.z.abs() < 1e-5);
        assert!((p.body.velocity.length() - 1.25f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn vert_air_searches_down_then_stops_following() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.state = State::Air;
        p.vert.in_vert_air = true;
        p.vert.tracking = true;
        p.vert.point = Vec3::new(0.0, 1.0, 0.0);
        p.vert.normal = Vec3::NEG_Z;
        p.body.position = Vec3::new(0.0, 2.0, -0.1);
        // Found 7 steps of 0.075 down (1 - 0.525 = 0.475 is under 0.5).
        p.vert_air_update(&s, &Wall { top: 0.5 });
        assert!(p.vert.tracking);
        assert!((p.vert.point.y - 0.475).abs() < 1e-4);
        // Nothing within 10 steps.
        p.vert.point.y = 2.0;
        p.vert_air_update(&s, &Wall { top: 0.5 });
        assert!(!p.vert.tracking);
        assert!(p.vert.in_vert_air);
    }

    #[test]
    fn holding_up_breaks_vert_over_the_lip() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.vert.in_vert_air = true;
        p.vert.eased_normal = Vec3::NEG_Z;
        p.body.velocity = Vec3::new(0.0, 5.0, 0.0);
        p.last_input = InputState { up: true, up_held_ms: 100, ..Default::default() };
        p.break_vert(&s, None, false);
        assert!(p.vert.in_vert_air, "not held long enough");
        p.last_input.up_held_ms = 200;
        p.break_vert(&s, None, false);
        assert!(!p.vert.in_vert_air);
        assert!((p.body.velocity - Vec3::new(0.0, 3.75, 3.75)).length() < 1e-5);
        // Tilted 45 degrees forward (nose down), then turned to the flat
        // direction of the velocity, which here it already had.
        let h = 0.5f32.sqrt();
        assert!((p.body.at() - Vec3::new(0.0, -h, h)).length() < 1e-5);
    }

    #[test]
    fn upright_sideways_rolls_up_toward_straight() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        crate::vert::rotate_about_at(&mut p.body.matrix, 0.5);
        let side = |p: &CorePhysics| p.body.up().dot(Vec3::new(-p.body.at().z, 0.0, p.body.at().x));
        let before = side(&p).abs();
        p.upright_sideways(&s);
        assert!(side(&p).abs() < before);
        assert!((p.body.at() - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn eased_normal_reaches_the_ground_normal_in_ten_frames() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.orient_to_ground(Vec3::NEG_Z);
        for _ in 0..9 {
            p.ease_normal(&s);
        }
        assert!(p.vert.eased_normal != Vec3::NEG_Z);
        p.ease_normal(&s);
        p.ease_normal(&s);
        assert_eq!(p.vert.eased_normal, Vec3::NEG_Z);
    }

    #[test]
    fn display_matrix_follows_the_eased_normal_with_the_object_rows() {
        // 820DA1A8: the display matrix's up takes the eased normal and its
        // x / z rows are then the object's, even on a frame with no easing.
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.orient_to_ground(Vec3::NEG_Z);
        p.ease_normal(&s);
        assert_eq!(p.matrix_32.y_axis, p.vert.eased_normal);
        assert_eq!(p.matrix_32.x_axis, p.body.matrix.x_axis);
        assert_eq!(p.matrix_32.z_axis, p.body.matrix.z_axis);
        for _ in 0..12 {
            p.ease_normal(&s);
        }
        assert_eq!(p.matrix_32.y_axis, Vec3::NEG_Z);
        p.matrix_32.y_axis = Vec3::Y;
        p.ease_normal(&s);
        assert_eq!(p.matrix_32.y_axis, Vec3::NEG_Z, "updated although nothing eased");
    }

    #[test]
    fn a_spine_transfer_keeps_the_display_matrix_on_the_object() {
        // 820F3018: no normal easing while +136 is set, so the display
        // matrix (read by the landing bail checks) is the object's.
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.vert.in_vert_air = true;
        p.vert.eased_normal = Vec3::NEG_X;
        p.body.matrix.y_axis = Vec3::Y;
        p.follow_display_matrix(&s);
        assert_eq!(p.matrix_32.y_axis, Vec3::NEG_X, "vert air without a transfer eases");
        p.transfer.active = true;
        p.follow_display_matrix(&s);
        assert_eq!(p.matrix_32, p.body.matrix);
    }

    #[test]
    fn the_vert_auto_turn_turns_the_skater_to_face_down_the_wall() {
        // 820E9620: in vert air the at row turns at skater_autoturn_speed
        // toward the takeoff at row with y negated, then the flag clears.
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.dt = 1.0 / 60.0;
        let at = Vec3::new(0.5, 0.866, 0.0).normalize();
        let up = Vec3::NEG_Z;
        p.body.matrix = glam::Mat3::from_cols(up.cross(at), up, at);
        p.vert.in_vert_air = true;
        p.vert.auto_turn = true;
        p.vert.auto_turn_dir = Vec3::new(at.x, -at.y, at.z);
        let input = InputState::default();
        let mut frames = 0;
        while p.vert.auto_turn && frames < 600 {
            p.air_rotation(&s, &input);
            frames += 1;
        }
        assert!(!p.vert.auto_turn, "never finished");
        assert!(p.body.at().dot(Vec3::new(at.x, -at.y, at.z)) > 0.999, "at {:?}", p.body.at());
        // 120 degrees at 3 rad/s (172 degrees/s) is 0.7 s.
        assert!((frames as f32 * p.dt - 0.70).abs() < 0.05, "frames {frames}");
        // A held spin input cancels it (skater_autoturn_cancel_time).
        p.vert.auto_turn = true;
        let held = InputState { l1: true, left: true, left_held_ms: 400, ..Default::default() };
        p.air_rotation(&s, &held);
        assert!(!p.vert.auto_turn);
    }
}
