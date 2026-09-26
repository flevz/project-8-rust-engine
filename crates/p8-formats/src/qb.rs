//! QB script files: global values (not script bytecode).
//!
//! CONFIRMED on retail `qb.pak.xen` (663 files, 9,644 globals): a 28-byte
//! header, then top-level items
//! `u32 type (0x0020kk00), u32 name key, u32 file key, u32 value-or-offset, u32 next`.
//! `kk` is the value kind. Struct members are linked items
//! `u32 type, u32 name key, u32 value-or-offset, u32 next`, whose kind sits in
//! bits 16..22 of the type. Offsets are relative to the start of the file.
//! Unnamed struct members (name key 0) are kept in order.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    String(String),
    Pair(f32, f32),
    Vector([f32; 3]),
    Checksum(u32),
    Struct(Vec<(u32, Value)>),
    Array(Vec<Value>),
    Script,
    Unknown(u8, u32),
}

impl Value {
    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::Float(f) => Some(f),
            Value::Int(i) => Some(i as f32),
            _ => None,
        }
    }

    /// Member of a struct by name key; unnamed members use key 0.
    pub fn get(&self, key: u32) -> Option<&Value> {
        match self {
            Value::Struct(members) => members.iter().find(|(k, _)| *k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_named(&self, name: &str) -> Option<&Value> {
        self.get(crate::qb_key(name))
    }
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    u32_at(d, o).map(f32::from_bits)
}

fn kind(t: u32) -> u8 {
    let k = ((t >> 8) & 0xFF) as u8;
    if k != 0 { k } else { ((t >> 16) & 0x7F) as u8 }
}

const MAX_DEPTH: u32 = 32;
const MAX_MEMBERS: usize = 100_000;

fn value(d: &[u8], k: u8, v: u32, depth: u32) -> Value {
    if depth > MAX_DEPTH {
        return Value::Unknown(k, v);
    }
    let at = v as usize;
    match k {
        1 => Value::Int(v as i32),
        2 => Value::Float(f32::from_bits(v)),
        3 => {
            let s = d.get(at..).unwrap_or(&[]);
            let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
            Value::String(String::from_utf8_lossy(&s[..end]).into_owned())
        }
        // Pairs and vectors may start with a 4-byte type word.
        5 => {
            let skip = if u32_at(d, at).is_some_and(|w| w >> 16 != 0) {
                4
            } else {
                0
            };
            match (f32_at(d, at + skip), f32_at(d, at + skip + 4)) {
                (Some(a), Some(b)) => Value::Pair(a, b),
                _ => Value::Unknown(k, v),
            }
        }
        6 => {
            let skip = if u32_at(d, at).is_some_and(|w| w >> 16 != 0) {
                4
            } else {
                0
            };
            match (
                f32_at(d, at + skip),
                f32_at(d, at + skip + 4),
                f32_at(d, at + skip + 8),
            ) {
                (Some(a), Some(b), Some(c)) => Value::Vector([a, b, c]),
                _ => Value::Unknown(k, v),
            }
        }
        7 => Value::Script,
        10 => structure(d, at, depth + 1),
        12 => array(d, at, depth + 1),
        13 | 26 => Value::Checksum(v),
        _ => Value::Unknown(k, v),
    }
}

fn structure(d: &[u8], at: usize, depth: u32) -> Value {
    let mut members = Vec::new();
    let mut p = u32_at(d, at + 4).unwrap_or(0) as usize;
    while p != 0 && members.len() < MAX_MEMBERS {
        let (Some(t), Some(key), Some(v), Some(next)) = (
            u32_at(d, p),
            u32_at(d, p + 4),
            u32_at(d, p + 8),
            u32_at(d, p + 12),
        ) else {
            break;
        };
        members.push((key, value(d, kind(t), v, depth)));
        if next as usize <= p {
            break; // Lists only move forward; guards against loops.
        }
        p = next as usize;
    }
    Value::Struct(members)
}

fn array(d: &[u8], at: usize, depth: u32) -> Value {
    let (Some(t), Some(n)) = (u32_at(d, at), u32_at(d, at + 4)) else {
        return Value::Array(vec![]);
    };
    let k = kind(t);
    let n = n as usize;
    if n == 0 || n > MAX_MEMBERS {
        return Value::Array(vec![]);
    }
    let words: Vec<u32> = if n == 1 {
        u32_at(d, at + 8).into_iter().collect()
    } else {
        let base = u32_at(d, at + 8).unwrap_or(0) as usize;
        (0..n).filter_map(|i| u32_at(d, base + 4 * i)).collect()
    };
    Value::Array(words.into_iter().map(|w| value(d, k, w, depth)).collect())
}

/// All top-level globals of one QB file, keyed by name checksum.
pub fn globals(d: &[u8]) -> BTreeMap<u32, Value> {
    let mut out = BTreeMap::new();
    let mut o = 28;
    let Some(file_key) = u32_at(d, o + 8) else {
        return out;
    };
    while let Some(t) = u32_at(d, o) {
        if t & 0xFFFF_00FF != 0x0020_0000 {
            break;
        }
        let (Some(key), Some(v)) = (u32_at(d, o + 4), u32_at(d, o + 12)) else {
            break;
        };
        out.insert(key, value(d, kind(t), v, 0));
        // Items are sequential; find the next top-level header of this file.
        let mut p = o + 20;
        loop {
            match (u32_at(d, p), u32_at(d, p + 8)) {
                (Some(t2), Some(f2)) if t2 & 0xFFFF_00FF == 0x0020_0000 && f2 == file_key => break,
                (Some(_), Some(_)) => p += 4,
                _ => return out,
            }
        }
        o = p;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qb_key;

    fn w(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    /// A synthetic file: a float global and a struct with an unnamed pair and a named float.
    fn sample() -> Vec<u8> {
        let file = qb_key("test.qb");
        let mut d = vec![0u8; 28];
        // Item 1 at 28: float 2.5.
        w(&mut d, 0x0020_0200);
        w(&mut d, qb_key("gravity_like"));
        w(&mut d, file);
        w(&mut d, 2.5f32.to_bits());
        w(&mut d, 0);
        // Item 2 at 48: struct at 68.
        w(&mut d, 0x0020_0A00);
        w(&mut d, qb_key("group"));
        w(&mut d, file);
        w(&mut d, 68);
        w(&mut d, 0);
        // Struct header at 68: flags, first member at 76.
        w(&mut d, 0x0000_0100);
        w(&mut d, 76);
        // Member at 76: unnamed pair at 108, next 92.
        w(&mut d, 0x0085_0000);
        w(&mut d, 0);
        w(&mut d, 108);
        w(&mut d, 92);
        // Member at 92: float "limit" 9.5, no next.
        w(&mut d, 0x0082_0000);
        w(&mut d, qb_key("limit"));
        w(&mut d, 9.5f32.to_bits());
        w(&mut d, 0);
        // Pair data at 108 (with a type word).
        w(&mut d, 0x0001_0000);
        w(&mut d, 7.0f32.to_bits());
        w(&mut d, 7.6f32.to_bits());
        d
    }

    #[test]
    fn decodes_floats_structs_and_unnamed_pairs() {
        let g = globals(&sample());
        assert_eq!(g[&qb_key("gravity_like")], Value::Float(2.5));
        let s = &g[&qb_key("group")];
        assert_eq!(s.get(0), Some(&Value::Pair(7.0, 7.6)));
        assert_eq!(s.get_named("limit").and_then(Value::as_f32), Some(9.5));
    }

    #[test]
    fn garbage_does_not_panic() {
        let noise: Vec<u8> = (0..512u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8)
            .collect();
        let _ = globals(&noise);
        let mut looped = sample();
        // Make the member list point backwards.
        looped[88..92].copy_from_slice(&76u32.to_be_bytes());
        let _ = globals(&looped);
    }
}
