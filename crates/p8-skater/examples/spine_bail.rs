//! `P8_NAMES=<names.txt> cargo run --release -p p8-skater --example spine_bail -- <DATA/COMPRESSED> [x z dx dz speed]`
//! The real scripts on the real level: roll at a vert wall holding R2 (the
//! spine button) like `transfer_ride`, and print the scripts that run, the
//! events and whether a bail starts. Defaults: the z_houses spine near x = -65.
//! P8_SPIN=<deg>: spin with L1 in the air until that many degrees.
use glam::Vec3;
use p8_formats::{qb_key, zone};
use p8_skater::world::Level;
use p8_skater::{InputState, Scripts, Skater};
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(&a[1]);
    let f = |i: usize, d: f32| a.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let (x, y, z0, dx, dz, speed) = (f(2, -69.67037), f(3, 2.6858814), f(4, -85.41025), f(5, 1.0), f(6, 0.0), f(7, 12.0));
    let mut names: HashMap<u32, String> = HashMap::new();
    if let Some(list) = std::env::var("P8_NAMES").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    let name = |k: u32| names.get(&k).cloned().unwrap_or(format!("{k:08x}"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let s = Scripts::new(globals);
    let zn = zone::load(&root.join("ZONES"), "z_houses").expect("zone");
    let r = zn.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    let level = Level::new(&zn.collision);
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    let spin_to: f32 = std::env::var("P8_SPIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let at = Vec3::new(dx, 0.0, dz).normalize();
    let mut last_script = 0;
    for i in 0..60 * 14 {
        if i == 240 {
            let p = &mut k.physics;
            p.body.position = Vec3::new(x, y + 0.0025, z0);
            p.body.matrix.z_axis = at;
            p.body.matrix.x_axis = Vec3::Y.cross(at).normalize();
            p.body.matrix.y_axis = Vec3::Y;
            p.matrix_32 = p.body.matrix;
            p.body.velocity = at * speed;
        }
        let in_air = k.physics.state == p8_skater::core_physics::State::Air;
        let input = InputState { up: i < 240 || (240..300).contains(&i), r2: i >= 240, l1: spin_to > 0.0 && in_air && k.physics.spin_degrees.abs() < spin_to, ..Default::default() };
        let events = k.step(&s, &input, &level);
        let sc = k.script_name().unwrap_or(0);
        if events.contains(&p8_skater::core_physics::Event::Landed) {
            // The tests `land` makes (queries use the display matrix `+32`
            // as it was at the start of the frame).
            let p = &k.physics;
            let (m, ef, v) = (&p.queries_matrix, p.vert.ease_from, p.body.velocity);
            use p8_skater::queries::*;
            println!(
                "LANDED: queries up {:?} at {:?} ease_from {:?} vel {:?}\n  yaw(80,100) {} yaw(60,120) {} abspitch>60 {} pitch>60 {} roll>50 {}  (object up {:?}, +32 up {:?})",
                m.y_axis, m.z_axis, ef, v, yaw_between(m, v, 80.0, 100.0), yaw_between(m, v, 60.0, 120.0), absolute_pitch_greater(m, 60.0),
                pitch_greater(m, ef, 60.0), roll_greater(m, ef, 50.0), p.body.up(), p.matrix_32.y_axis
            );
        }
        if i >= 240 && (sc != last_script || !events.is_empty() || (i % 30 == 0 && k.physics.state != p8_skater::core_physics::State::Ground && sc != qb_key("Stoppedstate"))) {
            let p = &k.physics;
            println!(
                "t={:5.2} {:?} spd={:5.2} pos=[{:.1},{:.1},{:.1}] up=[{:.2},{:.2},{:.2}] spin={:.0} bail={} landed_from_spine={} script {} {events:?}",
                i as f32 / 60.0,
                p.state,
                p.body.velocity.length(),
                p.body.position.x,
                p.body.position.y,
                p.body.position.z,
                p.body.up().x,
                p.body.up().y,
                p.body.up().z,
                p.spin_degrees,
                p.in_bail,
                p.transfer.landed_from_spine,
                name(sc)
            );
        }
        last_script = sc;
    }
}
