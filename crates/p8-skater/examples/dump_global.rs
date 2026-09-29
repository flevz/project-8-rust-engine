//! `P8_NAMES=<names.txt> cargo run -p p8-skater --example dump_global -- <DATA/COMPRESSED> <name or 8-hex key>...`
//! prints globals, with checksums shown as names when the names file knows them.
use p8_formats::qb_key;
use std::collections::HashMap;
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("DATA/COMPRESSED folder"));
    let (_, globals) = p8_formats::qb::load_pak_globals(&root.join("PAK/qb.pak.xen")).expect("scripts");
    let mut names: HashMap<u32, String> = HashMap::new();
    if let Some(list) = std::env::var("P8_NAMES").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
        for n in list.lines() {
            names.insert(qb_key(n), n.to_string());
        }
    }
    for n in std::env::args().skip(2) {
        let key = u32::from_str_radix(&n, 16).ok().filter(|_| n.len() == 8).unwrap_or_else(|| qb_key(&n));
        let text = format!("{:?}", globals.get(&key));
        let mut out = String::new();
        let bytes: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != '.') {
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let num: String = bytes[i..j].iter().collect();
                if j < bytes.len() && bytes[j] == '.' {
                    out.push_str(&num);
                } else if let Some(name) = num.parse::<u32>().ok().and_then(|v| names.get(&v)) {
                    out.push_str(&format!("<{name}>"));
                } else {
                    out.push_str(&num);
                }
                i = j;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        println!("{n} = {out}");
    }
}
