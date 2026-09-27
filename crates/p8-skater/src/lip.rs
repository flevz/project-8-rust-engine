//! Lip tricks: grabbing a coping rail from the air (`820FAAA8` ->
//! `820DCBE8` -> `820F8120` -> `820F44C0`), the lip state (`820F49D8`) and
//! `SkateInAble` (`820E55E0`).
//!
//! Grinds are not translated: a rail grab that is not a lip (`820F8120`'s
//! grind set-up, and `820F4108` for single-node rails) does nothing, and
//! the rail is then not remembered as ridden (`+1192`).
use crate::balance::{BalanceCtx, OffMeter};
use crate::core_physics::{CorePhysics, Event, State};
use crate::events::PhysicsEvents;
use crate::input::InputState;
use crate::rails::flag;
use crate::script::Scripts;
use crate::world::World;
use glam::Vec3;
use p8_formats::qb_key;

/// 0.0174533: degrees to radians (82000C10).
const DEG: f32 = 0.017453292;

impl CorePhysics {
    /// Retail `821E8508(n)`: a number in `0..n`. Its generator is not read;
    /// this is a plain LCG.
    pub fn random(&mut self, n: u32) -> u32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.rng >> 8) % n.max(1)
    }

    /// Retail `820FAAA8(0)`, once per frame after the state update: in the
    /// air with "Triangle" held, look for a rail crossed by this frame's move
    /// and take it.
    pub(crate) fn rail_check(
        &mut self,
        s: &Scripts,
        input: &InputState,
        world: &dyn World,
        events: &mut PhysicsEvents<'_>,
    ) {
        // SkaterState +208 (never set by translated code) would do instead
        // of the button.
        if self.state != State::Air || self.in_bail || self.flipping || self.no_rail_tricks || !input.triangle {
            return;
        }
        let window = s.global_float("Physics_Wallplant_Disallow_Grind_Duration") as i64;
        if self.time_ms.saturating_sub(self.last_wallplant_ms) < window {
            return;
        }
        let Some(rails) = world.rails() else { return };
        let snap = s.global_float("Rail_Max_Snap");
        let Some((node, point)) = rails.search(self.old_position, self.body.position, snap, None) else {
            // Retail then tries the rails of moving objects (not translated).
            return;
        };
        if !self.may_take_rail(node, rails, false) {
            return;
        }
        self.grab_rail(s, node, point, rails, events);
    }

    /// Retail `820DCBE8` on a normal level.
    fn may_take_rail(&mut self, node: usize, rails: &crate::rails::RailManager, force: bool) -> bool {
        let r = &rails.rails[node];
        let same = |cur: usize| cur == node || r.next == Some(cur) || r.prev == Some(cur);
        // In the air with SkaterState +192 and +240 (never set by translated
        // code) the current rail and its neighbours are refused.
        match self.rail {
            None => self.new_rail = true,
            Some(cur) if same(cur) => {}
            Some(_) => self.new_rail = true,
        }
        if !self.new_rail && self.time_ms - self.rail_left_ms <= self.rail_again_ms {
            return false;
        }
        if force {
            return true;
        }
        // State 4 (not translated) is refused too. Following a vert wall
        // on the way down (without SkaterState +208) refuses rails.
        if self.vert.tracking && self.body.velocity.y <= 0.0 {
            return false;
        }
        true
    }

    /// `820F8120` up to the lip check.
    fn grab_rail(
        &mut self,
        s: &Scripts,
        node: usize,
        point: Vec3,
        rails: &crate::rails::RailManager,
        events: &mut PhysicsEvents<'_>,
    ) {
        let r = &rails.rails[node];
        if r.next.is_none() && r.prev.is_none() {
            // 820F4108: single-node rails (not translated).
            return;
        }
        // If this is not a lip, the grind set-up follows in retail (not
        // translated).
        self.lip_entry(s, node, point, r, events);
    }

    /// Retail `820F44C0`: take the rail as a lip when the skater comes up it
    /// board-first against a steep ramp, then start script `LipTrick`.
    fn lip_entry(
        &mut self,
        s: &Scripts,
        node: usize,
        point: Vec3,
        rail: &crate::rails::Rail,
        events: &mut PhysicsEvents<'_>,
    ) -> bool {
        // The spine-transfer button (R2) refuses lips.
        if self.last_input.r2 {
            return false;
        }
        let allow = if rail.flags & flag::LIP_OVERRIDE != 0 { "LipAllowAngle_Override" } else { "LipAllowAngle" };
        let sin_allow = (s.global_float(allow) * DEG).sin();
        let sin_horizontal = (s.global_float("LipPlayerHorizontalAngle") * DEG).sin();
        let cos_ramp = (s.global_float("LipRampVertAngle") * DEG).cos();
        let up = self.body.up();
        let at = self.body.at();
        let facing = (at.y > 0.0 && !self.nollie) || (at.y < 0.0 && self.nollie);
        let ok = self.body.velocity.y > 0.0
            && up.y.abs() < sin_horizontal
            && facing
            && self.ground_normal.y.abs() < cos_ramp
            && self.body.row0().y.abs() < sin_allow;
        if !ok && !self.allow_lip_no_grind {
            return false;
        }
        self.body.velocity = Vec3::ZERO;
        self.vert.in_vert_air = false;
        self.vert.tracking = false;
        self.vert.on_vert_ground = false;
        self.set_break_window(false);
        self.balance.stop();
        self.lip_pos = self.body.position;
        if self.nollie {
            if self.state == State::Air {
                // `FlipAndRotate` (820FDAF0 -> 820D9008): turn round about
                // up; retail also toggles +2024 and flips the display.
                self.body.matrix.z_axis = -self.body.matrix.z_axis;
                self.body.matrix.x_axis = -self.body.matrix.x_axis;
                self.matrix_32 = self.body.matrix;
                self.flipped = !self.flipped;
            }
            // 821163E0: out of nollie (retail sends SkaterExitNollie).
            self.nollie = false;
        }
        self.set_state(State::Lip);
        self.body.position = point;
        let n = Vec3::new(self.ground_normal.x, 0.0, self.ground_normal.z).normalize_or_zero();
        self.body.matrix.y_axis = n;
        self.body.matrix.z_axis = Vec3::Y;
        self.body.matrix.x_axis = Vec3::new(-n.z, 0.0, n.x);
        self.matrix_32 = self.body.matrix;
        // The rail is selected before lip setup; the terrain copy follows
        // the synchronous goto/update (`820F44C0`, research notes section 19).
        self.rail = Some(node);
        events.goto(self, qb_key("LipTrick"));
        self.terrain = rail.terrain;
        true
    }

    /// Retail `820F49D8`, the lip state's update (moving platforms and
    /// scoring are not translated).
    pub(crate) fn lip_update(&mut self, s: &Scripts, input: &InputState, events: &mut PhysicsEvents<'_>) {
        self.vert.over_ground = false;
        if self.balance.kind == qb_key("Lip") {
            let dt = self.dt;
            let (stats, ctx, on_bike, now) = (self.stats.clone(), self.stat_context, self.on_bike, self.time_ms);
            let mut rng = self.rng;
            let mut random = |n: u32| {
                rng = rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (rng >> 8) % n.max(1)
            };
            let mut c = BalanceCtx { s, stats: &stats, stat_context: ctx, on_bike, now_ms: now, random: &mut random };
            let off = self.balance.update_lip(&mut c, input, dt);
            self.rng = rng;
            match off {
                Some(OffMeter::Top) => events.emit(self, Event::OffMeterTop),
                Some(OffMeter::Bottom) => events.emit(self, Event::OffMeterBottom),
                None => {}
            }
        }
        self.update_crouch(input);
        if self.ollie_trigger(input) {
            // Retail also sets SkaterState +248 and runs triggers 132, 144.
            events.emit(self, Event::Ollied);
        }
    }

    /// Script command `SkateInAble` (`820EB538` -> `820E55E0`): is there vert
    /// below to skate back into? With `Lip`: a feeler from `up` behind the
    /// point below the board down to it, then a longer one straight down.
    /// Without: from the side (`Left` picks the side).
    pub(crate) fn skate_in_able(&self, s: &Scripts, world: &dyn World, left: bool, lip: bool) -> bool {
        let pos = self.body.position;
        let up = self.body.up();
        let is_vert = |a: Vec3, b: Vec3| world.feeler(a, b, 0x10, 0).map(|h| h.flags & 0x8 != 0);
        if lip {
            let h = s.global_float("SkateInAble_LipHorizOffset");
            let d = s.global_float("SkateInAble_LipDownOffset");
            let end = Vec3::new(pos.x, pos.y - d, pos.z);
            if let Some(v) = is_vert(end - up * h, end) {
                return v;
            }
            let h2 = s.global_float("SkateInAble_LipExtraCheckHorizOffset");
            let d2 = s.global_float("SkateInAble_LipExtraCheckDownOffset");
            let start = pos - up * h2;
            return is_vert(start, start - Vec3::Y * d2).unwrap_or(false);
        }
        let row0 = self.body.row0();
        let h = s.global_float("SkateInAble_HorizOffset");
        let d = s.global_float("SkateInAble_DownOffset");
        let side = pos + row0 * h;
        let low = Vec3::new(side.x, side.y - d, side.z);
        // 2 is the constant at 82000D78.
        let across = low - row0 * (h * 2.0);
        let (a, b) = if left { (low, across) } else { (across, low) };
        if let Some(true) = is_vert(a, b) {
            return true;
        }
        let h2 = s.global_float("SkateInAble_ExtraCheckHorizOffset");
        let d2 = s.global_float("SkateInAble_ExtraCheckDownOffset");
        let start = if left { pos + row0 * h2 } else { pos - row0 * h2 };
        is_vert(start, start - Vec3::Y * d2).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ScriptAction;
    use crate::rails::Rail;

    #[test]
    fn lip_script_runs_after_placement_and_before_the_terrain_copy() {
        let s = crate::core_physics::tests::scripts();
        let mut p = CorePhysics::new(&s);
        p.set_state(State::Air);
        p.allow_lip_no_grind = true;
        p.ground_normal = Vec3::Z;
        p.body.position = Vec3::X;
        p.terrain = 2;
        let rail = Rail {
            min: Vec3::ZERO,
            max: Vec3::X,
            pos: Vec3::ZERO,
            flags: flag::ACTIVE,
            node: 0,
            terrain: 9,
            next: Some(1),
            prev: None,
        };
        let mut calls = 0;
        let mut dispatch = |p: &mut CorePhysics, action| {
            assert!(matches!(action, ScriptAction::Goto(n) if n == qb_key("LipTrick")));
            assert_eq!(p.state, State::Lip);
            assert_eq!(p.rail, Some(0));
            assert_eq!(p.body.position, Vec3::Y);
            assert_eq!(p.terrain, 2);
            // An immediate script exit must survive the remaining lip setup.
            p.set_state(State::Air);
            calls += 1;
            Vec::new()
        };
        let mut events = PhysicsEvents::new(&mut dispatch);
        assert!(p.lip_entry(&s, 0, Vec3::Y, &rail, &mut events));
        assert!(events.log.is_empty());
        assert_eq!(calls, 1);
        assert_eq!(p.state, State::Air);
        assert_eq!(p.body.position, Vec3::X);
        assert_eq!(p.terrain, 9);
        assert_eq!(p.script_goto, None);
    }
}
