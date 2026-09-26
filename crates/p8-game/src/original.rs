//! Map original Project 8 skater globals (read from the player's own
//! `qb.pak.xen`) onto `p8_sim::Tuning`.
//!
//! Every value copied here is a CONFIRMED ORIGINAL VALUE. How the simulation
//! uses it is labelled per field: CONFIRMED where the meaning is unambiguous,
//! LIKELY where the original code has not yet been translated. Fields not
//! listed stay TEMPORARY TUNING VALUES.
use p8_formats::qb::Value;
use p8_formats::qb_key;
use p8_sim::Tuning;
use std::collections::BTreeMap;

/// Default skater stat level. CONFIRMED: `skater_default_stats` = 5.0.
pub const DEFAULT_STAT: f32 = 5.0;

pub struct Mapped {
    pub tuning: Tuning,
    /// (tuning field, source, value, confidence)
    pub report: Vec<(&'static str, String, f32, &'static str)>,
}

/// A stat-scaled value: struct with an unnamed (min, max) pair.
/// LIKELY: linear interpolation over stat levels 0..10.
fn stat(v: &Value, level: f32) -> Option<f32> {
    match v.get(0)? {
        Value::Pair(lo, hi) => Some(lo + (hi - lo) * (level / 10.0).clamp(0.0, 1.0)),
        other => other.as_f32(),
    }
}

pub fn map(globals: &BTreeMap<u32, Value>) -> Result<Mapped, String> {
    let g = |name: &str| globals.get(&qb_key(name));
    let physics = g("skater_physics").ok_or("skater_physics not found in the scripts")?;
    let p = |name: &str| physics.get_named(name);
    let level = g("skater_default_stats")
        .and_then(Value::as_f32)
        .unwrap_or(DEFAULT_STAT);

    let mut t = Tuning::default();
    let mut report = Vec::new();
    let mut set = |field: &'static str,
                   source: &str,
                   value: Option<f32>,
                   confidence: &'static str,
                   slot: &mut f32| {
        if let Some(v) = value.filter(|v| v.is_finite()) {
            *slot = v;
            report.push((field, source.to_owned(), v, confidence));
        }
    };

    let air = p("physics_air_gravity").and_then(Value::as_f32);
    let ground = p("physics_ground_gravity").and_then(Value::as_f32);
    set(
        "gravity",
        "skater_physics.physics_air_gravity",
        air.map(f32::abs),
        "CONFIRMED",
        &mut t.gravity,
    );
    set(
        "slope_gravity_scale",
        "physics_ground_gravity / physics_air_gravity",
        air.zip(ground).map(|(a, g)| (g / a).abs()),
        "LIKELY",
        &mut t.slope_gravity_scale,
    );
    let ollie_min = g("physics_jump_speed_min_stat").and_then(|v| stat(v, level));
    let ollie_max = g("physics_jump_speed_stat").and_then(|v| stat(v, level));
    set(
        "ollie_speed",
        "physics_jump_speed_min_stat @ default stat",
        ollie_min,
        "LIKELY",
        &mut t.ollie_speed,
    );
    set(
        "ollie_crouch_bonus",
        "physics_jump_speed_stat - physics_jump_speed_min_stat",
        ollie_max.zip(ollie_min).map(|(a, b)| (a - b).max(0.0)),
        "LIKELY",
        &mut t.ollie_crouch_bonus,
    );
    set(
        "max_speed",
        "skater_physics.skater_max_speed_stat",
        p("skater_max_speed_stat").and_then(|v| stat(v, level)),
        "LIKELY",
        &mut t.max_speed,
    );
    set(
        "max_push_speed",
        "skater_physics.Skater_Max_Standing_Kick_Speed_Stat.limit",
        p("Skater_Max_Standing_Kick_Speed_Stat")
            .and_then(|v| v.get_named("limit"))
            .and_then(Value::as_f32),
        "LIKELY",
        &mut t.max_push_speed,
    );
    set(
        "push_acceleration",
        "skater_physics.physics_standing_acceleration_stat",
        p("physics_standing_acceleration_stat").and_then(|v| stat(v, level)),
        "LIKELY",
        &mut t.push_acceleration,
    );
    set(
        "brake_deceleration",
        "skater_physics.physics_brake_acceleration",
        p("physics_brake_acceleration").and_then(Value::as_f32),
        "LIKELY",
        &mut t.brake_deceleration,
    );
    set(
        "turn_rate_fast",
        "skater_physics.physics_ground_rotation",
        p("physics_ground_rotation").and_then(Value::as_f32),
        "LIKELY",
        &mut t.turn_rate_fast,
    );
    set(
        "turn_rate_slow",
        "skater_physics.physics_ground_sharp_rotation",
        p("physics_ground_sharp_rotation").and_then(Value::as_f32),
        "LIKELY",
        &mut t.turn_rate_slow,
    );
    set(
        "air_spin_rate",
        "skater_physics.physics_air_rotation_stat",
        p("physics_air_rotation_stat").and_then(|v| stat(v, level)),
        "LIKELY",
        &mut t.air_spin_rate,
    );
    set(
        "ground_snap",
        "skater_physics.physics_ground_snap_up",
        p("physics_ground_snap_up").and_then(Value::as_f32),
        "LIKELY",
        &mut t.ground_snap,
    );
    set(
        "body_radius",
        "skater_physics.skater_min_distance_to_wall",
        p("skater_min_distance_to_wall").and_then(Value::as_f32),
        "LIKELY",
        &mut t.body_radius,
    );
    if t.max_push_speed > t.max_speed {
        t.max_push_speed = t.max_speed;
    }
    t.validate()?;
    Ok(Mapped { tuning: t, report })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_stat_ranges_and_plain_values() {
        let named = |n: &str, v: Value| (qb_key(n), v);
        let stat_pair =
            |lo, hi| Value::Struct(vec![(0, Value::Pair(lo, hi)), (0, Value::Checksum(1))]);
        let physics = Value::Struct(vec![
            named("physics_air_gravity", Value::Float(-20.0)),
            named("physics_ground_gravity", Value::Float(-10.0)),
            named("skater_max_speed_stat", stat_pair(10.0, 20.0)),
        ]);
        let globals: BTreeMap<u32, Value> = [
            named("skater_physics", physics),
            named("skater_default_stats", Value::Float(5.0)),
            named("physics_jump_speed_min_stat", stat_pair(6.0, 8.0)),
            named("physics_jump_speed_stat", stat_pair(8.0, 8.0)),
        ]
        .into_iter()
        .collect();
        let m = map(&globals).unwrap();
        assert_eq!(m.tuning.gravity, 20.0);
        assert_eq!(m.tuning.slope_gravity_scale, 0.5);
        assert_eq!(m.tuning.max_speed, 15.0, "stat 5 of 10 is halfway");
        assert_eq!(m.tuning.ollie_speed, 7.0);
        assert_eq!(m.tuning.ollie_crouch_bonus, 1.0);
        assert!(m.report.iter().any(|(field, ..)| *field == "gravity"));
        assert!(map(&BTreeMap::new()).is_err());
    }
}
