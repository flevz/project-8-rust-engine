//! `cargo run -p p8-skater --example level_ride -- <path to qb.pak.xen> [zone]`
//! Starts the translated skater at the zone's default restart on the
//! player's own level collision and pushes forward, printing where it goes.
use glam::Vec3;
use p8_formats::{qb_key, zone};
use p8_skater::world::Level;
use p8_skater::{CorePhysics, InputState, Scripts};

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to qb.pak.xen"));
    let name = std::env::args().nth(2).unwrap_or("z_houses".into());
    let (_, globals) = p8_formats::qb::load_pak_globals(&path).expect("load scripts");
    let s = Scripts::new(globals);
    let zones = path.parent().and_then(|p| p.parent()).expect("DATA/COMPRESSED").join("ZONES");
    let z = zone::load(&zones, &name).expect("zone");
    println!("{} solids, {} restarts, unknown {:?}", z.collision.solids.len(), z.restarts.len(), z.collision.unknown);
    let default = qb_key(&format!("{name}_TRG_Restart_Default"));
    let r = z.restarts.iter().find(|r| r.name == default).expect("default restart");
    println!("restart at {:?} angles {:?}", r.pos, r.angles);
    let level = Level::new(&z.collision);
    let mut p = CorePhysics::at_restart(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let phases = [
        ("still", InputState::default(), 1.0),
        ("crouch", InputState { crouch: true, ..Default::default() }, 4.0),
        ("coast", InputState::default(), 4.0),
    ];
    for (label, input, seconds) in phases {
        for i in 0..(seconds * 60.0) as usize {
            let events = p.step(&s, &input, &level);
            if i % 15 == 14 || !events.is_empty() && events != [p8_skater::core_physics::Event::Stopped] {
                println!(
                    "{label:>7} t={:4.2}s pos {:7.2?} speed {:5.2} {:?} {events:?}",
                    (i + 1) as f32 / 60.0,
                    p.body.position.to_array(),
                    p.body.velocity.length(),
                    p.state
                );
            }
        }
    }
}
