//! The retail controller path, from an Xbox 360 pad to the skater's Input
//! component records, translated step by step:
//!
//! 1. `823A6420`: `XINPUT_GAMEPAD` -> PS2-style packet (sticks as bytes,
//!    buttons as bits; see `pad.rs`).
//! 2. `8222A320` (packet type 7 path): copy the stick bytes, set each
//!    button's pressure byte from its bit (`8222A220`, table `826E0990`),
//!    apply the stick dead zones (`82229E58`), and, with the analog stick
//!    active (`8222A030`), let the left stick press Up/Down/Left/Right when
//!    no D-pad button is held.
//!    Retail has a second path that copies the raw packet instead; there an
//!    unpressed button reads 1, which the record update counts as pressed,
//!    so every button would always be held. The working game therefore takes
//!    this path (inference from the code, not a traced setting).
//! 3. `822D6C98`: update each named record (`822D6058`) and the analog
//!    values.
//!
//! "Analog stick active" (`SetAnalogStickActiveForMenus`, global `+612`)
//! starts on (`82229C90`, `820911C0`); scripts only turn it off for bikes
//! and the level viewer. The per-controller flag (`+68`) is set once to 1
//! (`8222A290`).
use crate::input::InputState;
use crate::pad::xinput;

/// An Xbox 360 pad as `XINPUT_GAMEPAD` reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XboxPad {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

/// One input record (`822D6058`): held flag `+0`, press time `+4`,
/// release time `+8`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    pub held: bool,
    pub pressed_ms: i64,
    pub released_ms: i64,
}

impl Record {
    /// `822D6058`, without the lock-out timer (`+28`, not used here).
    fn update(&mut self, value: u8, now_ms: i64) {
        if value > 64 {
            if !self.held {
                self.pressed_ms = now_ms;
            }
            self.held = true;
        } else if self.held {
            self.released_ms = now_ms;
            self.held = false;
        }
    }

    /// `822D6200`: milliseconds since the last press.
    pub fn held_ms(&self, now_ms: i64) -> i64 {
        now_ms - self.pressed_ms
    }
}

/// Pad-state bytes (`8222A320`'s struct at controller `+64`).
#[derive(Clone, Copy, Debug)]
struct PadState {
    /// 0..3: right X, right Y, left X, left Y (per-axis dead zone).
    /// 4..15: pressure R, L, U, D, Triangle, Circle, X, Square, L1, R1, L2, R2.
    /// 29..32: the sticks again (dead zone on both axes together).
    b: [u8; 33],
    /// `+44`: PS2 button bits (Up 0x1000, Right 0x2000, Down 0x4000, Left 0x8000).
    bits: u32,
}

/// `823A6420`: signed XInput axis -> packet byte. `invert` for Y axes.
fn axis_byte(v: i16, invert: bool) -> u8 {
    let x = if invert { 32767 - v as i32 } else { v as i32 + 32767 };
    // srawi 8 + addze: division by 256 rounding toward zero.
    (x / 256) as u8
}

/// `82229E58`: stick dead zones (50 around 128).
fn dead_zones(s: &mut PadState) {
    for i in 29..=32 {
        s.b[i] = s.b[i - 29];
    }
    let near = |v: u8| (128 - v as i32).abs() <= 50;
    for i in 0..4 {
        if near(s.b[i]) {
            s.b[i] = 128;
        }
    }
    if near(s.b[29]) && near(s.b[30]) {
        s.b[29] = 128;
        s.b[30] = 128;
    }
    if near(s.b[31]) && near(s.b[32]) {
        s.b[31] = 128;
        s.b[32] = 128;
    }
}

/// `8222A030`: the left stick presses the D-pad when none is held.
fn analog_as_dpad(s: &mut PadState) {
    if s.bits & 0xF000 != 0 {
        return;
    }
    let lx = s.b[2];
    if lx > 128 {
        s.b[4] = lx.wrapping_add(128);
        s.bits |= 0x2000;
    } else if lx < 128 {
        s.b[5] = 128 - lx;
        s.bits |= 0x8000;
    }
    let ly = s.b[3];
    if ly < 128 {
        s.b[6] = ly.wrapping_add(128);
        s.bits |= 0x1000;
    } else if ly > 128 {
        s.b[7] = ly - 128;
        s.bits |= 0x4000;
    }
}

/// `822D6C98`: a stick byte shaped past the dead zone, about -1..1.
fn shaped(b: u8) -> f32 {
    let mut v = b as i32 - 128;
    if v == 0 {
        return 0.0;
    }
    if v == -128 {
        v = -127;
    }
    v += if v > 0 { -50 } else { 50 };
    // 0.012987 = 1/77 (constant at 82014B8C).
    v as f32 * 0.012987013
}

/// The skater's Input component records and analog values.
#[derive(Clone, Debug, Default)]
pub struct Controller {
    pub up: Record,
    pub down: Record,
    pub left: Record,
    pub right: Record,
    pub l1: Record,
    pub r1: Record,
    pub l2: Record,
    pub r2: Record,
    pub cross: Record,
    pub square: Record,
    pub circle: Record,
    pub triangle: Record,
    pub select: Record,
    /// Retail setting "analog stick active" (see module docs). On.
    pub analog_stick_off: bool,
}

impl Controller {
    /// One frame of the controller path; returns what the skater reads.
    pub fn update(&mut self, pad: &XboxPad, now_ms: i64) -> InputState {
        use xinput::*;
        let mut s = PadState { b: [0; 33], bits: 0 };
        s.b[0] = axis_byte(pad.thumb_rx, false);
        s.b[1] = axis_byte(pad.thumb_ry, true);
        s.b[2] = axis_byte(pad.thumb_lx, false);
        s.b[3] = axis_byte(pad.thumb_ly, true);
        // PS2 bits (823A6420) and their pressure bytes (table 826E0990).
        let buttons = [
            (DPAD_UP, 0x1000, 6),
            (DPAD_RIGHT, 0x2000, 4),
            (DPAD_DOWN, 0x4000, 7),
            (DPAD_LEFT, 0x8000, 5),
            (Y, 0x10, 8),
            (B, 0x20, 9),
            (A, 0x40, 10),
            (X, 0x80, 11),
            (LEFT_SHOULDER, 0x4, 12),
            (RIGHT_SHOULDER, 0x8, 13),
        ];
        for (xbit, bit, byte) in buttons {
            if pad.buttons & xbit != 0 {
                s.bits |= bit;
                s.b[byte] = 255;
            }
        }
        if pad.left_trigger > crate::pad::TRIGGER_THRESHOLD {
            s.bits |= 0x1;
            s.b[14] = 255;
        }
        if pad.right_trigger > crate::pad::TRIGGER_THRESHOLD {
            s.bits |= 0x2;
            s.b[15] = 255;
        }
        let select = pad.buttons & BACK != 0;
        dead_zones(&mut s);
        if !self.analog_stick_off {
            analog_as_dpad(&mut s);
        }
        // 822D6C98: records take 255 for any non-zero byte.
        let v = |b: u8| if b != 0 { 255 } else { 0 };
        self.up.update(v(s.b[6]), now_ms);
        self.down.update(v(s.b[7]), now_ms);
        self.left.update(v(s.b[5]), now_ms);
        self.right.update(v(s.b[4]), now_ms);
        self.square.update(v(s.b[11]), now_ms);
        self.circle.update(v(s.b[9]), now_ms);
        self.cross.update(v(s.b[10]), now_ms);
        self.triangle.update(v(s.b[8]), now_ms);
        self.l1.update(v(s.b[12]), now_ms);
        self.r1.update(v(s.b[13]), now_ms);
        self.l2.update(v(s.b[14]), now_ms);
        self.r2.update(v(s.b[15]), now_ms);
        self.select.update(if select { 255 } else { 0 }, now_ms);
        InputState {
            crouch: self.cross.held,
            kick: self.square.held,
            up: self.up.held,
            down: self.down.held,
            brake_digital: self.down.held,
            left: self.left.held,
            right: self.right.held,
            l1: self.l1.held,
            r1: self.r1.held,
            l2: self.l2.held,
            up_held_ms: self.up.held_ms(now_ms) as i32,
            down_held_ms: self.down.held_ms(now_ms) as i32,
            left_held_ms: self.left.held_ms(now_ms) as i32,
            right_held_ms: self.right.held_ms(now_ms) as i32,
            stick_x_raw: s.b[31] as f32 - 128.0,
            stick_back_raw: s.b[32] as f32 - 128.0,
            stick_x: shaped(s.b[2]),
            stick_y: shaped(s.b[3]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stick(lx: i16, ly: i16) -> XboxPad {
        XboxPad { thumb_lx: lx, thumb_ly: ly, ..Default::default() }
    }

    #[test]
    fn left_stick_presses_the_dpad_past_the_dead_zone() {
        let mut c = Controller::default();
        // Pushed up: Y is inverted into the packet (small byte = up).
        let i = c.update(&stick(0, 32767), 0);
        assert!(i.up && !i.down && !i.left && !i.right);
        assert_eq!(i.stick_back_raw, -128.0);
        // Inside the 50/128 dead zone: nothing.
        let i = c.update(&stick(0, 12000), 16);
        assert!(!i.up && i.stick_back_raw == 0.0 && i.stick_y == 0.0);
        // The analog setting off (bikes): the stick no longer presses Up.
        c.analog_stick_off = true;
        assert!(!c.update(&stick(0, 32767), 33).up);
    }

    #[test]
    fn dpad_takes_priority_and_records_hold_times() {
        let mut c = Controller::default();
        let pad = XboxPad { buttons: xinput::DPAD_LEFT, thumb_ly: 32767, ..Default::default() };
        let i = c.update(&pad, 100);
        assert!(i.left && !i.up, "stick ignored while the D-pad is held");
        let i = c.update(&pad, 350);
        assert_eq!(i.left_held_ms, 250);
    }

    #[test]
    fn shaped_values_start_at_the_dead_zone_edge() {
        assert_eq!(shaped(128), 0.0);
        assert!((shaped(255) - 1.0).abs() < 1e-6);
        assert!((shaped(0) + 1.0).abs() < 1e-6);
    }
}
