//! The balance meter on screen (retail `CScore`, the skater's
//! "skaterscore" component, `+24`).
//!
//! The meter's update (`82190F58`, end) calls `SetBalanceMeter`
//! (`821795A8`) or, when the meter's buttons are Up/Down, `SetManualMeter`
//! (`821795B0`); both are `82178D78`, which drives the screen element
//! `the_balance_meter` (made by the `create_panel_stuff` script: a
//! container holding the sprites `balancearrow_glow` (child 0, local id
//! `balance_arrow`), `balancemeter` (1), `balancemeter_2` (2) and
//! `balancemeter_bg` (3)) and runs the scripts `show_balance_meter`,
//! `hide_balance_meter` and `update_balance_meter_colors`.
//!
//! The screen element system is not translated: [`MeterDisplay`] keeps
//! what `82178D78` and those scripts set on the elements, and
//! [`MeterDisplay::sprites`] lists what they add up to, for the game to
//! draw. Coordinates are the HUD's 640 x 480 screen.
use glam::Vec2;
use p8_formats::qb::Value;
use p8_formats::qb_key;

use crate::script::Scripts;

/// Where the pieces go, from the script global `balance_meter_info`
/// (read by `82175D08`, the score's set-up). Split screen reads
/// `bar_positions_mp_h` / `_mp_v` instead (not translated).
#[derive(Clone, Debug, PartialEq)]
pub struct MeterLayout {
    /// Score `+148` (12 bytes each, x at `+4`, y at `+8`): the arrow's path
    /// from the middle (0) to one end (1), `arrow_positions`, with the last
    /// point repeated once more (`82175DB4`).
    pub arrow: Vec<Vec2>,
    /// Score `+388`: 1 / the number of `arrow_positions`.
    pub step: f32,
    /// Score `+408` (`+412`, `+416`) and `+420` (`+424`, `+428`):
    /// `bar_positions`, the container's place for balance (0) and manual (1).
    pub bars: [Vec2; 2],
}

fn pair(v: &Value) -> Option<Vec2> {
    match v {
        Value::Pair(x, y) => Some(Vec2::new(*x, *y)),
        Value::Array(a) if a.len() == 2 => Some(Vec2::new(a[0].as_f32()?, a[1].as_f32()?)),
        _ => None,
    }
}

fn pairs(v: Option<&Value>) -> Vec<Vec2> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(pair).collect(),
        _ => Vec::new(),
    }
}

impl MeterLayout {
    /// Retail `82175D08` (single screen).
    pub fn from_scripts(s: &Scripts) -> Option<Self> {
        let info = s.global("balance_meter_info")?;
        let mut arrow = pairs(info.get(qb_key("arrow_positions")));
        let last = *arrow.last()?;
        let step = 1.0 / arrow.len() as f32;
        arrow.push(last);
        let bars = pairs(info.get(qb_key("bar_positions")));
        Some(Self { arrow, step, bars: [*bars.first()?, *bars.get(1)?] })
    }
}

/// What `82178D78` and the meter scripts have set on `the_balance_meter`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterDisplay {
    /// Tag `tag_turned_on`: shown (set by `show_balance_meter`, cleared by
    /// `hide_balance_meter`).
    pub turned_on: bool,
    /// Tag `tag_mode`: `Manual` (up/down meter) rather than `balance`.
    pub manual: bool,
    /// The container's position and rotation (degrees; `8238B020`,
    /// `8238A9D8`).
    pub container_pos: Vec2,
    pub container_rot: f32,
    /// The arrow's position inside the container and its rotation
    /// (`82392E70`).
    pub arrow_pos: Vec2,
    pub arrow_rot: f32,
    /// Script parameters `left` / `right` of `update_balance_meter_colors`:
    /// that side of the meter is safe to fall off.
    pub left: bool,
    pub right: bool,
    /// `alpha1` / `alpha2`: how much the left / right half shows.
    pub alpha1: f32,
    pub alpha2: f32,
}

/// One sprite to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    /// Texture name (`.img` in `global.pak.xen`).
    pub texture: &'static str,
    /// Scale from `create_panel_stuff` (the texture's size times this).
    pub scale: f32,
    /// Justification (`just`): which point of the sprite sits at `pos`:
    /// -1 its left/top edge, 0 its centre, 1 its right/bottom edge.
    pub just: [f32; 2],
    /// Position inside the container.
    pub pos: Vec2,
    /// Rotation in degrees, inside the container.
    pub rot: f32,
    /// RGBA as the scripts give it (0..255 each).
    pub rgba: [u8; 4],
    /// The `alpha` the scripts set, 0..1.
    pub alpha: f32,
    /// Draw order (`z_priority`).
    pub z: i32,
}

fn rgba(s: &Scripts, name: &str) -> [u8; 4] {
    let mut out = [0u8; 4];
    if let Some(Value::Array(a)) = s.global(name) {
        for (o, v) in out.iter_mut().zip(a) {
            *o = v.as_f32().unwrap_or(0.0) as u8;
        }
    }
    out
}

impl MeterDisplay {
    /// Retail `82178D78`: `show` false hides the meter; otherwise it places
    /// the arrow for `value` (-1..1, negative = left) and sets the colours.
    /// `manual` is the mode (`SetManualMeter`). `left` / `right` are
    /// physics `+1909` / `+1908` as read there: with
    /// `FLAG_SKATER_LIPTRICK_CAM_REVERSED` set (by the retail camera,
    /// `820D1238`, not translated) both are inverted. `lean` is the running
    /// meter's lean (`820CEB80`).
    #[allow(clippy::too_many_arguments)]
    pub fn set(&mut self, layout: &MeterLayout, show: bool, value: f32, manual: bool, sides: [bool; 2], cam_reversed: bool, lean: f32) {
        // Tag tag_turned_on against `show`: run show/hide_balance_meter.
        // show_balance_meter also checks the global flag NO_DISPLAY_BALANCE
        // (and no_g_display_balance online); global flags are not
        // translated, so it is taken as clear.
        self.turned_on = show;
        if !show {
            return;
        }
        self.manual = manual;
        // Score +396/+400: the point `|value|` along `arrow` (steps of
        // `step`); +404: value * -45 (82004608).
        let a = value.abs();
        // Retail does not clamp `i`; the meter bails before |value| reaches
        // 1 (Lean_Bail_Angle 4000 < 4096). The clamp only keeps a bad
        // value from reading past the list.
        let i = ((a / layout.step) as usize).min(layout.arrow.len().saturating_sub(2));
        let rem = a - i as f32 * layout.step;
        let t = rem * (1.0 / layout.step);
        let (p0, p1) = (layout.arrow[i], layout.arrow[i + 1]);
        let p = p0 + (p1 - p0) * t;
        let rot = value * -45.0;
        // Manual: container at bars[1] turned -90 degrees (82004604);
        // balance: bars[0], not turned.
        let (pos, crot) = if manual { (layout.bars[1], -90.0) } else { (layout.bars[0], 0.0) };
        self.container_pos = pos;
        self.container_rot = crot;
        // The arrow: half the container's size (+408/+412; the container
        // has no dims, so 0: LIKELY) plus the point, mirrored left for
        // negative values; turned by -(+404).
        self.arrow_pos = Vec2::new(if value < 0.0 { -p.x } else { p.x }, p.y);
        self.arrow_rot = -rot;
        // "Right" / "Left" from physics +1908 / +1909.
        let (right, left) = (sides[0], sides[1]);
        self.right = right != cam_reversed;
        self.left = left != cam_reversed;
        // alpha1 / alpha2 from the lean: (|lean| + 1000) * 0.0002
        // (82000D70, 82004600) on the side leaned to, 0 on the other.
        if lean > 0.0 {
            self.alpha1 = (lean + 1000.0) * 0.0002;
            self.alpha2 = 0.0;
        } else {
            self.alpha1 = 0.0;
            self.alpha2 = (-lean + 1000.0) * 0.0002;
        }
    }

    /// `82190A60` (a meter stops) and `820CE618` call `82178D78` with
    /// `show` false for both modes.
    pub fn hide(&mut self) {
        self.turned_on = false;
    }

    /// The four sprites as `create_panel_stuff`, `do_show_balance_meter`
    /// and `update_balance_meter_colors` leave them, in draw order, or
    /// nothing while hidden (`do_hide_balance_meter` sets every alpha to 0).
    /// Colours come from the globals the scripts name:
    /// `DE_BALANCE_METER_COLOR` / `DE_BALANCE_ARROW_COLOR` (returned by
    /// `Theme_GetBalanceMeterColor` `821E1680` / `...ArrowColor`
    /// `821E1710`), `balance_meter_color_safe` / `_danger`.
    pub fn sprites(&self, s: &Scripts) -> Vec<Sprite> {
        if !self.turned_on {
            return Vec::new();
        }
        let meter = rgba(s, "DE_BALANCE_METER_COLOR");
        let ok = rgba(s, "balance_meter_color_safe");
        let bad = rgba(s, "balance_meter_color_danger");
        let color1 = if self.left { ok } else { bad };
        let color2 = if self.right { ok } else { bad };
        // The arrow: do_show_balance_meter gives it DE_BALANCE_ARROW_COLOR,
        // but every update then sets color1 if alpha1 > 0, else color2.
        let arrow = if self.alpha1 > 0.0 { color1 } else { color2 };
        vec![
            Sprite { texture: "balancemeter_bg", scale: 0.6, just: [0.0, 0.0], pos: Vec2::ZERO, rot: 0.0, rgba: meter, alpha: 1.0, z: 1 },
            Sprite { texture: "balancemeter", scale: 0.6, just: [1.0, 0.0], pos: Vec2::ZERO, rot: 0.0, rgba: color1, alpha: self.alpha1, z: 2 },
            Sprite { texture: "balancemeter_2", scale: 0.6, just: [-1.0, 0.0], pos: Vec2::ZERO, rot: 0.0, rgba: color2, alpha: self.alpha2, z: 2 },
            Sprite {
                texture: "balancearrow_glow",
                scale: 0.8,
                just: [0.0, 0.0],
                pos: self.arrow_pos,
                rot: self.arrow_rot,
                rgba: arrow,
                alpha: 1.0,
                z: 3,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn scripts() -> Scripts {
        let p = |x: f32, y: f32| Value::Pair(x, y);
        let arrow = [(0.0, -9.0), (10.0, -9.0), (20.0, -8.0), (35.0, -6.0), (50.0, -3.0), (65.0, 0.0), (80.0, 5.0)];
        let info = Value::Struct(vec![
            (qb_key("bar_positions"), Value::Array(vec![p(320.0, 165.0), p(250.0, 224.0)])),
            (qb_key("arrow_positions"), Value::Array(arrow.iter().map(|&(x, y)| p(x, y)).collect())),
        ]);
        let col = |c: [i32; 4]| Value::Array(c.iter().map(|&v| Value::Int(v)).collect());
        let mut g = BTreeMap::new();
        g.insert(qb_key("balance_meter_info"), info);
        g.insert(qb_key("balance_meter_color_safe"), col([10, 80, 40, 255]));
        g.insert(qb_key("balance_meter_color_danger"), col([110, 50, 40, 255]));
        g.insert(qb_key("DE_BALANCE_METER_COLOR"), col([110, 110, 110, 255]));
        Scripts::new(g)
    }

    #[test]
    fn layout_repeats_the_last_arrow_point() {
        let l = MeterLayout::from_scripts(&scripts()).unwrap();
        assert_eq!(l.arrow.len(), 8);
        assert_eq!(l.arrow[7], Vec2::new(80.0, 5.0));
        assert!((l.step - 1.0 / 7.0).abs() < 1e-6);
        assert_eq!(l.bars, [Vec2::new(320.0, 165.0), Vec2::new(250.0, 224.0)]);
    }

    #[test]
    fn arrow_follows_the_path_and_mirrors_left() {
        let s = scripts();
        let l = MeterLayout::from_scripts(&s).unwrap();
        let mut d = MeterDisplay::default();
        // Middle: first point, upright.
        d.set(&l, true, 0.0, false, [true, false], false, 0.0);
        assert!(d.turned_on);
        assert_eq!(d.arrow_pos, Vec2::new(0.0, -9.0));
        assert_eq!(d.arrow_rot, 0.0);
        assert_eq!(d.container_pos, Vec2::new(320.0, 165.0));
        // Half way (3.5 steps): between (35,-6) and (50,-3).
        d.set(&l, true, 0.5, false, [true, false], false, -2048.0);
        assert!((d.arrow_pos - Vec2::new(42.5, -4.5)).length() < 1e-4);
        assert!((d.arrow_rot - 22.5).abs() < 1e-5);
        assert!(d.alpha1 == 0.0 && (d.alpha2 - 0.6096).abs() < 1e-4);
        // Negative: mirrored, turned the other way; the left half shows.
        d.set(&l, true, -0.5, false, [true, false], false, 2048.0);
        assert!((d.arrow_pos - Vec2::new(-42.5, -4.5)).length() < 1e-4);
        assert!((d.arrow_rot + 22.5).abs() < 1e-5);
        assert!(d.alpha2 == 0.0 && d.alpha1 > 0.0);
        // Colours: right safe (+1908), left not (+1909); the arrow takes
        // the left colour while the left half shows.
        let sp = d.sprites(&s);
        assert_eq!(sp[1].rgba, [110, 50, 40, 255]);
        assert_eq!(sp[2].rgba, [10, 80, 40, 255]);
        assert_eq!(sp[3].rgba, [110, 50, 40, 255]);
        // Manual mode: the other bar position, turned -90.
        d.set(&l, true, 0.0, true, [true, false], false, 0.0);
        assert_eq!((d.container_pos, d.container_rot), (Vec2::new(250.0, 224.0), -90.0));
        d.hide();
        assert!(d.sprites(&s).is_empty());
    }
}
