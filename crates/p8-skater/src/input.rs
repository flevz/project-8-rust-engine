//! The skater's Input component (physics `+2816`), as far as the translated
//! code reads it. Retail keeps per-button records 32 bytes apart; only the
//! records used by translated functions are named here, with their offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// `+448`: sets SkaterState "crouched" (`820D7C08`). Holding the ollie
    /// button: CONFIRMED by in-game observation (holding A accelerates) plus
    /// the code; the pad-bit link itself is not yet traced.
    pub crouch: bool,
    /// `+384`: lets the skater kick when autokick is off (`820D9830`) and
    /// suppresses the stopped-brake (`820D9208`). Button UNKNOWN.
    pub kick: bool,
    /// `+64`: digital brake (`820D74B0`, `820D93F0`). LIKELY D-pad down.
    pub brake_digital: bool,
    /// `+96` and `+128`: consulted by the brake decision (`820D9208`).
    /// Buttons UNKNOWN.
    pub brake_hold_96: bool,
    pub brake_hold_128: bool,
    /// `+876`: analog back/forward value in retail units (-128..127),
    /// positive = pulled back. Scaled by 1/128 (constant at 82000E00).
    pub stick_back_raw: f32,
}
