//! `cargo run -p p8-sim --example probe [tuning.json]`
//! Prints measurable handling numbers (top push speed, ollie height and air
//! time, 180-spin time) for a tuning, to compare against the original game.
use glam::{Vec2, Vec3};
use p8_sim::{Input, Simulation, State, Tuning, World};

fn main() {
    let tuning = match std::env::args().nth(1) {
        Some(p) => Tuning::from_json(&std::fs::read_to_string(p).expect("read tuning"))
            .expect("valid tuning"),
        None => Tuning::default(),
    };
    let s = 500.0;
    let floor = vec![
        [
            Vec3::new(-s, 0.0, -s),
            Vec3::new(s, 0.0, -s),
            Vec3::new(s, 0.0, s),
        ],
        [
            Vec3::new(-s, 0.0, -s),
            Vec3::new(s, 0.0, s),
            Vec3::new(-s, 0.0, s),
        ],
    ];
    let world = World::new(floor, vec![]);
    let dt = 1.0 / 60.0;
    let mut sim = Simulation::new(tuning.clone(), Vec3::ZERO, 0.0, Vec3::ZERO);
    let push = Input {
        stick: Vec2::Y,
        ..Default::default()
    };
    let mut t = 0.0;
    while sim.skater.speed() < tuning.max_push_speed * 0.99 && t < 30.0 {
        sim.step(&push, dt, &world);
        t += dt;
    }
    println!("push to {:.2} m/s in {:.2} s", sim.skater.speed(), t);
    for _ in 0..(0.4 / dt) as usize {
        sim.step(
            &Input {
                ollie: true,
                ..Default::default()
            },
            dt,
            &world,
        );
    }
    sim.step(&Input::default(), dt, &world);
    let (mut apex, mut air) = (0.0f32, 0.0f32);
    while sim.skater.state == State::Air && air < 5.0 {
        sim.step(&Input::default(), dt, &world);
        apex = apex.max(sim.skater.position.y);
        air += dt;
    }
    println!("full-crouch ollie: apex {apex:.2} m, air time {air:.2} s");
    println!(
        "180 spin takes {:.2} s",
        std::f32::consts::PI / tuning.air_spin_rate
    );
}
