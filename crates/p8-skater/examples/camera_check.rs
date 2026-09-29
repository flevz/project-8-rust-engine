//! `cargo run --release -p p8-skater --example camera_check -- <DATA/COMPRESSED>`
//! The retail skater camera (`camera.rs`, `820D1238`) following the real
//! scripts on z_houses: push for 3 s, ollie, land, turn right for 1 s, roll.
//! Prints per frame the skater state and position, the camera position and
//! target, the distance and height of the camera from the skater, and the
//! camera's yaw against the direction of travel.
//! P8_VERT=1: roll at the z_houses vert wall near x = -65 instead (vert air).
//! P8_EVERY=<n>: print every n-th frame (default 1). P8_FRAMES=<n>: run n
//! frames (default 600).
use glam::Vec3;
use p8_formats::{qb_key, zone};
use p8_skater::camera::SkaterCamera;
use p8_skater::core_physics::State;
use p8_skater::world::Level;
use p8_skater::{InputState, Scripts, Skater};

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let env = |n: &str, d: usize| std::env::var(n).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (every, frames, vert) = (env("P8_EVERY", 1).max(1), env("P8_FRAMES", 600), std::env::var_os("P8_VERT").is_some());
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let s = Scripts::new(globals);
    let zn = zone::load(&root.join("ZONES"), "z_houses").expect("zone");
    let r = zn.restarts.iter().find(|r| r.name == qb_key("z_houses_TRG_Restart_Default")).unwrap();
    let level = Level::new(&zn.collision);
    let mut k = Skater::new(&s, Vec3::from(r.pos), Vec3::from(r.angles));
    if vert {
        // The spine_bail start, without R2 (no transfer): plain vert air.
        let p = &mut k.physics;
        let at = Vec3::X;
        p.body.position = Vec3::new(-69.67037, 2.6858814 + 0.0025, -85.41025);
        p.body.matrix.z_axis = at;
        p.body.matrix.x_axis = Vec3::Y.cross(at).normalize();
        p.body.matrix.y_axis = Vec3::Y;
        p.matrix_32 = p.body.matrix;
        p.body.velocity = at * 12.0;
    }
    let mut cam = SkaterCamera::new(&s);
    let m = cam.mode;
    println!(
        "mode {} behind {} above {} tilt {} slerp {} lerp_xz {} lerp_y {} vert_air_slerp {} zoom_lerp {}",
        cam.mode_index, m.behind, m.above, m.tilt, m.slerp, m.lerp_xz, m.lerp_y, m.vert_air_slerp, m.zoom_lerp
    );
    let (mut air_seen, mut landed) = (false, None::<usize>);
    for i in 0..frames {
        let t = i as f32 / 60.0;
        let in_air = k.physics.state == State::Air;
        air_seen |= in_air;
        if air_seen && !in_air && landed.is_none() {
            landed = Some(i);
        }
        let input = if vert {
            InputState { up: i < 60, ..Default::default() }
        } else if t < 3.0 {
            InputState { up: true, ..Default::default() }
        } else if t < 3.5 {
            InputState { crouch: true, ..Default::default() }
        } else {
            let turning = landed.is_some_and(|l| (l + 20..l + 80).contains(&i));
            InputState { right: turning, ..Default::default() }
        };
        k.step(&s, &input, &level);
        cam.update(&s, &k.physics, 1.0 / 60.0);
        if i % every != 0 {
            continue;
        }
        let p = &k.physics;
        let pos = p.body.position;
        let d = cam.position - pos;
        let flat = Vec3::new(d.x, 0.0, d.z);
        let v = p.body.velocity;
        // The camera's yaw measured from behind the travel direction (0 =
        // straight behind).
        let behind = -Vec3::new(v.x, 0.0, v.z);
        let yaw = if behind.length() > 0.01 && flat.length() > 0.01 {
            let (a, b) = (behind.normalize(), flat.normalize());
            a.cross(b).y.atan2(a.dot(b)).to_degrees()
        } else {
            f32::NAN
        };
        let state = match p.state {
            State::Ground => "ground",
            State::Air if p.vert.in_vert_air => "vert",
            State::Air => "air",
            State::Lip => "lip",
        };
        println!(
            "f{i:4} {state:6} spd {:5.2} skater ({:7.2} {:6.2} {:7.2}) cam ({:7.2} {:6.2} {:7.2}) target ({:7.2} {:6.2} {:7.2}) back {:5.2} up {:5.2} yaw {:6.1} tilt {:4.2} blend {:3.1}",
            v.length(),
            pos.x, pos.y, pos.z,
            cam.position.x, cam.position.y, cam.position.z,
            cam.target.x, cam.target.y, cam.target.z,
            flat.length(),
            d.y,
            yaw,
            cam.air_tilt,
            cam.air_blend,
        );
    }
}
