//! One of the player's zone paks (`DATA/COMPRESSED/ZONES/z_*.pak.xen`): its
//! static collision (`.hkc`) and the restart and rail nodes of its node
//! array (`.nqb`, e.g. `Z_Houses_NodeArray`).
//!
//! Most nodes are stored compressed: an unnamed checksum names a global
//! struct (e.g. `Z_Houses_NodeArray_compressed_node_0`) whose fields the
//! node has too (retail struct lookups follow such references, LIKELY).
use crate::havok::{self, LevelCollision};
use crate::qb::{self, Value};
use crate::{pak, qb_key};
use std::path::Path;

/// A node with `Class = Restart`.
#[derive(Clone, Debug, PartialEq)]
pub struct Restart {
    /// Name key (e.g. `qb_key("Z_Houses_TRG_Restart_Default")`).
    pub name: u32,
    pub pos: [f32; 3],
    /// Node `Angles` (radians). Retail `822E0A88` builds the orientation as
    /// identity, then rotations about X, Y and Z by these, in that order.
    pub angles: [f32; 3],
    /// The node's `Type` (e.g. `Player1`, `MultiPlayer`).
    pub kind: Option<u32>,
}

/// A node the retail rail manager collects (`82197138`): `Class` RailNode,
/// ClimbingNode or ManualNode.
#[derive(Clone, Debug, PartialEq)]
pub struct RailNode {
    /// Position in the node array (what `links` refer to).
    pub index: u32,
    pub class: u32,
    pub pos: [f32; 3],
    pub links: Vec<u32>,
    /// `Type`.
    pub kind: Option<u32>,
    /// `TerrainType` (a terrain checksum).
    pub terrain: Option<u32>,
    /// The unnamed checksums (flags such as CreatedAtStart, LipOverride,
    /// DefaultLine, hangleft, HangRight, NoClimbing, AbsentInNetGames).
    pub flags: Vec<u32>,
}

#[derive(Debug, Default)]
pub struct Zone {
    pub collision: LevelCollision,
    pub restarts: Vec<Restart>,
    pub rails: Vec<RailNode>,
}

/// A node with its compressed-node templates spliced in.
fn expand(node: &Value, globals: &std::collections::BTreeMap<u32, Value>) -> Value {
    let Value::Struct(fields) = node else { return node.clone() };
    let mut out = Vec::new();
    for (k, v) in fields {
        match (k, v) {
            (0, Value::Checksum(c)) if matches!(globals.get(c), Some(Value::Struct(_))) => {
                if let Some(Value::Struct(t)) = globals.get(c) {
                    out.extend(t.iter().cloned());
                }
            }
            _ => out.push((*k, v.clone())),
        }
    }
    Value::Struct(out)
}

fn vector(v: Option<&Value>) -> Option<[f32; 3]> {
    match v? {
        Value::Vector(a) => Some(*a),
        _ => None,
    }
}

/// Read zone `name` (e.g. `"z_houses"`) from `zones_dir`.
pub fn load(zones_dir: &Path, name: &str) -> Result<Zone, String> {
    let path = zones_dir.join(format!("{name}.pak.xen"));
    let headers = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pab = std::fs::read(zones_dir.join(format!("{name}.pab.xen"))).ok();
    let (archive, data) =
        pak::parse_file(&headers, pab.as_deref()).map_err(|e| format!("{} was not recognised: {e:?}", path.display()))?;
    let mut zone = Zone::default();
    let node_array = qb_key(&format!("{name}_NodeArray"));
    for e in &archive.entries {
        let Some(bytes) = data.get(e.offset..e.offset + e.size) else {
            continue;
        };
        if e.type_key == qb_key(".hkc") {
            let c = havok::level_collision(bytes).map_err(|err| format!("{}: collision {err:?}", path.display()))?;
            zone.collision.solids.extend(c.solids);
            for (k, n) in c.unknown {
                *zone.collision.unknown.entry(k).or_default() += n;
            }
        } else if e.type_key == qb_key(".nqb")
            && let globals = qb::globals(bytes)
            && let Some(Value::Array(nodes)) = globals.get(&node_array)
        {
            let rail_classes = [qb_key("RailNode"), qb_key("ClimbingNode"), qb_key("ManualNode")];
            for (index, node) in nodes.iter().enumerate() {
                let node = &expand(node, &globals);
                if let Some(Value::Checksum(class)) = node.get_named("Class")
                    && rail_classes.contains(class)
                {
                    let checksum = |name: &str| match node.get_named(name) {
                        Some(Value::Checksum(c)) => Some(*c),
                        _ => None,
                    };
                    let links = match node.get_named("links") {
                        Some(Value::Array(l)) => l
                            .iter()
                            .filter_map(|v| match v {
                                Value::Int(i) => Some(*i as u32),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    let flags = match node {
                        Value::Struct(f) => f
                            .iter()
                            .filter_map(|(k, v)| match (k, v) {
                                (0, Value::Checksum(c)) => Some(*c),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    zone.rails.push(RailNode {
                        index: index as u32,
                        class: *class,
                        pos: vector(node.get_named("Pos")).unwrap_or([0.0; 3]),
                        links,
                        kind: checksum("Type"),
                        terrain: checksum("TerrainType"),
                        flags,
                    });
                }
                if node.get_named("Class") != Some(&Value::Checksum(qb_key("Restart"))) {
                    continue;
                }
                let (Some(Value::Checksum(name)), Some(pos)) = (node.get_named("Name"), vector(node.get_named("Pos")))
                else {
                    continue;
                };
                let kind = match node.get_named("Type") {
                    Some(Value::Checksum(k)) => Some(*k),
                    _ => None,
                };
                let angles = vector(node.get_named("Angles")).unwrap_or([0.0; 3]);
                zone.restarts.push(Restart { name: *name, pos, angles, kind });
            }
        }
    }
    Ok(zone)
}
