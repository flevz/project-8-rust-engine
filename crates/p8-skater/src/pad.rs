//! Controller mapping.
//!
//! CONFIRMED by emulating the retail XInput reader `823A6420`, which converts
//! `XINPUT_GAMEPAD` into a PS2-style pad packet. The game's scripts and trick
//! tables therefore use PS2 names.

/// Xbox 360 `wButtons` bits.
pub mod xinput {
    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LEFT_THUMB: u16 = 0x0040;
    pub const RIGHT_THUMB: u16 = 0x0080;
    pub const LEFT_SHOULDER: u16 = 0x0100;
    pub const RIGHT_SHOULDER: u16 = 0x0200;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

/// The game's (PS2-named) buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Start,
    Select,
    L3,
    R3,
    L1,
    R1,
    L2,
    R2,
    /// "x" in the scripts.
    Cross,
    Circle,
    Square,
    Triangle,
}

/// Triggers count as pressed above this raw value (0..255). CONFIRMED:
/// `subfic 30` comparisons in `823A6420`.
pub const TRIGGER_THRESHOLD: u8 = 30;

/// Which game button an Xbox input produces. CONFIRMED (`823A6420`).
pub fn from_xinput(buttons: u16, left_trigger: u8, right_trigger: u8) -> Vec<Button> {
    use xinput::*;
    let map = [
        (DPAD_UP, Button::Up),
        (DPAD_DOWN, Button::Down),
        (DPAD_LEFT, Button::Left),
        (DPAD_RIGHT, Button::Right),
        (START, Button::Start),
        (BACK, Button::Select),
        (LEFT_THUMB, Button::L3),
        (RIGHT_THUMB, Button::R3),
        (LEFT_SHOULDER, Button::L1),
        (RIGHT_SHOULDER, Button::R1),
        (A, Button::Cross),
        (B, Button::Circle),
        (X, Button::Square),
        (Y, Button::Triangle),
    ];
    let mut out: Vec<Button> = map.iter().filter(|(bit, _)| buttons & bit != 0).map(|(_, b)| *b).collect();
    if left_trigger > TRIGGER_THRESHOLD {
        out.push(Button::L2);
    }
    if right_trigger > TRIGGER_THRESHOLD {
        out.push(Button::R2);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_buttons_follow_the_retail_mapping() {
        assert_eq!(from_xinput(xinput::A, 0, 0), vec![Button::Cross]);
        assert_eq!(from_xinput(xinput::X, 0, 0), vec![Button::Square]);
        assert_eq!(from_xinput(0, 31, 30), vec![Button::L2], "right trigger at 30 is not pressed");
    }
}
