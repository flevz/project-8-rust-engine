//! `cargo run -p p8-formats --example pak_list -- <.pak.xen>`: lists the
//! files in one of the player's paks (type, name key and size).
use p8_formats::pak;

fn main() {
    let path = std::env::args().nth(1).expect(".pak.xen");
    let headers = std::fs::read(&path).expect("read pak");
    let pab = std::fs::read(path.replace(".pak.xen", ".pab.xen")).ok();
    let (archive, _) = pak::parse_file(&headers, pab.as_deref()).expect("pak");
    for e in &archive.entries {
        let t = pak::extension_name(e.type_key).map(str::to_string).unwrap_or(format!("{:08x}", e.type_key));
        println!("{t:8} {:08x} {:9}", e.full_name_key, e.size);
    }
}
