//! Play mode for the translated skater (`p8-skater`), driven by the player's
//! own Project 8 scripts.
//!
//! Translated so far: riding, the ollie, flying, landing, and staying on
//! the ground and bumping walls on the player's own level collision (drawn
//! as-is: the level's real textured models come later). Without the zone
//! files it falls back to a flat floor. The camera is a simple
//! placeholder, not the retail camera.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use p8_formats::havok::Solid;
use p8_formats::zone::Zone;
use p8_skater::core_physics::{Event, Turn};
use p8_skater::world::Level;
use p8_skater::{Controller, CorePhysics, FlatFloor, Scripts, World, XboxPad};

const TICK_HZ: f64 = 60.0;

pub struct TranslatedPlugin {
    pub scripts: Scripts,
    pub source: String,
    pub zone_name: &'static str,
    /// The player's zone, or why it could not be read.
    pub zone: Result<Zone, String>,
    /// The player's `ZONES` folder (HUD textures), if known.
    pub zones_dir: Option<std::path::PathBuf>,
}

impl Plugin for TranslatedPlugin {
    fn build(&self, app: &mut App) {
        let ground = match &self.zone {
            Ok(zone) => {
                let key = p8_formats::qb_key(&format!("{}_TRG_Restart_Default", self.zone_name));
                let (pos, angles) = zone
                    .restarts
                    .iter()
                    .find(|r| r.name == key)
                    .map(|r| (Vec3::from(r.pos), Vec3::from(r.angles)))
                    .unwrap_or_default();
                Ground {
                    level: Some(Level::new(&zone.collision).with_rails(&zone.rails, &|t| self.scripts.terrain_index(t))),
                    convex: zone.collision.solids.iter().filter(|s| !matches!(s, Solid::Triangle { .. })).cloned().collect(),
                    spawn: (pos, angles),
                    note: format!("{} ({} collision pieces)", self.zone_name, zone.collision.solids.len()),
                }
            }
            Err(why) => Ground { level: None, convex: Vec::new(), spawn: (Vec3::ZERO, Vec3::ZERO), note: format!("flat floor: {why}") },
        };
        let object = p8_skater::Skater::new(&self.scripts, ground.spawn.0, ground.spawn.1);
        let frame = Frame::of(&object.physics);
        let eye = object.physics.body.position - object.physics.body.at() * 4.5 + Vec3::Y * 2.0;
        let cam_dir = object.physics.body.at();
        app.insert_resource(Time::<Fixed>::from_hz(TICK_HZ))
            .insert_resource(ClearColor(Color::srgb(0.53, 0.72, 0.9)))
            .insert_resource(Skater {
                scripts: self.scripts.clone(),
                source: self.source.clone(),
                object,
                previous: frame,
                current: frame,
                controller: Controller::default(),
                last_event: None,
                eye,
                cam_dir,
            })
            .insert_resource(ground)
            .insert_resource(crate::balance_meter::ZonesDir(self.zones_dir.clone()))
            .add_systems(Startup, (setup, crate::balance_meter::setup))
            .add_systems(FixedUpdate, step)
            .add_systems(Update, (present, hud, crate::balance_meter::draw, crate::rumble::send, crate::skater_model::animate));
    }
}

#[derive(Resource)]
pub(crate) struct Skater {
    pub(crate) scripts: Scripts,
    source: String,
    /// The translated skater: physics plus the player's own scripts.
    pub(crate) object: p8_skater::Skater,
    previous: Frame,
    current: Frame,
    /// The translated retail controller path (records and hold times).
    controller: Controller,
    last_event: Option<Event>,
    /// Placeholder chase camera position.
    eye: Vec3,
    /// Placeholder camera: the flat direction of travel it stays behind.
    cam_dir: Vec3,
}

/// What the skater rides on: the player's level, or a flat floor.
#[derive(Resource)]
struct Ground {
    level: Option<Level>,
    /// Boxes, cylinders and capsules, for drawing.
    convex: Vec<Solid>,
    /// The zone's default restart node (position, angles).
    spawn: (Vec3, Vec3),
    note: String,
}

impl Ground {
    fn world(&self) -> &dyn World {
        match &self.level {
            Some(level) => level,
            None => &FLAT,
        }
    }
}

static FLAT: FlatFloor = FlatFloor { height: 0.0 };

/// The level's collision triangles as one mesh, flat-shaded, walls a little
/// darker than floors (retail `Wall_Non_Skatable_Angle` is 25 degrees).
fn level_mesh(level: &Level) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    for (v, _material) in level.triangles() {
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let shade = if n.y.abs() >= 25f32.to_radians().sin() { [0.62, 0.62, 0.6, 1.0] } else { [0.5, 0.45, 0.4, 1.0] };
        for p in v {
            positions.push(p.to_array());
            normals.push(n.to_array());
            colors.push(shade);
        }
    }
    let count = positions.len() as u32;
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32((0..count).collect()))
}

/// A mesh along Y (cylinder or capsule) placed between two points.
fn between(a: Vec3, b: Vec3) -> Transform {
    let d = b - a;
    Transform::from_translation((a + b) * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y)))
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

/// The placeholder board, removed when the real board loads.
#[derive(Component)]
struct BoardBox;
#[derive(Component)]
struct StatusText;

/// How the skater's model loaded (shown in the HUD).
#[derive(Resource)]
pub(crate) struct ModelNote(String);

#[allow(clippy::too_many_arguments)]
fn setup(
    ground: Res<Ground>,
    mut skater: ResMut<Skater>,
    zones: Res<crate::balance_meter::ZonesDir>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
) {
    if let Some(level) = &ground.level {
        commands.spawn((
            Mesh3d(meshes.add(level_mesh(level))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.95,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ));
        let solid = materials.add(Color::srgb(0.7, 0.5, 0.35));
        for s in &ground.convex {
            let v = Vec3::from;
            let (mesh, transform) = match s {
                Solid::Box { transform, half, .. } => {
                    let [x, y, z] = transform.cols.map(Vec3::from);
                    let rotation = Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize();
                    let h = v(*half) * 2.0;
                    (meshes.add(Cuboid::new(h.x, h.y, h.z)), Transform::from_translation(v(transform.t)).with_rotation(rotation))
                }
                Solid::Cylinder { a, b, radius, .. } => {
                    (meshes.add(Cylinder::new(*radius, (v(*b) - v(*a)).length())), between(v(*a), v(*b)))
                }
                Solid::Capsule { a, b, radius, .. } => {
                    (meshes.add(Capsule3d::new(*radius, (v(*b) - v(*a)).length())), between(v(*a), v(*b)))
                }
                Solid::Triangle { .. } => continue,
            };
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(solid.clone()), transform));
        }
    } else {
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
        for i in -20..=20 {
            for j in -20..=20 {
                if (i + j) % 4 == 0 {
                    commands.spawn((
                        Mesh3d(post.clone()),
                        MeshMaterial3d(post_color.clone()),
                        Transform::from_xyz(i as f32 * 10.0, 0.5, j as f32 * 10.0),
                    ));
                }
            }
        }
    }

    // The skater: the player's own pro skater model if it loads, else a
    // placeholder rider. The board is still a placeholder.
    let root = commands
        .spawn((SkaterRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                BoardBox,
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
        })
        .id();
    let model = match zones.0.as_deref().and_then(|z| z.parent()) {
        Some(compressed) => crate::skater_model::spawn(
            &mut commands,
            root,
            &mut skater,
            compressed,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut bindposes,
        ),
        None => Err("no DATA/COMPRESSED folder".into()),
    };
    let note = match model {
        Ok(n) => {
            let real_board = n.contains("board_default");
            commands.queue(move |world: &mut bevy::ecs::world::World| {
                if real_board {
                    let mut q = world.query_filtered::<Entity, With<BoardBox>>();
                    let boxes: Vec<Entity> = q.iter(world).collect();
                    for b in boxes {
                        world.despawn(b);
                    }
                }
                let mut q = world.query_filtered::<Entity, With<Rider>>();
                let riders: Vec<Entity> = q.iter(world).collect();
                for r in riders {
                    world.despawn(r);
                }
            });
            format!("model {n}")
        }
        Err(e) => format!("model not loaded: {e}"),
    };
    commands.insert_resource(ModelNote(note));

    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(20.0, 40.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 2500.0, ..default() },
        Transform::from_xyz(-10.0, 20.0, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, -5.0).looking_at(Vec3::Y, Vec3::Y)));

    commands.spawn((
        Text::new(
            "Translated Project 8 physics (level collision shown as plain shapes; no tricks yet)\n\
             Controller: hold A to crouch (and push), release A to ollie, left stick steer / spin in the air,\n\
             pull back to brake, LB/RB spin, Y wall push, D-pad works like the stick, Back restart\n\
             Keyboard: hold Space to crouch, release to ollie, W/A/S/D stick, Q/E spin, F wall push, R restart",
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

/// The player's controller (or keyboard) as the Xbox 360 pad the retail
/// code reads. Everything after this goes through the translated retail
/// controller path (`p8_skater::controller`).
fn read_pad(keys: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>) -> XboxPad {
    use p8_skater::pad::xinput::*;
    let key = |k| if keys.pressed(k) { 1.0 } else { 0.0 };
    // Keyboard: W/A/S/D as the left stick at full tilt.
    let mut stick = Vec2::new(key(KeyCode::KeyD) - key(KeyCode::KeyA), key(KeyCode::KeyW) - key(KeyCode::KeyS));
    let mut buttons = 0u16;
    let mut lt = 0.0f32;
    let mut rt = 0.0f32;
    let mut press = |on: bool, bit: u16| {
        if on {
            buttons |= bit;
        }
    };
    press(keys.pressed(KeyCode::Space), A);
    press(keys.pressed(KeyCode::KeyQ), LEFT_SHOULDER);
    press(keys.pressed(KeyCode::KeyE), RIGHT_SHOULDER);
    press(keys.pressed(KeyCode::KeyR), BACK);
    press(keys.pressed(KeyCode::KeyF), Y);
    if keys.pressed(KeyCode::ShiftLeft) {
        lt = 1.0;
    }
    for pad in gamepads {
        stick += pad.left_stick();
        let map = [
            (GamepadButton::South, A),
            (GamepadButton::East, B),
            (GamepadButton::West, X),
            (GamepadButton::North, Y),
            (GamepadButton::DPadUp, DPAD_UP),
            (GamepadButton::DPadDown, DPAD_DOWN),
            (GamepadButton::DPadLeft, DPAD_LEFT),
            (GamepadButton::DPadRight, DPAD_RIGHT),
            (GamepadButton::LeftTrigger, LEFT_SHOULDER),
            (GamepadButton::RightTrigger, RIGHT_SHOULDER),
            (GamepadButton::Select, BACK),
            (GamepadButton::Start, START),
        ];
        for (button, bit) in map {
            press(pad.pressed(button), bit);
        }
        lt = lt.max(pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0));
        rt = rt.max(pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0));
    }
    let stick = stick.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
    let axis = |v: f32| (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
    XboxPad {
        buttons,
        left_trigger: (lt * 255.0) as u8,
        right_trigger: (rt * 255.0) as u8,
        thumb_lx: axis(stick.x),
        thumb_ly: axis(stick.y),
        thumb_rx: 0,
        thumb_ry: 0,
    }
}

fn step(
    mut skater: ResMut<Skater>,
    ground: Res<Ground>,
    time: Res<Time<Fixed>>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    let skater = &mut *skater;
    let dt = time.delta_secs();
    let pad = read_pad(&keys, &gamepads);
    let was_held = skater.controller.select.held;
    let input = skater.controller.update(&pad, skater.object.physics.time_ms);
    if skater.controller.select.held && !was_held {
        // The new skater keeps the clips and skeleton of the old one.
        let lib = skater.object.anim.lib.take();
        let rig = skater.object.anim.rig.clone();
        let board_rig = skater.object.anim.board_rig.take();
        skater.object = p8_skater::Skater::new(&skater.scripts, ground.spawn.0, ground.spawn.1);
        if let Some(lib) = lib {
            let scripts = &skater.scripts;
            let g = |c: u32| scripts.globals.get(&c).cloned();
            skater.object.anim.attach(lib, rig, &g);
            skater.object.anim.board_rig = board_rig;
        }
        skater.current = Frame::of(&skater.object.physics);
        skater.cam_dir = skater.object.physics.body.at();
    }
    skater.previous = skater.current;
    let skater = &mut *skater;
    skater.object.physics.dt = dt;
    let events = skater.object.step(&skater.scripts, &input, ground.world());
    let inputs = skater.object.physics.anim_inputs_in(&skater.scripts, Some(ground.world()));
    skater.object.anim.update(dt, inputs);
    if let Some(e) = events.last() {
        skater.last_event = Some(*e);
    }
    skater.current = Frame::of(&skater.object.physics);
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
    let crouched = skater.object.physics.crouched;
    if let Ok(mut t) = rider.single_mut() {
        t.translation.y = if crouched { 0.62 } else { 0.8 };
        t.scale = Vec3::new(1.0, if crouched { 0.75 } else { 1.0 }, 1.0);
    }
    // Placeholder chase camera (not the retail camera): it stays behind the
    // direction of travel, so spins and riding backwards don't swing it
    // around. Below 1 m/s it keeps its last direction.
    let v = skater.object.physics.body.velocity;
    let travel = Vec3::new(v.x, 0.0, v.z);
    if travel.length() > 1.0 {
        let ease = 1.0 - (-4.0 * real.delta_secs()).exp();
        skater.cam_dir = skater.cam_dir.lerp(travel.normalize(), ease).normalize_or(skater.cam_dir);
    }
    let flat = skater.cam_dir;
    let goal = position - flat * 4.5 + Vec3::Y * 2.0;
    let ease = 1.0 - (-6.0 * real.delta_secs()).exp();
    skater.eye = skater.eye.lerp(goal, ease);
    if let Ok(mut t) = camera.single_mut() {
        *t = Transform::from_translation(skater.eye).looking_at(position + Vec3::Y * 1.0, Vec3::Y);
    }
}

/// The newest branch and the node types used that are not translated yet.
fn anim_note(a: &p8_skater::anim_tree::AnimTree) -> String {
    use p8_skater::anim_tree::name_of;
    let name = |k: u32| name_of(k).map_or(format!("{k:08x}"), str::to_string);
    let branch = a.branches.last().map_or("none yet".to_string(), |&b| name(b));
    if a.untranslated.is_empty() {
        format!("branch {branch}")
    } else {
        let u: Vec<String> = a.untranslated.iter().map(|&k| name(k)).collect();
        format!("branch {branch}; stand-ins for untranslated nodes: {}", u.join(", "))
    }
}

fn hud(
    skater: Res<Skater>,
    ground: Res<Ground>,
    meter: Option<Res<crate::balance_meter::MeterTextures>>,
    model: Option<Res<ModelNote>>,
    mut text: Query<&mut Text, With<StatusText>>,
) {
    let p = &skater.object.physics;
    let turn = match p.last_turn {
        Some(Turn::Left) => "left",
        Some(Turn::Right) => "right",
        None => "-",
    };
    let line = format!(
        "speed {:5.2} m/s   height {:4.2} m   {:?}   spin {:4.0}°   {}{}   turn {turn}   last event {:?}\n[original scripts: {}{}]  [level: {}]  [{}]  [{}]\n[animation: {}]",
        p.body.velocity.length(),
        p.body.position.y,
        p.state,
        p.spin_degrees,
        if p.crouched { "CROUCHED" } else { "standing" },
        if p.braking { "  BRAKING" } else { "" },
        skater.last_event,
        skater.source,
        if p.scripted {
            format!(", running; {} commands not translated yet", skater.object.untranslated.len())
        } else {
            String::new()
        },
        ground.note,
        meter.as_ref().map_or("balance meter textures not loaded yet", |m| m.note.as_str()),
        model.as_ref().map_or("model not loaded yet", |m| m.0.as_str()),
        anim_note(&skater.object.anim),
    );
    if let Ok(mut t) = text.single_mut()
        && **t != line
    {
        **t = line;
    }
}
