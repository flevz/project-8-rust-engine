//! `P8_NAMES=<names.txt> cargo run -p p8-skater --example find_text -- <DATA/COMPRESSED> <substring>`:
//! lists the globals (by name) whose contents, shown with names, mention the substring, and the matching names.
use p8_formats::qb_key;
use std::collections::{BTreeMap, BTreeSet, HashMap};
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let pat = std::env::args().nth(2).expect("substring").to_lowercase();
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let mut names: HashMap<u32, String> = HashMap::new();
    if let Some(list) = std::env::var("P8_NAMES").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    let hits: BTreeSet<u32> = names.iter().filter(|(_, n)| n.to_lowercase().contains(&pat)).map(|(k, _)| *k).collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (k, v) in globals.iter() {
        let text = format!("{v:?}");
        if text.starts_with("Script") || text.starts_with("Some(Script") {
            continue;
        }
        for h in &hits {
            if text.contains(&format!("Checksum({h})")) {
                let g = names.get(k).cloned().unwrap_or(format!("{k:08x}"));
                out.entry(g).or_default().insert(names[h].clone());
            }
        }
    }
    for (g, m) in out {
        println!("{g}: {}", m.into_iter().collect::<Vec<_>>().join(", "));
    }
}
