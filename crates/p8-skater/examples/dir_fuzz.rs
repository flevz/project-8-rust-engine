//! `cargo run --release -p p8-skater --example dir_fuzz -- <DATA/COMPRESSED> [seeds]`
//! Plays random inputs on the level and reports stretches where the skater
//! is on the ground, fast, and rolling backwards along the board without
//! the turn-around (`820DBAA8`) firing, with the flags that block it.
use glam::Vec3;
use p8_formats::{qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig};
use p8_skater::core_physics::State;
use p8_skater::world::Level;
use p8_skater::{InputState, Scripts, Skater};

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let seeds: u64 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(20);
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let s = Scripts::new(globals);
    let z = zone::load(&root.join("ZONES"), "z_houses").expect("zone");
    let r = z.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    let level = Level::new(&z.collision);
    let world: &dyn p8_skater::World = &level;
    let sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
    let rig = Rig::from_skeleton(&sk);
    let bsk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("board")).expect("board skeleton");
    let brig = Rig::from_skeleton(&bsk);
    let flip_speed = s.global_float("Skater_Flip_Speed");
    println!("Skater_Flip_Speed = {flip_speed}");
    let mut bails = 0;
    for seed in 0..seeds {
        let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
        let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
        let g = |c: u32| s.globals.get(&c).cloned();
        k.anim.attach(lib, rig.clone(), &g);
        k.anim.board_rig = Some(brig.clone());
        let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut next = move || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng >> 33) as u32
        };
        let mut input = InputState::default();
        let mut bad = 0;
        let mut was_bail = false;
        let mut hist: Vec<String> = Vec::new();
        for i in 0..60 * 60 {
            if i % 25 == 0 {
                let n = next();
                input = InputState {
                    up: n & 1 != 0 || i < 120,
                    crouch: n & 2 != 0,
                    left: n & 4 != 0,
                    right: n & 8 != 0 && n & 4 == 0,
                    l1: n & 16 != 0,
                    r1: n & 32 != 0 && n & 16 == 0,
                    circle: n & 256 != 0,
                    kick: n & 512 != 0,
                    triangle: n & 1024 != 0 && n & 2048 != 0,
                    r2: n & 4096 != 0 && n & 8192 != 0,
                    down: n & 64 != 0 && n & 128 == 0,
                    brake_digital: n & 64 != 0 && n & 128 == 0,
                    ..Default::default()
                };
            }
            k.step(&s, &input, world);
            let inputs = k.physics.anim_inputs_in(&s, Some(world));
            k.anim.update(1.0 / 60.0, inputs);
            let _ = k.anim.sample(inputs);
            let p = &k.physics;
            let v = p.body.velocity;
            let sc = k.script.as_ref().map_or(0, |x| x.name);
            let in_bail_script = [qb_key("Bail_WaitAnim"), qb_key("Baildone"), qb_key("Bail_WaitAnimFinished")].contains(&sc);
            if in_bail_script && !was_bail {
                bails += 1;
            }
            was_bail = in_bail_script;
            let back = p.state == State::Ground && v.length() > flip_speed && v.dot(p.body.at()) < 0.0;
            if back {
                bad += 1;
                if bad == 15 {
                    println!(
                        "seed {seed} frame {i}: backwards {:.1} m/s  bail={} braking={} lock_dir={} powerslide={} rotated={} balance_kind={:08x} script={} bailscript={}",
                        v.length(), p.in_bail, p.braking, p.lock_velocity_direction, p.powerslide, p.rotated, p.balance.kind, sc == 0, in_bail_script
                    );
                    for h in &hist {
                        println!("    {h}");
                    }
                }
            } else {
                bad = 0;
            }
            hist.push(format!("f{i} {:?} v.at={:.2} spd={:.1} bail={} brk={}", p.state, v.dot(p.body.at()), v.length(), p.in_bail, p.braking));
            if hist.len() > 12 {
                hist.remove(0);
            }
        }
    }
    println!("bails entered: {bails}");
}
