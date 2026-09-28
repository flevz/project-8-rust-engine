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
//! - `skatermodulate` (init `820B2D00`, update `820B2790`, sample
//!   `820B3288`, finish `820B26C0`): offwhenfinished, turn, slope, speed,
//!   brake, vert, crouch, board, time/play. Other types (grindlean, nollie,
//!   spin, ...) are not translated (they rise over blendtime).
//! - The rolling nodes: `skatertimer` (crouch, jump, cycle, play), `speedblend`,
//!   `crouchblend`, `ubercrouchblend`, `kicktimer`/`kickcatch`,
//!   `braketimer`/`brakecatch`, `skatertimedswitch`; and the air nodes:
//!   `blank`, `partialswitch`, `apextimer`, `takeoffblend`,
//!   `ollielandblend`, `spinleftrighttimer`, `spinleftrightadd` (addresses
//!   at each type).
//! - Inputs are the animinfo component's copies of skater values
//!   (`820B8EA0`, [`SkaterInputs`]). Slopes use the physics matrix where
//!   retail uses the model's display matrix (APPROXIMATE).
//! - The board object samples the same nodes with each clip's `_b`
//!   counterpart (see `AnimTree::sample_board`).
//!
//! Not translated (listed in `untranslated`; APPROXIMATE stand-ins): other
//! node types; with children they pass their first child through, leaves
//! give an empty pose. Anim events (`8237C178`/`8237C380`, e.g. the kick's
//! `KickBoostEvent`) are not fired yet.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use p8_formats::anim::{self, Clip, qmul};
use p8_formats::ik;
use p8_formats::qb::Value;
use p8_formats::{pak, qb_key, qb_key_extend};
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

    /// Weighted blend (`82376EA8` + op `823764D8`), used by `crouchblend`,
    /// `skatertimedswitch`: per bone b = B weight x B strength x `w`; a bone
    /// A lacks is taken from B; weight becomes 1.
    pub fn blend_weighted(&mut self, b: &Pose, w: f32, rig: &Rig) {
        let f = b.strength * w;
        if f == 0.0 {
            return;
        }
        self.flush(rig);
        for i in 0..self.q.len() {
            let bw = b.w[i] * f;
            if bw == 0.0 {
                continue;
            }
            if self.w[i] == 0.0 {
                self.q[i] = b.q[i];
                self.t[i] = b.t[i];
            } else {
                self.q[i] = anim::nlerp(self.q[i], b.q[i], bw);
                let (ta, tb) = (self.t[i], b.t[i]);
                self.t[i] = [ta[0] + (tb[0] - ta[0]) * bw, ta[1] + (tb[1] - ta[1]) * bw, ta[2] + (tb[2] - ta[2]) * bw];
            }
            self.w[i] = 1.0;
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
    /// `8237BC40` (init `8237BD20`): (e^(kt) - 1) / (e^k - 1), k =
    /// `blendcurvature` (default 5).
    Exponential(f32),
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
        } else if name == qb_key("exponential") {
            BlendFn::Exponential(5.0)
        } else {
            // 0 and `linear`.
            BlendFn::Linear
        }
    }

    fn eval(&self, t: f32) -> f32 {
        match self {
            BlendFn::Linear => t,
            BlendFn::Smooth => (t - 1.5) * t * t * -2.0,
            BlendFn::Exponential(k) => ((t * k).exp() - 1.0) / (k.exp() - 1.0),
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
    /// `+48`
    timertype: u32,
    /// `+52`
    invert: bool,
    /// `+56`
    min: f32,
    /// `+32` value before `820B26C0`
    x: f32,
    /// `+36` previous value
    prev: f32,
    /// `+40` (blendtime, or the clip length for offwhenfinished)
    blendtime: f32,
    /// `+44` (blendtime2)
    blendtime2: f32,
    /// `+76` (speed)
    speed: f32,
    /// `+68` measured speed, `+72` its largest change per frame (0.5)
    speed_now: f32,
    /// `+80` (max_slope, default 90)
    max_slope: f32,
    /// `+84` / `+88` (slope_dir / turn_dir)
    dir: u32,
    /// `+96`: flipped (SkaterState `+40`) when built, toggled by `mirror`
    flip: bool,
    /// `+97` (dont_flip)
    dont_flip: bool,
    /// `+98`: first frame (no smoothing)
    first: bool,
    /// `+92` (delay_anim length)
    delay: f32,
    /// `+24`
    func: Option<BlendFn>,
    /// `+28`
    pub strength: f32,
}

/// Angle in degrees between two directions (`821ED9E8`).
fn angle_deg(a: [f32; 3], b: [f32; 3]) -> f32 {
    let n = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l == 0.0 { v } else { v.map(|c| c / l) }
    };
    let (a, b) = (n(a), n(b));
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

fn flat(v: [f32; 3]) -> [f32; 3] {
    [v[0], 0.0, v[2]]
}

impl SkaterModulate {
    /// `820B26C0`: clamp, blend function, invert, minimum.
    fn finish(&mut self) {
        let x = self.x.clamp(0.0, 1.0);
        self.x = x;
        let mut s = self.func.as_ref().map_or(x, |f| f.eval(x));
        if self.invert {
            s = 1.0 - s;
        }
        if self.min > 0.0 {
            s = self.min + (1.0 - self.min) * s;
        }
        self.strength = s;
    }

    /// `820B2790`.
    fn update(&mut self, dt: f32, i: &SkaterInputs) {
        let k = qb_key;
        if self.delay > 0.0 {
            self.delay -= dt;
            return;
        }
        let t = self.timertype;
        if t == k("board") || t == k("slope") || t == k("speed") {
            return; // computed when sampled (820B3288)
        }
        if t == k("offwhenfinished") {
            // 28C4: count to the clip length, then off (on if inverted).
            self.x += dt;
            if self.x >= self.blendtime {
                self.strength = if self.invert { 1.0 } else { 0.0 };
            }
            return;
        }
        if t == k("crouch") {
            // 2834: rises while crouched, never falls.
            self.x = self.prev;
            if i.crouched && self.x < 1.0 {
                self.x = (self.x + dt / self.blendtime).min(1.0);
            }
            self.prev = self.x;
        } else if t == k("vert") {
            // 2938: towards 1 by 0.1 a frame in vert air or on vert ground.
            if i.in_vert_air || i.on_vert_ground {
                self.x = (self.x + 0.1).min(1.0);
            } else {
                self.x = (self.x - 0.1).max(0.0);
            }
        } else if t == k("brake") {
            // 2B3C: follow the brake amount while the brake input is held.
            self.x = self.prev;
            if i.brake_input {
                if self.x < i.brake_amount {
                    self.x = (self.x + dt / self.blendtime).min(i.brake_amount);
                } else {
                    self.x = i.brake_amount;
                }
            } else if self.x > 0.0 {
                self.x = (self.x - dt / self.blendtime).max(0.0);
            }
            self.prev = self.x;
        } else if t == k("turn") {
            // 29F4: the turn amount (+1944) on this side, eased in over
            // blendtime and out over blendtime2.
            let a = i.turn;
            let right = self.dir == k("right");
            let left = self.dir == k("left");
            let mut x = if self.dir == k("any") {
                a.abs()
            } else if (right && !self.flip) || (left && self.flip) {
                a.max(0.0)
            } else if right || left {
                (-a).max(0.0)
            } else {
                a
            };
            if self.first {
                self.first = false;
            } else {
                let rate = if x >= self.prev { dt / self.blendtime } else { dt / self.blendtime2 };
                if x - self.prev > rate {
                    x = self.prev + rate;
                } else if self.prev - x > rate {
                    x = self.prev - rate;
                }
            }
            self.x = x;
            self.prev = x;
        } else if self.x < 1.0 {
            // 2BF4 (time, play, ...): rise over blendtime.
            self.x += dt / self.blendtime;
        }
        self.finish();
    }

    /// The part of `820B3288` that runs when sampled (slope, speed, board).
    fn sample_value(&mut self, i: &SkaterInputs) {
        let k = qb_key;
        let t = self.timertype;
        if t == k("board") {
            self.strength = 1.0;
        } else if t == k("speed") {
            if i.in_vert_air {
                self.x = 1.0;
            } else {
                let mut s = (i.velocity[0] * i.velocity[0] + i.velocity[2] * i.velocity[2]).sqrt();
                if self.first {
                    self.first = false;
                } else if (s - self.speed_now).abs() > 0.5 {
                    s = if s > self.speed_now { self.speed_now + 0.5 } else { self.speed_now - 0.5 };
                }
                self.speed_now = s;
                self.x = if s <= 0.0 {
                    0.0
                } else if s >= self.speed {
                    1.0
                } else {
                    s / self.speed
                };
            }
            self.finish();
        } else if t == k("slope") {
            // Retail reads the model's display matrix (animinfo +272 right,
            // +288 up, +304 at); the physics matrix stands in (APPROXIMATE:
            // the display smoothing is not translated).
            let (r, u, a) = (i.right, i.up, i.at);
            let flipped = self.flip && !self.dont_flip;
            let d = self.dir;
            let mut ang = 0.0;
            if u[1] > -0.01 {
                if d == k("right") {
                    if (flipped && r[1] < 0.0) || (!flipped && r[1] > 0.0) {
                        ang = angle_deg(r, flat(r));
                    }
                } else if d == k("left") {
                    if (flipped && r[1] > 0.0) || (!flipped && r[1] < 0.0) {
                        ang = angle_deg(r, flat(r));
                    }
                } else if d == k("forward") {
                    if a[1] < 0.0 {
                        ang = angle_deg(a, flat(a));
                    }
                } else if d == k("back") {
                    if a[1] > 0.0 {
                        ang = angle_deg(a, flat(a));
                    }
                } else if d == k("any") {
                    ang = angle_deg([0.0, 1.0, 0.0], u).abs();
                }
            }
            self.x = if self.max_slope == 0.0 { 0.0 } else { ang / self.max_slope };
            self.finish();
        }
    }
}

/// `skatertimer` (init `820B3D08`, update `820B4038`, sample `823864B0`).
#[derive(Clone, Debug, PartialEq)]
pub struct SkaterTimer {
    timertype: u32,
    /// `+20`
    time: f32,
    /// `+28`
    speed: f32,
    /// `+32` clip length
    duration: f32,
    /// `+36`
    end: f32,
    /// `+84` (cycle)
    cycle: bool,
    /// `+85`: crouched when last seen
    crouched: bool,
    /// `+86`: the jump has started
    started: bool,
    /// `+100`
    delay: f32,
}

impl SkaterTimer {
    fn update(&mut self, dt: &mut f32, i: &SkaterInputs) {
        let k = qb_key;
        if self.delay > 0.0 {
            self.delay -= *dt;
            return;
        }
        *dt *= self.speed;
        let t = self.timertype;
        if t == k("crouch") {
            // 42F8: waits for the crouch, then plays.
            if self.crouched {
                self.time += *dt;
            } else {
                self.crouched = i.crouched;
            }
        } else if t == k("cycle") || t == k("play") {
            self.time += *dt;
        } else if t == k("jump") {
            // 41A8: starts when the crouch is released, runs while in the
            // air (SkaterState +24 == 1).
            if self.started {
                if i.in_air {
                    self.time += *dt;
                }
            } else {
                let was = self.crouched;
                self.crouched = i.crouched;
                if was && !self.crouched {
                    self.started = true;
                }
            }
        }
        // Other timer types (turn, spin, brake, grab...): not translated.
        if self.time < 0.0 {
            self.time = 0.0;
        } else if self.time > self.end {
            self.time = if t == k("cycle") || self.cycle { 0.0 } else { self.end };
        }
    }
}

/// `kicktimer` (init `820A6EB0`, update `820A70E8`, sample `820A7390`) and
/// `braketimer` (init `820A34D8`, update `820A3718`, sample `820A39B0`).
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseTimer {
    brake: bool,
    /// `+20`
    time: f32,
    /// `+76`
    phase: usize,
    /// `+96` (kick) / `+92` (brake): progress through the phase's clip
    progress: f32,
    /// `+100` (kick): smoothed speed; the brake timer measures it fresh
    speed_now: f32,
    /// `+108.. / +124..` (kick), `+108.. / +120..` (brake)
    slow: Vec<f32>,
    fast: Vec<f32>,
    slow_speed: f32,
    fast_speed: f32,
    min_speed: f32,
    max_speed: f32,
    /// Brake only: `+96` idle progress, `+100` idle weight, `+104`
    /// idle_out_time.
    idle_progress: f32,
    idle_weight: f32,
    idle_out_time: f32,
}

impl PhaseTimer {
    fn length(&self, speed: f32) -> f32 {
        let s = self.slow.get(self.phase).copied().unwrap_or(1.0) / self.slow_speed;
        let f = self.fast.get(self.phase).copied().unwrap_or(1.0) / self.fast_speed;
        if speed <= self.min_speed {
            s
        } else if speed < self.max_speed {
            f + (1.0 - (speed - self.min_speed) / (self.max_speed - self.min_speed)) * (s - f)
        } else {
            f
        }
    }

    fn speed(i: &SkaterInputs) -> f32 {
        let v = i.velocity;
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    fn update(&mut self, dt: f32, i: &SkaterInputs) {
        if self.brake { self.update_brake(dt, i) } else { self.update_kick(dt, i) }
    }

    fn update_kick(&mut self, dt: f32, i: &SkaterInputs) {
        if i.brake_input {
            self.time = 0.0;
            self.progress = 0.0;
            self.phase = 0;
            return;
        }
        if self.phase == 0 && self.time == 0.0 && !i.kick {
            return;
        }
        self.time += dt;
        let s = Self::speed(i);
        let d = s - self.speed_now;
        self.speed_now = if d.abs() > 0.5 { if d > 0.0 { self.speed_now + 0.5 } else { self.speed_now - 0.5 } } else { s };
        self.progress = self.time / self.length(self.speed_now);
        if self.progress >= 1.0 {
            self.progress = 0.0;
            self.time = 0.0;
            // in -> kick; kick -> reset (kick held) or out; reset -> kick;
            // out -> idle.
            self.phase = match self.phase {
                0 | 2 => 1,
                1 => {
                    if i.kick {
                        2
                    } else {
                        3
                    }
                }
                _ => 0,
            };
        }
    }

    fn update_brake(&mut self, dt: f32, i: &SkaterInputs) {
        if i.kick {
            self.progress = 0.0;
            self.phase = 0;
            self.idle_progress = 0.0;
            self.time = 0.0;
            self.idle_weight = 1.0;
            return;
        }
        if self.phase == 0 && self.time == 0.0 && !i.brake_input {
            return;
        }
        self.time += dt;
        self.progress = self.time / self.length(Self::speed(i));
        self.idle_progress = (self.idle_progress + dt / self.slow.get(2).copied().unwrap_or(1.0)).min(1.0);
        if self.progress >= 1.0 {
            if self.phase == 0 {
                if i.brake_input {
                    self.progress = 1.0; // hold the end of brake-in
                } else {
                    self.phase = 1;
                    self.progress = 0.0;
                    self.time = 0.0;
                }
            } else if self.phase == 1 {
                self.phase = 0;
                self.progress = 0.0;
                self.idle_progress = 0.0;
                self.time = 0.0;
            }
        } else if self.phase == 1 {
            self.idle_weight = (1.0 - self.time / self.idle_out_time).min(1.0);
        }
    }
}

/// `kickcatch` / `brakecatch` (init `820A3368`, updates `820A6E20` /
/// `820A3448`, sample `820A33E0` -> `820B0380`): fade the branch out over
/// `blendtime` from the pose captured when the brake (kick) starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Catch {
    on_brake: bool,
    /// `+48`
    strength: f32,
    /// `+52` (blendtime, default 0.1)
    blendtime: f32,
    /// `+21`
    latched: bool,
    /// `+44`
    done: bool,
    captured: Option<Pose>,
    captured_board: Option<Pose>,
}

impl Catch {
    fn update(&mut self, dt: f32, i: &SkaterInputs) {
        let trigger = if self.on_brake { i.brake_input } else { i.kick };
        if !trigger {
            self.strength = 1.0;
            self.latched = false;
            return;
        }
        if !self.latched {
            self.latched = true;
            self.strength = 1.0;
            self.done = false;
        }
        if !self.done {
            self.strength -= dt / self.blendtime;
            if self.strength <= 0.0 {
                self.strength = 0.0;
                self.done = true;
            }
        }
    }
}

/// `ollielandblend` (init `820A8050`, update `820A7D88`): blends the
/// landing child in as the predicted landing approaches.
#[derive(Clone, Debug, PartialEq)]
pub struct OllieLand {
    /// `+20`
    w: f32,
    func: Option<BlendFn>,
    /// `+28` blendintime (also the prediction horizon), `+32` the blend
    /// time in use, `+36` maxblendtime, `+40` blendthreshold, `+44`
    /// holdtime
    blendintime: f32,
    blend: f32,
    maxblendtime: f32,
    threshold: f32,
    hold: f32,
    /// `+52` seconds to landing, `+56` started, `+60` t, `+64` time
    to_land: f32,
    started: bool,
    t: f32,
    time: f32,
    /// `+68` / `+72` highdropstartblendtime / highdropendblendtime
    high_drop: Option<(f32, f32)>,
}

impl OllieLand {
    fn update(&mut self, dt: f32, i: &SkaterInputs) {
        if !i.in_air {
            return;
        }
        // The prediction with this node's horizon (`+28`): slice k counts
        // only if k == 1 or (k - 1) x 0.05 < horizon (820E2CD8's loop).
        let k = i.time_to_land_slice;
        // (retail sums 0.05 in single precision).
        let t_prev = (1..k).fold(0.0f32, |t, _| t + 0.05);
        let land = if k != 0 && (k == 1 || t_prev < self.blendintime) { i.time_to_land } else { -1.0 };
        if !self.started {
            self.to_land = land;
            if land != -1.0 {
                self.started = true;
                self.blend = self.blendintime;
                let f = (land - i.time_to_apex).min(self.threshold) / self.threshold;
                self.blend = (self.blend + (self.maxblendtime - self.blend) * f).min(land);
            }
        } else {
            self.to_land = land;
        }
        let old = self.t;
        if self.to_land == -1.0 {
            self.started = false;
            if self.t > 0.0 {
                self.t = (self.t - dt * 10.0).max(0.0);
            }
        } else if self.started {
            if self.to_land > self.blend + self.hold {
                self.to_land = (self.to_land - dt).max(0.0);
                if self.t > 0.0 {
                    self.t -= dt * 10.0;
                }
            } else if self.t < 1.0 {
                self.t = (self.t + dt / self.blend).min(1.0);
            }
        }
        if self.t != old {
            if old - self.t > 0.1 {
                self.t = old - 0.1;
            }
            self.t = self.t.clamp(0.0, 1.0);
        }
        self.w = self.func.as_ref().map_or(self.t, |f| f.eval(self.t));
        if let Some((a, b)) = self.high_drop {
            let f = if b - a != 0.0 { ((self.time - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
            if f > self.w {
                self.w = f;
            }
        }
        self.time += dt;
    }
}

/// `crouchblend` (init `820A3BB8`, update `820A3AC0`) and the crouch part of
/// `ubercrouchblend` (init `820B6008`, update `820B5E88`).
#[derive(Clone, Debug, PartialEq)]
pub struct CrouchWeight {
    /// `+36`
    t: f32,
    /// `+32` (blendintime)
    blendintime: f32,
    /// `+44`
    delay: f32,
    /// `+40` (dont_update)
    frozen: bool,
    func: BlendFn,
    /// `+20` / `+28`: the weight
    pub w: f32,
}

impl CrouchWeight {
    fn update(&mut self, dt: f32, i: &SkaterInputs) {
        if self.frozen {
            return;
        }
        if self.delay > 0.0 {
            self.delay -= dt;
            return;
        }
        if i.crouched && self.t < 1.0 {
            self.t = if self.blendintime > 0.0 { (self.t + dt / self.blendintime).min(1.0) } else { 1.0 };
        }
        self.w = self.func.eval(self.t);
    }
}

/// `ubercrouchblend` (sample `820B6530`): four clips, stand/crouch x
/// slow/fast, played at the parent timer's progress.
#[derive(Clone, Debug, PartialEq)]
pub struct UberCrouch {
    /// stand_slow, stand_fast, crouch_slow, crouch_fast
    clips: [u32; 4],
    crouch: CrouchWeight,
    /// `+56` speed_min, `+60` speed, `+64` measured speed, `+72` 0.5
    speed_min: f32,
    speed: f32,
    speed_now: f32,
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
    /// `+20` a pose was kept; the kept pose (`823772D0` stores each live
    /// sample). `posecapture_capture` (`820B38F0`) deletes the live child,
    /// after which the kept pose is returned (`820B37F0`).
    PoseCapture {
        kept: Option<Pose>,
        kept_board: Option<Pose>,
    },
    DegenerateBlend(DegenerateBlend),
    SkaterTimer(SkaterTimer),
    /// `speedblend` (init `820B4618`, sample `820B47B0`): `+32` speed_min,
    /// `+36` speed, `+40` measured speed (moves at most 0.5 a frame).
    SpeedBlend {
        speed_min: f32,
        speed: f32,
        speed_now: f32,
    },
    CrouchBlend(CrouchWeight),
    UberCrouch(UberCrouch),
    PhaseTimer(PhaseTimer),
    Catch(Catch),
    /// `blank` (sample `8237B608`): an empty pose.
    Blank,
    /// `partialswitch` (init `8237B610`, update `8237B7C8`, sample
    /// `8237B878`, commands `8237B698`): `+20` state (0 for `on`), `+28`
    /// weight, `+24` blend time (0 until `partialswitch_setstate` sets its
    /// `blendduration`, default 0.3).
    PartialSwitch {
        on: bool,
        w: f32,
        duration: f32,
    },
    /// `apextimer` (init `820A20E0`, update `820A2188`): plays its clip so
    /// that the middle falls on the top of the jump.
    ApexTimer {
        time: f32,
        duration: f32,
        started: bool,
        /// `+80` seconds to the apex
        to_apex: f32,
    },
    /// `takeoffblend` (init `820B5950`, update `820B5890`): `+36` rises from
    /// `strength` over `blendintime`; weight = blend function.
    TakeoffBlend {
        t: f32,
        blendintime: f32,
        func: Option<BlendFn>,
        w: f32,
    },
    OllieLand(OllieLand),
    /// `spinleftrighttimer` (init `820B53C8`, update `820B5460`): the clip
    /// time follows the spin, -180..180 degrees over the clip.
    SpinTimer {
        time: f32,
        duration: f32,
    },
    /// `spinleftrightadd` (init `820B5100`, update `820B51A8`, sample
    /// `820B52F0`): left and right weights that ease in (`induration`) and
    /// out (`outduration`) with the spin direction.
    SpinAdd {
        cur: f32,
        prev: f32,
        wl: f32,
        wr: f32,
        induration: f32,
        outduration: f32,
    },
    /// `skatertimedswitch` (init `820B3B50`, update `820B3C10`): `+20`
    /// weight (from `start`), `+28` the landing clip's length, `+32` time.
    TimedSwitch {
        w: f32,
        length: f32,
        time: f32,
    },
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
    /// Skater state when the branch is built (read by several inits).
    crouched0: bool,
    flipped0: bool,
    vert0: bool,
    speed0: f32,
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
            // 82375378: both weights default to 1; their params (keys
            // 0x9D5E2C7C, 0x1A524142, names unknown) appear in no script
            // tree, so 1 is what every tree gets.
            let wa = lookup(&items, 0x9D5E_2C7C).and_then(Value::as_f32).unwrap_or(1.0);
            let wb = lookup(&items, 0x1A52_4142).and_then(Value::as_f32).unwrap_or(1.0);
            Kind::Add { wa, wb }
        } else if ty == k("applydifference") {
            // 820A2390: the weight `+20` is `strength`, default 1.
            Kind::ApplyDifference { weight: self.float(&items, "strength", scope).unwrap_or(1.0) }
        } else if ty == k("modulate") {
            Kind::Modulate(self.modulate(&items, scope))
        } else if ty == k("skatermodulate") {
            Kind::SkaterModulate(self.skater_modulate(&items, scope))
        } else if ty == k("skaterflip") {
            Kind::Flip
        } else if ty == k("boardrotateoverlay") {
            Kind::PassThrough
        } else if ty == k("skaterposecapture") {
            Kind::PoseCapture { kept: None, kept_board: None }
        } else if ty == k("degenerateblend") {
            Kind::DegenerateBlend(DegenerateBlend { records: Vec::new(), duration: 0.0, next_duration: -1.0 })
        } else if ty == k("ik") {
            Kind::Ik { chains: self.ik_chains(&items) }
        } else if ty == k("skatertimer") {
            Kind::SkaterTimer(self.skater_timer(&items, scope))
        } else if ty == k("speedblend") {
            Kind::SpeedBlend {
                speed_min: self.float(&items, "speed_min", scope).unwrap_or(0.0),
                speed: self.float(&items, "speed", scope).unwrap_or(0.0),
                speed_now: self.speed0,
            }
        } else if ty == k("crouchblend") {
            Kind::CrouchBlend(self.crouch_weight(&items, scope, 0.0))
        } else if ty == k("ubercrouchblend") {
            let st = match lookup(&items, k("anim_struct")).map(|v| self.resolve(v, scope)) {
                Some(Value::Checksum(c)) => (self.globals)(c),
                Some(v @ Value::Struct(_)) => Some(v),
                _ => None,
            };
            let clip = |n: &str| match st.as_ref().and_then(|v| v.get(k(n))) {
                Some(Value::Checksum(c)) => *c,
                _ => 0,
            };
            Kind::UberCrouch(UberCrouch {
                clips: [clip("stand_slow"), clip("stand_fast"), clip("crouch_slow"), clip("crouch_fast")],
                crouch: self.crouch_weight(&items, scope, 0.3),
                speed_min: self.float(&items, "speed_min", scope).unwrap_or(0.0),
                speed: self.float(&items, "speed", scope).unwrap_or(0.0),
                speed_now: self.speed0,
            })
        } else if ty == k("kicktimer") || ty == k("braketimer") {
            Kind::PhaseTimer(self.phase_timer(&items, scope, ty == k("braketimer")))
        } else if ty == k("kickcatch") || ty == k("brakecatch") {
            Kind::Catch(Catch {
                on_brake: ty == k("kickcatch"),
                strength: 1.0,
                blendtime: self.float(&items, "blendtime", scope).unwrap_or(0.1),
                latched: false,
                done: false,
                captured: None,
                captured_board: None,
            })
        } else if ty == k("blank") {
            Kind::Blank
        } else if ty == k("partialswitch") {
            let on = self.checksum(&items, "state", scope) == Some(k("on"));
            Kind::PartialSwitch { on, w: if on { 0.0 } else { 1.0 }, duration: 0.0 }
        } else if ty == k("apextimer") {
            let duration = self.checksum(&items, "anim", scope).map_or(0.0, |a| self.lib.duration(a));
            Kind::ApexTimer { time: 0.0, duration, started: false, to_apex: 1.0 }
        } else if ty == k("takeoffblend") {
            let func = self
                .checksum(&items, "blendfunction", scope)
                .map(|f| BlendFn::new(f, lookup(&items, k("blendcurve")).map(|v| self.resolve(v, scope)).as_ref()));
            Kind::TakeoffBlend {
                t: self.float(&items, "strength", scope).unwrap_or(0.0),
                blendintime: self.float(&items, "blendintime", scope).unwrap_or(0.0),
                func,
                w: 0.0,
            }
        } else if ty == k("ollielandblend") {
            let blendintime = self.float(&items, "blendintime", scope).unwrap_or(0.0);
            let func = self
                .checksum(&items, "blendfunction", scope)
                .map(|f| BlendFn::new(f, lookup(&items, k("blendcurve")).map(|v| self.resolve(v, scope)).as_ref()));
            let hs = self.float(&items, "highdropstartblendtime", scope);
            let he = self.float(&items, "highdropendblendtime", scope);
            Kind::OllieLand(OllieLand {
                w: 0.0,
                func,
                blendintime,
                blend: blendintime,
                maxblendtime: self.float(&items, "maxblendtime", scope).unwrap_or(blendintime),
                threshold: self.float(&items, "blendthreshold", scope).unwrap_or(1.0),
                hold: self.float(&items, "holdtime", scope).unwrap_or(0.0),
                to_land: 1.0,
                started: false,
                t: 0.0,
                time: 0.0,
                high_drop: if hs.is_some() || he.is_some() { Some((hs.unwrap_or(9999.9), he.unwrap_or(9999.9))) } else { None },
            })
        } else if ty == k("spinleftrighttimer") {
            let duration = self.checksum(&items, "anim", scope).map_or(0.0, |a| self.lib.duration(a));
            Kind::SpinTimer { time: 0.0, duration }
        } else if ty == k("spinleftrightadd") {
            Kind::SpinAdd {
                cur: 0.0,
                prev: 0.0,
                wl: 0.0,
                wr: 0.0,
                induration: self.float(&items, "induration", scope).unwrap_or(1.0),
                outduration: self.float(&items, "outduration", scope).unwrap_or(1.0),
            }
        } else if ty == k("skatertimedswitch") {
            let anim = self.checksum(&items, if self.crouched0 { "crouchanim" } else { "standanim" }, scope).unwrap_or(0);
            Kind::TimedSwitch { w: self.float(&items, "start", scope).unwrap_or(0.0), length: self.lib.duration(anim), time: 0.0 }
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
        // `speed` (0xF0D90109, `607C`), default 1.
        let mut speed = lookup(items, qb_key("speed")).map(|v| self.resolve(v, scope)).and_then(|v| v.as_f32()).unwrap_or(1.0);
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

    /// `820B2D00`.
    fn skater_modulate(&mut self, items: &[(u32, Value)], scope: &Params) -> SkaterModulate {
        let k = qb_key;
        let timertype = self.checksum(items, "timertype", scope).unwrap_or(k("play"));
        let flag = |b: &Self, n: &str| matches!(lookup(items, k(n)).map(|v| b.resolve(v, scope)), Some(Value::Int(1)));
        let dir = if timertype == k("turn") { "turn_dir" } else { "slope_dir" };
        let mut m = SkaterModulate {
            timertype,
            invert: flag(self, "invert"),
            min: self.float(items, "min", scope).unwrap_or(0.0),
            x: 0.0,
            prev: 0.0,
            blendtime: self.float(items, "blendtime", scope).unwrap_or(1.0),
            blendtime2: self.float(items, "blendtime2", scope).unwrap_or(1.0),
            speed: self.float(items, "speed", scope).unwrap_or(1.0),
            speed_now: self.speed0,
            max_slope: self.float(items, "max_slope", scope).unwrap_or(90.0),
            dir: self.checksum(items, dir, scope).unwrap_or(0),
            flip: self.flipped0 ^ flag(self, "mirror"),
            dont_flip: flag(self, "dont_flip"),
            first: true,
            delay: self.checksum(items, "delay_anim", scope).map_or(0.0, |a| self.lib.duration(a)),
            func: None,
            strength: 0.0,
        };
        if timertype == k("vert") {
            m.x = if self.vert0 { 1.0 } else { 0.0 };
        }
        if timertype == k("offwhenfinished") {
            m.strength = self.float(items, "start_strength", scope).unwrap_or(1.0);
            if let Some(a) = self.checksum(items, "anim", scope) {
                m.blendtime = self.lib.duration(a);
            }
        }
        if let Some(f) = self.checksum(items, "blendfunction", scope) {
            if let Some(a) = self.checksum(items, "anim", scope) {
                m.blendtime = self.lib.duration(a);
            }
            let curve = lookup(items, k("blendcurve")).map(|v| self.resolve(v, scope));
            m.func = Some(BlendFn::new(f, curve.as_ref()));
        }
        m
    }

    fn skater_timer(&mut self, items: &[(u32, Value)], scope: &Params) -> SkaterTimer {
        let k = qb_key;
        let duration = match self.checksum(items, "anim", scope) {
            Some(a) => self.lib.duration(a),
            None => self.float(items, "length", scope).unwrap_or(0.0),
        };
        let timertype = self.checksum(items, "timertype", scope).unwrap_or(0);
        let mut t = SkaterTimer {
            timertype,
            time: 0.0,
            speed: lookup(items, 0xF0D9_0109).map(|v| self.resolve(v, scope)).and_then(|v| v.as_f32()).unwrap_or(1.0),
            duration,
            end: duration,
            cycle: lookup(items, k("cycle")).is_some(),
            crouched: self.crouched0,
            started: false,
            delay: self.checksum(items, "delay_anim", scope).map_or(0.0, |a| self.lib.duration(a)),
        };
        // 3ECC: a crouch timer built while crouched starts at its end
        // unless `dont_skip`.
        if timertype == k("crouch") && t.crouched && lookup(items, k("dont_skip")).is_none() {
            t.time = t.end;
        }
        t
    }

    fn crouch_weight(&mut self, items: &[(u32, Value)], scope: &Params, default_time: f32) -> CrouchWeight {
        let k = qb_key;
        let frozen = matches!(lookup(items, k("dont_update")).map(|v| self.resolve(v, scope)), Some(Value::Int(1)));
        let func = match self.checksum(items, "blendfunction", scope) {
            Some(f) => BlendFn::new(f, lookup(items, k("blendcurve")).map(|v| self.resolve(v, scope)).as_ref()),
            None => BlendFn::Linear,
        };
        let mut c = CrouchWeight {
            t: 0.0,
            blendintime: self.float(items, "blendintime", scope).unwrap_or(default_time),
            delay: self.checksum(items, "delay_anim", scope).map_or(0.0, |a| self.lib.duration(a)),
            frozen,
            func,
            w: 0.0,
        };
        if frozen {
            c.w = if self.crouched0 { 1.0 } else { 0.0 };
        }
        c
    }

    fn phase_timer(&mut self, items: &[(u32, Value)], scope: &Params, brake: bool) -> PhaseTimer {
        let n = if brake { 3 } else { 4 };
        let lengths = |b: &mut Self, name: &str| -> Vec<f32> {
            let arr = match lookup(items, qb_key(name)).map(|v| b.resolve(v, scope)) {
                Some(Value::Checksum(c)) => (b.globals)(c),
                Some(v @ Value::Array(_)) => Some(v),
                _ => None,
            };
            let names: Vec<u32> = match arr {
                Some(Value::Array(a)) => a.iter().filter_map(|x| if let Value::Checksum(c) = x { Some(*c) } else { None }).collect(),
                _ => Vec::new(),
            };
            (0..n).map(|i| names.get(i).map_or(0.0, |&c| b.lib.duration(c))).collect()
        };
        let slow = lengths(self, "slow_array");
        let fast = lengths(self, "fast_array");
        PhaseTimer {
            brake,
            time: 0.0,
            phase: 0,
            progress: 0.0,
            speed_now: self.speed0,
            slow,
            fast,
            slow_speed: self.float(items, "slow_speed", scope).unwrap_or(1.0),
            fast_speed: self.float(items, "fast_speed", scope).unwrap_or(1.0),
            min_speed: self.float(items, "min_speed", scope).unwrap_or(0.0),
            max_speed: self.float(items, "max_speed", scope).unwrap_or(0.0),
            idle_progress: 0.0,
            idle_weight: 1.0,
            idle_out_time: self.float(items, "idle_out_time", scope).unwrap_or(0.3),
        }
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

/// What the tree reads from the skater each frame: the animinfo component's
/// copies (`820B8EA0`) of skater values.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkaterInputs {
    /// SkaterState `+40` (stance flip; not translated yet).
    pub flipped: bool,
    /// SkaterState `+32`.
    pub crouched: bool,
    /// SkaterState `+24` == 1 (the air state).
    pub in_air: bool,
    /// SkaterState `+56` / `+72`.
    pub in_vert_air: bool,
    pub on_vert_ground: bool,
    /// Object velocity (animinfo `+192`).
    pub velocity: [f32; 3],
    /// Physics `+1944` (animinfo `+380`).
    pub turn: f32,
    /// `820D7570` (animinfo `+132`).
    pub brake_input: bool,
    /// Physics `+1656` (animinfo `+100`).
    pub brake_amount: f32,
    /// Physics `+2112`, cleared while the brake input is held (animinfo
    /// `+140`).
    pub kick: bool,
    /// Matrix rows (right, up, at).
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub at: [f32; 3],
    /// Trick component `+5360` (animinfo `+532`): degrees spun this air.
    pub spin: f32,
    /// `820B8220` (animinfo `+164`): riding switch.
    pub switch: bool,
    /// `820B9298` -> `820E79D8`: seconds until landing, -1 if not found,
    /// and the 0.05 s slice it was found in (see
    /// `CorePhysics::time_to_land_slice`).
    pub time_to_land: f32,
    pub time_to_land_slice: u32,
    /// `820B91D8` -> `820D7878`: seconds until the top of the jump.
    pub time_to_apex: f32,
}

impl Node {
    fn update(&mut self, mut dt: f32, i: &SkaterInputs) {
        match &mut self.kind {
            Kind::Timer(t) => t.update(&mut dt),
            Kind::SkaterTimer(t) => t.update(&mut dt, i),
            Kind::Modulate(m) => m.update(dt),
            Kind::SkaterModulate(m) => m.update(dt, i),
            Kind::CrouchBlend(c) => c.update(dt, i),
            Kind::UberCrouch(u) => u.crouch.update(dt, i),
            Kind::PhaseTimer(t) => t.update(dt, i),
            Kind::Catch(c) => c.update(dt, i),
            Kind::PartialSwitch { on, w, duration } => {
                // 8237B7C8: towards 1 (off) or 0 (on) over the blend time.
                let step = if *duration != 0.0 { dt / *duration } else { 1.0 };
                *w = if *on { (*w - step).max(0.0) } else { (*w + step).min(1.0) };
            }
            Kind::ApexTimer { time, duration, started, to_apex } => {
                // 820A2188.
                let half = *duration * 0.5;
                if !*started {
                    if !i.in_air {
                        return self.update_children(dt, i);
                    }
                    *to_apex = i.time_to_apex;
                    *started = true;
                    if half > *to_apex {
                        *time = half - *to_apex;
                    }
                }
                *to_apex -= dt;
                *time = if *to_apex > half {
                    0.0
                } else if *to_apex < -half {
                    *duration
                } else {
                    *time + dt
                };
                if *time > *duration {
                    *time = *duration;
                }
            }
            Kind::TakeoffBlend { t, blendintime, func, w } => {
                if *t < 1.0 {
                    *t = if *blendintime > 0.0 { (*t + dt / *blendintime).min(1.0) } else { 1.0 };
                }
                *w = func.as_ref().map_or(*t, |f| f.eval(*t));
            }
            Kind::OllieLand(o) => o.update(dt, i),
            Kind::SpinTimer { time, duration } => {
                let a = if i.switch { -i.spin } else { i.spin }.clamp(-180.0, 180.0);
                *time = (a / 180.0 * 0.5 + 0.5) * *duration;
            }
            Kind::SpinAdd { cur, prev, wl, wr, induration, outduration } => {
                *prev = *cur;
                *cur = if i.switch { -i.spin } else { i.spin };
                if *cur < *prev {
                    if *wl < 1.0 {
                        *wl += dt / *induration;
                    }
                    if *wr > 0.0 {
                        *wr -= dt / *outduration;
                    }
                } else if *cur > *prev {
                    if *wl > 0.0 {
                        *wl -= dt / *outduration;
                    }
                    if *wr < 1.0 {
                        *wr += dt / *induration;
                    }
                } else {
                    if *wl > 0.0 {
                        *wl -= dt / *outduration;
                    }
                    if *wr > 0.0 {
                        *wr -= dt / *outduration;
                    }
                }
                *wl = wl.clamp(0.0, 1.0);
                *wr = wr.clamp(0.0, 1.0);
            }
            Kind::TimedSwitch { w, length, time } => {
                // 820B3C10; then (820B3C60) the second child is only
                // updated once the switch has started.
                *time += dt;
                if *time > *length && *w < 1.0 {
                    *w = (*w + 0.1).min(1.0);
                }
                let started = *w != 0.0;
                if let Some(c) = self.children.first_mut() {
                    c.update(dt, i);
                }
                if started && self.children.len() > 1 {
                    let n = self.children.len();
                    self.children[n - 1].update(dt, i);
                }
                return;
            }
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
        self.update_children(dt, i);
    }

    fn update_children(&mut self, dt: f32, i: &SkaterInputs) {
        for c in &mut self.children {
            c.update(dt, i);
        }
    }

    fn sample(&mut self, phase: f32, cx: &mut Sampler) -> Pose {
        let rig = cx.rig;
        let first = |n: &mut Node, cx: &mut Sampler| match n.children.first_mut() {
            Some(c) => c.sample(phase, cx),
            None => Pose::zero(rig),
        };
        match &mut self.kind {
            Kind::Source { anim } => {
                let key = if cx.board { qb_key_extend(*anim, "_b") } else { *anim };
                match cx.lib.get(key) {
                    Some(c) => Pose::from_clip(rig, &c, c.duration * phase),
                    None => Pose::zero(rig),
                }
            }
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
                // Runs on every sample, the board's too (82245DC0/82245E58).
                m.sample_value(&cx.inputs);
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
            Kind::PoseCapture { .. } => {
                if self.children.is_empty() {
                    let Kind::PoseCapture { kept, kept_board } = &self.kind else { unreachable!() };
                    let k = if cx.board { kept_board } else { kept };
                    return k.clone().unwrap_or_else(|| Pose::zero(rig));
                }
                let p = first(self, cx);
                if let Kind::PoseCapture { kept, kept_board } = &mut self.kind {
                    // The board keeps its own copy (INFERRED: retail keeps
                    // one buffer per node).
                    *(if cx.board { kept_board } else { kept }) = Some(p.clone());
                }
                p
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
                if !cx.board {
                    solve_ik(&mut p, &chains, rig);
                }
                p
            }
            Kind::SkaterTimer(t) => {
                let p = if t.duration > 0.0 { t.time / t.duration } else { 0.0 };
                match self.children.first_mut() {
                    Some(c) => c.sample(p, cx),
                    None => Pose::zero(rig),
                }
            }
            Kind::SpeedBlend { speed_min, speed, speed_now } => {
                // 820B47B0: the speed moves at most 0.5 a frame; then an add
                // of the slow and fast children weighted 1 - u and u.
                let v = cx.inputs.velocity;
                let s = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                let d = s - *speed_now;
                // Stepped on every sample, the board's too (82245E58).
                *speed_now = if d.abs() > 0.5 { if d > 0.0 { *speed_now + 0.5 } else { *speed_now - 0.5 } } else { s };
                let u = if *speed == *speed_min { 0.0 } else { ((*speed_now - *speed_min) / (*speed - *speed_min)).clamp(0.0, 1.0) };
                let (wa, wb) = (1.0 - u, u);
                let mut a =
                    if wa != 0.0 { self.children.first_mut().map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                let b = if wb != 0.0 { self.children.get_mut(1).map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                a.add(wa, wb, b);
                a
            }
            Kind::CrouchBlend(c) => {
                let w = c.w;
                two_child_blend(&mut self.children, w, phase, cx)
            }
            Kind::TimedSwitch { w, .. } => {
                let w = *w;
                two_child_blend(&mut self.children, w, phase, cx)
            }
            Kind::UberCrouch(u) => {
                let v = cx.inputs.velocity;
                let s = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                let d = s - u.speed_now;
                u.speed_now = if d > 0.5 {
                    u.speed_now + 0.5
                } else if d < -0.5 {
                    u.speed_now - 0.5
                } else {
                    s
                };
                let f = if u.speed == u.speed_min { 0.0 } else { ((u.speed_now - u.speed_min) / (u.speed - u.speed_min)).clamp(0.0, 1.0) };
                let c = u.crouch.w;
                let clips = u.clips;
                let pair = |slow: u32, fast: u32, cx: &mut Sampler| {
                    // 820B5DE0: add slow and fast weighted 1 - f and f.
                    let src = |k: u32, cx: &mut Sampler| {
                        let k = if cx.board { qb_key_extend(k, "_b") } else { k };
                        match cx.lib.get(k) {
                            Some(clip) => Pose::from_clip(rig, &clip, clip.duration * phase),
                            None => Pose::zero(rig),
                        }
                    };
                    let mut a = if 1.0 - f != 0.0 { src(slow, cx) } else { Pose::zero(rig) };
                    let b = if f != 0.0 { src(fast, cx) } else { Pose::zero(rig) };
                    a.add(1.0 - f, f, b);
                    a
                };
                if c == 0.0 {
                    pair(clips[0], clips[1], cx)
                } else if c == 1.0 {
                    pair(clips[2], clips[3], cx)
                } else {
                    let mut a = pair(clips[0], clips[1], cx);
                    let b = pair(clips[2], clips[3], cx);
                    a.add(1.0 - c, c, b);
                    a
                }
            }
            Kind::PhaseTimer(t) => {
                if t.phase == 0 && t.progress == 0.0 {
                    return Pose::zero(rig);
                }
                let (phase_i, progress) = (t.phase, t.progress);
                let (brake, idle_w, idle_p) = (t.brake, t.idle_weight, t.idle_progress);
                let mut out = self.children.get_mut(phase_i).map_or(Pose::zero(rig), |c| c.sample(progress, cx));
                if brake && idle_w > 0.0 {
                    let n = self.children.len();
                    if n > 0 {
                        let mut idle = self.children[n - 1].sample(idle_p, cx);
                        idle.scale(idle_w);
                        out.add(1.0, 1.0, idle);
                    }
                }
                out
            }
            Kind::Catch(c) => {
                if c.strength == 0.0 {
                    return Pose::zero(rig);
                }
                let s = c.strength;
                // The captured pose lives in each object's own pose buffer
                // (skater +24, board +32: 820B3798 / 820B0380).
                let kept = if cx.board { &c.captured_board } else { &c.captured };
                let mut p = if c.latched && kept.is_some() {
                    kept.clone().unwrap()
                } else {
                    let live = match self.children.first_mut() {
                        Some(ch) => ch.sample(phase, cx),
                        None => Pose::zero(rig),
                    };
                    if let Kind::Catch(c) = &mut self.kind {
                        *(if cx.board { &mut c.captured_board } else { &mut c.captured }) = Some(live.clone());
                    }
                    live
                };
                p.scale(s);
                p
            }
            Kind::Blank => Pose::zero(rig),
            Kind::PartialSwitch { w, .. } => {
                // 8237B878: the second child, blended towards the first by
                // 1 - weight.
                let w = *w;
                if w == 1.0 || self.children.len() < 2 {
                    let n = self.children.len();
                    return self.children.get_mut(n.min(2).saturating_sub(1)).map_or(Pose::zero(rig), |c| c.sample(phase, cx));
                }
                let a = self.children[0].sample(phase, cx);
                let mut b = self.children[1].sample(phase, cx);
                b.blend_weighted(&a, 1.0 - w, rig);
                b
            }
            Kind::ApexTimer { time, duration, .. } | Kind::SpinTimer { time, duration } => {
                let p = if *duration > 0.0 { *time / *duration } else { 0.0 };
                match self.children.first_mut() {
                    Some(c) => c.sample(p, cx),
                    None => Pose::zero(rig),
                }
            }
            Kind::TakeoffBlend { w, .. } => {
                let w = *w;
                two_child_blend(&mut self.children, w, phase, cx)
            }
            Kind::OllieLand(o) => {
                let w = o.w;
                two_child_blend(&mut self.children, w, phase, cx)
            }
            Kind::SpinAdd { wl, wr, .. } => {
                let (wl, wr) = (*wl, *wr);
                let mut a =
                    if wl != 0.0 { self.children.first_mut().map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                let b = if wr != 0.0 { self.children.get_mut(1).map_or(Pose::zero(rig), |c| c.sample(phase, cx)) } else { Pose::zero(rig) };
                a.add(wl, wr, b);
                a
            }
            Kind::Untranslated(_) => Pose::zero(rig),
        }
    }

    /// One line per node with its live values (for debugging tools).
    pub fn describe(&self, depth: usize, out: &mut String) {
        use std::fmt::Write;
        let v = match &self.kind {
            Kind::Source { anim } => format!("source {}", name_of(*anim).map_or(format!("{anim:08x}"), str::to_string)),
            Kind::Timer(t) => format!("timer t={:.2}/{:.2}", t.time, t.duration),
            Kind::SkaterTimer(t) => format!("skatertimer t={:.2}/{:.2}", t.time, t.duration),
            Kind::Add { wa, wb } => format!("add {wa} {wb}"),
            Kind::ApplyDifference { .. } => "applydifference".into(),
            Kind::Modulate(m) => format!("modulate s={:.2}", m.strength),
            Kind::SkaterModulate(m) => format!("skatermodulate {:08x} x={:.2} s={:.2}", m.timertype, m.x, m.strength),
            Kind::Flip => "skaterflip".into(),
            Kind::PassThrough => "pass".into(),
            Kind::PoseCapture { .. } => "posecapture".into(),
            Kind::DegenerateBlend(d) => format!("degenerateblend {:?}", d.records.iter().map(|r| r.0).collect::<Vec<_>>()),
            Kind::SpeedBlend { speed_now, speed, .. } => format!("speedblend {speed_now:.2}/{speed}"),
            Kind::CrouchBlend(c) => format!("crouchblend w={:.2}", c.w),
            Kind::UberCrouch(u) => format!("ubercrouch c={:.2} speed {:.2}/{} clips {:08x?}", u.crouch.w, u.speed_now, u.speed, u.clips),
            Kind::PhaseTimer(t) => format!("{} phase {} p={:.2}", if t.brake { "braketimer" } else { "kicktimer" }, t.phase, t.progress),
            Kind::Catch(c) => format!("catch s={:.2}", c.strength),
            Kind::TimedSwitch { w, time, length } => format!("timedswitch w={w:.2} t={time:.2}/{length:.2}"),
            Kind::Ik { .. } => "ik".into(),
            Kind::Blank => "blank".into(),
            Kind::PartialSwitch { w, .. } => format!("partialswitch w={w:.2}"),
            Kind::ApexTimer { time, duration, .. } => format!("apextimer t={time:.2}/{duration:.2}"),
            Kind::TakeoffBlend { w, .. } => format!("takeoffblend w={w:.2}"),
            Kind::OllieLand(o) => format!("ollielandblend w={:.2} to_land={:.2}", o.w, o.to_land),
            Kind::SpinTimer { time, duration } => format!("spintimer t={time:.2}/{duration:.2}"),
            Kind::SpinAdd { wl, wr, .. } => format!("spinadd {wl:.2} {wr:.2}"),
            Kind::Untranslated(t) => format!("UNTRANSLATED {}", name_of(*t).unwrap_or("?")),
        };
        let _ = writeln!(out, "{}{}{}", "  ".repeat(depth), if self.id != 0 { format!("[{:08x}] ", self.id) } else { String::new() }, v);
        for c in &self.children {
            c.describe(depth + 1, out);
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

/// The generic two-child blend sample (`8237B978`): the first child at
/// weight 0, the second at 1, else a weighted blend.
fn two_child_blend(children: &mut [Node], w: f32, phase: f32, cx: &mut Sampler) -> Pose {
    let rig = cx.rig;
    if w == 0.0 || children.len() < 2 {
        return children.first_mut().map_or(Pose::zero(rig), |c| c.sample(phase, cx));
    }
    if w == 1.0 {
        return children[1].sample(phase, cx);
    }
    let mut a = children[0].sample(phase, cx);
    let b = children[1].sample(phase, cx);
    a.blend_weighted(&b, w, rig);
    a
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
    /// Sampling for the board object: sources use the clip's board
    /// counterpart (`820B3A80` picks handle `+48`, named `<clip>_b` by
    /// `820B39C8`), no IK, and per-frame smoothing is not stepped a second
    /// time (INFERRED).
    board: bool,
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
    /// The skater values given to the last update.
    pub inputs: SkaterInputs,
    /// The board's skeleton (`board` in global.pak), when shown.
    pub board_rig: Option<Rig>,
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
                let i = self.inputs;
                let v = i.velocity;
                let mut b = Build {
                    globals,
                    lib,
                    rig: &self.rig,
                    types: &mut self.untranslated,
                    crouched0: i.crouched,
                    flipped0: i.flipped,
                    vert0: i.in_vert_air || i.on_vert_ground,
                    speed0: (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(),
                };
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
                // 82381F5C: only `+24`, the strength.
                if let Some(s) = params.float(k("strength")) {
                    m.strength = s;
                }
                true
            }
            Kind::Modulate(m) if command == k("modulate_startblend") => {
                // 82381E68 -> 82381AC8: restart the blend from the current
                // strength (nothing when no blendfunction is given).
                let Some(f) = params.checksum(k("blendfunction")) else { return true };
                let anim = params.checksum(k("anim")).unwrap_or(0);
                m.duration = if anim != 0 { lib.duration(anim) } else { params.float(k("blendtime")).unwrap_or(0.0) };
                let curve = params.get(k("blendcurve")).cloned();
                m.func = BlendFn::new(f, curve.as_ref());
                m.start = m.strength;
                m.done = false;
                m.t = 0.0;
                if let Some(Value::Array(a)) = &curve {
                    m.from_start = a.first().and_then(Value::as_f32) == Some(1.0);
                }
                true
            }
            Kind::PoseCapture { kept, .. } if command == k("posecapture_capture") => {
                // 820B38F0: with a kept pose and a live child, delete the
                // child; the kept pose is returned from then on.
                if kept.is_some() && node.children.len() == 1 {
                    node.children.clear();
                }
                true
            }
            Kind::PartialSwitch { on, duration, .. } if command == k("partialswitch_setstate") => {
                // 8237B6F0: the first unnamed checksum (or `state`).
                let st = params.unnamed_checksum().or_else(|| params.checksum(k("state"))).unwrap_or(0);
                *on = st == k("on");
                let d = params.float(k("blendduration")).unwrap_or(-1.0);
                *duration = if d == -1.0 { 0.3 } else { d };
                true
            }
            Kind::PartialSwitch { on, .. } if command == k("partialswitch_ison") => *on,
            _ => false,
        }
    }

    /// `Skater_AnimNodeExists`.
    pub fn node_exists(&self, id: u32) -> bool {
        self.body.as_ref().is_some_and(|b| b.contains(id))
    }

    /// Advance the tree (every node's update, slot 4) with the skater's
    /// current values.
    pub fn update(&mut self, dt: f32, inputs: SkaterInputs) {
        self.inputs = inputs;
        if let Some(b) = self.body.as_mut() {
            b.update(dt, &inputs);
        }
    }

    /// The body pose, or `None` before any branch was added.
    pub fn sample(&mut self, inputs: SkaterInputs) -> Option<Pose> {
        let (Some(lib), Some(body)) = (self.lib.as_mut(), self.body.as_mut()) else { return None };
        if body.children.is_empty() {
            return None;
        }
        let mut cx = Sampler { board: false, rig: &self.rig, lib, inputs, untranslated: &mut self.untranslated };
        let mut p = body.sample(0.0, &mut cx);
        p.flush(&self.rig);
        Some(p)
    }

    /// The board's pose from the same tree (the board object samples the
    /// skater's nodes with its own clips), or `None` without a board rig or
    /// branch.
    pub fn sample_board(&mut self) -> Option<Pose> {
        let (Some(lib), Some(body), Some(rig)) = (self.lib.as_mut(), self.body.as_mut(), self.board_rig.as_ref()) else { return None };
        if body.children.is_empty() {
            return None;
        }
        let mut cx = Sampler { board: true, rig, lib, inputs: self.inputs, untranslated: &mut self.untranslated };
        let mut p = body.sample(0.0, &mut cx);
        p.flush(rig);
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
