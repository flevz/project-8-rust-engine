//! `cargo run -p p8-formats --example node_classes -- <ZONES dir> [zone]`
//! Counts the zone's NodeArray nodes by Class and prints one of each.
use p8_formats::qb::{self, Value};
use p8_formats::{pak, qb_key};
use std::collections::BTreeMap;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("ZONES dir"));
    let name = std::env::args().nth(2).unwrap_or("z_houses".into());
    let headers = std::fs::read(dir.join(format!("{name}.pak.xen"))).expect("pak");
    let pab = std::fs::read(dir.join(format!("{name}.pab.xen"))).ok();
    let (archive, data) = pak::parse_file(&headers, pab.as_deref()).expect("parse");
    let key = qb_key(&format!("{name}_NodeArray"));
    for e in &archive.entries {
        if e.type_key != qb_key(".nqb") {
            continue;
        }
        let globals = qb::globals(&data[e.offset..e.offset + e.size]);
        let Some(Value::Array(nodes)) = globals.get(&key).cloned() else {
            continue;
        };
        // Unnamed checksums naming a global struct (the compressed-node
        // templates) contribute that struct's fields.
        let nodes: Vec<Value> = nodes
            .iter()
            .map(|n| {
                let Value::Struct(fields) = n else { return n.clone() };
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
            })
            .collect();
        let mut by: BTreeMap<u32, (usize, String)> = BTreeMap::new();
        for n in &nodes {
            let c = match n.get_named("Class") {
                Some(Value::Checksum(c)) => *c,
                _ => 0,
            };
            let entry = by.entry(c).or_insert((0, format!("{n:?}")));
            entry.0 += 1;
        }
        println!("{} nodes", nodes.len());
        for (c, (count, sample)) in by {
            println!("class {c:08x}: {count}\n  {}", &sample[..sample.len().min(600)]);
        }
    }
}
