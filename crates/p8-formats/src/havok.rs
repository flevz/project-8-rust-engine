//! Havok 4.0 binary packfiles (`.hkc` in the zone paks) and the static level
//! collision inside them.
//!
//! CONFIRMED on the retail houses zone: big-endian, 4-byte pointers, three
//! sections (`__classnames__`, `__data__`, `__types__`). Pointers are stored
//! as local/global fixups; each object's class comes from the virtual
//! fixups. The object layouts used below were read from the file's own
//! `hkClass` reflection data (`__types__`), e.g. `hkStorageMeshShape.storage`
//! at +80 and `hkMeshShapeSubpart` 48 bytes long.
//!
//! Every retail level body is fixed (`hkMotion.type` 7). Surface info is a
//! u32 per triangle or list child: terrain in bits 16..23 and collision
//! flags in the low 16 bits (retail line check `8221B3C0`).
use std::collections::HashMap;

#[derive(Debug)]
pub enum HavokError {
    NotPackfile,
    Truncated,
}

fn be32(d: &[u8], o: usize) -> Result<u32, HavokError> {
    Ok(u32::from_be_bytes(d.get(o..o + 4).ok_or(HavokError::Truncated)?.try_into().unwrap()))
}

fn f32_at(d: &[u8], o: usize) -> Result<f32, HavokError> {
    be32(d, o).map(f32::from_bits)
}

type Loc = (usize, u32); // (section, offset)

struct Section {
    start: usize,
}

/// A parsed packfile: sections, pointer fixups and object classes.
pub struct Packfile<'a> {
    d: &'a [u8],
    sections: Vec<Section>,
    ptr: HashMap<Loc, Loc>,
    pub objects: Vec<(Loc, String)>,
}

impl<'a> Packfile<'a> {
    pub fn parse(d: &'a [u8]) -> Result<Self, HavokError> {
        if be32(d, 0)? != 0x57E0_E057 || be32(d, 4)? != 0x10C0_C010 {
            return Err(HavokError::NotPackfile);
        }
        let n = be32(d, 0x14)? as usize;
        let mut sections = Vec::new();
        let mut heads = Vec::new();
        for i in 0..n.min(16) {
            let o = 0x40 + 0x30 * i + 20;
            let v: Vec<u32> = (0..7).map(|k| be32(d, o + 4 * k)).collect::<Result<_, _>>()?;
            sections.push(Section { start: v[0] as usize });
            heads.push(v);
        }
        let mut ptr = HashMap::new();
        let mut objects = Vec::new();
        for (si, h) in heads.iter().enumerate() {
            let b = h[0] as usize;
            let (lf, gf, vf, ex) = (h[1] as usize, h[2] as usize, h[3] as usize, h[4] as usize);
            let mut i = lf;
            while i + 8 <= gf {
                let (src, dst) = (be32(d, b + i)?, be32(d, b + i + 4)?);
                if src != u32::MAX {
                    ptr.insert((si, src), (si, dst));
                }
                i += 8;
            }
            let mut i = gf;
            while i + 12 <= vf {
                let (src, sec, dst) = (be32(d, b + i)?, be32(d, b + i + 4)?, be32(d, b + i + 8)?);
                if src != u32::MAX {
                    ptr.insert((si, src), (sec as usize, dst));
                }
                i += 12;
            }
            let mut i = vf;
            while i + 12 <= ex {
                let (src, sec, off) = (be32(d, b + i)?, be32(d, b + i + 4)?, be32(d, b + i + 8)?);
                if src != u32::MAX {
                    let s = heads.get(sec as usize).ok_or(HavokError::Truncated)?[0] as usize + off as usize;
                    let end = d[s..].iter().position(|&c| c == 0).ok_or(HavokError::Truncated)?;
                    objects.push(((si, src), String::from_utf8_lossy(&d[s..s + end]).into_owned()));
                }
                i += 12;
            }
        }
        Ok(Self { d, sections, ptr, objects })
    }

    fn abs(&self, l: Loc) -> usize {
        self.sections[l.0].start + l.1 as usize
    }
    fn u32(&self, l: Loc, off: u32) -> Result<u32, HavokError> {
        be32(self.d, self.abs((l.0, l.1 + off)))
    }
    fn f32(&self, l: Loc, off: u32) -> Result<f32, HavokError> {
        f32_at(self.d, self.abs((l.0, l.1 + off)))
    }
    fn vec3(&self, l: Loc, off: u32) -> Result<[f32; 3], HavokError> {
        Ok([self.f32(l, off)?, self.f32(l, off + 4)?, self.f32(l, off + 8)?])
    }
    fn deref(&self, l: Loc, off: u32) -> Option<Loc> {
        self.ptr.get(&(l.0, l.1 + off)).copied()
    }
    /// An `hkArray` (pointer, size, capacity) at `off`.
    fn array(&self, l: Loc, off: u32) -> Result<Option<(Loc, u32)>, HavokError> {
        let n = self.u32(l, off + 4)?;
        Ok(self.deref(l, off).map(|p| (p, n)))
    }
}

/// A rigid transform: rotation columns and translation (`hkTransform`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub cols: [[f32; 3]; 3],
    pub t: [f32; 3],
}

impl Transform {
    pub const IDENTITY: Self = Self { cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let mut r = self.t;
        for (i, ri) in r.iter_mut().enumerate() {
            *ri += self.cols[0][i] * v[0] + self.cols[1][i] * v[1] + self.cols[2][i] * v[2];
        }
        r
    }
    pub fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let mut r = [0.0; 3];
        for (i, ri) in r.iter_mut().enumerate() {
            *ri = self.cols[0][i] * v[0] + self.cols[1][i] * v[1] + self.cols[2][i] * v[2];
        }
        r
    }
    /// `self(other(v))`.
    pub fn then(&self, other: &Transform) -> Transform {
        Transform { cols: other.cols.map(|c| self.rotate(c)), t: self.apply(other.t) }
    }
}

/// One solid piece of level collision, in world space.
#[derive(Clone, Debug, PartialEq)]
pub enum Solid {
    /// Triangle (from `hkStorageMeshShape`), corners in winding order.
    Triangle { v: [[f32; 3]; 3], material: u32 },
    /// `hkBoxShape` placed by `transform`.
    Box { transform: Transform, half: [f32; 3], material: u32 },
    /// `hkCylinderShape` between two world points.
    Cylinder { a: [f32; 3], b: [f32; 3], radius: f32, material: u32 },
    /// `hkCapsuleShape` between two world points.
    Capsule { a: [f32; 3], b: [f32; 3], radius: f32, material: u32 },
}

/// All static collision from one packfile.
#[derive(Debug, Default)]
pub struct LevelCollision {
    pub solids: Vec<Solid>,
    /// Shape classes met but not understood (counts), for reporting.
    pub unknown: HashMap<String, usize>,
}

struct Walker<'a, 'b> {
    p: &'b Packfile<'a>,
    class: HashMap<Loc, &'b str>,
    out: LevelCollision,
}

impl Walker<'_, '_> {
    fn transform(&self, l: Loc, off: u32) -> Result<Transform, HavokError> {
        Ok(Transform {
            cols: [self.p.vec3(l, off)?, self.p.vec3(l, off + 16)?, self.p.vec3(l, off + 32)?],
            t: self.p.vec3(l, off + 48)?,
        })
    }

    fn shape(&mut self, s: Option<Loc>, xf: &Transform, material: u32) -> Result<(), HavokError> {
        let Some(s) = s else { return Ok(()) };
        let class = self.class.get(&s).copied().unwrap_or("");
        let p = self.p;
        match class {
            // hkBvTreeShape.child (hkSingleShapeContainer) -> childShape.
            "hkMoppBvTreeShape" => self.shape(p.deref(s, 16), xf, material)?,
            "hkListShape" => {
                if let Some((c, n)) = p.array(s, 20)? {
                    for i in 0..n {
                        let info = p.u32(c, 8 * i + 4)?;
                        self.shape(p.deref(c, 8 * i), xf, info)?;
                    }
                }
            }
            "hkConvexTransformShape" => {
                let local = self.transform(s, 32)?;
                self.shape(p.deref(s, 20), &xf.then(&local), material)?;
            }
            // childShape (hkSingleShapeContainer) at +16, translation at +32.
            "hkConvexTranslateShape" => {
                let local = Transform { t: p.vec3(s, 32)?, ..Transform::IDENTITY };
                self.shape(p.deref(s, 20), &xf.then(&local), material)?;
            }
            "hkStorageMeshShape" => self.mesh(s, xf)?,
            "hkBoxShape" => {
                let half = p.vec3(s, 16)?;
                self.out.solids.push(Solid::Box { transform: *xf, half, material });
            }
            "hkCylinderShape" => {
                let (a, b) = (p.vec3(s, 32)?, p.vec3(s, 48)?);
                let radius = p.f32(s, 16)?;
                self.out.solids.push(Solid::Cylinder { a: xf.apply(a), b: xf.apply(b), radius, material });
            }
            "hkCapsuleShape" => {
                let (a, b) = (p.vec3(s, 16)?, p.vec3(s, 32)?);
                let radius = p.f32(s, 12)?;
                self.out.solids.push(Solid::Capsule { a: xf.apply(a), b: xf.apply(b), radius, material });
            }
            // hkSphereShape: its convex radius (+12) about the local origin;
            // stored as a capsule of zero length.
            "hkSphereShape" => {
                let c = xf.apply([0.0; 3]);
                let radius = p.f32(s, 12)?;
                self.out.solids.push(Solid::Capsule { a: c, b: c, radius, material });
            }
            other => *self.out.unknown.entry(other.to_string()).or_default() += 1,
        }
        Ok(())
    }

    /// `hkStorageMeshShape`: subparts (48 bytes at +52) with their storage
    /// (+80): vertices (+8, floats), indices16 (+20), indices32 (+32),
    /// materialIndices (+44, u8), materials (+56, u32),
    /// materialIndices16 (+68).
    fn mesh(&mut self, s: Loc, xf: &Transform) -> Result<(), HavokError> {
        let p = self.p;
        let scale = p.vec3(s, 32)?;
        let (Some((subs, nsub)), Some((stores, _))) = (p.array(s, 52)?, p.array(s, 80)?) else {
            return Ok(());
        };
        for i in 0..nsub {
            let sub = (subs.0, subs.1 + 48 * i);
            let vstride = p.u32(sub, 4)?;
            let nv = p.u32(sub, 8)?;
            let wide = p.d[p.abs(sub) + 16] != 1; // stridingType: 1 = 16-bit
            let mat16 = p.d[p.abs(sub) + 17] == 2; // materialIndexStridingType
            let istride = p.u32(sub, 20)?;
            let ntri = p.u32(sub, 24)?;
            let mstride = p.u32(sub, 32)?; // materialIndexStriding
            let Some(store) = p.deref(stores, 4 * i) else { continue };
            let Some((verts, _)) = p.array(store, 8)? else { continue };
            let idx = p.array(store, if wide { 32 } else { 20 })?;
            let mats = p.array(store, 56)?;
            let midx = p.array(store, if mat16 { 68 } else { 44 })?;
            let Some((idx, _)) = idx else { continue };
            let vertex = |k: u32| -> Result<[f32; 3], HavokError> {
                let v = p.vec3(verts, vstride * k)?;
                Ok(xf.apply([v[0] * scale[0], v[1] * scale[1], v[2] * scale[2]]))
            };
            for t in 0..ntri {
                let o = p.abs((idx.0, idx.1 + istride * t));
                let k: [u32; 3] = if wide {
                    [be32(p.d, o)?, be32(p.d, o + 4)?, be32(p.d, o + 8)?]
                } else {
                    let h = |j: usize| u16::from_be_bytes([p.d[o + j], p.d[o + j + 1]]) as u32;
                    [h(0), h(2), h(4)]
                };
                if k.iter().any(|&x| x >= nv) {
                    continue;
                }
                let material = match (mats, midx) {
                    (Some((m, nm)), Some((mi, _))) if nm > 0 => {
                        let o = p.abs((mi.0, mi.1 + mstride * t));
                        let j = if mat16 { u16::from_be_bytes([p.d[o], p.d[o + 1]]) as u32 } else { p.d[o] as u32 };
                        if j < nm { p.u32(m, 4 * j)? } else { 0 }
                    }
                    _ => 0,
                };
                let v = [vertex(k[0])?, vertex(k[1])?, vertex(k[2])?];
                self.out.solids.push(Solid::Triangle { v, material });
            }
        }
        Ok(())
    }
}

/// Every fixed rigid body's collision, placed in the world.
/// `hkRigidBody`: collidable shape at +28, motion at +160 whose motion
/// state transform is at +16.
pub fn level_collision(d: &[u8]) -> Result<LevelCollision, HavokError> {
    let p = Packfile::parse(d)?;
    let class: HashMap<Loc, &str> = p.objects.iter().map(|(l, c)| (*l, c.as_str())).collect();
    let bodies: Vec<Loc> = p.objects.iter().filter(|(_, c)| c == "hkRigidBody").map(|(l, _)| *l).collect();
    let mut w = Walker { p: &p, class, out: LevelCollision::default() };
    for b in bodies {
        let xf = w.transform(b, 160 + 16)?;
        let shape = w.p.deref(b, 28);
        w.shape(shape, &xf, 0)?;
    }
    Ok(w.out)
}

/// Terrain index (bits 16..23) and collision flags (low 16 bits).
pub fn split_material(material: u32) -> (u8, u16) {
    (((material >> 16) & 0xFF) as u8, (material & 0xFFFF) as u16)
}
