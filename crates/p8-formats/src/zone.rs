//! One of the player's zone paks (`DATA/COMPRESSED/ZONES/z_*.pak.xen`): its
//! static collision (`.hkc`) and the restart nodes of its node array
//! (`.nqb`, e.g. `Z_Houses_NodeArray`).
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

#[derive(Debug, Default)]
pub struct Zone {
    pub collision: LevelCollision,
    pub restarts: Vec<Restart>,
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
            && let Some(Value::Array(nodes)) = qb::globals(bytes).get(&node_array)
        {
            for node in nodes {
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
