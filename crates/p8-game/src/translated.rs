//! Play mode for the translated skater (`p8-skater`), driven by the player's
//! own Project 8 scripts.
//!
//! Translated so far: riding, the ollie, flying and landing. The level is a
//! flat floor; until the retail ground snapping is translated the height is
//! held at 0 while on the ground. The camera is a simple placeholder, not
//! the retail camera.
use bevy::prelude::*;
use p8_skater::core_physics::{Event, State, Turn};
use p8_skater::{CorePhysics, FlatFloor, InputState, Scripts};

const TICK_HZ: f64 = 60.0;

pub struct TranslatedPlugin {
    pub scripts: Scripts,
    pub source: String,
}

impl Plugin for TranslatedPlugin {
    fn build(&self, app: &mut App) {
        let physics = CorePhysics::new(&self.scripts);
        let frame = Frame::of(&physics);
        app.insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
            .insert_resource(ClearColor(Color::srgb(0.53, 0.72, 0.9)))
            .insert_resource(Skater {
                scripts: self.scripts.clone(),
                source: self.source.clone(),
                physics,
                previous: frame,
                current: frame,
                left_ms: 0.0,
                right_ms: 0.0,
                last_event: None,
                eye: Vec3::new(0.0, 2.0, -5.0),
            })
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, step)
            .add_systems(Update, (present, hud));
    }
}

#[derive(Resource)]
struct Skater {
    scripts: Scripts,
    source: String,
    physics: CorePhysics,
    previous: Frame,
    current: Frame,
    /// How long the digital left/right inputs have been held (ms).
    left_ms: f32,
    right_ms: f32,
    last_event: Option<Event>,
    /// Placeholder chase camera position.
    eye: Vec3,
}

#[derive(Clone, Copy)]
struct Frame {
    position: Vec3,
    rotation: Quat,
}

impl Frame {
    fn of(p: &CorePhysics) -> Self {
        // Matrix columns are the retail rows (row 0, up, at), which map the
        // local X/Y/Z axes of the board model.
        Self { position: p.body.position, rotation: Quat::from_mat3(&p.body.matrix).normalize() }
    }
}

#[derive(Component)]
struct SkaterRoot;
#[derive(Component)]
struct Rider;
#[derive(Component)]
struct StatusText;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Flat test floor with posts every 10 m to show speed and direction.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.55, 0.55),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let post = meshes.add(Cuboid::new(0.15, 1.0, 0.15));
    let post_color = materials.add(Color::srgb(0.9, 0.9, 0.9));
    let line = meshes.add(Cuboid::new(0.06, 0.01, 400.0));
    let line_color = materials.add(Color::srgb(0.45, 0.45, 0.45));
    for i in -20..=20 {
        let x = i as f32 * 10.0;
        commands.spawn((Mesh3d(line.clone()), MeshMaterial3d(line_color.clone()), Transform::from_xyz(x, 0.005, 0.0)));
        commands.spawn((
            Mesh3d(line.clone()),
            MeshMaterial3d(line_color.clone()),
            Transform::from_xyz(0.0, 0.005, x).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        ));
        for j in -20..=20 {
            if (i + j) % 4 == 0 {
                commands.spawn((
                    Mesh3d(post.clone()),
                    MeshMaterial3d(post_color.clone()),
                    Transform::from_xyz(x, 0.5, j as f32 * 10.0),
                ));
            }
        }
    }

    // Placeholder skater: a board and a rider. Project 8 models come later.
    commands
        .spawn((SkaterRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.22, 0.05, 0.82))),
                MeshMaterial3d(materials.add(Color::srgb(0.12, 0.12, 0.12))),
                Transform::from_xyz(0.0, 0.09, 0.0),
            ));
            root.spawn((
                Rider,
                Mesh3d(meshes.add(Capsule3d::new(0.2, 0.95))),
                MeshMaterial3d(materials.add(Color::srgb(0.95, 0.55, 0.1))),
                Transform::from_xyz(0.0, 0.8, 0.0),
            ));
        });

    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(20.0, 40.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, -5.0).looking_at(Vec3::Y, Vec3::Y)));

    commands.spawn((
        Text::new(
            "Translated Project 8 physics (flat test floor; no spins, tricks or ramps yet)\n\
             Controller: hold A to crouch (and push), release A to ollie, left stick steer, pull back to brake,\n\
             D-pad left/right steer, D-pad down brake, Back reset\n\
             Keyboard: hold Space to crouch, release to ollie, A/D steer, S brake, R reset",
        ),
        TextFont { font_size: 15.0, ..default() },
        Node { position_type: PositionType::Absolute, left: px(12.0), bottom: px(12.0), ..default() },
    ));
    commands.spawn((
        StatusText,
        Text::new(""),
        TextFont { font_size: 20.0, ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.2)),
        Node { position_type: PositionType::Absolute, left: px(12.0), top: px(12.0), ..default() },
    ));
}

/// Retail stick units: -128..127 (scaled by 1/128 in the game code).
fn retail_axis(v: f32) -> f32 {
    (v * 128.0).clamp(-128.0, 127.0).round()
}

fn read_input(
    keys: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    skater: &mut Skater,
    dt_ms: f32,
) -> (InputState, bool) {
    let key = |k| if keys.pressed(k) { 1.0 } else { 0.0 };
    // Keyboard keys act like the stick at full tilt.
    let mut stick = Vec2::new(key(KeyCode::KeyD) - key(KeyCode::KeyA), key(KeyCode::KeyW) - key(KeyCode::KeyS));
    let mut input = InputState { crouch: keys.pressed(KeyCode::Space), ..default() };
    let mut reset = keys.just_pressed(KeyCode::KeyR);
    for pad in gamepads {
        stick += pad.left_stick();
        // A holds the crouch (CONFIRMED by play and code, see p8-skater).
        input.crouch |= pad.pressed(GamepadButton::South);
        // D-pad records "Left"/"Right"/"Down" (named by retail 822D6270).
        input.left |= pad.pressed(GamepadButton::DPadLeft);
        input.right |= pad.pressed(GamepadButton::DPadRight);
        input.brake_digital |= pad.pressed(GamepadButton::DPadDown);
        reset |= pad.just_pressed(GamepadButton::Select);
    }
    let stick = stick.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
    input.stick_x_raw = retail_axis(stick.x);
    // Retail +876 is positive when the stick is pulled back.
    input.stick_back_raw = retail_axis(-stick.y);
    skater.left_ms = if input.left { skater.left_ms + dt_ms } else { 0.0 };
    skater.right_ms = if input.right { skater.right_ms + dt_ms } else { 0.0 };
    input.left_held_ms = skater.left_ms as i32;
    input.right_held_ms = skater.right_ms as i32;
    (input, reset)
}

fn step(
    mut skater: ResMut<Skater>,
    time: Res<Time<Fixed>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    let skater = &mut *skater;
    let dt = time.delta_secs();
    let (input, reset) = read_input(&keys, &gamepads, skater, dt * 1000.0);
    if reset {
        skater.physics = CorePhysics::new(&skater.scripts);
        skater.current = Frame::of(&skater.physics);
    }
    skater.previous = skater.current;
    let p = &mut skater.physics;
    p.dt = dt;
    let events = p.step(&skater.scripts, &input, &FlatFloor::default());
    // Flat floor stand-in for ground snapping (not yet translated).
    if p.state == State::Ground {
        p.body.position.y = 0.0;
    }
    if let Some(e) = events.last() {
        skater.last_event = Some(*e);
    }
    skater.current = Frame::of(&skater.physics);
}

#[allow(clippy::type_complexity)]
fn present(
    mut skater: ResMut<Skater>,
    time: Res<Time<Fixed>>,
    real: Res<Time>,
    mut root: Query<&mut Transform, (With<SkaterRoot>, Without<Camera3d>, Without<Rider>)>,
    mut rider: Query<&mut Transform, (With<Rider>, Without<SkaterRoot>, Without<Camera3d>)>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<SkaterRoot>, Without<Rider>)>,
) {
    let a = time.overstep_fraction();
    let position = skater.previous.position.lerp(skater.current.position, a);
    let rotation = skater.previous.rotation.slerp(skater.current.rotation, a);
    if let Ok(mut t) = root.single_mut() {
        t.translation = position;
        t.rotation = rotation;
    }
    let crouched = skater.physics.crouched;
    if let Ok(mut t) = rider.single_mut() {
        t.translation.y = if crouched { 0.62 } else { 0.8 };
        t.scale = Vec3::new(1.0, if crouched { 0.75 } else { 1.0 }, 1.0);
    }
    // Placeholder chase camera: behind and above, eased toward its goal.
    let at = rotation * Vec3::Z;
    let flat = Vec3::new(at.x, 0.0, at.z).normalize_or(Vec3::Z);
    let goal = position - flat * 4.5 + Vec3::Y * 2.0;
    let ease = 1.0 - (-6.0 * real.delta_secs()).exp();
    skater.eye = skater.eye.lerp(goal, ease);
    if let Ok(mut t) = camera.single_mut() {
        *t = Transform::from_translation(skater.eye).looking_at(position + Vec3::Y * 1.0, Vec3::Y);
    }
}

fn hud(skater: Res<Skater>, mut text: Query<&mut Text, With<StatusText>>) {
    let p = &skater.physics;
    let turn = match p.last_turn {
        Some(Turn::Left) => "left",
        Some(Turn::Right) => "right",
        None => "-",
    };
    let line = format!(
        "speed {:5.2} m/s   height {:4.2} m   {:?}   {}{}   turn {turn}   last event {:?}\n[original scripts: {}]",
        p.body.velocity.length(),
        p.body.position.y,
        p.state,
        if p.crouched { "CROUCHED" } else { "standing" },
        if p.braking { "  BRAKING" } else { "" },
        skater.last_event,
        skater.source,
    );
    if let Ok(mut t) = text.single_mut()
        && **t != line
    {
        **t = line;
    }
}
