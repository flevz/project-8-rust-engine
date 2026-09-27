//! `cargo run -p p8-skater --example vert_ride -- <path to qb.pak.xen> [zone] [speed] [up]`
//! Finds a vert wall (collision flag 0x8) near the zone's default restart,
//! starts the skater on the ground in front of it, rolling at it, and prints
//! the ride: up the wall, vert air, and back down.
use glam::Vec3;
use p8_formats::havok::{Solid, split_material};
use p8_formats::{qb_key, zone};
use p8_skater::world::{Level, World};
use p8_skater::{CorePhysics, InputState, Scripts};

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to qb.pak.xen"));
    let name = std::env::args().nth(2).unwrap_or("z_houses".into());
    let speed: f32 = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    // With "up", "Up" is held the whole ride (from before it starts).
    let hold_up = std::env::args().nth(4).as_deref() == Some("up");
    let (_, globals) = p8_formats::qb::load_pak_globals(&path).expect("load scripts");
    let s = Scripts::new(globals);
    let zones = path.parent().and_then(|p| p.parent()).expect("DATA/COMPRESSED").join("ZONES");
    let z = zone::load(&zones, &name).expect("zone");
    let default = qb_key(&format!("{name}_TRG_Restart_Default"));
    let r = z.restarts.iter().find(|r| r.name == default).expect("default restart");
    let level = Level::new(&z.collision);

    // Near-vertical vert triangles, closest to the restart first.
    let restart = Vec3::from(r.pos);
    let mut walls: Vec<(f32, Vec3, Vec3)> = Vec::new();
    for solid in &z.collision.solids {
        if let Solid::Triangle { v, material } = solid {
            let (_, flags) = split_material(*material);
            if flags & 0x8 == 0 {
                continue;
            }
            let [a, b, c] = v.map(Vec3::from);
            let n = (b - a).cross(c - a).normalize_or_zero();
            if n.y.abs() > 0.2 {
                continue;
            }
            let centre = (a + b + c) / 3.0;
            walls.push(((centre - restart).length(), centre, n));
        }
    }
    walls.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("{} near-vertical vert triangles", walls.len());

    for (_, centre, n) in walls.iter().take(40) {
        // Try both faces; the skater must find floor 4 m out, below the
        // wall, with a clear line to the wall.
        for n in [*n, -*n] {
            let flat = Vec3::new(n.x, 0.0, n.z).normalize_or_zero();
            let out = *centre + flat * 4.0;
            let Some(floor) = level.feeler(out + Vec3::Y * 1.0, out - Vec3::Y * 6.0, 0x10, 0) else {
                continue;
            };
            if floor.normal.y < 0.95 || floor.point.y > centre.y - 1.0 {
                continue;
            }
            if level.feeler(floor.point + Vec3::Y * 0.3, *centre, 0x10, 0).map(|h| h.flags & 0x8 == 0).unwrap_or(false) {
                continue;
            }
            ride(&s, &level, floor.point, flat, speed, hold_up);
            return;
        }
    }
    println!("no vert wall with floor in front found");
}

fn ride(s: &Scripts, level: &Level, floor: Vec3, flat: Vec3, speed: f32, hold_up: bool) {
    let mut p = CorePhysics::at_restart(s, floor + Vec3::Y * 0.0025, Vec3::ZERO);
    let at = -flat;
    p.body.matrix.z_axis = at;
    p.body.matrix.x_axis = Vec3::Y.cross(at).normalize();
    p.body.matrix.y_axis = Vec3::Y;
    p.matrix_32 = p.body.matrix;
    p.body.velocity = at * speed;
    println!("start {:?} rolling {:?} at {speed}", floor.to_array(), at.to_array());
    let mut input = InputState::default();
    let mut was_vert = false;
    for i in 0..(6 * 60) {
        if hold_up {
            input.up = true;
            input.up_held_ms = 1000 + i * 1000 / 60;
            input.up_released_ms = input.up_held_ms + 1000;
        }
        let events = p.step(s, &input, level);
        let vert = p.vert.in_vert_air;
        if i % 6 == 5 || !events.is_empty() || vert != was_vert {
            println!(
                "t={:4.2}s pos {:7.2?} vel {:6.2?} {:?}{}{} {events:?}",
                (i + 1) as f32 / 60.0,
                p.body.position.to_array(),
                p.body.velocity.to_array(),
                p.state,
                if vert { " VERT" } else { "" },
                if p.vert.on_vert_ground { " on-vert" } else { "" },
            );
        }
        was_vert = vert;
    }
}
