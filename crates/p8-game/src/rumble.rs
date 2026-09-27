//! Sends the translated rumble (`p8_skater::vibration`) to the player's
//! controllers. Retail writes the two motor speeds to the pad whenever they
//! change (`823A6160`, XInput left = heavy, right = light motor); here each
//! change replaces the controller's rumble with the new levels, held until
//! the next change (the long hold stands in for "until changed").
use std::time::Duration;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;

use crate::translated::Skater;

/// How long a level is held before the next change replaces it.
const HOLD: Duration = Duration::from_secs(3600);

pub fn send(
    skater: Res<Skater>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut last: Local<[u16; 2]>,
    mut requests: MessageWriter<GamepadRumbleRequest>,
) {
    let speeds = skater.object.physics.vibration.motor_speeds();
    if speeds == *last {
        return;
    }
    *last = speeds;
    let intensity = GamepadRumbleIntensity {
        strong_motor: speeds[0] as f32 / 65535.0,
        weak_motor: speeds[1] as f32 / 65535.0,
    };
    for gamepad in &gamepads {
        requests.write(GamepadRumbleRequest::Stop { gamepad });
        if speeds != [0, 0] {
            requests.write(GamepadRumbleRequest::Add { duration: HOLD, intensity, gamepad });
        }
    }
}
