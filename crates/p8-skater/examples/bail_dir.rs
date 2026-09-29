//! `cargo run --release -p p8-skater --example bail_dir -- <DATA/COMPRESSED>`
//! Push, ollie, hold a grab into a landing bail, wait for the get-up, then
//! push again; prints velocity against the board's forward axis.
use glam::Vec3;
use p8_formats::{qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig};
use p8_skater::{InputState, Scripts, Skater};

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
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
    // P8_SPIN=<deg>: spin with L1 that far in the air; P8_PUSH=<from>,<to>.
    let spin_to: f32 = std::env::var("P8_SPIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let spin = spin_to > 0.0;
    let push: (f32, f32) = std::env::var("P8_PUSH").ok().and_then(|v| v.split_once(',').map(|(a, b)| (a.parse().unwrap_or(9.0), b.parse().unwrap_or(13.0)))).unwrap_or((9.0, 13.0));
    let mut air_frames = 0;
    for i in 0..60 * 16 {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        air_frames = if in_air { air_frames + 1 } else { 0 };
        let mut input = InputState { up: t < 3.0 || (push.0..push.1).contains(&t), crouch: (3.0..3.5).contains(&t), ..Default::default() };
        if in_air {
            input.l1 = spin && k.physics.spin_degrees.abs() < spin_to;
            if air_frames >= 6 && std::env::var_os("P8_NOGRAB").is_none() {
                input.circle = true;
                input.down = air_frames < 10;
            }
        }
        // P8_BACK=<t>: at that time (on the ground) push the skater backwards
        // along the board at 6 m/s, like rolling back down a ramp.
        if std::env::var("P8_BACK").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|b| (t - b).abs() < 0.005) && grounded_now(&k) {
            let at = k.physics.body.at();
            k.physics.body.velocity = -at * 6.0;
        }
        k.step(&s, &input, &world);
        let inputs = k.physics.anim_inputs_in(&s, Some(&world));
        k.anim.update(1.0 / 60.0, inputs);
        let _ = k.anim.sample(inputs);
        if i % 12 == 0 && t > 3.4 && t < 8.0 {
            let p = &k.physics;
            let v = p.body.velocity;
            let mut tree = String::new();
            if let Some(b) = &k.anim.body {
                b.describe(0, &mut tree);
            }
            let turned = tree.matches("turned=true").count();
            let flips = tree.matches("skaterflip").count();
            println!(
                "flipnodes={flips} turned={turned} t={t:5.2} {:?} spd={:5.2} v.at={:6.2} bail={} brk={} rot={} flip={} lockdir={}",
                p.state, v.length(), v.dot(p.body.at()), p.in_bail, p.braking, p.rotated, p.flipped, p.lock_velocity_direction
            );
        }
    }
}

fn grounded_now(k: &Skater) -> bool {
    k.physics.state == p8_skater::core_physics::State::Ground
}
