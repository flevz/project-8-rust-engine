//! Original procedural test park. It stands in for Project 8 levels until the
//! level converter exists. Collision and visuals come from the same triangles.
use bevy::prelude::*;
use p8_sim::Rail;

pub struct Piece {
    pub triangles: Vec<[Vec3; 3]>,
    pub color: Color,
}

#[derive(Default)]
pub struct Park {
    pub pieces: Vec<Piece>,
    pub rails: Vec<Rail>,
    pub spawn: Vec3,
    pub spawn_heading: f32,
}

fn quad(t: &mut Vec<[Vec3; 3]>, a: Vec3, b: Vec3, c: Vec3, d: Vec3) {
    t.push([a, b, c]);
    t.push([a, c, d]);
}

fn cuboid(min: Vec3, max: Vec3) -> Vec<[Vec3; 3]> {
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let (a, b) = (min, max);
    let mut t = Vec::new();
    quad(
        &mut t,
        v(a.x, b.y, a.z),
        v(a.x, b.y, b.z),
        v(b.x, b.y, b.z),
        v(b.x, b.y, a.z),
    ); // top
    quad(
        &mut t,
        v(a.x, a.y, a.z),
        v(b.x, a.y, a.z),
        v(b.x, a.y, b.z),
        v(a.x, a.y, b.z),
    ); // bottom
    quad(
        &mut t,
        v(a.x, a.y, a.z),
        v(a.x, b.y, a.z),
        v(b.x, b.y, a.z),
        v(b.x, a.y, a.z),
    ); // -z
    quad(
        &mut t,
        v(a.x, a.y, b.z),
        v(b.x, a.y, b.z),
        v(b.x, b.y, b.z),
        v(a.x, b.y, b.z),
    ); // +z
    quad(
        &mut t,
        v(a.x, a.y, a.z),
        v(a.x, a.y, b.z),
        v(a.x, b.y, b.z),
        v(a.x, b.y, a.z),
    ); // -x
    quad(
        &mut t,
        v(b.x, a.y, a.z),
        v(b.x, b.y, a.z),
        v(b.x, b.y, b.z),
        v(b.x, a.y, b.z),
    ); // +x
    t
}

/// Quarter pipe along X from `x0` to `x1`. It rises towards +Z (`toward_positive_z`)
/// or -Z, starting at `z_start`.
fn quarter_pipe(
    x0: f32,
    x1: f32,
    z_start: f32,
    radius: f32,
    toward_positive_z: bool,
) -> (Vec<[Vec3; 3]>, Rail) {
    let s = if toward_positive_z { 1.0 } else { -1.0 };
    let segments = 12;
    let mut t = Vec::new();
    let point = |i: usize| {
        let a = (i as f32 / segments as f32) * std::f32::consts::FRAC_PI_2;
        (z_start + s * radius * a.sin(), radius * (1.0 - a.cos()))
    };
    for i in 0..segments {
        let (za, ya) = point(i);
        let (zb, yb) = point(i + 1);
        quad(
            &mut t,
            Vec3::new(x0, ya, za),
            Vec3::new(x0, yb, zb),
            Vec3::new(x1, yb, zb),
            Vec3::new(x1, ya, za),
        );
    }
    // Deck behind the coping and a back wall down to the ground.
    let top = z_start + s * radius;
    let deck_end = top + s * 2.0;
    quad(
        &mut t,
        Vec3::new(x0, radius, top),
        Vec3::new(x0, radius, deck_end),
        Vec3::new(x1, radius, deck_end),
        Vec3::new(x1, radius, top),
    );
    quad(
        &mut t,
        Vec3::new(x0, 0.0, deck_end),
        Vec3::new(x1, 0.0, deck_end),
        Vec3::new(x1, radius, deck_end),
        Vec3::new(x0, radius, deck_end),
    );
    let coping = Rail {
        start: Vec3::new(x0, radius, top),
        end: Vec3::new(x1, radius, top),
    };
    (t, coping)
}

/// Kicker ramp along +Z: rises from `z0` to `z1` up to `height`.
fn kicker(x0: f32, x1: f32, z0: f32, z1: f32, height: f32) -> Vec<[Vec3; 3]> {
    let mut t = Vec::new();
    quad(
        &mut t,
        Vec3::new(x0, 0.0, z0),
        Vec3::new(x0, height, z1),
        Vec3::new(x1, height, z1),
        Vec3::new(x1, 0.0, z0),
    );
    quad(
        &mut t,
        Vec3::new(x0, 0.0, z1),
        Vec3::new(x1, 0.0, z1),
        Vec3::new(x1, height, z1),
        Vec3::new(x0, height, z1),
    );
    t.push([
        Vec3::new(x0, 0.0, z0),
        Vec3::new(x0, 0.0, z1),
        Vec3::new(x0, height, z1),
    ]);
    t.push([
        Vec3::new(x1, 0.0, z0),
        Vec3::new(x1, height, z1),
        Vec3::new(x1, 0.0, z1),
    ]);
    t
}

pub fn build() -> Park {
    let mut park = Park {
        spawn: Vec3::new(0.0, 0.0, -25.0),
        spawn_heading: 0.0,
        ..default()
    };
    let ground = Color::srgb(0.42, 0.44, 0.47);
    let wood = Color::srgb(0.72, 0.56, 0.36);
    let concrete = Color::srgb(0.66, 0.66, 0.62);
    let metal = Color::srgb(0.85, 0.2, 0.15);

    let mut floor = Vec::new();
    let s = 60.0;
    quad(
        &mut floor,
        Vec3::new(-s, 0.0, -s),
        Vec3::new(-s, 0.0, s),
        Vec3::new(s, 0.0, s),
        Vec3::new(s, 0.0, -s),
    );
    park.pieces.push(Piece {
        triangles: floor,
        color: ground,
    });

    // Quarter pipes at both ends of the park.
    for (z, toward) in [(30.0, true), (-40.0, false)] {
        let (t, coping) = quarter_pipe(-20.0, 20.0, z, 3.5, toward);
        park.pieces.push(Piece {
            triangles: t,
            color: wood,
        });
        park.rails.push(coping);
    }

    // Kickers.
    park.pieces.push(Piece {
        triangles: kicker(-3.0, 3.0, -8.0, -4.0, 1.2),
        color: wood,
    });
    park.pieces.push(Piece {
        triangles: kicker(10.0, 14.0, 0.0, 3.0, 0.8),
        color: wood,
    });

    // Funbox with grindable ledges.
    let (min, max) = (Vec3::new(-16.0, 0.0, 2.0), Vec3::new(-10.0, 0.6, 14.0));
    park.pieces.push(Piece {
        triangles: cuboid(min, max),
        color: concrete,
    });
    for x in [min.x, max.x] {
        park.rails.push(Rail {
            start: Vec3::new(x, max.y, min.z),
            end: Vec3::new(x, max.y, max.z),
        });
    }

    // Flat handrail.
    let (a, b) = (Vec3::new(5.0, 0.7, 6.0), Vec3::new(5.0, 0.7, 22.0));
    park.pieces.push(Piece {
        triangles: cuboid(Vec3::new(4.97, 0.64, 6.0), Vec3::new(5.03, 0.7, 22.0)),
        color: metal,
    });
    for z in [6.5, 21.5] {
        park.pieces.push(Piece {
            triangles: cuboid(
                Vec3::new(4.98, 0.0, z - 0.02),
                Vec3::new(5.02, 0.64, z + 0.02),
            ),
            color: metal,
        });
    }
    park.rails.push(Rail { start: a, end: b });
    park
}

/// Flat-shaded mesh from triangles.
pub fn mesh(triangles: &[[Vec3; 3]]) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let mut positions = Vec::with_capacity(triangles.len() * 3);
    let mut normals = Vec::with_capacity(triangles.len() * 3);
    for [a, b, c] in triangles {
        let n = (*b - *a).cross(*c - *a).normalize_or_zero();
        // Render both faces: emit the triangle and its reverse.
        for (p, nn) in [(*a, n), (*b, n), (*c, n), (*a, -n), (*c, -n), (*b, -n)] {
            positions.push(p.to_array());
            normals.push(nn.to_array());
        }
    }
    let indices = (0..positions.len() as u32).collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn park_spawn_is_on_the_ground_and_rails_exist() {
        let park = build();
        let triangles: Vec<_> = park
            .pieces
            .iter()
            .flat_map(|p| p.triangles.iter().copied())
            .collect();
        let world = p8_sim::World::new(triangles, park.rails.clone());
        let hit = world
            .raycast(park.spawn + Vec3::Y, Vec3::NEG_Y, 2.0)
            .unwrap();
        assert!(hit.distance > 0.99 && hit.normal.y > 0.99);
        assert!(park.rails.len() >= 5);
    }
}
