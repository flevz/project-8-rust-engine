//! The skater's Input component (physics `+2816`), as far as the translated
//! code reads it. Retail keeps per-button records 32 bytes apart starting at
//! `+32`; their names are CONFIRMED by the naming function `822D6270`
//! (Up, Down, Left, Right, L1, L2, L3, R1, R2, R3, Circle, Square, Triangle,
//! X, start, select, ...), and `822D6C98` fills them from the game's
//! PlayStation-style pad state (see `pad.rs` for the Xbox mapping).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// `+448` = record "X" (Xbox A): sets SkaterState "crouched" (`820D7C08`).
    pub crouch: bool,
    /// `+32` = record "Up": sets the kick flag `+2112` (`820D7C88`), which the
    /// animation tree (`820B8EA0`) turns into a push animation. The push
    /// speed itself comes from script `HandleKickBoostEvent`, fired by those
    /// animations; not translated until animations are.
    pub up: bool,
    /// `+384` = record "Square" (Xbox X): lets the skater kick when autokick
    /// is off (`820D9830`) and suppresses the stopped-brake (`820D9208`).
    pub kick: bool,
    /// `+64` = record "Down": digital brake (`820D74B0`, `820D93F0`).
    pub brake_digital: bool,
    /// Record "Down" (same record as `brake_digital`).
    pub down: bool,
    /// Records "L1", "R1", "L2" (Xbox LB, RB, LT).
    pub l1: bool,
    pub r1: bool,
    pub l2: bool,
    /// Milliseconds since "Up" / "Down" were last pressed (`822D6200`).
    pub up_held_ms: i32,
    pub down_held_ms: i32,
    /// `+96` = record "Left": digital turn left (`820ECEE8`).
    pub left: bool,
    /// `+128` = record "Right": digital turn right.
    pub right: bool,
    /// Milliseconds since `left` / `right` were last pressed (`822D6200`;
    /// retail does not reset this on release).
    pub left_held_ms: i32,
    pub right_held_ms: i32,
    /// `+872`: left stick X in retail units (-128..127), positive = right,
    /// zero inside the dead zone of both axes (`82229E58`). Scaled by 1/128
    /// (constant at 82000E00) where read.
    pub stick_x_raw: f32,
    /// `+876`: analog back/forward value in retail units (-128..127),
    /// positive = pulled back. Scaled by 1/128.
    pub stick_back_raw: f32,
    /// Record base `+856` / `+860`: left stick X / Y past the per-axis dead
    /// zone, about -1..1 (Y positive = pulled back). Read by air rotation.
    pub stick_x: f32,
    pub stick_y: f32,
}
