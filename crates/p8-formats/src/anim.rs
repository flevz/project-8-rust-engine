//! `.ska` animation clips (in `DATA/COMPRESSED/PAK/perm_anims.pak.xen`,
//! named by checksum, e.g. `Sk8_Gnd_Stnd_Base_xx`), translated from the
//! retail sampler. Every rule below cites the retail function it comes from.
//!
//! File layout (big endian, offsets from the file start):
//!
//! - `+0x0C` header offset `H`, `+0x10` the clip's own key table (6 bytes
//!   per entry), `+0x18` bone mask: a bit count, then one bit per skeleton
//!   bone (bone `b` = bit `b & 31` of word `b >> 5`).
//! - `H + 4` flags, `H + 8` duration (s), `H + 0xD` bone count, `H + 0x18`
//!   rotation data, `H + 0x1C` position data, `H + 0x20` rotation sizes
//!   (u16 per bone), `H + 0x24` position sizes.
//! - Flag `0x0080_0000`: compressed keys (822EEA30, decompressed by
//!   822EF058 + 822EEF98 into the "fullres" cache, then sampled by
//!   822EB590). Otherwise one pose of floats: quaternions (x, y, z, w) at
//!   `H + 0x18` and positions (x, y, z, pad) at `H + 0x1C`, 16 bytes per
//!   bone, used at every time (822EA3D0).
//! - Flag `0x0008_0000`: the bone mask is used; bones not in it get
//!   weight 0 (822EB590, 822EA3D0).
//!
//! Rotations are returned as stored. The skeleton turns every local
//! rotation into its conjugate before building matrices (82327678), the
//! same as [`crate::skeleton`]; [`Pose::bevy_rotation`] does that.
use std::path::Path;

use crate::{pak, qb_key};

pub const COMPRESSED: u32 = 0x0080_0000;
pub const USES_MASK: u32 = 0x0008_0000;
/// With [`SKIP_EMPTY_POSITIONS`]: positions are added to what is already
/// in the output (822EB590).
pub const ADD_POSITIONS: u32 = 0x4000;
/// Bones with no position keys keep the output's position (822EB590).
pub const SKIP_EMPTY_POSITIONS: u32 = 0x0002_0000;
/// Frames per second of key times (822EB590 multiplies by 60).
pub const FPS: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QKey {
    /// Time field as the decompressor writes it; the frame is the low 15
    /// bits, sign extended (822E90B8).
    pub time: u16,
    pub q: [f32; 4],
}

impl QKey {
    pub fn frame(&self) -> f32 {
        (((self.time as u32 & 0x7FFF) << 17) as i32 >> 17) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TKey {
    /// Frame, read as a signed 16-bit value (822E9330).
    pub time: u16,
    pub t: [f32; 3],
}

impl TKey {
    pub fn frame(&self) -> f32 {
        self.time as i16 as f32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tracks {
    /// One pose used at every time (822EA3D0).
    Pose { q: Vec<[f32; 4]>, t: Vec<[f32; 3]> },
    /// Keys per bone (fullres cache of 822EF058).
    Keys { q: Vec<Vec<QKey>>, t: Vec<Vec<TKey>> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub flags: u32,
    pub duration: f32,
    pub bones: usize,
    /// Bone mask words (without the count).
    pub mask: Vec<u32>,
    pub tracks: Tracks,
}

/// One bone of a sampled clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneSample {
    /// 1 when the clip animates this bone, 0 otherwise.
    pub weight: f32,
    /// Stored rotation (x, y, z, w), not yet conjugated.
    pub q: [f32; 4],
    /// `None`: the clip leaves the position alone (see
    /// [`SKIP_EMPTY_POSITIONS`]).
    pub t: Option<[f32; 3]>,
}

pub type Pose = Vec<BoneSample>;

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| format!("clip too short at {at:#x}"))
}
fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("clip too short at {at:#x}"))
}
fn bef(b: &[u8], at: usize) -> Result<f32, String> {
    be32(b, at).map(f32::from_bits)
}
fn byte(b: &[u8], at: usize) -> Result<u8, String> {
    b.get(at).copied().ok_or_else(|| format!("clip too short at {at:#x}"))
}
/// `standardkeyQ.bin.xen` holds little-endian u16s (822E9B10 swaps them).
fn le16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| format!("key table too short at {at:#x}"))
}
fn s16(v: u32) -> i32 {
    v as u16 as i16 as i32
}

/// Rotation keys (822EF058 mode 4), times as 822EF058 leaves them.
fn decompress_q(d: &[u8], std_q: &[u8], h: usize, flags: u32, n: usize) -> Result<Vec<Vec<QKey>>, String> {
    let (data, sizes, table) = (be32(d, h + 0x18)? as usize, be32(d, h + 0x20)? as usize, be32(d, 0x10)? as usize);
    let small = flags & 0x8000 != 0;
    let (mut p, mut prev, mut tmp_time) = (data, 0i32, 0u32);
    let mut out = Vec::with_capacity(n);
    for j in 0..n {
        let end = p + be16(d, sizes + 2 * j)? as usize;
        let mut keys = Vec::new();
        while p < end {
            let explicit_time = flags & 0x100 != 0;
            let mut q = p + 2;
            let (r4, r10) = if explicit_time {
                let t = be16(d, p)?;
                let h = be16(d, q)?;
                q += 2;
                (t, h)
            } else {
                (0, be16(d, p)?)
            };
            let first = if explicit_time { r4 } else { r10 };
            let mut r7 = s16(((prev as u32) & 0xFFFF_8000) | (first as u32 & 0x7FFF));
            let hdr = r10 as u32;
            let (x, y, z);
            if hdr & 0x4000 == 0 {
                if small {
                    x = s16((byte(d, q)? as u32) << 8) as u32;
                    y = s16((byte(d, q + 1)? as u32) << 8) as u32;
                    z = s16((byte(d, q + 2)? as u32) << 8) as u32;
                    q += 3;
                } else {
                    x = be16(d, q)? as u32;
                    y = be16(d, q + 2)? as u32;
                    z = be16(d, q + 4)? as u32;
                    q += 6;
                }
            } else if hdr & 0x3800 == 0 {
                let idx = byte(d, q)? as usize;
                q += 1;
                if flags & 0x2000 != 0 {
                    let e = table + 6 * idx;
                    (x, y, z) = (be16(d, e)? as u32, be16(d, e + 2)? as u32, be16(d, e + 4)? as u32);
                } else {
                    let e = 8 * idx;
                    (x, y, z) = (le16(std_q, e)? as u32, le16(std_q, e + 2)? as u32, le16(std_q, e + 4)? as u32);
                }
                r7 = s16((r7 as u32 & 0xFFFF_8000) | (r7 as u32 & 0x7FF));
            } else {
                let mut v = [0u32; 3];
                for (k, bit) in [0x2000u32, 0x1000, 0x0800].into_iter().enumerate() {
                    let set = hdr & bit != 0;
                    v[k] = if small {
                        let b = byte(d, q)? as u32;
                        q += 1;
                        match (set, flags & 0x10000 != 0) {
                            (true, true) => le16(std_q, 8 * b as usize + 6)? as u32,
                            (true, false) => b,
                            (false, _) => s16(b << 8) as u32,
                        }
                    } else if set {
                        let b = byte(d, q)? as u32;
                        q += 1;
                        if flags & 0x10000 != 0 {
                            le16(std_q, 8 * b as usize + 6)? as u32
                        } else {
                            b
                        }
                    } else {
                        let w = be16(d, q)? as u32;
                        q += 2;
                        w
                    };
                }
                (x, y, z) = (v[0], v[1], v[2]);
                r7 = s16((r7 as u32 & 0xFFFF_8000) | (r7 as u32 & 0x7FF));
            }
            if explicit_time {
                r7 = s16((r7 as u32 & 0xFFFF_8000) | (r4 as u32 & 0x7FFF));
            }
            p = q;
            // 822EEF98: 14-bit fixed point xyz, w rebuilt as positive.
            let v = [s16(x) as f32 / 16384.0, s16(y) as f32 / 16384.0, s16(z) as f32 / 16384.0];
            let w = (1.0 - (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])).clamp(0.0, 1.0).sqrt();
            let t = (tmp_time & 0x8000) | (r7 as u32 & 0x7FFF);
            tmp_time = t;
            keys.push(QKey { time: t as u16, q: [v[0], v[1], v[2], w] });
            prev = r7 & 0xFFFF;
        }
        if p != end {
            return Err(format!("bone {j}: rotation keys overran their size"));
        }
        out.push(keys);
    }
    Ok(out)
}

/// Position keys (822EF058 mode 4).
fn decompress_t(d: &[u8], h: usize, n: usize) -> Result<Vec<Vec<TKey>>, String> {
    let (data, sizes) = (be32(d, h + 0x1C)? as usize, be32(d, h + 0x24)? as usize);
    let (mut p, mut last) = (data, [0.0f32; 3]);
    let mut out = Vec::with_capacity(n);
    for j in 0..n {
        let end = p + be16(d, sizes + 2 * j)? as usize;
        let mut keys = Vec::new();
        while p < end {
            let h = byte(d, p)?;
            let mut q = p + 1;
            let time = if h & 0x40 != 0 {
                (h & 0x3F) as u16
            } else {
                q += 2;
                be16(d, p + 1)?
            };
            if h & 0x80 != 0 {
                q += 1; // reuse the previous position
            } else {
                last = [bef(d, q)?, bef(d, q + 4)?, bef(d, q + 8)?];
                q += 12;
            }
            keys.push(TKey { time, t: last });
            p = q;
        }
        if p != end {
            return Err(format!("bone {j}: position keys overran their size"));
        }
        out.push(keys);
    }
    Ok(out)
}

/// The two keys around `frame` (822E90B8 / 822E9330): the last key at or
/// before the frame and the one after it; the first two keys when the
/// frame is before the second key; the last key twice past the end or when
/// there is only one key. (The retail search also keeps a per-caller hint
/// to start from; that only makes it faster.)
fn around<K>(keys: &[K], frame: f32, time: impl Fn(&K) -> f32) -> (usize, usize) {
    if keys.len() < 2 {
        return (0, 0);
    }
    let (mut k0, mut k1) = (0, 1);
    while time(&keys[k1]) <= frame {
        k0 = k1;
        k1 += 1;
        if k1 >= keys.len() {
            return (k0, k0);
        }
    }
    (k0, k1)
}

/// Quaternion blend 822EAEB0: `u` = 0 or 1 give a key exactly; otherwise
/// the second key is flipped into the first one's hemisphere, the two are
/// lerped and the result normalised (a zero result stays zero).
pub fn nlerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    if u == 0.0 {
        return a;
    }
    if u == 1.0 {
        return b;
    }
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let b = if dot < 0.0 { [-b[0], -b[1], -b[2], -b[3]] } else { b };
    let r = [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * u);
    normalize(r)
}

fn normalize(r: [f32; 4]) -> [f32; 4] {
    let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
    if l == 0.0 {
        r
    } else {
        r.map(|c| c / l)
    }
}

impl Clip {
    /// `std_q` is `DATA/ANIMS/standardkeyQ.bin.xen` as read from disk.
    pub fn parse(d: &[u8], std_q: &[u8]) -> Result<Self, String> {
        let h = be32(d, 0x0C)? as usize;
        let flags = be32(d, h + 4)?;
        let duration = bef(d, h + 8)?;
        let bones = byte(d, h + 0x0D)? as usize;
        let mask_at = be32(d, 0x18)? as usize;
        let mask = if flags & USES_MASK != 0 {
            let count = be32(d, mask_at)? as usize;
            if count > 1024 {
                return Err(format!("implausible mask size {count}"));
            }
            (0..count.div_ceil(32)).map(|i| be32(d, mask_at + 4 + 4 * i)).collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        let tracks = if flags & COMPRESSED != 0 {
            Tracks::Keys { q: decompress_q(d, std_q, h, flags, bones)?, t: decompress_t(d, h, bones)? }
        } else {
            let (qa, ta) = (be32(d, h + 0x18)? as usize, be32(d, h + 0x1C)? as usize);
            let mut q = Vec::with_capacity(bones);
            let mut t = Vec::with_capacity(bones);
            for i in 0..bones {
                let (a, b) = (qa + 16 * i, ta + 16 * i);
                q.push([bef(d, a)?, bef(d, a + 4)?, bef(d, a + 8)?, bef(d, a + 12)?]);
                t.push([bef(d, b)?, bef(d, b + 4)?, bef(d, b + 8)?]);
            }
            Tracks::Pose { q, t }
        };
        Ok(Self { flags, duration, bones, mask, tracks })
    }

    /// Whether the clip animates skeleton bone `b` (822EB590 / 822EA3D0).
    pub fn animates(&self, b: usize) -> bool {
        if b >= self.bones {
            return false;
        }
        if self.flags & USES_MASK == 0 {
            return true;
        }
        self.mask.get(b >> 5).is_some_and(|w| w & (1 << (b & 31)) != 0)
    }

    /// Sample every bone at `time` seconds (822EB590 for keyed clips,
    /// 822EA3D0 for single poses). The caller wraps or clamps `time`
    /// (the cycle node does that in retail).
    pub fn sample(&self, time: f32) -> Pose {
        let none = BoneSample { weight: 0.0, q: [0.0, 0.0, 0.0, 1.0], t: None };
        let mut out = vec![none; self.bones];
        match &self.tracks {
            Tracks::Pose { q, t } => {
                for b in 0..self.bones {
                    out[b] = BoneSample { weight: if self.animates(b) { 1.0 } else { 0.0 }, q: q[b], t: Some(t[b]) };
                }
            }
            Tracks::Keys { q, t } => {
                // Retail clamps the frame into the key pair and keeps the
                // clamped value for the following bones (separately for
                // rotations and positions).
                let (mut fq, mut ft) = (time * FPS, time * FPS);
                for b in 0..self.bones {
                    if !self.animates(b) {
                        continue;
                    }
                    let mut s = BoneSample { weight: 1.0, ..none };
                    let keys = &q[b];
                    if !keys.is_empty() {
                        let (k0, k1) = around(keys, fq, QKey::frame);
                        let (t0, t1) = (keys[k0].frame(), keys[k1].frame());
                        let u = if t0 == t1 {
                            0.0
                        } else {
                            fq = fq.clamp(t0, t1);
                            (fq - t0) / (t1 - t0)
                        };
                        s.q = nlerp(keys[k0].q, keys[k1].q, u);
                    }
                    // No position keys: with SKIP_EMPTY_POSITIONS retail leaves
                    // the output alone; without it retail reads the next
                    // bone's first key (UNKNOWN whether that ever happens);
                    // both are left as `None` here.
                    let keys = &t[b];
                    if !keys.is_empty() {
                        let (k0, k1) = around(keys, ft, TKey::frame);
                        let (t0, t1) = (keys[k0].frame(), keys[k1].frame());
                        let u = if t0 == t1 {
                            0.0
                        } else {
                            ft = ft.clamp(t0, t1);
                            (ft - t0) / (t1 - t0)
                        };
                        let (a, c) = (keys[k0].t, keys[k1].t);
                        s.t = Some([0, 1, 2].map(|i| a[i] + (c[i] - a[i]) * u));
                    }
                    out[b] = s;
                }
            }
        }
        out
    }

    /// Positions are added to the output instead of replacing it.
    pub fn adds_positions(&self) -> bool {
        self.flags & ADD_POSITIONS != 0 && self.flags & SKIP_EMPTY_POSITIONS != 0
    }

    /// Load the clip whose name checksum is `key` from an archive (e.g.
    /// `perm_anims.pak.xen`), with `standardkeyQ.bin.xen` at `std_q`.
    pub fn load(pak_path: &Path, key: u32, std_q: &[u8]) -> Result<Self, String> {
        let all = load_all(pak_path, &[key], std_q)?;
        all.into_iter().next().ok_or_else(|| format!("no clip {key:08x} in {}", pak_path.display()))
    }
}

/// Load several clips from one archive, in the order of `keys`.
pub fn load_all(pak_path: &Path, keys: &[u32], std_q: &[u8]) -> Result<Vec<Clip>, String> {
    let headers = std::fs::read(pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
    let pab = std::fs::read(pak_path.to_string_lossy().replace(".pak.xen", ".pab.xen")).ok();
    let (archive, data) =
        pak::parse_file(&headers, pab.as_deref()).map_err(|e| format!("{} was not recognised: {e:?}", pak_path.display()))?;
    let ska = qb_key(".ska");
    keys.iter()
        .map(|&key| {
            let e = archive
                .entries
                .iter()
                .find(|e| e.type_key == ska && e.full_name_key == key)
                .ok_or_else(|| format!("no clip {key:08x} in {}", pak_path.display()))?;
            Clip::parse(data.get(e.offset..e.offset + e.size).ok_or("clip entry past the archive")?, std_q)
                .map_err(|err| format!("clip {key:08x}: {err}"))
        })
        .collect()
}

/// Hamilton product `a ⊗ b` (x, y, z, w).
pub fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// ApplyDifference node (8237B1D8 / 82377058 / 823767C8): `base` is the
/// node's second child, `diff` its first (an `_xDx` clip). Per bone with
/// the difference's weight `w`: rotation = nlerp(base, base ⊗ diff, w)
/// (with the hemisphere fix), position = base + w · diff.
pub fn apply_difference(base: &Pose, diff: &Pose) -> Pose {
    base.iter()
        .zip(diff.iter().chain(std::iter::repeat(&BoneSample { weight: 0.0, q: [0.0, 0.0, 0.0, 1.0], t: None })))
        .map(|(a, d)| {
            let w = d.weight;
            if w == 0.0 {
                return *a;
            }
            let q = nlerp(a.q, qmul(a.q, d.q), w);
            let t = match (a.t, d.t) {
                (Some(at), Some(dt)) => Some([0, 1, 2].map(|i| at[i] + w * dt[i])),
                (at, _) => at,
            };
            BoneSample { weight: a.weight, q, t }
        })
        .collect()
}

impl BoneSample {
    /// Rotation relative to the parent as the renderer uses it: the
    /// conjugate of the stored value (82327678).
    pub fn bevy_rotation(&self) -> [f32; 4] {
        [-self.q[0], -self.q[1], -self.q[2], self.q[3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// A two-bone compressed clip: bone 0 has two rotation keys (frames 0
    /// and 10, 16-bit components), bone 1 one position key with a short
    /// time and one that reuses the previous position.
    fn synthetic() -> Vec<u8> {
        let mut d = vec![0u8; 0x200];
        put32(&mut d, 0x0C, 0x20);
        put32(&mut d, 0x10, 0x1F0);
        put32(&mut d, 0x18, 0x1E0);
        put32(&mut d, 0x20 + 4, COMPRESSED);
        put32(&mut d, 0x20 + 8, 1.0f32.to_bits());
        d[0x20 + 0x0D] = 2;
        put32(&mut d, 0x20 + 0x18, 0x80);
        put32(&mut d, 0x20 + 0x1C, 0x100);
        put32(&mut d, 0x20 + 0x20, 0x180);
        put32(&mut d, 0x20 + 0x24, 0x190);
        // Rotation keys: header u16 = time, then x, y, z (u16, 1/16384).
        let q = [0u16, 0, 0, 0, 10, 0x2000, 0, 0];
        for (i, v) in q.iter().enumerate() {
            d[0x80 + 2 * i..0x82 + 2 * i].copy_from_slice(&v.to_be_bytes());
        }
        d[0x180..0x182].copy_from_slice(&16u16.to_be_bytes());
        // Position keys for bone 1: short time 0 with a value, then time 20
        // (u16) reusing it.
        d[0x100] = 0x40;
        for (k, v) in [1.0f32, 2.0, 3.0].iter().enumerate() {
            put32(&mut d, 0x101 + 4 * k, v.to_bits());
        }
        d[0x10D] = 0x80;
        d[0x10E..0x110].copy_from_slice(&20u16.to_be_bytes());
        d[0x192..0x194].copy_from_slice(&17u16.to_be_bytes());
        d
    }

    #[test]
    fn decompresses_rotation_and_position_keys() {
        let c = Clip::parse(&synthetic(), &[]).unwrap();
        let Tracks::Keys { q, t } = &c.tracks else { panic!("keys expected") };
        assert_eq!(q[0].len(), 2);
        assert_eq!(q[0][1].frame(), 10.0);
        assert_eq!(q[0][1].q[0], 0.5);
        assert!((q[0][1].q[3] - 0.75f32.sqrt()).abs() < 1e-6);
        assert!(q[1].is_empty());
        assert_eq!(t[1], vec![TKey { time: 0, t: [1.0, 2.0, 3.0] }, TKey { time: 20, t: [1.0, 2.0, 3.0] }]);
    }

    #[test]
    fn samples_between_keys_and_holds_the_last() {
        let c = Clip::parse(&synthetic(), &[]).unwrap();
        let half = c.sample(5.0 / FPS);
        let expect = normalize([0.25, 0.0, 0.0, (1.0 + 0.75f32.sqrt()) / 2.0]);
        assert!((0..4).all(|i| (half[0].q[i] - expect[i]).abs() < 1e-6));
        let late = c.sample(1.0);
        assert_eq!(late[0].q[0], 0.5);
        assert_eq!(late[1].t, Some([1.0, 2.0, 3.0]));
        assert_eq!(late[0].weight, 1.0);
    }

    #[test]
    fn key_search_matches_retail_rules() {
        let k = [0.0, 10.0, 20.0];
        let f = |x: &f32| *x;
        assert_eq!(around(&k, -5.0, f), (0, 1));
        assert_eq!(around(&k, 10.0, f), (1, 2));
        assert_eq!(around(&k, 25.0, f), (2, 2));
        assert_eq!(around(&k[..1], 3.0, f), (0, 0));
    }

    #[test]
    fn difference_is_applied_on_the_right_and_weighted() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let base = vec![BoneSample { weight: 1.0, q: [0.0, s, 0.0, s], t: Some([1.0, 0.0, 0.0]) }];
        let diff = vec![BoneSample { weight: 1.0, q: [s, 0.0, 0.0, s], t: Some([0.0, 1.0, 0.0]) }];
        let r = apply_difference(&base, &diff);
        assert_eq!(r[0].q, qmul(base[0].q, diff[0].q));
        assert_eq!(r[0].t, Some([1.0, 1.0, 0.0]));
        let none = vec![BoneSample { weight: 0.0, ..diff[0] }];
        assert_eq!(apply_difference(&base, &none), base);
    }

    #[test]
    fn nlerp_takes_the_short_way() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 0.0, -1.0];
        assert_eq!(nlerp(a, b, 0.5), a);
    }
}
