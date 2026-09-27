//! `.ske` skeletons (in `ZONES/global.pak.xen`, e.g. `Pros_Hawk_skel`,
//! named by a ped's `skeletonname`).
//!
//! Layout (big endian), read from the pro skeletons; CONFIRMED by the bind
//! pose it builds (see below) matching the skin meshes:
//!
//! - `+0x04` bone count; `+0x10` names (checksums), `+0x14` parent names
//!   (0 = root), `+0x18` mirror bone names, `+0x1C` mirror bone indices
//!   (-1 = none), `+0x20` a byte per bone (UNKNOWN), `+0x24` 64 bytes per
//!   bone (UNKNOWN, LIKELY matrices), `+0x28` local positions (x, y, z, 1),
//!   `+0x2C` local rotations (x, y, z, w quaternions).
//! - A bone's rotation relative to its parent is the **conjugate** of the
//!   stored quaternion: that reading puts the pelvis at 1.0 m, the knee at
//!   0.52 m and the ankle at 0.09 m (Y up), and every skin vertex on average
//!   0.12 m from its first bone. The stored value read as-is does not
//!   (the skeleton would lie on its back, 1.8 m from the mesh).
use std::path::Path;

use crate::{pak, qb_key};

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Name checksum.
    pub name: u32,
    /// Index of the parent bone (`None` for a root).
    pub parent: Option<usize>,
    /// Mirror bone index (left/right), used when the skater flips.
    pub mirror: Option<usize>,
    /// Position relative to the parent.
    pub position: [f32; 3],
    /// Rotation relative to the parent (x, y, z, w), already conjugated.
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("skeleton too short at {at:#x}"))
}

fn bef(b: &[u8], at: usize) -> Result<f32, String> {
    be32(b, at).map(f32::from_bits)
}

impl Skeleton {
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        let n = be32(b, 0x04)? as usize;
        if n == 0 || n > 1024 {
            return Err(format!("implausible bone count {n}"));
        }
        let (names, parents, mirrors) = (be32(b, 0x10)? as usize, be32(b, 0x14)? as usize, be32(b, 0x1C)? as usize);
        let (positions, rotations) = (be32(b, 0x28)? as usize, be32(b, 0x2C)? as usize);
        let name: Vec<u32> = (0..n).map(|i| be32(b, names + 4 * i)).collect::<Result<_, _>>()?;
        let mut bones = Vec::with_capacity(n);
        for i in 0..n {
            let parent_name = be32(b, parents + 4 * i)?;
            let parent = if parent_name == 0 { None } else { name.iter().position(|&x| x == parent_name) };
            let m = be32(b, mirrors + 4 * i)?;
            let p = positions + 16 * i;
            let r = rotations + 16 * i;
            let q = [bef(b, r)?, bef(b, r + 4)?, bef(b, r + 8)?, bef(b, r + 12)?];
            bones.push(Bone {
                name: name[i],
                parent,
                mirror: if m == u32::MAX { None } else { Some(m as usize) },
                position: [bef(b, p)?, bef(b, p + 4)?, bef(b, p + 8)?],
                rotation: [-q[0], -q[1], -q[2], q[3]],
            });
        }
        Ok(Self { bones })
    }

    /// Load the skeleton whose name checksum is `key` from an archive (e.g.
    /// `global.pak.xen`).
    pub fn load(pak_path: &Path, key: u32) -> Result<Self, String> {
        let headers = std::fs::read(pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
        let pab = std::fs::read(pak_path.to_string_lossy().replace(".pak.xen", ".pab.xen")).ok();
        let (archive, data) =
            pak::parse_file(&headers, pab.as_deref()).map_err(|e| format!("{} was not recognised: {e:?}", pak_path.display()))?;
        let ske = qb_key(".ske");
        let e = archive
            .entries
            .iter()
            .find(|e| e.type_key == ske && e.full_name_key == key)
            .ok_or_else(|| format!("no skeleton {key:08x} in {}", pak_path.display()))?;
        Self::parse(data.get(e.offset..e.offset + e.size).ok_or("skeleton entry past the archive")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two bones: a root and a child 1 m up, the child's stored rotation
    /// (x, y, z, w) = (0.6, 0, 0, 0.8) comes back conjugated.
    #[test]
    fn parses_names_parents_and_conjugates_rotations() {
        let mut b = vec![0u8; 0x200];
        let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_be_bytes());
        put(&mut b, 0x04, 2);
        for (field, off) in [(0x10, 0x100), (0x14, 0x110), (0x18, 0x120), (0x1C, 0x130), (0x28, 0x140), (0x2C, 0x160)] {
            put(&mut b, field, off);
        }
        put(&mut b, 0x100, 0xAAAA);
        put(&mut b, 0x104, 0xBBBB);
        put(&mut b, 0x110, 0);
        put(&mut b, 0x114, 0xAAAA);
        put(&mut b, 0x130, u32::MAX);
        put(&mut b, 0x134, u32::MAX);
        put(&mut b, 0x150 + 4, 1.0f32.to_bits());
        put(&mut b, 0x170, 0.6f32.to_bits());
        put(&mut b, 0x17C, 0.8f32.to_bits());
        put(&mut b, 0x16C, 1.0f32.to_bits());
        let s = Skeleton::parse(&b).unwrap();
        assert_eq!(s.bones.len(), 2);
        assert_eq!((s.bones[0].parent, s.bones[1].parent), (None, Some(0)));
        assert_eq!(s.bones[1].position, [0.0, 1.0, 0.0]);
        assert_eq!(s.bones[1].rotation, [-0.6, -0.0, -0.0, 0.8]);
        assert_eq!(s.bones[0].rotation, [-0.0, -0.0, -0.0, 1.0]);
    }
}
