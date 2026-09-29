//! The script interpreter: one running script (retail `CScript`) with its
//! call stack, loops, waits and event handlers. Each part names the retail
//! function it follows; parts not read from the game say so.
use crate::code::{self, *};
use crate::params::Params;
use p8_formats::qb::Value;
use p8_formats::qb_key;
use std::sync::Arc;

/// Checksum of the unnamed `TRUE` a script returns to answer an `if`.
pub const TRUE: u32 = 0x0203_B372;
/// `null_script`: handlers with it only take the event.
const NULL_SCRIPT: u32 = 0xC377_C572;
/// Handler group used when none is given (`DEFAULT`).
const DEFAULT_GROUP: u32 = 0x1CA1_FF20;

/// What the script's game object provides: its globals, the commands the
/// VM does not run itself, random numbers and the game clock.
pub trait Host {
    /// A global by name (scripts are [`Value::Script`]).
    fn global(&self, key: u32) -> Option<Value>;
    /// Run a command. `None` means the command is not translated yet.
    fn command(&mut self, script: &mut Script, name: u32, params: &Params) -> Option<bool>;
    /// `object : command` (retail 82226718). `None` when not translated.
    fn object_command(&mut self, _script: &mut Script, _object: u32, _name: u32, _params: &Params) -> Option<bool> {
        None
    }
    /// Whether `name` is a command (retail: a CFunction in the symbol
    /// table). Used for names inside expressions.
    fn is_command(&self, _name: u32) -> bool {
        false
    }
    /// A random number in `0..n` (retail 821E8508).
    fn random(&mut self, n: u32) -> u32;
    /// Game time in milliseconds (for timed waits).
    fn now_ms(&self) -> f64;
    /// Whether a wait the host set with [`Script::wait_on_host`] is over
    /// (e.g. an animation timer reaching a point).
    fn wait_done(&mut self, _token: u64, _target: f32) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Wait {
    None,
    /// Frames still to wait (retail wait type 1, `8221A270`).
    Frames(u32),
    /// Until this game time (retail time waits, `8221A1A8`/`8221A2C0`).
    Until(f64),
    /// `Block`: until something else moves the script on.
    Forever,
    /// Until the host says so (a script blocked on an object, e.g. an
    /// animation timer's wait list, `82386620`).
    Host(u64, f32),
}

#[derive(Clone, Debug)]
struct Loop {
    start: usize,
    end: usize,
    count: Option<i32>,
    first: bool,
}

/// A pending `if` (retail flags 0x40/0x20 at `+189`, target at `+192`).
#[derive(Clone, Copy, Debug)]
struct Cond {
    target: usize,
    negate: bool,
}

/// One level of the call stack (retail 52-byte frames at `+164`).
#[derive(Clone, Debug)]
struct Frame {
    name: u32,
    code: Arc<[u8]>,
    pc: usize,
    locals: Params,
    loops: Vec<Loop>,
    cond: Option<Cond>,
    on_exit: Option<(u32, Params)>,
}

/// An event handler (retail entries of the table at `+240`, 20 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct Handler {
    pub event: u32,
    pub script: u32,
    pub group: u32,
    pub exception: bool,
    pub params: Params,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Waiting for a later frame.
    Waiting,
    /// Ran to the end.
    Done,
}

/// Retail scripts with the stop-on-return flag stop the update when the
/// flagged level returns (update result 5); spawned-and-run-now scripts use
/// it. Not needed by what is translated so far.
#[derive(Clone, Debug)]
pub struct Script {
    pub name: u32,
    code: Arc<[u8]>,
    pc: Option<usize>,
    pub locals: Params,
    frames: Vec<Frame>,
    loops: Vec<Loop>,
    cond: Option<Cond>,
    wait: Wait,
    pub handlers: Vec<Handler>,
    on_exception_run: Option<u32>,
    on_exit: Option<(u32, Params)>,
    sites: code::Sites,
    /// Commands met that are not translated (for reporting).
    pub untranslated: Vec<u32>,
    /// Guard against a script looping without waiting.
    steps: u32,
}

fn script_code(host: &dyn Host, name: u32) -> Option<Arc<[u8]>> {
    match host.global(name)? {
        Value::Script(c) => Some(c),
        _ => None,
    }
}

/// Retail `822178C0`: are two values equal? A checksum naming a global is
/// compared as that global's value.
pub fn equal(host: &dyn Host, a: &Value, b: &Value) -> bool {
    let resolve = |v: &Value| match v {
        Value::Checksum(k) => match host.global(*k) {
            Some(g @ (Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Pair(..) | Value::Vector(_))) => g,
            Some(Value::Struct(s)) => Value::Struct(s),
            _ => v.clone(),
        },
        v => v.clone(),
    };
    let (a, b) = (resolve(a), resolve(b));
    use Value::*;
    match (&a, &b) {
        (Int(x), Int(y)) => x == y,
        (Int(x), Float(y)) | (Float(y), Int(x)) => *x as f32 == *y,
        // 1e-7 is the constant at 8201144C.
        (Float(x), Float(y)) => (x - y).abs() < 1e-7,
        (String(x), String(y)) => x == y,
        (Checksum(x), Checksum(y)) => x == y,
        (Pair(a1, a2), Pair(b1, b2)) => a1 == b1 && a2 == b2,
        (Vector(x), Vector(y)) => x == y,
        (Script(x), Script(y)) => Arc::ptr_eq(x, y),
        (Struct(x), Struct(y)) => {
            let sub = |p: &Vec<(u32, Value)>, q: &Vec<(u32, Value)>| p.iter().all(|c| q.contains(c));
            sub(x, y) && sub(y, x)
        }
        (Array(x), Array(y)) => x == y,
        _ => false,
    }
}

impl Script {
    /// Start script `name` with `params` (retail spawn: the script's own
    /// first-line parameters, then `params` over them).
    pub fn new(host: &mut dyn Host, name: u32, params: &Params) -> Option<Self> {
        let code = script_code(host, name)?;
        let mut s = Script {
            name,
            code,
            pc: Some(0),
            locals: Params::new(),
            frames: Vec::new(),
            loops: Vec::new(),
            cond: None,
            wait: Wait::None,
            handlers: Vec::new(),
            on_exception_run: None,
            on_exit: None,
            sites: code::Sites::default(),
            untranslated: Vec::new(),
            steps: 0,
        };
        s.enter(host, params);
        Some(s)
    }

    /// Block until the host's [`Host::wait_done`] answers true.
    pub fn wait_on_host(&mut self, token: u64, target: f32) {
        self.wait = Wait::Host(token, target);
    }

    pub fn is_done(&self) -> bool {
        self.pc.is_none()
    }

    /// The first line of a script may give default parameters; the passed
    /// ones merge over them (retail 8220EF50 / 8220EBC8).
    fn enter(&mut self, host: &mut dyn Host, params: &Params) {
        let (defaults, pc) = self.parse_params(host, 0, None);
        self.locals = defaults;
        self.locals.merge(params);
        self.pc = Some(pc);
    }

    fn rng(host: &mut dyn Host) -> impl FnMut(u32) -> u32 + '_ {
        move |n| host.random(n)
    }

    fn transparent(&mut self, host: &mut dyn Host, pc: usize) -> usize {
        let code = self.code.clone();
        code::transparent(&code, pc, &mut Self::rng(host), &mut self.sites)
    }

    /// Retail `8220F8F0`: run until the script waits or ends.
    pub fn update(&mut self, host: &mut dyn Host) -> Status {
        self.steps = 0;
        loop {
            let Some(pc) = self.pc else { return Status::Done };
            // 8221A568: count down or finish a wait.
            match self.wait {
                Wait::None => {}
                Wait::Frames(n) if n > 0 => {
                    self.wait = Wait::Frames(n - 1);
                    return Status::Waiting;
                }
                Wait::Frames(_) => self.wait = Wait::None,
                Wait::Until(t) if host.now_ms() < t => return Status::Waiting,
                Wait::Until(_) => self.wait = Wait::None,
                Wait::Forever => return Status::Waiting,
                Wait::Host(token, target) if !host.wait_done(token, target) => return Status::Waiting,
                Wait::Host(..) => self.wait = Wait::None,
            }
            self.steps += 1;
            if self.steps > 100_000 {
                // Retail would hang here; stop the script instead.
                self.pc = None;
                return Status::Done;
            }
            self.statement(host, pc);
            if self.wait != Wait::None && self.pc.is_some() {
                // Loop back to the wait check (retail does it at the top).
                continue;
            }
        }
    }

    fn statement(&mut self, host: &mut dyn Host, pc: usize) {
        let code = self.code.clone();
        match byte(&code, pc) {
            NEWLINE | ENDIF | ENDSWITCH => self.pc = Some(pc + 1),
            LINE => self.pc = Some(pc + 5),
            IF => {
                // 8220C5E8.
                let target = pc + 1 + u16_at(&code, pc + 1);
                let mut p = pc + 3;
                let negate = byte(&code, p) == NOT;
                if negate {
                    p += 1;
                }
                self.cond = Some(Cond { target, negate });
                self.pc = Some(p);
            }
            ELSE => self.pc = Some(pc + 1 + u16_at(&code, pc + 1)),
            SWITCH => self.switch(host, pc),
            BEGIN => {
                self.loops.push(Loop { start: pc + 1, end: 0, count: None, first: true });
                self.pc = Some(pc + 1);
            }
            REPEAT => self.repeat(host, pc),
            BREAK => self.break_loop(host, pc),
            ENDSCRIPT => {
                self.run_on_exit(host);
                self.return_from(host, Params::new());
            }
            RETURN => {
                let (ret, _) = self.parse_params(host, pc + 1, Some(&self.locals.clone()));
                self.run_on_exit(host);
                self.return_from(host, ret);
            }
            LONG_JUMP | RANDOM | RANDOM_B | RANDOM_NO_REPEAT | RANDOM_PERMUTE | SHORT_JUMP => {
                let p = self.transparent(host, pc);
                self.pc = Some(p);
            }
            END => self.return_from(host, Params::new()),
            _ => {
                let result = self.line(host, pc);
                if let (Some(r), Some(c)) = (result, self.cond.take())
                    && r == c.negate
                {
                    self.pc = Some(c.target);
                }
            }
        }
    }

    /// Retail `8220F210`: one command line. Returns its value, or `None`
    /// when it called a script (the answer comes with its return).
    fn line(&mut self, host: &mut dyn Host, pc: usize) -> Option<bool> {
        let code = self.code.clone();
        if byte(&code, pc) == OPEN_PAREN {
            let (v, end) = self.expression(host, pc);
            self.pc = Some(end);
            return Some(truth(&v) != 0);
        }
        let (name, mut p) = self.read_name(host, &code, pc);
        p = self.transparent(host, p);
        match byte(&code, p) {
            COLON => {
                // `object : command params`.
                let p2 = self.transparent(host, p + 1);
                let (cmd, p3) = self.read_name(host, &code, p2);
                let (params, end) = self.parse_params(host, p3, Some(&self.locals.clone()));
                self.pc = Some(end);
                return Some(match host.object_command(self, name, cmd, &params) {
                    Some(r) => r,
                    None => {
                        self.note_untranslated(cmd);
                        false
                    }
                });
            }
            EQUALS => {
                // Retail takes the target from the four bytes before `=`
                // (the name operand), so `<x> = v` sets `x` (8220F5EC).
                let name = u32_at(&code, p - 4);
                let p2 = self.transparent(host, p + 1);
                let mut tmp = Params::new();
                let end = if byte(&code, p2) == OPEN_PAREN {
                    let (v, end) = self.expression(host, p2);
                    tmp.add(name, v);
                    end
                } else {
                    let locals = self.locals.clone();
                    self.value(host, p2, name, &mut tmp, Some(&locals))
                };
                self.locals.merge(&tmp);
                self.pc = Some(end);
                return Some(true);
            }
            _ => {}
        }
        let (params, end) = self.parse_params(host, p, Some(&self.locals.clone()));
        self.pc = Some(end);
        self.call(host, name, &params)
    }

    /// Run `name` with `params` as a line would: a VM command, a script
    /// (pushed as a call), or a host command.
    fn call(&mut self, host: &mut dyn Host, name: u32, params: &Params) -> Option<bool> {
        if let Some(r) = self.vm_command(host, name, params) {
            return Some(r);
        }
        if let Some(code) = script_code(host, name) {
            self.push_call(host, name, code, params);
            return None;
        }
        match host.command(self, name, params) {
            Some(r) => Some(r),
            None => {
                self.note_untranslated(name);
                Some(false)
            }
        }
    }

    fn note_untranslated(&mut self, name: u32) {
        if !self.untranslated.contains(&name) {
            self.untranslated.push(name);
        }
    }

    /// Retail `8220EF50`: call a script as a subroutine.
    fn push_call(&mut self, host: &mut dyn Host, name: u32, code: Arc<[u8]>, params: &Params) {
        let frame = Frame {
            name: self.name,
            code: self.code.clone(),
            pc: self.pc.unwrap_or(0),
            locals: std::mem::take(&mut self.locals),
            loops: std::mem::take(&mut self.loops),
            cond: self.cond.take(),
            on_exit: self.on_exit.take(),
        };
        self.frames.push(frame);
        self.name = name;
        self.code = code;
        self.enter(host, params);
    }

    /// Retail `8220E258`: return to the caller, merging `ret` into its
    /// locals; a pending `if` on the call is true iff `ret` has `TRUE`.
    fn return_from(&mut self, _host: &mut dyn Host, ret: Params) {
        let Some(f) = self.frames.pop() else {
            self.pc = None;
            return;
        };
        self.name = f.name;
        self.code = f.code;
        self.pc = Some(f.pc);
        self.locals = f.locals;
        self.loops = f.loops;
        self.on_exit = f.on_exit;
        if let Some(c) = f.cond
            && ret.flag(TRUE) == c.negate
        {
            self.pc = Some(c.target);
        }
        self.locals.merge(&ret);
    }

    /// Retail `822108B8`: run this level's `OnExitRun` script, if any.
    fn run_on_exit(&mut self, host: &mut dyn Host) {
        if let Some((name, params)) = self.on_exit.take() {
            run_now(host, name, &params);
        }
    }

    /// Retail `8220EBC8` (via `8220EE00`): replace the running script, as
    /// `goto` and exceptions do. Pending `OnExitRun` scripts run first,
    /// innermost first (`8220DAD8`).
    pub fn goto(&mut self, host: &mut dyn Host, name: u32, params: &Params) {
        let params = params.clone();
        self.run_on_exit(host);
        while let Some(f) = self.frames.pop() {
            if let Some((n, p)) = f.on_exit {
                run_now(host, n, &p);
            }
        }
        self.loops.clear();
        self.cond = None;
        self.wait = Wait::None;
        match script_code(host, name) {
            Some(code) => {
                self.name = name;
                self.code = code;
                self.enter(host, &params);
            }
            None => self.pc = None,
        }
    }

    /// `repeat` (retail `8220C830`).
    fn repeat(&mut self, host: &mut dyn Host, pc: usize) {
        if self.loops.last().is_some_and(|l| l.first) {
            let (params, end) = self.parse_params(host, pc + 1, Some(&self.locals.clone()));
            let l = self.loops.last_mut().unwrap();
            l.end = end;
            l.count = params.unnamed_int();
            l.first = false;
        }
        let Some(l) = self.loops.last_mut() else {
            self.pc = Some(pc + 1);
            return;
        };
        if let Some(c) = &mut l.count {
            *c -= 1;
            if *c == 0 {
                let end = l.end;
                self.loops.pop();
                self.pc = Some(end);
                return;
            }
        }
        self.pc = Some(l.start);
    }

    /// `break` (retail `8220C928`): on past the matching `repeat` and its
    /// parameters.
    fn break_loop(&mut self, host: &mut dyn Host, pc: usize) {
        let code = self.code.clone();
        let mut p = pc;
        let mut depth = 0;
        loop {
            let t = byte(&code, p);
            p = skip(&code, p);
            if t == BEGIN {
                depth += 1;
            } else if t == REPEAT {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if t == END || p >= code.len() {
                break;
            }
        }
        let (_, end) = self.parse_params(host, p, None);
        self.loops.pop();
        self.pc = Some(end);
    }

    /// `switch` (retail `8220D4E0`).
    fn switch(&mut self, host: &mut dyn Host, pc: usize) {
        let code = self.code.clone();
        let locals = self.locals.clone();
        let (params, mut p) = self.parse_params(host, pc + 1, Some(&locals));
        let value = params.first().cloned();
        loop {
            match byte(&code, p) {
                SWITCH => p = skip_switch(&code, p + 1),
                CASE => {
                    let target = p + 2 + u16_at(&code, p + 2);
                    let (case, after) = self.parse_params(host, p + 4, Some(&locals));
                    let hit = match (&value, case.first()) {
                        (Some(a), Some(b)) => equal(host, a, b),
                        _ => false,
                    };
                    if !hit {
                        p = target;
                        continue;
                    }
                    // Skip further labels sharing this body.
                    let mut q = skip_newlines(&code, after);
                    while byte(&code, q) == SHORT_JUMP {
                        match byte(&code, q + 3) {
                            CASE => q = self.parse_params(host, q + 4, None).1,
                            DEFAULT => q += 4,
                            _ => break,
                        }
                        q = skip_newlines(&code, q);
                    }
                    self.pc = Some(q);
                    return;
                }
                DEFAULT => {
                    self.pc = Some(p + 4);
                    return;
                }
                ENDSWITCH => {
                    self.pc = Some(p + 1);
                    return;
                }
                END | ENDSCRIPT => {
                    self.pc = Some(p);
                    return;
                }
                _ => p = skip(&code, p),
            }
        }
    }

    /// Retail `8220C500`: the name at the start of a line: `name`, the
    /// checksum held by `<name>`, or for `<...>` the first unnamed checksum.
    fn read_name(&self, host: &dyn Host, code: &[u8], pc: usize) -> (u32, usize) {
        match byte(code, pc) {
            NAME => (u32_at(code, pc + 1), pc + 5),
            ALL_ARGS => (self.locals.unnamed_checksum().unwrap_or(0), pc + 1),
            ARG if byte(code, pc + 1) == NAME => {
                let key = u32_at(code, pc + 2);
                let v = match self.locals.get_in(key, &|k| host.global(k)) {
                    Some(Value::Checksum(c)) => c,
                    _ => 0,
                };
                (v, pc + 6)
            }
            _ => (0, skip(code, pc)),
        }
    }

    /// Retail `8220A228`: the parameters from `pc` to the end of the line
    /// (or a closing parenthesis). `caller` is used by `<...>` and `<name>`.
    fn parse_params(&mut self, host: &mut dyn Host, pc: usize, caller: Option<&Params>) -> (Params, usize) {
        let code = self.code.clone();
        let mut out = Params::new();
        if byte(&code, pc) == PACKED_STRUCT {
            let (start, end) = packed_struct_range(&code, pc);
            let block = code.get(start..end).unwrap_or(&[]);
            return (Params::from_struct(&p8_formats::qb::embedded_struct(block)), end);
        }
        let mut p = pc;
        for _ in 0..10_000 {
            p = self.transparent(host, p);
            match byte(&code, p) {
                END | NEWLINE | LINE | CLOSE_PAREN => break,
                NAME => {
                    let key = u32_at(&code, p + 1);
                    let after = self.transparent(host, p + 5);
                    if byte(&code, after) == EQUALS {
                        let v_at = self.transparent(host, after + 1);
                        if byte(&code, v_at) == OPEN_PAREN {
                            let (v, end) = self.expression(host, v_at);
                            out.add(key, v);
                            p = end;
                        } else {
                            p = self.value(host, v_at, key, &mut out, caller);
                        }
                    } else {
                        p = self.value(host, p, 0, &mut out, caller);
                    }
                }
                ALL_ARGS => {
                    if let Some(c) = caller {
                        out.merge(c);
                    }
                    p += 1;
                }
                STRUCT_OPEN => {
                    // 82209EF8 into the list itself.
                    let (s, end) = self.text_struct(host, p, caller);
                    out.merge(&s);
                    p = end;
                }
                COMMA => p += 1,
                OPEN_PAREN => {
                    let (v, end) = self.expression(host, p);
                    match &v {
                        Value::Struct(_) => out.merge(&Params::from_struct(&v)),
                        _ => out.add(0, v),
                    }
                    p = end;
                }
                _ => p = self.value(host, p, 0, &mut out, caller),
            }
        }
        (out, p)
    }

    /// Retail `82208CE8`: one value at `pc`, added to `out` as `key`.
    fn value(&mut self, host: &mut dyn Host, pc: usize, key: u32, out: &mut Params, caller: Option<&Params>) -> usize {
        let code = self.code.clone();
        let p = pc + 1;
        match byte(&code, pc) {
            NAME => {
                out.add(key, Value::Checksum(u32_at(&code, p)));
                p + 4
            }
            INT => {
                out.add(key, Value::Int(u32_at(&code, p) as i32));
                p + 4
            }
            FLOAT => {
                out.add(key, Value::Float(f32_at(&code, p)));
                p + 4
            }
            VECTOR => {
                out.add(key, Value::Vector([f32_at(&code, p), f32_at(&code, p + 4), f32_at(&code, p + 8)]));
                p + 12
            }
            PAIR => {
                out.add(key, Value::Pair(f32_at(&code, p), f32_at(&code, p + 4)));
                p + 8
            }
            STRING => {
                let n = u32_at(&code, p) as usize;
                let s = code.get(p + 4..p + 4 + n).unwrap_or(&[]);
                let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                out.add(key, Value::String(String::from_utf8_lossy(&s[..end]).into_owned()));
                p + 4 + n
            }
            WIDE_STRING => {
                let n = u32_at(&code, p) as usize;
                let s = code.get(p + 4..p + 4 + n).unwrap_or(&[]);
                let units: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
                out.add(key, Value::String(String::from_utf16_lossy(&units)));
                p + 4 + n
            }
            STRUCT_OPEN => {
                let (s, end) = self.text_struct(host, pc, caller);
                out.add(key, s.to_struct());
                end
            }
            ARRAY_OPEN => {
                let (a, end) = self.text_array(host, pc, caller);
                out.add(key, a);
                end
            }
            PACKED_STRUCT => {
                let (start, end) = packed_struct_range(&code, pc);
                out.add(key, p8_formats::qb::embedded_struct(code.get(start..end).unwrap_or(&[])));
                end
            }
            RANDOM_RANGE | RANDOM_RANGE_B => {
                // 82208858 (not read): a number between the range's ends.
                let mut tmp = Params::new();
                let end = self.value(host, p, 0, &mut tmp, caller);
                match tmp.first() {
                    Some(Value::Pair(a, b)) => {
                        let t = host.random(10_001) as f32 / 10_000.0;
                        out.add(key, Value::Float(a + (b - a) * t));
                    }
                    Some(v) => out.add(key, v.clone()),
                    None => {}
                }
                end
            }
            ARG => {
                // `<name>`: missing adds nothing; an unnamed struct merges.
                let name = u32_at(&code, p + 1);
                if let Some(v) = caller.and_then(|c| c.get_in(name, &|k| host.global(k))) {
                    match (&v, key) {
                        (Value::Struct(_), 0) => out.merge(&Params::from_struct(&v)),
                        _ => out.add(key, v),
                    }
                }
                p + 5
            }
            ALL_ARGS => {
                if let Some(c) = caller {
                    if key == 0 {
                        out.merge(c);
                    } else {
                        out.add(key, c.to_struct());
                    }
                }
                p
            }
            GLOBAL => {
                // `?name`, or `?<name>` naming the global through a local.
                let (name, end) = if byte(&code, p) == ARG {
                    let k = u32_at(&code, p + 2);
                    let n = match caller.and_then(|c| c.get_in(k, &|g| host.global(g))) {
                        Some(Value::Checksum(c)) => c,
                        _ => 0,
                    };
                    (n, p + 6)
                } else {
                    (u32_at(&code, p + 1), p + 5)
                };
                // Retail adds a link to the global (82214898); an unnamed
                // struct link reads as its members.
                match host.global(name) {
                    Some(v @ Value::Struct(_)) if key == 0 => out.merge(&Params::from_struct(&v)),
                    Some(v) => out.add(key, v),
                    None => {}
                }
                end
            }
            // Retail returns the same position for other tokens; skip them
            // here rather than stop.
            _ => skip(&code, pc).max(pc + 1),
        }
    }

    /// `{ ... }` written out in the bytecode (retail `82209EF8`).
    fn text_struct(&mut self, host: &mut dyn Host, pc: usize, caller: Option<&Params>) -> (Params, usize) {
        let code = self.code.clone();
        let mut out = Params::new();
        let mut p = pc + 1;
        for _ in 0..10_000 {
            p = self.transparent(host, p);
            match byte(&code, p) {
                STRUCT_CLOSE => return (out, p + 1),
                END | ENDSCRIPT => return (out, p),
                NEWLINE | COMMA => p += 1,
                LINE => p += 5,
                NAME => {
                    let key = u32_at(&code, p + 1);
                    let after = self.transparent(host, p + 5);
                    if byte(&code, after) == EQUALS {
                        let v_at = self.transparent(host, after + 1);
                        if byte(&code, v_at) == OPEN_PAREN {
                            let (v, end) = self.expression(host, v_at);
                            out.add(key, v);
                            p = end;
                        } else {
                            p = self.value(host, v_at, key, &mut out, caller);
                        }
                    } else {
                        p = self.value(host, p, 0, &mut out, caller);
                    }
                }
                _ => p = self.value(host, p, 0, &mut out, caller),
            }
        }
        (out, p)
    }

    /// `[ ... ]` written out in the bytecode (retail `8220A5E8`, not read in
    /// full: elements separated by commas and new lines).
    fn text_array(&mut self, host: &mut dyn Host, pc: usize, caller: Option<&Params>) -> (Value, usize) {
        let code = self.code.clone();
        let mut items = Vec::new();
        let mut p = pc + 1;
        for _ in 0..10_000 {
            p = self.transparent(host, p);
            match byte(&code, p) {
                ARRAY_CLOSE => return (Value::Array(items), p + 1),
                END | ENDSCRIPT => break,
                NEWLINE | COMMA => p += 1,
                LINE => p += 5,
                _ => {
                    let mut tmp = Params::new();
                    p = self.value(host, p, 0, &mut tmp, caller);
                    items.extend(tmp.0.into_iter().map(|(_, v)| v));
                }
            }
        }
        (Value::Array(items), p)
    }

    /// Retail `8220B878`: an expression starting at `(`, to its matching
    /// `)`. Operators apply by the precedence table at 826DE878.
    fn expression(&mut self, host: &mut dyn Host, pc: usize) -> (Value, usize) {
        let code = self.code.clone();
        let locals = self.locals.clone();
        let mut values: Vec<Value> = Vec::new();
        // Operators; 0 marks an open parenthesis.
        let mut ops: Vec<u8> = Vec::new();
        let mut depth = 0;
        let mut expect_operand = true;
        let mut p = pc;
        for _ in 0..10_000 {
            p = self.transparent(host, p);
            let t = byte(&code, p);
            match t {
                OPEN_PAREN => {
                    ops.push(0);
                    depth += 1;
                    expect_operand = true;
                    p += 1;
                }
                CLOSE_PAREN => {
                    p += 1;
                    while let Some(op) = ops.pop() {
                        if op == 0 {
                            break;
                        }
                        apply(host, op, &mut values);
                    }
                    depth -= 1;
                    expect_operand = false;
                    if depth == 0 {
                        break;
                    }
                }
                EQUALS | DOT | MINUS | PLUS | DIVIDE | MULTIPLY | LESS | 0x13 | GREATER | 0x15 | OR | 0x33 | 0x34 | 0x35 | 0x36
                | ARRAY_OPEN
                    if !expect_operand =>
                {
                    while let Some(&top) = ops.last() {
                        if top == 0 || precedence(top) < precedence(t) {
                            break;
                        }
                        ops.pop();
                        apply(host, top, &mut values);
                    }
                    ops.push(t);
                    expect_operand = true;
                    p += 1;
                }
                ARRAY_CLOSE => p += 1,
                NEWLINE | END | ENDSCRIPT => break,
                NAME if expect_operand => {
                    // A command is called with the rest as parameters and
                    // gives its result; a global gives its value; else the
                    // checksum itself (8220BBD0).
                    let name = u32_at(&code, p + 1);
                    let global = host.global(name);
                    if is_vm_command(name) || host.is_command(name) || matches!(global, Some(Value::Script(_))) {
                        let (params, end) = self.parse_params(host, p + 5, Some(&locals));
                        let r = self.call(host, name, &params).unwrap_or(false);
                        values.push(Value::Int(r as i32));
                        p = end;
                    } else {
                        // Right after '.', the member name stays a name.
                        let after_dot = ops.last() == Some(&DOT);
                        values.push(if after_dot { Value::Checksum(name) } else { resolve_name(host, name) });
                        p += 5;
                    }
                    expect_operand = false;
                }
                INT | FLOAT if !expect_operand => {
                    // A negative literal right after a value is a
                    // subtraction (8220BAD8).
                    let mut tmp = Params::new();
                    let end = self.value(host, p, 0, &mut tmp, Some(&locals));
                    let neg = match tmp.first() {
                        Some(Value::Int(i)) if *i < 0 => Some(Value::Int(-i)),
                        Some(Value::Float(f)) if *f < 0.0 => Some(Value::Float(-f)),
                        _ => None,
                    };
                    match neg {
                        Some(v) => {
                            while let Some(&top) = ops.last() {
                                if top == 0 || precedence(top) < precedence(MINUS) {
                                    break;
                                }
                                ops.pop();
                                apply(host, top, &mut values);
                            }
                            ops.push(MINUS);
                            values.push(v);
                            p = end;
                        }
                        None => break,
                    }
                }
                GLOBAL if byte(&code, p + 1) == NAME => {
                    // `?name` as an operand is the global's whole value (a
                    // struct stays one value, for `.` to read).
                    let v = host.global(u32_at(&code, p + 2)).unwrap_or(Value::Int(0));
                    values.push(v);
                    p += 6;
                    expect_operand = false;
                }
                _ => {
                    let mut tmp = Params::new();
                    let end = self.value(host, p, 0, &mut tmp, Some(&locals));
                    let mut v = tmp.first().cloned().unwrap_or(Value::Int(0));
                    // Retail `82218210`: a `<local>` holding a name that
                    // names a global stands for the global's value (not
                    // right after '.').
                    if t == ARG
                        && ops.last() != Some(&DOT)
                        && let Value::Checksum(c) = v
                    {
                        v = resolve_name(host, c);
                    }
                    values.push(v);
                    p = end.max(p + 1);
                    expect_operand = false;
                }
            }
        }
        while let Some(op) = ops.pop() {
            if op != 0 {
                apply(host, op, &mut values);
            }
        }
        (values.pop().unwrap_or(Value::Int(0)), p)
    }

    /// Commands the VM runs itself (retail CFunctions that act on the
    /// script). `None` when `name` is not one of them.
    fn vm_command(&mut self, host: &mut dyn Host, name: u32, params: &Params) -> Option<bool> {
        let k = qb_key;
        if name == k("Wait") {
            // 822A6FB8.
            let n = params.unnamed_float().unwrap_or(0.0);
            let seconds = params.flag(k("seconds")) || params.flag(k("second"));
            let frames = params.flag(k("GameFrame")) || params.flag(k("gameframes")) || params.flag(k("game"));
            let frame_time = params.flag(k("Frame")) || params.flag(k("Frames"));
            self.wait = if seconds {
                Wait::Until(host.now_ms() + (n * 1000.0) as i64 as f64)
            } else if frames {
                Wait::Frames(n as u32)
            } else if frame_time {
                // 16.6667 ms per frame (constant at 8200346C).
                Wait::Until(host.now_ms() + (n * 16.666_666) as i32 as f64)
            } else if params.flag(k("None")) {
                Wait::Frames(n as u32)
            } else {
                Wait::Until(host.now_ms() + n as i64 as f64)
            };
            return Some(true);
        }
        if name == k("Block") {
            self.wait = Wait::Forever;
            return Some(true);
        }
        if name == k("GotParam") {
            let key = params.unnamed_checksum().unwrap_or(0);
            return Some(self.locals.got(key));
        }
        if name == k("GetArraySize") {
            // 822A7A00: the unnamed array (822A6EF0), then `index1`..`index3`
            // into nested arrays; `array_size` = its length.
            let mut a = match params.0.iter().find(|(n, v)| *n == 0 && matches!(v, Value::Array(_))) {
                Some((_, Value::Array(a))) => a.clone(),
                _ => return None,
            };
            for key in ["index1", "index2", "index3"] {
                let Some(i) = params.int(k(key)) else { break };
                a = match a.get(i as usize) {
                    Some(Value::Array(x)) => x.clone(),
                    _ => return None,
                };
            }
            self.locals.add(k("array_size"), Value::Int(a.len() as i32));
            return Some(true);
        }
        if name == k("SetArrayElement") && !params.flag(k("globalarray")) {
            // 822ADE48: element `index` of the script's array `arrayname`
            // becomes `newvalue` (the `globalarray` form is not translated).
            let array = params.checksum(k("arrayname")).unwrap_or(0);
            let index = params.int(k("index")).unwrap_or(0) as usize;
            let value = params.get(k("newvalue")).cloned()?;
            let mut a = match self.locals.get(array) {
                Some(Value::Array(a)) => a.clone(),
                _ => return None,
            };
            *a.get_mut(index)? = value;
            self.locals.add(array, Value::Array(a));
            return Some(true);
        }
        if name == k("FormatText") {
            // 822A9B78 -> 822A96E0: the first unnamed string with `%c` (a
            // one-letter parameter name) or `%%name` (a name of letters,
            // digits and `_`) replaced by that parameter's value (a name of
            // a global stands for its value unless `DoNotResolve`); `\%` is
            // a plain `%`. A missing parameter fails the command. The text
            // goes to the parameter named by `TextName`, its checksum to
            // the one named by `ChecksumName`. Values: integers (`%d`, or
            // `%0*d` with `integer_width`) and strings; floats, vectors,
            // pairs, names (822A9128 cases not read) and `UseCommas`
            // (821EEEB0, not read) are not translated.
            let fmt = params.0.iter().find_map(|(n, v)| match (n, v) {
                (0, Value::String(t)) => Some(t.clone()),
                _ => None,
            })?;
            let chars: Vec<char> = fmt.chars().collect();
            let mut out = String::new();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '\\' && chars.get(i + 1) == Some(&'%') {
                    out.push('%');
                    i += 2;
                    continue;
                }
                if c != '%' || i + 1 >= chars.len() {
                    out.push(c);
                    i += 1;
                    continue;
                }
                let key = if chars[i + 1] == '%' {
                    let start = i + 2;
                    let mut end = start;
                    while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                        end += 1;
                    }
                    i = end;
                    chars[start..end].iter().collect::<String>()
                } else {
                    i += 2;
                    chars[i - 1].to_string()
                };
                let Some(v) = params.get(k(&key)) else { return Some(false) };
                let v = match v {
                    Value::Checksum(c) if !params.flag(k("DoNotResolve")) => resolve_name(host, *c),
                    v => v.clone(),
                };
                if params.flag(k("UseCommas")) {
                    return None;
                }
                let width = params.int(k("integer_width")).unwrap_or(0).max(0) as usize;
                match v {
                    Value::Int(n) if width != 0 => out.push_str(&format!("{n:0width$}")),
                    Value::Int(n) => out.push_str(&n.to_string()),
                    Value::String(t) => out.push_str(&t),
                    _ => return None,
                }
            }
            if let Some(t) = params.checksum(k("TextName")) {
                self.locals.add(t, Value::String(out.clone()));
            }
            if let Some(c) = params.checksum(k("ChecksumName")) {
                self.locals.add(c, Value::Checksum(qb_key(&out)));
            }
            return Some(true);
        }
        if name == k("AppendSuffixToChecksum") {
            // 822A9E70: `appended_id` = the checksum `base` continued over
            // `suffixstring` (821E57A0 -> 821E5718: the same CRC as a whole
            // name, so no name table is needed).
            let base = params.checksum(k("base"))?;
            let suffix = match params.get(k("suffixstring")) {
                Some(Value::String(t)) => t.clone(),
                _ => String::new(),
            };
            self.locals.add(k("appended_id"), Value::Checksum(p8_formats::checksum::qb_key_extend(base, &suffix)));
            return Some(true);
        }
        if name == k("GlobalExists") {
            // 822A8648: a global named `name` exists and, when `type` is
            // given, is of that type (structure/struct, array, string,
            // float, checksum/name, vector, integer/int, pair).
            let Some(v) = params.checksum(k("name")).and_then(|n| host.global(n)) else { return Some(false) };
            let Some(t) = params.checksum(k("type")) else { return Some(true) };
            let is = match v {
                Value::Struct(_) => t == k("structure") || t == k("struct"),
                Value::Array(_) => t == k("array"),
                Value::String(_) => t == k("string"),
                Value::Float(_) => t == k("float"),
                Value::Checksum(_) => t == k("checksum") || t == k("name"),
                Value::Vector(_) => t == k("vector"),
                Value::Int(_) => t == k("integer") || t == k("int"),
                Value::Pair(..) => t == k("pair"),
                _ => false,
            };
            return Some(is);
        }
        if name == k("StructureContains") {
            // 822ACAD0: `Structure` (a struct, or a name: the typed getter
            // 82212A68 -> 82212218 follows a name through the globals when
            // the global is a struct (82212550..82212608), else the name of
            // a struct among the script's locals); `Name` or the first
            // unnamed checksum; true if it has a component or a flag of
            // that name.
            let s = match params.get(k("Structure")) {
                Some(Value::Struct(m)) => Some(Params(m.clone())),
                Some(Value::Checksum(c)) => match crate::params::resolve_alias(*c, &|g| host.global(g)) {
                    Some(Value::Struct(m)) => Some(Params(m)),
                    _ => match self.locals.get(*c) {
                        Some(Value::Struct(m)) => Some(Params(m.clone())),
                        _ => None,
                    },
                },
                _ => None,
            };
            let Some(s) = s else { return Some(false) };
            let key = params.checksum(k("Name")).or(params.unnamed_checksum()).unwrap_or(0);
            return Some(s.get_in(key, &|g| host.global(g)).is_some() || s.flag(key));
        }
        if name == k("Goto") {
            let target = params.unnamed_checksum().unwrap_or(0);
            let p = params.params(k("Params")).unwrap_or_default();
            self.goto(host, target, &p);
            return Some(true);
        }
        if name == k("GotoRandomScript") {
            // 822A9078: the first unnamed array; if it is a non-empty array
            // of checksums, go to one picked at random (821E8508) with no
            // parameters (8220EE00 is given the object, not a struct).
            // Returns TRUE either way.
            if let Some(list) = params.unnamed_array()
                && !list.is_empty()
                && matches!(list[0], Value::Checksum(_))
            {
                let i = host.random(list.len() as u32) as usize;
                if let Some(Value::Checksum(target)) = list.get(i) {
                    self.goto(host, *target, &Params::new());
                }
            }
            return Some(true);
        }
        if name == k("SetException") || name == k("SetExceptionHandler") || name == k("SetEventHandler") {
            let exception = name != k("SetEventHandler") || params.flag(k("Exception"));
            let h = Handler {
                event: params.checksum(k("Ex")).unwrap_or(0),
                script: params.checksum(k("Scr")).unwrap_or(0),
                group: params.checksum(k("Group")).unwrap_or(DEFAULT_GROUP),
                exception,
                params: params.params(k("Params")).unwrap_or_default(),
            };
            self.add_handler(h);
            return Some(true);
        }
        if name == k("ClearEventHandler") {
            let ev = params.unnamed_checksum().unwrap_or(0);
            self.handlers.retain(|h| h.event != ev);
            return Some(true);
        }
        if name == k("ClearEventHandlerGroup") {
            let g = params.unnamed_checksum().unwrap_or(DEFAULT_GROUP);
            self.handlers.retain(|h| h.group != g);
            return Some(true);
        }
        if name == k("ResetEventHandlersFromTable") {
            // 822AA848 -> 8220D1F8: clear the group, then add the rows.
            let table = params.unnamed_checksum().and_then(|t| host.global(t));
            let group = params.checksum(k("Group")).unwrap_or(DEFAULT_GROUP);
            self.handlers.retain(|h| h.group != group);
            if let Some(Value::Array(rows)) = table {
                for row in &rows {
                    let r = Params::from_struct(row);
                    self.add_handler(Handler {
                        event: r.checksum(k("ex")).unwrap_or(0),
                        script: r.checksum(k("scr")).unwrap_or(0),
                        group,
                        exception: r.first() == Some(&Value::Checksum(k("Exception"))),
                        params: r.params(k("params")).unwrap_or_default(),
                    });
                }
            }
            return Some(true);
        }
        if name == k("OnExceptionRun") {
            self.on_exception_run = params.unnamed_checksum();
            return Some(true);
        }
        if name == k("OnExitRun") {
            self.on_exit = params.unnamed_checksum().map(|n| (n, params.params(k("Params")).unwrap_or_default()));
            return Some(true);
        }
        if name == k("Printf") {
            return Some(true);
        }
        None
    }

    /// Retail `822241F8`: add a handler, replacing one for the same event
    /// (LIKELY: the add function is not read in full).
    fn add_handler(&mut self, h: Handler) {
        self.handlers.retain(|o| o.event != h.event);
        self.handlers.push(h);
    }

    /// Retail `82224D78`: an event arrives. Returns whether a handler took
    /// it. Exceptions replace the running script and run it at once.
    pub fn event(&mut self, host: &mut dyn Host, event: u32, data: &Params) -> bool {
        let Some(h) = self.handlers.iter().find(|h| h.event == event).cloned() else {
            return false;
        };
        if h.script == NULL_SCRIPT {
            return true;
        }
        let mut params = h.params.clone();
        params.merge(data);
        if h.exception {
            if let Some(n) = self.on_exception_run.take() {
                let mut p = Params::new();
                p.add(0, Value::Checksum(event));
                run_now(host, n, &p);
            }
            self.goto(host, h.script, &params);
            self.update(host);
        } else {
            run_now(host, h.script, &params);
        }
        true
    }
}

fn is_vm_command(name: u32) -> bool {
    [
        "Wait",
        "Block",
        "GotParam",
        "GetArraySize",
        "SetArrayElement",
        "StructureContains",
        "FormatText",
        "AppendSuffixToChecksum",
        "GlobalExists",
        "Goto",
        "GotoRandomScript",
        "SetException",
        "SetExceptionHandler",
        "SetEventHandler",
        "ClearEventHandler",
        "ClearEventHandlerGroup",
        "ResetEventHandlersFromTable",
        "OnExceptionRun",
        "OnExitRun",
        "Printf",
    ]
    .iter()
    .any(|n| qb_key(n) == name)
}

/// Retail `82210460`: run a script straight away (not read in full; runs
/// until it ends or first waits).
fn run_now(host: &mut dyn Host, name: u32, params: &Params) {
    if let Some(mut s) = Script::new(host, name, params) {
        s.update(host);
    }
}

/// Retail `82208290`: skip new lines and line numbers.
fn skip_newlines(code: &[u8], mut p: usize) -> usize {
    loop {
        match byte(code, p) {
            NEWLINE => p += 1,
            LINE => p += 5,
            _ => return p,
        }
    }
}

/// Retail `8220C670`: past a nested switch.
fn skip_switch(code: &[u8], mut p: usize) -> usize {
    while byte(code, p) != ENDSWITCH && p < code.len() {
        if byte(code, p) == SWITCH {
            p = skip_switch(code, p + 1);
        } else {
            p = skip(code, p);
        }
    }
    p + 1
}

/// Operator precedence (table at 826DE878).
fn precedence(op: u8) -> i32 {
    match op {
        ARRAY_OPEN | DOT => 100,
        MULTIPLY | DIVIDE => 99,
        MINUS | PLUS => 98,
        0x35 => 90,
        0x36 => 89,
        LESS => 80,
        0x13 => 79,
        GREATER => 78,
        0x15 => 77,
        EQUALS => 76,
        0x33 => 60,
        0x34 => 59,
        OR => 58,
        _ => -1,
    }
}

/// How `||` reads a value (82204838 case 45): an integer as itself, a float
/// as 1 when not zero, anything else as 0.
fn truth(v: &Value) -> i32 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => (*f != 0.0) as i32,
        _ => 0,
    }
}

fn num(v: &Value) -> Option<f32> {
    v.as_f32()
}

/// Retail `82204838`: apply an operator to the top two values. Confirmed:
/// `=` (822178C0), `||`, `+` on numbers and strings. Not read in full, so
/// unconfirmed: `-`, `*`, `/` (numbers as `+` does), `<`/`>` (82217CA0,
/// 82217F58, numbers only here), `.` (member of a struct), `[` (element).
/// Retail `82218210` / `8220BAD4`: a name standing for a global's value
/// (following names of names); scripts, functions and unknown names stay
/// names.
fn resolve_name(host: &dyn Host, name: u32) -> Value {
    let mut cur = name;
    for _ in 0..16 {
        match host.global(cur) {
            None => return Value::Checksum(cur),
            Some(Value::Script(_)) => return Value::Checksum(cur),
            Some(Value::Checksum(next)) => cur = next,
            Some(v) => return v,
        }
    }
    Value::Checksum(cur)
}

fn apply(host: &dyn Host, op: u8, values: &mut Vec<Value>) {
    let b = values.pop().unwrap_or(Value::Int(0));
    let a = values.pop().unwrap_or(Value::Int(0));
    use Value::*;
    let arith = |f: fn(f32, f32) -> f32, i: fn(i32, i32) -> i32| match (&a, &b) {
        (Int(x), Int(y)) => Int(i(*x, *y)),
        _ => match (num(&a), num(&b)) {
            (Some(x), Some(y)) => Float(f(x, y)),
            _ => Int(0),
        },
    };
    let r = match op {
        EQUALS => Int(equal(host, &a, &b) as i32),
        OR => Int(truth(&a) | truth(&b)),
        PLUS => match (&a, &b) {
            (String(x), String(y)) => String(format!("{x}{y}")),
            (Struct(x), Struct(y)) => {
                let mut p = crate::params::Params(x.clone());
                p.merge(&crate::params::Params(y.clone()));
                p.to_struct()
            }
            _ => arith(|x, y| x + y, |x, y| x.wrapping_add(y)),
        },
        MINUS => arith(|x, y| x - y, |x, y| x.wrapping_sub(y)),
        MULTIPLY => arith(|x, y| x * y, |x, y| x.wrapping_mul(y)),
        DIVIDE => match (&a, &b) {
            (Int(_), Int(0)) => Int(0),
            _ => arith(|x, y| x / y, |x, y| x / y),
        },
        LESS => Int(matches!((num(&a), num(&b)), (Some(x), Some(y)) if x < y) as i32),
        GREATER => Int(matches!((num(&a), num(&b)), (Some(x), Some(y)) if x > y) as i32),
        DOT => match (&a, &b) {
            // 82211BE0, which also searches included global structs.
            (Struct(m), Checksum(k)) => crate::params::Params(m.clone()).get_in(*k, &|g| host.global(g)).unwrap_or(Int(0)),
            _ => Int(0),
        },
        ARRAY_OPEN => match (&a, &b) {
            (Array(items), Int(i)) => items.get(*i as usize).cloned().unwrap_or(Int(0)),
            _ => Int(0),
        },
        // No handler in retail (an error) or not read.
        _ => Int(0),
    };
    values.push(r);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Assembles bytecode from readable pieces.
    #[derive(Default)]
    struct Asm(Vec<u8>);
    impl Asm {
        fn t(mut self, b: u8) -> Self {
            self.0.push(b);
            self
        }
        fn name(mut self, n: &str) -> Self {
            self.0.push(NAME);
            self.0.extend(qb_key(n).to_le_bytes());
            self
        }
        fn int(mut self, i: i32) -> Self {
            self.0.push(INT);
            self.0.extend(i.to_le_bytes());
            self
        }
        fn arg(self, n: &str) -> Self {
            self.t(ARG).name(n)
        }
        /// A jump token whose target is filled in later.
        fn jump(mut self, tok: u8) -> (Self, usize) {
            self.0.push(tok);
            let at = self.0.len();
            self.0.extend([0, 0]);
            (self, at)
        }
        fn land(mut self, at: usize) -> Self {
            let off = self.0.len() - at;
            self.0[at] = off as u8;
            self.0[at + 1] = (off >> 8) as u8;
            self
        }
        fn nl(self) -> Self {
            self.t(NEWLINE)
        }
    }

    struct Test {
        globals: BTreeMap<u32, Value>,
        log: Vec<(String, Params)>,
        answers: BTreeMap<u32, bool>,
        time: f64,
    }

    impl Test {
        fn new(scripts: &[(&str, Asm)]) -> Self {
            let globals = scripts.iter().map(|(n, a)| (qb_key(n), Value::Script(a.0.clone().into()))).collect();
            Test { globals, log: Vec::new(), answers: BTreeMap::new(), time: 0.0 }
        }
        fn called(&self) -> Vec<String> {
            self.log.iter().map(|(n, _)| n.clone()).collect()
        }
    }

    const NAMES: &[&str] = &["a", "b", "c", "check", "yes", "no", "ollied", "handler"];

    impl Host for Test {
        fn global(&self, key: u32) -> Option<Value> {
            self.globals.get(&key).cloned()
        }
        fn command(&mut self, _s: &mut Script, name: u32, params: &Params) -> Option<bool> {
            let n = NAMES.iter().find(|n| qb_key(n) == name)?;
            self.log.push((n.to_string(), params.clone()));
            Some(*self.answers.get(&name).unwrap_or(&true))
        }
        fn random(&mut self, _n: u32) -> u32 {
            0
        }
        fn now_ms(&self) -> f64 {
            self.time
        }
    }

    fn run(t: &mut Test, name: &str) -> Script {
        let mut s = Script::new(t, qb_key(name), &Params::new()).unwrap();
        s.update(t);
        s
    }

    #[test]
    fn manual_transition_names_are_built_as_the_manual_script_does() {
        // manualtricks.qb `manual`: getlastanimdata / appendsuffixtochecksum
        // / formattext checksumname = transition_start '%s_out_%n' / GlobalExists.
        let k = qb_key;
        let mut t = Test::new(&[("main", Asm::default().nl().t(ENDSCRIPT))]);
        t.globals.insert(k("Manual_out_7"), Value::Struct(vec![]));
        let mut sc = run(&mut t, "main");
        let mut p = Params::new();
        p.add(k("base"), Value::Checksum(k("manual")));
        p.add(k("suffixstring"), Value::String("_data".into()));
        assert_eq!(sc.vm_command(&mut t, k("AppendSuffixToChecksum"), &p), Some(true));
        assert_eq!(sc.locals.get(k("appended_id")), Some(&Value::Checksum(k("manual_data"))));
        let mut p = Params::new();
        p.add(k("checksumname"), Value::Checksum(k("transition_start")));
        p.add(0, Value::String("%s_out_%n".into()));
        p.add(k("s"), Value::String("manual".into()));
        p.add(k("n"), Value::Int(7));
        assert_eq!(sc.vm_command(&mut t, k("FormatText"), &p), Some(true));
        assert_eq!(sc.locals.get(k("transition_start")), Some(&Value::Checksum(k("manual_out_7"))));
        let mut p = Params::new();
        p.add(0, Value::String("%s_out_%n".into()));
        p.add(k("s"), Value::String("manual".into()));
        assert_eq!(sc.vm_command(&mut t, k("FormatText"), &p), Some(false), "a missing parameter fails");
        let mut p = Params::new();
        p.add(k("name"), Value::Checksum(k("manual_out_7")));
        p.add(k("type"), Value::Checksum(k("structure")));
        assert_eq!(sc.vm_command(&mut t, k("GlobalExists"), &p), Some(true));
        p.add(k("type"), Value::Checksum(k("array")));
        assert_eq!(sc.vm_command(&mut t, k("GlobalExists"), &p), Some(false));
        p.add(k("name"), Value::Checksum(k("manual_out_1")));
        assert_eq!(sc.vm_command(&mut t, k("GlobalExists"), &p), Some(false));
    }

    #[test]
    fn structure_contains_follows_a_name_to_a_global_struct() {
        // 822ACAD0 via the typed getter 82212218: `structure = <transition_start>`
        // holds the name of a global struct (Manual_out_Pivot, flag `flipafter`);
        // the manual script must see its flags. A local struct still works.
        let k = qb_key;
        let mut t = Test::new(&[("main", Asm::default().nl().t(ENDSCRIPT))]);
        let mut pivot = Params::new();
        pivot.add(k("anim"), Value::Checksum(k("x")));
        pivot.add(0, Value::Checksum(k("flipafter")));
        t.globals.insert(k("Manual_out_Pivot"), pivot.to_struct());
        t.globals.insert(k("alias"), Value::Checksum(k("Manual_out_Pivot")));
        let mut sc = run(&mut t, "main");
        let ask = |sc: &mut Script, t: &mut Test, structure: Value, name: &str| {
            let mut p = Params::new();
            p.add(k("structure"), structure);
            p.add(0, Value::Checksum(k(name)));
            sc.vm_command(t, k("StructureContains"), &p)
        };
        let named = Value::Checksum(k("Manual_out_Pivot"));
        assert_eq!(ask(&mut sc, &mut t, named.clone(), "flipafter"), Some(true));
        assert_eq!(ask(&mut sc, &mut t, named.clone(), "anim"), Some(true));
        assert_eq!(ask(&mut sc, &mut t, named, "boardrotate"), Some(false));
        assert_eq!(ask(&mut sc, &mut t, Value::Checksum(k("alias")), "flipafter"), Some(true));
        assert_eq!(ask(&mut sc, &mut t, Value::Checksum(k("nothing")), "flipafter"), Some(false));
        sc.locals.add(k("mine"), pivot.to_struct());
        assert_eq!(ask(&mut sc, &mut t, Value::Checksum(k("mine")), "flipafter"), Some(true));
    }

    #[test]
    fn if_else_follows_the_command_result_and_not() {
        // if check / a / else / b / endif / if not check / c / endif
        let (s, j1) = Asm::default().nl().jump(IF);
        let s = s.name("check").nl().name("a").nl();
        let (s, j2) = s.jump(ELSE);
        let s = s.land(j1).nl().name("b").nl().land(j2).t(ENDIF).nl();
        let (s, j3) = s.jump(IF);
        let s = s.t(NOT).name("check").nl().name("c").nl().land(j3).t(ENDIF).nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        run(&mut t, "main");
        assert_eq!(t.called(), ["check", "a", "check"]);
        t.log.clear();
        t.answers.insert(qb_key("check"), false);
        run(&mut t, "main");
        assert_eq!(t.called(), ["check", "b", "check", "c"]);
    }

    #[test]
    fn counted_loop_and_break() {
        // begin / a / repeat 3 ; begin / b / break / repeat
        let s = Asm::default().nl().t(BEGIN).nl().name("a").nl().t(REPEAT).int(3).nl();
        let s = s.t(BEGIN).nl().name("b").nl().t(BREAK).nl().t(REPEAT).nl().name("c").nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        run(&mut t, "main");
        assert_eq!(t.called(), ["a", "a", "a", "b", "c"]);
    }

    #[test]
    fn wait_one_gameframe_resumes_next_frame() {
        let s = Asm::default().nl().name("a").nl().name("Wait").int(1).name("gameframe").nl().name("b").nl();
        let s = s.t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        let mut sc = run(&mut t, "main");
        assert_eq!(t.called(), ["a"]);
        assert_eq!(sc.update(&mut t), Status::Done);
        assert_eq!(t.called(), ["a", "b"]);
    }

    #[test]
    fn calling_a_script_answers_an_if_with_return_true_and_passes_values_back() {
        // sub: <y> = 5 / return TRUE x = <y>   main: if sub / a <x> / endif
        // (a called script's own locals stay private; only returned values
        // reach the caller)
        let sub = Asm::default().nl().arg("y").t(EQUALS).int(5).nl().t(RETURN).name("TRUE").name("x").t(EQUALS);
        let sub = sub.arg("y").nl().t(ENDSCRIPT);
        let (m, j) = Asm::default().nl().jump(IF);
        let m = m.name("sub").nl().name("a").arg("x").nl().land(j).t(ENDIF).nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", m), ("sub", sub)]);
        run(&mut t, "main");
        assert_eq!(t.log.len(), 1);
        assert_eq!(t.log[0].1.first(), Some(&Value::Int(5)));
    }

    #[test]
    fn exception_replaces_the_script_and_runs_it_now() {
        // main: SetException ex = ollied scr = handler / Block
        let m = Asm::default().nl().name("SetException").name("ex").t(EQUALS).name("ollied");
        let m = m.name("scr").t(EQUALS).name("handler").nl().name("Block").nl().t(ENDSCRIPT);
        let h = Asm::default().nl().name("a").nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", m), ("handler", h)]);
        let mut s = run(&mut t, "main");
        assert!(t.called().is_empty());
        assert!(s.event(&mut t, qb_key("ollied"), &Params::new()));
        assert_eq!(t.called(), ["a"]);
        assert!(s.is_done());
    }

    #[test]
    fn goto_random_script_goes_to_one_of_the_list() {
        // main: GotoRandomScript [handler] / a   (the test host's random is 0)
        let m = Asm::default().nl().name("GotoRandomScript").t(ARRAY_OPEN).name("handler").t(ARRAY_CLOSE).nl();
        let m = m.name("a").nl().t(ENDSCRIPT);
        let h = Asm::default().nl().name("b").nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", m), ("handler", h)]);
        let s = run(&mut t, "main");
        assert_eq!(t.called(), ["b"]);
        assert!(s.is_done());
    }

    #[test]
    fn switch_picks_the_matching_case() {
        // switch <v> / case b / a / (jump end) / case c / c / (jump end) / default / b / endswitch
        let s = Asm::default().nl().arg("v").t(EQUALS).name("c").nl().t(SWITCH).arg("v").nl();
        let (s, next1) = s.t(CASE).jump(SHORT_JUMP);
        let s = s.name("b").nl().name("a").nl();
        let (s, end1) = s.jump(SHORT_JUMP);
        let s = s.land(next1);
        let (s, next2) = s.t(CASE).jump(SHORT_JUMP);
        let s = s.name("c").nl().name("c").nl();
        let (s, end2) = s.jump(SHORT_JUMP);
        let s = s.land(next2);
        let (s, next3) = s.t(DEFAULT).jump(SHORT_JUMP);
        let s = s.nl().name("b").nl();
        let s = s.land(end1).land(end2).land(next3).t(ENDSWITCH).nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        run(&mut t, "main");
        assert_eq!(t.called(), ["c"]);
    }

    #[test]
    fn a_global_struct_member_in_an_expression() {
        // <x> = ( 2 * ?g . m ) / if ( <x> = 6 ) / a / endif
        let s = Asm::default().nl().arg("x").t(EQUALS).t(OPEN_PAREN).int(2).t(MULTIPLY).t(GLOBAL).name("g");
        let (s, j) = s.t(DOT).name("m").t(CLOSE_PAREN).nl().jump(IF);
        let s = s.t(OPEN_PAREN).arg("x").t(EQUALS).int(6).t(CLOSE_PAREN).nl().name("a").nl();
        let s = s.land(j).t(ENDIF).nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        t.globals.insert(qb_key("g"), Value::Struct(vec![(qb_key("z"), Value::Int(1)), (qb_key("m"), Value::Int(3))]));
        run(&mut t, "main");
        assert_eq!(t.called(), ["a"]);
    }

    #[test]
    fn expressions_follow_precedence_and_compare() {
        // <x> = ( 1 + 2 * 3 ) / if ( <x> = 7 ) / a / endif
        let s = Asm::default().nl().arg("x").t(EQUALS).t(OPEN_PAREN).int(1).t(PLUS).int(2).t(MULTIPLY).int(3);
        let (s, j) = s.t(CLOSE_PAREN).nl().jump(IF);
        let s = s.t(OPEN_PAREN).arg("x").t(EQUALS).int(7).t(CLOSE_PAREN).nl().name("a").nl();
        let s = s.land(j).t(ENDIF).nl().t(ENDSCRIPT);
        let mut t = Test::new(&[("main", s)]);
        run(&mut t, "main");
        assert_eq!(t.called(), ["a"]);
    }
}
