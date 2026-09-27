//! `cargo run -p p8-skater --example transfer_ride -- <qb.pak.xen> [spine|acid] [zone] [speed]`
//! Spine transfers and acid drops on a real level, with R2 (the spine
//! button) held. Runs the physics alone (like `vert_ride`): the scripts the
//! physics asks for (the award scripts) are printed, not run.
//!
//! - `spine`: for each vert wall near the default restart with floor in
//!   front, ride up it holding R2 and report whether a transfer started,
//!   where it was aimed and where the skater landed.
//! - `acid`: for each vert wall with a deck behind its top, roll off the
//!   deck over the coping holding R2 and report the drop.
use glam::Vec3;
use p8_formats::havok::{Solid, split_material};
use p8_formats::{qb_key, zone};
use p8_skater::core_physics::{Event, State};
use p8_skater::world::{Level, World};
use p8_skater::transfer::ScriptAction;
use p8_skater::{CorePhysics, InputState, Scripts};

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to qb.pak.xen"));
    let mode = std::env::args().nth(2).unwrap_or("spine".into());
    let name = std::env::args().nth(3).unwrap_or("z_houses".into());
    let speed: f32 = std::env::args().nth(4).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let (_, globals) = p8_formats::qb::load_pak_globals(&path).expect("load scripts");
    let s = Scripts::new(globals);
    let zones = path.parent().and_then(|p| p.parent()).expect("DATA/COMPRESSED").join("ZONES");
    let z = zone::load(&zones, &name).expect("zone");
    let default = qb_key(&format!("{name}_TRG_Restart_Default"));
    let r = z.restarts.iter().find(|r| r.name == default).expect("default restart");
    let level = Level::new(&z.collision);

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

    let mut tried = 0;
    for (_, centre, n) in walls.iter().take(400) {
        for n in [*n, -*n] {
            let flat = Vec3::new(n.x, 0.0, n.z).normalize_or_zero();
            let start = if mode == "acid" {
                // The deck: above the wall's top, 1.5 m behind it.
                let behind = *centre - flat * 1.5;
                let Some(deck) = level.feeler(behind + Vec3::Y * 6.0, behind - Vec3::Y * 0.5, 0x10, 0) else {
                    continue;
                };
                if deck.normal.y < 0.95 || deck.point.y < centre.y + 0.3 {
                    continue;
                }
                (deck.point, flat)
            } else {
                let out = *centre + flat * 4.0;
                let Some(floor) = level.feeler(out + Vec3::Y * 1.0, out - Vec3::Y * 6.0, 0x10, 0) else {
                    continue;
                };
                if floor.normal.y < 0.95 || floor.point.y > centre.y - 1.0 {
                    continue;
                }
                let blocked = level.feeler(floor.point + Vec3::Y * 0.3, *centre, 0x10, 0);
                if blocked.map(|h| h.flags & 0x8 == 0).unwrap_or(false) {
                    continue;
                }
                (floor.point, -flat)
            };
            tried += 1;
            if ride(&s, &level, start.0, start.1, speed, mode == "acid") {
                return;
            }
            if tried >= 30 {
                println!("tried {tried} places, no {mode} happened");
                return;
            }
        }
    }
    println!("tried {tried} places, no {mode} happened");
}

/// One ride; prints it and returns true if a transfer or drop started.
fn ride(s: &Scripts, level: &Level, floor: Vec3, at: Vec3, speed: f32, acid: bool) -> bool {
    let mut k = Ride { physics: CorePhysics::at_restart(s, floor + Vec3::Y * 0.0025, Vec3::ZERO) };
    let p = &mut k.physics;
    p.body.matrix.z_axis = at;
    p.body.matrix.x_axis = Vec3::Y.cross(at).normalize();
    p.body.matrix.y_axis = Vec3::Y;
    p.matrix_32 = p.body.matrix;
    p.body.velocity = at * speed;
    let input = InputState { r2: true, ..Default::default() };
    let mut lines = Vec::new();
    lines.push(format!("start {:?} rolling {:?} at {speed}", floor.to_array(), at.to_array()));
    let mut started = false;
    let mut was_active = false;
    for i in 0..(6 * 60) {
        let events = k.physics.step(s, &input, level);
        for a in k.physics.transfer.actions.drain(..) {
            if let ScriptAction::Run(name, params) = a {
                let known = ["SkaterAwardTransfer", "SkaterAwardHipTransfer", "SkaterAwardTransferToBank", "SkaterAcidDropTriggered"];
                let n = known.iter().find(|n| qb_key(n) == name).copied().unwrap_or("?");
                lines.push(format!("  script {n} {params:?}"));
            }
        }
        let p = &k.physics;
        let t = &p.transfer;
        let active = t.active;
        if active && !was_active {
            started = true;
            lines.push(format!(
                "  START at {:?}: carry {:?} vel {:?} duration {:.2}s target_at {:?} bank {}",
                p.body.position.to_array(),
                t.carry.to_array(),
                p.body.velocity.to_array(),
                t.duration,
                t.target_at.to_array(),
                t.bank
            ));
        }
        if i % 6 == 5 || !events.is_empty() || active != was_active {
            lines.push(format!(
                "t={:4.2}s pos {:7.2?} vel {:6.2?} up {:5.2?} {:?}{}{}{} {events:?}",
                (i + 1) as f32 / 60.0,
                p.body.position.to_array(),
                p.body.velocity.to_array(),
                p.body.up().to_array(),
                p.state,
                if p.vert.in_vert_air { " VERT" } else { "" },
                if active { " TRANSFER" } else { "" },
                if t.retry { " retry" } else { "" },
            ));
        }
        if started && events.contains(&Event::Landed) {
            let p = &k.physics;
            lines.push(format!(
                "  LANDED: speed {:.2} LandedFromSpine {} LandedOnBank {} up {:?}",
                p.body.velocity.length(),
                p.transfer.landed_from_spine,
                p.transfer.landed_on_bank,
                p.body.up().to_array()
            ));
            for _ in 0..60 {
                k.physics.step(s, &input, level);
            }
            let p = &k.physics;
            lines.push(format!(
                "  1 s later: {:?} speed {:.2} speed factor {:.3} override {:?}",
                p.state,
                p.body.velocity.length(),
                p.transfer.speed_factor,
                p.override_limits.map(|o| (o.max, o.max_max))
            ));
            break;
        }
        if !started && k.physics.state == State::Ground && i > 120 && !acid {
            break;
        }
        was_active = active;
    }
    if started || std::env::var_os("P8_ALL").is_some() {
        for l in lines {
            println!("{l}");
        }
    }
    started
}

struct Ride {
    physics: CorePhysics,
}
