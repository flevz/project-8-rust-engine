//! The skatermatrixqueries component's tests (`PitchGreaterThan`,
//! `RollGreaterThan`, `YawBetween`, ...) and the vector helpers they use.
use glam::{Mat3, Vec3};

/// Degrees to radians (the float at `82000C10`).
fn radians(degrees: f32) -> f64 {
    (degrees * f32::from_bits(0x3C8E_FA35)) as f64
}

/// `821EE398`: the angle between `a` and `b` is more than `degrees` (false
/// when either is shorter than 0.5; a cosine below 0 counts as more than an
/// angle whose cosine is 0 or more).
pub fn angle_greater(a: Vec3, b: Vec3, degrees: f32) -> bool {
    let la = a.length();
    if la < 0.5 {
        return false;
    }
    let lb = b.length();
    if lb < 0.5 {
        return false;
    }
    let c = a.dot(b) / la / lb;
    let limit = crate::core_physics::retail_cos(radians(degrees));
    if c < 0.0 && limit >= 0.0 {
        return true;
    }
    c < limit
}

/// `PitchGreaterThan` (`82108AB0`): up (queries `+48`) against core `+128`
/// (where the eased normal eases from), unnormalized dot below the cosine.
pub fn pitch_greater(m: &Mat3, ease_from: Vec3, degrees: f32) -> bool {
    m.y_axis.dot(ease_from) < crate::core_physics::retail_cos(radians(degrees))
}

/// `AbsolutePitchGreaterThan` (`82108BA0`): up's height below the cosine.
pub fn absolute_pitch_greater(m: &Mat3, degrees: f32) -> bool {
    m.y_axis.y < crate::core_physics::retail_cos(radians(degrees))
}

/// `RollGreaterThan` (`82108C28`): right (queries `+32`) projected onto the
/// plane of core `+128`, then more than `degrees` from right.
pub fn roll_greater(m: &Mat3, ease_from: Vec3, degrees: f32) -> bool {
    angle_greater(crate::core_physics::project_keep_length(m.x_axis, ease_from), m.x_axis, degrees)
}

/// `YawBetween` (`82108890`): the velocity projected onto the plane of up
/// is more than `a` but not more than `b` degrees from at (queries `+64`).
pub fn yaw_between(m: &Mat3, velocity: Vec3, a: f32, b: f32) -> bool {
    let v = crate::core_physics::project_keep_length(velocity, m.y_axis);
    angle_greater(v, m.z_axis, a) && !angle_greater(v, m.z_axis, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Results of the recompiled retail `821EE398` on these inputs.
    #[test]
    fn angle_greater_matches_retail() {
        let cases: [([f32; 3], [f32; 3], f32, bool); 24] = [
            ([0.0737, 0.1451, 0.1771], [1.7698, 0.9596, 1.6893], 5.22, false),
            ([-0.1375, 1.7734, 0.5959], [1.6036, -1.5472, -0.1237], 44.38, true),
            ([0.1750, 0.2958, -1.9475], [-1.1331, -0.8821, 1.6654], 137.83, true),
            ([-1.3616, 1.1886, -1.4449], [0.4698, -1.4932, -1.9929], 156.85, false),
            ([-1.1622, -1.1381, 1.9297], [1.4896, -0.8428, 1.8459], 97.06, false),
            ([0.7113, -1.1809, 1.7639], [0.7626, 1.8663, 1.5750], 53.78, true),
            ([-0.0833, -0.2004, -0.2126], [-1.7394, -0.7946, 0.4124], 0.61, false),
            ([0.7117, -0.6484, -0.7602], [1.2741, -0.0770, -0.7368], 86.62, false),
            ([0.8187, -1.7720, 1.9004], [-1.9085, 0.9992, 1.3795], 3.25, true),
            ([1.1510, -0.5353, 0.3141], [-1.9637, -1.8131, -1.2763], 171.93, false),
            ([-1.2139, 1.0229, 1.7186], [1.7682, -0.6225, -0.5808], 94.45, true),
            ([1.1024, -1.5678, 0.9936], [1.1889, 1.4388, -1.8535], 170.24, false),
            ([-0.2453, -0.0956, 0.0665], [1.6723, -0.6402, 1.6968], 98.13, false),
            ([-0.7502, -0.7328, -1.2901], [-1.6872, -1.4045, 0.7567], 179.41, false),
            ([-1.3539, -1.8058, 1.9468], [0.1341, -0.3764, -1.0507], 106.91, true),
            ([1.3052, -0.1773, -0.3130], [-1.7772, 1.6643, -1.8691], 88.84, true),
            ([1.3537, -1.4777, 0.9267], [1.7992, 0.5216, 1.1520], 19.19, true),
            ([-0.2618, -1.4030, 1.3789], [-0.8207, -0.1874, 1.9972], 153.41, false),
            ([0.2856, -0.0279, -0.0071], [0.9180, -0.0838, -0.8359], 72.68, false),
            ([-1.4140, -0.4920, 1.9536], [1.8393, 0.5079, -0.0027], 60.93, true),
            ([-1.6435, -0.9108, 1.1281], [1.4695, -0.5547, 1.1441], 139.48, false),
            ([0.7784, 0.6561, 1.0386], [-0.5463, 0.8179, -0.8766], 87.42, true),
            ([1.0790, 0.7635, -0.8246], [1.7822, 0.5988, 0.3226], 2.08, true),
            ([0.1880, -0.9972, 0.6866], [-0.1482, 1.2667, 0.5897], 143.57, false),
        ];
        for (a, b, deg, want) in cases {
            assert_eq!(angle_greater(Vec3::from(a), Vec3::from(b), deg), want, "{a:?} {b:?} {deg}");
        }
    }
}
