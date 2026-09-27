//! The skater object: the translated physics plus the player's own skater
//! scripts running on the translated script VM (`p8-script`). Physics
//! events go to the scripts' handlers (retail `82225758` delivers them at
//! once, `82224D78`); script commands act on the physics.
//!
//! Commands are looked up the way retail registers them: a table of
//! (command, component, function) filled at start-up (e.g. `8265D378` for
//! SkaterCorePhysics; the pairs are listed in the research notes). Each
//! translated command names its function. Commands not translated yet are
//! recorded and answer false.
use crate::core_physics::{CorePhysics, Event, OverrideLimits, State};
use crate::input::InputState;
use crate::script::Scripts;
use crate::world::World;
use glam::Vec3;
use p8_formats::qb::Value;
use p8_formats::qb_key;
use p8_script::{Host, Params, Script};

/// The script name of a physics event sent to the skater itself, or
/// `None` for broadcasts (`SkaterJump`, `SkaterOffEdge`), which go to
/// other listeners.
fn event_name(e: Event) -> Option<&'static str> {
    Some(match e {
        Event::Stopped => "Stopped",
        Event::SteepGround => "SteepGround",
        Event::Ollied => "Ollied",
        Event::Landed => "Landed",
        Event::BailCollision => "BailCollision",
        Event::GroundGone => "GroundGone",
        Event::WallPush => "WallPush",
        Event::FlailLeft => "FlailLeft",
        Event::FlailRight => "FlailRight",
        Event::SkaterJump | Event::SkaterOffEdge => return None,
    })
}

pub struct Skater {
    pub physics: CorePhysics,
    pub script: Option<Script>,
    /// Every command met that is not translated yet (checksums).
    pub untranslated: Vec<u32>,
    seed: u32,
}

/// What a running script sees of the skater.
struct Ctx<'a> {
    p: &'a mut CorePhysics,
    s: &'a Scripts,
    input: InputState,
    seed: &'a mut u32,
    events: Vec<Event>,
}

impl Skater {
    /// A skater at a restart point, starting the scripts at `skaterinit`
    /// (which ends by going to `ongroundai` through `switch_ongroundai`).
    pub fn new(s: &Scripts, pos: Vec3, angles: Vec3) -> Self {
        let mut physics = CorePhysics::at_restart(s, pos, angles);
        physics.scripted = true;
        let mut seed = 1;
        let mut ctx = Ctx { p: &mut physics, s, input: InputState::default(), seed: &mut seed, events: Vec::new() };
        let script = Script::new(&mut ctx, qb_key("skaterinit"), &Params::new());
        let mut me = Skater { physics, script, untranslated: Vec::new(), seed };
        if me.script.is_none() {
            // Scripts without skaterinit: fall back to the stand-in handlers.
            me.physics.scripted = false;
        }
        me
    }

    /// One frame: physics, its events to the scripts, then the scripts'
    /// own update (LIKELY order; the script update's place in the frame
    /// is not read).
    pub fn step(&mut self, s: &Scripts, input: &InputState, world: &dyn World) -> Vec<Event> {
        let mut events = self.physics.step(s, input, world);
        let Some(script) = self.script.as_mut() else { return events };
        let mut ctx = Ctx { p: &mut self.physics, s, input: *input, seed: &mut self.seed, events: Vec::new() };
        for e in events.clone() {
            if let Some(name) = event_name(e) {
                script.event(&mut ctx, qb_key(name), &Params::new());
            }
        }
        script.update(&mut ctx);
        events.extend(ctx.events);
        for k in script.untranslated.drain(..) {
            if !self.untranslated.contains(&k) {
                self.untranslated.push(k);
            }
        }
        events
    }

    /// The running script's name.
    pub fn script_name(&self) -> Option<u32> {
        self.script.as_ref().map(|s| s.name)
    }
}

/// Button names as the trick component's `Held` reads them (table at
/// 826E08C8, index = position), mapped to our controller records. Retail
/// keeps per-button held flags (`+2664`) updated from input events; that
/// pipeline is not read, so the records' held state stands in (LIKELY).
fn held(input: &InputState, button: u32) -> bool {
    let k = qb_key;
    let i = input;
    let table: [(&str, bool); 17] = [
        ("Up", i.up),
        ("Down", i.down),
        ("Left", i.left),
        ("Right", i.right),
        ("UpLeft", i.up && i.left),
        ("UpRight", i.up && i.right),
        ("DownLeft", i.down && i.left),
        ("DownRight", i.down && i.right),
        ("Circle", i.circle),
        ("Square", i.kick),
        ("X", i.crouch),
        ("Triangle", i.triangle),
        ("L1", i.l1),
        ("L2", i.l2),
        ("R1", i.r1),
        ("R2", i.r2),
        ("L3", false),
    ];
    table.iter().any(|(n, v)| k(n) == button && *v)
}

impl Host for Ctx<'_> {
    fn global(&self, key: u32) -> Option<Value> {
        self.s.globals.get(&key).cloned()
    }

    fn is_command(&self, name: u32) -> bool {
        COMMANDS.iter().any(|n| qb_key(n) == name)
    }

    fn random(&mut self, n: u32) -> u32 {
        // Retail 821E8508's generator is not read; a plain LCG here.
        *self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (*self.seed >> 8) % n.max(1)
    }

    fn now_ms(&self) -> f64 {
        self.p.time_ms as f64 + self.p.time_frac_ms as f64
    }

    fn command(&mut self, script: &mut Script, name: u32, params: &Params) -> Option<bool> {
        let r = self.run(script, name, params);
        if std::env::var_os("P8_TRACE").is_some() {
            eprintln!("  [{:08x}] {:08x} {:?} -> {r:?}", script.name, name, params.0);
        }
        r
    }
}

impl Ctx<'_> {
    fn run(&mut self, script: &mut Script, name: u32, params: &Params) -> Option<bool> {
        let k = qb_key;
        let p = &mut *self.p;
        let n = name;
        // --- SkaterCorePhysics (registry 826D40C8, filled by 8265D378) ---
        if n == k("Jump") {
            // 820F0FD8 -> 820F0730, with the script's `speed`.
            let speed = params.float(k("Speed"));
            self.events.extend(p.jump(self.s, speed));
            return Some(true);
        }
        if n == k("Crouched") {
            return Some(p.crouched); // 820D4AC8: SkaterState +32
        }
        if n == k("AirTimeGreaterThan") || n == k("AirTimeLessThan") {
            // 820D4D60 / 820D4D20: air time (820D4CA8) against the number
            // (a global's name reads as the global).
            let limit = params
                .unnamed_float()
                .or_else(|| params.unnamed_checksum().and_then(|c| self.s.globals.get(&c)).and_then(Value::as_f32))
                .unwrap_or(0.0);
            let t = p.air_time_ms() as f32;
            return Some(if n == k("AirTimeGreaterThan") { t > limit } else { t < limit });
        }
        if n == k("Braking") {
            return Some(p.braking); // 820D4E10
        }
        if n == k("CanBrakeOn") || n == k("CanBrakeOff") {
            p.can_brake = n == k("CanBrakeOn"); // 820D4E40 / 820D4E28: +1957
            return Some(true);
        }
        if n == k("CanKickOn") || n == k("CanKickOff") {
            p.no_kick = n == k("CanKickOff"); // 820D4F90 / 820D4FA8: +1956
            return Some(true);
        }
        if n == k("ForceAutokickOn") || n == k("ForceAutokickOff") {
            p.autokick = n == k("ForceAutokickOn"); // 820D4FC0 / 820D4FD8: +1540
            return Some(true);
        }
        if n == k("RestoreAutoKick") {
            // 820D5000: the player profile's autokick setting. There is no
            // profile yet; autokick on stands in (as `CorePhysics::new`).
            p.autokick = true;
            return Some(true);
        }
        if n == k("NoSpin") || n == k("CanSpin") {
            // 820D5468 sets +2128 (and clears SkaterState +120) / 820D54C8
            // clears it; spinning needs +2128 clear (820E993C).
            p.no_spin = n == k("NoSpin");
            return Some(true);
        }
        if n == k("NotInBail") {
            p.in_bail = false; // 820D54E0: SkaterState +128
            return Some(true);
        }
        if n == k("IsInBail") {
            return Some(p.in_bail); // 820D5538
        }
        if n == k("BailOn") || n == k("BailOff") {
            p.bail_on = n == k("BailOn"); // 820D5550 / 820D5568: +2129
            return Some(true);
        }
        if n == k("BailIsOn") {
            return Some(p.bail_on); // 820D5580
        }
        if n == k("SetState") {
            // 820F1050: AIR -> 1, Ground -> 0 (then a heading history for
            // animation, 820DE3E8, not needed), Lip -> 3 (not translated).
            match params.unnamed_checksum() {
                Some(c) if c == k("AIR") => p.set_state(State::Air),
                Some(c) if c == k("Ground") => p.set_state(State::Ground),
                _ => return None,
            }
            return Some(true);
        }
        if n == k("LockVelocityDirection") {
            p.lock_velocity_direction = params.flag(k("On")); // 820D56E0: +2184
            return Some(true);
        }
        if n == k("SetRollingFriction") {
            // 820D5730: the number into +1964; `default` clears it when
            // +1976 is 0 (nothing translated sets +1976).
            if let Some(f) = params.unnamed_float() {
                p.special_friction = f;
            }
            if params.flag(k("default")) {
                p.special_friction = 0.0;
            }
            return Some(true);
        }
        if n == k("OverrideLimits") {
            // 820D5C68.
            if params.flag(k("levelend")) {
                p.override_limits = None;
                return Some(true);
            }
            if p.override_limits.is_some_and(|o| o.timer == -2.0) {
                return Some(true);
            }
            if params.flag(k("End")) {
                p.override_limits = None;
                return Some(true);
            }
            // "Max" is required (asserts when missing).
            let max = params.float(k("Max")).unwrap_or(0.0);
            let mut o = OverrideLimits {
                // 2.54e11 is the constant at 82002994.
                timer: params.float(k("Time")).unwrap_or(2.54e11),
                max,
                max_max: params.float(k("max_max")).unwrap_or(max),
                // 2e-6 is the constant at 82002990.
                friction: params.float(k("friction")).unwrap_or(2e-6),
                gravity: params.float(k("gravity")).unwrap_or(self.s.physics_float("Physics_Ground_Gravity", p.on_bike)),
            };
            if params.flag(k("notimelimit")) {
                o.timer = -1.0;
            }
            if params.flag(k("CurrentLevel")) {
                o.timer = -2.0;
            }
            // Retail stores the timer; a stored 0 means inactive.
            p.override_limits = if o.timer == 0.0 { None } else { Some(o) };
            return Some(true);
        }
        if n == k("SkaterIsFlipping") {
            return Some(p.flipping); // 820D6738: +1542
        }
        if n == k("SkaterIsUpsideDown") {
            return Some(p.body.up().y < 0.0); // 820D6750
        }
        if n == k("ResetIsFlipping") {
            p.flipping = false; // 820D6788
            return Some(true);
        }
        if n == k("IsSkaterOnBike") {
            return Some(p.on_bike); // 820D5180: +1564
        }
        if n == k("InVertAir") {
            return Some(p.vert.in_vert_air); // 820D5BA8: SkaterState +56
        }
        if n == k("setvertairflag") {
            p.vert.in_vert_air = true; // 820D63B0
            return Some(true);
        }
        if n == k("forcebreakvert") {
            // 820F1250: 820EC7B0(1), then 820DC5D0.
            p.break_vert(self.s, true);
            p.upright_sideways(self.s);
            return Some(true);
        }
        if n == k("LandedFromVert") {
            return Some(p.vert.landed_from_vert); // 820D5A68: +2131
        }
        if n == k("SetLandedFromVert") {
            p.vert.landed_from_vert = true; // 820D5AB0
            return Some(true);
        }
        if n == k("ResetLandedFromVert") {
            // 820D5AC8: also clears +2134 (spine landing, untranslated).
            p.vert.landed_from_vert = false;
            return Some(true);
        }
        if n == k("WasLastLandingVert") {
            return Some(p.vert.last_landing_vert); // 820D5BD8: +2136
        }
        if n == k("SetLastLandingVert") {
            p.vert.last_landing_vert = true; // 820D5B90
            return Some(true);
        }
        if n == k("SetLastLandingGround") {
            p.vert.last_landing_vert = false; // 820D5BC0
            return Some(true);
        }
        // --- SkaterState (registry 826D6300) ---
        if n == k("OnGround") {
            return Some(p.state == State::Ground); // 82116970: +24 == 0
        }
        if n == k("InAir") {
            return Some(p.state == State::Air); // 82116980: +24 == 1
        }
        if n == k("OnWall") || n == k("OnLip") || n == k("OnRail") || n == k("OnStall") {
            // 82116998 ...: other states, none translated, so never.
            return Some(false);
        }
        if n == k("InBailstate") {
            return Some(p.in_bail); // 82116A10: +128
        }
        if n == k("Walking") || n == k("Skating") {
            // 82116A28 / 82116A50: the walk/skate mode (+28); walking is not
            // translated, so always skating.
            return Some(n == k("Skating"));
        }
        // --- skaterstancepanel (registry 826D6250) ---
        if n == k("InNollie") {
            return Some(p.nollie); // 82116370: +29
        }
        if n == k("NollieON") || n == k("NollieOff") {
            // 82116388 / 821163E0 (their broadcasts are not translated).
            p.nollie = n == k("NollieON");
            return Some(true);
        }
        // --- compositeobject ---
        if n == k("GetSpeed") {
            // 8221F6A0: `speed` = |velocity| into the script's locals.
            script.locals.add(k("Speed"), Value::Float(p.body.velocity.length()));
            return Some(true);
        }
        if n == k("SpeedLessThan") {
            let limit = params.unnamed_float().unwrap_or(0.0);
            return Some(p.body.velocity.length() < limit); // 8221FF80
        }
        if n == k("SetSpeed") {
            // 820D86F0: along the velocity, or along `at` when still
            // (speed^2 below 2.5e-5, constant at 820027D8).
            let speed = params.unnamed_float().unwrap_or(0.0);
            let v = p.body.velocity;
            let dir = if v.length_squared() < 2.5e-5 { p.body.at() } else { v / v.length() };
            p.body.velocity = dir * speed;
            return Some(true);
        }
        if n == k("SpeedGreaterThan") {
            let limit = params.unnamed_float().unwrap_or(0.0);
            return Some(p.body.velocity.length() > limit); // 8221FE98
        }
        // --- script commands acting on the skater ---
        if n == k("MakeSkaterGoto") {
            // 821C8800 -> 82199690: goto on the skater's script (here the
            // script running the command) and run it now.
            let target = params.unnamed_checksum().unwrap_or(0);
            let args = params.params(k("Params")).unwrap_or_default();
            script.goto(self, target, &args);
            return Some(true);
        }
        // --- trick ---
        if n == k("Held") {
            // 8211F680: any of `Buttons`, else the unnamed button.
            if let Some(Value::Array(items)) = params.get(k("Buttons")) {
                return Some(items.iter().any(|v| matches!(v, Value::Checksum(c) if held(&self.input, *c))));
            }
            return Some(params.unnamed_checksum().is_some_and(|c| held(&self.input, c)));
        }
        None
    }
}

/// Commands this host translates (so expressions call them).
const COMMANDS: &[&str] = &[
    "Jump",
    "Crouched",
    "AirTimeGreaterThan",
    "AirTimeLessThan",
    "Braking",
    "CanBrakeOn",
    "CanBrakeOff",
    "CanKickOn",
    "CanKickOff",
    "ForceAutokickOn",
    "ForceAutokickOff",
    "RestoreAutoKick",
    "NoSpin",
    "CanSpin",
    "NotInBail",
    "IsInBail",
    "BailOn",
    "BailOff",
    "BailIsOn",
    "SetState",
    "LockVelocityDirection",
    "SetRollingFriction",
    "OverrideLimits",
    "SkaterIsFlipping",
    "SkaterIsUpsideDown",
    "ResetIsFlipping",
    "IsSkaterOnBike",
    "InVertAir",
    "setvertairflag",
    "forcebreakvert",
    "LandedFromVert",
    "SetLandedFromVert",
    "ResetLandedFromVert",
    "WasLastLandingVert",
    "SetLastLandingVert",
    "SetLastLandingGround",
    "OnGround",
    "InAir",
    "OnWall",
    "OnLip",
    "OnRail",
    "OnStall",
    "InBailstate",
    "Walking",
    "Skating",
    "InNollie",
    "NollieON",
    "NollieOff",
    "GetSpeed",
    "SpeedGreaterThan",
    "SpeedLessThan",
    "SetSpeed",
    "Held",
    "MakeSkaterGoto",
];
