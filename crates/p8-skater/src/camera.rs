//! The skater camera (retail component `skatercamera`, update `820D1238`),
//! translated for the ground and air states (skater state 0 and 1, including
//! vert air). Everything here was read in the retail code; what is not
//! translated is listed in [`SkaterCamera::update`].
//!
//! Matrices are kept as rows the way retail stores them: `[right, up, at]`.
//! The camera's own matrix (camera object `+144`) is `[-right, up, -at]`
//! of the frame it follows, so its third row points from the skater back
//! towards the camera.
//!
//! Per frame (`820D1238`):
//! 1. A follow frame from the skater (`820D1AA4..820D2980`): on the ground
//!    the velocity direction mostly flattened onto the ground (`0.8` of its
//!    normal part removed), in the air the flat velocity direction with a
//!    world-up frame, in vert air straight down (`(0, -1, 0)`); standing
//!    still keeps the skater's display matrix.
//! 2. Pitch by `tilt` plus an extra air tilt (`+348`, up to 20 degrees).
//! 3. Slerp the camera's orientation to that frame (`slerp`, or
//!    `vert_air_slerp` / `vert_air_landed_slerp`), with a limiter on how fast
//!    the turn may speed up (`+44`).
//! 4. Position: a focus point that follows the skater with `lerp_xz` /
//!    `lerp_y` (`820D0F48`), moved back along the camera by `behind * zoom`
//!    (`820D02A8`) and up by `above`.
//! 5. Look at the skater (+ `above`), then roll the horizon level on the
//!    ground (`skater_cam_lean_out_transition_time`, `820D3E00..820D3ED8`).
use crate::core_physics::{CorePhysics, State};
use crate::script::Scripts;
use glam::{Mat3, Quat, Vec3};
use p8_formats::qb::Value;
use p8_formats::qb_key;

/// Rows `[right, up, at]` (retail row-major 3x3 part of a 4x4).
pub type Rows = [Vec3; 3];

/// One camera mode (`Skater_Camera_*` struct), read by `820CFB30`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mode {
    pub horiz_fov: f32,
    pub behind: f32,
    pub above: f32,
    /// `+240`
    pub balance_trick_above: f32,
    /// `+344`
    pub tilt: f32,
    /// `+376` (read, not used by the translated states)
    pub origin_offset: f32,
    /// `+352`
    pub lip_trick_tilt: f32,
    /// `+356`
    pub lip_trick_above: f32,
    /// `+364`
    pub slerp: f32,
    /// `+368`
    pub vert_air_slerp: f32,
    /// `+372`
    pub vert_air_landed_slerp: f32,
    /// `+260`
    pub lerp_xz: f32,
    /// `+264`
    pub lerp_y: f32,
    /// `+268`
    pub vert_air_lerp_xz: f32,
    /// `+272`
    pub vert_air_lerp_y: f32,
    /// `+280`
    pub grind_lerp: f32,
    /// `+276`
    pub focus_mode_lerp: f32,
    /// `+312`
    pub zoom_lerp: f32,
    /// `+316`
    pub big_air_trick_zoom: f32,
    /// `+320`
    pub grind_zoom: f32,
    /// `+324`
    pub focus_mode_zoom: f32,
    /// `+328`
    pub lip_trick_zoom: f32,
}

impl Mode {
    /// Retail `820CFB30`: mode `index` of `Skater_Camera_Array` (one player,
    /// not on a bike; the split-screen and bike arrays are not used here).
    /// Members missing from the struct keep the component's previous value
    /// (`r6 = 0` optional reads); `prev` holds those (the constructor's
    /// values, `820D0890`, for the first mode).
    ///
    /// Not translated: 820CFB5C, 820CFB68, 820CFB84, 820CFB8C, 820CFB98 (the
    /// split-screen test), 820CFBA4, 820CFBAC, 820CFBB8 (on a bike, core
    /// +1564) and the other arrays 820CFBC8, 820CFBF0, 820CFC04.
    /// Translated: 820CFBDC (Skater_Camera_Array), 820CFC14 / 820CFC1C (the
    /// entry), the member reads 820CFC20..820CFF94 (`Name`, 820CFFC0, is
    /// not needed).
    pub fn load(s: &Scripts, index: usize, prev: &Mode) -> Option<Mode> {
        let Some(Value::Array(list)) = s.global("Skater_Camera_Array") else { return None };
        let v = match list.get(index)? {
            Value::Checksum(k) => s.globals.get(k)?,
            v => v,
        };
        let f = |name: &str, old: f32| v.get_named(name).and_then(Value::as_f32).unwrap_or(old);
        let p = prev;
        Some(Mode {
            // 820CFC44: horiz_fov starts at 0 before the read.
            horiz_fov: f("horiz_fov", 0.0),
            behind: f("behind", p.behind),
            above: f("above", p.above),
            balance_trick_above: f("balance_trick_above", p.balance_trick_above),
            tilt: f("tilt", p.tilt),
            origin_offset: f("origin_offset", p.origin_offset),
            // 820CFC44: lip_trick_tilt (+352) is cleared first too.
            lip_trick_tilt: f("lip_trick_tilt", 0.0),
            lip_trick_above: f("lip_trick_above", p.lip_trick_above),
            slerp: f("slerp", p.slerp),
            vert_air_slerp: f("vert_air_slerp", p.vert_air_slerp),
            vert_air_landed_slerp: f("vert_air_landed_slerp", p.vert_air_landed_slerp),
            lerp_xz: f("lerp_xz", p.lerp_xz),
            lerp_y: f("lerp_y", p.lerp_y),
            vert_air_lerp_xz: f("vert_air_lerp_xz", p.vert_air_lerp_xz),
            vert_air_lerp_y: f("vert_air_lerp_y", p.vert_air_lerp_y),
            grind_lerp: f("grind_lerp", p.grind_lerp),
            focus_mode_lerp: f("focus_mode_lerp", p.focus_mode_lerp),
            // 820CFEE0..820CFF00: the four zooms are set to 1 before the reads.
            zoom_lerp: f("zoom_lerp", p.zoom_lerp),
            big_air_trick_zoom: f("big_air_trick_zoom", 1.0),
            grind_zoom: f("grind_zoom", 1.0),
            focus_mode_zoom: f("focus_mode_zoom", 1.0),
            lip_trick_zoom: f("lip_trick_zoom", 1.0),
        })
    }

    /// The constructor's values (`820D0890`).
    fn constructed() -> Mode {
        Mode {
            vert_air_lerp_xz: 1.0,
            vert_air_lerp_y: 1.0,
            zoom_lerp: 0.0625, // 82002410
            focus_mode_lerp: 1.0,
            lerp_xz: 0.25, // 82000BEC
            lerp_y: 0.5,   // 82000BE8
            grind_lerp: 0.1, // 82000BF4
            ..Default::default()
        }
    }
}

/// The camera component's state. Field docs give the retail offsets.
#[derive(Clone, Debug)]
pub struct SkaterCamera {
    pub mode: Mode,
    /// `+36`
    pub mode_index: usize,
    /// `+224` behind now, `+228` time left, `+232` step per frame.
    pub behind: f32,
    behind_time: f32,
    behind_step: f32,
    /// `+236` above now, `+244` time left, `+248` step per frame.
    pub above: f32,
    above_time: f32,
    above_step: f32,
    /// `+40`: frames left that snap instead of easing (3 when the skater is
    /// attached, `820D0618`).
    pub instant_frames: i32,
    /// Camera object `+144..+176`: the camera's final rows `[-right, up,
    /// -at]` (the look-at matrix, `820D43DC`).
    pub matrix: Rows,
    /// Component `+64..+96`: the eased orientation (`[-right, up, -at]` of
    /// the follow frame), saved at `820D368C` and copied into the camera
    /// object at the start of each update (`820D135C`), so the slerp
    /// continues from it, not from the look-at matrix.
    pub orient: Rows,
    /// Camera object `+112`: the camera position.
    pub position: Vec3,
    /// The point looked at this frame (stack `400(r1)`).
    pub target: Vec3,
    /// `+128..+160`: the follow frame kept from the last update.
    pub frame: Rows,
    /// `+208`: the last velocity followed.
    pub last_velocity: Vec3,
    /// `+48`: the focus point (`820D0F48`).
    pub focus: Vec3,
    /// `+44`: the last frame's turn (dot of the at rows), for the limiter.
    turn_dot: f32,
    /// `+252`: time left of the landed-from-vert slerp.
    landed_time: f32,
    /// `+296`: big-air-trick zoom latched.
    big_air: bool,
    /// `+300`: zoom now.
    zoom: f32,
    /// `+304`: above now (eased by `zoom_lerp`).
    above_eased: f32,
    /// `+340`: grind lean (stays 0: grinds are not translated).
    lean: f32,
    /// `+348`: extra air tilt (radians).
    pub air_tilt: f32,
    /// `+404`: air blend (0 ground .. 1 vert air).
    pub air_blend: f32,
    /// `+408`: upside-down blend.
    upside_blend: f32,
    /// `+416`: the up used for the offset last frame.
    last_up: Vec3,
    /// `+480`: lean-out time left.
    lean_out: f32,
    /// `+484`: 2 s timer re-armed while the up is steep or in vert.
    air_timer: f32,
}

/// Retail `820BBD60`: a per-60 Hz-frame blend factor for frame time `dt`.
pub fn blend_for_dt(s: f32, dt: f32) -> f32 {
    let n = dt * 60.0; // 82001EB4
    n * s / (1.0 - s + n * s)
}

fn safe_normalize(v: Vec3) -> Vec3 {
    // The retail vector normalize leaves a zero vector as it is.
    let l = v.length();
    if l == 0.0 || !l.is_finite() { v } else { v / l }
}

/// Retail `820B9ED0` (`821E9168`): rotate the rows about their own up.
fn rotate_y(m: &mut Rows, a: f32) {
    let (s, c) = a.sin_cos();
    let (r0, r2) = (m[0], m[2]);
    m[0] = r0 * c - r2 * s;
    m[2] = r0 * s + r2 * c;
}

/// Retail `820BE1B0` (`821E8EE8`): rotate the rows about their own right.
fn rotate_x(m: &mut Rows, a: f32) {
    let (s, c) = a.sin_cos();
    let (r1, r2) = (m[1], m[2]);
    m[1] = r1 * c + r2 * s;
    m[2] = r2 * c - r1 * s;
}

/// Retail `820BF0E8` (`821E93E8`): rotate the rows about their own at.
fn rotate_z(m: &mut Rows, a: f32) {
    let (s, c) = a.sin_cos();
    let (r0, r1) = (m[0], m[1]);
    m[0] = r0 * c + r1 * s;
    m[1] = r1 * c - r0 * s;
}

/// Retail `820BAEC0` (`820BA2E0`): both matrices to quaternions and slerp.
/// The slerp's own branches (shortest path, near-parallel) were not read:
/// glam's are used (APPROXIMATE there).
fn slerp_rows(from: &Rows, to: &Rows, t: f32) -> Rows {
    let q0 = Quat::from_mat3(&Mat3::from_cols(from[0], from[1], from[2])).normalize();
    let q1 = Quat::from_mat3(&Mat3::from_cols(to[0], to[1], to[2])).normalize();
    let m = Mat3::from_quat(q0.slerp(q1, t));
    [m.x_axis, m.y_axis, m.z_axis]
}

/// `+x` rows of a glam matrix stored as columns (the skater's matrices).
fn rows_of(m: &Mat3) -> Rows {
    [m.x_axis, m.y_axis, m.z_axis]
}

fn angle_deg(a: Vec3, b: Vec3) -> f32 {
    // 821ED9E8
    safe_normalize(a).dot(safe_normalize(b)).clamp(-1.0, 1.0).acos().to_degrees()
}

impl SkaterCamera {
    /// Constructor (`820D0890`), mode 2 loaded with no lerp time (`8219A044`,
    /// `Skater_Camera_Standard_Medium`), skater attached (`820D0618`: three
    /// snapping frames).
    pub fn new(s: &Scripts) -> Self {
        let base = Mode::constructed();
        let mode = Mode::load(s, 2, &base).unwrap_or(base);
        let mut c = SkaterCamera {
            mode,
            mode_index: 2,
            behind: mode.behind,
            behind_time: 0.0,
            behind_step: 0.0,
            above: mode.above,
            above_time: 0.0,
            above_step: 0.0,
            instant_frames: 3,
            matrix: [Vec3::X, Vec3::Y, Vec3::Z],
            orient: [Vec3::X, Vec3::Y, Vec3::Z],
            position: Vec3::ZERO,
            target: Vec3::ZERO,
            frame: [Vec3::X, Vec3::Y, Vec3::Z],
            last_velocity: Vec3::ZERO,
            focus: Vec3::ZERO,
            turn_dot: 1.0,
            landed_time: 0.0,
            big_air: false,
            zoom: 1.0,
            above_eased: 0.0,
            lean: 0.0,
            air_tilt: 0.0,
            air_blend: 0.0,
            upside_blend: 0.0,
            last_up: Vec3::Y,
            lean_out: 0.0,
            air_timer: 0.0,
        };
        c.set_mode(s, 2, 0.0);
        c
    }

    /// Retail `820CFB30` (`ToggleSkaterCamMode` / `SetSkaterCamMode` land
    /// here): load mode `index`, easing behind / above over `time` seconds
    /// (820CFFE4). Not translated: 820D0048 (split vertical halves the fov),
    /// 820D0068 (822EFC08, the camera's own fov update: p8-game sets the fov),
    /// 820D0074 (the look-around yaw per mode, +488 +40: 0 in modes 0..4).
    pub fn set_mode(&mut self, s: &Scripts, index: usize, time: f32) {
        let Some(m) = Mode::load(s, index, &self.mode) else { return };
        self.mode = m;
        self.mode_index = index;
        if time > 0.0 {
            // 820CFFE8: step = (target - now) * (1/60) / time (8200284C + 4).
            let k = (1.0 / 60.0) / time;
            self.behind_time = time;
            self.above_time = time;
            self.behind_step = (m.behind - self.behind) * k;
            self.above_step = (m.above - self.above) * k;
        } else {
            self.behind = m.behind;
            self.above = m.above;
            self.behind_time = 0.0;
            self.above_time = 0.0;
        }
    }

    /// Retail `820D05A8`: ease behind / above one step (1/60 s a call,
    /// 82002850).
    fn advance_mode_lerp(&mut self) {
        let f = 1.0 / 60.0;
        if self.behind_time > 0.0 {
            self.behind_time -= f;
            self.behind += self.behind_step;
        } else {
            self.behind_time = 0.0;
        }
        if self.above_time > 0.0 {
            self.above_time -= f;
            self.above += self.above_step;
        } else {
            self.above_time = 0.0;
        }
    }

    /// Retail `820D0128`: the vert camera (look down from above): lip, or
    /// vert air without the break window or a spine transfer. Translated:
    /// 820D0148 (in bail), 820D0160 (lip), 820D0174, 820D0180, 820D018C (vert
    /// air). Not translated: 820D0130 (no skater), 820D0154 (walking, +28),
    /// and +288 at 820D0190 (UNKNOWN, taken as clear).
    fn vert_camera(p: &CorePhysics) -> bool {
        if p.in_bail {
            return false;
        }
        p.state == State::Lip || (p.vert.in_vert_air && !p.vert.break_window && !p.transfer.active)
    }

    fn in_vert_air(p: &CorePhysics) -> bool {
        // The test repeated through 820D1238: +56 && !+80 && !+136.
        p.vert.in_vert_air && !p.vert.break_window && !p.transfer.active
    }

    /// Retail `820D0D90`: the point on the skater the camera follows. Retail
    /// takes the object position with its height (along the object's up)
    /// from skeleton bone 0; that bone is the root at the object origin, so
    /// the object position is used (APPROXIMATE: animation moving the root
    /// is ignored). The bail blend towards bone 1 (`+432`) is not translated.
    fn skater_point(p: &CorePhysics) -> Vec3 {
        p.body.position
    }

    /// Retail `820D0F48`: ease the focus point (`+48`) towards the skater.
    /// 820D0F78 (no skater attached: returns (0, 0, 0)): not translated, the
    /// camera always has one.
    fn update_focus(&mut self, p: &CorePhysics, dt: f32, instant: bool) -> Vec3 {
        let point = Self::skater_point(p);
        // 820D0FF4, 820D1004: +284/+288/+292, a timed boost of the lerps
        // started elsewhere (not found): not translated; idle, +292 = 1.
        let boost = 1.0;
        // 820D1030, 820D103C, 820D1048: the vert air test.
        let (kxz, ky) = if Self::in_vert_air(p) {
            (
                blend_for_dt(self.mode.vert_air_lerp_xz * boost, dt),
                blend_for_dt(self.mode.vert_air_lerp_y * boost, dt),
            )
        } else {
            // 820D10BC (SkaterState +176, ground step snapped: ky = 1) and
            // 820D10CC (+184: snap): not translated, taken as clear.
            (blend_for_dt(self.mode.lerp_xz * boost, dt), blend_for_dt(self.mode.lerp_y * boost, dt))
        };
        // 820D10E0..820D10F4: focus mode (core +1641): not translated.
        // 820D111C: snap when instant.
        if instant {
            self.focus = point;
        } else {
            self.focus.x += (point.x - self.focus.x) * kxz;
            self.focus.z += (point.z - self.focus.z) * kxz;
            self.focus.y += (point.y - self.focus.y) * ky;
        }
        self.focus
    }

    /// Retail `820D02A8`: (above, distance) with the zooms.
    /// Translated: 820D02DC..820D0354 (big-air latch), 820D0360, 820D0390
    /// (lip zoom), 820D0440..820D0460 (zoom lerp), 820D04C0 (lip above),
    /// 820D04CC..820D0554 (balance-trick above; state 4 never happens here),
    /// 820D0580..820D0594 (above lerp).
    /// Not translated: 820D02D0 (no skater), 820D0374, 820D0380 (grind
    /// zoom, state 4), 820D03A4..820D0440 (focus mode and the look-around
    /// zoom), 820D0478 (look-around pitch scale), 820D055C..820D057C (focus
    /// mode above).
    fn zoom_and_above(&mut self, p: &CorePhysics, zoom_lerp: f32) -> (f32, f32) {
        let vert = Self::in_vert_air(p);
        if !self.big_air && vert && p.doing_trick {
            self.big_air = true;
        } else if !vert {
            self.big_air = false;
        }
        let zoom = if self.big_air {
            self.mode.big_air_trick_zoom
        } else if p.state == State::Lip {
            self.mode.lip_trick_zoom
        } else {
            1.0 // grind (state 4) not translated
        };
        self.zoom += (zoom - self.zoom) * zoom_lerp;
        let distance = self.behind * self.zoom;
        // The look-around pitch (+488 +32 < 0) is not translated (no right
        // stick look-around), so no distance scale.
        let k = p.balance.kind;
        let above = if p.state == State::Lip {
            self.mode.lip_trick_above
        } else if p.balance.doing
            && [qb_key("Flatland"), qb_key("NoseManual"), qb_key("Grind"), qb_key("Slide"), qb_key("Manual")]
                .contains(&k)
        {
            self.mode.balance_trick_above
        } else {
            self.above
        };
        self.above_eased += (above - self.above_eased) * zoom_lerp;
        (self.above_eased, distance)
    }

    /// Retail `820D1238`, for skater states ground, air and vert air.
    ///
    /// Not translated (the code takes the plain path instead): walking and
    /// the walk camera, grinds (state 4: grind-start slerp, lean, zoom),
    /// wallrides (state 2), the lip camera turn (`820D0780`), the bail /
    /// ragdoll camera (`820D1648..820D1AA4`), the right-stick look-around
    /// (`cameralookaround`), focus mode, nail-the-trick, camera shake
    /// (`820D01A8`) and camera collision (`820BBF58`).
    pub fn update(&mut self, s: &Scripts, p: &CorePhysics, dt: f32) {
        // 820D12F8: snapping frames (the teleport flag, object +308 bit 3,
        // is not translated).
        let mut instant = false;
        if self.instant_frames > 0 {
            self.instant_frames -= 1;
            instant = true;
        }
        // 820D135C: the camera object takes the eased orientation; its at
        // row is kept (576(r1)) for the turn limiter.
        let old_at = self.orient[2];
        self.advance_mode_lerp();
        let air = p.state == State::Air;
        // 820D1B14: the skater's display matrix (core +32; object +224 set).
        let mut m: Rows = rows_of(&p.matrix_32);
        self.frame = m;
        // 820D1E38..820D1EC4: the grind lean eases back to 0.
        if instant {
            self.lean = 0.0;
        } else {
            self.lean += (0.0 - self.lean) * self.mode.grind_lerp;
        }
        // 820D1EC8: the up used for the offset: SkaterState +304 on the
        // ground (only written by the reset 820DFFA8, to (0, 1, 0)), else
        // the display up.
        let up_for_offset = if p.state == State::Ground { Vec3::Y } else { m[1] };
        let v = p.body.velocity;
        let flat_speed = Vec3::new(v.x, 0.0, v.z).length();
        // 820D200C: stopped in the air (not in a spine transfer).
        let stopped_air = flat_speed < 0.0025 && air && !p.transfer.active;
        let vert_cam = Self::vert_camera(p);
        // 820D2050..820D209C: +484.
        if vert_cam || up_for_offset.y < 0.2 {
            self.air_timer = 2.0;
        }
        if self.air_timer > 0.0 && air {
            self.air_timer -= dt;
        } else {
            self.air_timer = 0.0;
        }
        // 820D2124: moving faster than 10 * dt (m/s).
        let moving = flat_speed > dt * 10.0;
        // 820D2138: skaterphysicscontrol +24 (UNKNOWN) taken as clear.
        if moving || vert_cam || stopped_air || p.transfer.active {
            self.follow_frame(p, &mut m, vert_cam, stopped_air, air);
        }
        // 820D29F0: the extra air tilt, 0.0116357 rad a 60 Hz frame up to
        // 0.34907 (20 degrees) in the air, back at 0.0465427 on the ground.
        let step = dt * 60.0;
        if air {
            self.air_tilt = (self.air_tilt + step * 0.011_635_7).min(0.349_07);
        } else if self.air_tilt > 0.0 {
            self.air_tilt = (self.air_tilt - step * 0.046_542_7).max(0.0);
        } else if self.air_tilt < 0.0 {
            self.air_tilt = (self.air_tilt + step * 0.046_542_7).min(0.0);
        }
        // 820D2AC4: yaw by the mode angle (+488 +40, 0 for one player) and
        // the look-around (+36, not translated): no turn.
        rotate_y(&mut m, 0.0);
        // 820D2D1C: pitch by tilt + air tilt, except in vert air.
        if !Self::in_vert_air(p) {
            rotate_x(&mut m, self.air_tilt + self.mode.tilt);
        }
        // Look-around pitch (+488 +32): not translated (0).
        // 820D31B4: the orientation to reach: [-right, up, -at].
        let goal: Rows = [-m[0], m[1], -m[2]];
        self.turn_towards(&goal, old_at, p, dt, instant);
        // 820D36C4: position.
        let focus = self.update_focus(p, dt, instant);
        let zoom_lerp = if instant { 1.0 } else { self.mode.zoom_lerp };
        let (above, distance) = self.zoom_and_above(p, zoom_lerp);
        let mut position = focus + self.orient[2] * distance;
        // 820D3754: +404 (air blend) and +408 (upside down).
        if p.vert.in_vert_air {
            self.air_blend += 0.1;
        } else if self.air_timer > 0.0 {
            self.air_blend += 0.2;
        } else {
            self.air_blend -= 0.1;
        }
        self.air_blend = self.air_blend.clamp(0.0, 1.0);
        // 820D37C4: object +164 = the object's up.y.
        if p.body.up().y >= 0.0 {
            self.upside_blend = (self.upside_blend - 0.25).max(0.0);
        } else {
            self.upside_blend = (self.upside_blend + 0.25).min(1.0);
        }
        let b = (self.upside_blend + self.air_blend).min(1.0);
        // 820D3824: offset = lerp(world up, the up above) * above.
        let world = Vec3::Y * above;
        let offset = world + (up_for_offset * above - world) * b;
        self.last_up = up_for_offset;
        // 820D3A78: the camera goes up by offset * above (above twice, as
        // retail does), the target by the offset.
        position += offset * above;
        let target = Self::skater_point(p) + offset;
        // 820D3AF4: +436 (a drop after a bail, set elsewhere): not translated.
        // 820D3C00: look at the target.
        let at = safe_normalize(target - position);
        let right = safe_normalize(self.orient[1].cross(at));
        let up = safe_normalize(at.cross(right));
        let mut look: Rows = [right, up, at];
        // 820D3E00: level the horizon (less in vert air).
        let lean_time = s.physics_float("skater_cam_lean_out_transition_time", p.on_bike);
        if !p.vert.in_vert_air {
            self.lean_out = lean_time;
        }
        if self.lean_out > 0.0 {
            let k = if lean_time != 0.0 { (self.lean_out / lean_time).clamp(0.0, 1.0) } else { 0.0 };
            let a = angle_deg(look[0], Vec3::Y);
            // 82000DF4 = 90, 82002950 = -0.0174533.
            rotate_z(&mut look, (1.0 - self.air_blend) * ((90.0 - a) * k) * -0.017_453_3);
            self.lean_out -= dt;
        }
        // 820D3EE0: the grind lean roll (+340) is 0 here.
        // 820D43DC: the camera rows.
        self.matrix = [-look[0], look[1], -look[2]];
        self.position = position;
        self.target = target;
    }

    /// `820D21C8..820D2980`: the follow frame.
    fn follow_frame(&mut self, p: &CorePhysics, m: &mut Rows, vert_cam: bool, stopped_air: bool, air: bool) {
        // 820D21D0: the display matrix must be a rotation (|det - 1| <= 0.01)
        // for the stopped test.
        let det = Mat3::from_cols(m[0], m[1], m[2]).determinant();
        let keep = (det - 1.0).abs() <= 0.01 && p.body.velocity.length() <= 0.01 && air;
        let (mut up, at);
        if vert_cam {
            // 820D22E0: straight down.
            let down = Vec3::new(0.0, -1.0, 0.0);
            let right = safe_normalize(m[1].cross(down));
            up = if right.length_squared() < 0.1 { Vec3::new(0.0, 0.0, 1.0) } else { m[1] };
            at = down;
        } else if keep {
            // 820D24B4: keep the display matrix.
            up = m[1];
            at = m[2];
        } else {
            let mut dir = if stopped_air {
                self.last_velocity
            } else {
                let v = p.body.velocity;
                if !p.transfer.active {
                    self.last_velocity = v;
                }
                v
            };
            // 820D251C: in a spine transfer only the vertical part and the
            // carry (SkaterState +272).
            if p.transfer.active {
                dir = Vec3::new(0.0, dir.y, 0.0) + p.transfer.carry;
            }
            let dir = safe_normalize(dir);
            let (w, k);
            if air {
                // 820D25DC: 0.7 of the vertical part kept going down in a
                // transfer (82001FA0), else flat.
                w = Vec3::Y;
                k = if p.transfer.active && dir.y < 0.0 { dir.dot(w) * 0.7 } else { dir.y };
                up = Vec3::Y;
            } else {
                // 820D27B0: SkaterState +320 (only set to (0, 1, 0) by the
                // reset 820DFFA8); 0.8 of the normal part (82002964).
                w = Vec3::Y;
                k = dir.dot(w) * 0.8;
                up = m[1];
            }
            at = safe_normalize(dir - w * k);
        }
        // 820D2868: make the frame orthonormal.
        let right = safe_normalize(up.cross(at));
        up = safe_normalize(at.cross(right));
        *m = [right, up, at];
        // 820D2948: kept for the next frame (+128).
        self.frame = *m;
    }

    /// `820D31B4..820D3688`: turn the camera's orientation towards `goal`.
    fn turn_towards(&mut self, goal: &Rows, old_at: Vec3, p: &CorePhysics, dt: f32, instant: bool) {
        if instant {
            self.orient = *goal;
        }
        let d = [0, 1, 2].map(|i| self.orient[i].dot(goal[i]));
        // 82002958 = 0.9999
        if d.iter().all(|&x| x > 0.9999) {
            return;
        }
        let from = self.orient;
        if Self::in_vert_air(p) {
            self.orient = slerp_rows(&from, goal, blend_for_dt(self.mode.vert_air_slerp, dt));
            self.landed_time = 1.0 / 6.0; // 8200285C
            return;
        }
        // Wallride slerp (state 2): not translated.
        if p.state == State::Ground && self.landed_time >= dt {
            self.landed_time -= dt;
            self.orient = slerp_rows(&from, goal, blend_for_dt(self.mode.vert_air_landed_slerp, dt));
            return;
        }
        self.landed_time = 0.0;
        self.orient = slerp_rows(&from, goal, blend_for_dt(self.mode.slerp, dt));
        // 820D35A4: limit how fast the turn may grow (0.9998, 82002954).
        let dot = self.orient[2].dot(old_at);
        let limit = self.turn_dot * 0.9998;
        if dot < limit {
            let t = blend_for_dt(dot / limit * self.mode.slerp, dt);
            self.orient = slerp_rows(&from, goal, t);
            self.turn_dot = limit;
        } else {
            self.turn_dot = dot;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripts with one camera mode (made-up values, not the game's).
    fn scripts() -> Scripts {
        let mode = |b: f32| {
            Value::Struct(vec![
                (qb_key("behind"), Value::Float(b)),
                (qb_key("above"), Value::Float(1.0)),
                (qb_key("tilt"), Value::Float(0.2)),
                (qb_key("slerp"), Value::Float(0.1)),
                (qb_key("lerp_xz"), Value::Float(0.2)),
                (qb_key("lerp_y"), Value::Float(0.5)),
                (qb_key("zoom_lerp"), Value::Float(0.1)),
            ])
        };
        let mut g = std::collections::BTreeMap::new();
        g.insert(qb_key("cam_a"), mode(3.0));
        g.insert(
            qb_key("Skater_Camera_Array"),
            Value::Array(vec![Value::Checksum(qb_key("cam_a")), Value::Checksum(qb_key("cam_a")), Value::Checksum(qb_key("cam_a"))]),
        );
        g.insert(qb_key("skater_cam_lean_out_transition_time"), Value::Float(1.0));
        Scripts::new(g)
    }

    #[test]
    fn follows_behind_and_above_the_travel_direction() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        let mut c = SkaterCamera::new(&s);
        for _ in 0..600 {
            p.body.position += p.body.velocity / 60.0;
            c.update(&s, &p, 1.0 / 60.0);
        }
        let d = c.position - p.body.position;
        // Behind (-z), no sideways drift, above; looking at the skater + 1.
        assert!(d.z < -2.0 && d.x.abs() < 1e-3 && d.y > 1.0, "{d:?}");
        assert!((c.target - (p.body.position + Vec3::Y)).length() < 1e-4);
        // The camera's third row points back towards the camera.
        assert!(c.matrix[2].z < -0.9, "{:?}", c.matrix);
        // Steady state: the focus lags by v * (1 - k) / k at 60 Hz, k = 0.2.
        let lag = 5.0 / 60.0 * 0.8 / 0.2;
        let tilt_back = 3.0 * 0.2f32.cos();
        assert!((-d.z - (tilt_back + lag)).abs() < 0.05, "{d:?}");
    }

    #[test]
    fn turns_to_the_new_direction_without_overshoot() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, 5.0);
        let mut c = SkaterCamera::new(&s);
        for _ in 0..300 {
            c.update(&s, &p, 1.0 / 60.0);
        }
        p.body.velocity = Vec3::new(5.0, 0.0, 0.0);
        let mut last = f32::MAX;
        for _ in 0..600 {
            c.update(&s, &p, 1.0 / 60.0);
            let back = c.orient[2];
            // Angle from straight behind the new direction (-x).
            let a = back.dot(Vec3::new(-1.0, 0.0, 0.0)).clamp(-1.0, 1.0).acos();
            assert!(a <= last + 1e-4, "turned away: {a} after {last}");
            last = a;
        }
        assert!(last < 0.25, "{last}"); // what is left is the 0.2 tilt
    }

    #[test]
    fn blend_is_the_factor_at_60_hz() {
        assert!((blend_for_dt(0.04, 1.0 / 60.0) - 0.04).abs() < 1e-6);
        // Twice the frame time: 2s / (1 + s).
        assert!((blend_for_dt(0.5, 2.0 / 60.0) - 1.0 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn rotations_match_the_retail_builders() {
        // 821E9168: row0 = (c, 0, -s), row2 = (s, 0, c) times the rows.
        let mut m: Rows = [Vec3::X, Vec3::Y, Vec3::Z];
        rotate_y(&mut m, 0.3);
        assert!((m[0] - Vec3::new(0.3f32.cos(), 0.0, -0.3f32.sin())).length() < 1e-6);
        // 821E8EE8: pitching the at row up moves it towards -up.
        let mut m: Rows = [Vec3::X, Vec3::Y, Vec3::Z];
        rotate_x(&mut m, 0.2);
        assert!((m[2] - Vec3::new(0.0, -0.2f32.sin(), 0.2f32.cos())).length() < 1e-6);
        let mut m: Rows = [Vec3::X, Vec3::Y, Vec3::Z];
        rotate_z(&mut m, 0.2);
        assert!((m[0] - Vec3::new(0.2f32.cos(), 0.2f32.sin(), 0.0)).length() < 1e-6);
    }
}
