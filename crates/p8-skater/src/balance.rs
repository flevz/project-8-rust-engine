//! The balance meter of manuals, grinds, lips and skitching (retail
//! component "skaterbalancetrick", physics `+2844`): `DoBalanceTrick`
//! (`820CF748`), `StopBalanceTrick` (`820CF100`), the meter's start
//! (`82190C10`) and its update (`82190F58`), which fires "OffMeterTop" /
//! "OffMeterBottom" when the lean passes `Lean_Bail_Angle`.
//!
//! The display call at the end of the update is [`Balance::show_on_screen`]
//! (see `meter_display.rs`).
//!
//! Not translated: the grind-only parts (same/new rail timing, robot rail),
//! scoring, pausing the meter (`820CE618`, `+97`), the perfect-balance
//! cheats, the network hook and the animation calls.
use crate::input::InputState;
use crate::meter_display::{MeterDisplay, MeterLayout};
use crate::script::Scripts;
use crate::script::StatContext;
use crate::stats::StatLevels;
use p8_formats::qb::Value;
use p8_formats::qb_key;

/// One meter (manual `+48`, grind `+160`, lip `+272`, skitch `+384`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meter {
    /// `+48`: seconds in this balance.
    pub time: f32,
    /// `+52`: seconds that raise the instability.
    pub instable_time: f32,
    /// `+56`: the longest `time` so far.
    pub max_time: f32,
    /// `+60`: the lean; past +-`Lean_Bail_Angle` the balance is lost.
    pub lean: f32,
    /// `+64`: the lean's speed.
    pub lean_speed: f32,
    /// `+68`: the push given to the lean when the balance starts again.
    pub cheese: f32,
    /// `+72`: the buttons were let go (or enough time passed) since the
    /// start, so they count.
    pub buttons_live: bool,
    /// `+76`: when the balance started (ms).
    pub start_ms: i64,
    /// `+80`, `+84`: the buttons that lean one way and the other (0 = the
    /// meter is not running).
    pub button_a: u32,
    pub button_b: u32,
    /// `+88`.
    pub tweak: i32,
}

/// The balance component.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Balance {
    /// `+24`: the balance type running (a checksum: Manual, NoseManual,
    /// Flatland, Grind, Slide, Lip, Skitch; 0 = none).
    pub kind: u32,
    /// `+28`: a balance trick has started (`StartBalanceTrick`, `820CE600`;
    /// read by `DoingBalanceTrick`, `820CE5E8`). Cleared with the type by
    /// `StopBalanceTrick` (`820CEAE8`) and the combo reset (`820CE840`).
    pub doing: bool,
    /// `+32`: `balanceparams` given to `DoBalanceTrick`, used instead of
    /// the type's parameter struct.
    pub params: Option<Value>,
    pub manual: Meter,
    pub grind: Meter,
    pub lip: Meter,
    pub skitch: Meter,
    /// The meter on screen (kept by the score component in retail; the
    /// meter is what drives it).
    pub display: MeterDisplay,
}

/// What the meter update reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffMeter {
    Top,
    Bottom,
}

/// What the balance code needs from the rest of the skater.
pub struct BalanceCtx<'a> {
    pub s: &'a Scripts,
    pub stats: &'a StatLevels,
    pub stat_context: StatContext,
    pub on_bike: bool,
    pub now_ms: i64,
    /// Retail `821E8508(n)`: a number in `0..n` (the generator is not read).
    pub random: &'a mut dyn FnMut(u32) -> u32,
}

impl Balance {
    /// Retail `820CE9D0`: a balance parameter for the running type, from
    /// `balanceparams` if given, else ManualParams (Manual, NoseManual,
    /// Flatland), GrindParams (Grind, Slide), LipParams or SkitchParams;
    /// stat-scaled (`82199DD8`).
    pub fn param(&self, c: &BalanceCtx, name: &str) -> f32 {
        self.param_for(c, 0, name)
    }

    /// `820CE9D0` with its type argument: `kind` 0 means the running type.
    fn param_for(&self, c: &BalanceCtx, kind: u32, name: &str) -> f32 {
        let k = qb_key;
        let group = if let Some(p) = &self.params {
            Some(p.clone())
        } else {
            let t = if kind == 0 { self.kind } else { kind };
            let g = if t == k("Manual") || t == k("NoseManual") || t == k("Flatland") {
                "ManualParams"
            } else if t == k("Grind") || t == k("Slide") {
                "GrindParams"
            } else if t == k("Lip") {
                "LipParams"
            } else if t == k("Skitch") {
                "SkitchParams"
            } else {
                return 0.0;
            };
            c.s.global(g).cloned()
        };
        match group.as_ref().and_then(|g| g.get_named(name)) {
            Some(def) => c.s.stat_value_of(def, c.stats, c.stat_context),
            None => 0.0,
        }
    }

    /// `820CE8D8`: the running balance's two buttons (meter `+80`/`+84`),
    /// which make no trick events; `None` when no balance runs.
    pub fn buttons(&self) -> Option<[u32; 2]> {
        let k = qb_key;
        let kind = self.kind;
        let m = if kind == k("Manual") || kind == k("NoseManual") || kind == k("Flatland") {
            &self.manual
        } else if kind == k("Grind") || kind == k("Slide") {
            &self.grind
        } else if kind == k("Lip") {
            &self.lip
        } else if kind == k("Skitch") {
            &self.skitch
        } else {
            return None;
        };
        Some([m.button_a, m.button_b])
    }

    /// The running balance's lean for the animation (`820CEB80`, which
    /// picks the meter the same way), `None` when none runs.
    pub fn anim_lean(&self) -> Option<f32> {
        let k = qb_key;
        let kind = self.kind;
        if kind == k("Manual") || kind == k("NoseManual") || kind == k("Flatland") {
            Some(self.manual.lean)
        } else if kind == k("Grind") || kind == k("Slide") {
            Some(self.grind.lean)
        } else if kind == k("Lip") {
            Some(self.lip.lean)
        } else if kind == k("Skitch") {
            Some(self.skitch.lean)
        } else {
            None
        }
    }

    fn meter_mut(&mut self, kind: u32) -> Option<&mut Meter> {
        let k = qb_key;
        if kind == k("Manual") || kind == k("NoseManual") || kind == k("Flatland") {
            Some(&mut self.manual)
        } else if kind == k("Grind") || kind == k("Slide") {
            Some(&mut self.grind)
        } else if kind == k("Lip") {
            Some(&mut self.lip)
        } else if kind == k("Skitch") {
            Some(&mut self.skitch)
        } else {
            None
        }
    }

    /// Script command `DoBalanceTrick` (`820CF748`).
    pub fn do_balance_trick(&mut self, c: &mut BalanceCtx, params: &p8_script::Params) {
        let k = qb_key;
        if let Some(p) = params.get(k("balanceparams")) {
            self.params = Some(p.clone());
        }
        let tweak = params.int(k("Tweak")).unwrap_or(0);
        let a = params.checksum(k("ButtonA")).unwrap_or(0);
        let b = params.checksum(k("ButtonB")).unwrap_or(0);
        let kind = params.checksum(k("Type")).unwrap_or(0);
        let cur = self.kind;
        // Switching between related types keeps the balance going.
        if cur == kind
            || (cur == k("Manual") && kind == k("NoseManual"))
            || (cur == k("NoseManual") && kind == k("Manual"))
            || (cur == k("Flatland") && (kind == k("NoseManual") || kind == k("Manual")))
            || (cur == k("Grind") && kind == k("Slide"))
            || (cur == k("Slide") && kind == k("Grind"))
        {
            return;
        }
        if params.flag(k("IsATap")) && cur != 0 {
            return;
        }
        self.kind = kind;
        // `DoFlipCheck` and `PlayRangeAnimBackwards` only matter to the
        // animation code (not translated).
        let Some(m) = self.meter_mut(kind) else { return };
        // 82190AD8.
        if m.time > m.max_time {
            m.max_time = m.time;
        }
        m.time = 0.0;
        self.start(c, kind, a, b, tweak);
    }

    /// Retail `82190C10` (without the grind-only rail timing).
    fn start(&mut self, c: &mut BalanceCtx, kind: u32, a: u32, b: u32, tweak: i32) {
        let k = qb_key;
        let repeat_min = self.param(c, "Repeat_Min");
        let repeat_mult = self.param(c, "Repeat_Multiplier");
        let lean_mult = self.param(c, "Lean_Repeat_Multiplier");
        let bail = self.param(c, "Lean_Bail_Angle");
        let cheese = self.param(c, "Cheese");
        let sign = |x: f32| if x >= 0.0 { 1.0 } else { -1.0 };
        let fresh = self.meter_mut(kind).is_some_and(|m| m.lean_speed == 0.0);
        let random_flip = fresh
            && kind != k("NoseManual")
            && kind != k("Flatland")
            && kind != k("Manual")
            && (c.random)(2) != 0;
        let m = self.meter_mut(kind).expect("a balance type with a meter");
        m.time = 0.0;
        m.button_a = a;
        m.button_b = b;
        m.tweak = tweak;
        m.buttons_live = false;
        if m.lean_speed != 0.0 {
            m.lean_speed *= repeat_mult;
            if m.lean_speed.abs() < repeat_min {
                m.lean_speed = sign(m.lean_speed) * repeat_min;
            }
        } else {
            m.lean_speed = repeat_min;
            if kind == k("NoseManual") {
                m.lean_speed = -repeat_min;
            } else if kind != k("Flatland") && kind != k("Manual") && random_flip {
                m.lean_speed = -m.lean_speed;
            }
        }
        m.lean *= lean_mult;
        m.lean += sign(m.lean) * m.cheese;
        if m.lean.abs() > bail {
            // 0.9 is the constant at 82002A74.
            m.lean = (bail * 0.9).abs() * sign(m.lean_speed);
            m.lean_speed = sign(m.lean_speed) * bail;
        }
        m.cheese = cheese;
        m.start_ms = c.now_ms;
    }

    /// Script command `StopBalanceTrick` (`820CF100` -> `820CEAE8`): every
    /// meter stops (`82190A60` clears its buttons) and no type runs.
    /// Retail `820D4A20` (from `820FC990` every frame after the state
    /// update, unless SkaterState `+24` is 8 or 9, states not translated):
    /// `82190B10` on the Grind, Manual, Lip and Skitch meters. While a
    /// meter's cheese (`+68`) is above 0.01 (82000D7C) it drops by
    /// Cheese / CheeseFrames per 60th of a second (82001EB4), not below 0.
    /// `dt` is retail's frame time `[826E6B98]`.
    pub fn wear_off_cheese(&mut self, c: &BalanceCtx, dt: f32) {
        let k = qb_key;
        for kind in [k("Grind"), k("Manual"), k("Lip"), k("Skitch")] {
            let cheese = self.meter_mut(kind).map_or(0.0, |m| m.cheese);
            if cheese <= 0.01 {
                continue;
            }
            let per_frame = self.param_for(c, kind, "Cheese") / self.param_for(c, kind, "CheeseFrames");
            let m = self.meter_mut(kind).expect("a balance type with a meter");
            m.cheese = (m.cheese - per_frame * dt * 60.0).max(0.0);
        }
    }

    /// The component's per-frame update (`820CF438`, vtable `820027F4`
    /// slot 1): rumble while a meter leans. Returns the percent it sends to
    /// both motors (`Vibrate Actuator = 1`, then `Actuator = 0`, no
    /// duration), or `None` when the lean (`820CEB80`) is 0. Percent =
    /// |lean| / 4096 (82001D5C) * 100 (82000D8C) * f + `min_balance_vibration`,
    /// clamped to 0..100; f = the object's speed (`+208`, x y z) capped at 1,
    /// or 1 on a lip or when physics `+1548` is set (field UNKNOWN, taken as
    /// clear).
    pub fn rumble_percent(&self, s: &Scripts, speed: f32) -> Option<f32> {
        let lean = self.meter_of(self.kind).map_or(0.0, |m| m.lean);
        if lean == 0.0 {
            return None;
        }
        let f = if self.kind == qb_key("Lip") { 1.0 } else { speed.min(1.0) };
        let percent = lean.abs() * (1.0 / 4096.0) * 100.0 * f + s.global_float("min_balance_vibration");
        Some(percent.clamp(0.0, 100.0))
    }

    fn meter_of(&self, kind: u32) -> Option<&Meter> {
        let k = qb_key;
        if kind == k("Manual") || kind == k("NoseManual") || kind == k("Flatland") {
            Some(&self.manual)
        } else if kind == k("Grind") || kind == k("Slide") {
            Some(&self.grind)
        } else if kind == k("Lip") {
            Some(&self.lip)
        } else if kind == k("Skitch") {
            Some(&self.skitch)
        } else {
            None
        }
    }

    /// Retail `820CE7F8` (only `ClearPanel_Landed` calls it, just before
    /// `820CE840`): `82190AD8` on each meter keeps the longest time
    /// (`+56`) and zeroes the time (`+48`).
    pub fn record_longest(&mut self) {
        for m in [&mut self.manual, &mut self.grind, &mut self.lip, &mut self.skitch] {
            if m.time > m.max_time {
                m.max_time = m.time;
            }
            m.time = 0.0;
        }
    }

    /// Retail `820CE840`, from the script commands `ClearPanel_Landed`
    /// (`82124BA8`) and `ClearPanel_Bailed` (`82124EC0`), i.e. when a combo
    /// ends: every meter is reset (`821909A0`: cheese `+68`, `+72`, time
    /// `+48`, `+96`, instability time `+52`, lean `+60` and its speed `+64`
    /// to 0, `+92` to -1; the longest time `+56` stays) and no type runs
    /// (`+24`, `+28`). `821909A0` also runs `reset_balance_indicator` when
    /// the global flag `balance_check` is set (a debug display; global flags
    /// are not translated). Retail also clears SkaterState `+256`
    /// (untranslated, as in the update).
    pub fn reset_all(&mut self) {
        for m in [&mut self.manual, &mut self.grind, &mut self.skitch, &mut self.lip] {
            m.cheese = 0.0;
            m.buttons_live = false;
            m.time = 0.0;
            m.instable_time = 0.0;
            m.lean = 0.0;
            m.lean_speed = 0.0;
        }
        self.kind = 0;
        self.doing = false;
    }

    /// `82190A60` also hides the meter on screen (both modes).
    pub fn stop(&mut self) {
        for m in [&mut self.manual, &mut self.grind, &mut self.lip, &mut self.skitch] {
            m.button_a = 0;
            m.button_b = 0;
        }
        self.display.hide();
        self.kind = 0;
        self.doing = false;
    }

    /// The end of `82190F58` (from `8219179C`), after an update that did
    /// not fall off: while a type runs and the meter is not paused (`+97`,
    /// never set here: pausing is not translated), show the meter with
    /// value lean * -1/4096 (82004E3C); the manual (up/down) mode when both
    /// of the meter's buttons are Up or Down. `sides` is physics `+1908`,
    /// `+1909` ([`crate::CorePhysics::balance_sides`]).
    pub fn show_on_screen(&mut self, s: &Scripts, sides: [bool; 2]) {
        let k = qb_key;
        let kind = self.kind;
        let Some(m) = self.meter_mut(kind) else {
            return;
        };
        if m.button_a == 0 || m.button_b == 0 {
            return;
        }
        let up_down = [k("Up"), k("Down")];
        let manual = up_down.contains(&m.button_a) && up_down.contains(&m.button_b);
        let lean = m.lean;
        let Some(layout) = MeterLayout::from_scripts(s) else {
            return;
        };
        // FLAG_SKATER_LIPTRICK_CAM_REVERSED is set only by the retail
        // camera (820D1238), not translated: clear.
        self.display.set(&layout, true, lean * (-1.0 / 4096.0), manual, sides, false, lean);
    }

    /// Retail `82190F58` on the lip meter (`820F49D8` calls it while the
    /// type is Lip). `dt` is retail's frame time `[826E6B98]` (seconds).
    pub fn update_lip(&mut self, c: &mut BalanceCtx, input: &InputState, dt: f32) -> Option<OffMeter> {
        self.update(c, qb_key("Lip"), input, dt)
    }

    /// Retail `82190F58` on the meter of the running type (the ground update
    /// `820F6978` calls it for Manual / NoseManual / Flatland on the manual
    /// meter).
    pub fn update_running(&mut self, c: &mut BalanceCtx, input: &InputState, dt: f32) -> Option<OffMeter> {
        let kind = self.kind;
        self.update(c, kind, input, dt)
    }

    fn update(&mut self, c: &mut BalanceCtx, kind: u32, input: &InputState, dt: f32) -> Option<OffMeter> {
        let k = qb_key;
        let mut dt = dt;
        // `balance_hack_toggle` caps the step at 1/30 s (82000E50).
        if c.s.global_float("balance_hack_toggle") != 0.0 && dt > 1.0 / 30.0 {
            dt = 1.0 / 30.0;
        }
        let gravity = self.param(c, "Lean_Gravity_Stat");
        let rate = self.param(c, "Instable_Rate");
        let base = self.param(c, "Instable_Base");
        let lean_acc = self.param(c, "Lean_Acc");
        let min_speed = self.param(c, "Lean_Min_Speed");
        let rnd_speed = self.param(c, "Lean_Rnd_Speed");
        let bail = self.param(c, "Lean_Bail_Angle");
        let analog = c.s.physics_float("Use_New_Analog_Controls", false) != 0.0;
        let safe_period = c.s.global_float("BalanceSafeButtonPeriod") as i64;
        let ignore_period = c.s.global_float("BalanceIgnoreButtonPeriod") as i64;
        let held = |b: u32| {
            if b == k("Up") {
                input.up
            } else if b == k("Down") {
                input.down
            } else if b == k("Left") {
                input.left
            } else if b == k("Right") {
                input.right
            } else {
                false
            }
        };
        let now = c.now_ms;
        let m = self.meter_mut(kind)?;
        if m.button_a == 0 || m.button_b == 0 {
            return None;
        }
        m.time += dt;
        m.instable_time += dt;
        let instability = m.instable_time * rate + base;
        // 60 is the constant at 82001EB4.
        m.lean += m.lean * instability * gravity * dt * 60.0 + m.lean_speed * instability * dt * 60.0;

        let mut a = held(m.button_a);
        let mut b = held(m.button_b);
        // 1/128 (82000E00): the stick; up/down balances use its y.
        let stick_y = input.stick_back_raw / 128.0;
        let mut stick = input.stick_x_raw / 128.0;
        let vertical = [k("Up"), k("Down")].contains(&m.button_a) && [k("Up"), k("Down")].contains(&m.button_b);
        if vertical {
            stick = -stick_y;
        }
        if analog {
            // Dead zone 0.2 (82000D34), rescaled by 1.25 (820029B8).
            if stick > 0.0 {
                stick = ((stick - 0.2) * 1.25).max(0.0);
            } else if stick < 0.0 {
                stick = ((stick + 0.2) * 1.25).min(0.0);
            }
            stick = stick.clamp(-1.0, 1.0);
            if a && stick == 0.0 && stick_y == 0.0 {
                stick = 1.0;
                b = false;
            } else if b && stick == 0.0 && stick_y == 0.0 {
                stick = -1.0;
                a = false;
            } else if stick < -0.2 {
                b = true;
                a = false;
            } else if stick > 0.2 {
                a = true;
                b = false;
            } else {
                a = false;
                b = false;
            }
        }
        if !a && !b {
            m.buttons_live = true;
        }
        let elapsed = now - m.start_ms;
        if !m.buttons_live && elapsed > ignore_period {
            m.buttons_live = true;
        }
        let sign = |x: f32| if x >= 0.0 { 1.0 } else { -1.0 };
        if a && m.buttons_live {
            let mut f = 1.0;
            if m.lean < 0.0 && elapsed < safe_period {
                f = elapsed as f32 / safe_period as f32;
            }
            if analog {
                f *= stick.abs().sqrt();
            }
            m.lean_speed -= lean_acc * f * dt * 60.0;
        } else if b && m.buttons_live {
            let mut f = 1.0;
            if m.lean > 0.0 && elapsed < safe_period {
                f = elapsed as f32 / safe_period as f32;
            }
            if analog {
                f *= stick.abs().sqrt();
            }
            m.lean_speed += lean_acc * f * dt * 60.0;
        } else if m.lean_speed.abs() < min_speed {
            let r = (c.random)((rnd_speed as i32 | 1) as u32);
            m.lean_speed = (r + 1) as f32 * sign(m.lean_speed);
        } else {
            let r = (c.random)(50);
            // 0.01 is the constant at 82000D7C.
            m.lean_speed += r as f32 * sign(m.lean_speed) * 0.01;
        }
        if m.lean.abs() <= bail {
            return None;
        }
        let off = if m.lean > 0.0 { OffMeter::Top } else { OffMeter::Bottom };
        m.lean = 0.0;
        self.kind = 0;
        // Retail also clears SkaterState +256 (untranslated).
        Some(off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripts() -> Scripts {
        let k = qb_key;
        let stat = |v: f32| Value::Struct(vec![(0, Value::Pair(v, v))]);
        let lip = Value::Struct(vec![
            (k("cheese"), stat(3000.0)),
            (k("cheeseframes"), stat(180.0)),
            (k("lean_gravity_stat"), stat(0.02)),
            (k("instable_rate"), stat(0.45)),
            (k("instable_base"), stat(1.0)),
            (k("lean_min_speed"), stat(10.0)),
            (k("lean_rnd_speed"), stat(20.0)),
            (k("repeat_min"), stat(1.0)),
            (k("repeat_multiplier"), stat(1.0)),
            (k("lean_repeat_multiplier"), stat(1.0)),
            (k("lean_acc"), stat(10.0)),
            (k("lean_bail_angle"), stat(4000.0)),
        ]);
        Scripts::new(
            [
                (k("LipParams"), lip),
                (k("skater_physics"), Value::Struct(vec![(k("use_new_analog_controls"), Value::Int(1))])),
                (k("BalanceSafeButtonPeriod"), Value::Int(1000)),
                (k("BalanceIgnoreButtonPeriod"), Value::Int(0)),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn start(bal: &mut Balance, s: &Scripts, stats: &StatLevels) {
        let k = qb_key;
        let mut rnd = |_n: u32| 0;
        let mut c = BalanceCtx {
            s,
            stats,
            stat_context: StatContext::default(),
            on_bike: false,
            now_ms: 0,
            random: &mut rnd,
        };
        let p = p8_script::Params(vec![
            (k("ButtonA"), Value::Checksum(k("Right"))),
            (k("ButtonB"), Value::Checksum(k("Left"))),
            (k("Type"), Value::Checksum(k("Lip"))),
        ]);
        bal.do_balance_trick(&mut c, &p);
    }

    #[test]
    fn untouched_the_lip_meter_falls_off_one_side() {
        let s = scripts();
        let stats = StatLevels::with_default(10.0);
        let mut bal = Balance::default();
        start(&mut bal, &s, &stats);
        assert_eq!(bal.kind, qb_key("Lip"));
        assert_eq!(bal.lip.lean_speed, 1.0);
        let mut rnd = |_n: u32| 0;
        let mut off = None;
        for i in 0..600 {
            let mut c = BalanceCtx {
                s: &s,
                stats: &stats,
                stat_context: StatContext::default(),
                on_bike: false,
                now_ms: i * 16,
                random: &mut rnd,
            };
            off = bal.update_lip(&mut c, &InputState::default(), 1.0 / 60.0);
            if off.is_some() {
                break;
            }
        }
        assert_eq!(off, Some(OffMeter::Top));
        assert_eq!(bal.kind, 0);
    }

    #[test]
    fn cheese_wears_off_over_cheese_frames() {
        let s = scripts();
        let stats = StatLevels::with_default(10.0);
        let mut bal = Balance::default();
        bal.lip.cheese = 3000.0;
        let mut rnd = |_n: u32| 0;
        let c = BalanceCtx { s: &s, stats: &stats, stat_context: StatContext::default(), on_bike: false, now_ms: 0, random: &mut rnd };
        bal.wear_off_cheese(&c, 1.0 / 60.0);
        assert!((bal.lip.cheese - (3000.0 - 3000.0 / 180.0)).abs() < 0.01);
        for _ in 0..179 {
            bal.wear_off_cheese(&c, 1.0 / 60.0);
        }
        assert!(bal.lip.cheese <= 0.01);
    }

    #[test]
    fn combo_end_resets_the_cheese_and_lean() {
        let mut bal = Balance::default();
        bal.lip.cheese = 2000.0;
        bal.lip.lean = 1500.0;
        bal.lip.lean_speed = 30.0;
        bal.lip.max_time = 2.0;
        bal.lip.time = 3.0;
        bal.kind = qb_key("Lip");
        bal.record_longest();
        bal.reset_all();
        assert_eq!((bal.lip.cheese, bal.lip.lean, bal.lip.lean_speed, bal.kind), (0.0, 0.0, 0.0, 0));
        assert_eq!((bal.lip.max_time, bal.lip.time), (3.0, 0.0));
    }

    #[test]
    fn lip_rumble_follows_the_lean() {
        let mut g: std::collections::BTreeMap<u32, Value> = std::collections::BTreeMap::new();
        g.insert(qb_key("min_balance_vibration"), Value::Float(10.0));
        let s = Scripts::new(g);
        let mut bal = Balance::default();
        assert_eq!(bal.rumble_percent(&s, 0.0), None);
        bal.kind = qb_key("Lip");
        bal.lip.lean = -2048.0;
        // 2048 / 4096 * 100 + 10, speed ignored on a lip.
        assert_eq!(bal.rumble_percent(&s, 0.0), Some(60.0));
        bal.lip.lean = 4000.0;
        assert_eq!(bal.rumble_percent(&s, 0.0), Some(100.0));
    }

    #[test]
    fn stop_ends_every_meter() {
        let s = scripts();
        let stats = StatLevels::with_default(10.0);
        let mut bal = Balance::default();
        start(&mut bal, &s, &stats);
        bal.stop();
        assert_eq!(bal.kind, 0);
        assert_eq!(bal.lip.button_a, 0);
    }
}
