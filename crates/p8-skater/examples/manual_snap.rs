//! `cargo run --release -p p8-skater --example manual_snap -- <DATA/COMPRESSED> [names.txt]`
//! Ollie on a flat floor, press the manual input (Up then Down) in the air,
//! land, and print how far the skeleton pose moves each frame around the
//! landing (sum of quaternion differences over the bones) with the script
//! and animation branches, to see whether the air -> manual change snaps.
use glam::Vec3;
use p8_formats::{qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig};
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
    let world = p8_skater::FlatFloor { height: r.pos[1] };
    let sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
    let rig = Rig::from_skeleton(&sk);
    let bsk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("board")).expect("board skeleton");
    let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let g = |c: u32| s.globals.get(&c).cloned();
    k.anim.attach(lib, rig, &g);
    k.anim.board_rig = Some(Rig::from_skeleton(&bsk));
    let mut air_frames = 0;
    let mut prev: Option<p8_skater::anim_tree::Pose> = None;
    let mut land_frame: Option<i32> = None;
    let mut last_script = 0;
    for i in 0..60 * 7 {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        air_frames = if in_air { air_frames + 1 } else { 0 };
        let mut input = InputState { up: t < 3.0, crouch: (2.5..3.0).contains(&t), ..Default::default() };
        if in_air {
            // Up then Down, the manual input, before landing.
            input.up = (12..15).contains(&air_frames);
            input.down = (18..21).contains(&air_frames);
        }
        let events = k.step(&s, &input, &world);
        let inputs = k.physics.anim_inputs_in(&s, Some(&world));
        k.anim.update(1.0 / 60.0, inputs);
        let pose = k.anim.sample(inputs);
        if events.contains(&p8_skater::core_physics::Event::Landed) {
            land_frame = Some(i);
        }
        if land_frame.is_some_and(|l| i == l + 5) {
            let mut tree = String::new();
            if let Some(b) = &k.anim.body {
                b.describe(0, &mut tree);
            }
            for l in tree.lines().take(60) {
                println!("  TREE {}", l.trim());
            }
        }
        let sc = k.script_name().unwrap_or(0);
        if let (Some(a), Some(b)) = (&prev, &pose) {
            let mut d = 0.0;
            for (qa, qb) in a.q.iter().zip(&b.q) {
                let dot = (qa[0] * qb[0] + qa[1] * qb[1] + qa[2] * qb[2] + qa[3] * qb[3]).abs().min(1.0);
                d += 2.0 * dot.acos();
            }
            let near = land_frame.is_some_and(|l| i >= l - 2 && i <= l + 40);
            if near || sc != last_script {
                println!(
                    "f{i} t={t:.2} air={in_air} pose_delta={d:.3} manual={} script {} branches {}",
                    k.physics.balance.kind != 0,
                    name(sc),
                    k.anim.branches.iter().rev().take(3).map(|&b| name(b)).collect::<Vec<_>>().join(" < ")
                );
            }
        }
        last_script = sc;
        prev = pose;
    }
    let mut un: Vec<String> = k.untranslated.iter().map(|&c| name(c)).collect();
    un.sort();
    println!("untranslated commands: {}", un.join(" "));
    println!("untranslated nodes: {:?}", k.anim.untranslated.iter().map(|&c| name(c)).collect::<Vec<_>>());
}
