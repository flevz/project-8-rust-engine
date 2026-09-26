//! `cargo run -p p8-formats --example level_collision -- <zone .pak.xen>`
//! Lists the static collision in a zone (from the player's own files).
use p8_formats::{havok, pak, qb_key};

fn main() {
    let path = std::env::args().nth(1).expect("zone .pak.xen");
    let headers = std::fs::read(&path).expect("read pak");
    let pab = std::fs::read(path.replace(".pak.xen", ".pab.xen")).ok();
    let (archive, data) = pak::parse_file(&headers, pab.as_deref()).expect("pak");
    for e in archive.entries.iter().filter(|e| e.type_key == qb_key(".hkc")) {
        let bytes = &data[e.offset..e.offset + e.size];
        match havok::level_collision(bytes) {
            Ok(c) => {
                let mut kinds = std::collections::BTreeMap::new();
                for s in &c.solids {
                    let k = match s {
                        havok::Solid::Triangle { .. } => "triangle",
                        havok::Solid::Box { .. } => "box",
                        havok::Solid::Cylinder { .. } => "cylinder",
                        havok::Solid::Capsule { .. } => "capsule",
                    };
                    *kinds.entry(k).or_insert(0) += 1;
                }
                println!("{:08x}: {} bytes, {:?}, unknown {:?}", e.full_name_key, e.size, kinds, c.unknown);
            }
            Err(err) => println!("{:08x}: {err:?}", e.full_name_key),
        }
    }
}
