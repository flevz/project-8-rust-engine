//! Script parameter lists (retail `CStruct`): ordered components, each named
//! or unnamed (name 0), as scripts pass them to commands and to each other.
use p8_formats::qb::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(pub Vec<(u32, Value)>);

/// Whether AddComponent (`82214618`) treats two values as the same kind:
/// equal types, with integers and floats counted as one.
fn same_kind(a: &Value, b: &Value) -> bool {
    use Value::*;
    matches!((a, b), (Int(_) | Float(_), Int(_) | Float(_))) || std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// A global named by a checksum, following globals that are themselves
/// names (retail symbol type 13 aliases).
pub fn resolve_alias(mut c: u32, global: &dyn Fn(u32) -> Option<Value>) -> Option<Value> {
    for _ in 0..16 {
        match global(c)? {
            Value::Checksum(next) => c = next,
            v => return Some(v),
        }
    }
    None
}

impl Params {
    pub fn new() -> Self {
        Self::default()
    }

    /// From a struct value (anything else gives an empty list).
    pub fn from_struct(v: &Value) -> Self {
        match v {
            Value::Struct(m) => Self(m.clone()),
            _ => Self::default(),
        }
    }

    pub fn to_struct(&self) -> Value {
        Value::Struct(self.0.clone())
    }

    /// Retail AddComponent (`82214618`, mode 0): drop components with the
    /// same name and kind (unnamed checksums only when the checksum is the
    /// same), then append.
    pub fn add(&mut self, key: u32, v: Value) {
        self.0.retain(|(k, old)| {
            if *k != key || !same_kind(old, &v) {
                return true;
            }
            match (old, &v) {
                (Value::Checksum(a), Value::Checksum(b)) if key == 0 => a != b,
                _ => false,
            }
        });
        self.0.push((key, v));
    }

    /// Retail `82215B88`: add every component of `other`.
    pub fn merge(&mut self, other: &Params) {
        for (k, v) in &other.0 {
            self.add(*k, v.clone());
        }
    }

    /// Retail `82211BE0`: the component named `key` (the last one found).
    pub fn get(&self, key: u32) -> Option<&Value> {
        self.0.iter().rev().find(|(k, _)| *k == key && key != 0).map(|(_, v)| v)
    }

    /// Retail `82211DD0(params, 0)`: the first component, named or not.
    pub fn first(&self) -> Option<&Value> {
        self.0.first().map(|(_, v)| v)
    }

    /// Retail `82211BE0` in full: the component named `key`, also searching
    /// the global structures that unnamed checksums name (a struct can
    /// "include" another this way); the last match wins. `global` looks up
    /// a global.
    pub fn get_in(&self, key: u32, global: &dyn Fn(u32) -> Option<Value>) -> Option<Value> {
        self.get_in_depth(key, global, 0)
    }

    fn get_in_depth(&self, key: u32, global: &dyn Fn(u32) -> Option<Value>, depth: u32) -> Option<Value> {
        let mut found = None;
        for (k, v) in &self.0 {
            if key != 0 && *k == key {
                found = Some(v.clone());
            } else if *k == 0
                && let Value::Checksum(c) = v
                && depth < 16
                && let Some(Value::Struct(inner)) = resolve_alias(*c, global)
                && let Some(v) = Params(inner).get_in_depth(key, global, depth + 1)
            {
                found = Some(v);
            }
        }
        found
    }

    /// Retail `82213628`: an unnamed checksum equal to `key`.
    pub fn flag(&self, key: u32) -> bool {
        self.0.iter().any(|(k, v)| *k == 0 && *v == Value::Checksum(key))
    }

    /// Retail `GotParam` (`82213400`): a component named `key`, or the flag.
    pub fn got(&self, key: u32) -> bool {
        self.0.iter().any(|(k, _)| *k == key) || self.flag(key)
    }

    /// A named number (integers read as floats).
    pub fn float(&self, key: u32) -> Option<f32> {
        self.get(key).and_then(Value::as_f32)
    }

    pub fn int(&self, key: u32) -> Option<i32> {
        match self.get(key)? {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn checksum(&self, key: u32) -> Option<u32> {
        match self.get(key)? {
            Value::Checksum(c) => Some(*c),
            _ => None,
        }
    }

    /// The first unnamed checksum (retail `82212D48(params, 0, ...)`).
    pub fn unnamed_checksum(&self) -> Option<u32> {
        self.0.iter().find_map(|(k, v)| match (k, v) {
            (0, Value::Checksum(c)) => Some(*c),
            _ => None,
        })
    }

    /// The first unnamed array (retail `82212AD0(params, 0, ...)`).
    pub fn unnamed_array(&self) -> Option<&[Value]> {
        self.0.iter().find_map(|(k, v)| match (k, v) {
            (0, Value::Array(a)) => Some(a.as_slice()),
            _ => None,
        })
    }

    /// The first unnamed number (retail `822128C8(params, 0, ...)`).
    pub fn unnamed_float(&self) -> Option<f32> {
        self.0.iter().find_map(|(k, v)| if *k == 0 { v.as_f32() } else { None })
    }

    /// The first unnamed integer (retail `82212860(params, 0, ...)`).
    pub fn unnamed_int(&self) -> Option<i32> {
        self.0.iter().find_map(|(k, v)| match (k, v) {
            (0, Value::Int(i)) => Some(*i),
            _ => None,
        })
    }

    /// A named struct as a parameter list.
    pub fn params(&self, key: u32) -> Option<Params> {
        match self.get(key)? {
            v @ Value::Struct(_) => Some(Params::from_struct(v)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_replaces_same_name_and_kind_and_keeps_distinct_flags() {
        let mut p = Params::new();
        p.add(1, Value::Int(3));
        p.add(1, Value::Float(2.5));
        p.add(1, Value::Checksum(9));
        p.add(0, Value::Checksum(5));
        p.add(0, Value::Checksum(6));
        p.add(0, Value::Checksum(5));
        assert_eq!(
            p.0,
            vec![(1, Value::Float(2.5)), (1, Value::Checksum(9)), (0, Value::Checksum(6)), (0, Value::Checksum(5))]
        );
        assert!(p.got(1) && p.got(5) && p.flag(6) && !p.got(7));
    }
}
