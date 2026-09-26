//! `p8-inspect <installed game folder>`
//!
//! Surveys a Project 8 installation and writes `p8-inspect-report.txt`: file
//! types and sizes, and whether each `.pak.xen` matches the archive layout
//! hypothesised in `p8_formats::pak`. The report contains structure only
//! (names, counts, sizes and the first 8 bytes of each archive), never game
//! content, so it is safe to share.
use p8_formats::pak::{Pak, extension_name};
use p8_formats::qb_key;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Everything after the first dot, e.g. `pak.xen`.
fn compound_extension(p: &Path) -> String {
    let name = lower_name(p);
    name.split_once('.')
        .map(|(_, e)| e.to_owned())
        .unwrap_or_else(|| "(none)".into())
}

fn magic(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(8)];
    let hex: Vec<String> = head.iter().map(|b| format!("{b:02X}")).collect();
    let ascii: String = head
        .iter()
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
        .collect();
    format!("{} |{}|", hex.join(" "), ascii)
}

/// Neversoft debug data is often plain text lines: `0x1234abcd name`.
fn harvest_names(data: &[u8], names: &mut HashMap<u32, String>) -> usize {
    let Ok(text) = std::str::from_utf8(data) else {
        return 0;
    };
    let mut added = 0;
    for line in text.lines() {
        let mut parts = line.trim().splitn(2, char::is_whitespace);
        let (Some(key), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(hex) = key.strip_prefix("0x").or_else(|| key.strip_prefix("0X")) else {
            continue;
        };
        if let Ok(k) = u32::from_str_radix(hex, 16) {
            let name = name.trim();
            if !name.is_empty() && names.insert(k, name.to_owned()).is_none() {
                added += 1;
            }
        }
    }
    added
}

fn main() {
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: p8-inspect <folder containing your installed Project 8 files>");
        std::process::exit(2);
    };
    if !root.is_dir() {
        eprintln!("Not a folder: {}", root.display());
        std::process::exit(2);
    }
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let mut report = String::new();
    let _ = writeln!(
        report,
        "p8-inspect report v1 (structure only, no game content)"
    );
    let _ = writeln!(report, "files scanned: {}", files.len());

    // 1. Overall layout by extension.
    let mut by_ext: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for f in &files {
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        let e = by_ext.entry(compound_extension(f)).or_default();
        e.0 += 1;
        e.1 += size;
    }
    let _ = writeln!(report, "\n== File types ==");
    for (ext, (count, bytes)) in &by_ext {
        let _ = writeln!(
            report,
            "{ext:<24} {count:>6} files {:>10.1} MiB",
            *bytes as f64 / 1048576.0
        );
    }

    // 2. Archives.
    let paks: Vec<_> = files
        .iter()
        .filter(|f| lower_name(f).ends_with(".pak.xen"))
        .collect();
    let _ = writeln!(report, "\n== Archives (.pak.xen): {} ==", paks.len());
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut parsed_ok = 0;
    let mut type_totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut samples = Vec::new();
    for pak_path in &paks {
        let rel = pak_path
            .strip_prefix(&root)
            .unwrap_or(pak_path)
            .display()
            .to_string();
        let Ok(headers) = std::fs::read(pak_path) else {
            let _ = writeln!(report, "{rel}: unreadable");
            continue;
        };
        let name = lower_name(pak_path);
        let pab_path = pak_path.with_file_name(name.replace(".pak.xen", ".pab.xen"));
        let pab = std::fs::read(&pab_path).ok();
        let mut data = headers.clone();
        if let Some(pab) = &pab {
            data.extend_from_slice(pab);
        }
        match Pak::parse(&headers, data.len()) {
            Ok(pak) => {
                parsed_ok += 1;
                let mut types: BTreeMap<String, usize> = BTreeMap::new();
                for e in &pak.entries {
                    let t = extension_name(e.type_key)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{:08X}", e.type_key));
                    *types.entry(t.clone()).or_default() += 1;
                    *type_totals.entry(t).or_default() += 1;
                    if let Some(bytes) = data.get(e.offset..e.offset + e.size) {
                        harvest_names(bytes, &mut names);
                    }
                }
                let summary: Vec<String> = types.iter().map(|(t, n)| format!("{t}×{n}")).collect();
                let _ = writeln!(
                    report,
                    "{rel}: OK {:?} entries={} terminated={} pab={} magic={} types: {}",
                    pak.endian,
                    pak.entries.len(),
                    pak.terminated,
                    pab.is_some(),
                    magic(&headers),
                    summary.join(", ")
                );
                if samples.len() < 40 {
                    for e in pak.entries.iter().take(3) {
                        samples.push((rel.clone(), e.clone()));
                    }
                }
            }
            Err(err) => {
                let _ = writeln!(
                    report,
                    "{rel}: NOT RECOGNISED ({err:?}) size={} magic={}",
                    headers.len(),
                    magic(&headers)
                );
            }
        }
    }
    let _ = writeln!(
        report,
        "\narchives matching the hypothesised layout: {parsed_ok}/{}",
        paks.len()
    );
    let _ = writeln!(report, "\n== Entry types across all archives ==");
    for (t, n) in &type_totals {
        let _ = writeln!(report, "{t:<12} {n}");
    }
    let _ = writeln!(report, "\nnames harvested from debug text: {}", names.len());
    let resolved = |k: u32| names.get(&k).cloned().unwrap_or_else(|| format!("{k:08X}"));
    let _ = writeln!(
        report,
        "\n== Sample entries (first 3 per archive, up to 40) =="
    );
    for (pak, e) in &samples {
        let _ = writeln!(
            report,
            "{pak}: type={} size={} name={} full={} flags={:#x}",
            extension_name(e.type_key).unwrap_or("?"),
            e.size,
            e.name.clone().unwrap_or_else(|| resolved(e.name_key)),
            resolved(e.full_name_key),
            e.flags
        );
    }
    let _ = writeln!(
        report,
        "\nself-check: qb_key(\"level\") = {:08X} (expected 651533EC)",
        qb_key("level")
    );

    print!("{report}");
    match std::fs::write("p8-inspect-report.txt", &report) {
        Ok(()) => eprintln!(
            "\nSaved p8-inspect-report.txt in {}",
            std::env::current_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        ),
        Err(e) => eprintln!("\nCould not save the report file: {e}"),
    }
}
