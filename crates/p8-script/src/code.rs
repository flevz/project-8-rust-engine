//! Reading script bytecode. Tokens are single bytes; their operands are
//! little-endian. Token meanings are named from how the retail VM handles
//! them (see the notes in each reader), not from older tools.

pub const END: u8 = 0x00;
pub const NEWLINE: u8 = 0x01;
/// Line number (4-byte operand), skipped.
pub const LINE: u8 = 0x02;
pub const STRUCT_OPEN: u8 = 0x03;
pub const STRUCT_CLOSE: u8 = 0x04;
pub const ARRAY_OPEN: u8 = 0x05;
pub const ARRAY_CLOSE: u8 = 0x06;
pub const EQUALS: u8 = 0x07;
pub const DOT: u8 = 0x08;
pub const COMMA: u8 = 0x09;
pub const MINUS: u8 = 0x0A;
pub const PLUS: u8 = 0x0B;
pub const DIVIDE: u8 = 0x0C;
pub const MULTIPLY: u8 = 0x0D;
pub const OPEN_PAREN: u8 = 0x0E;
pub const CLOSE_PAREN: u8 = 0x0F;
pub const LESS: u8 = 0x12;
pub const GREATER: u8 = 0x14;
pub const NAME: u8 = 0x16;
pub const INT: u8 = 0x17;
pub const FLOAT: u8 = 0x1A;
pub const STRING: u8 = 0x1B;
pub const VECTOR: u8 = 0x1E;
pub const PAIR: u8 = 0x1F;
pub const BEGIN: u8 = 0x20;
pub const REPEAT: u8 = 0x21;
pub const BREAK: u8 = 0x22;
pub const ENDSCRIPT: u8 = 0x24;
pub const ENDIF: u8 = 0x28;
pub const RETURN: u8 = 0x29;
/// `<...>`: all of the caller's parameters.
pub const ALL_ARGS: u8 = 0x2C;
/// `<name>`: a local parameter.
pub const ARG: u8 = 0x2D;
pub const LONG_JUMP: u8 = 0x2E;
pub const RANDOM: u8 = 0x2F;
pub const RANDOM_RANGE: u8 = 0x30;
pub const OR: u8 = 0x32;
pub const RANDOM_B: u8 = 0x37;
pub const RANDOM_RANGE_B: u8 = 0x38;
pub const NOT: u8 = 0x39;
pub const SWITCH: u8 = 0x3C;
pub const ENDSWITCH: u8 = 0x3D;
pub const CASE: u8 = 0x3E;
pub const DEFAULT: u8 = 0x3F;
pub const RANDOM_NO_REPEAT: u8 = 0x40;
pub const RANDOM_PERMUTE: u8 = 0x41;
pub const COLON: u8 = 0x42;
pub const IF: u8 = 0x47;
pub const ELSE: u8 = 0x48;
pub const SHORT_JUMP: u8 = 0x49;
pub const PACKED_STRUCT: u8 = 0x4A;
/// `?name`: the value of a global.
pub const GLOBAL: u8 = 0x4B;
pub const WIDE_STRING: u8 = 0x4C;

pub fn byte(code: &[u8], at: usize) -> u8 {
    code.get(at).copied().unwrap_or(END)
}

pub fn u16_at(code: &[u8], at: usize) -> usize {
    byte(code, at) as usize | (byte(code, at + 1) as usize) << 8
}

pub fn u32_at(code: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([byte(code, at), byte(code, at + 1), byte(code, at + 2), byte(code, at + 3)])
}

pub fn i16_at(code: &[u8], at: usize) -> i32 {
    u16_at(code, at) as u16 as i16 as i32
}

pub fn f32_at(code: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(code, at))
}

/// Retail `82207ED8`: the position after the token at `pc`.
pub fn skip(code: &[u8], pc: usize) -> usize {
    let t = byte(code, pc);
    match t {
        0x02 | 0x16 | 0x17 | 0x18 | 0x1A | 0x2E | 0x43 | 0x44 | 0x46 => pc + 5,
        0x1E => pc + 13,
        0x1F => pc + 9,
        STRING | WIDE_STRING => pc + 5 + u32_at(code, pc + 1) as usize,
        0x2B => {
            let mut p = pc + 5;
            while byte(code, p) != 0 && p < code.len() {
                p += 1;
            }
            p + 1
        }
        RANDOM | RANDOM_B | RANDOM_NO_REPEAT | RANDOM_PERMUTE => pc + 5 + 6 * u32_at(code, pc + 1) as usize,
        IF | ELSE | SHORT_JUMP => pc + 3,
        PACKED_STRUCT => packed_struct_range(code, pc).1,
        // Tokens with no handler (0x10, 0x19, 0x1C, 0x1D, 0x2A, 0x31):
        // retail returns the same position.
        0x10 | 0x19 | 0x1C | 0x1D | 0x2A | 0x31 => pc,
        _ if (1..=76).contains(&t) => pc + 1,
        _ => pc,
    }
}

/// Token 0x4A: a u16 length, then the struct data at `(pc + 6) & !3`.
/// Returns (data start, end).
pub fn packed_struct_range(code: &[u8], pc: usize) -> (usize, usize) {
    let len = u16_at(code, pc + 1);
    let start = (pc + 6) & !3;
    (start, start + len)
}

/// Retail `82209830`: follow jump and random tokens until a real token.
/// `random(n)` picks 0..n; `sites` keeps the state of the no-repeat and
/// permutation randoms per position.
pub fn transparent(code: &[u8], mut pc: usize, rng: &mut dyn FnMut(u32) -> u32, sites: &mut Sites) -> usize {
    for _ in 0..10_000 {
        match byte(code, pc) {
            LONG_JUMP => pc = (pc as i64 + 5 + u32_at(code, pc + 1) as i32 as i64) as usize,
            SHORT_JUMP => pc = pc + 1 + u16_at(code, pc + 1),
            t @ (RANDOM | RANDOM_B) => {
                let n = u32_at(code, pc + 1) as usize;
                let weights = pc + 5;
                let total: i32 = (0..n).map(|i| i16_at(code, weights + 2 * i)).sum();
                // Retail picks with 821E8508 (0x2F) or 821E8560 (0x37); the
                // generators themselves are not translated.
                let _ = t;
                let mut r = rng(total.max(1) as u32) as i32;
                let mut i = 0;
                while i + 1 < n && r >= i16_at(code, weights + 2 * i) {
                    r -= i16_at(code, weights + 2 * i);
                    i += 1;
                }
                pc = random_target(code, pc, n, i);
            }
            RANDOM_NO_REPEAT => {
                let n = u32_at(code, pc + 1) as usize;
                let last = sites.last.get(&pc).copied();
                let mut pick = rng(n.max(1) as u32) as usize;
                let mut tries = 0;
                while Some(pick) == last && tries < 100 {
                    pick = rng(n.max(1) as u32) as usize;
                    tries += 1;
                }
                sites.last.insert(pc, pick);
                pc = random_target(code, pc, n, pick);
            }
            RANDOM_PERMUTE => {
                let n = u32_at(code, pc + 1) as usize;
                let state = sites.permute.entry(pc).or_insert_with(|| (Vec::new(), 0));
                if state.0.is_empty() || state.1 >= state.0.len() {
                    // Retail 822082F0 makes a new order when it runs out; its
                    // shuffle is not read, so this order is a plain shuffle.
                    let mut order: Vec<usize> = (0..n).collect();
                    for i in (1..order.len()).rev() {
                        let j = rng(i as u32 + 1) as usize;
                        order.swap(i, j);
                    }
                    *state = (order, 0);
                }
                let pick = state.0.get(state.1).copied().unwrap_or(0);
                state.1 += 1;
                pc = random_target(code, pc, n, pick);
            }
            _ => return pc,
        }
    }
    pc
}

/// The branch `i` of a random token: offsets follow the weights; each is
/// relative to the end of its own 4 bytes.
fn random_target(code: &[u8], pc: usize, n: usize, i: usize) -> usize {
    let off_at = pc + 5 + 2 * n + 4 * i;
    (off_at as i64 + 4 + u32_at(code, off_at) as i32 as i64) as usize
}

/// Per-position state of the no-repeat and permutation randoms.
#[derive(Clone, Debug, Default)]
pub struct Sites {
    last: std::collections::HashMap<usize, usize>,
    permute: std::collections::HashMap<usize, (Vec<usize>, usize)>,
}
