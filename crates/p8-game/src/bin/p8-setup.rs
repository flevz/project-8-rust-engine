//! `p8-setup <installed game folder>`: reads the player's own `qb.pak.xen`,
//! extracts the original skater physics globals and writes `tuning.json`
//! (used by the game) plus `p8-setup-report.txt`. Nothing is copied from the
//! game except these numeric settings, and they stay on the player's machine.
#[path = "../original.rs"]
mod original;

use p8_formats::qb;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Where the game finds the player's `qb.pak.xen`.
const SCRIPTS_LOCATION: &str = "scripts-location.txt";

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
    let (files, globals) = match qb::load_pak_globals(pak_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    // The game reads the scripts from here at start-up (nothing is copied).
    let location = std::fs::canonicalize(pak_path).unwrap_or_else(|_| pak_path.clone());
    std::fs::write(SCRIPTS_LOCATION, location.to_string_lossy().as_bytes())
        .expect("write scripts-location.txt");
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
