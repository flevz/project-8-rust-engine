//! Access to the player's QB script globals, as the retail code reads them.
use crate::stats::StatLevels;
use p8_formats::qb::Value;
use p8_formats::qb_key;
use std::collections::BTreeMap;

/// All script globals from the player's `qb.pak.xen`, keyed by name checksum.
#[derive(Clone, Debug, Default)]
pub struct Scripts {
    pub globals: BTreeMap<u32, Value>,
}

/// Game state consulted by stat lookups.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatContext {
    /// Retail `820B8220(skater)` (LIKELY "riding switch").
    pub switch_stance: bool,
    /// Retail difficulty index from `8219D3C0`/`82159570` (plus one when
    /// `is_classic`). Only 0 and 2 scale values; `None` means no scaling.
    pub difficulty: Option<u32>,
}

impl Scripts {
    pub fn new(globals: BTreeMap<u32, Value>) -> Self {
        Self { globals }
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(&qb_key(name))
    }

    /// Retail `82216DE8`: a global float, or 0 when missing.
    pub fn global_float(&self, name: &str) -> f32 {
        self.global(name).and_then(Value::as_f32).unwrap_or(0.0)
    }

    /// The struct holding physics values: `skater_physics`, or
    /// `bicycle_physics` while on a bike (retail `+1564`).
    fn physics_struct(&self, on_bike: bool) -> Option<&Value> {
        self.global(if on_bike { "bicycle_physics" } else { "skater_physics" })
    }

    /// Retail `82197F30`: a member of `skater_physics` (or `bicycle_physics`),
    /// else the global of the same name, else 0.
    pub fn physics_float(&self, name: &str, on_bike: bool) -> f32 {
        self.physics_struct(on_bike)
            .and_then(|s| s.get_named(name))
            .and_then(Value::as_f32)
            .unwrap_or_else(|| self.global_float(name))
    }

    /// A struct global, following global names that point at other globals
    /// (e.g. `terrain_default` = `standard_terrain_default`).
    fn struct_global(&self, key: u32) -> Option<&Value> {
        let mut v = self.globals.get(&key)?;
        for _ in 0..8 {
            match v {
                Value::Checksum(k) => v = self.globals.get(k)?,
                Value::Struct(_) => return Some(v),
                _ => return None,
            }
        }
        None
    }

    /// A number, or a checksum naming a number global.
    fn number(&self, v: &Value) -> Option<f32> {
        match v {
            Value::Checksum(k) => self.globals.get(k).and_then(Value::as_f32),
            v => v.as_f32(),
        }
    }

    /// Retail `8228E620`: a terrain's `PhysicsActions` value (e.g.
    /// `SKATE_ROLL_FRICTION`), falling back to `TERRAIN_DEFAULT`, else 0.
    ///
    /// LIKELY: that a checksum value (`skate_roll_friction = default_friction`)
    /// resolves to the global of that name, and that the terrain index at
    /// physics `+298` maps to these `terrain_*` globals (`8228E460` is not
    /// translated; callers pass the terrain name directly).
    pub fn terrain_float(&self, terrain: &str, name: &str) -> f32 {
        let lookup = |t: &str| {
            self.struct_global(qb_key(t))?
                .get_named("PhysicsActions")?
                .get_named(name)
                .and_then(|v| self.number(v))
        };
        lookup(terrain).or_else(|| lookup("TERRAIN_DEFAULT")).unwrap_or(0.0)
    }

    /// Retail `82199D00` + `82199A28`: a stat-scaled value.
    pub fn stat(&self, name: &str, on_bike: bool, stats: &StatLevels, ctx: StatContext) -> f32 {
        let found = self.physics_struct(on_bike).and_then(|s| s.get_named(name)).or_else(|| self.global(name));
        match found {
            Some(v) => self.stat_value(v, stats, ctx),
            None => 0.0,
        }
    }

    /// Retail `82199A28`, translated line by line.
    fn stat_value(&self, def: &Value, stats: &StatLevels, ctx: StatContext) -> f32 {
        let unnamed = |pick: &dyn Fn(&Value) -> bool| match def {
            Value::Struct(m) => m.iter().find(|(k, v)| *k == 0 && pick(v)).map(|(_, v)| v.clone()),
            _ => None,
        };
        let (lo, hi) = match unnamed(&|v| matches!(v, Value::Pair(..))) {
            Some(Value::Pair(a, b)) => (a, b),
            _ => (0.0, 0.0),
        };
        // The stat index is an unnamed checksum naming a STATS_* global.
        // Missing -> level 10.0 (constant at 820564C4).
        let level = match unnamed(&|v| matches!(v, Value::Checksum(_))) {
            Some(Value::Checksum(k)) => match self.globals.get(&k).and_then(Value::as_f32) {
                Some(i) => stats.level(i as usize),
                None => 10.0,
            },
            _ => 10.0,
        };
        let mut value = lo + (hi - lo) * level * 0.1;
        if ctx.switch_stance
            && let Some((a, b)) = self.pair_member(def, "switch")
        {
            let factor = (a + (b - a) * stats.level(crate::stats::index::SWITCH) * 0.1).clamp(0.0, 1.0);
            value *= factor;
        }
        if let (Some(d), Some((a, b))) = (ctx.difficulty, self.pair_member(def, "diff")) {
            match d {
                0 => value *= a,
                2 => value *= b,
                _ => {}
            }
        }
        if let Some(limit) = def.get_named("limit").and_then(Value::as_f32) {
            if lo < hi {
                value = value.min(limit);
            } else {
                value = value.max(limit);
            }
        }
        if let Some(m) = self.float_member(def, "modifier")
            && let Some((a, b)) = self.pair_member(def, "modifier_range")
        {
            // 0.111111 = 1/9 (constant at 820035A8).
            value *= a + (m - 1.0) * (1.0 / 9.0) * (b - a);
        }
        value
    }

    /// A struct member that may be a pair directly or name a pair global.
    fn pair_member(&self, def: &Value, name: &str) -> Option<(f32, f32)> {
        match def.get_named(name)? {
            Value::Pair(a, b) => Some((*a, *b)),
            Value::Checksum(k) => match self.globals.get(k)? {
                Value::Pair(a, b) => Some((*a, *b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// A struct member that may be a number directly or name a number global.
    fn float_member(&self, def: &Value, name: &str) -> Option<f32> {
        self.number(def.get_named(name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripts() -> Scripts {
        let k = qb_key;
        let stat_def = Value::Struct(vec![
            (0, Value::Pair(7.0, 7.6)),
            (0, Value::Checksum(k("STATS_AIR"))),
            (k("switch"), Value::Checksum(k("standard_switch"))),
        ]);
        let limited = Value::Struct(vec![
            (0, Value::Pair(9.5, 9.5)),
            (k("limit"), Value::Float(9.0)),
            (0, Value::Checksum(k("STATS_SPEED"))),
            (k("modifier"), Value::Checksum(k("test_current_skater_speed"))),
            (k("modifier_range"), Value::Pair(0.5, 1.0)),
        ]);
        let physics = Value::Struct(vec![(k("physics_brake_acceleration"), Value::Int(23)), (k("limited"), limited)]);
        Scripts::new(
            [
                (k("skater_physics"), physics),
                (k("jump"), stat_def),
                (k("STATS_AIR"), Value::Int(0)),
                (k("STATS_SPEED"), Value::Int(3)),
                (k("STATS_SWITCH"), Value::Int(6)),
                (k("standard_switch"), Value::Pair(0.5, 1.0)),
                (k("test_current_skater_speed"), Value::Int(10)),
                (k("default_friction"), Value::Float(0.025)),
            ]
            .into_iter()
            .collect(),
        )
    }

    #[test]
    fn physics_float_prefers_the_struct_then_globals() {
        let s = scripts();
        assert_eq!(s.physics_float("physics_brake_acceleration", false), 23.0);
        assert_eq!(s.physics_float("default_friction", false), 0.025);
        assert_eq!(s.physics_float("missing", false), 0.0);
    }

    #[test]
    fn stat_is_linear_over_ten_levels_with_switch_limit_and_modifier() {
        let s = scripts();
        let mut stats = StatLevels::with_default(5.0);
        let ctx = StatContext::default();
        assert!((s.stat("jump", false, &stats, ctx) - 7.3).abs() < 1e-5);
        stats.levels[0] = Some(10.0);
        assert!((s.stat("jump", false, &stats, ctx) - 7.6).abs() < 1e-5);
        // Switch at switch-stat 5: factor 0.5 + 0.5*0.5 = 0.75.
        let sw = StatContext { switch_stance: true, ..ctx };
        assert!((s.stat("jump", false, &stats, sw) - 7.6 * 0.75).abs() < 1e-5);
        // lo == hi: the limit acts as a floor (retail branch at 82199C3C).
        // Modifier 10 with range (0.5, 1.0) gives factor 1.0.
        assert!((s.stat("limited", false, &stats, ctx) - 9.5).abs() < 1e-5);
    }
}
