//! Provisional presentation of the translated lip balance, not retail UI
//! artwork. All dimensions/colors below are display choices, not physics.
use super::Skater;
use bevy::prelude::*;
use p8_skater::core_physics::State;

const TRACK_HEIGHT: f32 = 200.0;
const NEEDLE_HEIGHT: f32 = 6.0;

#[derive(Component)]
pub(super) struct LipGauge;
#[derive(Component)]
pub(super) struct LipNeedle;

type NeedleFilter = (With<LipNeedle>, Without<LipGauge>);

pub(super) fn spawn(commands: &mut Commands) {
    commands
        .spawn((
            LipGauge,
            Node {
                position_type: PositionType::Absolute,
                right: percent(18.0),
                top: percent(28.0),
                width: px(150.0),
                padding: UiRect::all(px(12.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.035, 0.05, 0.88)),
        ))
        .with_children(|panel| {
            panel.spawn((Text::new("LIP BALANCE"), TextFont { font_size: 16.0, ..default() }, TextColor(Color::WHITE)));
            panel
                .spawn((
                    Node { width: px(18.0), height: px(TRACK_HEIGHT), ..default() },
                    BackgroundColor(Color::srgb(0.1, 0.55, 0.5)),
                ))
                .with_children(|track| {
                    for top in [0.0, 85.0] {
                        track.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: percent(top),
                                width: percent(100.0),
                                height: percent(15.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.85, 0.22, 0.2)),
                        ));
                    }
                    track.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(-4.0),
                            top: px(TRACK_HEIGHT * 0.5 - 1.0),
                            width: px(26.0),
                            height: px(2.0),
                            ..default()
                        },
                        BackgroundColor(Color::WHITE),
                    ));
                    track.spawn((
                        LipNeedle,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(-13.0),
                            top: px(needle_top(0.0)),
                            width: px(44.0),
                            height: px(NEEDLE_HEIGHT),
                            border: UiRect::all(px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::WHITE),
                        BorderColor::all(Color::BLACK),
                    ));
                });
            panel.spawn((
                Text::new("Keep it centered"),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.8, 0.85, 0.9)),
            ));
        });
}

fn needle_top(fraction: f32) -> f32 {
    // Positive lean fires OffMeterTop; keep the marker inside the track.
    (1.0 - fraction.clamp(-1.0, 1.0)) * 0.5 * (TRACK_HEIGHT - NEEDLE_HEIGHT)
}

pub(super) fn update(
    skater: Res<Skater>,
    mut panels: Query<&mut Node, With<LipGauge>>,
    mut needles: Query<(&mut Node, &mut BackgroundColor), NeedleFilter>,
) {
    let p = &skater.object.physics;
    let fraction =
        (p.state == State::Lip).then(|| p.balance.lip_fraction(&skater.scripts, &p.stats, p.stat_context)).flatten();
    for mut panel in &mut panels {
        panel.display = if fraction.is_some() { Display::Flex } else { Display::None };
    }
    if let Some(fraction) = fraction {
        for (mut node, mut color) in &mut needles {
            node.top = px(needle_top(fraction));
            // Warning color is presentation only; it does not change the
            // meter's failure threshold or control response.
            color.0 = if fraction.abs() > 0.75 { Color::srgb(1.0, 0.78, 0.15) } else { Color::WHITE };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Frame;
    use super::*;
    use p8_formats::{qb::Value, qb_key};
    use p8_skater::{Controller, Scripts};

    #[test]
    fn gauge_tracks_balance_and_hides_after_lip_exit_without_changing_physics() {
        let scripts = Scripts::new(
            [(
                qb_key("LipParams"),
                Value::Struct(vec![(qb_key("Lean_Bail_Angle"), Value::Struct(vec![(0, Value::Pair(20.0, 20.0))]))]),
            )]
            .into_iter()
            .collect(),
        );
        let mut object = p8_skater::Skater::new(&scripts, Vec3::ZERO, Vec3::ZERO);
        object.physics.state = State::Lip;
        object.physics.balance.kind = qb_key("Lip");
        object.physics.balance.lip.button_a = qb_key("Up");
        object.physics.balance.lip.button_b = qb_key("Down");
        object.physics.balance.lip.lean = 10.0;
        let frame = Frame::of(&object.physics);
        let balance = object.physics.balance.clone();
        let rng = object.physics.rng;
        let mut app = App::new();
        app.insert_resource(Skater {
            scripts,
            source: "synthetic".into(),
            object,
            previous: frame,
            current: frame,
            controller: Controller::default(),
            last_event: None,
            eye: Vec3::ZERO,
            cam_dir: Vec3::Z,
        });
        app.add_systems(Startup, |mut commands: Commands| spawn(&mut commands));
        app.add_systems(Update, update);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query_filtered::<&Node, With<LipGauge>>().single(world).unwrap().display, Display::Flex);
        assert_eq!(world.query_filtered::<&Node, With<LipNeedle>>().single(world).unwrap().top, px(needle_top(0.5)));
        assert_eq!(world.resource::<Skater>().object.physics.balance, balance);
        assert_eq!(world.resource::<Skater>().object.physics.rng, rng);

        world.resource_mut::<Skater>().object.physics.balance.lip.lean = -20.0;
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.query_filtered::<&Node, With<LipNeedle>>().single(world).unwrap().top,
            px(TRACK_HEIGHT - NEEDLE_HEIGHT)
        );
        world.resource_mut::<Skater>().object.physics.state = State::Air;
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query_filtered::<&Node, With<LipGauge>>().single(world).unwrap().display, Display::None);
    }
}
