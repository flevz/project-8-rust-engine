//! Print the decompressed keys of `.ska` files (for checking against the
//! retail decompressor). Usage: anim_dump <standardkeyQ.bin.xen> <clip.ska>...
use p8_formats::anim::{Clip, Tracks};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let std_q = std::fs::read(&args[1]).expect("standardkeyQ");
    for path in &args[2..] {
        println!("file {path}");
        let d = std::fs::read(path).expect("clip");
        match Clip::parse(&d, &std_q) {
            Err(e) => println!("error {e}"),
            Ok(c) => {
                if let Tracks::Keys { q, t } = &c.tracks {
                    for (b, keys) in q.iter().enumerate() {
                        for k in keys {
                            println!("q {b} {} {} {} {} {}", k.time, k.q[0], k.q[1], k.q[2], k.q[3]);
                        }
                    }
                    for (b, keys) in t.iter().enumerate() {
                        for k in keys {
                            println!("t {b} {} {} {} {}", k.time, k.t[0], k.t[1], k.t[2]);
                        }
                    }
                }
            }
        }
    }
}
