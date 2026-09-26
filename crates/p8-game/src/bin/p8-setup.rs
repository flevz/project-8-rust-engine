//! `p8-setup <installed game folder>`: reads the player's own `qb.pak.xen`,
//! extracts the original skater physics globals and writes `tuning.json`
//! (used by the game) plus `p8-setup-report.txt`. Nothing is copied from the
//! game except these numeric settings, and they stay on the player's machine.
#[path = "../original.rs"]
mod original;

use p8_formats::{pak, qb, qb_key};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn find(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            find(&p, name, out);
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        {
            out.push(p);
        }
    }
}

fn main() {
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: p8-setup <folder containing your installed Project 8 files>");
        std::process::exit(2);
    };
    let mut hits = Vec::new();
    find(&root, "qb.pak.xen", &mut hits);
    let Some(pak_path) = hits.first() else {
        eprintln!("Could not find qb.pak.xen under {}", root.display());
        std::process::exit(1);
    };
    let headers = std::fs::read(pak_path).expect("read qb.pak.xen");
    let pab = std::fs::read(pak_path.with_file_name("qb.pab.xen")).ok();
    let (archive, data) = match pak::parse_file(&headers, pab.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("qb.pak.xen was not recognised: {e:?}");
            std::process::exit(1);
        }
    };
    let mut globals: BTreeMap<u32, qb::Value> = BTreeMap::new();
    let mut files = 0;
    for entry in archive
        .entries
        .iter()
        .filter(|e| e.type_key == qb_key(".qb"))
    {
        if let Some(bytes) = data.get(entry.offset..entry.offset + entry.size) {
            files += 1;
            globals.extend(qb::globals(bytes));
        }
    }
    let mapped = match original::map(&globals) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Could not map the skater physics: {e}");
            std::process::exit(1);
        }
    };
    let json = serde_json::to_string_pretty(&mapped.tuning).unwrap();
    std::fs::write("tuning.json", json).expect("write tuning.json");
    let mut report = String::new();
    let _ = writeln!(
        report,
        "p8-setup: {files} scripts, {} globals from {}",
        globals.len(),
        pak_path.display()
    );
    let _ = writeln!(report, "Original Project 8 values written to tuning.json:");
    for (field, source, value, confidence) in &mapped.report {
        let _ = writeln!(
            report,
            "  {field:<20} = {value:<10.4} from {source} (use: {confidence})"
        );
    }
    let _ = writeln!(
        report,
        "Other fields keep temporary values until the original code is translated."
    );
    std::fs::write("p8-setup-report.txt", &report).expect("write report");
    print!("{report}");
}
