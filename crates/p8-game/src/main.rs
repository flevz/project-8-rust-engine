//! Project 8 Rust Engine: playable prototype.
//!
//! Stage 1 of `docs/ROADMAP.md`: the `p8-sim` skater in an original test park
//! with placeholder visuals. Project 8 levels, models and animations arrive in
//! later stages through converters that read the player's own installation.
mod park;

use bevy::prelude::*;
use p8_sim::{Input, Pose, Simulation, State, Tuning, World};
use std::sync::Arc;

const TICK_HZ: f64 = 60.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Project 8 Rust Engine (prototype)".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
        .insert_resource(ClearColor(Color::srgb(0.53, 0.72, 0.9)))
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, step)
        .add_systems(Update, (present, hud))
        .run();
}

#[derive(Resource)]
struct Game {
    sim: Simulation,
    world: Arc<World>,
    spawn: (Vec3, f32),
    previous: Frame,
    current: Frame,
    tuning_note: String,
}

/// One simulated tick, as presented. Rendering interpolates between two.
#[derive(Clone, Copy)]
struct Frame {
    position: Vec3,
    rotation: Quat,
    eye: Vec3,
    target: Vec3,
}

#[derive(Component)]
struct SkaterRoot;
#[derive(Component)]
struct Body;
#[derive(Component)]
struct Board;
#[derive(Component)]
struct StatusText;

fn load_tuning() -> (Tuning, String) {
    match std::fs::read_to_string("tuning.json") {
        Ok(text) => match Tuning::from_json(&text) {
            Ok(t) => (t, "tuning.json loaded".into()),
            Err(e) => (Tuning::default(), format!("tuning.json ignored: {e}")),
        },
        Err(_) => (
            Tuning::default(),
            "default tuning (temporary values)".into(),
        ),
    }
}

fn frame(sim: &Simulation) -> Frame {
    let o = sim.output();
    Frame {
        position: o.position,
        rotation: o.rotation,
        eye: o.camera_eye,
        target: o.camera_target,
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let park = park::build();
    for piece in &park.pieces {
        commands.spawn((
            Mesh3d(meshes.add(park::mesh(&piece.triangles))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: piece.color,
                perceptual_roughness: 0.9,
                ..default()
            })),
        ));
    }
    let triangles: Vec<_> = park
        .pieces
        .iter()
        .flat_map(|p| p.triangles.iter().copied())
        .collect();
    let world = Arc::new(World::new(triangles, park.rails.clone()));
    let (tuning, tuning_note) = load_tuning();
    let sim = Simulation::new(tuning, park.spawn, park.spawn_heading, Vec3::ZERO);
    let first = frame(&sim);
    commands.insert_resource(Game {
        sim,
        world,
        spawn: (park.spawn, park.spawn_heading),
        previous: first,
        current: first,
        tuning_note,
    });

    // Placeholder skater: a board and a body. Project 8 models come later.
    commands
        .spawn((
            SkaterRoot,
            Transform::from_translation(park.spawn),
            Visibility::default(),
        ))
        .with_children(|root| {
            root.spawn((
                Board,
                Mesh3d(meshes.add(Cuboid::new(0.22, 0.05, 0.82))),
                MeshMaterial3d(materials.add(Color::srgb(0.12, 0.12, 0.12))),
                Transform::from_xyz(0.0, 0.09, 0.0),
            ));
            root.spawn((
                Body,
                Mesh3d(meshes.add(Capsule3d::new(0.2, 0.95))),
                MeshMaterial3d(materials.add(Color::srgb(0.95, 0.55, 0.1))),
                Transform::from_xyz(0.0, 0.8, 0.0),
            ));
        });

    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 40.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(first.eye).looking_at(first.target, Vec3::Y),
    ));

    commands.spawn((
        Text::new(
            "Controller: left stick push/steer/balance, A ollie (hold to crouch), X flip, B grab, Y grind,\n\
             LB/RB spin, stick up-then-down manual, Back reset\n\
             Keyboard: W/A/S/D, Space ollie, J flip, K grab, L grind, Q/E spin, R reset",
        ),
        TextFont { font_size: 15.0, ..default() },
        Node { position_type: PositionType::Absolute, left: px(12.0), bottom: px(12.0), ..default() },
    ));
    commands.spawn((
        StatusText,
        Text::new(""),
        TextFont {
            font_size: 22.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.9, 0.2)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12.0),
            top: px(12.0),
            ..default()
        },
    ));
}

fn read_input(keys: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>) -> (Input, bool) {
    let key = |k| if keys.pressed(k) { 1.0 } else { 0.0 };
    let mut stick = Vec2::new(
        key(KeyCode::KeyD) - key(KeyCode::KeyA),
        key(KeyCode::KeyW) - key(KeyCode::KeyS),
    );
    let mut input = Input {
        ollie: keys.pressed(KeyCode::Space),
        flip: keys.pressed(KeyCode::KeyJ),
        grab: keys.pressed(KeyCode::KeyK),
        grind: keys.pressed(KeyCode::KeyL),
        spin_left: keys.pressed(KeyCode::KeyQ),
        spin_right: keys.pressed(KeyCode::KeyE),
        ..default()
    };
    let mut reset = keys.just_pressed(KeyCode::KeyR);
    for pad in gamepads {
        let s = pad.left_stick();
        let dead = |v: f32| if v.abs() > 0.2 { v } else { 0.0 };
        stick += Vec2::new(dead(s.x), dead(s.y));
        input.ollie |= pad.pressed(GamepadButton::South);
        input.flip |= pad.pressed(GamepadButton::West);
        input.grab |= pad.pressed(GamepadButton::East);
        input.grind |= pad.pressed(GamepadButton::North);
        input.spin_left |= pad.pressed(GamepadButton::LeftTrigger);
        input.spin_right |= pad.pressed(GamepadButton::RightTrigger);
        reset |= pad.just_pressed(GamepadButton::Select);
    }
    input.stick = stick.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
    (input, reset)
}

fn step(
    mut game: ResMut<Game>,
    time: Res<Time<Fixed>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    let (input, reset) = read_input(&keys, &gamepads);
    let game = &mut *game;
    if reset {
        game.sim.reset(game.spawn.0, game.spawn.1, Vec3::ZERO);
        game.previous = frame(&game.sim);
    } else {
        game.previous = game.current;
    }
    game.sim.step(&input, time.delta_secs(), &game.world);
    game.current = frame(&game.sim);
}

#[allow(clippy::type_complexity)]
fn present(
    game: Res<Game>,
    time: Res<Time<Fixed>>,
    mut root: Query<
        &mut Transform,
        (
            With<SkaterRoot>,
            Without<Camera3d>,
            Without<Body>,
            Without<Board>,
        ),
    >,
    mut body: Query<&mut Transform, (With<Body>, Without<Board>, Without<Camera3d>)>,
    mut board: Query<&mut Transform, (With<Board>, Without<Body>, Without<Camera3d>)>,
    mut camera: Query<(&mut Transform, &mut Projection), (With<Camera3d>, Without<SkaterRoot>)>,
) {
    let a = time.overstep_fraction();
    let (p, c) = (&game.previous, &game.current);
    let out = game.sim.output();
    let tuning = &game.sim.tuning;

    // Placeholder "animation": tilt and squash simple shapes per pose.
    let tilt = match (out.pose, game.sim.skater.state) {
        (Pose::Manual, _) => Quat::from_rotation_x(-0.3),
        (Pose::NoseManual, _) => Quat::from_rotation_x(0.3),
        (Pose::Bail, _) => Quat::from_rotation_z(1.3),
        (_, State::Grind { balance, .. }) => Quat::from_rotation_z(balance * 0.6),
        _ => Quat::IDENTITY,
    };
    if let Ok(mut t) = root.single_mut() {
        t.translation = p.position.lerp(c.position, a);
        t.rotation = p.rotation.slerp(c.rotation, a) * tilt;
    }
    let crouch = matches!(out.pose, Pose::Crouch | Pose::Grab);
    if let Ok(mut t) = body.single_mut() {
        t.translation.y = if crouch { 0.62 } else { 0.8 };
        t.scale = Vec3::new(1.0, if crouch { 0.75 } else { 1.0 }, 1.0);
    }
    if let Ok(mut t) = board.single_mut() {
        let flip = if out.pose == Pose::Flip {
            (out.pose_time / tuning.flip_duration).min(1.0) * std::f32::consts::TAU
        } else {
            0.0
        };
        t.rotation = Quat::from_rotation_z(flip);
        t.translation.y = if out.pose == Pose::Flip { 0.35 } else { 0.09 };
    }
    if let Ok((mut t, mut projection)) = camera.single_mut() {
        *t = Transform::from_translation(p.eye.lerp(c.eye, a))
            .looking_at(p.target.lerp(c.target, a), Vec3::Y);
        if let Projection::Perspective(pp) = &mut *projection {
            pp.fov = out.fov_degrees.to_radians();
        }
    }
}

fn hud(game: Res<Game>, mut text: Query<&mut Text, With<StatusText>>) {
    if let Ok(mut t) = text.single_mut() {
        let line = format!("{}\n[{}]", game.sim.output().hud, game.tuning_note);
        if **t != line {
            **t = line;
        }
    }
}
