//! `cargo run -p p8-skater --example find_ref -- <DATA/COMPRESSED> <name>...`: globals mentioning a name (as checksum or text).
use p8_formats::qb_key;
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    for n in std::env::args().skip(2) {
        let key = qb_key(&n);
        let needle = format!("Checksum({key})");
        for (k, v) in globals.iter() {
            let text = format!("{v:?}");
            if text.contains(&needle) && !text.starts_with("Script") {
                println!("{n}: global {k:08x} {}", &text[..text.len().min(300)]);
            }
        }
    }
}
