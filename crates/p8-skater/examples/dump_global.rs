//! `cargo run -p p8-skater --example dump_global -- <DATA/COMPRESSED> <name>...` prints globals.
use p8_formats::qb_key;
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    for n in std::env::args().skip(2) {
        println!("{n} = {:?}", globals.get(&qb_key(&n)));
    }
}
