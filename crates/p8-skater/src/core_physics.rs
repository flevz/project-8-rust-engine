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
use p8_formats::qb::Value;
use p8_formats::qb_key;
use p8_script::Params;
use crate::transfer::ScriptAction;

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
    /// Retail event "BailCollision" (hit a wall while bailing).
    BailCollision,
    /// Retail broadcast "SkaterOffEdge" and event "GroundGone": no ground
    /// under the skater (`820F12C0`).
    SkaterOffEdge,
    GroundGone,
    /// Retail event "WallPush" (`820DB418`).
    WallPush,
    /// Retail events "FlailLeft"/"FlailRight": hit a wall fast (`820E5F40`).
    FlailLeft,
    FlailRight,
    /// Retail events "OffMeterTop"/"OffMeterBottom": the balance meter
    /// tipped over (`82190F58`).
    OffMeterTop,
    OffMeterBottom,
}

/// SkaterState `+24` (set by `SetState`, `820D71B0`). Only the states with
/// translated updates are listed; retail has ten (0..9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// 0: ground update `820F6978`.
    Ground,
    /// 1: air update `820F2310`.
    Air,
    /// 3: lip update `820F49D8` (a lip trick on a coping rail).
    Lip,
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
    /// `+2084`: seconds left; -1 = `notimelimit`, -2 = `CurrentLevel`
    /// (only `levelend` ends it). Counted down by `820E0188`.
    pub timer: f32,
    /// `+2088`: speed above which the heavy air friction applies ("Max").
    pub max: f32,
    /// `+2092`: speed cap ("max_max", default "Max").
    pub max_max: f32,
    /// `+2096`: air friction ("friction", default 2e-6).
    pub friction: f32,
    /// `+2100`: gravity while moving up ("gravity", default
    /// `Physics_Ground_Gravity`).
    pub gravity: f32,
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
    /// `+1940`: the last ground turn ("Left" / "Right"), cleared each update.
    pub last_turn: Option<Turn>,
    /// `+2217`: the last rotation (air spin `820E9DF8`, ground turn
    /// `820ED548`) went the positive way ("Left"). Kept until the next
    /// rotation; read by `LastSpinWas` (`820D5908`), which picks the revert's
    /// direction. Also written by the bike turn (`820E7A20`) and the grind
    /// update (`820DBD40`), not translated.
    pub last_spin_positive: bool,
    /// Object `+44`: the object's flag bits (`Obj_SetFlag` / `Obj_ClearFlag`
    /// / `Obj_FlagSet`, `821C8510`); bit n is the flag global whose value is
    /// n (e.g. `FLAG_SKATER_REVERTFS` = 1). Kept on the physics for now
    /// (the object is not modelled separately).
    pub object_flags: u32,
    /// `+298`: terrain index under the board (from the last ground or
    /// landing hit; entry of the scripts' `terrain_types`).
    pub terrain: u8,
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
    /// SkaterState `+48`: toggled with every turn-around (the backwards
    /// flip `820DBAA8` via `820D45F0`, and `FlipAndRotate` `820D9008`). The
    /// skaterflip node compares it with its value when built (`820B0E20`,
    /// not translated).
    pub rotated: bool,
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
    /// SkaterState `+264`: doing a trick (`DoingTrick`; set when a trick
    /// runs, `82122E10`, cleared by each `DoNextTrick`, `821230D8`).
    pub doing_trick: bool,
    /// `+1524`: the last anim data (`SetLastAnimData`, `820D6DE8`; read by
    /// `GetLastAnimData`, `820D6E50`): a checksum the manual script makes
    /// from its anim data's `string`.
    pub last_anim_data: u32,
    /// flipandrotate `+24`: the board is rotated (toggled by every flip,
    /// `820FD8E8` -> `820FD838`; the board model's turn is not
    /// translated).
    pub board_rotated: bool,
    /// flipandrotate `+25` / `+26` / `+27`: `FlipAfter`, `RotateAfter`,
    /// `BoardRotateAfter` pending.
    pub flip_after: bool,
    pub rotate_after: bool,
    pub board_rotate_after: bool,
    /// skatermatrixqueries `+32..+80`: the display matrix as it was when
    /// that component updated (before the core physics, `82108848`).
    pub queries_matrix: Mat3,
    /// Trick component `+5360`: degrees spun this air (for trick names).
    pub spin_degrees: f32,
    /// The input of the current frame (retail reads the Input component).
    pub last_input: InputState,
    /// `+2724`: air gravity multiplier (`AdjustGravity`); 0 = unused.
    pub gravity_multiplier: f32,
    /// SkaterState `+128`: in a bail (`IsInBail`).
    pub in_bail: bool,
    /// `+1936`: the vertical velocity after the last air frame's gravity
    /// (`GetLastInAirVerticalVelocity`, `820D6330`).
    pub last_in_air_vy: f32,
    /// Object `+128`: the position at the start of this frame (LIKELY: the
    /// object update stores it before the physics runs).
    pub old_position: Vec3,
    /// SkaterState `+40`: flipped (the stance the animations are mirrored
    /// for). Set to the profile's goofy flag at a restart (`820DFB88`,
    /// `Obj_MoveToNode` without `NoReset`); toggled with every turn-around
    /// (`820FD8E8`). Also picks which way a wall flail goes.
    pub flipped: bool,
    /// Skater profile `+36`: goofy (the profile's `stance` is `goofy`,
    /// `821989D0`).
    pub goofy: bool,
    /// SkaterState `+216`: when set, a wall push only clears it. UNKNOWN
    /// meaning; nothing translated sets it.
    pub state_216: bool,
    /// `+2532`: game time of the last wall push.
    pub last_wallpush_ms: i64,
    /// `+2129`: `BailOn` / `BailOff` (820D5550 / 820D5568).
    pub bail_on: bool,
    /// The skater's scripts are running and handle the events (see
    /// `skater.rs`); without them, [`CorePhysics::step`] stands in for the
    /// ollie handlers as before.
    pub scripted: bool,
    /// `+2176`: game time the skater last entered the air (`SetState`,
    /// 820D7430); air time is now minus this (`820D4CA8`).
    pub air_start_ms: i64,
    /// Script state: the "Ollied" exception runs `ollie` while in the air.
    /// Script `groundgone` sets it after rolling off an edge and
    /// `WaitAnimWhilstCheckingLateOllie` clears it once
    /// `AirTimeGreaterThan skater_late_jump_slop` (333 ms); the `ollie`
    /// script's `InAirExceptions` also clears it (the air table has no
    /// "Ollied").
    pub late_ollie: bool,
    /// `+2544`: the last `Jump` had `BonelessHeight` or `NoComply`
    /// (`LastWasJumpBoneless`, `820D5BF0`).
    pub last_jump_boneless: bool,
    pub stats: StatLevels,
    pub stat_context: StatContext,
    /// Vert state (see `vert.rs`).
    pub vert: Vert,
    /// `+1192`: the rail record being ridden (a lip's coping).
    pub rail: Option<usize>,
    /// `+1200`, `+1208`: when the last rail was left and how long it may
    /// not be taken again. Only grind code (not translated) sets them.
    pub rail_left_ms: i64,
    pub rail_again_ms: i64,
    /// SkaterState `+88`: the rail found is a different one.
    pub new_rail: bool,
    /// `+1248`: where the skater was when the lip started (restored when the
    /// lip state ends, `820D71B0`); zero when not on a lip.
    pub lip_pos: Vec3,
    /// `+2528`: the last wallplant (wallplants are not translated; the
    /// default is "long ago", as retail's clock is far past its reset 0).
    pub last_wallplant_ms: i64,
    /// `+2216`: `NoRailTricks` (`AllowRailTricks` clears it): no rails.
    pub no_rail_tricks: bool,
    /// `+2137`: `AllowLipNoGrind` (`ClearAllowLipNoGrind` clears it): a
    /// rail grab becomes a lip whatever the angles.
    pub allow_lip_no_grind: bool,
    /// The balance component (`+2844`).
    pub balance: crate::balance::Balance,
    /// The trick component (`trick`), see [`crate::trick`].
    pub tricks: crate::trick::Tricks,
    /// `+1908`, `+1909`: which way off the balance meter is safe (the
    /// meter's colours), set by `820E5988` ([`Self::update_balance_sides`]).
    pub balance_sides: [bool; 2],
    /// The "vibration" component (controller rumble), kept here so script
    /// commands can reach it.
    pub vibration: crate::vibration::Vibration,
    /// A script the physics starts on the skater's script now (retail does
    /// the goto and a script update in place, e.g. `LipTrick` from
    /// `820F44C0`); `skater.rs` does it right after the physics step.
    pub script_goto: Option<u32>,
    /// Spine transfers and acid drops (`transfer.rs`).
    pub transfer: crate::transfer::Transfer,
    /// `+2540`: game time of the last `Jump` (820F0EA8).
    pub jump_ms: i64,
    /// `+2084`..`+2100` as last written. `override_limits` is these while
    /// `+2084` is not 0; the rest stay in memory when it is 0, and
    /// `820DAFF0` reuses them. Before any `OverrideLimits`, friction
    /// `+2096` and gravity `+2100` are UNKNOWN: no writer other than
    /// `820D5C68` was found, so they start at 0 (INFERRED zero-filled).
    pub override_mem: OverrideLimits,
    /// State of the stand-in for retail's random numbers (`821E8508`; its
    /// generator is not read).
    pub rng: u32,
}

/// The vert bookkeeping, SkaterState (`+2848`) flags and physics fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vert {
    /// SkaterState `+72`: the ground under the board has flag 0x8 (vert).
    /// Set by the ground snap from the hit (`+1185`), cleared each air frame.
    pub on_vert_ground: bool,
    /// SkaterState `+56`: in vert air (`InVertAir`).
    pub in_vert_air: bool,
    /// SkaterState `+64`: following the vert wall below while in vert air.
    pub tracking: bool,
    /// SkaterState `+80` and its time stamp `+84`: the break-vert window
    /// after a vert takeoff.
    pub break_window: bool,
    pub break_window_ms: i64,
    /// SkaterState `+144`: set when the air leveling found ground below or
    /// vert was broken; lets the air leveling run in vert air.
    pub over_ground: bool,
    /// `+1264`: the vert wall point followed in the air.
    pub point: Vec3,
    /// `+1280`: the vert wall's flat normal.
    pub normal: Vec3,
    /// `+1312`: how far above `point` to look for the wall first.
    pub lift: f32,
    /// `+2131`: `LandedFromVert` (set by a landing from vert air or
    /// `SetLandedFromVert`; `ResetLandedFromVert` clears it).
    pub landed_from_vert: bool,
    /// `+2135`: the last landing was from vert air (set or cleared by every
    /// landing).
    pub landing_from_vert: bool,
    /// `+2136`: `WasLastLandingVert`; only the scripts set it
    /// (`SetLastLandingVert` / `SetLastLandingGround`).
    pub last_landing_vert: bool,
    /// `+96`: the normal eased toward `+112` by `820DA1A8`, `+128` where it
    /// eases from and `+160` how much of the way is left (1 to 0).
    pub eased_normal: Vec3,
    pub ease_from: Vec3,
    pub ease_left: f32,
    /// SkaterState `+120`: the vert auto-turn is running (set by the vert
    /// takeoff; cleared by `NoSpin`, a transfer, the auto-turn finishing,
    /// or a held spin input).
    pub auto_turn: bool,
    /// `+1296`: the facing the auto-turn turns to (the at row at takeoff
    /// with y negated).
    pub auto_turn_dir: Vec3,
}

/// Retail `8262BE78`, cosine (its neighbour `8262BDA0` is sine: CONFIRMED by
/// its series coefficients; together they build rotation matrices).
pub(crate) fn retail_cos(x: f64) -> f32 {
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
pub(crate) fn signed_angle(a: Vec3, b: Vec3, axis: Vec3) -> f32 {
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
            last_spin_positive: false,
            object_flags: 0,
            terrain: 0,
            crouched: false,
            crouch_changed_ms: 0,
            crouch_duration_ms: 0,
            state: State::Ground,
            time_ms: 0,
            time_frac_ms: 0.0,
            jump_start: Vec3::ZERO,
            gravity_multiplier: 0.0,
            last_input: InputState::default(),
            rotated: false,
            no_spin: false,
            turning_enabled: true,
            analog_turning: true,
            nollie: false,
            lean_degrees: 0.0,
            previous_lean_degrees: 0.0,
            flipping: false,
            spin_degrees: 0.0,
            in_bail: false,
            last_in_air_vy: 0.0,
            old_position: Vec3::ZERO,
            flipped: false,
            goofy: false,
            doing_trick: false,
            last_anim_data: 0,
            board_rotated: false,
            flip_after: false,
            rotate_after: false,
            board_rotate_after: false,
            queries_matrix: Mat3::IDENTITY,
            state_216: false,
            last_wallpush_ms: i64::MIN / 2,
            air_start_ms: 0,
            bail_on: false,
            scripted: false,
            late_ollie: false,
            last_jump_boneless: false,
            stats: StatLevels::with_default(if default > 0.0 { default } else { 5.0 }),
            stat_context: StatContext::default(),
            // Reset (820D4700) sets +96 and +128 like +112: straight up.
            vert: Vert { eased_normal: Vec3::Y, ease_from: Vec3::Y, ..Vert::default() },
            rail: None,
            rail_left_ms: 0,
            rail_again_ms: 0,
            new_rail: false,
            lip_pos: Vec3::ZERO,
            last_wallplant_ms: i64::MIN / 4,
            no_rail_tricks: false,
            allow_lip_no_grind: false,
            balance: Default::default(),
            tricks: Default::default(),
            balance_sides: [false; 2],
            vibration: Default::default(),
            script_goto: None,
            transfer: Default::default(),
            jump_ms: 0,
            override_mem: OverrideLimits { timer: 0.0, max: 0.0, max_max: 0.0, friction: 0.0, gravity: 0.0 },
            rng: 0x1234_5678,
        }
    }

    /// A fresh skater standing at a restart node: `Obj_MoveToNode` with
    /// `orient` (`82266148` -> `822E0A88`) sets the matrix to identity then
    /// rotates it by the node's `Angles` about X, Y and Z. Only the Y turn
    /// (`820D0780`, same rotation as [`rotate_about_up`] on identity) is
    /// translated; X and Z are 0 in the restarts used so far.
    ///
    /// The skater's stance: `820DFB88` sets flipped (SkaterState `+40`) to
    /// the profile's goofy flag.
    pub fn at_restart(scripts: &Scripts, pos: Vec3, angles: Vec3) -> Self {
        let mut p = Self::new(scripts);
        p.goofy = profile_is_goofy(scripts, PLAYER_SKATER);
        p.flipped = p.goofy;
        p.stats.levels = profile_stats(scripts, PLAYER_SKATER);
        p.body.position = pos;
        p.old_position = pos;
        rotate_about_up(&mut p.body.matrix, angles.y);
        p.matrix_32 = p.body.matrix;
        p
    }

    fn speed(&self) -> f32 {
        self.body.velocity.length()
    }

    /// The override fields `+2084`..`+2100` as they stand.
    pub(crate) fn override_base(&self) -> OverrideLimits {
        self.override_limits.unwrap_or(OverrideLimits { timer: 0.0, ..self.override_mem })
    }

    /// Write the override fields; a timer (`+2084`) of 0 means off.
    pub(crate) fn set_override(&mut self, o: OverrideLimits) {
        self.override_mem = o;
        self.override_limits = if o.timer == 0.0 { None } else { Some(o) };
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

    /// Retail `820D7570`: the brake input as the animation reads it
    /// (animinfo `+132`). With the stick at rest: Down held without Left or
    /// Right; otherwise the stick pulled back past
    /// `physics_brake_stick_threshold` and held within it sideways.
    pub fn anim_brake_input(&self, s: &Scripts, input: &InputState) -> bool {
        if input.stick_back_raw == 0.0 && input.stick_x_raw == 0.0 {
            return input.down && !input.left && !input.right;
        }
        let t = s.physics_float("Physics_Brake_Stick_Threshold", self.on_bike);
        input.stick_back_raw * 0.0078125 > t && (input.stick_x_raw * 0.0078125).abs() < t
    }

    /// Retail `820E2CD8` (via `820E79D8`): seconds until the skater would
    /// land, stepping the flight in 0.05 s slices under air gravity from
    /// 0.0025 above the object and casting a feeler along each slice; -1 if
    /// nothing upward-facing is hit within `max` seconds.
    pub fn time_to_land(&self, s: &Scripts, world: &dyn crate::world::World, max: f32) -> f32 {
        self.time_to_land_slice(s, world, max).0
    }

    /// [`CorePhysics::time_to_land`] and the 1-based slice the hit fell in
    /// (0 when none), so a caller with a shorter horizon can apply the
    /// retail cut-off: slice k is cast only if k == 1 or (k - 1) x 0.05 <
    /// its horizon.
    pub fn time_to_land_slice(&self, s: &Scripts, world: &dyn crate::world::World, max: f32) -> (f32, u32) {
        let g = Vec3::new(0.0, self.air_gravity(s), 0.0);
        let mut p = self.body.position + Vec3::new(0.0, 0.0025, 0.0);
        let mut v = self.body.velocity;
        let step = 0.05;
        let mut t = step;
        let mut k = 1u32;
        loop {
            let next = p + v * step;
            // The feeler's defaults (8221B0D0 -> 8221AEA8): ignore_1 = 0x10,
            // ignore_0 = 0.
            if let Some(hit) = world.feeler(p, next, 0x10, 0) {
                if hit.normal.y > 0.0 {
                    let len = (next - p).length();
                    let frac = if len > 0.0 { (hit.point - p).length() / len } else { 0.0 };
                    return ((t - step) + frac * step, k);
                }
                return (-1.0, 0);
            }
            if t >= max {
                return (-1.0, 0);
            }
            t += step;
            k += 1;
            p = next;
            v += g * step;
        }
    }

    /// Retail `820D7878`: seconds until the top of the jump, -vy / gravity.
    pub fn time_to_apex(&self, s: &Scripts) -> f32 {
        -1.0 / self.air_gravity(s) * self.body.velocity.y
    }

    /// What the animation tree reads (the animinfo copies, `820B8EA0`).
    /// `world` is needed for the landing prediction.
    pub fn anim_inputs_in(&self, s: &Scripts, world: Option<&dyn crate::world::World>) -> crate::anim_tree::SkaterInputs {
        let mut i = self.anim_inputs(s);
        if self.state == State::Air {
            i.time_to_apex = self.time_to_apex(s);
            // One prediction with a horizon longer than any node's (the
            // scripts' land_blend_time is 0.3); each node applies its own
            // cut-off by slice.
            let (t, k) = world.map_or((-1.0, 0), |w| self.time_to_land_slice(s, w, 2.0));
            i.time_to_land = t;
            i.time_to_land_slice = k;
        }
        i
    }

    /// `820FD8E8` (skaterflipandrotate's flip): toggle the stance flag
    /// (SkaterState `+40`, time `+44` not kept) and the board-rotated flag
    /// (`820FD838`).
    pub fn flip_stance(&mut self) {
        self.flipped = !self.flipped;
        self.board_rotated = !self.board_rotated;
    }

    /// `820D9008` (FlipAndRotate's turn): the object matrix turned round
    /// about up (x and z rows negated), copied to the display matrix, core
    /// `+2024` (not kept here) and SkaterState `+48` toggled.
    pub fn turn_round(&mut self) {
        self.body.matrix.x_axis = -self.body.matrix.x_axis;
        self.body.matrix.z_axis = -self.body.matrix.z_axis;
        self.matrix_32 = self.body.matrix;
        self.rotated = !self.rotated;
    }

    /// `HandleFlipOrBoardRotateAfter` (`820FDCE0` -> `820FDBE0`): do what
    /// `FlipAfter` / `RotateAfter` / `BoardRotateAfter` left pending;
    /// whether anything was.
    pub fn handle_flip_or_rotate_after(&mut self) -> bool {
        let mut any = false;
        if self.flip_after {
            self.flip_stance();
            self.flip_after = false;
            any = true;
        }
        if self.rotate_after {
            self.turn_round();
            self.rotate_after = false;
            any = true;
        }
        if self.board_rotate_after {
            // 820FD838 with the board object: the board-rotated flag.
            self.board_rotated = !self.board_rotated;
            self.board_rotate_after = false;
            any = true;
        }
        any
    }

    /// Riding switch (animinfo `+164`, `820B8220`): flipped, inverted for a
    /// goofy skater.
    pub fn switch(&self) -> bool {
        if self.goofy { !self.flipped } else { self.flipped }
    }

    /// What the animation tree reads (the animinfo copies, `820B8EA0`).
    pub fn anim_inputs(&self, s: &Scripts) -> crate::anim_tree::SkaterInputs {
        let brake_input = self.anim_brake_input(s, &self.last_input);
        let m = self.body.matrix;
        crate::anim_tree::SkaterInputs {
            // SkaterState +40 (animinfo +580).
            flipped: self.flipped,
            rotated: self.rotated,
            balance_lean: self.balance.anim_lean(),
            board_rotated: self.board_rotated,
            grab: self.last_input.circle || self.last_input.kick,
            crouched: self.crouched,
            in_air: self.state == State::Air,
            in_vert_air: self.vert.in_vert_air,
            on_vert_ground: self.vert.on_vert_ground,
            velocity: self.body.velocity.to_array(),
            turn: self.turn_amount,
            brake_input,
            brake_amount: self.brake_amount,
            kick: self.kick_flag && !brake_input,
            right: m.x_axis.to_array(),
            up: m.y_axis.to_array(),
            at: m.z_axis.to_array(),
            // Trick component +5360 (animinfo +532).
            spin: self.spin_degrees,
            switch: self.switch(),
            time_to_land: -1.0,
            time_to_land_slice: 0,
            time_to_apex: 0.0,
        }
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

    /// Retail `820E0188`: the speed limits, run each frame after the
    /// crouch update and before the state update (`820FC990`).
    pub fn speed_limits(&mut self, s: &Scripts) {
        // 820E01AC: skipped in a spine transfer (SkaterState `+136`).
        if self.transfer.active {
            return;
        }
        let mut max_max = self.stat(s, "Skater_Max_Max_Speed_Stat");
        let mut max = self.stat(s, "Skater_Max_Speed_Stat");
        // Retail also reads Skater_Vert_Max_Speed_Time here and ignores it.
        if let Some(o) = self.override_limits.as_mut() {
            // -1 and -2 never count down.
            if o.timer != -1.0 && o.timer != -2.0 {
                o.timer -= self.dt;
                if o.timer < 0.0 {
                    o.timer = 0.0;
                }
            }
            max_max = o.max_max;
            max = o.max;
            if o.timer == 0.0 {
                self.override_limits = None;
            }
        }
        let mut speed = self.speed();
        if speed > max_max {
            // Retail keeps w; the length and direction use x, y and z.
            self.body.velocity = self.body.velocity.normalize() * max_max;
            speed = max_max;
        }
        if speed > max {
            self.air_drag(s.physics_float("physics_heavy_air_friction", false));
        }
        // `+1976`, the time left on a special friction: only the restart
        // resets (820EB690) write it, to 0, so it never counts down here.
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
        let terrain = s.terrain_float_index(self.terrain, "SKATE_ROLL_FRICTION");
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
        self.last_spin_positive = angle > 0.0; // 820ED548
        if !self.lock_velocity_direction {
            // Rotation about world Y. The -1 at 827329F0 is filled at run time
            // (LIKELY -1: required for this to be a rotation).
            let (s, c) = (angle.sin(), angle.cos());
            let v = self.body.velocity;
            self.body.velocity = Vec3::new(c * v.x + s * v.z, v.y, c * v.z - s * v.x);
        }
        self.rotate(angle);
    }

    /// Retail `820DBAA8(landing)`: rolling backwards along the board faster
    /// than `Skater_Flip_Speed` turns the skater around (board forward and
    /// sideways rows negated) and toggles SkaterState `+48`. Retail then
    /// spawns a script (via `822109A0`, INFERRED to run like `822265F8`):
    /// `flip_manualing_backwards` when a balance trick has started (balance
    /// `+28`), else `flip_landing_backwards` (`landing`, the call from the
    /// landing code) or `flip_skating_backwards`. The skating one is
    /// `Skater_PlayOnGroundAnim {no_land=1}`: it rebuilds the ground
    /// animation with the new turned-round flag, so the skater is drawn
    /// facing the way it now rolls. The check of the current manual trick
    /// (balance types Manual / NoseManual / Flatland, `820DBB8C`, which
    /// bails a manual that is not one of six tricks) is not translated.
    /// Returns whether it flipped.
    pub fn flip_if_backwards(&mut self, s: &Scripts, landing: bool) -> bool {
        // Retail also requires not skitching (SkaterState +160).
        if self.on_bike || self.in_bail || self.braking {
            return false;
        }
        let v = self.body.velocity;
        if v.dot(self.body.at()) >= 0.0 {
            return false;
        }
        let flip_speed = s.global_float("Skater_Flip_Speed");
        if v.length_squared() <= flip_speed * flip_speed {
            return false;
        }
        for m in [&mut self.body.matrix, &mut self.matrix_32] {
            m.x_axis = -m.x_axis;
            m.z_axis = -m.z_axis;
        }
        // `flip_backwards_dont_blend` (0 in the retail scripts) skips this:
        // the flip `820FD8E8` (+40) and `820D45F0` (+48).
        if s.global_float("flip_backwards_dont_blend") == 0.0 {
            self.flip_stance();
            self.rotated = !self.rotated;
            let script = if self.balance.doing {
                "flip_manualing_backwards" // 58F45CFE
            } else if landing {
                "flip_landing_backwards" // 2DD785C7
            } else {
                "flip_skating_backwards" // 3A8D6AB2
            };
            self.transfer.actions.push(ScriptAction::Run(qb_key(script), Params::new()));
        }
        true
    }

    /// Retail `820DB318`: point the velocity exactly along the board,
    /// forwards or backwards, keeping the speed.
    pub(crate) fn velocity_along_board(&mut self) {
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
    pub fn ground_update(&mut self, s: &Scripts, input: &InputState, world: &dyn crate::world::World) -> Vec<Event> {
        let mut events = Vec::new();
        // 820F6978 starts by clearing SkaterState +80, +56, +64, +136,
        // +192, +144, +200 and physics +1380 (and +152, +208, +1616:
        // untranslated).
        self.set_break_window(false);
        self.vert.in_vert_air = false;
        self.vert.tracking = false;
        self.set_transfer(false);
        self.set_flag_192(false);
        self.vert.over_ground = false;
        self.set_no_acid_drop(false);
        self.transfer.retry = false;
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

        // 75A0: by the running balance type (balance +24): none -> the
        // drive section; Manual / NoseManual / Flatland -> not braking
        // (+1980), the meter's safe sides (820E5988) and the manual meter
        // (82190F58; the trick component's balance scoring 821248A8 is not
        // translated); Skitch (not translated) and other types -> nothing.
        let kind = self.balance.kind;
        let k = qb_key;
        if kind == 0 {
            if let Some(e) = self.drive(s, input) {
                events.push(e);
            }
        } else if kind == k("Manual") || kind == k("NoseManual") || kind == k("Flatland") {
            self.braking = false;
            self.update_balance_sides(s, world);
            self.run_balance_meter(s, input, &mut events);
        }
        self.friction(s, gravity_cancelled);
        if self.body.velocity.y < 0.0 {
            self.flag_2637 = false;
        }
        // 0.1 and 0.9 are the constants at 82000BF4 and 82002A74.
        if self.speed() < 0.1 && self.ground_normal.y > 0.9 {
            events.push(Event::Stopped);
        }

        // Move, collide with walls and stay on the ground.
        self.ground_move(s, input, world, &mut events);
        if self.state != State::Ground {
            return events;
        }

        if !self.powerslide {
            self.ground_turn(s, input);
        }
        if !self.lock_velocity_direction {
            self.velocity_along_board();
        }
        // `820DBAA8`, unless the velocity direction is locked (+2184).
        if !self.lock_velocity_direction {
            self.flip_if_backwards(s, false);
        }
        // `820D7AB0` runs later in the ground update.
        if self.ollie_trigger(input) {
            // 820F80D0: the vert takeoff runs as soon as the trigger fires.
            self.vert_takeoff(s);
            events.push(Event::Ollied);
        }
        events
    }
}

/// Which entry of `master_skater_list` the player skates as (APPROXIMATE:
/// the player's skater choice is not read yet; the model is `Pro_Hawk`).
pub const PLAYER_SKATER: &str = "hawk";

/// The profile's `stance` is `goofy` (`821989D0` stores that in the skater
/// profile `+36`).
pub fn profile_is_goofy(s: &Scripts, name: &str) -> bool {
    let is = |v: Option<&Value>, n: &str| match v {
        Some(Value::Checksum(k)) => *k == qb_key(n),
        Some(Value::String(t)) => t.eq_ignore_ascii_case(n),
        _ => false,
    };
    match s.global("master_skater_list") {
        Some(Value::Array(list)) => list.iter().any(|e| is(e.get_named("name"), name) && is(e.get_named("stance"), "goofy")),
        _ => false,
    }
}

/// `821984C8` (not career: `is_career` takes the career copy `821A20D8`,
/// not read): each of the profile's stat fields (`8219D540`, integers) into
/// the skater's stat array at `+760` (`82198268`), in `STATS_*` order. Only a
/// split-screen game clamps them to 0..10, so Hawk's 11s stay 11. A missing
/// field leaves the stat unset.
pub fn profile_stats(s: &Scripts, name: &str) -> [Option<f32>; 12] {
    const FIELDS: [&str; 12] = [
        "air", "run", "ollie", "speed", "spin", "flip_speed", "switch", "rail_balance", "lip_balance", "manual_balance", "wall", "special",
    ];
    let mut out = [None; 12];
    let is_name = |v: Option<&Value>| match v {
        Some(Value::Checksum(k)) => *k == qb_key(name),
        Some(Value::String(t)) => t.eq_ignore_ascii_case(name),
        _ => false,
    };
    if let Some(Value::Array(list)) = s.global("master_skater_list")
        && let Some(e) = list.iter().find(|e| is_name(e.get_named("name")))
    {
        for (slot, field) in out.iter_mut().zip(FIELDS) {
            *slot = e.get_named(field).and_then(Value::as_f32);
        }
    }
    out
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
            (k("skater_max_speed_stat"), stat(18.0, 18.0, "STATS_SPEED")),
            (k("skater_max_max_speed_stat"), stat(38.0, 38.0, "STATS_SPEED")),
            f("physics_heavy_air_friction", 0.0004),
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
            (k("physics_recover_rate_stat"), stat(2.0, 2.0, "STATS_SPIN")),
            (k("physics_air_no_rotate_time"), Value::Int(150)),
            (k("physics_air_ramp_rotate_time"), Value::Int(50)),
            (k("physics_air_no_lean_time"), Value::Int(200)),
            (k("physics_air_ramp_lean_time"), Value::Int(200)),
            f("physics_ground_snap_up", 0.33),
            f("physics_ground_snap_down", 0.2),
            f("skater_first_forward_collision_height", 0.2),
            f("skater_first_forward_collision_length", 0.25),
            f("skater_autoturn_vert_angle", 5.0),
            f("skater_autoturn_speed", 3.0),
            f("skater_autoturn_cancel_time", 300.0),
            f("ground_stick_angle", 30.0),
            f("ground_stick_angle_forward", 30.0),
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
                f("Skater_Flip_Speed", 1.0),
                f("Skater_Default_Stats", 5.0),
                f("skater_max_sloped_turn_cosine", 0.5),
                f("default_friction", 0.025),
                f("wall_non_skatable_angle", 25.0),
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
        triangle: false,
        circle: false,
        r2: false,
        up: false,
        brake_digital: false,
        down: false,
        l1: false,
        r1: false,
        l2: false,
        up_held_ms: 0,
        up_released_ms: 0,
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
    fn a_tilted_board_levels_out_in_the_air_at_the_recover_rate() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        // Tipped 30 degrees nose-up off a kicker, 3 m up, flying forward.
        p.body.matrix = Mat3::from_rotation_x(-30f32.to_radians());
        p.body.position = Vec3::new(0.0, 3.0, 0.0);
        p.body.velocity = Vec3::new(0.0, 4.0, 6.0);
        p.state = State::Air;
        let floor = crate::world::FlatFloor::default();
        let tilt = |p: &CorePhysics| p.body.up().y.clamp(-1.0, 1.0).acos().to_degrees();
        p.step(&s, &InputState::default(), &floor);
        // One frame at 2 rad/s: 30 - 1.91 degrees.
        assert!((tilt(&p) - (30.0 - 2f32.to_degrees() / 60.0)).abs() < 0.01, "{}", tilt(&p));
        for _ in 0..20 {
            p.step(&s, &InputState::default(), &floor);
        }
        // Stops once up.y > 0.975 (about 12.8 degrees), within one step.
        assert!(tilt(&p) <= 12.84 && tilt(&p) > 12.84 - 2f32.to_degrees() / 60.0, "{}", tilt(&p));
        assert_eq!(p.matrix_32, p.body.matrix);
    }

    /// An infinite plane through the origin, hit from its front side.
    struct Plane(Vec3);

    impl crate::world::World for Plane {
        fn feeler(&self, a: Vec3, b: Vec3, _: u16, _: u16) -> Option<crate::world::Hit> {
            let (da, db) = (a.dot(self.0), b.dot(self.0));
            if da < 0.0 || db > 0.0 || da <= db {
                return None;
            }
            let point = a + (b - a) * (da / (da - db));
            Some(crate::world::Hit { point, normal: self.0, flags: 0, terrain: 0 })
        }
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
            let world = Plane(p.ground_normal);
            for _ in 0..60 {
                p.step(&s, &input, &world);
            }
            assert_eq!(p.state, State::Ground);
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

    #[test]
    fn landing_more_than_90_degrees_round_turns_the_skater_to_face_travel() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.no_kick = true;
        p.body.velocity = Vec3::new(0.0, 0.0, 8.0);
        let floor = crate::world::FlatFloor::default();
        // Ollie, then turn the board 150 degrees while in the air.
        run(&mut p, &s, CROUCH, 0.1);
        p.step(&s, &InputState::default(), &floor);
        assert_eq!(p.state, State::Air);
        p.rotate(150f32.to_radians());
        let (flipped_before, rotated_before) = (p.flipped, p.rotated);
        let mut landed = false;
        for _ in 0..120 {
            landed |= p.step(&s, &InputState::default(), &floor).contains(&Event::Landed);
            if landed {
                break;
            }
        }
        assert!(landed);
        // Rolling backwards on landing: the board is turned round (150 -> -30
        // degrees from travel) and the stance flag toggled.
        assert_ne!(p.flipped, flipped_before);
        assert_ne!(p.rotated, rotated_before);
        assert!(p.body.velocity.dot(p.body.at()) > 0.0, "facing the way it moves");
        let heading = p.body.at().x.atan2(p.body.at().z).to_degrees();
        assert!((heading + 30.0).abs() < 0.5, "heading {heading}");
    }

    #[test]
    fn slow_backwards_rolling_below_the_flip_speed_is_left_alone() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 0.0, -0.5);
        assert!(!p.flip_if_backwards(&s, false));
        assert!(p.transfer.actions.is_empty());
        p.body.velocity = Vec3::new(0.0, 0.0, -1.5);
        assert!(p.flip_if_backwards(&s, false));
        assert_eq!(p.body.at(), Vec3::NEG_Z);
        assert_eq!(p.body.row0(), Vec3::NEG_X);
    }

    #[test]
    fn the_turn_around_spawns_the_script_that_rebuilds_the_ground_animation() {
        // 820DBAA8: flip_skating_backwards on the ground, flip_landing_backwards
        // from the landing code, flip_manualing_backwards in a balance trick.
        let s = scripts();
        let run = |landing: bool, doing: bool| {
            let mut p = CorePhysics::new(&s);
            p.balance.doing = doing;
            p.body.velocity = Vec3::new(0.0, 0.0, -5.0);
            assert!(p.flip_if_backwards(&s, landing));
            p.transfer.actions.clone()
        };
        let one = |name: &str| vec![ScriptAction::Run(qb_key(name), Params::new())];
        assert_eq!(run(false, false), one("flip_skating_backwards"));
        assert_eq!(run(true, false), one("flip_landing_backwards"));
        assert_eq!(run(false, true), one("flip_manualing_backwards"));
    }

    #[test]
    fn speed_limits_cap_at_max_max_and_slow_above_max() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(0.0, 30.0, 40.0);
        p.speed_limits(&s);
        // Capped to 38, then the heavy friction: 38 - dt * 38² * 0.0004 * 60.
        let expected = 38.0 - (1.0 / 60.0) * 38.0 * 38.0 * 0.0004 * 60.0;
        assert!((p.body.velocity.length() - expected).abs() < 1e-3);
        assert!((p.body.velocity.normalize() - Vec3::new(0.0, 0.6, 0.8)).length() < 1e-5);

        p.body.velocity = Vec3::new(0.0, 0.0, 18.0);
        p.speed_limits(&s);
        assert_eq!(p.body.velocity, Vec3::new(0.0, 0.0, 18.0));
    }

    #[test]
    fn override_limits_count_down_then_end() {
        let s = scripts();
        let mut p = CorePhysics::new(&s);
        p.override_limits =
            Some(OverrideLimits { timer: 0.02, max: 5.0, max_max: 6.0, friction: 2e-6, gravity: -9.8 });
        p.body.velocity = Vec3::new(0.0, 0.0, 10.0);
        p.speed_limits(&s);
        assert!(p.body.velocity.length() < 6.0 && p.override_limits.is_some());
        p.speed_limits(&s);
        // The last frame still uses the override, then it ends.
        assert!(p.override_limits.is_none());
        p.body.velocity = Vec3::new(0.0, 0.0, 10.0);
        p.speed_limits(&s);
        assert_eq!(p.body.velocity, Vec3::new(0.0, 0.0, 10.0));
    }

}
