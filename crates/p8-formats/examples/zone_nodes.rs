//! `cargo run -p p8-formats --example zone_nodes -- <zone .pak.xen>`: reads
//! the zone's `.nqb` files as QB globals and prints what they contain.
use p8_formats::{pak, qb, qb_key};

fn main() {
    let path = std::env::args().nth(1).expect("zone .pak.xen");
    let headers = std::fs::read(&path).expect("read pak");
    let pab = std::fs::read(path.replace(".pak.xen", ".pab.xen")).ok();
    let (archive, data) = pak::parse_file(&headers, pab.as_deref()).expect("pak");
    for e in archive.entries.iter().filter(|e| e.type_key == qb_key(".nqb")) {
        let g = qb::globals(&data[e.offset..e.offset + e.size]);
        println!("== {:08x}: {} globals", e.full_name_key, g.len());
        for (k, v) in &g {
            let s = format!("{v:?}");
            println!("  {k:08x} {}", &s[..s.len().min(if std::env::var("FULL").is_ok() { usize::MAX } else { 600 })]);
        }
    }
}
