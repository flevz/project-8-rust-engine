//! What the translated physics asks of the level: retail "feeler" line
//! checks (`820E5048` -> `8221BC60` -> `8221B3C0`): the closest surface on a
//! line, skipping surfaces by flag masks.
use glam::Vec3;

/// A feeler hit (physics `+352` point, `+368` normal, `+296` flags,
/// `+298` terrain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    /// Collision flags (low 16 bits of the surface's material).
    pub flags: u16,
    /// Terrain index (bits 16..23 of the material; `terrain_types` order).
    pub terrain: u8,
}

/// Retail's per-hit filter (`82201558`), with the feeler's two masks
/// (`+100` ignore_1, `+102` ignore_0): skip a surface having any
/// `ignore_1` flag; if `ignore_0` is non-zero, require one of its flags.
pub fn filter_allows(flags: u16, ignore_1: u16, ignore_0: u16) -> bool {
    if ignore_1 != 0 && flags & ignore_1 != 0 {
        return false;
    }
    ignore_0 == 0 || flags & ignore_0 != 0
}

pub trait World {
    /// The closest allowed surface on the line from `start` to `end`.
    fn feeler(&self, start: Vec3, end: Vec3, ignore_1: u16, ignore_0: u16) -> Option<Hit>;

    /// The level's rails, if it has any.
    fn rails(&self) -> Option<&crate::rails::RailManager> {
        None
    }
}

/// A level that is one infinite floor at `height` with no flags, facing up:
/// a line hits it only when going down onto it. A real (trivial) level,
/// used for testing without game files.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatFloor {
    pub height: f32,
}

impl World for FlatFloor {
    fn feeler(&self, start: Vec3, end: Vec3, ignore_1: u16, ignore_0: u16) -> Option<Hit> {
        if !filter_allows(0, ignore_1, ignore_0) {
            return None;
        }
        let (a, b) = (start.y - self.height, end.y - self.height);
        if a < 0.0 || b > 0.0 || a <= b {
            return None;
        }
        let t = a / (a - b);
        Some(Hit { point: start + (end - start) * t, normal: Vec3::Y, flags: 0, terrain: 0 })
    }
}

/// One convex level piece (Havok box, cylinder or capsule).
#[derive(Clone, Copy, Debug)]
enum Convex {
    /// Box: world-from-local rotation columns, centre and half extents.
    Box { axes: [Vec3; 3], centre: Vec3, half: Vec3 },
    Cylinder { a: Vec3, b: Vec3, r: f32 },
    Capsule { a: Vec3, b: Vec3, r: f32 },
}

#[derive(Clone, Copy, Debug)]
struct Tri {
    v0: Vec3,
    e1: Vec3,
    e2: Vec3,
    normal: Vec3,
    material: u32,
}

const CELL: f32 = 4.0;

/// Static level collision from the player's zone files
/// (`p8_formats::havok::level_collision`), answering feelers.
///
/// Ray tests are exact geometry (Havok's `castRay` on triangles, boxes,
/// cylinders and capsules). One behaviour inside Havok is not read from the
/// game: a triangle hit from behind reports the normal facing the ray start
/// (LIKELY; the game's own checks expect normals to face the ray, e.g. the
/// air update treats `normal.y < -0.01` as a ceiling).
pub struct Level {
    tris: Vec<Tri>,
    convex: Vec<(Convex, u32)>,
    grid: std::collections::HashMap<(i32, i32), Vec<u32>>,
    /// The level's rails (see [`Level::with_rails`]).
    pub rails: crate::rails::RailManager,
}

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::from_array(a)
}

fn material_hit(point: Vec3, normal: Vec3, material: u32) -> Hit {
    let (terrain, flags) = p8_formats::havok::split_material(material);
    Hit { point, normal, flags, terrain }
}

impl Level {
    pub fn new(collision: &p8_formats::havok::LevelCollision) -> Self {
        use p8_formats::havok::Solid;
        let mut tris = Vec::new();
        let mut convex = Vec::new();
        for s in &collision.solids {
            match s {
                Solid::Triangle { v: t, material } => {
                    let (v0, v1, v2) = (v(t[0]), v(t[1]), v(t[2]));
                    let (e1, e2) = (v1 - v0, v2 - v0);
                    let n = e1.cross(e2);
                    if n.length_squared() > 0.0 {
                        tris.push(Tri { v0, e1, e2, normal: n.normalize(), material: *material });
                    }
                }
                Solid::Box { transform, half, material } => {
                    let axes = transform.cols.map(v);
                    convex.push((Convex::Box { axes, centre: v(transform.t), half: v(*half) }, *material));
                }
                Solid::Cylinder { a, b, radius, material } => {
                    convex.push((Convex::Cylinder { a: v(*a), b: v(*b), r: *radius }, *material))
                }
                Solid::Capsule { a, b, radius, material } => {
                    convex.push((Convex::Capsule { a: v(*a), b: v(*b), r: *radius }, *material))
                }
            }
        }
        let mut grid: std::collections::HashMap<(i32, i32), Vec<u32>> = Default::default();
        for (i, t) in tris.iter().enumerate() {
            let pts = [t.v0, t.v0 + t.e1, t.v0 + t.e2];
            let lo = pts.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(*p));
            let hi = pts.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(*p));
            for x in (lo.x / CELL).floor() as i32..=(hi.x / CELL).floor() as i32 {
                for z in (lo.z / CELL).floor() as i32..=(hi.z / CELL).floor() as i32 {
                    grid.entry((x, z)).or_default().push(i as u32);
                }
            }
        }
        Self { tris, convex, grid, rails: Default::default() }
    }

    /// Every triangle, for drawing (corners and material).
    pub fn triangles(&self) -> impl Iterator<Item = ([Vec3; 3], u32)> + '_ {
        self.tris.iter().map(|t| ([t.v0, t.v0 + t.e1, t.v0 + t.e2], t.material))
    }
}

/// Segment `s + d*t`, t in [0,1], against a triangle (both sides).
fn ray_tri(s: Vec3, d: Vec3, t: &Tri) -> Option<(f32, Vec3)> {
    let p = d.cross(t.e2);
    let det = t.e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let q = s - t.v0;
    let u = q.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let r = q.cross(t.e1);
    let w = d.dot(r) * inv;
    if w < 0.0 || u + w > 1.0 {
        return None;
    }
    let k = t.e2.dot(r) * inv;
    if !(0.0..=1.0).contains(&k) {
        return None;
    }
    let n = if t.normal.dot(d) > 0.0 { -t.normal } else { t.normal };
    Some((k, n))
}

/// Segment against a box (slabs in the box frame); no hit from inside.
fn ray_box(s: Vec3, d: Vec3, axes: &[Vec3; 3], c: Vec3, h: Vec3) -> Option<(f32, Vec3)> {
    let ls = Vec3::new((s - c).dot(axes[0]), (s - c).dot(axes[1]), (s - c).dot(axes[2]));
    let ld = Vec3::new(d.dot(axes[0]), d.dot(axes[1]), d.dot(axes[2]));
    let (mut t0, mut t1, mut axis, mut sign) = (0.0f32, 1.0f32, usize::MAX, 0.0);
    for i in 0..3 {
        if ld[i].abs() < 1e-12 {
            if ls[i].abs() > h[i] {
                return None;
            }
            continue;
        }
        let (mut a, mut b) = ((-h[i] - ls[i]) / ld[i], (h[i] - ls[i]) / ld[i]);
        let mut sgn = -1.0;
        if a > b {
            std::mem::swap(&mut a, &mut b);
            sgn = 1.0;
        }
        if a > t0 {
            t0 = a;
            axis = i;
            sign = sgn;
        }
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    if axis == usize::MAX {
        return None; // starts inside
    }
    Some((t0, axes[axis] * sign))
}

/// Segment against the tube around a..b, clipped at the ends, plus the two
/// end discs when `caps` (a cylinder; a capsule's ends are spheres instead).
fn ray_cylinder(s: Vec3, d: Vec3, a: Vec3, b: Vec3, r: f32, caps: bool) -> Option<(f32, Vec3)> {
    let axis = b - a;
    let len2 = axis.length_squared();
    let mut best: Option<(f32, Vec3)> = None;
    let mut take = |t: f32, n: Vec3| {
        if (0.0..=1.0).contains(&t) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, n));
        }
    };
    if len2 > 0.0 {
        let u = axis / len2.sqrt();
        let m = s - a;
        let dp = d - u * d.dot(u);
        let mp = m - u * m.dot(u);
        let qa = dp.dot(dp);
        let qb = 2.0 * dp.dot(mp);
        let qc = mp.dot(mp) - r * r;
        if qc > 0.0 && qa > 0.0 {
            let disc = qb * qb - 4.0 * qa * qc;
            if disc >= 0.0 {
                let t = (-qb - disc.sqrt()) / (2.0 * qa);
                let along = (s + d * t - a).dot(u);
                if along >= 0.0 && along * along <= len2 {
                    take(t, (mp + dp * t).normalize_or_zero());
                }
            }
        }
        if caps {
            for (centre, n) in [(a, -u), (b, u)] {
                let dn = d.dot(n);
                if dn < 0.0 {
                    let t = (centre - s).dot(n) / dn;
                    if (s + d * t - centre).length_squared() <= r * r {
                        take(t, n);
                    }
                }
            }
        }
    }
    best
}

/// Segment against a sphere; no hit from inside.
fn ray_sphere(s: Vec3, d: Vec3, c: Vec3, r: f32) -> Option<(f32, Vec3)> {
    let m = s - c;
    let qa = d.dot(d);
    let qb = 2.0 * m.dot(d);
    let qc = m.dot(m) - r * r;
    if qc <= 0.0 || qa == 0.0 {
        return None;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let t = (-qb - disc.sqrt()) / (2.0 * qa);
    (0.0..=1.0).contains(&t).then(|| (t, (m + d * t).normalize_or_zero()))
}

impl Level {
    /// The level with its rails (the zone's rail nodes; `terrain` maps a
    /// terrain checksum to its index, see [`crate::Scripts::terrain_index`]).
    pub fn with_rails(mut self, nodes: &[p8_formats::zone::RailNode], terrain: &dyn Fn(u32) -> u8) -> Self {
        self.rails = crate::rails::RailManager::build(nodes, terrain);
        self
    }
}

impl World for Level {
    fn rails(&self) -> Option<&crate::rails::RailManager> {
        Some(&self.rails)
    }

    fn feeler(&self, start: Vec3, end: Vec3, ignore_1: u16, ignore_0: u16) -> Option<Hit> {
        let d = end - start;
        let mut best: Option<(f32, Vec3, u32)> = None;
        let mut consider = |t: f32, n: Vec3, m: u32| {
            if filter_allows((m & 0xFFFF) as u16, ignore_1, ignore_0) && best.is_none_or(|(bt, _, _)| t < bt) {
                best = Some((t, n, m));
            }
        };
        let (lo, hi) = (start.min(end), start.max(end));
        let mut seen = std::collections::HashSet::new();
        for x in (lo.x / CELL).floor() as i32..=(hi.x / CELL).floor() as i32 {
            for z in (lo.z / CELL).floor() as i32..=(hi.z / CELL).floor() as i32 {
                for &i in self.grid.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if seen.insert(i) {
                        let t = &self.tris[i as usize];
                        if let Some((k, n)) = ray_tri(start, d, t) {
                            consider(k, n, t.material);
                        }
                    }
                }
            }
        }
        for (c, m) in &self.convex {
            let hit = match *c {
                Convex::Box { axes, centre, half } => ray_box(start, d, &axes, centre, half),
                Convex::Cylinder { a, b, r } => ray_cylinder(start, d, a, b, r, true),
                Convex::Capsule { a, b, r } => {
                    let tube = ray_cylinder(start, d, a, b, r, false);
                    let ends = [ray_sphere(start, d, a, r), ray_sphere(start, d, b, r)];
                    [tube, ends[0], ends[1]].into_iter().flatten().min_by(|x, y| x.0.total_cmp(&y.0))
                }
            };
            if let Some((t, n)) = hit {
                consider(t, n, *m);
            }
        }
        best.map(|(t, n, m)| material_hit(start + d * t, n, m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p8_formats::havok::{LevelCollision, Solid, Transform};

    fn level() -> Level {
        let quad = |y: f32, material: u32| {
            [
                Solid::Triangle { v: [[-10.0, y, -10.0], [-10.0, y, 10.0], [10.0, y, 10.0]], material },
                Solid::Triangle { v: [[-10.0, y, -10.0], [10.0, y, 10.0], [10.0, y, -10.0]], material },
            ]
        };
        let mut solids: Vec<Solid> = quad(0.0, 0x0012_0000).into();
        solids.extend(quad(3.0, 0x0000_0010));
        solids.push(Solid::Box {
            transform: Transform { t: [5.0, 0.5, 0.0], ..Transform::IDENTITY },
            half: [0.5; 3],
            material: 0x80,
        });
        Level::new(&LevelCollision { solids, unknown: Default::default() })
    }

    #[test]
    fn closest_hit_with_normal_terrain_and_flags() {
        let l = level();
        let h = l.feeler(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 0, 0).unwrap();
        assert!(h.point.y.abs() < 1e-5 && h.normal == Vec3::Y);
        assert_eq!((h.terrain, h.flags), (0x12, 0));
        // Going up hits the underside of the upper floor, normal facing down.
        let h = l.feeler(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 5.0, 0.0), 0, 0).unwrap();
        assert!((h.point.y - 3.0).abs() < 1e-5 && h.normal == Vec3::NEG_Y);
        // ignore_1 = 0x10 skips it.
        assert!(l.feeler(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 5.0, 0.0), 0x10, 0).is_none());
    }

    #[test]
    fn boxes_report_the_face_normal() {
        let l = level();
        let h = l.feeler(Vec3::new(0.0, 0.5, 0.0), Vec3::new(10.0, 0.5, 0.0), 0, 0).unwrap();
        assert!((h.point.x - 4.5).abs() < 1e-5 && h.normal == Vec3::NEG_X && h.flags == 0x80);
    }

    #[test]
    fn ignore_0_requires_one_of_its_flags() {
        assert!(!filter_allows(0, 0, 0x80));
        assert!(filter_allows(0x81, 0, 0x80));
        assert!(!filter_allows(0x81, 0x1, 0x80));
    }
}
