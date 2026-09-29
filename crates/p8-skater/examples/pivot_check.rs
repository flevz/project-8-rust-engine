//! `P8_NAMES=<names.txt> cargo run --release -p p8-skater --example pivot_check -- <DATA/COMPRESSED>`
//! Push, ollie, Up+Down in the air (manual), land, then tap R2 in the manual
//! (manual pivot, `Trick_Gturn2` from `ManualBranches`, script `nosemanual`)
//! and print, per frame, the scripts, the animation branches, and the feet /
//! board yaw relative to the skater's facing (a jump is a visible snap).
//! P8_L2=1 taps L2 instead; P8_SPIN=<deg> spins with L1 in the air first;
//! P8_FRAMES=<from>,<to> prints only that many frames after the tap.
use glam::Vec3;
use p8_formats::{ik, qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig};
use p8_skater::{InputState, Scripts, Skater};
use std::collections::HashMap;

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let mut names: HashMap<u32, String> = HashMap::new();
    if let Some(list) = std::env::var("P8_NAMES").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    let name = |k: u32| names.get(&k).cloned().unwrap_or(format!("{k:08x}"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let s = Scripts::new(globals);
    let z = zone::load(&root.join("ZONES"), "z_houses").expect("zone");
    let r = z.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    let world = p8_skater::FlatFloor { height: r.pos[1] };
    let sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
    let rig = Rig::from_skeleton(&sk);
    let bsk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("board")).expect("board skeleton");
    let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
    let (l, rr) = (rig.bone(qb_key("bone_ankle_l")).unwrap(), rig.bone(qb_key("bone_ankle_r")).unwrap());
    // P8_CLIP=<name>[,<name>..]: raw clips only: feet / hips yaw over the clip.
    if let Ok(list) = std::env::var("P8_CLIP") {
        let mut lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
        for n in list.split(',') {
            let Some(c) = lib.get(qb_key(n)) else {
                println!("{n}: not found");
                continue;
            };
            println!("{n}: {:.3} s", c.duration);
            let hips = rig.bone(qb_key("bone_pelvis")).or_else(|| rig.bone(qb_key("bone_hips")));
            for j in 0..=10 {
                let t = c.duration * j as f32 / 10.0;
                let p = p8_skater::anim_tree::Pose::from_clip(&rig, &c, t);
                let m = ik::model_space(
                    &p.q.iter().zip(&p.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect::<Vec<_>>(),
                    &rig.parents,
                );
                let d = Vec3::from(m[l].translation) - Vec3::from(m[rr].translation);
                println!("  {:5.2}s feet yaw {:7.1} hips {:?}", t, d.x.atan2(d.z).to_degrees(), hips.map(|h| m[h].translation));
            }
        }
        return;
    }
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let g = |c: u32| s.globals.get(&c).cloned();
    k.anim.attach(lib, rig.clone(), &g);
    k.anim.board_rig = Some(Rig::from_skeleton(&bsk));
    let l2 = std::env::var_os("P8_L2").is_some();
    let (from, to): (i32, i32) = std::env::var("P8_FRAMES").ok().and_then(|v| v.split_once(',').map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(90)))).unwrap_or((-5, 90));
    let mut air_start: Option<i32> = None;
    let (mut air_seen, mut landed_at, mut last_script, mut last_branches) = (false, None::<i32>, 0, Vec::new());
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..60 * 14 {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        if in_air && air_start.is_none() { air_start = Some(i); }
        air_seen |= in_air;
        if air_seen && !in_air && landed_at.is_none() {
            landed_at = Some(i);
        }
        let tap = landed_at.map_or(false, |a| (a + 10..a + 13).contains(&i));
        let air_n = i - air_start.unwrap_or(i);
        let mut input = if t < 3.0 {
            InputState { up: true, ..Default::default() }
        } else if t < 3.5 {
            InputState { crouch: true, ..Default::default() }
        } else if in_air {
            InputState { up: (12..15).contains(&air_n), down: (18..21).contains(&air_n), ..Default::default() }
        } else {
            InputState::default()
        };
        input.r2 = tap && !l2;
        input.l2 = tap && l2;
        let events = k.step(&s, &input, &world);
        let inputs = k.physics.anim_inputs_in(&s, Some(&world));
        k.anim.update(1.0 / 60.0, inputs);
        let Some(p) = k.anim.sample(inputs) else { continue };
        let rel = landed_at.map_or(i32::MIN, |a| i - (a + 10));
        let m = ik::model_space(
            &p.q.iter().zip(&p.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect::<Vec<_>>(),
            &rig.parents,
        );
        // Relative to the skater's facing (object matrix), so a turn of the object does not show.
        let inv = k.physics.body.matrix.transpose();
        let yaw_of = |v: Vec3| {
            let d = inv * v;
            d.x.atan2(d.z).to_degrees()
        };
        let feet = yaw_of(k.physics.body.matrix * (Vec3::from(m[l].translation) - Vec3::from(m[rr].translation)));
        let board = k.anim.sample_board().map_or(f32::NAN, |bp| {
            let mm = ik::model_space(
                &bp.q.iter().zip(&bp.t).map(|(q, t)| ik::Local { rotation: [-q[0], -q[1], -q[2], q[3]], translation: *t }).collect::<Vec<_>>(),
                &k.anim.board_rig.as_ref().unwrap().parents,
            );
            yaw_of(k.physics.body.matrix * (Vec3::from(mm[4].translation) - Vec3::from(mm[6].translation)))
        });
        let body_yaw = k.physics.body.at().x.atan2(k.physics.body.at().z).to_degrees();
        let vel_yaw = k.physics.body.velocity.x.atan2(k.physics.body.velocity.z).to_degrees();
        let sc = k.script_name().unwrap_or(0);
        let changed = sc != last_script || k.anim.branches != last_branches || !events.is_empty();
        if rel != i32::MIN && rel >= from && rel <= to {
            let (df, db) = prev.map_or((0.0, 0.0), |(f, b)| (((feet - f + 540.0) % 360.0) - 180.0, ((board - b + 540.0) % 360.0) - 180.0));
            println!(
                "f{rel:+4} spd={:5.2} feet {feet:7.1} ({df:+6.1}) board {board:7.1} ({db:+6.1}) body {body_yaw:7.1} vel {vel_yaw:7.1} flip={} rot={} {}{}{events:?}",
                k.physics.body.velocity.length(),
                k.physics.flipped,
                k.physics.rotated,
                if changed { format!("script {} ", name(sc)) } else { String::new() },
                if changed { format!("branches {:?} ", k.anim.branches.iter().map(|b| name(*b)).collect::<Vec<_>>()) } else { String::new() },
            );
        }
        if std::env::var_os("P8_TIMERS").is_some() && rel != i32::MIN && rel >= from && rel <= to && rel % 3 == 0 {
            let mut t = String::new();
            if let Some(b) = &k.anim.body {
                b.describe(0, &mut t);
            }
            for line in t.lines().filter(|l| l.contains("timer") || l.contains("skaterflip") || l.contains("mirror")) {
                println!("      {}", line.trim());
            }
        }
        prev = Some((feet, board));
        last_script = sc;
        last_branches = k.anim.branches.clone();
        if landed_at.is_some_and(|a| i > a + 10 + to.max(0)) {
            break;
        }
    }
}
