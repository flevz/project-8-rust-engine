//! The anim tree's `ik` node (the root of the skater's ground branches,
//! e.g. `OnGround_AnimBranch`, `Stopped_AnimBranch`), translated from
//! retail. Without it the legs follow the pelvis and the feet drift off the
//! board; with it each foot is put where the animation's IK bone says.
//!
//! - Chains come from the script struct `Skater_IK_Params`
//!   (`two_bone_chains`: `bone0` thigh, `bone1` knee, `bone2` ankle,
//!   `bonetarget` e.g. `Bone_IK_Foot_Slave_L`, `bonealign`), read by the
//!   node's init `82380500`, which also sets the hinge axis (0, 1, 0) in the
//!   knee's frame and both gains to 1.
//! - Each frame (`8237F5D8`): the target is the model-space transform of the
//!   `bonetarget` bone in the animated pose; Havok's two-joint solver
//!   (`824E9118`, with no hinge limits: cosines -1 and 1) bends the knee and
//!   turns the thigh; then the ankle's model-space rotation is set to the
//!   target's rotation. `bonealign` is read but not used by that code.
//!
//! Rotations here are in the renderer's convention (the conjugate of what
//! the clips store, see [`crate::anim::BoneSample::bevy_rotation`]), which
//! is what the node hands Havok (`82327678`).
use crate::anim::qmul;

type V3 = [f32; 3];
type Q = [f32; 4];

/// A bone's transform relative to its parent (or to the model for a root).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub rotation: Q,
    pub translation: V3,
}

/// One leg: skeleton bone indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chain {
    pub thigh: usize,
    pub knee: usize,
    pub ankle: usize,
    pub target: usize,
}

/// Hinge axis in the knee's frame. Retail's constant (set by `82380500`,
/// CONFIRMED in the recompiled code) is (0, 1, 0), but the node solves on a
/// Havok skeleton reached through a skeleton mapper (`824E47E8`) whose bone
/// frames are not traced. In this skeleton's frame the clips bend the knee
/// about local Z (Sk8_Gnd_Stnd_To_Crch_Base_xx: -Z by 36-40 degrees when
/// crouching), and only +Z puts every foot on its target (the `anim_check`
/// example: under 0.1 mm; with Y the knees twist sideways). INFERRED from
/// that data.
pub const HINGE_AXIS: V3 = [0.0, 0.0, 1.0];
/// The gains (`82380500`).
pub const FIRST_JOINT_GAIN: f32 = 1.0;
pub const SECOND_JOINT_GAIN: f32 = 1.0;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn normalize3(a: V3) -> V3 {
    scale(a, 1.0 / dot(a, a).sqrt())
}
fn normalize4(q: Q) -> Q {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|c| c / l)
}
fn conj(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}
/// Rotate `v` by `q` (the formula of `824E9118`: 2((w² - ½)v + (q·v)q + w(q×v))).
fn rotate(q: Q, v: V3) -> V3 {
    let (u, w) = ([q[0], q[1], q[2]], q[3]);
    let a = scale(v, w * w - 0.5);
    let b = scale(u, dot(u, v));
    let c = scale(cross(u, v), w);
    scale([a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]], 2.0)
}
/// `8244D8F8`: arc cosine, 0 or π outside [-1, 1].
fn acos(x: f32) -> f32 {
    if x.abs() >= 1.0 { if x > 0.0 { 0.0 } else { std::f32::consts::PI } } else { x.acos() }
}
/// `8244D950`: rotation of `angle` about `axis`.
fn axis_angle(axis: V3, angle: f32) -> Q {
    let (s, c) = (angle * 0.5).sin_cos();
    [axis[0] * s, axis[1] * s, axis[2] * s, c]
}
/// `824E9000` (hkQuaternion::setShortestRotationDamped): rotation turning
/// `from` towards `to`, scaled by `gain`.
fn shortest_rotation_damped(gain: f32, from: V3, to: V3) -> Q {
    let damped = (1.0 - gain) + gain * dot(from, to);
    if damped > 0.99999 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if damped < -0.99999 {
        // `8244DCF0` (not read): a half turn about an axis perpendicular to
        // `from`. APPROXIMATE: any perpendicular axis is used here. A leg
        // never needs this in practice (the target would be straight behind
        // the thigh).
        let p = if from[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let a = normalize3(cross(from, p));
        return [a[0], a[1], a[2], 0.0];
    }
    let c = ((damped + 1.0) * 0.5).sqrt();
    let v = scale(cross(from, to), gain / c * 0.5);
    normalize4([v[0], v[1], v[2], c])
}

/// Model-space transforms from local ones (`parents[i] < i`).
pub fn model_space(locals: &[Local], parents: &[Option<usize>]) -> Vec<Local> {
    let mut out: Vec<Local> = Vec::with_capacity(locals.len());
    for (i, l) in locals.iter().enumerate() {
        out.push(match parents[i] {
            Some(p) if p < i => {
                let m = out[p];
                let t = rotate(m.rotation, l.translation);
                Local {
                    rotation: qmul(m.rotation, l.rotation),
                    translation: [m.translation[0] + t[0], m.translation[1] + t[1], m.translation[2] + t[2]],
                }
            }
            _ => *l,
        });
    }
    out
}

/// Set bone `b`'s model-space rotation, keeping its children's local
/// transforms (Havok `accessBoneModelSpace(.., PROPAGATE)`).
fn set_model_rotation(locals: &mut [Local], model: &mut Vec<Local>, parents: &[Option<usize>], b: usize, rotation: Q) {
    let parent_rot = parents[b].map_or([0.0, 0.0, 0.0, 1.0], |p| model[p].rotation);
    locals[b].rotation = normalize4(qmul(conj(parent_rot), rotation));
    *model = model_space(locals, parents);
}

/// Solve one leg in place (`8237F5D8` + `824E9118`).
pub fn solve_chain(locals: &mut [Local], parents: &[Option<usize>], c: Chain) {
    let mut model = model_space(locals, parents);
    let target = model[c.target];
    let (a, b, e) = (model[c.thigh].translation, model[c.knee].translation, model[c.ankle].translation);
    let (ab, be, at) = (sub(b, a), sub(e, b), sub(target.translation, a));
    let (ab2, be2, at2) = (dot(ab, ab), dot(be, be), dot(at, at));
    // Knee: the angle that makes thigh-to-ankle as long as thigh-to-target.
    let cos = ((at2 - ab2 - be2) / (ab2.sqrt() * be2.sqrt() * -2.0)).clamp(-1.0, 1.0);
    let desired = acos(cos);
    let current = acos(dot(scale(normalize3(ab), -1.0), normalize3(be)));
    let knee = model[c.knee].rotation;
    let hinge = rotate(knee, HINGE_AXIS);
    let turned = qmul(axis_angle(hinge, (desired - current) * SECOND_JOINT_GAIN), knee);
    set_model_rotation(locals, &mut model, parents, c.knee, normalize4(turned));
    // Thigh: point the leg at the target.
    let a = model[c.thigh].translation;
    let from = normalize3(sub(model[c.ankle].translation, a));
    let to = normalize3(sub(target.translation, a));
    let r = shortest_rotation_damped(FIRST_JOINT_GAIN, from, to);
    let thigh = normalize4(qmul(r, model[c.thigh].rotation));
    set_model_rotation(locals, &mut model, parents, c.thigh, thigh);
    // Ankle: the target's rotation.
    set_model_rotation(locals, &mut model, parents, c.ankle, target.rotation);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    /// Root, thigh (1 m up), knee and ankle along the bones' X (as in the
    /// skater skeleton), and a target bone
    /// under the root. The foot must end on the target and take its
    /// rotation; bone lengths must not change.
    #[test]
    fn foot_reaches_the_target() {
        let id = [0.0, 0.0, 0.0, 1.0];
        let l = |t: V3| Local { rotation: id, translation: t };
        let parents = [None, Some(0), Some(1), Some(2), Some(0)];
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let mut locals = vec![
            l([0.0, 0.0, 0.0]),
            l([0.0, 1.0, 0.0]),
            l([0.5, 0.0, 0.0]),
            l([0.5, 0.0, 0.0]),
            Local { rotation: [0.0, s, 0.0, s], translation: [0.3, 0.4, 0.3] },
        ];
        solve_chain(&mut locals, &parents, Chain { thigh: 1, knee: 2, ankle: 3, target: 4 });
        let m = model_space(&locals, &parents);
        assert!(close(m[3].translation, [0.3, 0.4, 0.3]), "{:?}", m[3].translation);
        assert!((0..4).all(|i| (m[3].rotation[i] - [0.0, s, 0.0, s][i]).abs() < 1e-4));
        let len = |a: V3, b: V3| dot(sub(a, b), sub(a, b)).sqrt();
        assert!((len(m[1].translation, m[2].translation) - 0.5).abs() < 1e-5);
        assert!((len(m[2].translation, m[3].translation) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn damped_rotation_turns_from_into_to() {
        let r = shortest_rotation_damped(1.0, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!(close(rotate(r, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
    }
}
