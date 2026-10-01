//! Grinds: taking a rail that is not a lip (the rest of `820F8120`), the
//! bad-ledge check (`820EAB68`), the rail speed boost (`820DD5F0`), the
//! pending grind trick (`82124048`) and the default grind script
//! (`820DDDC8`).
//!
//! Not translated (each also noted where retail does it): the rail node
//! TriggerScripts (`820F0550`), moving-object rails (`82193630` uses the
//! record as placed), the SkaterState `+208` branch of `820F8120`
//! (820F8168..820F82B0; nothing translated sets `+208`), the wallride
//! state's re-orientation (820F8714..820F87C0, state 2 is not translated),
//! the walking rail grab's arguments (`820FA850`), bikes (`+1564` is
//! always false) and created tricks (`createatrick`).
use crate::core_physics::{CorePhysics, State};
use crate::rails::RailManager;
use crate::script::Scripts;
use glam::Vec3;
use p8_formats::qb::Value;
use p8_formats::qb_key;

/// What the grind keeps on the physics component.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grind {
    /// `+1220`: which way along the rail: 1 toward the next record, -1
    /// toward the previous one.
    pub dir_sign: f32,
    /// `+1224`: ground found by the bad-ledge check on the side the jump
    /// came from.
    pub ledge_side: bool,
    /// `+1504`: where the skater was relative to the grab point.
    pub offset: Vec3,
    /// `+1389`: R2 was held at the grab (`ShouldRunStallScript`).
    pub stall_script: bool,
    /// `+1392`: the velocity when R2 was held at the grab.
    pub stall_velocity: Vec3,
    /// `+1948`, `+1949`: the bad-ledge check found ground on the left /
    /// right of the rail (`820EAB68`; animinfo `+176` / `+184` read them).
    pub ledge: [bool; 2],
    /// `+2220`: `SetGrindTweak`.
    pub tweak: i32,
    /// `+1528`, `+1532`: `SetBackwardsGrindValues` `ScriptName` and `Anim`.
    pub backwards_script: u32,
    pub backwards_anim: u32,
    /// `+2160`: frames the stall has been on a steep rail (`820F4DE8`).
    pub slip_count: i32,
    /// `+2164`, `+2165`: the stall slipped into a grind (and the grind is
    /// to go the other way), read by the rail update.
    pub slip_pending: bool,
    pub slip_reverse: bool,
    /// `+1548`: on a single-node rail (Natas spin, `820F4108`).
    pub natas: bool,
    /// `+2608`: R1 was held at the single-node grab (the spin's direction).
    pub natas_r1: bool,
    /// `+2612`: when the single-node rail was taken (ms).
    pub natas_ms: i64,
    /// SkaterState `+240`: ollied off a rail (set by the stall and rail
    /// updates, cleared when the air state ends, `820D73C0`); with `+192`
    /// it refuses the same rail in the air (`820DCBE8`).
    pub ollied_off_rail: bool,
}

/// A trick to go to on the skater's script with its parameters, run now
/// (`82226588` + `8220F8F0`).
#[derive(Clone, Debug, PartialEq)]
pub struct Goto {
    pub script: u32,
    pub params: p8_script::Params,
}

impl CorePhysics {
    /// `820F8120` after the lip check, for a rail grab from the air
    /// (`820FAAA8` passes 0 for the last two arguments, so the lip check
    /// runs and the walking branch 820F8800..820F88F8 is skipped).
    pub(crate) fn grind_entry(&mut self, s: &Scripts, node: usize, point: Vec3, rails: &RailManager, world: &dyn crate::World) {
        let rail = &rails.rails[node];
        // 820F82BC..820F82CC: the velocity at the start (stack +176, read by
        // the R2 branch).
        let grab_velocity = self.body.velocity;
        // 820F8338..820F8348: 820E53D8(terrain of the record).
        self.terrain = rail.terrain;
        // 820F834C..820F8400: the rail's direction (82193630 gives a
        // record's position; moving rails are not translated).
        let Some(next) = rail.next else { return };
        let dir = (rails.rails[next].pos - rail.pos).normalize_or_zero();
        // 820F8404..820F844C: a feeler from the skater to the grab point
        // (820E5048(16, 0)); a hit further than 0.15 (0.0225 = 0.15^2,
        // 82002ACC) from the point refuses the rail.
        if let Some(hit) = world.feeler(self.body.position, point, 0x10, 0)
            && (point - hit.point).length_squared() > 0.0225
        {
            return;
        }
        // 820F8450..820F8478: SkaterState +192 (acid drop) adds the drop's
        // carry +2768.
        if self.transfer.flag_192 {
            self.body.velocity += self.transfer.acid_carry;
        }
        // 820F847C..820F8500: no horizontal speed: take the facing's.
        let at = self.body.at();
        let v = &mut self.body.velocity;
        if v.x == 0.0 && v.z == 0.0 {
            v.x = at.x;
            v.z = at.z;
        }
        // 820F8504..820F8590: faster than 2.5 (6.25 at 82002AC8)
        // horizontally: made horizontal keeping its length (821EDB50 with
        // up).
        if v.x * v.x + v.z * v.z > 6.25 {
            *v = crate::core_physics::project_keep_length(*v, Vec3::Y);
        }
        // 820F8594..820F8644: which way along the rail (+1220); straight
        // across picks at random (821E8508(2): 0 -> -1).
        let along = dir.dot(self.body.velocity.normalize_or_zero());
        let sign = if along == 0.0 {
            if self.random(2) == 0 { -1.0 } else { 1.0 }
        } else if along < 0.0 {
            -1.0
        } else {
            1.0
        };
        self.grind.dir_sign = sign;
        // 820F8648..820F8698: grabbed exactly at the far end of the line
        // when heading off it: refused.
        let end = if sign < 0.0 {
            rail.prev.is_none().then_some(node)
        } else {
            rails.rails[next].next.is_none().then_some(next)
        };
        if let Some(end) = end
            && rails.rails[end].pos == point
        {
            return;
        }
        // 820F869C..820F86BC: +1504.
        self.grind.offset = self.body.position - point;
        // 820F86C0..820F8704: out of vert air (+56) and off the vert wall
        // (+64); their time stamps +60 / +68 are not kept.
        self.vert.in_vert_air = false;
        self.vert.tracking = false;
        // 820F870C: +1380.
        self.transfer.retry = false;
        // 820F8714..820F87C0: from state 2 (wallride, not translated) the
        // skater is first stood up straight (820D7648 with up).
        // 820F87C4..820F87CC.
        self.set_state(State::Rail);
        // 820F87D0..820F87E4: the lean (+1912) cleared and its display
        // reset (820DEB20, not drawn).
        if self.lean_degrees != 0.0 {
            self.lean_degrees = 0.0;
        }
        // 820F87E8..820F87FC: the velocity along the rail (821ED7D0),
        // except that its height is put back at 820F88FC..820F8934.
        let vy = self.body.velocity.y;
        self.body.velocity = dir * self.body.velocity.dot(dir);
        self.body.velocity.y = vy;
        // 820F8938..820F895C: the rail boost along the way we go.
        self.rail_boost(s, dir * self.grind.dir_sign);
        // 820F8960..820F8990: +2024 cleared; the direction we go.
        self.rail_backwards = false;
        let dir = if self.grind.dir_sign < 0.0 { -dir } else { dir };
        // 820F8994..820F89CC: which side of the rail the jump started
        // (+2000).
        let d = point - self.jump_start;
        let from_left = dir.x * d.z - dir.z * d.x < 0.0;
        // 820F89D0..820F89F8.
        self.bad_ledge_check(s, dir, point, world);
        // 820F89FC..820F8A40: +1224.
        self.grind.ledge_side = (self.grind.ledge[0] && !from_left) || (self.grind.ledge[1] && from_left);
        // 820F8A44..820F8AC8: along the board (|dir . row0| <
        // Rail_Tolerance) or across it, and backwards (+2024).
        let tolerance = s.global_float("Rail_Tolerance");
        let across = dir.dot(self.body.row0());
        let along_board = across.abs() < tolerance;
        let backwards = if along_board {
            dir.dot(self.body.at()) < 0.0
        } else {
            (if self.flipped { across } else { -across }) < 0.0
        };
        if backwards {
            self.rail_backwards = true;
        }
        // 820F8ACC..820F8AF8: the pending grind trick (trick +2724 cleared
        // first: its reader is not translated).
        let flipped = self.flipped;
        if let Some(goto) = self.pending_grind_trick(s, from_left, along_board, self.rail_backwards, flipped) {
            self.trick_goto = Some(goto);
        } else {
            // 820F8AFC..820F8B40: the display matrix.
            self.matrix_32 = self.body.matrix;
            let input = self.last_input;
            // 820F8B44..820F8B4C: R2 (Input +288).
            let (stall, backwards) = if !input.r2 {
                // 820F8B50..820F8B78: trigger 8200 (not translated); +1389.
                self.grind.stall_script = false;
                (false, self.rail_backwards)
            } else {
                // 820F8B7C..820F8C24: trigger 0x20008 (not translated);
                // moving across the rail (the velocity at the start of the
                // grab, flat) -> state 6.
                let flat = Vec3::new(grab_velocity.x, 0.0, grab_velocity.z).normalize_or_zero();
                if dir.dot(flat).abs() < tolerance {
                    self.set_state(State::Stall);
                }
                // 820F8C28..820F8C90.
                self.grind.stall_script = true;
                self.grind.stall_velocity = self.body.velocity;
                let mut b = self.rail_backwards;
                if self.nollie && along > -0.5 && along < 0.5 {
                    b = !b;
                }
                (true, b)
            };
            // 820F8B5C..820F8B6C / 820F8C94..820F8CA4: the d-pad direction.
            let dpad = crate::trick::dpad_direction(&input);
            // 820F8CB0..820F8CC8.
            self.default_grind(s, dpad, from_left, along_board, backwards, flipped, stall);
        }
        // 820F8CCC..820F8CD8: +1200 (and the rail, +1192, set by the
        // caller at 820F8150).
        self.rail_left_ms = self.time_ms;
    }

    /// Retail `820DD5F0`: the speed boost onto a rail, along `dir`, up to
    /// `Rail_Boost_Threshold`; going up, a second boost scaled by the
    /// slope (2 * dir.y, at most 1) up to `Rail_Boost_Threshold_Uphill`.
    pub(crate) fn rail_boost(&mut self, s: &Scripts, dir: Vec3) {
        // 820DD6CC..820DD7C0 (and 820DD8A8..820DD99C uphill).
        let push = |boost: f32, threshold: f32, v: &mut Vec3| {
            if v.length() < threshold {
                *v += dir * boost;
                if v.length() > threshold {
                    *v = v.normalize_or_zero() * threshold;
                }
            }
        };
        // 822162F0: global floats.
        let boost = s.global_float("Rail_Speed_Boost");
        let threshold = s.global_float("Rail_Boost_Threshold");
        push(boost, threshold, &mut self.body.velocity);
        // 820DD7C4..820DD9A0.
        if dir.y > 0.0 {
            let boost = s.global_float("Rail_Speed_Boost_Uphill");
            let threshold = s.global_float("Rail_Boost_Threshold_Uphill");
            // 2 (82000D78), 1 (82000C14).
            let f = (dir.y * 2.0).min(1.0);
            push(f * boost, threshold, &mut self.body.velocity);
        }
    }

    /// Retail `820EAB68`: is there ground beside the rail? Feelers from
    /// `Physics_Ground_Snap_Up` above to `Rail_Bad_Ledge_Drop_Down_Dist`
    /// below the point, `Rail_Bad_Ledge_Side_Dist` to each side; a hit
    /// facing up (normal y > 0.5) counts. When both sides hit, only the
    /// nearer hit along its feeler stays (`+336`; taken as the distance
    /// along the feeler, INFERRED: both feelers are the same length and
    /// straight down, so the higher ground wins either way).
    fn bad_ledge_check(&mut self, s: &Scripts, dir: Vec3, point: Vec3, world: &dyn crate::World) {
        // 820EAB7C..820EAC10: a zero direction does nothing.
        if dir.length() == 0.0 {
            return;
        }
        let dir = dir.normalize();
        // 820EAC14..820EAD7C.
        let up = s.physics_float("Physics_Ground_Snap_Up", self.on_bike);
        let down = s.global_float("Rail_Bad_Ledge_Drop_Down_Dist");
        let side = Vec3::new(dir.z, 0.0, -dir.x) * s.global_float("Rail_Bad_Ledge_Side_Dist");
        let start = point + Vec3::new(0.0, up, 0.0) + side;
        let end = point + Vec3::new(0.0, -down, 0.0) + side;
        // 820EAD80..820EADFC: the first side; 0.5 at 82000BE8.
        let first = world.feeler(start, end, 0x10, 0).filter(|h| h.normal.y > 0.5);
        self.grind.ledge[0] = first.is_some();
        // 820EAE00..820EAE7C: the other side (2 at 82000D78).
        let (start2, end2) = (start - side * 2.0, end - side * 2.0);
        let second = world.feeler(start2, end2, 0x10, 0).filter(|h| h.normal.y > 0.5);
        self.grind.ledge[1] = second.is_some();
        // 820EAE80..820EAEB8.
        if let (Some(a), Some(b)) = (first, second) {
            let ta = (a.point - start).length();
            let tb = (b.point - start2).length();
            if ta > tb {
                self.grind.ledge[0] = false;
            } else {
                self.grind.ledge[1] = false;
            }
        }
    }

    /// Retail `82124048` (trick component): run the pending grind trick
    /// (`+4912`, list `+4916`, index `+4920`), if any. The entry names its
    /// scripts in `scripts` (an array) or `Template` (an array of suffixes)
    /// plus `Prefix`, else through its `TrickSlot`; one of 16 is picked by
    /// `from_left` (1), `along_board` (2), `backwards` (4), `flipped` (8).
    fn pending_grind_trick(&mut self, s: &Scripts, from_left: bool, along_board: bool, backwards: bool, flipped: bool) -> Option<Goto> {
        let k = qb_key;
        // 82124068..82124080.
        let pending = self.tricks.grind_pending.take()?;
        // 82124084..8212408C: 82217048 (the global list).
        let Some(Value::Array(entries)) = s.globals.get(&pending.list) else { return None };
        // 82124090..82124094.
        let entry = entries.get(pending.index)?;
        let entry = Value::Struct(crate::trick::expand_struct(entry, &s.globals));
        let member = |v: &Value, name: &str| crate::trick::struct_member(v, name).cloned();
        let (mut scripts, mut template, mut prefix) = (member(&entry, "scripts"), member(&entry, "Template"), member(&entry, "Prefix"));
        // 82124110..821242E4: neither -> the trick slot through the mapping.
        if scripts.is_none() && template.is_none() {
            let slot = crate::trick::checksum_of(member(&entry, "TrickSlot").as_ref());
            if slot != 0 {
                match self.tricks.mapping.iter().rev().find(|(n, _)| *n == slot).map(|(_, v)| v.clone()) {
                    // 82124184..8212426C: an integer is a created trick
                    // (`createatrick`, not translated).
                    Some(Value::Int(_)) => return None,
                    // 82124278..821242E4.
                    Some(Value::Checksum(c)) => {
                        if let Some(t) = s.globals.get(&c) {
                            scripts = member(t, "scripts");
                            template = member(t, "Template");
                            prefix = member(t, "Prefix");
                        }
                    }
                    _ => {}
                }
            }
        }
        // 821242E8..8212432C.
        let index = from_left as usize | (along_board as usize) << 1 | (backwards as usize) << 2 | (flipped as usize) << 3;
        // 8212432C..82124364.
        let resolve = |v: Option<Value>| match v {
            Some(Value::Checksum(c)) => s.globals.get(&c).cloned(),
            other => other,
        };
        let script = if let Some(Value::Array(list)) = resolve(scripts) {
            crate::trick::checksum_of(list.get(index))
        } else {
            let Some(Value::Array(list)) = resolve(template) else { return None };
            let suffix = match list.get(index) {
                Some(Value::String(t)) => t.clone(),
                _ => return None,
            };
            let prefix = match prefix {
                Some(Value::String(p)) => p,
                _ => String::new(),
            };
            // "%s%s" (82003470), then its checksum (8220CF88).
            k(&format!("{prefix}{suffix}"))
        };
        if script == 0 {
            return None;
        }
        // 82124368..82124390: `Params`.
        let params = match member(&entry, "Params") {
            Some(Value::Struct(items)) => p8_script::Params(items),
            _ => p8_script::Params::default(),
        };
        // 82124394..821243B0: +2220 cleared for the skater.
        self.grind.tweak = 0;
        // 821243B4..821243DC: trick +2724 = +4932 (not translated), the
        // goto, SkaterState +264 (doing a trick). 821243E0..8212440C: the
        // trick count +5368 and "skaterstartingrun" past 3 (not translated).
        self.doing_trick = true;
        Some(Goto { script, params })
    }

    /// Retail `820DDDC8`: go to the grind script for the d-pad direction
    /// (`GrindTrickList`, or `StallTrickList` with R2), picked by
    /// `from_left` (1), `along_board` (2), `backwards` (4) and not
    /// `flipped` (8). In nollie, a grind along the board turns the skater
    /// round first (FlipAndRotate) with `from_left` and `backwards`
    /// inverted; nollie then ends.
    #[allow(clippy::too_many_arguments)]
    fn default_grind(&mut self, s: &Scripts, dpad: usize, from_left: bool, along_board: bool, backwards: bool, flipped: bool, stall: bool) {
        // 820DDDE8..820DDE24: the skater's script is made if missing (it
        // always exists here). 820DDE28..820DDEB0: direction 0..8 (above
        // 8: 0).
        let dpad = if dpad > 8 { 0 } else { dpad };
        // 820DDEB4..820DDF04 (bikes not translated).
        let list = if stall { qb_key("StallTrickList") } else { qb_key("GrindTrickList") };
        let (mut from_left, mut backwards) = (from_left, backwards);
        // 820DDF10..820DDF8C: 82116500 nollie.
        if self.nollie {
            if along_board {
                from_left = !from_left;
                backwards = !backwards;
                // 820FDAF0: FlipAndRotate.
                self.turn_round();
                self.flip_stance();
            }
            // 821163E0.
            self.nollie = false;
        }
        // 820DDF90..820DDFDC.
        let index = from_left as usize | (along_board as usize) << 1 | (backwards as usize) << 2 | (!flipped as usize) << 3;
        // 820DDFE0..820DE014: +2220 cleared, then the goto and a script
        // update. 820DE018..820DE020: SkaterState +264.
        self.grind.tweak = 0;
        // 82217048 then 82203D40 twice: the list (StallTrickList names
        // GrindTrickList), its direction's array, the entry.
        let mut v = s.globals.get(&list).cloned();
        while let Some(Value::Checksum(c)) = v {
            v = s.globals.get(&c).cloned();
        }
        let Some(Value::Array(dirs)) = v else { return };
        let Some(Value::Array(scripts)) = dirs.get(dpad) else { return };
        let script = crate::trick::checksum_of(scripts.get(index));
        if script == 0 {
            return;
        }
        self.trick_goto = Some(Goto { script, params: p8_script::Params::default() });
        self.doing_trick = true;
    }
}

impl CorePhysics {
    /// Retail `820F4108`: take a single-node rail (Natas spin): stand on
    /// the node, state 4 (whose update spins the skater, `820DBD40`), and
    /// event `PointRailSpin` (script `pointrailspin`). False when a wall is
    /// in the way.
    pub(crate) fn single_node_rail(&mut self, input_r1: bool, node: usize, point: Vec3, rails: &RailManager, world: &dyn crate::World) -> Option<crate::core_physics::Event> {
        // 820F411C..820F4144.
        self.grind.natas = true;
        self.grind.natas_r1 = input_r1;
        // 820F4148..820F41C0: the feeler from the skater to the node
        // (820E5048(16, 0)); 0.0225 at 82002ACC.
        if let Some(hit) = world.feeler(self.body.position, point, 0x10, 0)
            && (point - hit.point).length_squared() > 0.0225
        {
            return None;
        }
        // 820F41C4..820F4224.
        self.body.position = point;
        self.vert.in_vert_air = false;
        self.vert.tracking = false;
        self.transfer.retry = false;
        // 820F4228..820F4298: stood up straight (820D7648 with up), the
        // display matrix copied.
        self.orient_to_ground(Vec3::Y);
        self.matrix_32 = self.body.matrix;
        // 820F42A0..820F42E4.
        self.set_state(State::Rail);
        // 820F42E8..820F4324: the terrain (82116930; the sound, motion and
        // particle copies are not kept).
        self.terrain = rails.rails[node].terrain;
        // 820F4328..820F43C8.
        self.body.velocity = Vec3::ZERO;
        self.rail_left_ms = self.time_ms;
        // 820F43CC..820F4478: SkaterState +136, +192, +144, +200 cleared
        // (+152 is the bike's).
        self.clear_rail_flags();
        // 820F447C..820F44B0: event PointRailSpin (82225758), trigger
        // 0x1008 (820F0550, not translated), +2612.
        self.grind.natas_ms = self.time_ms;
        Some(crate::core_physics::Event::PointRailSpin)
    }

    /// SkaterState `+136`, `+192`, `+144` and `+200` cleared (with their
    /// stamps), as `820F4108` and `820F4DE8` do.
    pub(crate) fn clear_rail_flags(&mut self) {
        self.set_transfer(false);
        self.set_flag_192(false);
        self.vert.over_ground = false;
        self.set_no_acid_drop(false);
    }

    /// Retail `820DD070`: ease the grab offset (`+1504`, skater minus rail
    /// point) toward zero, its height at `Snap_To_Rail_Vert_Below_Speed`
    /// (below the rail) or `Snap_To_Rail_Vert_Above_Speed`, its flat part
    /// at `Snap_To_Rail_Horiz_Speed`, each in units per 60th of a second
    /// (60 at 82001EB4, times the frame time `[826E6B98]`).
    pub(crate) fn rail_snap_ease(&mut self, s: &Scripts) {
        let off = self.grind.offset;
        // 820DD084..820DD0EC: no offset, nothing to do (retail compares
        // with (0,0,0,1) and stores w = 0, so the test is never true and a
        // zero offset just goes through unchanged).
        if off == Vec3::ZERO {
            return;
        }
        // 820DD0F0..820DD128.
        self.body.position -= off;
        let shrink = |v: Vec3, step: f32| {
            let len = v.length();
            if len > step && len != 0.0 { v - v.normalize() * step } else { Vec3::ZERO }
        };
        let mut vert = Vec3::new(0.0, off.y, 0.0);
        let mut flat = Vec3::new(off.x, 0.0, off.z);
        // 820DD12C..820DD3C8.
        if vert.length() > 0.0 {
            let name = if off.y < 0.0 { "Snap_To_Rail_Vert_Below_Speed" } else { "Snap_To_Rail_Vert_Above_Speed" };
            vert = shrink(vert, self.dt * 60.0 * s.global_float(name));
        }
        // 820DD3CC..820DD58C.
        if flat.length() > 0.0 {
            flat = shrink(flat, self.dt * 60.0 * s.global_float("Snap_To_Rail_Horiz_Speed"));
        }
        // 820DD590..820DD5DC.
        self.grind.offset = vert + flat;
        self.body.position += self.grind.offset;
    }

    /// Retail `820F4DE8`, the stall update (state 6): the skater held still
    /// on the rail, lined up with it, with the grind balance meter. On a
    /// rail steeper than about 18 degrees (up.y < 0.95) it slips into a
    /// grind after 11 frames.
    ///
    /// Not translated: moving platforms (820F4EC0..820F5148), the rail node
    /// TriggerScripts on the ollie, `+2208` and `+2552` (cleared,
    /// 820F4EBC..820F4ED0; their readers are not translated), the
    /// `CHEAT_PERFECT_RAIL` flag write (no effect), the `rail_highlights`
    /// debug lines (820F532C..820F5774), the score per frame
    /// (820F5D48..820F5D80) and the trick-object list (820F5D84..820F5DA8).
    pub(crate) fn stall_update(&mut self, s: &Scripts, input: &crate::InputState, world: &dyn crate::World) -> Vec<crate::core_physics::Event> {
        use crate::core_physics::Event;
        let mut events = Vec::new();
        // 820F4E08..820F4EB8.
        self.clear_rail_flags();
        // 820F514C.
        self.rail_snap_ease(s);
        // 820F5154..820F5190: +1208 (the int part), SkaterState +88.
        self.rail_again_ms = s.global_float("Rail_minimum_rerail_time") as i64;
        self.new_rail = false;
        if self.ollie_trigger(input) {
            // 820F5194..820F5270.
            events.push(Event::Ollied);
            self.rail_again_ms = s.global_float("Rail_jump_rerail_time") as i64;
            self.grind.ollied_off_rail = true;
            // 0.0025 at 820027D0.
            self.body.position.y += 0.0025;
        } else {
            // 820F5274..820F52A8: the grind meter (then 820F52B0..820F5300,
            // the CHEAT_PERFECT_RAIL flag write, not translated: no effect).
            if self.balance.kind == qb_key("Grind") || self.balance.kind == qb_key("Slide") {
                self.update_balance_sides(s, world);
                self.run_balance_meter(s, input, &mut events);
            }
            // 820F5304..820F577C: the end of the line: nothing more.
            let Some(rails) = world.rails() else { return events };
            let Some(node) = self.rail else { return events };
            let Some(next) = rails.rails[node].next else { return events };
            // 820F5780..820F57DC.
            self.body.velocity = Vec3::ZERO;
            if self.state == State::Stall {
                // 820F57E0..820F58B4.
                let dir = (rails.rails[next].pos - rails.rails[node].pos).normalize_or_zero();
                self.rail_left_ms = self.time_ms;
                let sign = if dir.dot(self.body.at()) < 0.0 { -1.0 } else { 1.0 };
                self.grind.dir_sign = sign;
                // 820F58B8..820F5A60: the skater's matrix along the rail.
                let along = dir * sign;
                let at = along.normalize_or_zero();
                let row0 = Vec3::new(at.z, 0.0, -at.x).normalize_or_zero();
                let up = at.cross(row0).normalize_or_zero();
                self.body.matrix.x_axis = row0;
                self.body.matrix.y_axis = up;
                self.body.matrix.z_axis = at;
                // 820F5A00..820F5BC4: the display matrix eases toward it
                // (0.3 at 82000BF0, 0.125 at 82002414).
                let m = &mut self.matrix_32;
                m.z_axis = (m.z_axis + (along - m.z_axis) * 0.3).normalize_or_zero();
                m.y_axis = (m.y_axis + (up - m.y_axis) * 0.125).normalize_or_zero();
                m.x_axis = m.y_axis.cross(m.z_axis).normalize_or_zero();
                // 820F5BC8..820F5C1C.
                self.body.velocity = Vec3::ZERO;
                let under = self.body.position - self.grind.offset;
                self.bad_ledge_check(s, along, under, world);
            }
        }
        // 820F5C20..820F5C34.
        self.ground_normal = self.body.up();
        // 820F5C38..820F5D44: 0.95 is a double at 82002AF0.
        if (self.ground_normal.y as f64) < 0.95 {
            if self.grind.slip_count < 10 {
                self.grind.slip_count += 1;
            } else {
                self.set_state(State::Rail);
                self.grind.slip_count = 0;
                self.grind.slip_pending = true;
                self.grind.slip_reverse = false;
                let up = self.ground_normal;
                let at = self.body.at();
                let fu = Vec3::new(up.x, 0.0, up.z).normalize_or_zero();
                let fa = Vec3::new(at.x, 0.0, at.z).normalize_or_zero();
                if fa.dot(fu) < 0.0 && !self.on_bike {
                    self.grind.slip_reverse = true;
                }
            }
        }
        events
    }
}

/// 0.0174533: degrees to radians (82000C10).
const DEG: f32 = 0.017453292;

impl CorePhysics {
    /// Retail `820F8CF0`, the rail update (state 4): gravity along the
    /// rail, moving along it and on to the next segment, the grind meter,
    /// the ollie off, and falling off ("OffRail").
    ///
    /// Not translated: moving platforms and moving rails
    /// (820F8DDC..820F9050), the rail node TriggerScripts (820F0550 with
    /// 20, 8208, 8200, 18), `+2208` / `+2552` (cleared at 820F8DC4; their
    /// readers are not translated), the `CHEAT_PERFECT_RAIL` flag write (no
    /// effect), the collision cache box (`820E5158`), the created-park
    /// branches (`82194D20` is true on a normal level; 820FA3A8..820FA4CC),
    /// the score per frame (820FA690..820FA6C4) and the trick-object list
    /// (820FA6C8..820FA6EC).
    pub(crate) fn rail_update(&mut self, s: &Scripts, input: &crate::InputState, world: &dyn crate::World) -> Vec<crate::core_physics::Event> {
        use crate::core_physics::Event;
        use crate::rails::flag;
        let mut events = Vec::new();
        // 820F8D10..820F8DC0.
        self.clear_rail_flags();
        // 820F9054..820F9094.
        self.rail_again_ms = s.global_float("Rail_minimum_rerail_time") as i64;
        self.new_rail = false;
        // 820F9098..820F9280: the ollie off.
        if self.ollie_trigger(input) {
            events.push(Event::Ollied);
            self.rail_again_ms = s.global_float("Rail_jump_rerail_time") as i64;
            self.grind.ollied_off_rail = true;
            // 0.025 at 82002AF8.
            self.body.position.y += 0.025;
            // A feeler `uber_frig_current_height` down; if nothing is there,
            // one from 1 above down to the skater, which lifts the skater
            // onto what it hits.
            let pos = self.body.position;
            let down = Vec3::new(pos.x, pos.y - s.global_float("uber_frig_current_height"), pos.z);
            if world.feeler(pos, down, 0x10, 0).is_none()
                && let Some(hit) = world.feeler(pos + Vec3::Y, pos, 0x10, 0)
            {
                self.body.position = hit.point;
                self.body.position.y += 0.025;
            }
            self.rail_tail(s);
            return events;
        }
        // 820F9284..820F9310: the grind meter.
        if self.balance.kind == qb_key("Grind") || self.balance.kind == qb_key("Slide") {
            self.update_balance_sides(s, world);
            self.run_balance_meter(s, input, &mut events);
        }
        let Some(rails) = world.rails() else { return events };
        let Some(node) = self.rail else { return events };
        // 820F9314..820F933C: a single-node rail: the Natas spin.
        let Some(next) = rails.rails[node].next else {
            self.natas_spin(s);
            return events;
        };
        // 820F9340..820F9424.
        let (a, b) = (rails.rails[node].pos, rails.rails[next].pos);
        let seg = b - a;
        let mut old_sign = self.grind.dir_sign;
        let seglen = seg.length();
        let dir = seg * (1.0 / seglen);
        // 820F9428..820F9540: rail gravity along the rail (not while R2
        // was held at the grab); crouched going down, extra gravity.
        if !self.grind.stall_script {
            let g = s.physics_float("Physics_Rail_Gravity", self.on_bike);
            let mut grav = dir * Vec3::new(0.0, g, 0.0).dot(dir);
            if self.body.velocity.y < 0.0 && self.crouched {
                grav.y -= s.physics_float("additional_downhill_rail_gravity", false);
            }
            self.body.velocity += grav * self.dt;
        }
        // 820F9558..820F9600: the way along the rail (0.01 at 82000D7C).
        if self.body.velocity.length().abs() > 0.01 {
            self.grind.dir_sign = if dir.dot(self.body.velocity) < 0.0 { -1.0 } else { 1.0 };
        }
        // 820F9604..820F962C: just slipped out of a stall.
        if self.grind.slip_pending {
            self.grind.slip_pending = false;
            old_sign = if self.grind.slip_reverse { -self.grind.dir_sign } else { self.grind.dir_sign };
        }
        // 820F9630..820F9920: can the grind go on to the next segment? A
        // corner sharper than `Rail_Corner_Leave_Angle` (flat) blocks it.
        let corner_cos = (s.global_float("Rail_Corner_Leave_Angle") * DEG).cos();
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
        let active = |i: usize| rails.rails[i].flags & flag::ACTIVE != 0;
        let mut blocked = true;
        if self.grind.dir_sign < 0.0 {
            if let Some(p) = rails.rails[node].prev
                && active(p)
            {
                let (cur, adj) = (b - a, a - rails.rails[p].pos);
                blocked = flat(cur).dot(flat(adj)) < corner_cos;
            }
        } else if let Some(n2) = rails.rails[next].next
            && active(n2)
        {
            let (cur, adj) = (a - b, b - rails.rails[n2].pos);
            blocked = flat(cur).dot(flat(adj)) < corner_cos;
        }
        // 820F9924..820F9C04: past the end of the segment (0.0025 at
        // 820027D0): on to the next one, or the end of the line.
        let mut overshoot = 0.0;
        let rail_pt = self.body.position - self.grind.offset;
        let dist = if self.grind.dir_sign < 0.0 { (rail_pt - b).length() } else { (rail_pt - a).length() };
        if dist > seglen + 0.0025 {
            overshoot = dist - seglen;
            // Going back the next record is the previous one and its own
            // flags are tested; going on, the flags of `next` (the record
            // being passed) are, unlike the corner test above.
            let (end, onto) = if self.grind.dir_sign < 0.0 {
                (a, rails.rails[node].prev.filter(|&p| active(p)))
            } else {
                (b, rails.rails[next].next.filter(|_| active(next)).map(|_| next))
            };
            match onto {
                Some(onto) if !blocked => {
                    self.body.position = end + self.grind.offset;
                    self.rail = Some(onto);
                    self.terrain = rails.rails[onto].terrain;
                }
                _ => {
                    if let Some(e) = self.rail_end(s, rails) {
                        events.push(e);
                    }
                }
            }
        }
        // 820F9C08..820F9C14.
        if self.state != State::Rail {
            self.rail_tail(s);
            return events;
        }
        // 820F9C18..820F9CF4: line up with the (maybe new) segment.
        let node = self.rail.unwrap_or(node);
        let Some(next) = rails.rails[node].next else {
            self.rail_tail(s);
            return events;
        };
        let (p0, p1) = (rails.rails[node].pos, rails.rails[next].pos);
        let dirn = (p1 - p0).normalize_or_zero();
        self.grind.dir_sign = if dirn.dot(self.body.velocity) < 0.0 { -1.0 } else { 1.0 };
        self.rail_left_ms = self.time_ms;
        // 820F9CF8..820F9D20: 821EDD60, then the sign.
        self.body.velocity = dirn * self.body.velocity.length() * self.grind.dir_sign;
        // 820F9D24..820F9EE0: the skater faces along the rail (+2024:
        // backwards).
        let facing = if self.rail_backwards { -self.grind.dir_sign } else { self.grind.dir_sign };
        let along = dirn * facing;
        let at = along.normalize_or_zero();
        let row0 = Vec3::new(at.z, 0.0, -at.x).normalize_or_zero();
        let up = at.cross(row0).normalize_or_zero();
        self.body.matrix.x_axis = row0;
        self.body.matrix.y_axis = up;
        self.body.matrix.z_axis = at;
        // 820F9EE4..820F9EEC.
        if self.grind.dir_sign != old_sign {
            // 820F9EF0..820FA008: the way along the rail changed: the
            // display matrix snaps, the stance flips (820FD8E8), SkaterState
            // +48 toggles (820D45F0) and `flip_grinding_backwards` runs with
            // the `SetBackwardsGrindValues` values.
            self.matrix_32 = self.body.matrix;
            self.flip_stance();
            self.rotated = !self.rotated;
            let k = qb_key;
            let params = p8_script::Params(vec![
                (k("ScriptName"), Value::Checksum(self.grind.backwards_script)),
                (k("Anim"), Value::Checksum(self.grind.backwards_anim)),
            ]);
            self.transfer.actions.push(crate::transfer::ScriptAction::Run(k("flip_grinding_backwards"), params));
        } else {
            // 820FA00C..820FA154: the display matrix eases toward it (0.3 at
            // 82000BF0, 0.125 at 82002414).
            let m = &mut self.matrix_32;
            m.z_axis = (m.z_axis + (along - m.z_axis) * 0.3).normalize_or_zero();
            m.y_axis = (m.y_axis + (up - m.y_axis) * 0.125).normalize_or_zero();
            m.x_axis = m.y_axis.cross(m.z_axis).normalize_or_zero();
        }
        // 820FA158..820FA274: move; the overshoot past the end goes on along
        // the facing (so against the travel with +2024 set, as retail).
        let seg2 = p1 - p0;
        if overshoot >= seg2.length() {
            overshoot = 0.0;
        }
        let old_rail_pt = self.body.position - self.grind.offset;
        self.body.position += self.body.velocity * self.dt;
        self.body.position += along * overshoot;
        // 820FA278..820FA2DC.
        self.rail_snap_ease(s);
        let rail_pt = self.body.position - self.grind.offset;
        self.bad_ledge_check(s, seg2 * self.grind.dir_sign, rail_pt, world);
        self.rolling_friction_on(s, Some(rails.rails[node].terrain));
        // 820FA2E0..820FA5EC: did the rail point move into something? A
        // feeler along the move, raised by `up`; at the end of the line
        // (blocked) it is raised 0.025 (820027D4) and reaches 0.15
        // (820029DC) past the move.
        let mv = rail_pt - old_rail_pt;
        let mvn = if mv.length_squared() != 0.0 { mv.normalize() } else { mv };
        let (mut from, mut to) = (old_rail_pt, rail_pt);
        if !blocked {
            from += up;
            to += up;
        } else {
            to += mv + mvn * 0.15;
            from += up * 0.025;
            to += up * 0.025;
        }
        if let Some(hit) = world.feeler(from, to, 0x10, 0) {
            // 820FA5FC..820FA674: knocked off: back to the start of the
            // frame, 0.025 up (82002AF8), the velocity off the hit's
            // normal (821ED798), air, "OffRail".
            self.body.position = self.old_position;
            self.body.position.y += 0.025;
            let n = hit.normal;
            self.body.velocity -= n * self.body.velocity.dot(n);
            self.set_state(State::Air);
            events.push(Event::OffRail);
        }
        self.rail_tail(s);
        events
    }

    /// The common end of `820F8CF0` (820FA678..820FA7C4): the ground normal
    /// is the skater's up; with R2 held at the grab, after
    /// `stall_slip_time` ms in the grind it becomes a stall (state 6),
    /// until then the grab's velocity is taken off a little every frame
    /// (per frame, not per second).
    fn rail_tail(&mut self, s: &Scripts) {
        self.ground_normal = self.body.up();
        if !self.grind.stall_script {
            return;
        }
        if self.state != State::Rail {
            self.grind.stall_script = false;
            return;
        }
        let slip = s.global_float("stall_slip_time");
        if (self.time_ms - self.state_ms) as f32 > slip {
            self.grind.stall_script = false;
            self.set_state(State::Stall);
            return;
        }
        self.body.velocity -= self.grind.stall_velocity * (1.0 / slip);
    }

    /// Retail `820F5DC0` on a normal level: at the end of the line, look
    /// for another rail crossed from just behind the skater (where it was
    /// a frame ago, 0.15 up, 820029DC) to 0.16 (82002AFC) above it, within
    /// `Rail_Corner_Leave_Angle`; if one goes on past the skater, carry on
    /// along it, else fall off ("OffRail", 0.025 up).
    ///
    /// Not translated: the created-park start point (the argument), the
    /// same-collision-object preference (`82194940`, see
    /// [`RailManager::search`]) and the moving objects' rails
    /// (820F617C..820F68B4).
    fn rail_end(&mut self, s: &Scripts, rails: &RailManager) -> Option<crate::core_physics::Event> {
        let pos = self.body.position;
        // (8262BC00: register save.) 820F5DE8..820F5E14: the park editor is OFF on a normal level, so
        // the start is not the argument. 820F5E18..820F5ED8.
        let mut start = pos - self.body.velocity * self.dt;
        start.y += 0.15;
        let end = Vec3::new(pos.x, pos.y + 0.16, pos.z);
        // 820F5EDC..820F5F60: the search; nothing found (820F5F3C) goes on
        // to the moving rails (not translated), a found record that is the
        // current one or has no next (820F5F48..820F5F60) falls off.
        let corner_cos = (s.global_float("Rail_Corner_Leave_Angle") * DEG).cos();
        let snap = s.global_float("Rail_Max_Snap");
        if let Some((rec, point)) = rails.search(start, end, snap, self.rail, corner_cos)
            && let Some(next) = rails.rails[rec].next
        {
            // 820F5F64..820F5FF0: the new rail must go on past the skater.
            let to_a = rails.rails[rec].pos - pos;
            let to_b = rails.rails[next].pos - pos;
            if to_a.dot(to_b) < 0.0 {
                // 820F5FF4..820F6178.
                let dir = (rails.rails[next].pos - rails.rails[rec].pos).normalize_or_zero();
                let v = &mut self.body.velocity;
                if v.x == 0.0 && v.z == 0.0 {
                    let at = self.body.matrix.z_axis;
                    v.x = at.x;
                    v.z = at.z;
                }
                self.grind.dir_sign = if dir.dot(self.body.velocity) < 0.0 { -1.0 } else { 1.0 };
                self.body.velocity = dir * self.body.velocity.length() * self.grind.dir_sign;
                self.rail = Some(rec);
                // 820F68B8..820F68E0.
                self.body.position = point;
                self.terrain = rails.rails[rec].terrain;
                return None;
            }
        }
        // 820F68E4..820F6960: triggers 8208 and 18 (820F0550, not
        // translated), air, 0.025 up (82002AF8), "OffRail".
        self.set_state(State::Air);
        self.body.position.y += 0.025;
        Some(crate::core_physics::Event::OffRail)
    }

    /// Retail `820DBD40`: the Natas spin on a single-node rail: spin at
    /// `natas_spin_factor` less the balance lean / (4000 / factor), plus 5
    /// (4000 at 82002688, 5 at 82000C38) degrees per second, the other way
    /// with R1 held at the grab; the spin count grows (57.2958 at
    /// 82001EB0). Not translated: the spin score (820DBE00..820DBEC0).
    fn natas_spin(&mut self, s: &Scripts) {
        let factor = s.global_float("natas_spin_factor");
        // 820CEB80: the running balance's lean (0 with none).
        let lean = self.balance.anim_lean().unwrap_or(0.0);
        let mut rate = factor - lean.abs() / (4000.0 / factor) + 5.0;
        // 820DBDA0.
        if self.grind.natas_r1 {
            rate = -rate;
        }
        // +2217 (820DBDB0..820DBDC4).
        self.last_spin_positive = rate > 0.0;
        let angle = self.dt * rate;
        // 820B9ED0 on the object and display matrices.
        self.rotate(angle);
        self.spin_degrees += angle * 57.2958;
        self.grind.natas_ms = self.time_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use p8_formats::qb::Value;
    use std::collections::BTreeMap;

    fn scripts(items: &[(&str, f32)]) -> Scripts {
        let g: BTreeMap<u32, Value> = items.iter().map(|(n, v)| (qb_key(n), Value::Float(*v))).collect();
        Scripts::new(g)
    }

    #[test]
    fn the_grab_offset_eases_onto_the_rail() {
        // 820DD070: 0.1 per 60th of a second up, 0.2 flat.
        let s = scripts(&[("Snap_To_Rail_Vert_Above_Speed", 0.1), ("Snap_To_Rail_Vert_Below_Speed", 1.5), ("Snap_To_Rail_Horiz_Speed", 0.2)]);
        let mut p = CorePhysics::new(&s);
        p.body.position = Vec3::new(0.3, 1.25, 0.0);
        p.grind.offset = Vec3::new(0.3, 0.25, 0.0);
        p.rail_snap_ease(&s);
        assert!((p.grind.offset - Vec3::new(0.1, 0.15, 0.0)).length() < 1e-5, "{:?}", p.grind.offset);
        assert!((p.body.position - Vec3::new(0.1, 1.15, 0.0)).length() < 1e-5);
        for _ in 0..3 {
            p.rail_snap_ease(&s);
        }
        assert_eq!(p.grind.offset, Vec3::ZERO);
        assert!((p.body.position - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn the_rail_boost_stops_at_its_threshold() {
        // 820DD5F0: +2 along the rail, up to 10; uphill another 2 * slope.
        let s = scripts(&[("Rail_Speed_Boost", 2.0), ("Rail_Boost_Threshold", 10.0)]);
        let mut p = CorePhysics::new(&s);
        p.body.velocity = Vec3::new(9.0, 0.0, 0.0);
        p.rail_boost(&s, Vec3::X);
        assert!((p.body.velocity.x - 10.0).abs() < 1e-5);
        p.rail_boost(&s, Vec3::X);
        assert!((p.body.velocity.x - 10.0).abs() < 1e-5, "no boost at the threshold");
    }
}
