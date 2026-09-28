//! The skater's animation tree, translated from retail. Scripts build it:
//! `Skater_PlayStoppedAnim` / `Skater_PlayOnGroundAnim` etc. run
//! `skater_anim_command { target = body command = degenerateblend_addbranch
//! params = { tree = <branch> params = { ... } } }`, which adds a branch
//! (a script struct such as `Stopped_AnimBranch`) to the `body` blend node;
//! the tree is updated and sampled every frame to pose the skeleton.
//!
//! Poses (`82375BE0` buffers): per bone a rotation (as the clips store it),
//! a position and a weight, plus an overall strength (`+4688`). The pose
//! operations are the lazy ops run by `82376AE8`, done here immediately:
//! source `823757A0`, flush `82376BF8`, add `82376220`, apply difference
//! `823767C8`, blend `82376648`.
//!
//! Nodes (`82379F18` generic / `820A5E40` skater factories; per class the
//! vtable slot 2 is init, 4 update, 6 sample):
//! - `source`, `skatersource` (`820B3A80` -> `82376D18`): the clip at
//!   (clip length x the progress its timer passes down).
//! - `cycle` (`82386220`), `play` (`823862E0`), init `82386028`.
//! - `add` (`823753F8`, init `82375378`), `applydifference` (`8237B1D8`).
//! - `modulate` (init `82381C18`/`82381AC8`, update `823819D8`, sample
//!   `82381D98`) with blend functions (`8237BE30`: linear, smooth, step,
//!   curve; exponential not read).
//! - `degenerateblend` (update `8237CAB8`, sample `8237C910`, add-branch
//!   `8237CCA8`).
//! - `ik` (`8237F5D8`, see `p8_formats::ik`).
//! - `skaterflip` (`820B2638`): passes the pose through; mirrors it when the
//!   skater is flipped (`82377178`, not translated: no stance yet).
//! - `boardrotateoverlay` (`820A32C8`): passes the skater's pose through.
//! - `skaterposecapture` (`820B37F0`): live pass-through; after
//!   `posecapture_capture` it returns the captured pose (LIKELY: the
//!   command handler was not read, only the sample).
//! - `skatermodulate` (update `820B2790`, sample `820B3288`, finish
//!   `820B26C0`): `offwhenfinished` translated. `turn` and `slope` read
//!   skater-component fields (+380 turn; +228/+272/+304 slope vectors) that
//!   are not traced yet: UNKNOWN, taken as 0 (not turning, level ground).
//!
//! Not translated (listed in `untranslated`; APPROXIMATE stand-ins): timers
//! other than cycle/play run as `cycle`; other nodes with children pass
//! their first child through; leaves give an empty pose.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use p8_formats::anim::{self, Clip, qmul};
use p8_formats::ik;
use p8_formats::qb::Value;
use p8_formats::{pak, qb_key};
use p8_script::Params;

/// Node type names of the two factories (`82379F18`, `820A5E40`), for
/// reports.
pub const NODE_TYPES: &[&str] = &[
    "AI_Agent",
    "AI_Orient",
    "HitReactionDifference",
    "add",
    "addbutton",
    "addoscillating",
    "addthumbstick",
    "applydifference",
    "applydifferencebutton",
    "applydifferenceoscillating",
    "applydifferencethumbstick",
    "blank",
    "blendbutton",
    "blendconstant",
    "blenddebugthree",
    "blenddebugtwo",
    "blendoscillating",
    "blendthreethumbstick",
    "blendthumbstick",
    "bonealign",
    "catchandrelease",
    "cutsceneoverlay",
    "cycle",
    "degenerateblend",
    "degeneratepartial",
    "difference",
    "differencebutton",
    "differencelookat",
    "differenceoscillating",
    "differencethumbstick",
    "framefactoroffset",
    "igchead",
    "ik",
    "mappedmappedoverlay",
    "mirror",
    "modelviewer",
    "modulate",
    "motionextractedsource",
    "null",
    "overlay",
    "overlaybutton",
    "overlayoscillating",
    "overlaythumbstick",
    "partialswitch",
    "pingpong",
    "play",
    "posecapture",
    "ragdoll",
    "rotate",
    "sequencedsource",
    "source",
    "sourcewithoverlay",
    "speed",
    "test",
    "timedblend",
    "timedswitch",
    "timerangeswitch",
    "analogmodulate",
    "animsavesound",
    "apextimer",
    "applydifferencewithfacefixuphack",
    "balanceadd",
    "balanceblend",
    "boardrotateoverlay",
    "brakecatch",
    "braketimer",
    "crouchblend",
    "differencetoggle",
    "fakeskatersource",
    "grindlandadd",
    "kickcatch",
    "kicktimer",
    "landplay",
    "leanadd",
    "ledgeswitch",
    "mocapviewer",
    "ollielandblend",
    "programmerboard",
    "runningturn",
    "skatercatchandrelease",
    "skaterflip",
    "skateridleswitch",
    "skatermodulate",
    "skaterposecapture",
    "skatersource",
    "skatertimedswitch",
    "skatertimer",
    "speedblend",
    "speedtimer",
    "speedtimerthreeway",
    "spinblend",
    "spinleftrightadd",
    "spinleftrighttimer",
    "spinoutblend",
    "spinouttimer",
    "spinstraightblend",
    "takeoffblend",
    "takeoffplay",
    "turnblend",
    "ubercrouchblend",
    "walkingsource",
    "walkingspeedblend",
    "walkingtimer",
    "walkmodulate",
    "walkmonitor",
    "walkspeed",
    "wallridetimer",
    "wheelspeed",
    "wobble",
];

/// Branch names the skater scripts add to `body`, for reports.
pub const BRANCHES: &[&str] = &[
    "FlipTrick_AnimBranch",
    "GrabTrick_AnimBranch",
    "GrabTrick_AnimBranchNoIdle",
    "GrindLedge_AnimBranch",
    "Grind_AnimBranch",
    "InAirApex_AnimBranch",
    "InAirLand_AnimBranch",
    "InAirTakeOff_AnimBranch",
    "InAirUpper_AnimBranch",
    "InAir_AnimBranch",
    "InvertRange_AnimBranch",
    "Invert_AnimBranch",
    "Manual_AnimBranch",
    "NollieOnGround_AnimBranch",
    "OllieLand_AnimBranch",
    "OllieSpin_AnimBranch",
    "OllieTakeOff_AnimBranch",
    "Ollie_AnimBranch",
    "OnGround_AnimBranch",
    "Stopped_AnimBranch",
    "Stopped_Out_Branch",
    "SwitchBoard_Branch",
    "WallRide_AnimBranch_Standard",
    "blank_animbranch",
];

/// A node type's or branch's name from its checksum.
pub fn name_of(key: u32) -> Option<&'static str> {
    NODE_TYPES.iter().chain(BRANCHES).copied().find(|n| qb_key(n) == key)
}

/// Clips from `perm_anims.pak.xen`, parsed on first use.
pub struct ClipLib {
    data: Vec<u8>,
    index: HashMap<u32, (usize, usize)>,
    std_q: Vec<u8>,
    cache: HashMap<u32, Option<Arc<Clip>>>,
}

impl ClipLib {
    pub fn open(pak_path: &Path, std_q_path: &Path) -> Result<Self, String> {
        let headers = std::fs::read(pak_path).map_err(|e| format!("{}: {e}", pak_path.display()))?;
        let pab = std::fs::read(pak_path.to_string_lossy().replace(".pak.xen", ".pab.xen")).ok();
        let (archive, data) =
            pak::parse_file(&headers, pab.as_deref()).map_err(|e| format!("{} was not recognised: {e:?}", pak_path.display()))?;
        let ska = qb_key(".ska");
        let index = archive.entries.iter().filter(|e| e.type_key == ska).map(|e| (e.full_name_key, (e.offset, e.size))).collect();
        let std_q = std::fs::read(std_q_path).map_err(|e| format!("{}: {e}", std_q_path.display()))?;
        Ok(Self { data, index, std_q, cache: HashMap::new() })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn get(&mut self, key: u32) -> Option<Arc<Clip>> {
        if let Some(c) = self.cache.get(&key) {
            return c.clone();
        }
        let clip =
            self.index.get(&key).and_then(|&(o, n)| self.data.get(o..o + n)).and_then(|d| Clip::parse(d, &self.std_q).ok()).map(Arc::new);
        self.cache.insert(key, clip.clone());
        clip
    }

    /// A clip's length in seconds (`822448D0`), 0 when missing.
    pub fn duration(&mut self, key: u32) -> f32 {
        self.get(key).map_or(0.0, |c| c.duration)
    }
}

/// The skeleton as the tree needs it: rest pose in the clips' convention
/// (skeleton `+0x28`/`+0x2C`, copied into every source pose by `823757A0`).
#[derive(Clone, Debug, Default)]
pub struct Rig {
    pub names: Vec<u32>,
    pub parents: Vec<Option<usize>>,
    pub bind_q: Vec<[f32; 4]>,
    pub bind_t: Vec<[f32; 3]>,
}

impl Rig {
    /// From a skeleton (whose rotations are already conjugated for the
    /// renderer; the tree works in the clips' convention).
    pub fn from_skeleton(sk: &p8_formats::skeleton::Skeleton) -> Self {
        Self {
            names: sk.bones.iter().map(|b| b.name).collect(),
            parents: sk.bones.iter().map(|b| b.parent).collect(),
            bind_q: sk.bones.iter().map(|b| [-b.rotation[0], -b.rotation[1], -b.rotation[2], b.rotation[3]]).collect(),
            bind_t: sk.bones.iter().map(|b| b.position).collect(),
        }
    }

    pub fn bone(&self, name: u32) -> Option<usize> {
        self.names.iter().position(|&n| n == name)
    }
}

/// A pose buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub q: Vec<[f32; 4]>,
    pub t: Vec<[f32; 3]>,
    pub w: Vec<f32>,
    pub strength: f32,
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l == 0.0 { q } else { q.map(|c| c / l) }
}

/// Blend from identity towards `q` by `a` after putting `q` in the w >= 0
/// hemisphere (`82376220`).
fn from_identity(q: [f32; 4], a: f32) -> [f32; 4] {
    let q = if q[3] >= 0.0 { q } else { q.map(|c| -c) };
    normalize4([q[0] * a, q[1] * a, q[2] * a, 1.0 + (q[3] - 1.0) * a])
}

impl Pose {
    /// `82377160`: strength 0. Flushing it gives the rest pose with zero
    /// weights (`82383540`).
    pub fn zero(rig: &Rig) -> Self {
        Self { q: rig.bind_q.clone(), t: rig.bind_t.clone(), w: vec![0.0; rig.bind_q.len()], strength: 0.0 }
    }

    /// `82376BF8`: fold the strength into the bone weights.
    pub fn flush(&mut self, rig: &Rig) {
        if self.strength == 1.0 {
            return;
        }
        if self.strength == 0.0 {
            *self = Self::zero(rig);
        } else {
            let s = self.strength;
            self.w.iter_mut().for_each(|w| *w *= s);
        }
        self.strength = 1.0;
    }

    /// `82377148`.
    pub fn scale(&mut self, s: f32) {
        self.strength *= s;
    }

    /// Source (`823757A0`): the rest pose, then the clip at `time`.
    pub fn from_clip(rig: &Rig, clip: &Clip, time: f32) -> Self {
        let mut p = Self { q: rig.bind_q.clone(), t: rig.bind_t.clone(), w: vec![0.0; rig.bind_q.len()], strength: 1.0 };
        for (b, s) in clip.sample(time).iter().enumerate().take(p.q.len()) {
            p.w[b] = s.weight;
            if s.weight == 0.0 {
                continue;
            }
            p.q[b] = s.q;
            if let Some(t) = s.t {
                p.t[b] = if clip.adds_positions() { [p.t[b][0] + t[0], p.t[b][1] + t[1], p.t[b][2] + t[2]] } else { t };
            }
        }
        p
    }

    /// Add node combine (`823773A0` + op `82376220`): each side is blended
    /// from identity by its weight, rotation = A ⊗ B, position = A + B.
    pub fn add(&mut self, wa: f32, wb: f32, mut b: Pose) {
        if b.strength == 0.0 {
            self.strength *= wa;
            return;
        }
        if self.strength == 0.0 {
            b.strength *= wb;
            *self = b;
            return;
        }
        let (fa, fb) = (self.strength * wa, b.strength * wb);
        for i in 0..self.q.len() {
            let (a, bw) = (self.w[i] * fa, b.w[i] * fb);
            if a == 0.0 && bw == 0.0 {
                self.w[i] = 0.0;
                continue;
            }
            let (mut qa, mut ta) = (self.q[i], self.t[i]);
            if a != 1.0 {
                qa = from_identity(qa, a);
                ta = ta.map(|c| c * a);
            }
            let (mut qb, mut tb) = (b.q[i], b.t[i]);
            if bw != 1.0 {
                qb = from_identity(qb, bw);
                tb = tb.map(|c| c * bw);
            }
            self.q[i] = qmul(qa, qb);
            self.t[i] = [ta[0] + tb[0], ta[1] + tb[1], ta[2] + tb[2]];
            self.w[i] = 1.0;
        }
        self.strength = 1.0;
    }

    /// ApplyDifference (`82377058` + op `823767C8`): per bone with the
    /// difference's weight w (bone weight x its strength x `weight`),
    /// rotation = nlerp(base, base ⊗ diff, w), position = base + w·diff.
    pub fn apply_difference(&mut self, weight: f32, diff: &Pose, rig: &Rig) {
        if diff.strength == 0.0 {
            return;
        }
        self.flush(rig);
        let f = diff.strength * weight;
        for i in 0..self.q.len() {
            let w = diff.w[i] * f;
            if w == 0.0 {
                continue;
            }
            self.q[i] = anim::nlerp(self.q[i], qmul(self.q[i], diff.q[i]), w);
            let d = diff.t[i];
            self.t[i] = [self.t[i][0] + w * d[0], self.t[i][1] + w * d[1], self.t[i][2] + w * d[2]];
        }
    }

    /// Blend towards `b` by `f` (`82376F88` + op `82376648`).
    pub fn blend(&mut self, mut b: Pose, f: f32, rig: &Rig) {
        self.flush(rig);
        b.flush(rig);
        for i in 0..self.q.len() {
            let (a, bw) = (self.w[i], b.w[i] * f);
            if a == 0.0 && bw == 0.0 {
                continue;
            }
            let u = bw / ((1.0 - a) * bw + a);
            self.q[i] = anim::nlerp(self.q[i], b.q[i], u);
            let (ta, tb) = (self.t[i], b.t[i]);
            self.t[i] = [ta[0] + (tb[0] - ta[0]) * u, ta[1] + (tb[1] - ta[1]) * u, ta[2] + (tb[2] - ta[2]) * u];
            self.w[i] = (a + bw).min(1.0);
        }
    }
}

/// Blend functions (`8237BE30`).
#[derive(Clone, Debug, PartialEq)]
pub enum BlendFn {
    Linear,
    /// `8237BBD0`: 3t² - 2t³.
    Smooth,
    /// `8237BC08`: 0 below 0.5, else 1.
    Step,
    /// `8237BD70` stores 1 - each value; `8237BFE8` interpolates them.
    Curve(Vec<f32>),
}

impl BlendFn {
    fn new(name: u32, curve: Option<&Value>) -> Self {
        if name == qb_key("smooth") {
            BlendFn::Smooth
        } else if name == qb_key("step") {
            BlendFn::Step
        } else if name == qb_key("curve") {
            let v = match curve {
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_f32).map(|x| 1.0 - x).collect(),
                _ => Vec::new(),
            };
            BlendFn::Curve(v)
        } else {
            // 0 and `linear`; `exponential` (8237BC40) is not read.
            BlendFn::Linear
        }
    }

    fn eval(&self, t: f32) -> f32 {
        match self {
            BlendFn::Linear => t,
            BlendFn::Smooth => (t - 1.5) * t * t * -2.0,
            BlendFn::Step => {
                if t >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            BlendFn::Curve(v) => {
                let n = v.len() as i32;
                if n < 2 {
                    return v.first().copied().unwrap_or(t);
                }
                let mut i = ((n - 1) as f32 * t) as i32;
                if i == n - 1 {
                    i = n - 2;
                }
                let x = (n - 1) as f32 * t;
                let (i0, i1) = (i as usize, i as usize + 1);
                v[i0] + (x - i as f32) / 1.0 * (v[i1] - v[i0])
            }
        }
    }
}

/// Timer nodes (`82386028` init).
#[derive(Clone, Debug, PartialEq)]
pub struct Timer {
    pub looping: bool,
    /// `+20`
    pub time: f32,
    /// `+28`
    pub speed: f32,
    /// `+32`
    pub duration: f32,
    /// `+36`
    pub end: f32,
    /// `+44`
    pub finished: bool,
}

impl Timer {
    /// `82386220` (cycle) / `823862E0` (play). The children get dt x speed.
    fn update(&mut self, dt: &mut f32) {
        *dt *= self.speed;
        self.time += *dt;
        if self.looping {
            if self.end > 0.0 {
                while self.time < 0.0 {
                    self.finished = true;
                    self.time += self.end;
                }
                while self.time > self.end {
                    self.finished = true;
                    self.time -= self.end;
                }
            }
        } else if self.time < 0.0 || self.time > self.end {
            self.time = self.time.clamp(0.0, self.end);
            self.finished = true;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Modulate {
    /// `+24`
    pub strength: f32,
    /// `+40`
    start: f32,
    /// `+28`
    t: f32,
    /// `+32`
    duration: f32,
    /// `+36`
    done: bool,
    /// `+44`
    from_start: bool,
    func: BlendFn,
}

impl Modulate {
    /// `823819D8`.
    fn update(&mut self, dt: f32) {
        if self.done {
            return;
        }
        self.t += dt / self.duration;
        if self.t >= 1.0 {
            self.t = 1.0;
            self.done = true;
        }
        let raw = self.func.eval(self.t);
        self.strength = if self.from_start { self.start + (1.0 - self.start) * raw } else { self.start * raw };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkaterModulate {
    timertype: u32,
    invert: bool,
    /// `+56`
    min: f32,
    /// `+32` target before `820B26C0`
    x: f32,
    /// `+40`
    length: f32,
    /// `+28`
    pub strength: f32,
}

impl SkaterModulate {
    /// `820B26C0`: clamp, curve (none translated here), invert, minimum.
    fn finish(&mut self) {
        let mut s = self.x.clamp(0.0, 1.0);
        if self.invert {
            s = 1.0 - s;
        }
        if self.min > 0.0 {
            s = self.min + (1.0 - self.min) * s;
        }
        self.strength = s;
    }

    /// `820B2790` (update) and the value part of `820B3288` (sample).
    fn update(&mut self, dt: f32) {
        if self.timertype == qb_key("offwhenfinished") {
            // 28C4: count up to the clip length, then off (on if inverted).
            self.x += dt;
            if self.x >= self.length {
                self.strength = if self.invert { 1.0 } else { 0.0 };
            }
            return;
        }
        // `turn` (+380 of the skater's component) and `slope` (+228/+272/
        // +304): UNKNOWN inputs, taken as 0 (not turning, level ground).
        self.x = 0.0;
        self.finish();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DegenerateBlend {
    /// `+32` records: remaining (1 -> 0) and the blend function.
    records: Vec<(f32, BlendFn)>,
    /// `+20`
    duration: f32,
    /// `+28` (`degenerateblend_setnextblendduration`), -1 when unset.
    next_duration: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Source {
        anim: u32,
    },
    Timer(Timer),
    Add {
        wa: f32,
        wb: f32,
    },
    ApplyDifference {
        weight: f32,
    },
    Modulate(Modulate),
    SkaterModulate(SkaterModulate),
    Flip,
    PassThrough,
    PoseCapture {
        frozen: Option<Pose>,
    },
    DegenerateBlend(DegenerateBlend),
    Ik {
        chains: Vec<ik::Chain>,
    },
    /// Not translated: the node type.
    Untranslated(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u32,
    pub kind: Kind,
    pub children: Vec<Node>,
}

/// Expand unnamed references to global structs (the search `82211BE0`
/// does), keeping the other items in order.
fn flatten(v: &Value, g: &dyn Fn(u32) -> Option<Value>, depth: u32) -> Vec<(u32, Value)> {
    let mut out = Vec::new();
    if let Value::Struct(items) = v {
        for (k, x) in items {
            match (k, x) {
                (0, Value::Checksum(c)) if depth < 16 => match g(*c) {
                    Some(s @ Value::Struct(_)) => out.extend(flatten(&s, g, depth + 1)),
                    _ => out.push((*k, x.clone())),
                },
                _ => out.push((*k, x.clone())),
            }
        }
    }
    out
}

/// Tree building context: the branch's parameters and the scripts.
struct Build<'a> {
    globals: &'a dyn Fn(u32) -> Option<Value>,
    lib: &'a mut ClipLib,
    rig: &'a Rig,
    types: &'a mut Vec<u32>,
}

fn lookup(items: &[(u32, Value)], key: u32) -> Option<&Value> {
    items.iter().rev().find(|(k, _)| *k == key).map(|(_, v)| v)
}

impl Build<'_> {
    /// A value with branch parameters substituted (LIKELY: `anim = my_anim`
    /// style references resolve through the branch's params).
    fn resolve(&self, v: &Value, scope: &Params) -> Value {
        let mut v = v.clone();
        for _ in 0..8 {
            match &v {
                Value::Checksum(c) if *c != 0 => match scope.get(*c) {
                    Some(n) => v = n.clone(),
                    None => break,
                },
                _ => break,
            }
        }
        v
    }

    fn checksum(&self, items: &[(u32, Value)], name: &str, scope: &Params) -> Option<u32> {
        match lookup(items, qb_key(name)).map(|v| self.resolve(v, scope)) {
            Some(Value::Checksum(c)) => Some(c),
            _ => None,
        }
    }

    fn float(&self, items: &[(u32, Value)], name: &str, scope: &Params) -> Option<f32> {
        lookup(items, qb_key(name)).map(|v| self.resolve(v, scope)).and_then(|v| v.as_f32())
    }

    fn node(&mut self, v: &Value, scope: &Params) -> Node {
        let items = flatten(v, self.globals, 0);
        // `user_params` rename outer params for the included sub-branch.
        let mut scope = scope.clone();
        if let Some(Value::Struct(up)) = lookup(&items, qb_key("user_params")) {
            let outer = scope.clone();
            for (k, x) in up {
                scope.add(*k, self.resolve(x, &outer));
            }
        }
        let scope = &scope;
        let children: Vec<Node> = items
            .iter()
            .filter(|(k, _)| *k == 0)
            .filter_map(|(_, x)| match x {
                Value::Array(a) => Some(a),
                _ => None,
            })
            .flatten()
            .map(|c| self.node(c, scope))
            .collect();
        let ty = self.checksum(&items, "type", scope).unwrap_or(0);
        let id = self.checksum(&items, "id", scope).unwrap_or(0);
        let k = qb_key;
        let kind = if ty == k("source") || ty == k("skatersource") {
            Kind::Source { anim: self.checksum(&items, "anim", scope).unwrap_or(0) }
        } else if ty == k("cycle") || ty == k("play") {
            Kind::Timer(self.timer(&items, scope, ty == k("cycle")))
        } else if ty == k("add") {
            // 82375378: both weights default to 1 (their param names are
            // not resolved: 0x9D5E2C7C, 0x1A524142).
            let wa = lookup(&items, 0x9D5E_2C7C).and_then(Value::as_f32).unwrap_or(1.0);
            let wb = lookup(&items, 0x1A52_4142).and_then(Value::as_f32).unwrap_or(1.0);
            Kind::Add { wa, wb }
        } else if ty == k("applydifference") {
            // The node weight `+20`: its init (slot 2 `820A2390`) is not
            // read; 1 (INFERRED from the trees giving no weight).
            Kind::ApplyDifference { weight: 1.0 }
        } else if ty == k("modulate") {
            Kind::Modulate(self.modulate(&items, scope))
        } else if ty == k("skatermodulate") {
            let invert = matches!(lookup(&items, k("invert")).map(|v| self.resolve(v, scope)), Some(Value::Int(1)));
            let anim = self.checksum(&items, "anim", scope).unwrap_or(0);
            let timertype = self.checksum(&items, "timertype", scope).unwrap_or(0);
            let mut m = SkaterModulate {
                timertype,
                invert,
                min: self.float(&items, "min", scope).unwrap_or(0.0),
                x: 0.0,
                length: self.lib.duration(anim),
                strength: 1.0,
            };
            if timertype != k("offwhenfinished") {
                m.finish();
            }
            Kind::SkaterModulate(m)
        } else if ty == k("skaterflip") {
            Kind::Flip
        } else if ty == k("boardrotateoverlay") {
            Kind::PassThrough
        } else if ty == k("skaterposecapture") {
            Kind::PoseCapture { frozen: None }
        } else if ty == k("degenerateblend") {
            Kind::DegenerateBlend(DegenerateBlend { records: Vec::new(), duration: 0.0, next_duration: -1.0 })
        } else if ty == k("ik") {
            Kind::Ik { chains: self.ik_chains(&items) }
        } else {
            if !self.types.contains(&ty) {
                self.types.push(ty);
            }
            if children.is_empty() { Kind::Untranslated(ty) } else { Kind::Timer(self.timer(&items, scope, true)) }
        };
        let kind = match kind {
            // Untranslated nodes with children: the first child (APPROXIMATE),
            // timed like `cycle` when they name a clip.
            Kind::Timer(t) if !(ty == k("cycle") || ty == k("play")) => {
                if lookup(&items, k("anim")).is_some() {
                    Kind::Timer(t)
                } else {
                    Kind::PassThrough
                }
            }
            other => other,
        };
        Node { id, kind, children }
    }

    fn timer(&mut self, items: &[(u32, Value)], scope: &Params, looping: bool) -> Timer {
        let anim = self.checksum(items, "anim", scope).unwrap_or(0);
        let mut duration = self.lib.duration(anim);
        // 0xF0DE0109: the speed parameter (name not resolved), default 1.
        let mut speed = lookup(items, 0xF0DE_0109).map(|v| self.resolve(v, scope)).and_then(|v| v.as_f32()).unwrap_or(1.0);
        if let Some(c) = self.float(items, "cycle_length", scope) {
            if duration == -1.0 {
                duration = c;
            }
            speed = duration / c;
        }
        if duration == 0.0 {
            duration = 1.0;
        }
        let time = self.float(items, "start", scope).map_or(0.0, |s| duration * s);
        let end = self.float(items, "end", scope).map_or(duration, |e| duration * e);
        Timer { looping, time, speed, duration, end, finished: false }
    }

    fn modulate(&mut self, items: &[(u32, Value)], scope: &Params) -> Modulate {
        let strength = self.float(items, "strength", scope).unwrap_or(1.0);
        let mut m = Modulate { strength, start: 1.0, t: 0.0, duration: 1.0, done: true, from_start: true, func: BlendFn::Linear };
        // 82381AC8 with `blendfunction`, `blendtime`, `anim`, `blendcurve`.
        if let Some(f) = self.checksum(items, "blendfunction", scope) {
            let anim = self.checksum(items, "anim", scope).unwrap_or(0);
            m.duration = if anim != 0 { self.lib.duration(anim) } else { self.float(items, "blendtime", scope).unwrap_or(0.0) };
            let curve = lookup(items, qb_key("blendcurve")).map(|v| self.resolve(v, scope));
            m.func = BlendFn::new(f, curve.as_ref());
            m.start = strength;
            m.done = false;
            m.t = 0.0;
            if let Some(Value::Array(a)) = &curve {
                m.from_start = a.first().and_then(Value::as_f32) == Some(1.0);
            }
        }
        m
    }

    /// `82380500`: `two_bone_chains` names to bone indices.
    fn ik_chains(&self, items: &[(u32, Value)]) -> Vec<ik::Chain> {
        let Some(Value::Array(chains)) = lookup(items, qb_key("two_bone_chains")) else { return Vec::new() };
        chains
            .iter()
            .filter_map(|c| {
                let b = |n: &str| match c.get(qb_key(n)) {
                    Some(Value::Checksum(x)) => self.rig.bone(*x),
                    _ => None,
                };
                Some(ik::Chain { thigh: b("bone0")?, knee: b("bone1")?, ankle: b("bone2")?, target: b("bonetarget")? })
            })
            .collect()
    }
}

/// What the tree reads from the skater each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkaterInputs {
    pub flipped: bool,
}

impl Node {
    fn update(&mut self, mut dt: f32) {
        match &mut self.kind {
            Kind::Timer(t) => t.update(&mut dt),
            Kind::Modulate(m) => m.update(dt),
            Kind::SkaterModulate(m) => m.update(dt),
            Kind::DegenerateBlend(d) => {
                // 8237CAB8: fade out older branches; drop them when done.
                if self.children.len() >= 2 {
                    let step = if d.duration != 0.0 { dt.abs() / d.duration } else { 1.0 };
                    let mut k = 0;
                    while k + 1 < d.records.len() {
                        d.records[k].0 -= step;
                        if d.records[k].0 <= 0.0 {
                            d.records.remove(k);
                            self.children.remove(k);
                        } else {
                            k += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        for c in &mut self.children {
            c.update(dt);
        }
    }

    fn sample(&mut self, phase: f32, cx: &mut Sampler) -> Pose {
        let rig = cx.rig;
        let first = |n: &mut Node, cx: &mut Sampler| match n.children.first_mut() {
            Some(c) => c.sample(phase, cx),
            None => Pose::zero(rig),
        };
        match &mut self.kind {
            Kind::Source { anim } => match cx.lib.get(*anim) {
                Some(c) => Pose::from_clip(rig, &c, c.duration * phase),
                None => Pose::zero(rig),
            },
            Kind::Timer(t) => {
                let p = t.time / t.duration;
                match self.children.first_mut() {
                    Some(c) => c.sample(p, cx),
                    None => Pose::zero(rig),
                }
            }
            Kind::Add { wa, wb } => {
                let (wa, wb) = (*wa, *wb);
                let mut a =
                    if wa != 0.0 { self.children.first_mut().map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                let b = if wb != 0.0 { self.children.get_mut(1).map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                a.add(wa, wb, b);
                a
            }
            Kind::ApplyDifference { weight } => {
                let w = *weight;
                if w == 0.0 || self.children.len() < 2 {
                    return self.children.get_mut(1).map_or(Pose::zero(rig), |c| c.sample(phase, cx));
                }
                let diff = self.children[0].sample(phase, cx);
                let mut base = self.children[1].sample(phase, cx);
                base.apply_difference(w, &diff, rig);
                base
            }
            Kind::Modulate(m) => {
                let s = m.strength;
                if s == 0.0 {
                    return Pose::zero(rig);
                }
                let mut p = first(self, cx);
                p.scale(s);
                p
            }
            Kind::SkaterModulate(m) => {
                let s = m.strength;
                if s == 0.0 {
                    return Pose::zero(rig);
                }
                let mut p = first(self, cx);
                p.scale(s);
                p
            }
            Kind::Flip => {
                // 82377178 mirrors the pose when flipped: not translated.
                if cx.inputs.flipped && !cx.untranslated.contains(&qb_key("skaterflip")) {
                    cx.untranslated.push(qb_key("skaterflip"));
                }
                first(self, cx)
            }
            Kind::PassThrough => first(self, cx),
            Kind::PoseCapture { frozen } => {
                if let Some(p) = frozen {
                    return p.clone();
                }
                first(self, cx)
            }
            Kind::DegenerateBlend(d) => {
                // 8237C910.
                if self.children.is_empty() {
                    return Pose::zero(rig);
                }
                let mut out = self.children[0].sample(phase, cx);
                for k in 1..self.children.len() {
                    let w = d.records.get(k - 1).map_or(0.0, |(r, f)| f.eval(*r));
                    let next = self.children[k].sample(phase, cx);
                    out.blend(next, 1.0 - w, rig);
                }
                out
            }
            Kind::Ik { chains } => {
                let chains = chains.clone();
                let mut p = first(self, cx);
                p.flush(rig);
                solve_ik(&mut p, &chains, rig);
                p
            }
            Kind::Untranslated(_) => Pose::zero(rig),
        }
    }

    pub fn find_mut(&mut self, id: u32) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(id))
    }

    pub fn contains(&self, id: u32) -> bool {
        self.id == id || self.children.iter().any(|c| c.contains(id))
    }
}

/// Run the legs' IK on a pose (clips' convention in, out).
fn solve_ik(p: &mut Pose, chains: &[ik::Chain], rig: &Rig) {
    let conj = |q: [f32; 4]| [-q[0], -q[1], -q[2], q[3]];
    let mut locals: Vec<ik::Local> = p.q.iter().zip(&p.t).map(|(q, t)| ik::Local { rotation: conj(*q), translation: *t }).collect();
    for c in chains {
        ik::solve_chain(&mut locals, &rig.parents, *c);
    }
    for (i, l) in locals.iter().enumerate() {
        p.q[i] = conj(l.rotation);
        p.t[i] = l.translation;
    }
}

struct Sampler<'a> {
    rig: &'a Rig,
    lib: &'a mut ClipLib,
    inputs: SkaterInputs,
    untranslated: &'a mut Vec<u32>,
}

/// Finds which global a value is (see [`AnimTree::command_named`]).
pub type GlobalName<'a> = dyn Fn(&Value) -> Option<u32> + 'a;

/// A queued `skater_anim_command` (kept until the tree has its clips).
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub target: u32,
    pub command: u32,
    pub params: Params,
}

/// The skater's tree: the `body` blend node of `Skater_StaticAnimTree`.
/// APPROXIMATE: the static tree's other layers (wheel spin and face on
/// `applydifferencewithfacefixuphack`, ragdoll, cutscene overlay) are not
/// translated.
#[derive(Default)]
pub struct AnimTree {
    pub lib: Option<ClipLib>,
    pub rig: Rig,
    pub body: Option<Node>,
    pending: Vec<Command>,
    /// Node types met that are not translated.
    pub untranslated: Vec<u32>,
    /// The name of each branch added to `body`, newest last.
    pub branches: Vec<u32>,
}

impl AnimTree {
    /// Give the tree its clips and skeleton, then run queued commands.
    pub fn attach(&mut self, lib: ClipLib, rig: Rig, globals: &dyn Fn(u32) -> Option<Value>) {
        self.lib = Some(lib);
        self.rig = rig;
        self.body = Some(Node {
            id: qb_key("body"),
            kind: Kind::DegenerateBlend(DegenerateBlend { records: Vec::new(), duration: 0.0, next_duration: -1.0 }),
            children: Vec::new(),
        });
        for c in std::mem::take(&mut self.pending) {
            self.command(&c.target, c.command, &c.params, globals);
        }
    }

    /// `skater_anim_command` (`8237CFB8` for the blend node's commands).
    pub fn command(&mut self, target: &u32, command: u32, params: &Params, globals: &dyn Fn(u32) -> Option<Value>) -> bool {
        self.command_named(target, command, params, globals, None)
    }

    /// As [`Self::command`]; `named` finds a global's name from its value
    /// (the script VM passes `tree = ?4b X` as the struct itself), used only
    /// to report which branch was added.
    pub fn command_named(
        &mut self,
        target: &u32,
        command: u32,
        params: &Params,
        globals: &dyn Fn(u32) -> Option<Value>,
        named: Option<&GlobalName<'_>>,
    ) -> bool {
        let (Some(lib), Some(body)) = (self.lib.as_mut(), self.body.as_mut()) else {
            self.pending.push(Command { target: *target, command, params: params.clone() });
            return true;
        };
        let k = qb_key;
        let Some(node) = body.find_mut(*target) else { return false };
        match &mut node.kind {
            Kind::DegenerateBlend(d) if command == k("degenerateblend_addbranch") => {
                // 8237CFB8 reads `tree`, `params`, `blendduration`,
                // `blendfunction`; 8237CCA8 adds the branch.
                let tree = match params.get(k("tree")) {
                    Some(Value::Checksum(c)) => globals(*c),
                    Some(v @ Value::Struct(_)) => Some(v.clone()),
                    _ => None,
                };
                let Some(tree) = tree else { return false };
                let branch_name = named.and_then(|f| f(&tree));
                let scope = params.get(k("params")).map(|v| Params(flatten(v, globals, 0))).unwrap_or_default();
                let duration = params.float(k("blendduration")).unwrap_or(-1.0);
                let func = params.checksum(k("blendfunction")).unwrap_or(0);
                let mut b = Build { globals, lib, rig: &self.rig, types: &mut self.untranslated };
                let branch = b.node(&tree, &scope);
                if let Some(last) = d.records.last_mut() {
                    last.1 = BlendFn::new(func, None);
                }
                node.children.push(branch);
                d.records.push((1.0, BlendFn::Linear));
                d.duration = if d.next_duration != -1.0 {
                    let n = d.next_duration;
                    d.next_duration = -1.0;
                    n
                } else if duration == -1.0 {
                    0.3 // 8237CE28: +24, set to 0.3 by the constructor 82377B40
                } else {
                    duration
                };
                if d.duration == 0.0 && node.children.len() >= 2 {
                    let n = node.children.len();
                    node.children.drain(..n - 1);
                    d.records.drain(..n - 1);
                }
                if *target == k("body") {
                    let name = match params.get(k("tree")) {
                        Some(Value::Checksum(c)) => *c,
                        _ => 0,
                    };
                    self.branches.push(if name != 0 { name } else { branch_name.unwrap_or(0) });
                    if self.branches.len() > 8 {
                        self.branches.remove(0);
                    }
                }
                true
            }
            Kind::DegenerateBlend(d) if command == k("degenerateblend_setnextblendduration") => {
                d.next_duration = params.float(k("blendduration")).unwrap_or(-1.0);
                true
            }
            Kind::Modulate(m) if command == k("modulate_setstrength") => {
                // Handler not read: sets the strength and stops any blend
                // (LIKELY).
                if let Some(s) = params.float(k("strength")) {
                    m.strength = s;
                    m.done = true;
                }
                true
            }
            Kind::PoseCapture { .. } if command == k("posecapture_capture") => {
                // Capturing needs the last sampled pose; not kept yet, so
                // the node stays live (APPROXIMATE).
                true
            }
            _ => false,
        }
    }

    /// `Skater_AnimNodeExists`.
    pub fn node_exists(&self, id: u32) -> bool {
        self.body.as_ref().is_some_and(|b| b.contains(id))
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(b) = self.body.as_mut() {
            b.update(dt);
        }
    }

    /// The body pose, or `None` before any branch was added.
    pub fn sample(&mut self, inputs: SkaterInputs) -> Option<Pose> {
        let (Some(lib), Some(body)) = (self.lib.as_mut(), self.body.as_mut()) else { return None };
        if body.children.is_empty() {
            return None;
        }
        let mut cx = Sampler { rig: &self.rig, lib, inputs, untranslated: &mut self.untranslated };
        let mut p = body.sample(0.0, &mut cx);
        p.flush(&self.rig);
        Some(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(n: usize) -> Rig {
        Rig { names: (0..n as u32).collect(), parents: vec![None; n], bind_q: vec![[0.0, 0.0, 0.0, 1.0]; n], bind_t: vec![[0.0; 3]; n] }
    }

    fn pose(q: [f32; 4], t: [f32; 3], w: f32) -> Pose {
        Pose { q: vec![q], t: vec![t], w: vec![w], strength: 1.0 }
    }

    #[test]
    fn curve_function_stores_one_minus_values() {
        // Stopped_AnimBranch: blendcurve [1, 0] fades in linearly.
        let f = BlendFn::new(qb_key("curve"), Some(&Value::Array(vec![Value::Float(1.0), Value::Float(0.0)])));
        assert_eq!(f.eval(0.0), 0.0);
        assert_eq!(f.eval(0.25), 0.25);
        assert_eq!(f.eval(1.0), 1.0);
        assert_eq!(BlendFn::Smooth.eval(0.5), 0.5);
    }

    #[test]
    fn add_multiplies_rotations_and_sums_positions() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let mut a = pose([s, 0.0, 0.0, s], [1.0, 0.0, 0.0], 1.0);
        let b = pose([0.0, s, 0.0, s], [0.0, 2.0, 0.0], 1.0);
        a.add(1.0, 1.0, b);
        assert_eq!(a.q[0], qmul([s, 0.0, 0.0, s], [0.0, s, 0.0, s]));
        assert_eq!(a.t[0], [1.0, 2.0, 0.0]);
        // Half weight on B: halfway from identity.
        let mut a = pose([0.0, 0.0, 0.0, 1.0], [0.0; 3], 1.0);
        a.add(1.0, 0.5, pose([0.0, 1.0, 0.0, 0.0], [0.0, 2.0, 0.0], 1.0));
        assert!((a.q[0][1] - s).abs() < 1e-6 && a.t[0] == [0.0, 1.0, 0.0]);
    }

    #[test]
    fn blend_weights_follow_retail() {
        let r = rig(1);
        let mut a = pose([0.0, 0.0, 0.0, 1.0], [0.0; 3], 1.0);
        a.blend(pose([0.0, 0.0, 0.0, 1.0], [2.0, 0.0, 0.0], 1.0), 0.25, &r);
        assert_eq!(a.t[0], [0.5, 0.0, 0.0]);
        // A has no weight: B is taken whole (u = 1) with its weight.
        let mut a = pose([0.0, 0.0, 0.0, 1.0], [0.0; 3], 0.0);
        a.blend(pose([0.0, 0.0, 0.0, 1.0], [2.0, 0.0, 0.0], 1.0), 0.25, &r);
        assert_eq!((a.t[0], a.w[0]), ([2.0, 0.0, 0.0], 0.25));
    }

    #[test]
    fn flush_folds_strength_and_zero_restores_rest() {
        let r = rig(1);
        let mut a = pose([0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0);
        a.scale(0.5);
        a.flush(&r);
        assert_eq!((a.w[0], a.strength), (0.5, 1.0));
        a.scale(0.0);
        a.flush(&r);
        assert_eq!(a, Pose { q: vec![[0.0, 0.0, 0.0, 1.0]], t: vec![[0.0; 3]], w: vec![0.0], strength: 1.0 });
    }

    #[test]
    fn timers_loop_or_clamp() {
        let mut t = Timer { looping: true, time: 0.9, speed: 1.0, duration: 1.0, end: 1.0, finished: false };
        let mut dt = 0.2;
        t.update(&mut dt);
        assert!((t.time - 0.1).abs() < 1e-6 && t.finished);
        let mut p = Timer { looping: false, ..t.clone() };
        p.time = 0.9;
        p.update(&mut 0.2);
        assert_eq!(p.time, 1.0);
    }
}
