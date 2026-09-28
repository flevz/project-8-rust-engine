//! `cargo run -p p8-skater --example anim_check -- <DATA/COMPRESSED folder> [names.txt]`
//! Runs the player's scripts with the animation tree attached: prints the
//! branches the scripts add to `body`, untranslated node types, and how far
//! each foot ends from its IK target.
use glam::Vec3;
use p8_formats::{ik, qb_key, skeleton::Skeleton, zone};
use p8_skater::anim_tree::{ClipLib, Rig, SkaterInputs};
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
    let level = Level::new(&z.collision);
    let sk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("Pros_Hawk_skel")).expect("skeleton");
    let rig = Rig::from_skeleton(&sk);
    let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let g = |c: u32| s.globals.get(&c).cloned();
    k.anim.attach(lib, rig.clone(), &g);
    let feet = [("L", "Bone_IK_Foot_Slave_L", "bone_ankle_l"), ("R", "Bone_IK_Foot_Slave_R", "bone_ankle_r")];
    let phases = [
        ("still", InputState::default(), 2.0),
        ("hold A", InputState { crouch: true, ..Default::default() }, 2.0),
        ("release", InputState::default(), 2.0),
    ];
    let mut last = Vec::new();
    for (label, input, secs) in phases {
        for i in 0..(secs * 60.0) as usize {
            k.step(&s, &input, &level);
            k.anim.update(1.0 / 60.0);
            let pose = k.anim.sample(SkaterInputs::default());
            if k.anim.branches != last {
                last = k.anim.branches.clone();
                println!(
                    "{label:>8} t={:4.2} branches: {}",
                    (i + 1) as f32 / 60.0,
                    last.iter().map(|&b| name(b)).collect::<Vec<_>>().join(" > ")
                );
            }
            if let (Ok(dir), Some(p)) = (std::env::var("P8_POSE_DUMP"), &pose)
                && i % 60 == 59
            {
                let lines: Vec<String> =
                    p.q.iter().zip(&p.t).map(|(q, t)| format!("{} {} {} {} {} {} {}", q[0], q[1], q[2], q[3], t[0], t[1], t[2])).collect();
                let f = format!("{dir}/{}_{}.txt", label.replace(' ', "_"), (i + 1) / 60);
                std::fs::write(f, lines.join("\n")).expect("dump");
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
