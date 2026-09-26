//! Centralised Project 8 controller tuning.
//!
//! Every value carries a provenance label:
//! - CONFIRMED ORIGINAL VALUE: read from the original game (none yet).
//! - MEASURED/ESTIMATED: fitted from recordings of the original game (none yet).
//! - TEMPORARY TUNING VALUE: chosen by hand for an arcade THPS feel.
//!
//! See `docs/research.md` for how to replace
//! temporary values with measured ones. A `tuning.json` beside the game may
//! override any field; missing fields keep these defaults.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tuning {
    // ---- Ground movement -------------------------------------------------
    /// m/s² while pushing. TEMPORARY TUNING VALUE.
    pub push_acceleration: f32,
    /// Pushing stops adding speed above this, in m/s. TEMPORARY TUNING VALUE.
    pub max_push_speed: f32,
    /// Absolute speed limit, including downhill, in m/s. TEMPORARY TUNING VALUE.
    pub max_speed: f32,
    /// Constant rolling resistance, in m/s². TEMPORARY TUNING VALUE.
    pub rolling_friction: f32,
    /// Deceleration while braking (stick back), in m/s². TEMPORARY TUNING VALUE.
    pub brake_deceleration: f32,
    /// Share of gravity applied along slopes. 1 = physically exact. TEMPORARY TUNING VALUE.
    pub slope_gravity_scale: f32,
    /// Turn rate at standstill, in rad/s. TEMPORARY TUNING VALUE.
    pub turn_rate_slow: f32,
    /// Turn rate at `max_push_speed`, in rad/s. TEMPORARY TUNING VALUE.
    pub turn_rate_fast: f32,
    /// Speed kept per wall hit, from 0 to 1. TEMPORARY TUNING VALUE.
    pub wall_speed_retention: f32,

    // ---- Air ---------------------------------------------------------------
    /// m/s². Arcade THPS gravity is heavier than Earth's. TEMPORARY TUNING VALUE.
    pub gravity: f32,
    /// Ollie launch speed, in m/s. TEMPORARY TUNING VALUE.
    pub ollie_speed: f32,
    /// Extra launch speed after a full crouch, in m/s. TEMPORARY TUNING VALUE.
    pub ollie_crouch_bonus: f32,
    /// Seconds of crouch needed for the full bonus. TEMPORARY TUNING VALUE.
    pub ollie_crouch_time: f32,
    /// Air spin rate, in rad/s. TEMPORARY TUNING VALUE.
    pub air_spin_rate: f32,
    /// Horizontal air steering, in m/s². TEMPORARY TUNING VALUE.
    pub air_control: f32,
    /// Largest angle, in radians, between travel direction and board
    /// (forward or fakie) that still lands cleanly. TEMPORARY TUNING VALUE.
    pub landing_tolerance: f32,
    /// Minimum air time, in seconds, before landing is evaluated. TEMPORARY TUNING VALUE.
    pub min_air_time: f32,
    /// Flip trick duration, in seconds. TEMPORARY TUNING VALUE.
    pub flip_duration: f32,

    // ---- Manual ------------------------------------------------------------
    /// Balance drift acceleration, in rad/s². TEMPORARY TUNING VALUE.
    pub manual_instability: f32,
    /// Balance correction per unit of stick input, in rad/s². TEMPORARY TUNING VALUE.
    pub manual_correction: f32,
    /// Balance angle, in radians, at which a manual is lost. TEMPORARY TUNING VALUE.
    pub manual_limit: f32,

    // ---- Grind -------------------------------------------------------------
    /// Largest distance, in metres, from a rail at which a grind can start. TEMPORARY TUNING VALUE.
    pub grind_snap_distance: f32,
    /// Grind friction, in m/s². TEMPORARY TUNING VALUE.
    pub grind_friction: f32,
    /// Minimum grind speed, in m/s. TEMPORARY TUNING VALUE.
    pub grind_min_speed: f32,
    /// Balance drift acceleration on rails, in rad/s². TEMPORARY TUNING VALUE.
    pub grind_instability: f32,
    /// Balance correction per unit of stick input, in rad/s². TEMPORARY TUNING VALUE.
    pub grind_correction: f32,
    /// Balance angle, in radians, at which a grind bails. TEMPORARY TUNING VALUE.
    pub grind_limit: f32,

    // ---- Bail --------------------------------------------------------------
    /// Seconds before control returns after a bail. TEMPORARY TUNING VALUE.
    pub bail_duration: f32,

    // ---- Body and collision ---------------------------------------------
    /// Height of the skater's centre above the board contact, in metres. TEMPORARY TUNING VALUE.
    pub body_height: f32,
    /// Radius of horizontal wall probes, in metres. TEMPORARY TUNING VALUE.
    pub body_radius: f32,
    /// Largest step onto which the board is snapped, in metres. TEMPORARY TUNING VALUE.
    pub ground_snap: f32,
    /// Minimum walkable normal Y (cos of max slope). TEMPORARY TUNING VALUE.
    pub min_ground_normal_y: f32,

    // ---- Camera ------------------------------------------------------------
    /// Distance behind the skater, in metres. TEMPORARY TUNING VALUE.
    pub camera_distance: f32,
    /// Height above the skater, in metres. TEMPORARY TUNING VALUE.
    pub camera_height: f32,
    /// Look-at height above the board, in metres. TEMPORARY TUNING VALUE.
    pub camera_target_height: f32,
    /// Follow stiffness, per second. TEMPORARY TUNING VALUE.
    pub camera_stiffness: f32,
    /// Vertical field of view, in degrees. TEMPORARY TUNING VALUE.
    pub camera_fov_degrees: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            push_acceleration: 6.0,
            max_push_speed: 9.0,
            max_speed: 16.0,
            rolling_friction: 0.35,
            brake_deceleration: 9.0,
            slope_gravity_scale: 0.8,
            turn_rate_slow: 3.0,
            turn_rate_fast: 1.7,
            wall_speed_retention: 0.35,
            gravity: 18.0,
            ollie_speed: 5.6,
            ollie_crouch_bonus: 1.4,
            ollie_crouch_time: 0.35,
            air_spin_rate: 7.0,
            air_control: 2.0,
            landing_tolerance: 0.7,
            min_air_time: 0.12,
            flip_duration: 0.45,
            manual_instability: 2.2,
            manual_correction: 4.5,
            manual_limit: 0.6,
            grind_snap_distance: 0.9,
            grind_friction: 0.6,
            grind_min_speed: 2.0,
            grind_instability: 2.0,
            grind_correction: 4.0,
            grind_limit: 0.7,
            bail_duration: 1.4,
            body_height: 0.9,
            body_radius: 0.3,
            ground_snap: 0.45,
            min_ground_normal_y: 0.55,
            camera_distance: 4.2,
            camera_height: 1.9,
            camera_target_height: 1.1,
            camera_stiffness: 8.0,
            camera_fov_degrees: 65.0,
        }
    }
}

impl Tuning {
    /// Parse a package tuning file. Missing fields keep defaults. Unknown
    /// fields are rejected so that typos fail visibly.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let tuning: Self =
            serde_json::from_str(text).map_err(|e| format!("controller tuning: {e}"))?;
        tuning.validate()?;
        Ok(tuning)
    }

    pub fn validate(&self) -> Result<(), String> {
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        for (key, v) in value.as_object().into_iter().flatten() {
            let n = v.as_f64().unwrap_or(f64::NAN);
            if !n.is_finite() || !(0.0..=1000.0).contains(&n) {
                return Err(format!(
                    "controller tuning: {key} must be finite and within 0..1000"
                ));
            }
        }
        if self.max_push_speed > self.max_speed {
            return Err("controller tuning: max_push_speed cannot exceed max_speed".into());
        }
        if !(0.0..=1.0).contains(&self.wall_speed_retention)
            || !(0.0..=1.0).contains(&self.min_ground_normal_y)
        {
            return Err(
                "controller tuning: wall_speed_retention and min_ground_normal_y are 0..1".into(),
            );
        }
        if self.gravity <= 0.0
            || self.body_height <= 0.0
            || self.ollie_crouch_time <= 0.0
            || self.flip_duration <= 0.0
        {
            return Err("controller tuning: gravity, body_height, ollie_crouch_time and flip_duration must be positive".into());
        }
        Ok(())
    }
}
