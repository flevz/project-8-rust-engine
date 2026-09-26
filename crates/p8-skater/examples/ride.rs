//! `cargo run -p p8-skater --example ride -- <path to qb.pak.xen>`
//! Rides the translated skater with the player's own scripts and prints
//! speed over time: push, coast, turn, ollie, brake.
use p8_skater::{CorePhysics, FlatFloor, InputState, Scripts};

fn main() {
    let path = std::env::args().nth(1).expect("path to qb.pak.xen");
    let (files, globals) = p8_formats::qb::load_pak_globals(path.as_ref()).expect("load scripts");
    let s = Scripts::new(globals);
    println!("{files} scripts");
    let mut p = CorePhysics::new(&s);
    let phases = [
        ("crouch", InputState { crouch: true, ..Default::default() }, 4.0),
        ("coast", InputState::default(), 4.0),
        ("crouch+right", InputState { crouch: true, stick_x_raw: 127.0, ..Default::default() }, 2.0),
        ("release", InputState::default(), 1.0),
        ("brake", InputState { stick_back_raw: 127.0, ..Default::default() }, 2.0),
    ];
    for (name, input, seconds) in phases {
        for i in 0..(seconds * 60.0) as usize {
            let events = p.step(&s, &input, &FlatFloor::default());
            if p.state == p8_skater::core_physics::State::Ground {
                p.body.position.y = 0.0;
            }
            if i % 6 == 5 || !events.is_empty() && events != [p8_skater::core_physics::Event::Stopped] {
                let at = p.body.at();
                println!(
                    "{name:>12} t={:4.2}s speed {:6.3} y {:5.3} heading {:6.1} deg {events:?}",
                    (i + 1) as f32 / 60.0,
                    p.body.velocity.length(),
                    p.body.position.y,
                    at.x.atan2(at.z).to_degrees()
                );
            }
        }
    }
}
