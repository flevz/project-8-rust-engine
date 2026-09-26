//! Headless behaviour tests. They check mechanics, not feel or fidelity.
use super::*;
use glam::{Vec2, Vec3};

const DT: f32 = 1.0 / 60.0;

fn flat() -> World {
    World::new(world::test_floor(200.0), vec![])
}

fn run(sim: &mut Simulation, world: &World, input: Input, seconds: f32) {
    for _ in 0..(seconds / DT).round() as usize {
        sim.step(&input, DT, world);
    }
}

fn push() -> Input {
    Input {
        stick: Vec2::new(0.0, 1.0),
        ..Default::default()
    }
}

#[test]
fn pushing_accelerates_to_push_speed_and_friction_slows() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::ZERO);
    run(&mut sim, &w, push(), 4.0);
    let top = sim.skater.speed();
    assert!(
        (top - sim.tuning.max_push_speed).abs() < 0.3,
        "top speed {top}"
    );
    assert!(sim.skater.position.z > 10.0, "heading 0 travels +Z");
    assert!(sim.skater.position.y.abs() < 1e-3);
    run(&mut sim, &w, Input::default(), 1.0);
    assert!(sim.skater.speed() < top);
    assert_eq!(sim.skater.state, State::Ground);
}

#[test]
fn stick_right_turns_right() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 5.0));
    run(
        &mut sim,
        &w,
        Input {
            stick: Vec2::new(1.0, 0.0),
            ..Default::default()
        },
        0.3,
    );
    assert!(
        sim.skater.heading < 0.0,
        "heading decreases when turning right"
    );
    assert!(sim.skater.position.x < 0.0, "right of +Z is -X");
}

#[test]
fn ollie_leaves_ground_and_lands_cleanly() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 6.0));
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.2,
    );
    sim.step(&Input::default(), DT, &w);
    assert_eq!(sim.skater.state, State::Air);
    let mut apex: f32 = 0.0;
    for _ in 0..120 {
        sim.step(&Input::default(), DT, &w);
        apex = apex.max(sim.skater.position.y);
    }
    assert!(apex > 0.8 && apex < 2.0, "apex {apex}");
    assert_eq!(sim.skater.state, State::Ground);
    assert_eq!(sim.skater.last_landing.as_deref(), Some("Ollie"));
}

#[test]
fn half_spin_lands_fakie_and_a_quarter_spin_bails() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 6.0));
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.35,
    );
    sim.step(&Input::default(), DT, &w);
    let spin_time = std::f32::consts::PI / sim.tuning.air_spin_rate;
    run(
        &mut sim,
        &w,
        Input {
            spin_left: true,
            ..Default::default()
        },
        spin_time,
    );
    run(&mut sim, &w, Input::default(), 1.5);
    assert_eq!(sim.skater.state, State::Ground);
    assert!(sim.skater.fakie);
    assert!(
        sim.skater
            .last_landing
            .as_deref()
            .unwrap()
            .contains("180 Spin")
    );

    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 6.0));
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.35,
    );
    sim.step(&Input::default(), DT, &w);
    run(
        &mut sim,
        &w,
        Input {
            spin_left: true,
            ..Default::default()
        },
        spin_time / 2.0,
    );
    for _ in 0..90 {
        sim.step(&Input::default(), DT, &w);
        if matches!(sim.skater.state, State::Bail { .. }) {
            return;
        }
    }
    panic!("a sideways landing must bail");
}

#[test]
fn flip_requires_time_to_complete() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 6.0));
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.35,
    );
    sim.step(&Input::default(), DT, &w);
    run(
        &mut sim,
        &w,
        Input {
            flip: true,
            ..Default::default()
        },
        0.1,
    );
    run(&mut sim, &w, Input::default(), 1.5);
    assert_eq!(sim.skater.state, State::Ground);
    assert!(
        sim.skater
            .last_landing
            .as_deref()
            .unwrap()
            .contains("Kickflip")
    );
}

#[test]
fn walls_stop_head_on_motion() {
    let size = 50.0;
    let mut tris = world::test_floor(size);
    // A wall across +Z at z = 5.
    let (a, b, c, d) = (
        Vec3::new(-size, 0.0, 5.0),
        Vec3::new(size, 0.0, 5.0),
        Vec3::new(size, 3.0, 5.0),
        Vec3::new(-size, 3.0, 5.0),
    );
    tris.push([a, b, c]);
    tris.push([a, c, d]);
    let w = World::new(tris, vec![]);
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 8.0));
    run(&mut sim, &w, Input::default(), 2.0);
    assert!(
        sim.skater.position.z < 5.0,
        "went through the wall: {}",
        sim.skater.position.z
    );
    assert!(sim.skater.speed() < 4.0);
}

#[test]
fn rolling_off_a_ledge_becomes_air_then_lands_below() {
    let mut tris = world::test_floor(100.0);
    for t in &mut tris {
        for v in t.iter_mut() {
            v.y -= 2.0;
        }
    }
    // A raised deck ending at z = 3.
    let (a, b, c, d) = (
        Vec3::new(-5.0, 0.0, -5.0),
        Vec3::new(5.0, 0.0, -5.0),
        Vec3::new(5.0, 0.0, 3.0),
        Vec3::new(-5.0, 0.0, 3.0),
    );
    tris.push([a, b, c]);
    tris.push([a, c, d]);
    let w = World::new(tris, vec![]);
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 6.0));
    let mut was_air = false;
    for _ in 0..120 {
        sim.step(&Input::default(), DT, &w);
        was_air |= sim.skater.state == State::Air;
    }
    assert!(was_air);
    assert_eq!(sim.skater.state, State::Ground);
    assert!((sim.skater.position.y + 2.0).abs() < 1e-3);
}

#[test]
fn grind_holds_the_rail_and_ollie_leaves_it() {
    let rail = Rail {
        start: Vec3::new(0.0, 0.6, 2.0),
        end: Vec3::new(0.0, 0.6, 30.0),
    };
    let w = World::new(world::test_floor(100.0), vec![rail]);
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 7.0));
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.35,
    );
    sim.step(&Input::default(), DT, &w);
    let mut grinding = false;
    for _ in 0..60 {
        sim.step(
            &Input {
                grind: true,
                ..Default::default()
            },
            DT,
            &w,
        );
        if matches!(sim.skater.state, State::Grind { .. }) {
            grinding = true;
            break;
        }
    }
    assert!(grinding, "should lock onto the rail");
    run(&mut sim, &w, Input::default(), 0.3);
    assert!(matches!(sim.skater.state, State::Grind { .. }));
    assert!((sim.skater.position.y - 0.6).abs() < 1e-3 && sim.skater.position.x.abs() < 1e-3);
    run(
        &mut sim,
        &w,
        Input {
            ollie: true,
            ..Default::default()
        },
        0.05,
    );
    sim.step(&Input::default(), DT, &w);
    assert_eq!(sim.skater.state, State::Air);
}

#[test]
fn unattended_manual_eventually_bails_and_recovers() {
    let w = flat();
    let mut sim = Simulation::new(Tuning::default(), Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 5.0));
    for stick in [1.0, 0.0, -1.0, 0.0] {
        sim.step(
            &Input {
                stick: Vec2::new(0.0, stick),
                ..Default::default()
            },
            DT,
            &w,
        );
    }
    assert!(
        matches!(sim.skater.state, State::Manual { nose: false, .. }),
        "{:?}",
        sim.skater.state
    );
    run(&mut sim, &w, Input::default(), 2.0);
    assert!(matches!(sim.skater.state, State::Bail { .. }));
    run(&mut sim, &w, Input::default(), 2.0);
    assert_eq!(sim.skater.state, State::Ground);
}

#[test]
fn simulation_is_deterministic() {
    let w = flat();
    let script = |i: usize| Input {
        stick: Vec2::new(((i as f32) * 0.1).sin(), 1.0),
        ollie: i % 50 < 20,
        ..Default::default()
    };
    let mut a = Simulation::new(Tuning::default(), Vec3::ZERO, 0.3, Vec3::ZERO);
    let mut b = Simulation::new(Tuning::default(), Vec3::ZERO, 0.3, Vec3::ZERO);
    for i in 0..600 {
        a.step(&script(i), DT, &w);
        b.step(&script(i), DT, &w);
    }
    assert_eq!(a.skater.position, b.skater.position);
}

#[test]
fn tuning_overrides_and_rejects_bad_values() {
    let t = Tuning::from_json(r#"{"gravity": 20.0}"#).unwrap();
    assert_eq!(t.gravity, 20.0);
    assert_eq!(t.ollie_speed, Tuning::default().ollie_speed);
    assert!(Tuning::from_json(r#"{"gravitee": 20.0}"#).is_err());
    assert!(Tuning::from_json(r#"{"gravity": -1.0}"#).is_err());
}

#[test]
fn landing_discards_falling_speed() {
    let w = flat();
    // Dropped from 3 m while rolling forward at 4 m/s.
    let mut sim = Simulation::new(
        Tuning::default(),
        Vec3::new(0.0, 3.0, 0.0),
        0.0,
        Vec3::new(0.0, 1.0, 4.0),
    );
    run(&mut sim, &w, Input::default(), 1.5);
    assert_eq!(sim.skater.state, State::Ground);
    assert!(
        sim.skater.speed() <= 4.0,
        "landing must not convert fall speed: {}",
        sim.skater.speed()
    );
}
