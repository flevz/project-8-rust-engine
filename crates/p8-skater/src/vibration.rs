//! Controller rumble: the skater's "vibration" component (constructor
//! `8228D180`, 56 bytes) and the pad driver's motor levels.
//!
//! - Script command `Vibrate` (`8228D528`): `Actuator` (0 = left, heavy
//!   motor; 1 = right, light motor), `Percent`, optional `duration`
//!   (seconds; none = until changed), or the flag `OFF` (both motors off).
//! - Per-frame update (`8228D260`, vtable `82012EE4` slot 1): a motor whose
//!   duration has passed is set to 0; an inactive motor is kept at 0.
//! - Pad (`823A67E0`): level = motor maximum (+117/+118, 255 on a standard
//!   pad, `823A6280`) * percent / 100, sent as (level << 8) to the motor.
//!
//! Vibration starts on: the script `default_system_startup` runs
//! `vibrationon` (the player's profile option, `options_retrieve_from_user_profile`,
//! is not read). `Vibrate` does nothing while the game is paused (byte
//! `82778B54`, set by `82189530`) or the object is paused (`+308` bit 0);
//! pausing is not translated.

/// One motor's timer (`+28 + 12 * actuator`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    /// `+28`: a `Vibrate` with a percent is running on this motor.
    pub active: bool,
    /// `+32`: when it started (ms).
    pub start_ms: i64,
    /// `+36`: how long (ms); `None` = until changed (retail -1).
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vibration {
    /// `+24`: vibration on (`VibrationOn` / `VibrationOff`, `8228D440`).
    pub on: bool,
    pub slots: [Slot; 2],
    /// Pad `+85`, `+86`: each motor's level, 0..255.
    pub levels: [u8; 2],
}

impl Default for Vibration {
    fn default() -> Self {
        Self { on: true, slots: [Slot::default(); 2], levels: [0; 2] }
    }
}

/// A motor's maximum on a standard pad (`823A6280`: 255).
const MOTOR_MAX: u32 = 255;

impl Vibration {
    /// `823A67E0`: set a motor to `percent` (an integer, as retail passes).
    fn set_motor(&mut self, actuator: usize, percent: i32) {
        // Only actuators 0 and 1 exist (pad +212 = 2); others are ignored.
        if let Some(l) = self.levels.get_mut(actuator) {
            // (max * percent) / 100 with integer division, stored in a byte.
            *l = ((MOTOR_MAX as i32 * percent) / 100) as u8;
        }
    }

    /// Script command `Vibrate` (`8228D528`). Returns TRUE.
    pub fn vibrate(&mut self, off: bool, actuator: i32, percent: f32, duration: Option<f32>, now_ms: i64) -> bool {
        if !self.on {
            return true;
        }
        if !off && percent != 0.0 {
            if let Ok(a) = usize::try_from(actuator) {
                self.set_motor(a, percent as i32);
            }
        } else {
            // 823A6AE0: every motor off; both timers stop.
            self.levels = [0; 2];
            for s in &mut self.slots {
                s.active = false;
            }
        }
        // Retail writes the slot of `Actuator` (0 if not given) even for
        // OFF; only actuators 0 and 1 exist (pad +212 = 2).
        if let Some(slot) = usize::try_from(actuator).ok().and_then(|a| self.slots.get_mut(a)) {
            slot.duration_ms = duration.map(|d| (d * 1000.0) as i64);
            slot.active = true;
            slot.start_ms = now_ms;
        }
        true
    }

    /// The component's per-frame update (`8228D260`).
    pub fn update(&mut self, now_ms: i64) {
        if !self.on {
            return;
        }
        for a in [1usize, 0] {
            let slot = self.slots[a];
            if !slot.active {
                self.set_motor(a, 0);
            } else if let Some(d) = slot.duration_ms
                && now_ms - slot.start_ms >= d
            {
                self.set_motor(a, 0);
                self.slots[a].active = false;
            }
        }
    }

    /// Motor speeds as sent to the pad (`823A6160`): left, right, 0..65535.
    pub fn motor_speeds(&self) -> [u16; 2] {
        [(self.levels[0] as u16) << 8, (self.levels[1] as u16) << 8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_vibration_stops_after_its_duration() {
        let mut v = Vibration::default();
        v.vibrate(false, 1, 50.0, Some(0.05), 1000);
        v.update(1000);
        assert_eq!(v.levels, [0, 127]);
        assert_eq!(v.motor_speeds(), [0, 127 << 8]);
        v.update(1049);
        assert_eq!(v.levels, [0, 127]);
        v.update(1050);
        assert_eq!(v.levels, [0, 0]);
        assert!(!v.slots[1].active);
    }

    #[test]
    fn untimed_vibration_lasts_until_off() {
        let mut v = Vibration::default();
        v.vibrate(false, 0, 80.0, None, 0);
        v.update(100_000);
        assert_eq!(v.levels, [204, 0]);
        v.vibrate(true, 0, 0.0, None, 100_000);
        v.update(100_016);
        assert_eq!(v.levels, [0, 0]);
    }

    #[test]
    fn switched_off_vibration_ignores_vibrate() {
        let mut v = Vibration { on: false, ..Default::default() };
        v.vibrate(false, 1, 100.0, None, 0);
        assert_eq!(v.levels, [0, 0]);
    }
}
