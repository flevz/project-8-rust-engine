//! The skater's trick component: button events, the trick lists' triggers,
//! the trick queue and the commands that run queued tricks. Translated from
//! retail (addresses per function; research notes section 28).
//!
//! How it works in retail: each frame (`821247F0`) the pad's buttons become
//! press / release events in a 100-event ring (`8211EF08`). Each trick list
//! the scripts set (`SetQueueTricks`, e.g. `airtricks`) pairs a button
//! pattern (`trigger`) with a trick; a list entry whose trigger matches the
//! recent events is queued (`82123540`). The state scripts call
//! `DoNextTrick` (e.g. every frame in `Airborne`), which runs the oldest
//! queued trick that has not expired: its `scr` with its `params`, or the
//! trick its `trickslot` names through the skater's trick mapping
//! (`HawkTricks`: `Air_SquareL` -> `Trick_Kickflip`).
//!
//! Times: retail stamps events with the real-time clock (`823976E8`) and the
//! pending manual / grind tricks with the game clock (`8222A6C0`); both are
//! the game's milliseconds here (the same while the game runs unpaused at
//! normal speed).
use crate::input::InputState;
use p8_formats::qb::Value;
use p8_formats::qb_key;
use p8_script::Params;
use std::collections::VecDeque;

/// Button names by id (table `826E08C8`; `82229A28` / `82229A40`).
const BUTTONS: [&str; 40] = [
    "",
    "up",
    "down",
    "left",
    "right",
    "upleft",
    "upright",
    "downleft",
    "downright",
    "circle",
    "square",
    "x",
    "triangle",
    "l1",
    "l2",
    "l3",
    "r1",
    "r2",
    "r3",
    "rightup",
    "rightdown",
    "rightleft",
    "rightright",
    "rightupleft",
    "rightupright",
    "rightdownleft",
    "rightdownright",
    "leftup",
    "leftdown",
    "leftleft",
    "leftright",
    "leftupleft",
    "leftupright",
    "leftdownleft",
    "leftdownright",
    "black",
    "white",
    "z",
    "tiltup",
    "tiltdown",
];

/// `82229A40`: a button's id from its name (0 = none).
pub fn button_id(name: u32) -> usize {
    (1..BUTTONS.len()).find(|&i| qb_key(BUTTONS[i]) == name).unwrap_or(0)
}

/// `8211E080`: one of the eight d-pad directions.
fn is_direction(name: u32) -> bool {
    (1..=8).any(|i| qb_key(BUTTONS[i]) == name)
}

/// One button event (24 bytes at `+56`, `8211E268`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonEvent {
    /// `+0`: milliseconds.
    pub time: u32,
    /// `+4`: 1 press, 2 release.
    pub kind: u32,
    /// `+8`: the button's name.
    pub button: u32,
    /// `+16`: marked by the trigger being tested.
    temp: bool,
    /// `+20`: which consumers have used it (bits).
    pub used: u32,
}

/// A queued trick (20 bytes at `+2740`, `8211E908`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Queued {
    /// `+0`: the list's name.
    pub list: u32,
    /// `+4`: the entry's index in it.
    pub index: usize,
    /// `+8`: when it was queued.
    pub time: u32,
    /// `+12`: how long it stays valid (ms).
    pub duration: u32,
    /// `+16`: from the special list.
    pub special: bool,
}

/// A pending manual or grind trick (`+4844..` / `+4916..`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    pub list: u32,
    pub index: usize,
    pub time: u32,
    /// `None` = never expires (-1).
    pub duration: Option<u32>,
    pub special: bool,
}

/// What running a trick asks of the script host.
#[derive(Clone, Debug, PartialEq)]
pub struct RunTrick {
    /// The script to go to (`scr`).
    pub script: u32,
    /// Its parameters (`params`, plus the flag and extra parameters).
    pub params: Params,
    /// `scripttorunfirst` and its parameters (run before the goto).
    pub first: Option<(u32, Params)>,
}

/// The trick component's state (offsets are the retail component's).
#[derive(Clone, Debug)]
pub struct Tricks {
    /// `+2664`: held flags by button id.
    pub held: [bool; 48],
    /// `+2472`: presses ignored until this time (0 = not blocked).
    pub blocked_until: [f32; 48],
    /// `+2712` / `+2716`: buttons that make no events (the running
    /// balance's two buttons, `820CE8D8`).
    pub ignore: Vec<u32>,
    /// `+56`: the event ring; `+2458` head, `+2456` count.
    pub events: Vec<ButtonEvent>,
    pub head: usize,
    pub count: usize,
    /// `+2460..+2468`: the values of the last matched trigger.
    pub last: [u32; 3],
    /// `+2740`: the queue (`+2732` head, `+2736` tail; 100 entries).
    pub queue: VecDeque<Queued>,
    /// `+4744..` / `+4740`: the lists `SetQueueTricks` set.
    pub lists: Vec<u32>,
    /// `+4784`: `special` list.
    pub special_list: u32,
    /// `+4788`: consumer flags (`UseGrindEvents` sets 8).
    pub flags: u32,
    /// `+4796..` / `+4792`, `+4836`: manual lists and special list.
    pub manual_lists: Vec<u32>,
    pub manual_special: u32,
    /// `+4840..`: the pending manual trick.
    pub manual_pending: Option<Pending>,
    /// `+4868..` / `+4864`, `+4908`: grind extra lists, special.
    pub grind_lists: Vec<u32>,
    pub grind_special: u32,
    /// `+4912..`: the pending grind trick.
    pub grind_pending: Option<Pending>,
    /// `+4936`: extra tricks on; `+4937`: without a time limit.
    pub extra_on: bool,
    pub extra_forever: bool,
    /// `+4940` duration (ms), `+4944` start.
    pub extra_duration: u32,
    pub extra_start: u32,
    /// `+4952..` / `+4948`: extra lists; `+4992..`: per list, entries to
    /// skip (bits); `+5032` / `+5036`: special extra list and its bits.
    pub extra_lists: Vec<u32>,
    pub extra_skip: Vec<u32>,
    pub extra_special: u32,
    pub extra_special_skip: u32,
    /// `+2728`: the trick mapping (slot -> trick), includes expanded.
    pub mapping: Vec<(u32, Value)>,
    /// `+2724`: the last run trick came from a special list.
    pub last_special: bool,
    /// `+5042` / `+5144`: `SetTrickName` / `SetTrickScore` (for the score
    /// display, not translated).
    pub trick_name: String,
    pub trick_score: i32,
    /// `+7428`: the right stick is centred (`8211F480`).
    pub right_stick_centred: bool,
}

impl Default for Tricks {
    /// `82120970` (the component's reset).
    fn default() -> Self {
        Self {
            held: [false; 48],
            blocked_until: [0.0; 48],
            ignore: Vec::new(),
            events: vec![ButtonEvent::default(); 100],
            head: 0,
            count: 0,
            last: [0; 3],
            queue: VecDeque::new(),
            lists: Vec::new(),
            special_list: 0,
            flags: 0,
            manual_lists: Vec::new(),
            manual_special: 0,
            manual_pending: None,
            grind_lists: Vec::new(),
            grind_special: 0,
            grind_pending: None,
            extra_on: false,
            extra_forever: false,
            extra_duration: 0,
            extra_start: 0,
            extra_lists: Vec::new(),
            extra_skip: Vec::new(),
            extra_special: 0,
            extra_special_skip: 0,
            mapping: Vec::new(),
            last_special: false,
            trick_name: String::new(),
            trick_score: 0,
            right_stick_centred: true,
        }
    }
}

/// Global lookups the component needs.
pub trait Globals {
    fn global(&self, key: u32) -> Option<&Value>;
}

impl Globals for std::collections::BTreeMap<u32, Value> {
    fn global(&self, key: u32) -> Option<&Value> {
        self.get(&key)
    }
}

/// The items of a struct with unnamed references to global structs
/// expanded in place (`82211BE0`); a later item replaces an earlier one of
/// the same name.
pub fn expand_struct(v: &Value, g: &dyn Globals) -> Vec<(u32, Value)> {
    fn go(v: &Value, g: &dyn Globals, out: &mut Vec<(u32, Value)>, depth: u32) {
        let Value::Struct(items) = v else { return };
        for (k, x) in items {
            if *k == 0
                && depth < 8
                && let Value::Checksum(c) = x
                && let Some(s @ Value::Struct(_)) = g.global(*c)
            {
                go(s, g, out, depth + 1);
                continue;
            }
            if *k != 0 {
                out.retain(|(n, _)| n != k);
            }
            out.push((*k, x.clone()));
        }
    }
    let mut out = Vec::new();
    go(v, g, &mut out, 0);
    out
}

fn member<'a>(items: &'a [(u32, Value)], name: &str) -> Option<&'a Value> {
    let k = qb_key(name);
    items.iter().rev().find(|(n, _)| *n == k).map(|(_, v)| v)
}

fn struct_member<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    match v {
        Value::Struct(items) => member(items, name),
        _ => None,
    }
}

fn checksum_of(v: Option<&Value>) -> u32 {
    match v {
        Some(Value::Checksum(c)) => *c,
        _ => 0,
    }
}

impl Tricks {
    /// Build the mapping (`82120A88`): the profile's special tricks
    /// (slot -> trick name), then its trick mapping (set from
    /// `default_trick_mapping`, `821A2978`).
    pub fn set_mapping(&mut self, profile: Option<&Value>, g: &dyn Globals) {
        let mut out = Vec::new();
        if let Some(Value::Array(specials)) = profile.and_then(|p| struct_member(p, "specials")).and_then(|s| match s {
            Value::Struct(items) => items.first().map(|(_, v)| v),
            v @ Value::Array(_) => Some(v),
            _ => None,
        }) {
            for s in specials.iter().take(12) {
                let slot = checksum_of(struct_member(s, "trickslot"));
                let name = checksum_of(struct_member(s, "trickname"));
                if slot != 0 {
                    out.push((slot, Value::Checksum(name)));
                }
            }
        }
        let default = checksum_of(profile.and_then(|p| struct_member(p, "default_trick_mapping")));
        if let Some(m) = g.global(default) {
            for (k, v) in expand_struct(m, g) {
                out.retain(|(n, _)| *n != k || k == 0);
                out.push((k, v));
            }
        }
        self.mapping = out;
    }

    fn now_f(now: u32) -> f32 {
        now as f32
    }

    /// `8211E268`: a new event (the ring's head moves on first).
    fn push_event(&mut self, kind: u32, button: u32, now: u32) {
        self.head = (self.head + 1) % 100;
        if self.count < 100 {
            self.count += 1;
        }
        self.events[self.head] = ButtonEvent { time: now, kind, button, temp: false, used: 0 };
    }

    /// Event indices, newest first.
    fn order(&self) -> Vec<usize> {
        (0..self.count).map(|n| (self.head + 100 - n) % 100).collect()
    }

    /// `8211E7D8`: a button's state this frame.
    fn set_button(&mut self, id: usize, down: bool, now: u32) {
        if self.blocked_until[id] != 0.0 {
            if down && Self::now_f(now) <= self.blocked_until[id] {
                return;
            }
            self.blocked_until[id] = 0.0;
        }
        if self.held[id] == down {
            return;
        }
        self.held[id] = down;
        let name = qb_key(BUTTONS[id]);
        if self.ignore.contains(&name) {
            return;
        }
        self.push_event(if down { 1 } else { 2 }, name, now);
    }

    /// `8211EF08`: the buttons from the Input component's mask (`82259ED8`
    /// sets a bit per non-zero pressure byte, which is what the records'
    /// held flags are). The d-pad resolves to one of eight directions
    /// (`822D6798`, table `826E3B00`: up and down -> down, left and right
    /// -> right). L3, R3, black, white, Z and the sticks' own directions are
    /// not in our pad records (never held here).
    pub fn read_buttons(&mut self, i: &InputState, now: u32) {
        let dir = match (i.up && !i.down, i.down, i.left && !i.right, i.right) {
            (_, true, false, false) => 2,
            (_, true, true, false) => 7,
            (_, true, _, true) => 8,
            (true, false, false, false) => 1,
            (true, false, true, false) => 5,
            (true, false, _, true) => 6,
            (false, false, true, false) => 3,
            (false, false, _, true) => 4,
            _ => 0,
        };
        for d in 1..=8 {
            self.set_button(d, dir == d, now);
        }
        // Order as retail calls them: 9, 10, 12, 11, 13, 14, 15, 16, 17, 18,
        // 35, 36, 37, then the stick directions 19..34.
        for (id, down) in [
            (9, i.circle),
            (10, i.kick),
            (12, i.triangle),
            (11, i.crouch),
            (13, i.l1),
            (14, i.l2),
            (15, false),
            (16, i.r1),
            (17, i.r2),
            (18, false),
            (35, false),
            (36, false),
            (37, false),
        ] {
            self.set_button(id, down, now);
        }
        for id in 19..=34 {
            self.set_button(id, false, now);
        }
    }

    fn clear_temp(&mut self) {
        for e in &mut self.events {
            e.temp = false;
        }
    }

    /// `8211E3A8`: temp marks become used bits.
    fn commit(&mut self, mask: u32) {
        for idx in self.order() {
            let e = &mut self.events[idx];
            if e.temp {
                e.temp = false;
                e.used |= mask;
            }
        }
    }

    /// `8211E748`: `n` positional values after the type, then the time (an
    /// integer, else a global integer's name).
    fn trigger_values(&self, items: &[(u32, Value)], n: usize, g: &dyn Globals) -> ([u32; 3], u32) {
        let specials = [(0x0374_9393u32, 1usize), (0x7473_A305, 2), (0x9A7D_C229, 0)];
        let mut out = [0u32; 3];
        for (slot, (_, v)) in items.iter().skip(1).take(n).enumerate() {
            let mut c = checksum_of(Some(v));
            if let Some(&(_, i)) = specials.iter().find(|(s, _)| *s == c) {
                c = self.last[i];
            }
            if slot < 3 {
                out[slot] = c;
            }
        }
        let time = match items.get(1 + n).map(|(_, v)| v) {
            Some(Value::Int(t)) => *t as u32,
            Some(Value::Float(f)) => *f as u32,
            Some(Value::Checksum(c)) => g.global(*c).and_then(Value::as_f32).map_or(0, |f| f as u32),
            _ => 0,
        };
        (out, time)
    }

    fn done(&mut self, ok: bool) -> bool {
        self.clear_temp();
        ok
    }

    /// `82120B80`: does this trigger match the recent events? `use_mask`
    /// is OR'd into the events it uses, events with `used & ignore` are
    /// skipped.
    pub fn trigger(&mut self, trigger: Option<&Value>, use_mask: u32, ignore: u32, now: u32, g: &dyn Globals) -> bool {
        let Some(Value::Struct(items)) = trigger else { return self.done(false) };
        let Some((_, Value::Checksum(ty))) = items.first() else { return self.done(false) };
        let ty = *ty;
        let k = qb_key;
        let usable = |e: &ButtonEvent| e.used & ignore == 0;
        let order = self.order();
        let old = |e: &ButtonEvent, t: u32| now.wrapping_sub(e.time) >= t;
        let success = |me: &mut Self, vals: [u32; 3]| {
            me.commit(use_mask);
            me.last = vals;
            me.clear_temp();
            true
        };
        if ty == k("AirTrickLogic") {
            let (v, t) = self.trigger_values(items, 2, g);
            let (button, dir) = (v[0], v[1]);
            let ok = if self.held[button_id(dir)] {
                if let Some(&i) = order.iter().find(|&&i| self.events[i].kind == 1 && self.events[i].button == dir) {
                    self.events[i].temp = true;
                }
                self.pressed_within(button, t, ignore, now)
            } else {
                self.both_pressed(button, dir, t, ignore, now)
            };
            return if ok { success(self, v) } else { self.done(false) };
        }
        if ty == k("Press") || ty == k("release") {
            let (v, t) = self.trigger_values(items, 1, g);
            let kind = if ty == k("Press") { 1 } else { 2 };
            for &i in &order {
                let e = self.events[i];
                if old(&e, t) {
                    return self.done(false);
                }
                if usable(&e) && e.kind == kind && e.button == v[0] {
                    self.events[i].used |= use_mask;
                    self.last = v;
                    return self.done(true);
                }
            }
            return self.done(false);
        }
        if ty == k("ExtraGrabTrickLogic") {
            let mut dir = 0;
            for &b in &self.last {
                if is_direction(b) {
                    dir = b;
                }
            }
            let (v, t) = self.trigger_values(items, 1, g);
            let mut found = None;
            for &i in &order {
                let e = self.events[i];
                if old(&e, t) {
                    break;
                }
                if usable(&e) && e.kind == 1 {
                    if e.button == v[0] {
                        found = Some(i);
                    } else if is_direction(e.button) {
                        if e.button != dir {
                            return self.done(false);
                        }
                        self.events[i].temp = true;
                    }
                }
            }
            let Some(i) = found else { return self.done(false) };
            self.events[i].used |= use_mask;
            return success(self, v);
        }
        if ty == k("PressTwoAnyOrder") {
            let (v, t) = self.trigger_values(items, 2, g);
            let (mut a_down, mut b_down) = (false, false);
            for &i in &order {
                let e = self.events[i];
                if old(&e, t) {
                    return self.done(false);
                }
                if !usable(&e) {
                    continue;
                }
                for (which, b) in [(0, v[0]), (1, v[1])] {
                    if e.button != b {
                        continue;
                    }
                    self.events[i].temp = true;
                    if e.kind == 2 {
                        a_down = false;
                        b_down = false;
                    } else if e.kind == 1 {
                        let other = if which == 0 { b_down } else { a_down };
                        if other {
                            return success(self, v);
                        }
                        if which == 0 {
                            a_down = true;
                        } else {
                            b_down = true;
                        }
                    }
                }
            }
            return self.done(false);
        }
        // The state machines: (event kind, value index) per step, newest
        // event first; the last step completes the trigger.
        let (n, steps): (usize, &[&[(u32, usize)]]) = if ty == k("InOrder") {
            (2, &[&[(1, 1)], &[(1, 0)]])
        } else if ty == k("Tap") {
            (1, &[&[(2, 0)], &[(1, 0)]])
        } else if ty == k("PressAndRelease") {
            (2, &[&[(2, 1)], &[(1, 0)]])
        } else if ty == k("TapTwiceRelease") {
            (2, &[&[(2, 1)], &[(1, 0)], &[(1, 0)]])
        } else if ty == k("TripleInOrderSloppy") {
            (3, &[&[(1, 2)], &[(1, 1)], &[(1, 0)]])
        } else if ty == k("TripleInOrder") {
            (3, &[])
        } else {
            return self.done(false);
        };
        let (v, t) = self.trigger_values(items, n, g);
        if ty == k("TripleInOrder") {
            // States 0..4 (82121FB4): press c -> 1 or press b -> 2; 1: press
            // b -> 3; 2: press c -> 3; 3: release a -> 4; 4: press a.
            let mut s = 0;
            for &i in &order {
                let e = self.events[i];
                if old(&e, t) {
                    return self.done(false);
                }
                if !usable(&e) {
                    continue;
                }
                let next = match (s, e.kind) {
                    (0, 1) if e.button == v[2] => Some(1),
                    (0, 1) if e.button == v[1] => Some(2),
                    (1, 1) if e.button == v[1] => Some(3),
                    (2, 1) if e.button == v[2] => Some(3),
                    (3, 2) if e.button == v[0] => Some(4),
                    (4, 1) if e.button == v[0] => Some(5),
                    _ => None,
                };
                if let Some(ns) = next {
                    self.events[i].temp = true;
                    if ns == 5 {
                        return success(self, v);
                    }
                    s = ns;
                }
            }
            return self.done(false);
        }
        let mut s = 0;
        for &i in &order {
            let e = self.events[i];
            if old(&e, t) {
                return self.done(false);
            }
            if !usable(&e) {
                continue;
            }
            if steps[s].iter().any(|&(kind, vi)| e.kind == kind && e.button == v[vi]) {
                self.events[i].temp = true;
                s += 1;
                if s == steps.len() {
                    if ty == k("Tap") {
                        // 1F84: used, but the values are not kept.
                        self.commit(use_mask);
                        return self.done(true);
                    }
                    return success(self, v);
                }
            }
        }
        self.done(false)
    }

    /// `8211E4E8`: `button` pressed within `t` ms (and not used by
    /// `ignore`); marks it.
    fn pressed_within(&mut self, button: u32, t: u32, ignore: u32, now: u32) -> bool {
        for i in self.order() {
            let e = self.events[i];
            if now.wrapping_sub(e.time) >= t {
                return false;
            }
            if e.used & ignore == 0 && e.kind == 1 && e.button == button {
                self.events[i].temp = true;
                return true;
            }
        }
        false
    }

    /// `8211E5E8`: `a` and `b` both pressed within `t` ms in either order,
    /// with no other press between them; marks them.
    fn both_pressed(&mut self, a: u32, b: u32, t: u32, ignore: u32, now: u32) -> bool {
        let (mut got_a, mut got_b, mut any) = (false, false, false);
        for i in self.order() {
            let e = self.events[i];
            if now.wrapping_sub(e.time) >= t {
                return false;
            }
            if e.used & ignore != 0 || e.kind != 1 {
                continue;
            }
            if e.button == a {
                self.events[i].temp = true;
                if got_b {
                    return true;
                }
                got_a = true;
                any = true;
            } else if e.button == b {
                self.events[i].temp = true;
                if got_a {
                    return true;
                }
                got_b = true;
                any = true;
            } else if any {
                return false;
            }
        }
        false
    }

    /// `8211EA68`: an entry with a script, or a trick slot the mapping
    /// fills.
    fn valid(&self, entry: &Value, g: &dyn Globals) -> bool {
        if struct_member(entry, "scr").is_some() || struct_member(entry, "scripts").is_some() {
            return true;
        }
        if matches!(struct_member(entry, "template"), Some(Value::Struct(_))) {
            return true;
        }
        let slot = checksum_of(struct_member(entry, "trickslot"));
        if slot == 0 {
            return false;
        }
        match self.mapping.iter().rev().find(|(n, _)| *n == slot).map(|(_, v)| v) {
            Some(Value::Checksum(c)) => g.global(*c).is_some(),
            Some(Value::Int(_)) => true,
            _ => false,
        }
    }

    /// `8211E908`: add to the queue (dropped when full).
    fn enqueue(&mut self, q: Queued) {
        if self.queue.len() < 100 {
            self.queue.push_back(q);
        }
    }

    /// `821233A8`: queue the entries of `list` whose trigger matches.
    fn match_list(&mut self, list: u32, special: bool, now: u32, g: &dyn Globals) {
        let Some(Value::Array(entries)) = g.global(list).cloned() else { return };
        let ignore = !self.flags & !4;
        for (index, entry) in entries.iter().enumerate() {
            if !self.valid(entry, g) {
                continue;
            }
            let hit = self.trigger(struct_member(entry, "trigger"), 1, ignore, now, g)
                || self.trigger(struct_member(entry, "alt_trigger"), 1, ignore, now, g);
            if !hit {
                continue;
            }
            let duration = match struct_member(entry, "durationqueued") {
                Some(v) => v.as_f32().unwrap_or(0.0) as u32,
                None => g.global(qb_key("defaulttrickdurationqueued")).and_then(Value::as_f32).unwrap_or(0.0) as u32,
            };
            self.enqueue(Queued { list, index, time: now, duration, special });
        }
    }

    /// The start of a frame (`821247F0`): the buttons (`8211EF08`), then
    /// the running balance's two buttons become the ignore list for the
    /// next frame (`820CE8D8`). The extra tricks come next (see
    /// [`Tricks::extra_active`]), then [`Tricks::update_lists`].
    pub fn update(&mut self, input: &InputState, balance_buttons: Option<[u32; 2]>, now: u32) {
        self.read_buttons(input, now);
        self.ignore = balance_buttons.map(|b| b.to_vec()).unwrap_or_default();
    }

    /// The rest of `821247F0` after the extra tricks: `82123540` (queue
    /// lists), `82123A70` + `8211EBC8` (manual), `82124598` + `8211EC68`
    /// (grind extras), `8211F480`. `special` as in [`Tricks::update`].
    pub fn update_lists(&mut self, special: bool, now: u32, game_ms: u32, g: &dyn Globals) {
        if self.special_list != 0 && special {
            self.match_list(self.special_list, true, now, g);
        }
        for list in self.lists.clone() {
            self.match_list(list, false, now, g);
        }
        // 82123A70: one pending manual trick; after a special list's match
        // the normal lists are not tried.
        let mut skip = false;
        if self.manual_special != 0 && special {
            self.match_pending(self.manual_special, true, 4, !4, false, game_ms, g);
            skip = self.manual_pending.is_some();
        }
        if !skip {
            for list in self.manual_lists.clone() {
                self.match_pending(list, false, 4, u32::MAX, false, game_ms, g);
            }
        }
        // 8211EBC8.
        if let Some(p) = self.manual_pending
            && let Some(d) = p.duration
            && game_ms.wrapping_sub(p.time) > d
        {
            self.manual_pending = None;
        }
        // 82124598 / 82124428: grind extras (every list, the last match
        // wins).
        if self.grind_special != 0 && special {
            self.match_pending(self.grind_special, true, 8, !4, true, now, g);
        }
        for list in self.grind_lists.clone() {
            self.match_pending(list, false, 8, !4, true, now, g);
        }
        // 8211EC68.
        if let Some(p) = self.grind_pending
            && let Some(d) = p.duration
            && now.wrapping_sub(p.time) > d
        {
            self.grind_pending = None;
        }
        // 8211F480: no right-stick records here, so it is centred.
        self.right_stick_centred = true;
    }

    /// `821238F0` (manual) / `82124428` (grind): each matching entry
    /// becomes the pending trick (`duration` in ms, -1 = none).
    #[allow(clippy::too_many_arguments)]
    fn match_pending(&mut self, list: u32, special: bool, use_mask: u32, ignore: u32, grind: bool, now: u32, g: &dyn Globals) {
        let Some(Value::Array(entries)) = g.global(list).cloned() else { return };
        for (index, entry) in entries.iter().enumerate() {
            if !self.valid(entry, g) {
                continue;
            }
            let hit = self.trigger(struct_member(entry, "trigger"), use_mask, ignore, now, g)
                || self.trigger(struct_member(entry, "alt_trigger"), use_mask, ignore, now, g);
            if !hit {
                continue;
            }
            let duration = struct_member(entry, "duration").and_then(Value::as_f32).map(|d| d as u32);
            let p = Some(Pending { list, index, time: now, duration, special });
            if grind {
                self.grind_pending = p;
            } else {
                self.manual_pending = p;
            }
        }
    }

    /// `82123CF0`: whether extra tricks are on this frame (they expire
    /// `+4940` ms after `+4944` unless set without a duration).
    pub fn extra_active(&mut self, now: u32) -> bool {
        if !self.extra_on {
            return false;
        }
        if !self.extra_forever && now.wrapping_sub(self.extra_start) > self.extra_duration {
            self.extra_on = false;
            return false;
        }
        true
    }

    /// The extra lists to try in order (`82123CF0`): the special one (when
    /// the special meter is on) then the others, each with its skip bits.
    pub fn extra_lists_to_try(&self, special: bool) -> Vec<(u32, u32, bool)> {
        let mut out = Vec::new();
        if self.extra_special != 0 && special {
            out.push((self.extra_special, self.extra_special_skip, true));
        }
        for (n, &list) in self.extra_lists.iter().enumerate() {
            out.push((list, self.extra_skip.get(n).copied().unwrap_or(0), false));
        }
        out
    }

    /// `82123B00` for one entry: not skipped, valid, and its trigger (or
    /// `alt_trigger`) matches (use bit 2, ignoring events used by bits
    /// other than 4).
    pub fn extra_hit(&mut self, entry: &Value, index: usize, skip: u32, now: u32, g: &dyn Globals) -> bool {
        if index < 32 && skip & (1 << index) != 0 {
            return false;
        }
        if !self.valid(entry, g) {
            return false;
        }
        self.trigger(struct_member(entry, "trigger"), 2, !4, now, g) || self.trigger(struct_member(entry, "alt_trigger"), 2, !4, now, g)
    }

    /// The trick struct an entry stands for: itself when it has a script,
    /// else its `trickslot` through the mapping (`821230D8`, `3220`).
    pub fn resolve<'a>(&'a self, entry: &'a Value, g: &'a dyn Globals) -> Option<Value> {
        if struct_member(entry, "scr").is_some() {
            return Some(entry.clone());
        }
        let slot = checksum_of(struct_member(entry, "trickslot"));
        match self.mapping.iter().rev().find(|(n, _)| *n == slot).map(|(_, v)| v) {
            Some(Value::Checksum(c)) => g.global(*c).cloned(),
            _ => None,
        }
    }

    /// `82122E10`: what running a trick struct does: go to its `scr` with
    /// its `params` (plus `flag` as an unnamed flag and `extra` params),
    /// clearing the pending manual and grind tricks and the flags. `None`
    /// when there is no script (the `grind` script also needs a rail
    /// candidate, not checked here).
    pub fn run(&mut self, trick: &Value, flag: Option<u32>, extra: Option<&Params>) -> Option<RunTrick> {
        let script = checksum_of(struct_member(trick, "scr"));
        if script == 0 {
            return None;
        }
        self.manual_pending = None;
        self.grind_pending = None;
        self.flags = 0;
        let mut params = match struct_member(trick, "params") {
            Some(Value::Struct(items)) => Params(items.clone()),
            _ => Params::default(),
        };
        if let Some(f) = flag {
            params.0.push((0, Value::Checksum(f)));
        }
        if let Some(e) = extra {
            params.0.extend(e.0.iter().cloned());
        }
        Some(RunTrick { script, params, first: None })
    }

    /// `DoNextTrick` (`82124960` -> `821230D8`): the oldest queued trick that
    /// has not expired, as a trick struct to run. `last_special` is set from
    /// it.
    pub fn next_queued(&mut self, now: u32, g: &dyn Globals) -> Option<Value> {
        self.last_special = false;
        while let Some(q) = self.queue.pop_front() {
            self.last_special = q.special;
            let expired = now.wrapping_sub(q.time) > q.duration;
            if expired || q.list == 0 {
                continue;
            }
            let Some(Value::Array(entries)) = g.global(q.list) else { continue };
            let Some(entry) = entries.get(q.index) else { continue };
            return self.resolve(entry, g);
        }
        None
    }

    /// `DoNextManualTrick` (`82124AF8` -> `821235C0`): the pending manual
    /// trick, if any, as a trick struct (cleared).
    pub fn next_manual(&mut self, g: &dyn Globals) -> Option<Value> {
        self.last_special = false;
        let p = self.manual_pending.take()?;
        self.last_special = p.special;
        let Some(Value::Array(entries)) = g.global(p.list) else { return None };
        let entry = entries.get(p.index)?;
        self.resolve(entry, g)
    }

    /// `SetQueueTricks` (`8211F898`).
    pub fn set_queue_tricks(&mut self, params: &Params, g: &dyn Globals) {
        self.lists = params.0.iter().filter_map(|(k, v)| if *k == 0 { Some(checksum_of(Some(v))) } else { None }).collect();
        if self.lists.len() == 1 && g.global(self.lists[0]).is_none() {
            self.lists[0] = qb_key("defaultairtricks");
        }
        self.special_list = params.checksum(qb_key("special")).unwrap_or(0);
    }

    /// `SetManualTricks` (`8211FA20`).
    pub fn set_manual_tricks(&mut self, params: &Params) {
        self.manual_lists = params.0.iter().filter_map(|(k, v)| if *k == 0 { Some(checksum_of(Some(v))) } else { None }).collect();
        self.manual_special = params.checksum(qb_key("special")).unwrap_or(0);
    }

    /// `SetExtraTricks` (`82124B80` -> `82123DF8`). `duration` is in frames
    /// (x 16.6667 ms, `82003459`).
    pub fn set_extra_tricks(&mut self, params: &Params, now: u32, g: &dyn Globals) {
        let k = qb_key;
        self.extra_on = true;
        self.extra_forever = true;
        if let Some(d) = params.float(k("duration")) {
            self.extra_forever = false;
            self.extra_start = now;
            self.extra_duration = (d * 16.666_666) as u32;
        }
        self.extra_lists = match params.checksum(k("tricks")) {
            Some(t) => vec![t],
            None => params.0.iter().filter_map(|(n, v)| if *n == 0 { Some(checksum_of(Some(v))) } else { None }).collect(),
        };
        self.extra_special = params.checksum(k("special")).unwrap_or(0);
        self.extra_skip = vec![0; self.extra_lists.len()];
        self.extra_special_skip = 0;
        // `ignore` (a name or an array of names): 8211EE80 -> 8211EDF8.
        let ignore: Vec<u32> = match params.get(k("ignore")) {
            Some(Value::Array(a)) => a.iter().filter_map(|v| if let Value::Checksum(c) = v { Some(*c) } else { None }).collect(),
            Some(Value::Checksum(c)) => vec![*c],
            _ => Vec::new(),
        };
        for name in ignore {
            for n in 0..self.extra_lists.len() {
                let bits = self.skip_bits(self.extra_lists[n], name, g);
                self.extra_skip[n] |= bits;
            }
            if self.extra_special != 0 {
                self.extra_special_skip |= self.skip_bits(self.extra_special, name, g);
            }
        }
    }

    /// `8211EDF8` (not read in full): entries of `list` whose trick is
    /// named `name` (INFERRED: compares the entry's resolved `params.name`).
    fn skip_bits(&self, list: u32, name: u32, g: &dyn Globals) -> u32 {
        let Some(Value::Array(entries)) = g.global(list) else { return 0 };
        let mut bits = 0;
        for (i, e) in entries.iter().enumerate().take(32) {
            let Some(t) = self.resolve(e, g) else { continue };
            let n = struct_member(&t, "params").and_then(|p| struct_member(p, "name"));
            let hit = match n {
                Some(Value::String(s)) => qb_key(s) == name,
                Some(Value::Checksum(c)) => *c == name,
                _ => false,
            };
            if hit {
                bits |= 1 << i;
            }
        }
        bits
    }

    /// `ClearEventBuffer` (`8211FC20`): with `buttons`, their events older
    /// than `olderthan` ms are marked used by everyone (`8211E438`); else
    /// the buffer is emptied.
    pub fn clear_event_buffer(&mut self, params: &Params, now: u32, g: &dyn Globals) {
        let k = qb_key;
        let buttons: Option<Vec<u32>> = match params.get(k("buttons")) {
            Some(Value::Array(a)) => Some(a.iter().filter_map(|v| if let Value::Checksum(c) = v { Some(*c) } else { None }).collect()),
            Some(Value::Checksum(c)) => match g.global(*c) {
                Some(Value::Array(a)) => Some(a.iter().filter_map(|v| if let Value::Checksum(c) = v { Some(*c) } else { None }).collect()),
                _ => None,
            },
            _ => None,
        };
        match buttons {
            Some(list) => {
                let older = match params.get(k("olderthan")) {
                    Some(Value::Int(n)) => *n as u32,
                    Some(Value::Float(f)) => *f as u32,
                    Some(Value::Checksum(c)) => g.global(*c).and_then(Value::as_f32).map_or(0, |f| f as u32),
                    _ => 0,
                };
                for b in list {
                    for i in self.order() {
                        let e = &mut self.events[i];
                        if e.button == b && now.wrapping_sub(e.time) >= older {
                            e.used = u32::MAX;
                        }
                    }
                }
            }
            None => {
                self.events = vec![ButtonEvent::default(); 100];
                self.head = 0;
                self.count = 0;
            }
        }
    }

    /// `pressed` (`8211F598`): the unnamed button pressed within `dur` ms
    /// (default 15), not used by anyone.
    pub fn pressed(&mut self, params: &Params, now: u32) -> bool {
        let Some(b) = params.unnamed_checksum() else { return false };
        let dur = params.float(qb_key("dur")).map_or(15, |d| d as u32);
        let r = self.pressed_within(b, dur, u32::MAX, now);
        self.clear_temp();
        r
    }

    /// `Released` (`8211F778`): the unnamed button is not held.
    pub fn released(&self, params: &Params) -> bool {
        match params.unnamed_checksum() {
            Some(b) if b != 0 => !self.held[button_id(b)],
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn trig(items: &[(u32, Value)]) -> Value {
        Value::Struct(items.to_vec())
    }

    fn c(n: &str) -> Value {
        Value::Checksum(qb_key(n))
    }

    fn press(t: &mut Tricks, i: InputState, now: u32) {
        t.read_buttons(&i, now);
    }

    #[test]
    fn air_trick_logic_square_with_left() {
        let g = BTreeMap::new();
        let mut t = Tricks::default();
        let air = trig(&[(0, c("AirTrickLogic")), (0, c("square")), (0, c("left")), (0, Value::Int(500))]);
        press(&mut t, InputState { left: true, ..Default::default() }, 1000);
        press(&mut t, InputState { left: true, kick: true, ..Default::default() }, 1100);
        assert!(t.trigger(Some(&air), 1, !4, 1110, &g));
        // Used now: the same presses do not match again.
        assert!(!t.trigger(Some(&air), 1, !4, 1120, &g));
    }

    #[test]
    fn air_trick_logic_both_pressed_without_holding() {
        let g = BTreeMap::new();
        let mut t = Tricks::default();
        let air = trig(&[(0, c("AirTrickLogic")), (0, c("square")), (0, c("left")), (0, Value::Int(500))]);
        press(&mut t, InputState { kick: true, ..Default::default() }, 1000);
        press(&mut t, InputState { kick: true, left: true, ..Default::default() }, 1050);
        press(&mut t, InputState::default(), 1060);
        assert!(t.trigger(Some(&air), 1, !4, 1070, &g));
    }

    #[test]
    fn too_old_presses_do_not_match() {
        let g = BTreeMap::new();
        let mut t = Tricks::default();
        let p = trig(&[(0, c("Press")), (0, c("r2")), (0, Value::Int(20))]);
        press(&mut t, InputState { r2: true, ..Default::default() }, 1000);
        assert!(!t.trigger(Some(&p), 1, !4, 1030, &g));
        let mut t = Tricks::default();
        press(&mut t, InputState { r2: true, ..Default::default() }, 1000);
        assert!(t.trigger(Some(&p), 1, !4, 1010, &g));
    }

    #[test]
    fn triple_in_order_left_left_square() {
        let g = BTreeMap::new();
        let mut t = Tricks::default();
        let tr =
            trig(&[(0, c("TripleInOrder")), (qb_key("a"), c("left")), (qb_key("b"), c("left")), (0, c("square")), (0, Value::Int(300))]);
        press(&mut t, InputState { left: true, ..Default::default() }, 1000);
        press(&mut t, InputState::default(), 1050);
        press(&mut t, InputState { left: true, ..Default::default() }, 1100);
        press(&mut t, InputState { left: true, kick: true, ..Default::default() }, 1150);
        assert!(t.trigger(Some(&tr), 1, !4, 1160, &g));
    }

    #[test]
    fn tap_twice_release_is_the_boneless() {
        let g = BTreeMap::new();
        let mut t = Tricks::default();
        let tr = trig(&[(0, c("TapTwiceRelease")), (0, c("up")), (0, c("x")), (0, Value::Int(550))]);
        press(&mut t, InputState { crouch: true, up: true, ..Default::default() }, 1000);
        press(&mut t, InputState { crouch: true, ..Default::default() }, 1050);
        press(&mut t, InputState { crouch: true, up: true, ..Default::default() }, 1100);
        press(&mut t, InputState { up: true, ..Default::default() }, 1150);
        assert!(t.trigger(Some(&tr), 1, !4, 1160, &g));
    }

    #[test]
    fn queue_runs_the_mapped_trick() {
        let mut g = BTreeMap::new();
        let entry = trig(&[
            (qb_key("trickslot"), c("Air_SquareL")),
            (qb_key("trigger"), trig(&[(0, c("AirTrickLogic")), (0, c("square")), (0, c("left")), (0, Value::Int(500))])),
        ]);
        g.insert(qb_key("airtricks"), Value::Array(vec![entry]));
        g.insert(qb_key("defaulttrickdurationqueued"), Value::Int(200));
        let flip = trig(&[(qb_key("scr"), c("fliptrick")), (qb_key("params"), trig(&[(qb_key("score"), Value::Int(100))]))]);
        g.insert(qb_key("Trick_Kickflip"), flip);
        g.insert(qb_key("DefaultFliptricks"), trig(&[(qb_key("Air_SquareL"), c("Trick_Kickflip"))]));
        g.insert(qb_key("HawkTricks"), trig(&[(0, c("DefaultFliptricks"))]));
        let profile = trig(&[(qb_key("default_trick_mapping"), c("HawkTricks"))]);
        let mut t = Tricks::default();
        t.set_mapping(Some(&profile), &g);
        t.set_queue_tricks(&Params(vec![(0, c("airtricks"))]), &g);
        press(&mut t, InputState { left: true, kick: true, ..Default::default() }, 1000);
        t.update_lists(false, 1000, 1000, &g);
        assert_eq!(t.queue.len(), 1);
        let trick = t.next_queued(1010, &g).expect("queued");
        let run = t.run(&trick, None, None).expect("script");
        assert_eq!(run.script, qb_key("fliptrick"));
        // Expired entries are skipped.
        press(&mut t, InputState::default(), 1020);
        press(&mut t, InputState { left: true, kick: true, ..Default::default() }, 2000);
        t.update_lists(false, 2000, 2000, &g);
        assert!(t.next_queued(2300, &g).is_none());
    }
}
