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
        Event::OffMeterTop => "OffMeterTop",
        Event::OffMeterBottom => "OffMeterBottom",
        Event::SkaterJump | Event::SkaterOffEdge => return None,
    })
}

pub struct Skater {
    pub physics: CorePhysics,
    pub script: Option<Script>,
    /// Scripts the physics started with `822265F8` that did not finish in
    /// their first update. Retail keeps such a script alive on the object
    /// (INFERRED from `82210860` creating it and only finished ones being
    /// deleted); they are updated after the main script each frame.
    pub spawned: Vec<Script>,
    /// Every command met that is not translated yet (checksums).
    pub untranslated: Vec<u32>,
    seed: u32,
    /// The animation tree the scripts drive (`skater_anim_command`).
    pub anim: crate::anim_tree::AnimTree,
}

/// What a running script sees of the skater.
struct Ctx<'a> {
    p: &'a mut CorePhysics,
    s: &'a Scripts,
    seed: &'a mut u32,
    events: Vec<Event>,
    world: Option<&'a dyn World>,
    anim: &'a mut crate::anim_tree::AnimTree,
}

impl Skater {
    /// A skater at a restart point, starting the scripts at `skaterinit`
    /// (which ends by going to `ongroundai` through `switch_ongroundai`).
    pub fn new(s: &Scripts, pos: Vec3, angles: Vec3) -> Self {
        let mut physics = CorePhysics::at_restart(s, pos, angles);
        physics.scripted = true;
        // The trick mapping from the player's profile (82120A88).
        let profile = match s.global("master_skater_list") {
            Some(Value::Array(list)) => list
                .iter()
                .find(|e| {
                    matches!(e.get_named("name"), Some(Value::Checksum(c)) if *c == qb_key(crate::core_physics::PLAYER_SKATER))
                        || matches!(e.get_named("name"), Some(Value::String(t)) if t.eq_ignore_ascii_case(crate::core_physics::PLAYER_SKATER))
                })
                .cloned(),
            _ => None,
        };
        physics.tricks.set_mapping(profile.as_ref(), &s.globals);
        let mut seed = 1;
        let mut anim = crate::anim_tree::AnimTree::default();
        let mut ctx = Ctx {
            p: &mut physics,
            s,
            seed: &mut seed,
            events: Vec::new(),
            world: None,
            anim: &mut anim,
        };
        let script = Script::new(&mut ctx, qb_key("skaterinit"), &Params::new());
        let mut me = Skater { physics, script, spawned: Vec::new(), untranslated: Vec::new(), seed, anim };
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
        // skatermatrixqueries (82108848) and the trick component update
        // before the core physics (the skater's components are added in
        // that order, 8219C3C8).
        self.physics.queries_matrix = self.physics.matrix_32;
        self.update_tricks(s, input, world);
        let mut events = self.physics.step(s, input, world);
        let Some(script) = self.script.as_mut() else { return events };
        let goto = self.physics.script_goto.take();
        let mut ctx = Ctx {
            p: &mut self.physics,
            s,
            seed: &mut self.seed,
            events: Vec::new(),
            world: Some(world),
            anim: &mut self.anim,
        };
        if let Some(name) = goto {
            // Retail goes to the script and updates it in place (820F4990).
            script.goto(&mut ctx, name, &Params::new());
            script.update(&mut ctx);
        }
        // Script work the transfer code asked for (retail does it inside
        // the physics; here right after it, in the same order).
        let actions = std::mem::take(&mut ctx.p.transfer.actions);
        for a in actions {
            match a {
                crate::transfer::ScriptAction::Run(name, params) => {
                    if let Some(mut run) = Script::new(&mut ctx, name, &params) {
                        run.update(&mut ctx);
                        Self::untranslated_from(&mut self.untranslated, &mut run);
                        if !run.is_done() {
                            self.spawned.push(run);
                        }
                    }
                }
                crate::transfer::ScriptAction::ClearHandler(event) => {
                    script.handlers.retain(|h| h.event != event);
                }
            }
        }
        for e in events.clone() {
            if let Some(name) = event_name(e) {
                script.event(&mut ctx, qb_key(name), &Params::new());
            }
        }
        script.update(&mut ctx);
        for run in &mut self.spawned {
            run.update(&mut ctx);
        }
        events.extend(ctx.events);
        for k in script.untranslated.drain(..).chain(self.spawned.iter_mut().flat_map(|r| r.untranslated.drain(..))) {
            if !self.untranslated.contains(&k) {
                self.untranslated.push(k);
            }
        }
        self.spawned.retain(|r| !r.is_done());
        // Component updates after the scripts (their order among the
        // skater's components is LIKELY, not read): the balance
        // component's rumble (820CF438), then the vibration timers (8228D260).
        let p = &mut self.physics;
        let now = p.time_ms;
        if let Some(percent) = p.balance.rumble_percent(s, p.body.velocity.length()) {
            p.vibration.vibrate(false, 1, percent, None, now);
            p.vibration.vibrate(false, 0, percent, None, now);
        }
        p.vibration.update(now);
        events
    }

    /// The trick component's frame (`821247F0`): buttons, extra tricks run at
    /// once (`82123CF0` -> `82123B00` -> `82122E10` on the skater's script),
    /// then the queue and the pending manual / grind tricks.
    fn update_tricks(&mut self, s: &Scripts, input: &InputState, world: &dyn World) {
        let now = self.physics.time_ms as u32;
        let buttons = self.physics.balance.buttons();
        self.physics.tricks.update(input, buttons, now);
        // No special meter yet: its lists are never tried ([+28]+116).
        let special = false;
        if self.physics.tricks.extra_active(now) {
            'lists: for (list, skip, is_special) in self.physics.tricks.extra_lists_to_try(special) {
                let Some(Value::Array(entries)) = s.globals.get(&list).cloned() else { continue };
                self.physics.tricks.last_special = is_special;
                for (index, entry) in entries.iter().enumerate() {
                    if !self.physics.tricks.extra_hit(entry, index, skip, now, &s.globals) {
                        continue;
                    }
                    if let Some(trick) = self.physics.tricks.resolve(entry, &s.globals)
                        && let Some(run) = self.physics.tricks.run(&trick, Some(qb_key("isextra")), None)
                    {
                        self.physics.doing_trick = true;
                        self.goto_trick(s, world, run);
                    }
                    if !self.physics.tricks.extra_on {
                        break 'lists;
                    }
                }
            }
        }
        self.physics.tricks.update_lists(special, now, now, &s.globals);
    }

    /// Go to a trick's script on the skater's script and run it now
    /// (`82226588` + `8220F8F0`).
    fn goto_trick(&mut self, s: &Scripts, world: &dyn World, run: crate::trick::RunTrick) {
        let Some(script) = self.script.as_mut() else { return };
        let mut ctx = Ctx {
            p: &mut self.physics,
            s,
            seed: &mut self.seed,
            events: Vec::new(),
            world: Some(world),
            anim: &mut self.anim,
        };
        script.goto(&mut ctx, run.script, &run.params);
        script.update(&mut ctx);
    }

    fn untranslated_from(untranslated: &mut Vec<u32>, run: &mut Script) {
        for k in run.untranslated.drain(..) {
            if !untranslated.contains(&k) {
                untranslated.push(k);
            }
        }
    }

    /// Launch the animation events the timers fired in the last
    /// `anim.update` (`8237C380` -> `822F10E8` -> `82225758`): each goes to
    /// the skater's script like any event, whose handlers
    /// (`ActualSkaterAnimHandlerExceptionTable`) run the matching script
    /// (`SpacewalkBoost`, `HandleKickBoostEvent`, ...). Call it after
    /// updating the animation; returns the events the scripts raised.
    pub fn launch_anim_events(&mut self, s: &Scripts, world: Option<&dyn World>) -> Vec<Event> {
        let fired = std::mem::take(&mut self.anim.fired);
        let Some(script) = self.script.as_mut() else { return Vec::new() };
        if fired.is_empty() {
            return Vec::new();
        }
        let mut ctx = Ctx { p: &mut self.physics, s, seed: &mut self.seed, events: Vec::new(), world, anim: &mut self.anim };
        for (name, params) in fired {
            script.event(&mut ctx, name, &params);
        }
        ctx.events
    }

    /// The running script's name.
    pub fn script_name(&self) -> Option<u32> {
        self.script.as_ref().map(|s| s.name)
    }
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

    fn wait_done(&mut self, token: u64, target: f32) -> bool {
        self.anim.timer_wait_done(token, target)
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
            let (boneless, no_comply) = (params.flag(k("BonelessHeight")), params.flag(k("NoComply")));
            self.events.extend(p.jump(self.s, speed, boneless, no_comply));
            return Some(true);
        }
        if n == k("LastWasJumpBoneless") {
            return Some(p.last_jump_boneless); // 820D5BF0: +2544
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
            if p.no_spin {
                p.vert.auto_turn = false; // 820D5468 clears +120
            }
            return Some(true);
        }
        if n == k("InBail") {
            // 820D8868: SkaterState `+128` = 1 (its time `+132`, now plus a
            // constant, is not read anywhere yet) and `+2784` cleared. Without
            // `RunOut` retail then goes to state 9, whose update `820EAED0`
            // drives the skater from the ragdoll bones: not translated, so
            // the skater keeps the ground / air physics with the flag set
            // (APPROXIMATE; `RunOut` bails, which keep the state, are exact).
            p.in_bail = true;
            return Some(true);
        }
        if n == k("GetLastInAirVerticalVelocity") {
            // 820D6330: `last_vert_vel` = core `+1936`.
            script.locals.add(k("last_vert_vel"), Value::Float(p.last_in_air_vy));
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
            // Leaving the lip runs trigger 144 (not translated); taking the
            // lip without being on one forgets the rail (+1192).
            match params.unnamed_checksum() {
                Some(c) if c == k("AIR") => p.set_state(State::Air),
                Some(c) if c == k("Ground") => p.set_state(State::Ground),
                Some(c) if c == k("Lip") => {
                    if p.state != State::Lip {
                        p.rail = None;
                    }
                    p.set_state(State::Lip);
                }
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
            p.set_override(o);
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
            p.break_vert(self.s, self.world, true);
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
            // 820D5AC8: +2131 and +2134 (`LandedOnBank`).
            p.vert.landed_from_vert = false;
            p.transfer.landed_on_bank = false;
            return Some(true);
        }
        if n == k("IsInSpineTransfer") {
            return Some(p.transfer.active); // 820D6C70: SkaterState +136
        }
        if n == k("LandedFromSpine") {
            return Some(p.transfer.landed_from_spine); // 820D5A50: +2130
        }
        if n == k("LandedOnBank") {
            return Some(p.transfer.landed_on_bank); // 820D5A80: +2134
        }
        if n == k("landedfromtiretap") {
            return Some(p.transfer.landed_from_tiretap); // 820D5A98: +2133
        }
        if n == k("DisallowAcidDrops") {
            p.set_no_acid_drop(true); // 8211F9A8: SkaterState +200
            return Some(true);
        }
        // Object flags (`821C8510`: set `+44 |= 1 << n`, clear `&= !(1 << n)`,
        // test `+44 & (1 << n)`). The flag is its global's int, given as the
        // int or as the global's name. Which of 821C8510's cases each command
        // reaches was not read (their member table is filled at start-up):
        // the names make it LIKELY.
        let flag_cmd = [k("Obj_SetFlag"), k("Obj_ClearFlag"), k("Obj_FlagSet"), k("Obj_FlagNotSet")];
        if flag_cmd.contains(&n) {
            let bit = params.unnamed_int().or_else(|| {
                params.unnamed_checksum().and_then(|c| match self.s.globals.get(&c) {
                    Some(Value::Int(i)) => Some(*i),
                    _ => None,
                })
            });
            let Some(bit) = bit.filter(|b| (0..32).contains(b)) else { return Some(false) };
            let m = 1u32 << bit;
            return Some(match n {
                x if x == k("Obj_SetFlag") => {
                    p.object_flags |= m;
                    true
                }
                x if x == k("Obj_ClearFlag") => {
                    p.object_flags &= !m;
                    true
                }
                x if x == k("Obj_FlagSet") => p.object_flags & m != 0,
                _ => p.object_flags & m == 0,
            });
        }
        // Input component members (table 826E1D00, filled at start-up by
        // 82669990): LeftPressed 82259928 = record "Left" held (input +96),
        // RightPressed 82116A10 = record "Right" held (+128). The cess turn
        // (`ToggleSwitchRegular`) picks its side with them.
        if n == k("LeftPressed") {
            return Some(p.last_input.left);
        }
        if n == k("RightPressed") {
            return Some(p.last_input.right);
        }
        if n == k("LastSpinWas") {
            // 820D5908: +2217 (the last rotation was positive) read against
            // the stance (SkaterState +40): `Frontside` is positive when
            // flipped, negative when not; `Backside` the other way; neither
            // param: true. Script `revert` (GroundTricks) does a FS revert
            // when `lastspinwas frontside` is false, else a BS revert.
            // 820D593C / 820D5948 `Backside` param, 820D5960 stance;
            // 820D5984 / 820D5990 `Frontside`, 820D59A8 stance; neither: 1.
            let (b, f) = (p.last_spin_positive, p.flipped);
            if params.flag(k("Backside")) {
                return Some(if f { !b } else { b });
            }
            if params.flag(k("Frontside")) {
                return Some(if f { b } else { !b });
            }
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
        if n == k("OnLip") {
            return Some(p.state == State::Lip); // 821169B0: +24 == 3
        }
        if n == k("OnWall") || n == k("OnRail") || n == k("OnStall") {
            // 82116998 ...: other states, none translated, so never.
            return Some(false);
        }
        if n == k("SkateInAble") {
            // 820EB538: flags `Lip` and `Left`.
            let world = self.world?;
            return Some(p.skate_in_able(self.s, world, params.flag(k("Left")), params.flag(k("Lip"))));
        }
        if n == k("Move") {
            // 8221DE50: along the object's own axes (X row 0, Y up, Z at).
            let m = p.body.matrix;
            let x = params.float(k("X")).unwrap_or(0.0);
            let y = params.float(k("Y")).unwrap_or(0.0);
            let z = params.float(k("Z")).unwrap_or(0.0);
            p.body.position += m.x_axis * x + m.y_axis * y + m.z_axis * z;
            return Some(true);
        }
        if n == k("SetSkaterVelocity") {
            // 821DE1AC: refused (false) in a spine transfer.
            if p.transfer.active {
                return Some(false);
            }
            // 821DE168: an unnamed
            // number sets the speed along the velocity; otherwise vel_x,
            // vel_y and vel_z (world axes, missing = 0).
            if let Some(speed) = params.unnamed_float() {
                p.body.velocity = p.body.velocity.normalize_or_zero() * speed;
            } else {
                p.body.velocity = Vec3::new(
                    params.float(k("vel_x")).unwrap_or(0.0),
                    params.float(k("vel_y")).unwrap_or(0.0),
                    params.float(k("vel_z")).unwrap_or(0.0),
                );
            }
            return Some(true);
        }
        if n == k("GetSkaterVelocity") {
            // 821DFCB0: vel_x/y/z as integers (truncated). Its skewed and
            // scaled variants are not translated.
            let v = p.body.velocity;
            script.locals.add(k("vel_x"), Value::Int(v.x as i32));
            script.locals.add(k("vel_y"), Value::Int(v.y as i32));
            script.locals.add(k("vel_z"), Value::Int(v.z as i32));
            return Some(true);
        }
        if n == k("DoBalanceTrick") {
            let (stats, ctx, on_bike, now) = (p.stats.clone(), p.stat_context, p.on_bike, p.time_ms);
            let mut rng = p.rng;
            let mut random = |n: u32| {
                rng = rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (rng >> 8) % n.max(1)
            };
            let mut c = crate::balance::BalanceCtx {
                s: self.s,
                stats: &stats,
                stat_context: ctx,
                on_bike,
                now_ms: now,
                random: &mut random,
            };
            p.balance.do_balance_trick(&mut c, params);
            p.rng = rng;
            return Some(true);
        }
        if n == k("NoRailTricks") || n == k("AllowRailTricks") {
            p.no_rail_tricks = n == k("NoRailTricks"); // 820D5C38 / 820D5C50: +2216
            return Some(true);
        }
        if n == k("AllowLipNoGrind") || n == k("ClearAllowLipNoGrind") {
            p.allow_lip_no_grind = n == k("AllowLipNoGrind"); // 820D5C08 / 820D5C20: +2137
            return Some(true);
        }
        // `anim_command` is the anim tree component's command (82244CF0),
        // the same one `Skater_Anim_Command` (820B8558) forwards to.
        if n == k("Skater_Anim_Command") || n == k("Anim_Command") {
            // Nodes built now read the skater's state as it is now (e.g.
            // skaterflip's init 820B04F8 reads SkaterState +40 / +48), not
            // as the last animation update saw it; the landing prediction
            // (needs the world) keeps the last update's values.
            let old = self.anim.inputs;
            let mut now_inputs = self.p.anim_inputs(self.s);
            now_inputs.time_to_land = old.time_to_land;
            now_inputs.time_to_land_slice = old.time_to_land_slice;
            now_inputs.time_to_apex = old.time_to_apex;
            self.anim.inputs = now_inputs;
            // The anim component's command (see anim_tree.rs): `target` node
            // id, `command`, `params`.
            let target = params.checksum(k("target")).unwrap_or(0);
            let command = params.checksum(k("command")).unwrap_or(0);
            let inner = params.params(k("params")).unwrap_or_default();
            let g = |c: u32| self.s.globals.get(&c).cloned();
            let globals = &self.s.globals;
            let named = |v: &Value| globals.iter().find(|(_, x)| *x == v).map(|(k, _)| *k);
            if command == k("timer_wait") {
                // 823866C8 -> 82386620: the script waits on the timer.
                return Some(match self.anim.timer_wait(target, &inner) {
                    Some((serial, f)) => {
                        script.wait_on_host(serial, f);
                        true
                    }
                    None => false,
                });
            }
            return Some(self.anim.command_named(&target, command, &inner, &g, Some(&named)));
        }
        if n == k("Skater_AnimNodeExists") {
            let id = params.checksum(k("id")).unwrap_or(0);
            return Some(self.anim.node_exists(id));
        }
        if n == k("Vibrate") {
            // 8228D528 (the "vibration" component).
            let off = params.flag(k("OFF"));
            let actuator = params.int(k("Actuator")).unwrap_or(0);
            let percent = params.float(k("Percent")).unwrap_or(0.0);
            let duration = params.float(k("duration"));
            let now = p.time_ms;
            return Some(p.vibration.vibrate(off, actuator, percent, duration, now));
        }
        if n == k("ClearPanel_Landed") || n == k("ClearPanel_Bailed") {
            // 82124BA8 / 82124EC0 (the combo ends). Only the balance part is
            // translated: 820CE840 resets every meter. The score, gaps,
            // SkaterLanded / SkaterBailed / SkaterExitCombo events and the
            // rest are not (no score system yet). Returns TRUE (li r3,1).
            if n == k("ClearPanel_Landed") {
                p.balance.record_longest(); // 82124E98: 820CE7F8
            }
            p.balance.reset_all();
            return Some(true);
        }
        if n == k("StopBalanceTrick") {
            p.balance.stop();
            return Some(true);
        }
        if n == k("DoingBalanceTrick") {
            // 820CE5E8: balance `+28`.
            return Some(p.balance.doing);
        }
        if n == k("StartBalanceTrick") {
            // 820CE600.
            p.balance.doing = true;
            return Some(true);
        }
        if n == k("SetBalanceTrickType") {
            // 820CE750: the first unnamed checksum into balance `+24`.
            if let Some(t) = params.unnamed_checksum() {
                p.balance.kind = t;
            }
            return Some(true);
        }
        if n == k("GetBalanceTrickType") {
            // 820CE700: `trick_type` = balance `+24`.
            script.locals.add(k("trick_type"), Value::Checksum(p.balance.kind));
            return Some(true);
        }
        if n == k("SetLastAnimData") {
            // 820D6DE8: the first unnamed checksum into physics `+1524`.
            if let Some(c) = params.unnamed_checksum() {
                p.last_anim_data = c;
            }
            return Some(true);
        }
        if n == k("GetLastAnimData") {
            // 820D6E50: `prev_data` = physics `+1524`.
            script.locals.add(k("prev_data"), Value::Checksum(p.last_anim_data));
            return Some(true);
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
        // --- trick (component commands 8211F598..82124BA8) ---
        let now = p.time_ms as u32;
        let g = &self.s.globals;
        if n == k("Held") {
            // 8211F680: any of `Buttons`, else the unnamed button; the held
            // flags +2664.
            let held = |c: u32| p.tricks.held[crate::trick::button_id(c)];
            if let Some(Value::Array(items)) = params.get(k("Buttons")) {
                return Some(items.iter().any(|v| matches!(v, Value::Checksum(c) if held(*c))));
            }
            return Some(params.unnamed_checksum().is_some_and(held));
        }
        if n == k("pressed") {
            return Some(p.tricks.pressed(params, now));
        }
        if n == k("ClearTricksFrom") {
            // 8211F808 -> 8211E9C0 for each unnamed list: dropped from the
            // queue lists, its queued entries emptied.
            for (key, v) in &params.0 {
                if *key == 0
                    && let Value::Checksum(list) = v
                {
                    p.tricks.lists.retain(|l| l != list);
                    for q in p.tricks.queue.iter_mut().filter(|q| q.list == *list) {
                        q.list = 0;
                    }
                }
            }
            return Some(true);
        }
        // --- SkaterState / skaterflipandrotate / animinfo flags ---
        if n == k("Flipped") {
            return Some(p.flipped); // 820B8688: SkaterState +40
        }
        if n == k("DoingFlip") {
            return Some(p.flipping); // 820D6738: +1542
        }
        if n == k("DoingTrick") {
            return Some(p.doing_trick); // 82116AC0: SkaterState +264
        }
        if n == k("setdoingtrick") || n == k("unsetdoingtrick") {
            p.doing_trick = n == k("setdoingtrick"); // 82116AD8 / 82116AF0
            return Some(true);
        }
        if n == k("Flip") {
            p.flip_stance(); // 820FDA80 -> 820FD8E8
            return Some(true);
        }
        if n == k("FlipAndRotate") {
            // 820FDAF0: 820D9008 (turn round) then 820FD8E8 (flip).
            // Skater_PlayOllieAnim runs it in nollie, so the ollie clips
            // play from the nose.
            p.turn_round();
            p.flip_stance();
            return Some(true);
        }
        if n == k("FlipAfter") || n == k("UnsetFlipAfter") {
            p.flip_after = n == k("FlipAfter"); // 820FD738 / 820FD750: +25
            return Some(true);
        }
        if n == k("IsFlipAfterSet") {
            return Some(p.flip_after); // 820FD7C8
        }
        if n == k("RotateAfter") || n == k("UnsetRotateAfter") {
            p.rotate_after = n == k("RotateAfter"); // 820FD768 / 820FD780: +26
            return Some(true);
        }
        if n == k("IsRotateAfterSet") {
            return Some(p.rotate_after); // 820FD7E0
        }
        if n == k("BoardRotateAfter") || n == k("UnsetBoardRotateAfter") {
            p.board_rotate_after = n == k("BoardRotateAfter"); // 820FD798 / 820FD7B0: +27
            return Some(true);
        }
        if n == k("IsBoardRotateAfterSet") {
            return Some(p.board_rotate_after); // 820FD7F8
        }
        if n == k("HandleFlipOrBoardRotateAfter") {
            return Some(p.handle_flip_or_rotate_after()); // 820FDCE0 -> 820FDBE0
        }
        if n == k("Backwards") {
            // 8221E068: velocity . at < 0.
            return Some(p.body.velocity.dot(p.body.at()) < 0.0);
        }
        if n == k("ProfileEquals") {
            // 8210E4D8 (only `stance` translated: the profile's goofy flag
            // +36 against goofy / regular).
            if let Some(st) = params.checksum(k("stance")) {
                let mine = if p.goofy { k("goofy") } else { k("regular") };
                return Some(st == mine);
            }
            return None;
        }
        // --- skatermatrixqueries (its matrix: `queries_matrix`) ---
        if n == k("PitchGreaterThan") {
            let d = params.unnamed_float().unwrap_or(0.0);
            return Some(crate::queries::pitch_greater(&p.queries_matrix, p.vert.ease_from, d));
        }
        if n == k("AbsolutePitchGreaterThan") {
            let d = params.unnamed_float().unwrap_or(0.0);
            return Some(crate::queries::absolute_pitch_greater(&p.queries_matrix, d));
        }
        if n == k("RollGreaterThan") {
            let d = params.unnamed_float().unwrap_or(0.0);
            return Some(crate::queries::roll_greater(&p.queries_matrix, p.vert.ease_from, d));
        }
        if n == k("YawBetween") {
            // 82213370: the unnamed pair.
            let (a, b) = params.0.iter().find_map(|(key, v)| if *key == 0 { if let Value::Pair(a, b) = v { Some((*a, *b)) } else { None } } else { None })?;
            return Some(crate::queries::yaw_between(&p.queries_matrix, p.body.velocity, a, b));
        }
        // --- script helpers ---
        if n == k("Anim_GetAnimLength") {
            // 8228F098: `length` = the clip's length.
            let anim = params.checksum(k("anim")).unwrap_or(0);
            let length = self.anim.clip_length(anim)?;
            script.locals.add(k("length"), Value::Float(length));
            return Some(true);
        }
        if n == k("GetScriptedStat") {
            // 8219B390 -> 82199A28 on the unnamed stat struct: `stat_value`.
            // Its one use, `GetScriptedStat ?Skater_Flip_Speed_Stat`, passes
            // a link to the global (82214898), which 82212A68 finds as the
            // unnamed struct; our VM merges a linked struct's members into
            // the params, so those members are the struct.
            let def = params
                .0
                .iter()
                .find(|(key, v)| *key == 0 && matches!(v, Value::Struct(_)))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| Value::Struct(params.0.clone()));
            let v = self.s.stat_value_of(&def, &p.stats, p.stat_context);
            script.locals.add(k("stat_value"), Value::Float(v));
            return Some(true);
        }
        if n == k("Released") {
            return Some(p.tricks.released(params));
        }
        if n == k("SetQueueTricks") {
            p.tricks.set_queue_tricks(params, g);
            return Some(true);
        }
        if n == k("ClearTrickQueue") {
            p.tricks.queue.clear();
            return Some(true);
        }
        if n == k("SetManualTricks") {
            p.tricks.set_manual_tricks(params);
            return Some(true);
        }
        if n == k("ClearManualTrick") {
            // 8211FAF0.
            p.tricks.manual_pending = None;
            p.tricks.manual_lists.clear();
            return Some(true);
        }
        if n == k("SetExtraTricks") {
            p.tricks.set_extra_tricks(params, now, g);
            return Some(true);
        }
        if n == k("KillExtraTricks") {
            p.tricks.extra_on = false;
            return Some(true);
        }
        if n == k("UseGrindEvents") {
            p.tricks.flags = 8;
            return Some(true);
        }
        if n == k("ClearEventBuffer") {
            p.tricks.clear_event_buffer(params, now, g);
            return Some(true);
        }
        if n == k("SetTrickName") {
            // 82120160 (kept for the score display, not translated).
            p.tricks.trick_name = match params.0.first().map(|(_, v)| v) {
                Some(Value::String(t)) => t.clone(),
                _ => String::new(),
            };
            return Some(true);
        }
        if n == k("SetTrickScore") {
            // 82120250.
            p.tricks.trick_score = params.unnamed_int().unwrap_or(0);
            return Some(true);
        }
        if n == k("DoNextTrick") || n == k("DoNextManualTrick") {
            // 82124960 / 82124AF8: first the script detect_dodgy_ragdoll_state
            // (822104B0), then the next trick (821230D8 / 821235C0).
            if let Some(mut run) = Script::new(self, k("detect_dodgy_ragdoll_state"), &Params::new()) {
                run.update(self);
            }
            let p = &mut *self.p;
            let g = &self.s.globals;
            // 821230D8 clears SkaterState +264 first.
            if n == k("DoNextTrick") {
                p.doing_trick = false;
            }
            let trick = if n == k("DoNextTrick") { p.tricks.next_queued(now, g) } else { p.tricks.next_manual(g) };
            let Some(trick) = trick else { return Some(true) };
            let extra = params.params(k("trickparams"));
            let Some(mut run) = p.tricks.run(&trick, None, extra.as_ref()) else { return Some(true) };
            if let Some(first) = params.checksum(k("scripttorunfirst")) {
                // 82210460 (INFERRED: runs it now with `params`).
                let args = params.params(k("params")).unwrap_or_default();
                if let Some(mut f) = Script::new(self, first, &args) {
                    f.update(self);
                }
            }
            // 82226588: the skater's script goes to the trick (here the
            // script running the command; INFERRED the same one); 82122E10
            // sets SkaterState +264.
            self.p.doing_trick = true;
            // 821230D8 / 821235C0: the command's own parameters (`FromAir`,
            // `FromGroundGone`, ...) are passed on to the trick script, with
            // the trick's `params` merged in (82215B88).
            let mut args = params.clone();
            args.merge(&std::mem::take(&mut run.params));
            script.goto(self, run.script, &args);
            return Some(true);
        }
        None
    }
}

/// Commands this host translates (so expressions call them).
const COMMANDS: &[&str] = &[
    "Skater_Anim_Command",
    "Anim_Command",
    "Skater_AnimNodeExists",
    "Vibrate",
    "ClearPanel_Landed",
    "ClearPanel_Bailed",
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
    "InBail",
    "GetLastInAirVerticalVelocity",
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
    "IsInSpineTransfer",
    "LandedFromSpine",
    "LandedOnBank",
    "landedfromtiretap",
    "DisallowAcidDrops",
    "WasLastLandingVert",
    "SetLastLandingVert",
    "SetLastLandingGround",
    "OnGround",
    "InAir",
    "OnWall",
    "OnLip",
    "SkateInAble",
    "Move",
    "SetSkaterVelocity",
    "GetSkaterVelocity",
    "DoBalanceTrick",
    "StopBalanceTrick",
    "DoingBalanceTrick",
    "StartBalanceTrick",
    "SetBalanceTrickType",
    "GetBalanceTrickType",
    "SetLastAnimData",
    "GetLastAnimData",
    "NoRailTricks",
    "AllowRailTricks",
    "AllowLipNoGrind",
    "ClearAllowLipNoGrind",
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
    "pressed",
    "Released",
    "SetQueueTricks",
    "ClearTrickQueue",
    "SetManualTricks",
    "ClearManualTrick",
    "SetExtraTricks",
    "KillExtraTricks",
    "UseGrindEvents",
    "ClearEventBuffer",
    "SetTrickName",
    "SetTrickScore",
    "DoNextTrick",
    "DoNextManualTrick",
    "ClearTricksFrom",
    "Flipped",
    "DoingFlip",
    "DoingTrick",
    "setdoingtrick",
    "unsetdoingtrick",
    "Flip",
    "FlipAfter",
    "UnsetFlipAfter",
    "IsFlipAfterSet",
    "RotateAfter",
    "UnsetRotateAfter",
    "IsRotateAfterSet",
    "BoardRotateAfter",
    "UnsetBoardRotateAfter",
    "IsBoardRotateAfterSet",
    "HandleFlipOrBoardRotateAfter",
    "LastWasJumpBoneless",
    "FlipAndRotate",
    "Backwards",
    "ProfileEquals",
    "PitchGreaterThan",
    "AbsolutePitchGreaterThan",
    "RollGreaterThan",
    "YawBetween",
    "Anim_GetAnimLength",
    "GetScriptedStat",
    "MakeSkaterGoto",
];
