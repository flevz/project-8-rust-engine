//! `cargo run -p p8-script --example check_scripts -- <qb.pak.xen>`
//! Walks every script in the player's scripts token by token (retail
//! token lengths) and reports any that do not end cleanly.
use p8_formats::qb::Value;
use p8_script::code::{self, byte};

fn main() {
    let path = std::env::args().nth(1).expect("qb.pak.xen");
    let (_, globals) = p8_formats::qb::load_pak_globals(path.as_ref()).expect("scripts");
    let (mut ok, mut bad) = (0, 0);
    for (k, v) in &globals {
        let Value::Script(c) = v else { continue };
        let mut p = 0;
        let mut steps = 0;
        while p < c.len() && steps < 1_000_000 {
            let n = code::skip(c, p);
            if n == p {
                break;
            }
            p = n;
            steps += 1;
        }
        let tail_clean = c[p.min(c.len())..].iter().all(|&b| b == 0);
        if p >= c.len() || tail_clean {
            ok += 1;
        } else {
            bad += 1;
            if bad <= 10 {
                println!("{k:08x}: stopped at {p} of {} on token {:02x}", c.len(), byte(c, p));
            }
        }
    }
    println!("{ok} scripts walk cleanly, {bad} do not");
}
