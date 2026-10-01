//! `cargo run -p p8-skater --example grind_ride -- <path to qb.pak.xen> [zone] [speed] [ollie|nobalance] [rail index]`
//! With the skater's scripts and the level's rails: finds a flat, linked
//! rail with floor 0.3..0.9 m below it, rolls at it on a slant, ollies and
//! holds "Triangle" (Xbox Y) to grab it, then prints position, speed, state,
//! the grind balance and the running script every frame while grinding
//! (every 10 frames otherwise).
//!
//! By default the harness leans against the meter (Left/Right, like a
//! player) so the grind lasts until the rail ends; `nobalance` leaves the
//! meter alone (it tips over: "OffMeterTop/Bottom"), `ollie` ollies off half a
//! second into the grind. `up` / `down` / `left` / `right` hold that
//! direction until the grab (a different grind, `GrindTrickList`);
//! `blunt` taps Up, Up then presses Y in the air (`grindtricks`:
//! Nosebluntslide; use speed 5..7 so the grab comes after the taps). `rail
//! index` picks another of the candidates; `natas` goes for a single-node
//! rail (the Natas spin) instead. With `P8_ANIMS=<DATA/COMPRESSED>`
//! it also runs the animation tree and prints its branches and, while
//! grinding, the untranslated or modulate nodes.
use glam::Vec3;
use p8_formats::{qb_key, zone};
use p8_skater::core_physics::State;
use p8_skater::rails::flag;
use p8_skater::world::{Level, World};
use p8_skater::{InputState, Scripts, Skater};

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to qb.pak.xen"));
    let name = std::env::args().nth(2).unwrap_or("z_houses".into());
    let speed: f32 = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(8.0);
    let mode = std::env::args().nth(4).unwrap_or_default();
    // Candidate 2 on z_houses is a short rail that ends (0.8 s of grind).
    let pick: usize = std::env::args().nth(5).and_then(|s| s.parse().ok()).unwrap_or(if mode == "natas" { 0 } else { 2 });
    let (_, globals) = p8_formats::qb::load_pak_globals(&path).expect("load scripts");
    let s = Scripts::new(globals);
    let zones = path.parent().and_then(|p| p.parent()).expect("DATA/COMPRESSED").join("ZONES");
    let z = zone::load(&zones, &name).expect("zone");
    let level = Level::new(&z.collision).with_rails(&z.rails, &|t| s.terrain_index(t));
    let rails = &level.rails.rails;
    println!(
        "{} rail nodes, {} linked, {} single-node (Natas spin)",
        rails.len(),
        rails.iter().filter(|r| r.next.is_some()).count(),
        rails.iter().filter(|r| r.next.is_none() && r.prev.is_none() && r.flags & flag::ACTIVE != 0).count()
    );

    // Candidate segments: active, at least 4 m long, nearly flat, floor
    // 0.3..0.9 m below the middle and on one side 1.5 m out.
    let mut found = Vec::new();
    for (i, r) in rails.iter().enumerate() {
        let Some(n) = r.next else { continue };
        if r.flags & flag::ACTIVE == 0 || r.flags & (flag::CLIMBING | flag::MANUAL) != 0 {
            continue;
        }
        let d = rails[n].pos - r.pos;
        let len = d.length();
        if len < 4.0 || d.y.abs() > 0.05 * len {
            continue;
        }
        let dir = d / len;
        let mid = r.pos + d * 0.5;
        let side = Vec3::new(dir.z, 0.0, -dir.x);
        for side in [side, -side] {
            let out = mid + side * 1.5;
            let Some(floor) = level.feeler(out + Vec3::Y * 0.2, out - Vec3::Y * 2.0, 0x10, 0) else { continue };
            let drop = mid.y - floor.point.y;
            if floor.normal.y < 0.98 || !(0.3..0.9).contains(&drop) {
                continue;
            }
            // Clear path along the floor to the start point.
            let start = floor.point - dir * 3.0;
            if level.feeler(start + Vec3::Y * 0.3, floor.point + Vec3::Y * 0.3, 0x10, 0).is_some() {
                continue;
            }
            let Some(f2) = level.feeler(start + Vec3::Y * 0.3, start - Vec3::Y * 1.0, 0x10, 0) else { continue };
            if (f2.point.y - floor.point.y).abs() > 0.05 {
                continue;
            }
            found.push((i, n, mid, dir, f2.point, side, drop));
            break;
        }
    }
    // `natas`: a single-node rail instead, approached along the floor.
    if mode == "natas" {
        found.clear();
        for (i, r) in rails.iter().enumerate() {
            if r.next.is_some() || r.prev.is_some() || r.flags & flag::ACTIVE == 0 {
                continue;
            }
            for a in 0..8 {
                let ang = a as f32 * std::f32::consts::FRAC_PI_4;
                let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
                // The node tops a post: the floor is where the run starts.
                let out = r.pos - dir * 3.0;
                let Some(f2) = level.feeler(out + Vec3::Y * 0.5, out - Vec3::Y * 2.0, 0x10, 0) else { continue };
                let drop = r.pos.y - f2.point.y;
                if f2.normal.y < 0.9 || !(0.2..1.0).contains(&drop) {
                    continue;
                }
                let near = r.pos - dir * 0.5;
                if level.feeler(f2.point + Vec3::Y * 0.3, Vec3::new(near.x, f2.point.y + 0.3, near.z), 0x10, 0).is_some() {
                    continue;
                }
                found.push((i, i, r.pos, dir, f2.point, Vec3::ZERO, drop));
                break;
            }
        }
    }
    println!("{} candidate rails", found.len());
    let Some(&(i, n, mid, dir, start, side, drop)) = found.get(pick) else {
        println!("no rail found");
        return;
    };
    println!(
        "rail record {i} -> {n}: {:?} -> {:?}, {:.2} m above the floor; rolling from {:?}",
        rails[i].pos.to_array(),
        rails[n].pos.to_array(),
        drop,
        start.to_array()
    );
    ride(&s, &level, start, mid - side * 0.0, speed, &mode, dir);
}

fn ride(s: &Scripts, level: &Level, start: Vec3, target: Vec3, speed: f32, mode: &str, rail_dir: Vec3) {
    // Aim at the rail's middle, along the floor.
    let to = Vec3::new(target.x - start.x, 0.0, target.z - start.z).normalize();
    let mut k = Skater::new(s, start + Vec3::Y * 0.0025, Vec3::ZERO);
    let p = &mut k.physics;
    p.body.matrix.z_axis = to;
    p.body.matrix.x_axis = Vec3::Y.cross(to).normalize();
    p.body.matrix.y_axis = Vec3::Y;
    p.matrix_32 = p.body.matrix;
    p.body.velocity = to * speed;
    if let Ok(root) = std::env::var("P8_ANIMS") {
        let root = std::path::PathBuf::from(root);
        let sk = p8_formats::skeleton::Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
        let lib = p8_skater::anim_tree::ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen"))
            .expect("clips");
        let g = |c: u32| s.globals.get(&c).cloned();
        k.anim.attach(lib, p8_skater::anim_tree::Rig::from_skeleton(&sk), &g);
    }
    let names: std::collections::HashMap<u32, String> = std::fs::read_to_string("/home/user/p8work/extra_names.txt")
        .map(|t| t.lines().map(|n| (qb_key(n), n.to_string())).collect())
        .unwrap_or_default();
    let name = |c: Option<u32>| c.map(|c| names.get(&c).cloned().unwrap_or(format!("{c:08x}"))).unwrap_or_default();
    println!("rolling {:?} at {speed} m/s (rail runs {:?})", to.to_array(), rail_dir.to_array());
    let mut last = (State::Ground, 0u32);
    let mut rail_frames = 0;
    let mut ollied = false;
    let mut last_branches = Vec::new();
    for i in 0..(6 * 60) {
        let on_rail = matches!(k.physics.state, State::Rail | State::Stall);
        rail_frames = if on_rail { rail_frames + 1 } else { 0 };
        // Crouch from the start, let go at 0.25 s: the ollie onto the rail.
        let mut crouch = i < 15;
        // `ollie`: crouch 1/3 s into the grind, let go 10 frames later.
        if mode == "ollie" && (20..30).contains(&rail_frames) {
            crouch = true;
        }
        // Lean against the meter like a player (Left / Right push the lean
        // one way and the other), unless `nobalance`.
        // (Steers against where the lean is heading: a test harness, not
        // game code.)
        let lean = k.physics.balance.grind.lean + k.physics.balance.grind.lean_speed * 0.3;
        let balancing = on_rail && mode != "nobalance";
        let left = balancing && lean < -5.0;
        let right = balancing && lean > 5.0;
        // A direction held until the grab picks the grind.
        let before = !on_rail && rail_frames == 0 && i < 30;
        let (left, right) = (left || (before && mode == "left"), right || (before && mode == "right"));
        let (mut up, down) = (before && mode == "up", before && mode == "down");
        let mut triangle = !ollied;
        if mode == "blunt" {
            // In the air, after the ollie (let go at frame 15).
            up = (17..19).contains(&i) || (21..23).contains(&i);
            triangle = i >= 24;
        }
        let input = InputState { triangle, crouch, left, right, up, down, ..Default::default() };
        let events = k.step(s, &input, level);
        let inputs = k.physics.anim_inputs_in(s, Some(level));
        k.anim.update(1.0 / 60.0, inputs);
        let _ = k.anim.sample(inputs);
        if std::env::var_os("P8_ANIMS").is_some() {
            if k.anim.branches != last_branches {
                last_branches = k.anim.branches.clone();
                println!("      branches: {}", last_branches.iter().map(|&b| name(Some(b))).collect::<Vec<_>>().join(" > "));
            }
            if on_rail && rail_frames % 15 == 1 {
                let mut t = String::new();
                if let Some(b) = &k.anim.body {
                    b.describe(0, &mut t);
                }
                for l in t.lines().filter(|l| l.contains("UNTRANSLATED") || l.contains("modulate")) {
                    println!("      {}", l.trim());
                }
            }
        }
        if mode == "ollie" && rail_frames >= 30 && !on_rail {
            ollied = true;
        }
        let p = &k.physics;
        let now = (p.state, k.script_name().unwrap_or(0));
        let grinding = matches!(p.state, State::Rail | State::Stall);
        if grinding || i % 10 == 9 || !events.is_empty() || now != last {
            println!(
                "t={:4.2}s pos {:7.2?} speed {:5.2} {:?} rail {:?} dir {:+.0} spin {:5.0} balance {} lean {:6.1} script {} trick {:?} {events:?}",
                (i + 1) as f32 / 60.0,
                p.body.position.to_array(),
                p.body.velocity.length(),
                p.state,
                p.rail,
                p.grind.dir_sign,
                p.spin_degrees,
                name(Some(p.balance.kind)),
                p.balance.grind.lean,
                name(k.script_name()),
                p.tricks.trick_name,
            );
        }
        if !k.untranslated.is_empty() && i % 60 == 59 {
            println!("      untranslated so far: {}", k.untranslated.iter().map(|&c| name(Some(c))).collect::<Vec<_>>().join(", "));
        }
        last = now;
    }
}
