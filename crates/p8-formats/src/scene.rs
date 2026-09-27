//! `.skin.xen` models (Xbox 360 scenes), e.g.
//! `MODELS/SKATER_PRO/pro_hawk.skin.xen`, named by the `ped_body` table
//! (`desc_id` -> `mesh`). The file is one raw DEFLATE stream.
//!
//! Layout (big endian), worked out from the pro skater files and CONFIRMED
//! by the result (a drawn T-pose of the right shape; every index in range;
//! vertex blocks adding up exactly to each mesh's vertex count; normals
//! agreeing with the triangles' facing in 99.5% of cases):
//!
//! - `+0x20` u32: materials (low 16 bits = count), `+0x24` u32 section size.
//!   Material records start at `+0x30`, 0x134 bytes each: `+0` name,
//!   `+0xA8` three texture names (their role comes from the texture
//!   dictionary's type byte, not from the slot).
//! - At `+0x20 + size`: `0xBABEFACE`, u32 n, n bytes, a bounding box
//!   (2 x 16 bytes), then the geometry base: bounding sphere (16 bytes),
//!   `base + 0x2C` mesh count, `base + 0x5C` offset of the mesh table
//!   (0x80 bytes per mesh). All offsets below are from the base.
//! - Mesh record (as u32 words `w`): `w[5]` material; `w[9]` index count
//!   << 16 | vertex count; `w[16]` second stream (colour, then half-float
//!   texture coordinates; stride = `w[19]` / vertex count); `w[23]` indices
//!   (u16 triangle strips, 0x7FFF restarts); `w[24]` vertex blocks.
//! - Vertex blocks: a 16-byte header (u32 vertex count, up to four bone
//!   indices as bytes), then 32 bytes per vertex: position (3 floats),
//!   two u16 weights (the second is the block's first bone, the first the
//!   second bone, the third bone gets the rest: LIKELY, the first bone has
//!   the largest weight in 99% of vertices), normal, tangent and binormal
//!   packed 11:11:10 (x in the low bits), then 4 bytes filler (`BAADF00D`).
//!   The normal is the first of the three (CONFIRMED against the faces).
//!
//! Not read yet: the vertex format words (`w[6]`, `w[7]`), the other
//! texture coordinate sets and the vertex colour, the material passes and
//! blend modes (`+0x12C` flags), LODs.
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: u32,
    pub textures: [u32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub material: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Skeleton bone indices (up to three used).
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Triangle list, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub materials: Vec<Material>,
    pub meshes: Vec<Mesh>,
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("model too short at {at:#x}"))
}

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| format!("model too short at {at:#x}"))
}

fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let man = (h & 0x3FF) as f32;
    match exp {
        0 => sign * man * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

fn signed(x: u32, bits: u32) -> i32 {
    if x >> (bits - 1) != 0 { x as i32 - (1 << bits) } else { x as i32 }
}

/// 11:11:10 packed direction, x in the low bits.
fn unpack_111110(u: u32) -> [f32; 3] {
    [signed(u & 2047, 11) as f32 / 1023.0, signed((u >> 11) & 2047, 11) as f32 / 1023.0, signed(u >> 22, 10) as f32 / 511.0]
}

impl Scene {
    /// Parse the decompressed file.
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        let mat_count = (be32(b, 0x20)? & 0xFFFF) as usize;
        let section = be32(b, 0x24)? as usize;
        let mut materials = Vec::with_capacity(mat_count);
        for i in 0..mat_count {
            let r = 0x30 + 0x134 * i;
            materials.push(Material { name: be32(b, r)?, textures: [be32(b, r + 0xA8)?, be32(b, r + 0xAC)?, be32(b, r + 0xB0)?] });
        }
        let end = 0x20 + section;
        if be32(b, end)? != 0xBABE_FACE {
            return Err(format!("no geometry marker at {end:#x}"));
        }
        let base = end + 8 + be32(b, end + 4)? as usize + 32;
        let mesh_count = be32(b, base + 0x2C)? as usize;
        let table = base + be32(b, base + 0x5C)? as usize;
        if mesh_count > 4096 {
            return Err(format!("implausible mesh count {mesh_count}"));
        }
        let mut meshes = Vec::with_capacity(mesh_count);
        for m in 0..mesh_count {
            let rec = table + 0x80 * m;
            let w = |i: usize| be32(b, rec + 4 * i);
            let counts = w(9)?;
            let (ni, nv) = ((counts >> 16) as usize, (counts & 0xFFFF) as usize);
            let mut mesh = Mesh { material: w(5)?, ..Default::default() };
            // Indices: strips with 0x7FFF restarts.
            let ib = base + w(23)? as usize;
            let mut strip: Vec<u32> = Vec::new();
            for k in 0..ni {
                let i = be16(b, ib + 2 * k)?;
                if i == 0x7FFF {
                    strip.clear();
                    continue;
                }
                strip.push(i as u32);
                let n = strip.len();
                if n >= 3 {
                    let (mut a, mut c1, c2) = (strip[n - 3], strip[n - 2], strip[n - 1]);
                    if n.is_multiple_of(2) {
                        std::mem::swap(&mut a, &mut c1);
                    }
                    if a != c1 && c1 != c2 && a != c2 {
                        if a as usize >= nv || c1 as usize >= nv || c2 as usize >= nv {
                            return Err(format!("mesh {m}: index past {nv} vertices"));
                        }
                        mesh.indices.extend([a, c1, c2]);
                    }
                }
            }
            // Vertex blocks.
            let mut p = base + w(24)? as usize;
            while mesh.positions.len() < nv {
                let n = be32(b, p)? as usize;
                let bl = b.get(p + 4..p + 8).ok_or("model too short")?;
                let bones = [bl[0] as u16, bl[1] as u16, bl[2] as u16, bl[3] as u16];
                if n == 0 || mesh.positions.len() + n > nv {
                    return Err(format!("mesh {m}: bad vertex block of {n}"));
                }
                p += 16;
                for _ in 0..n {
                    let f = |o: usize| be32(b, p + o).map(f32::from_bits);
                    mesh.positions.push([f(0)?, f(4)?, f(8)?]);
                    let wb = be16(b, p + 14)? as f32 / 65535.0;
                    let wa = be16(b, p + 12)? as f32 / 65535.0;
                    let w2 = (1.0 - wa - wb).max(0.0);
                    mesh.joints.push([bones[0], bones[1], bones[2], 0]);
                    mesh.weights.push([wb, wa, if bones[2] != 0 { w2 } else { 0.0 }, 0.0]);
                    mesh.normals.push(unpack_111110(be32(b, p + 16)?));
                    p += 32;
                }
            }
            // Second stream: colour (4 bytes), then texture coordinate set 0.
            let s2 = base + w(16)? as usize;
            let stride = if nv > 0 { w(19)? as usize / nv } else { 0 };
            for v in 0..nv {
                let o = s2 + stride * v + 4;
                mesh.uvs.push([half(be16(b, o)?), half(be16(b, o + 2)?)]);
            }
            meshes.push(mesh);
        }
        Ok(Self { materials, meshes })
    }

    /// Read and decompress a `.skin.xen` file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&crate::pak::decompress(&raw)).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0x3800), 0.5);
        assert_eq!(half(0xC000), -2.0);
        assert_eq!(half(0x0000), 0.0);
    }

    #[test]
    fn packed_directions_are_unit_length() {
        // +X (1023 in the low 11 bits), -Y, +Z (511 in the top 10 bits).
        assert_eq!(unpack_111110(1023), [1.0, 0.0, 0.0]);
        assert_eq!(unpack_111110(1025 << 11), [0.0, -1.0, 0.0]);
        assert_eq!(unpack_111110(511 << 22), [0.0, 0.0, 1.0]);
    }
}
