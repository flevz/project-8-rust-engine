//! `cargo run --release -p p8-skater --example spacewalk_check -- <DATA/COMPRESSED> [names.txt]`
//! Push on a flat floor, start a manual (Up then Down), keep its balance,
//! then press Left, Right, Square (the spacewalk trick). Prints the speed,
//! the script, the trick and the animation events launched.
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
    let bsk = Skeleton::load(&root.join("ZONES/global.pak.xen"), qb_key("board")).expect("board skeleton");
    let lib = ClipLib::open(&root.join("PAK/perm_anims.pak.xen"), &root.join("../ANIMS/standardkeyQ.bin.xen")).expect("clips");
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let g = |c: u32| s.globals.get(&c).cloned();
    k.anim.attach(lib, Rig::from_skeleton(&sk), &g);
    k.anim.board_rig = Some(Rig::from_skeleton(&bsk));
    let mut last_script = 0;
    let mut boosts = 0;
    let mut speed_before = 0.0;
    let no_combo = std::env::var_os("P8_NOCOMBO").is_some();
    for f in 0..60 * 12 {
        let t = f as f32 / 60.0;
        let mut x = InputState { up: t < 2.0 || (150..153).contains(&f), ..Default::default() };
        x.down = (156..159).contains(&f);
        // Keep the balance: tap against the lean.
        if f > 160 && f % 6 < 3 {
            let lean = k.physics.balance.manual.lean;
            x.up |= lean > 600.0;
            x.down |= lean < -600.0;
        }
        if !no_combo {
            x.left = (190..194).contains(&f);
            x.right = (200..204).contains(&f);
            x.kick = (210..214).contains(&f);
        }
        if f == 189 {
            speed_before = k.physics.body.velocity.length();
        }
        let mut events = k.step(&s, &x, &world);
        let inputs = k.physics.anim_inputs_in(&s, Some(&world));
        k.anim.update(1.0 / 60.0, inputs);
        let launched = k.anim.fired.iter().filter(|(n, _)| *n == qb_key("SpaceWalkBoostEvent")).count();
        boosts += launched;
        events.extend(k.launch_anim_events(&s, Some(&world)));
        let sc = k.script_name().unwrap_or(0);
        if launched > 0 || (sc != last_script && f >= 180) || f % 60 == 0 {
            println!(
                "f{f} t={t:.2} speed={:.2} balance_kind={:08x} script {} {}",
                k.physics.body.velocity.length(),
                k.physics.balance.kind,
                name(sc),
                if launched > 0 { "<< SpaceWalkBoostEvent" } else { "" }
            );
        }
        last_script = sc;
    }
    println!("speed before the combo {speed_before:.2}; SpaceWalkBoostEvents launched: {boosts}");
}
