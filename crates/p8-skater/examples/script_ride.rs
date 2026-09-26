//! `cargo run -p p8-skater --example script_ride -- <qb.pak.xen> [names.txt]`
//! The skater with the player's own scripts running, on the houses level:
//! prints the running script, state, speed and events, then every script
//! command met that is not translated yet.
use glam::Vec3;
use p8_formats::{qb_key, zone};
use p8_skater::world::Level;
use p8_skater::{InputState, Scripts, Skater};
use std::collections::BTreeMap;

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("qb.pak.xen"));
    let mut names = BTreeMap::new();
    if let Some(list) = std::env::args().nth(2).and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    let name = |k: u32| names.get(&k).cloned().unwrap_or(format!("{k:08x}"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&path).expect("scripts");
    let s = Scripts::new(globals);
    let zones = path.parent().and_then(|p| p.parent()).unwrap().join("ZONES");
    let z = zone::load(&zones, "z_houses").expect("zone");
    let r = z.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    let level = Level::new(&z.collision);
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let phases = [
        ("still", InputState::default(), 1.0),
        ("hold A", InputState { crouch: true, ..Default::default() }, 3.0),
        ("release", InputState::default(), 3.0),
    ];
    for (label, input, secs) in phases {
        for i in 0..(secs * 60.0) as usize {
            let ev = k.step(&s, &input, &level);
            let p = &k.physics;
            if i % 20 == 19 || ev.iter().any(|e| *e != p8_skater::core_physics::Event::Stopped) {
                println!(
                    "{label:>8} t={:4.2} script {:>22} {:?} speed {:5.2} y {:5.2} friction+{:.2} {ev:?}",
                    (i + 1) as f32 / 60.0,
                    k.script_name().map(name).unwrap_or_default(),
                    p.state,
                    p.body.velocity.length(),
                    p.body.position.y,
                    p.special_friction
                );
            }
        }
    }
    println!("untranslated ({}):", k.untranslated.len());
    let mut u: Vec<String> = k.untranslated.iter().map(|&c| name(c)).collect();
    u.sort();
    println!("{}", u.join(" "));
}
