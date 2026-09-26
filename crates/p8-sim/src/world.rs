//! Map-format-neutral world queries for the Project 8 controller.
//!
//! The host supplies plain triangles and rail polylines. This module knows
//! nothing about `.skate`, retail collision or any Project 8 level format, so
//! a future converted Project 8 map works without controller changes.
use glam::Vec3;
use std::collections::HashMap;

const CELL: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

#[derive(Clone, Copy, Debug)]
struct Triangle {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    normal: Vec3,
}

/// A grindable edge (a rail, ledge or coping) as a straight segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rail {
    pub start: Vec3,
    pub end: Vec3,
}

/// Nearest point on a rail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailPoint {
    pub rail: usize,
    pub point: Vec3,
    /// Parameter along the rail, from 0 at `start` to 1 at `end`.
    pub t: f32,
    pub distance: f32,
}

#[derive(Default)]
pub struct World {
    triangles: Vec<Triangle>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    rails: Vec<Rail>,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl World {
    /// Build from triangle soup. Degenerate or non-finite triangles are skipped.
    pub fn new(triangles: impl IntoIterator<Item = [Vec3; 3]>, rails: Vec<Rail>) -> Self {
        let mut world = World {
            rails,
            ..Default::default()
        };
        for [a, b, c] in triangles {
            let n = (b - a).cross(c - a);
            if !(a.is_finite() && b.is_finite() && c.is_finite()) || n.length_squared() < 1e-12 {
                continue;
            }
            let index = world.triangles.len() as u32;
            world.triangles.push(Triangle {
                a,
                b,
                c,
                normal: n.normalize(),
            });
            let min = a.min(b).min(c);
            let max = a.max(b).max(c);
            // Guard against a single stray huge triangle exploding the grid.
            if (max.x - min.x) / CELL > 4096.0 || (max.z - min.z) / CELL > 4096.0 {
                continue;
            }
            for x in cell(min.x)..=cell(max.x) {
                for z in cell(min.z)..=cell(max.z) {
                    world.cells.entry((x, z)).or_default().push(index);
                }
            }
        }
        world
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn rails(&self) -> &[Rail] {
        &self.rails
    }

    /// Closest hit along a segment `origin + direction * [0, length]`.
    /// `direction` must be normalised. Intended for short probes of a few metres.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, length: f32) -> Option<Hit> {
        let end = origin + direction * length;
        let (min, max) = (origin.min(end), origin.max(end));
        let mut candidates = Vec::new();
        for x in cell(min.x)..=cell(max.x) {
            for z in cell(min.z)..=cell(max.z) {
                candidates.extend(self.cells.get(&(x, z)).into_iter().flatten().copied());
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut best: Option<Hit> = None;
        for i in candidates {
            let t = &self.triangles[i as usize];
            if let Some(d) = intersect(origin, direction, t)
                && d <= length
                && best.is_none_or(|b| d < b.distance)
            {
                // Report the face normal that opposes the ray.
                let normal = if t.normal.dot(direction) > 0.0 {
                    -t.normal
                } else {
                    t.normal
                };
                best = Some(Hit {
                    point: origin + direction * d,
                    normal,
                    distance: d,
                });
            }
        }
        best
    }

    /// Nearest rail point within `radius` of `p`.
    pub fn nearest_rail(&self, p: Vec3, radius: f32) -> Option<RailPoint> {
        let mut best: Option<RailPoint> = None;
        for (i, r) in self.rails.iter().enumerate() {
            let d = r.end - r.start;
            let len2 = d.length_squared();
            if len2 < 1e-8 {
                continue;
            }
            let t = ((p - r.start).dot(d) / len2).clamp(0.0, 1.0);
            let point = r.start + d * t;
            let distance = point.distance(p);
            if distance <= radius && best.is_none_or(|b| distance < b.distance) {
                best = Some(RailPoint {
                    rail: i,
                    point,
                    t,
                    distance,
                });
            }
        }
        best
    }
}

/// Möller–Trumbore, double-sided.
fn intersect(origin: Vec3, dir: Vec3, t: &Triangle) -> Option<f32> {
    let e1 = t.b - t.a;
    let e2 = t.c - t.a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - t.a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d >= 0.0).then_some(d)
}

#[cfg(test)]
pub(crate) fn test_floor(size: f32) -> Vec<[Vec3; 3]> {
    let a = Vec3::new(-size, 0.0, -size);
    let b = Vec3::new(size, 0.0, -size);
    let c = Vec3::new(size, 0.0, size);
    let d = Vec3::new(-size, 0.0, size);
    vec![[a, b, c], [a, c, d]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downward_ray_hits_floor_with_up_normal() {
        let w = World::new(test_floor(20.0), vec![]);
        let hit = w
            .raycast(Vec3::new(3.0, 2.0, -7.0), Vec3::NEG_Y, 5.0)
            .unwrap();
        assert!((hit.distance - 2.0).abs() < 1e-4);
        assert!(hit.normal.y > 0.99);
    }

    #[test]
    fn ray_misses_beyond_length_and_outside_floor() {
        let w = World::new(test_floor(20.0), vec![]);
        assert!(
            w.raycast(Vec3::new(0.0, 2.0, 0.0), Vec3::NEG_Y, 1.0)
                .is_none()
        );
        assert!(
            w.raycast(Vec3::new(50.0, 2.0, 0.0), Vec3::NEG_Y, 5.0)
                .is_none()
        );
    }

    #[test]
    fn nearest_rail_projects_onto_segment() {
        let rail = Rail {
            start: Vec3::new(0.0, 1.0, 0.0),
            end: Vec3::new(0.0, 1.0, 10.0),
        };
        let w = World::new([], vec![rail]);
        let p = w.nearest_rail(Vec3::new(0.3, 1.2, 4.0), 1.0).unwrap();
        assert!((p.t - 0.4).abs() < 1e-5);
        assert!(w.nearest_rail(Vec3::new(3.0, 1.0, 4.0), 1.0).is_none());
    }
}
