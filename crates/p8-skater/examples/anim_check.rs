//! `cargo run -p p8-skater --example anim_check -- <DATA/COMPRESSED folder> [names.txt]`
//! Runs the player's scripts with the animation tree attached: prints the
//! branches the scripts add to `body`, untranslated node types, and how far
//! each foot ends from its IK target.
use glam::Vec3;
use p8_formats::{ik, qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig};
use p8_skater::world::Level;
use p8_skater::{InputState, Scripts, Skater};
use std::collections::BTreeMap;

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let mut names = BTreeMap::new();
    if let Some(list) = std::env::args().nth(2).and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    let name = |k: u32| names.get(&k).cloned().unwrap_or(format!("{k:08x}"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let s = Scripts::new(globals);
    let z = zone::load(&root.join("ZONES"), "z_houses").expect("zone");
    let r = z.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    // P8_FLAT=1: a flat floor at the restart height instead of the level.
    let level = Level::new(&z.collision);
    let flat = p8_skater::FlatFloor { height: r.pos[1] };
    let world: &dyn p8_skater::World = if std::env::var_os("P8_FLAT").is_some() { &flat } else { &level };
    let sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
    let rig = Rig::from_skeleton(&sk);
    let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let g = |c: u32| s.globals.get(&c).cloned();
    k.anim.attach(lib, rig.clone(), &g);
    let board_sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("board")).expect("board skeleton");
    let board_rig = Rig::from_skeleton(&board_sk);
    k.anim.board_rig = Some(board_rig.clone());
    let feet = [("L", "Bone_IK_Foot_Slave_L", "bone_ankle_l"), ("R", "Bone_IK_Foot_Slave_R", "bone_ankle_r")];
    if let Ok(which) = std::env::var("P8_TRICK") {
        trick_check(&s, &mut k, world, &which, &name);
        return;
    }
    if std::env::var_os("P8_SPIN").is_some() {
        spin_check(&s, &mut k, world, &rig);
        return;
    }
    let phases = [
        ("still", InputState::default(), 2.0),
        ("hold A", InputState { crouch: true, ..Default::default() }, 2.0),
        ("release", InputState::default(), 2.0),
    ];
    let mut last = Vec::new();
    for (label, input, secs) in phases {
        for i in 0..(secs * 60.0) as usize {
            k.step(&s, &input, world);
            let inputs = k.physics.anim_inputs_in(&s, Some(world));
            k.anim.update(1.0 / 60.0, inputs);
            let pose = k.anim.sample(inputs);
            if k.anim.branches != last {
                last = k.anim.branches.clone();
                println!(
                    "{label:>8} t={:4.2} branches: {}",
                    (i + 1) as f32 / 60.0,
                    last.iter().map(|&b| name(b)).collect::<Vec<_>>().join(" > ")
                );
            }
            if let (Ok(dir), Some(p)) = (std::env::var("P8_POSE_DUMP"), &pose)
                && i % std::env::var("P8_DUMP_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(60) == 0
            {
                let lines: Vec<String> =
                    p.q.iter().zip(&p.t).map(|(q, t)| format!("{} {} {} {} {} {} {}", q[0], q[1], q[2], q[3], t[0], t[1], t[2])).collect();
                let f = format!("{dir}/{}_{}.txt", label.replace(' ', "_"), i);
                std::fs::write(f, lines.join("\n")).expect("dump");
                let mut t = format!("{:?}\n", k.physics.anim_inputs(&s));
                if let Some(bp) = k.anim.sample_board() {
                    let locals: Vec<ik::Local> =
                        bp.q.iter().zip(&bp.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect();
                    let m = ik::model_space(&locals, &board_rig.parents);
                    for (i, b) in m.iter().enumerate() {
                        t += &format!("board bone {i} w={} pos {:?} rot {:?}\n", bp.w[i], b.translation, b.rotation);
                    }
                }
                if let Some(b) = &k.anim.body {
                    b.describe(0, &mut t);
                }
                std::fs::write(format!("{dir}/{}_{}_tree.txt", label.replace(' ', "_"), i), t).expect("dump");
            }
            if i % 30 == 29
                && let Some(p) = pose
            {
                let locals: Vec<ik::Local> =
                    p.q.iter().zip(&p.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect();
                let m = ik::model_space(&locals, &rig.parents);
                let d: Vec<String> = feet
                    .iter()
                    .map(|(n, t, a)| {
                        let (t, a) = (m[rig.bone(qb_key(t)).unwrap()].translation, m[rig.bone(qb_key(a)).unwrap()].translation);
                        let th = m[rig.bone(qb_key(if *n == "L" { "bone_thigh_l" } else { "bone_thigh_r" })).unwrap()].translation;
                        let kn = m[rig.bone(qb_key(if *n == "L" { "bone_knee_l" } else { "bone_knee_r" })).unwrap()].translation;
                        let reach = Vec3::from(th).distance(Vec3::from(kn)) + Vec3::from(kn).distance(Vec3::from(a))
                            - Vec3::from(th).distance(Vec3::from(t));
                        format!("{n} {:.4} (spare reach {:+.3})", Vec3::from(t).distance(Vec3::from(a)), reach)
                    })
                    .collect();
                println!("{label:>8} t={:4.2} foot to IK target: {}", (i + 1) as f32 / 60.0, d.join(" "));
            }
        }
    }
    println!("untranslated node types: {}", k.anim.untranslated.iter().map(|&c| name(c)).collect::<Vec<_>>().join(" "));
}

/// P8_SPIN=1: push, ollie, spin with L1 in the air until 180 degrees, land.
/// Prints, per frame around the landing, the world direction from the right
/// ankle to the left one (a jump in it is a visible snap), with the stance
/// flags.
fn spin_check(s: &p8_skater::Scripts, k: &mut Skater, world: &dyn p8_skater::World, rig: &Rig) {
    let (l, r) = (rig.bone(qb_key("bone_ankle_l")).unwrap(), rig.bone(qb_key("bone_ankle_r")).unwrap());
    let mut air_seen = false;
    let mut after = 0;
    let mut prev_yaw: Option<f32> = None;
    for i in 0..60 * 12 {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        air_seen |= in_air;
        let input = if t < 3.0 {
            InputState { up: true, ..Default::default() }
        } else if t < 3.5 {
            InputState { crouch: true, ..Default::default() }
        } else if in_air && k.physics.spin_degrees.abs() < 175.0 {
            InputState { l1: true, ..Default::default() }
        } else {
            InputState::default()
        };
        k.step(s, &input, world);
        let inputs = k.physics.anim_inputs_in(s, Some(world));
        k.anim.update(1.0 / 60.0, inputs);
        let Some(p) = k.anim.sample(inputs) else { continue };
        let locals: Vec<ik::Local> =
            p.q.iter().zip(&p.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect();
        let m = ik::model_space(&locals, &rig.parents);
        let d = k.physics.body.matrix * (Vec3::from(m[l].translation) - Vec3::from(m[r].translation));
        let yaw = d.x.atan2(d.z).to_degrees();
        // Board: truck bone 4 to truck bone 6 (the board skeleton), in world.
        let board_yaw = k.anim.sample_board().map_or(f32::NAN, |bp| {
            let parents = k.anim.board_rig.as_ref().unwrap().parents.clone();
            let locals: Vec<ik::Local> =
                bp.q.iter().zip(&bp.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect();
            let m = ik::model_space(&locals, &parents);
            let v = k.physics.body.matrix * (Vec3::from(m[4].translation) - Vec3::from(m[6].translation));
            v.x.atan2(v.z).to_degrees()
        });
        let jump = prev_yaw.map_or(0.0, |y| ((yaw - y + 540.0) % 360.0) - 180.0);
        prev_yaw = Some(yaw);
        if air_seen && !in_air {
            after += 1;
        }
        if std::env::var_os("P8_TREE").is_some() && after == 30 {
            let mut t = String::new();
            if let Some(b) = &k.anim.body {
                b.describe(0, &mut t);
            }
            for l in t.lines().filter(|l| l.contains("degenerate") || l.contains("skaterflip") || l.contains("mirror")).take(30) {
                println!("{l}");
            }
        }
        if air_seen && after < 240 && (after > 0 || in_air) {
            println!(
                "t={t:5.2} air={in_air} spin={:6.1} flipped={} rotated={} feet L-R yaw {yaw:7.1} (change {jump:+6.1}) board yaw {board_yaw:7.1} branches {}",
                k.physics.spin_degrees,
                k.physics.flipped,
                k.physics.rotated,
                k.anim.branches.len()
            );
        }
        if after >= 240 {
            break;
        }
    }
}

/// P8_TRICK=<dir> (e.g. `left` for a kickflip, `up` for an impossible;
/// `grab:<dir>` uses circle): push, ollie, then in the air press the
/// direction with square (circle). Prints each change of script and
/// animation branch, and the commands not translated.
fn trick_check(s: &p8_skater::Scripts, k: &mut Skater, world: &dyn p8_skater::World, which: &str, name: &dyn Fn(u32) -> String) {
    let (grab, dir) = match which.strip_prefix("grab:") {
        Some(d) => (true, d),
        None => (false, which),
    };
    // ground:r2 (stance switch) / ground:manual (up then down): no ollie.
    let ground = which.strip_prefix("ground:");
    let mut air_frames = 0;
    let (mut last_script, mut last_branches) = (0, Vec::new());
    for i in 0..60 * 8 {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        air_frames = if in_air { air_frames + 1 } else { 0 };
        let f = i;
        let mut input = if let Some(g) = ground {
            let mut x = InputState { up: t < 2.0, ..Default::default() };
            match g {
                "r2" => x.r2 = (150..153).contains(&f),
                // ground:manual:<a><b> presses button a then b in the
                // manual (s square/kick, c circle, t triangle).
                g if g.starts_with("manual") => {
                    x.up = x.up || (150..153).contains(&f);
                    x.down = (156..159).contains(&f);
                    let seq: Vec<char> = g.strip_prefix("manual:").unwrap_or("").chars().collect();
                    for (n, c) in seq.iter().enumerate() {
                        let on = (180 + n as i32 * 8..183 + n as i32 * 8).contains(&f);
                        match c {
                            's' => x.kick |= on,
                            'c' => x.circle |= on,
                            't' => x.triangle |= on,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            x
        } else if t < 3.0 {
            InputState { up: true, ..Default::default() }
        } else if t < 3.5 {
            InputState { crouch: true, ..Default::default() }
        } else {
            InputState::default()
        };
        if ground.is_none() && (6..10).contains(&air_frames) {
            match dir {
                "left" => input.left = true,
                "right" => input.right = true,
                "up" => input.up = true,
                "down" => input.down = true,
                _ => {}
            }
            if grab {
                input.circle = true;
            } else {
                input.kick = true;
            }
        }
        let events = k.step(s, &input, world);
        let inputs = k.physics.anim_inputs_in(s, Some(world));
        k.anim.update(1.0 / 60.0, inputs);
        let _ = k.anim.sample(inputs);
        if std::env::var_os("P8_TREE").is_some() && (air_frames == 20 || (ground.is_some() && i == 200)) {
            let mut t = String::new();
            if let Some(b) = &k.anim.body {
                b.describe(0, &mut t);
            }
            println!("{t}");
        }
        let sc = k.script_name().unwrap_or(0);
        if sc != last_script || k.anim.branches != last_branches || !events.is_empty() || (ground.is_some() && i % 20 == 0) {
            println!(
                "t={t:5.2} air={in_air} flipped={} lean={:.0} script {} {events:?} branches {}",
                k.physics.flipped,
                k.physics.balance.manual.lean,
                name(sc),
                k.anim.branches.iter().map(|&b| name(b)).collect::<Vec<_>>().join(" > ")
            );
            last_script = sc;
            last_branches = k.anim.branches.clone();
        }
    }
    println!("commands not translated: {}", k.untranslated.iter().map(|&c| name(c)).collect::<Vec<_>>().join(" "));
}
