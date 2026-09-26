//! `cargo run -p p8-script --example run_script -- <qb.pak.xen> <script> [frames]`
//! Runs one of the player's scripts with a host that answers every command
//! `true` and prints it, frame by frame. For checking the VM.
use p8_formats::qb::Value;
use p8_script::{Host, Params, Script, Status};
use std::collections::BTreeMap;

struct Print {
    globals: BTreeMap<u32, Value>,
    names: BTreeMap<u32, String>,
    time: f64,
    seed: u32,
}

impl Print {
    fn name(&self, k: u32) -> String {
        self.names.get(&k).cloned().unwrap_or(format!("{k:08x}"))
    }
    fn show(&self, v: &Value) -> String {
        match v {
            Value::Checksum(k) => self.name(*k),
            Value::Struct(m) => format!(
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| if *k == 0 { self.show(v) } else { format!("{}={}", self.name(*k), self.show(v)) })
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Value::Script(_) => "<script>".into(),
            v => format!("{v:?}"),
        }
    }
}

impl Host for Print {
    fn global(&self, key: u32) -> Option<Value> {
        self.globals.get(&key).cloned()
    }
    fn command(&mut self, script: &mut Script, name: u32, params: &Params) -> Option<bool> {
        println!("    [{}] {} {}", self.name(script.name), self.name(name), self.show(&params.to_struct()));
        Some(true)
    }
    fn random(&mut self, n: u32) -> u32 {
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (self.seed >> 8) % n.max(1)
    }
    fn now_ms(&self) -> f64 {
        self.time
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (_, globals) = p8_formats::qb::load_pak_globals(args[1].as_ref()).expect("scripts");
    let frames: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    // Names for printing: every word in the scripts' strings is unknown to
    // us, so names come from the command line list file if present.
    let mut names = BTreeMap::new();
    if let Ok(list) = std::fs::read_to_string("names.txt") {
        for n in list.lines() {
            names.insert(p8_formats::qb_key(n), n.to_string());
        }
    }
    let mut host = Print { globals, names, time: 0.0, seed: 1 };
    let name = p8_formats::qb_key(&args[2]);
    let mut s = Script::new(&mut host, name, &Params::new()).expect("script not found");
    for f in 0..frames {
        println!("frame {f}:");
        let st = s.update(&mut host);
        host.time += 1000.0 / 60.0;
        if st == Status::Done {
            println!("done");
            break;
        }
    }
    println!("handlers: {:?}", s.handlers.iter().map(|h| (host.name(h.event), host.name(h.script), h.exception)).collect::<Vec<_>>());
}
