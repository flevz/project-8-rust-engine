//! Keeping the skater on the level while on the ground: the move loop of
//! the ground update (`820F7C98`..`820F8010`), forward collision
//! (`820EBD20`), the wall response (`820E5F40`, with `820DB858` bounce and
//! `820DB418` wall push) and ground snapping (`820F12C0`).
use crate::air::{orthonormalize_keep_up, surface_skatable};
use crate::core_physics::{CorePhysics, Event, State, project_keep_length};
use crate::input::InputState;
use crate::script::Scripts;
use crate::world::{Hit, World};
use glam::Vec3;

/// 0.0025: the distance kept from a surface (constant at 820027D0).
const SKIN: f32 = 0.0025;
/// 0.0174533: degrees to radians (82000C10).
const DEG: f32 = 0.017453292;

impl CorePhysics {
    /// The move of the ground update (`820F7C98`..`820F8000`): move by
    /// `velocity * dt`, run forward collision and ground snapping, and if a
    /// wall cut the move short, move once more along the velocity for the
    /// part of the distance that was lost.
    ///
    /// Not translated: the skitch and bike branches, and `820E5158` (sets a
    /// box of ±3.5 x, -150..12 y around the skater; LIKELY the collision
    /// cache region, which the translated [`World`] does not need).
    pub(crate) fn ground_move(&mut self, s: &Scripts, input: &InputState, world: &dyn World, events: &mut Vec<Event>) {
        let mut step = self.body.velocity * self.dt;
        let mut second = false;
        loop {
            let before = self.body.position;
            self.body.position += step;
            self.forward_collision(s, input, world, events);
            self.ground_snap(s, input, world, events);
            let moved = (self.body.position - before).length();
            let wanted = step.length();
            // 0.1 is f30 in 820F6978.
            if !second && self.state == State::Ground && moved < wanted - 0.1 {
                second = true;
                step = self.body.velocity.normalize_or_zero() * wanted * (1.0 - moved / wanted);
                continue;
            }
            break;
        }
    }

    /// Retail `820EBD20`: a feeler ahead of the move, `Skater_First_Forward_
    /// Collision_Height` above the board and `..._Length` past it.
    fn forward_collision(&mut self, s: &Scripts, input: &InputState, world: &dyn World, events: &mut Vec<Event>) {
        let old = self.old_position;
        if self.body.position == old {
            return;
        }
        let dir = (self.body.position - old).normalize_or_zero();
        let h = s.physics_float("Skater_First_Forward_Collision_Height", self.on_bike);
        let l = s.physics_float("Skater_First_Forward_Collision_Length", self.on_bike);
        let up = self.body.up();
        let Some(hit) = world.feeler(old + up * h, self.body.position + up * h + dir * l, 0x10, 0) else {
            return;
        };
        // 0.01 is the constant used at 820EBD20.
        if surface_skatable(s, &hit) && hit.normal.dot(self.ground_normal).abs() >= 0.01 {
            self.body.position = hit.point + hit.normal * SKIN;
            return;
        }
        // Retail records the wall's terrain at `+1950` (read by sounds) and
        // skips surfaces with trigger flag 512 before the wall response.
        self.wall_response(s, input, &hit, events);
    }

    /// Retail `820E5F40`: the skater ran into a wall on the ground.
    ///
    /// Not translated: the bonk sound (`82115D00`), and the second feeler
    /// and turn-around that only run when the hit object's `+224` flag is
    /// set (its meaning is UNKNOWN; level geometry is treated as unset).
    fn wall_response(&mut self, s: &Scripts, input: &InputState, hit: &Hit, events: &mut Vec<Event>) {
        let n = hit.normal;
        if self.wall_push(s, input, n, events) {
            return;
        }
        let h = s.physics_float("Skater_First_Forward_Collision_Height", self.on_bike);
        let speed_before = self.body.velocity.length();
        let a = self.bounce(s, 1.0, n, events);
        let dont_slow = s.global_float("Wall_Bounce_Dont_Slow_Angle") * DEG;
        if a.abs() > dont_slow {
            // 1.5708 = pi/2 (82001AF4).
            let slow = 1.0 - (a.abs() - dont_slow) / (std::f32::consts::FRAC_PI_2 - dont_slow);
            self.body.velocity *= slow;
            if speed_before > s.global_float("Wall_Bounce_Dont_Flail_Speed") {
                events.push(if (a < 0.0) != self.flipped { Event::FlailLeft } else { Event::FlailRight });
            }
        }
        // 0.15 is the constant at 820029DC.
        self.body.position = hit.point - self.body.up() * h + n * 0.15;
    }

    /// Retail `820DB858`: turn the skater and velocity away from a wall.
    /// Returns the angle between the board's row 0 and the wall, folded
    /// into -pi/2..pi/2.
    fn bounce(&mut self, s: &Scripts, scale: f32, n: Vec3, events: &mut Vec<Event>) -> f32 {
        if self.in_bail {
            events.push(Event::BailCollision);
            return 0.0;
        }
        let d = self.body.row0().dot(n).clamp(-1.0, 1.0);
        let mut a = d.acos();
        if a > std::f32::consts::FRAC_PI_2 {
            a -= std::f32::consts::PI;
        }
        let turn = s.global_float("Wall_Bounce_Angle_Multiplier") * a * scale;
        let (sn, c) = (turn.sin(), turn.cos());
        let v = self.body.velocity;
        self.body.velocity = Vec3::new(c * v.x + sn * v.z, v.y, c * v.z - sn * v.x);
        self.rotate(turn);
        a
    }

    /// Retail `820DB418`: with Triangle held (Xbox Y), facing into the wall,
    /// push off it. Returns whether the skater pushed.
    fn wall_push(&mut self, s: &Scripts, input: &InputState, n: Vec3, events: &mut Vec<Event>) -> bool {
        if !input.triangle {
            return false;
        }
        let disallow = s.global_float("Physics_Disallow_Rewallpush_Duration") as i64;
        if self.time_ms - self.last_wallpush_ms < disallow {
            return false;
        }
        let limit = ((s.global_float("Wall_Bounce_Dont_Slow_Angle") - 1.0) * DEG).sin();
        if self.body.at().dot(n) >= -limit {
            return false;
        }
        // Retail also requires `now - +2528 >= disallow`; nothing translated
        // writes `+2528` (UNKNOWN), so it never blocks here.
        events.push(Event::WallPush);
        if self.state_216 {
            // Retail clears SkaterState +216 (with a timestamp) instead.
            self.state_216 = false;
            return false;
        }
        let v = self.body.velocity;
        self.body.velocity = v - n * v.dot(n) * 2.0;
        let speed = self.body.velocity.length();
        let min_exit = s.global_float("Physics_Wallpush_Min_Exit_Speed");
        // 2.5e-5 is the retail zero-length threshold here.
        if speed > 2.5e-5 {
            let new_speed = (speed - s.global_float("Physics_Wallpush_Speed_Loss")).max(min_exit);
            self.body.velocity *= new_speed / speed;
        } else {
            self.body.velocity = self.body.at() * -min_exit;
        }
        self.body.velocity = project_keep_length(self.body.velocity, self.ground_normal);
        let at = self.body.velocity.normalize_or_zero();
        let up = self.ground_normal;
        self.body.matrix.z_axis = at;
        self.body.matrix.y_axis = up;
        self.body.matrix.x_axis = up.cross(at);
        self.matrix_32 = self.body.matrix;
        self.last_wallpush_ms = self.time_ms;
        true
    }

    /// Retail `820D7648`: take a new ground normal as the skater's up, and
    /// start easing `+96` toward it from where it is (`+128`, `+160` = 1).
    pub(crate) fn orient_to_ground(&mut self, n: Vec3) {
        if n == self.ground_normal {
            return;
        }
        self.vert.ease_from = self.vert.eased_normal;
        self.vert.ease_left = 1.0;
        self.ground_normal = n;
        self.body.matrix.y_axis = n;
        orthonormalize_keep_up(&mut self.body.matrix);
    }

    /// Retail `820F12C0`: find the ground under the skater and stay on it,
    /// or leave the ground (off an edge).
    ///
    /// Not translated: SkaterState `+96`/`+176` bookkeeping from the surface
    /// flags (read by animation code), `820E0A50` and trigger handling after
    /// a snap, the display-matrix part of `820DA1A8`, and after going off an
    /// edge the spine-transfer search (`820E1600`, `820DA7B0`).
    fn ground_snap(&mut self, s: &Scripts, input: &InputState, world: &dyn World, events: &mut Vec<Event>) {
        let up = self.body.up();
        let start = self.body.position + up * s.physics_float("Physics_Ground_Snap_Up", self.on_bike);
        // -5 is the constant at 82002A7C.
        let end = self.body.position + up * -5.0;
        let mut stick = false;
        if let Some(hit) = world.feeler(start, end, 0x10, 0) {
            let n = hit.normal;
            let to_skater = self.body.position - hit.point;
            let above = up.dot(to_skater);
            if !surface_skatable(s, &hit) {
                // 0.001 is the constant at 82000D80.
                if above < 0.001 {
                    self.body.position = hit.point + n * SKIN;
                }
            } else {
                stick = true;
                let at = self.body.at();
                let a1 = project_keep_length(at, n);
                let a0 = project_keep_length(at, self.ground_normal);
                let c = a1.dot(a0);
                let name = if input.up { "Ground_stick_angle_forward" } else { "Ground_stick_angle" };
                let stick_cos = (s.physics_float(name, self.on_bike) * DEG).cos();
                if at.dot(n) > 0.0 && c > 0.0 && c < stick_cos {
                    // The ground turns away downward too sharply: leave it.
                    stick = false;
                } else if above > 0.0 {
                    let moved = (self.body.position - self.old_position).length();
                    let allow = (c.clamp(-1.0, 1.0).acos().tan() * moved)
                        .max(s.physics_float("Physics_Ground_Snap_Down", self.on_bike));
                    if to_skater.length() > allow {
                        stick = false;
                    }
                }
                if stick {
                    // SkaterState +72 from the hit's vert flag (+1185).
                    self.vert.on_vert_ground = hit.flags & 0x8 != 0;
                    self.orient_to_ground(n);
                }
                // Upside down (0.0 up.y) and slower than 7.6 (82002A5C).
                let up = self.body.up();
                if stick && up.y < 0.0 && self.body.velocity.length() < 7.6 {
                    // -0.8 is the constant at 82002AD8.
                    if up.y < -0.8 {
                        self.body.position = hit.point + n * SKIN;
                        stick = false;
                    } else if self.body.velocity.y > 0.0 {
                        // Multiplied by the runtime vector at 827329F0,
                        // which the rotation code uses as -1.
                        self.body.velocity = -self.body.velocity;
                    }
                }
                if stick {
                    self.body.position = hit.point + n * SKIN;
                    // `820E53D8(+298)`: the new terrain under the board.
                    self.terrain = hit.terrain;
                    self.ease_normal(s);
                }
            }
        }
        if !stick {
            self.set_state(State::Air);
            events.push(Event::SkaterOffEdge);
            events.push(Event::GroundGone);
            self.vert_takeoff(s);
            if self.vert.in_vert_air {
                self.set_break_window(true);
                if !self.spine_button(input) {
                    self.break_vert(s, Some(world), false);
                }
                let t = s.global_float("Skater_vert_active_up_time") as i32;
                if input.up_released_ms > t && input.up_held_ms > t {
                    self.set_break_window(false);
                }
            } else if self.spine_button(input) {
                // 820F1B7C: off an edge, not vert, with the spine button:
                // an acid drop with the pop.
                if let Some(drop) = self.acid_drop_search(s, world, true) {
                    self.acid_drop_start(s, &drop);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core_physics::{CorePhysics, Event, State};
    use crate::input::InputState;
    use crate::script::Scripts;
    use crate::world::{FlatFloor, Hit, World};
    use glam::Vec3;
    use p8_formats::qb::Value;
    use p8_formats::qb_key as k;

    fn scripts() -> Scripts {
        let f = |name: &str, v: f32| (k(name), Value::Float(v));
        let physics = Value::Struct(vec![
            f("physics_rolling_friction", 0.0),
            (k("skater_max_speed_stat"), Value::Struct(vec![(0, Value::Pair(18.0, 18.0))])),
            (k("skater_max_max_speed_stat"), Value::Struct(vec![(0, Value::Pair(38.0, 38.0))])),
            f("physics_heavy_air_friction", 0.0004),
            f("physics_ground_snap_up", 0.33),
            f("physics_ground_snap_down", 0.2),
            f("skater_first_forward_collision_height", 0.2),
            f("skater_first_forward_collision_length", 0.25),
            f("ground_stick_angle", 30.0),
            f("ground_stick_angle_forward", 30.0),
            f("physics_air_gravity", -21.6),
            f("physics_air_snap_up", 0.38),
            f("skater_min_distance_to_wall", 0.2),
        ]);
        Scripts::new(
            [
                (k("skater_physics"), physics),
                f("physics_air_hang_stat", 0.9),
                f("wall_non_skatable_angle", 25.0),
                f("wall_bounce_angle_multiplier", 1.1),
                f("wall_bounce_dont_slow_angle", 30.0),
                f("wall_bounce_dont_flail_speed", 2.54),
                f("physics_disallow_rewallpush_duration", 800.0),
                f("physics_wallpush_speed_loss", 5.08),
                f("physics_wallpush_min_exit_speed", 2.54),
                f("Skater_Default_Stats", 5.0),
                f("skater_late_jump_slop", 333.0),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// Flat floor at y = 0 ending at x = `edge`, with a wall facing -z at
    /// z = `wall` (no flags: the wall is not skatable by angle).
    struct Test {
        edge: f32,
        wall: f32,
    }

    impl World for Test {
        fn feeler(&self, a: Vec3, b: Vec3, _: u16, _: u16) -> Option<Hit> {
            if a.z < self.wall && b.z >= self.wall {
                let t = (self.wall - a.z) / (b.z - a.z);
                return Some(Hit { point: a + (b - a) * t, normal: -Vec3::Z, flags: 0, terrain: 0 });
            }
            let hit = FlatFloor::default().feeler(a, b, 0, 0)?;
            (hit.point.x < self.edge).then_some(hit)
        }
    }

    fn rolling(s: &Scripts, v: Vec3) -> CorePhysics {
        let mut p = CorePhysics::new(s);
        p.body.velocity = v;
        p
    }

    fn frame(p: &mut CorePhysics, s: &Scripts, input: &InputState, w: &dyn World) -> Vec<Event> {
        p.old_position = p.body.position;
        let mut events = Vec::new();
        p.ground_move(s, input, w, &mut events);
        events
    }

    #[test]
    fn rolling_on_the_floor_stays_on_it_at_the_skin_distance() {
        let s = scripts();
        let mut p = rolling(&s, Vec3::new(0.0, 0.0, 5.0));
        let w = Test { edge: 100.0, wall: 100.0 };
        for _ in 0..30 {
            assert!(frame(&mut p, &s, &InputState::default(), &w).is_empty());
        }
        assert_eq!(p.state, State::Ground);
        assert!((p.body.position.y - 0.0025).abs() < 1e-6);
    }

    #[test]
    fn rolling_off_an_edge_leaves_the_ground() {
        let s = scripts();
        let mut p = rolling(&s, Vec3::new(5.0, 0.0, 0.0));
        p.body.matrix.z_axis = Vec3::X;
        p.body.matrix.x_axis = -Vec3::Z;
        let w = Test { edge: 0.5, wall: 100.0 };
        let mut events = Vec::new();
        for _ in 0..10 {
            events.extend(frame(&mut p, &s, &InputState::default(), &w));
            if p.state == State::Air {
                break;
            }
        }
        assert_eq!(p.state, State::Air);
        assert_eq!(events, vec![Event::SkaterOffEdge, Event::GroundGone]);
        assert!(p.body.position.x > 0.49, "{}", p.body.position);
    }

    #[test]
    fn head_on_wall_stops_at_the_wall_and_bounces_back_slower() {
        let s = scripts();
        let mut p = rolling(&s, Vec3::new(0.0, 0.0, 6.0));
        let w = Test { edge: 100.0, wall: 1.0 };
        let mut events = Vec::new();
        for _ in 0..30 {
            events.extend(frame(&mut p, &s, &InputState::default(), &w));
        }
        assert!(p.body.position.z < 1.0);
        assert!(events.contains(&Event::FlailLeft) || events.contains(&Event::FlailRight));
        // Head on: angle pi/2 from row 0, fully slowed.
        assert!(p.body.velocity.length() < 1e-3);
    }

    #[test]
    fn wall_push_reflects_and_keeps_speed_minus_the_loss() {
        let s = scripts();
        let mut p = rolling(&s, Vec3::new(0.0, 0.0, 9.0));
        p.time_ms = 10_000;
        p.last_wallpush_ms = 0;
        let w = Test { edge: 100.0, wall: 0.2 };
        let input = InputState { triangle: true, ..Default::default() };
        let events = frame(&mut p, &s, &input, &w);
        assert!(events.contains(&Event::WallPush));
        assert!((p.body.velocity - Vec3::new(0.0, 0.0, -(9.0 - 5.08))).length() < 1e-4);
        assert!(p.body.at().z < -0.99);
    }

    /// A level that is one box (from `min` to `max`), no flags.
    fn block(min: Vec3, max: Vec3) -> crate::world::Level {
        use p8_formats::havok::{LevelCollision, Solid, Transform};
        let c = (min + max) * 0.5;
        let half = ((max - min) * 0.5).to_array();
        let solid = Solid::Box { transform: Transform { t: c.to_array(), ..Transform::IDENTITY }, half, material: 0 };
        crate::world::Level::new(&LevelCollision { solids: vec![solid], unknown: Default::default() })
    }

    fn flying(s: &Scripts, pos: Vec3, v: Vec3) -> CorePhysics {
        let mut p = CorePhysics::new(s);
        p.body.position = pos;
        p.body.velocity = v;
        p.state = State::Air;
        p
    }

    #[test]
    fn flying_into_a_wall_slides_along_it_instead_of_through() {
        let s = scripts();
        // A tall wall from z = 1 to 2; flying at it on a slant.
        let w = block(Vec3::new(-10.0, 0.0, 1.0), Vec3::new(10.0, 3.0, 2.0));
        let mut p = flying(&s, Vec3::new(0.0, 1.0, 0.8), Vec3::new(3.0, 0.0, 6.0));
        for _ in 0..20 {
            p.step(&s, &InputState::default(), &w);
            assert!(p.body.position.z < 1.0, "{}", p.body.position);
        }
        // The part into the wall is gone (pushed off by a tenth of the
        // sliding speed); the part along it is kept.
        assert!(p.body.velocity.z <= 0.0, "{}", p.body.velocity);
        assert!((p.body.velocity.x - 3.0).abs() < 1e-3, "{}", p.body.velocity);
    }

    #[test]
    fn rising_into_a_ledge_steps_up_onto_it() {
        let s = scripts();
        // A ledge 1 m high starting at z = 1.
        let w = block(Vec3::new(-10.0, 0.0, 1.0), Vec3::new(10.0, 1.0, 5.0));
        let mut p = flying(&s, Vec3::new(0.0, 0.7, 0.8), Vec3::new(0.0, 2.0, 6.0));
        p.step(&s, &InputState::default(), &w);
        // Put on top of it (0.025 + 0.00254 above), past the edge.
        assert!((p.body.position.y - 1.02754).abs() < 1e-4, "{}", p.body.position);
        assert!(p.body.position.z > 1.0);
        let mut events = Vec::new();
        for _ in 0..30 {
            events.extend(p.step(&s, &InputState::default(), &w));
        }
        assert!(events.contains(&Event::Landed), "{events:?}");
        assert!(p.body.position.y > 1.0);
    }

    /// Roll off an edge holding the crouch, then let go after `release`
    /// seconds in the air. Returns every event.
    fn off_edge_release(release: f32) -> Vec<Event> {
        let s = scripts();
        let mut p = rolling(&s, Vec3::new(6.0, 0.0, 0.0));
        p.body.matrix.z_axis = Vec3::X;
        p.body.matrix.x_axis = -Vec3::Z;
        // A ledge at x = 0.5, a lower floor at y = -10 past it.
        let w = block(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(0.5, 0.0, 50.0));
        struct Drop(crate::world::Level);
        impl World for Drop {
            fn feeler(&self, a: Vec3, b: Vec3, i1: u16, i0: u16) -> Option<Hit> {
                self.0.feeler(a, b, i1, i0).or_else(|| FlatFloor { height: -10.0 }.feeler(a, b, i1, i0))
            }
        }
        let w = Drop(w);
        let hold = InputState { crouch: true, ..Default::default() };
        let mut events = Vec::new();
        while p.state == State::Ground {
            events.extend(p.step(&s, &hold, &w));
        }
        let left = p.time_ms;
        while ((p.time_ms - left) as f32) < release * 1000.0 {
            events.extend(p.step(&s, &hold, &w));
        }
        // Let go, fall and land on the lower floor, then keep rolling.
        for _ in 0..120 {
            events.extend(p.step(&s, &InputState::default(), &w));
        }
        events
    }

    #[test]
    fn letting_go_just_after_rolling_off_an_edge_is_a_late_ollie() {
        let events = off_edge_release(0.2);
        let jump = events.iter().position(|e| *e == Event::SkaterJump).expect("late ollie");
        let land = events.iter().position(|e| *e == Event::Landed).expect("lands");
        assert!(jump < land);
        assert_eq!(events.iter().filter(|e| **e == Event::SkaterJump).count(), 1);
    }

    #[test]
    fn letting_go_late_in_the_air_only_uncrouches_and_landing_does_not_ollie() {
        let events = off_edge_release(0.5);
        assert!(events.contains(&Event::Landed), "{events:?}");
        assert!(!events.contains(&Event::SkaterJump), "{events:?}");
        // The release is used up in the air: no ollie after landing either.
        let land = events.iter().position(|e| *e == Event::Landed).unwrap();
        assert!(!events[land..].contains(&Event::Ollied), "{events:?}");
    }
}
