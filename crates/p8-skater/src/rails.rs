//! The level's rails (retail rail manager): built from the zone's RailNode,
//! ClimbingNode and ManualNode nodes (`82197138`, `821939F8`) and searched
//! for the rail crossed by a move (`821968F8`).
//!
//! Only what normal levels use is translated: the park editor's state
//! (`[82731B2C]` `+64`) is "OFF" outside the park editor, which skips the
//! rail clearance feelers (`82194D20`) and the created-park rail sets.
use glam::Vec3;
use p8_formats::qb_key;
use p8_formats::zone::RailNode;

/// Record flags (`+64`).
pub mod flag {
    pub const LIP_OVERRIDE: u32 = 0x1;
    pub const DEFAULT_LINE: u32 = 0x2;
    /// Created at start (`822E2140`); only active records are searched.
    pub const ACTIVE: u32 = 0x4;
    pub const NO_CLIMBING: u32 = 0x8;
    pub const CLIMBING: u32 = 0x10;
    pub const LADDER: u32 = 0x20;
    pub const HANG_LEFT: u32 = 0x40;
    pub const HANG_RIGHT: u32 = 0x80;
    pub const MANUAL: u32 = 0x800;
}

/// One 96-byte retail record.
#[derive(Clone, Debug, PartialEq)]
pub struct Rail {
    /// `+0`, `+16`: bounds of the node and, once linked, its next node.
    pub min: Vec3,
    pub max: Vec3,
    /// `+32`.
    pub pos: Vec3,
    /// `+64`.
    pub flags: u32,
    /// `+68`: index in the node array.
    pub node: u32,
    /// `+72`: terrain index (rails only).
    pub terrain: u8,
    /// `+80`, `+84`: the linked records.
    pub next: Option<usize>,
    pub prev: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct RailManager {
    pub rails: Vec<Rail>,
}

/// Retail `821930C0`: the bounds of two points.
fn bounds(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    (a.min(b), a.max(b))
}

impl RailManager {
    /// Retail `82197138` for a normal level (single player: nodes flagged
    /// AbsentInNetGames are kept). `terrain` maps a `TerrainType` checksum
    /// to its terrain index (retail `8228E528`).
    pub fn build(nodes: &[RailNode], terrain: &dyn Fn(u32) -> u8) -> Self {
        let mut rails: Vec<Rail> = nodes.iter().map(|n| Self::record(n, terrain)).collect();
        // Sorted by node index.
        rails.sort_by_key(|r| r.node);
        let by_node: std::collections::HashMap<u32, usize> = rails.iter().enumerate().map(|(i, r)| (r.node, i)).collect();
        let mut nodes_by_index: Vec<&RailNode> = nodes.iter().collect();
        nodes_by_index.sort_by_key(|n| n.index);
        for (i, n) in nodes_by_index.iter().enumerate() {
            for link in &n.links {
                // Retail assumes every link is one of these records.
                let Some(&l) = by_node.get(link) else { continue };
                if rails[i].next.is_none() || rails[l].flags & flag::DEFAULT_LINE != 0 {
                    rails[i].next = Some(l);
                    let (min, max) = bounds(rails[i].pos, rails[l].pos);
                    rails[i].min = min;
                    rails[i].max = max;
                }
                if rails[l].prev.is_none() || rails[i].flags & flag::DEFAULT_LINE != 0 {
                    rails[l].prev = Some(i);
                }
            }
        }
        Self { rails }
    }

    /// Retail `821939F8`: one record from its node.
    fn record(n: &RailNode, terrain: &dyn Fn(u32) -> u8) -> Rail {
        let k = qb_key;
        let has = |f: &str| n.flags.contains(&k(f));
        let climbing = n.class == k("ClimbingNode");
        let manual = n.class == k("ManualNode");
        let mut flags = 0;
        // CreatedAtStart; nodes created from level variables or the time of
        // day (CreatedFromVariable, createdfromtod) are not handled.
        if has("CreatedAtStart") {
            flags |= flag::ACTIVE;
        }
        if has("LipOverride") {
            flags |= flag::LIP_OVERRIDE;
        }
        if has("DefaultLine") {
            flags |= flag::DEFAULT_LINE;
        }
        if climbing {
            flags |= flag::CLIMBING;
            if n.kind == Some(k("Ladder")) {
                flags |= flag::LADDER;
            }
        } else if manual {
            flags |= flag::MANUAL;
        }
        if has("hangleft") {
            flags |= flag::HANG_LEFT;
        }
        if has("HangRight") {
            flags |= flag::HANG_RIGHT;
        }
        if has("NoClimbing") {
            flags |= flag::NO_CLIMBING;
        }
        let pos = Vec3::from(n.pos);
        Rail {
            min: pos,
            max: pos,
            pos,
            flags,
            node: n.index,
            terrain: if climbing || manual { 0 } else { n.terrain.map(terrain).unwrap_or(0) },
            next: None,
            prev: None,
        }
    }

    /// Retail `821968F8`: the rail the move from `old` to `new` passes
    /// closest to, within `snap` (`Rail_Max_Snap`), skipping `ignore`
    /// (82196AE8). `corner_cos` is the angle limit (`f1`): unless it is 1
    /// (`820FAAA8` passes 1.0), a segment more across the move than that
    /// (|flat cos| < `corner_cos`) is refused (82196F20..82196F40). Returns
    /// the record and the point on it.
    ///
    /// Not translated: the preference for rails on the same collision
    /// object (`r10`, 82196EE0..82196F10: a rail whose `82194940` differs
    /// has its score doubled; the rail-end search `820F5DC0` passes it, our
    /// level has no collision objects), so with two rails at nearly equal
    /// distance the other one may win.
    pub fn search(&self, old: Vec3, new: Vec3, snap: f32, ignore: Option<usize>, corner_cos: f32) -> Option<(usize, Vec3)> {
        // 82196954..821969E4: the move's bounds grown by the snap.
        let (mut lo, mut hi) = bounds(old, new);
        lo -= Vec3::splat(snap);
        hi += Vec3::splat(snap);
        let overlaps = |r: &Rail| {
            !(lo.x > r.max.x || hi.x < r.min.x || lo.z > r.max.z || hi.z < r.min.z || lo.y > r.max.y || hi.y < r.min.y)
        };
        // 1e7 (82001F28), 1.122 (82004EE0), 2 (82000D78), 1e-5 (82000C18).
        let mut best = 1e7f32;
        let mut found: Option<(usize, Vec3)> = None;
        // 82196A98..82197104: every record; 82196ACC..82196AF8 the flags
        // (climbing, manual, `ignore`, active).
        for (i, r) in self.rails.iter().enumerate() {
            if r.flags & (flag::CLIMBING | flag::MANUAL) != 0 || r.flags & flag::ACTIVE == 0 || Some(i) == ignore {
                continue;
            }
            // 82196AFC..82196B04: a segment, else 82196F74 (a lone node).
            if let Some(next) = r.next {
                // 82196B14..82196B64.
                if !overlaps(r) {
                    continue;
                }
                let start = r.pos;
                let rail = self.rails[next].pos - start;
                let to_old = old - start;
                let tiny = |v: Vec3| v.x.abs() < 1e-5 && v.y.abs() < 1e-5 && v.z.abs() < 1e-5;
                // 82196BB4..82196C64: zero-length rail or move.
                if tiny(rail) {
                    continue;
                }
                let mv = new - old;
                if tiny(mv) {
                    continue;
                }
                let b = rail.dot(mv);
                let a = rail.dot(rail);
                let e = mv.dot(mv);
                let c = to_old.dot(rail);
                let f = to_old.dot(mv);
                let denom = e * a - b * b;
                // 82196D68.
                if denom.abs() < 1e-5 {
                    continue;
                }
                // 82196CBC..82196D1C: both clamped to 0..1.
                let t = ((b * c - a * f) / denom).clamp(0.0, 1.0);
                let s = ((b * t + c) / a).clamp(0.0, 1.0);
                let on_move = old + mv * t;
                let on_rail = start + rail * s;
                let d = (on_rail - on_move).length();
                let flat_rail = Vec3::new(rail.x, 0.0, rail.z).normalize_or_zero();
                let flat_move = Vec3::new(mv.x, 0.0, mv.z);
                let mut k = flat_rail.dot(flat_move.normalize_or_zero()).abs();
                // 82196EB8..82196ED0.
                if flat_move.x == 0.0 && flat_move.z == 0.0 && k == 0.0 {
                    k = 1.0;
                }
                let score = (1.122 - k) * d;
                let reach = (2.0 - k) * d;
                // 82196F14..82196F18.
                if score >= best {
                    continue;
                }
                // 1 at 82000C14.
                if corner_cos != 1.0 && k.abs() < corner_cos {
                    continue;
                }
                // 82196F44..82196F70.
                best = score;
                found = if reach > snap { None } else { Some((i, on_rail)) };
            } else if r.prev.is_none() {
                // 82196F74..82196FDC.
                if !overlaps(r) {
                    continue;
                }
                let mv = new - old;
                let dir = mv.normalize_or_zero();
                let to = r.pos - old;
                let along = to.dot(dir);
                let d = (to - dir * along).length();
                // 821970B8..821970C4.
                if d > snap {
                    continue;
                }
                if best < d * 2.0 {
                    continue;
                }
                best = d * 2.0;
                found = Some((i, r.pos));
            }
        }
        // 82197108..82197124: the point and record only when found.
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(index: u32, pos: [f32; 3], links: Vec<u32>) -> RailNode {
        RailNode {
            index,
            class: qb_key("RailNode"),
            pos,
            links,
            kind: None,
            terrain: None,
            flags: vec![qb_key("CreatedAtStart")],
        }
    }

    #[test]
    fn links_become_segments_and_the_crossed_one_is_found() {
        let m = RailManager::build(
            &[node(7, [0.0, 1.0, 0.0], vec![9]), node(9, [10.0, 1.0, 0.0], vec![])],
            &|_| 0,
        );
        assert_eq!(m.rails[0].next, Some(1));
        assert_eq!(m.rails[1].prev, Some(0));
        assert_eq!(m.rails[0].max, Vec3::new(10.0, 1.0, 0.0));
        // Falling across it at x = 4.
        let (i, p) = m.search(Vec3::new(4.0, 1.3, -0.2), Vec3::new(4.0, 1.1, 0.2), 1.0, None, 1.0).unwrap();
        assert_eq!(i, 0);
        assert!((p - Vec3::new(4.0, 1.0, 0.0)).length() < 1e-4);
        // Too far away.
        assert!(m.search(Vec3::new(4.0, 5.0, -0.2), Vec3::new(4.0, 5.0, 0.2), 1.0, None, 1.0).is_none());
    }
}
