//! The skater's model: a pro skater's skinned mesh, textures and skeleton
//! from the player's own files, in place of the placeholder capsule.
//!
//! Which skater: the ped profile `Pro_Hawk` (APPROXIMATE: the player's
//! skater choice / create-a-skater is not read yet). Its `skeletonname`
//! names the skeleton in `ZONES/global.pak.xen`; its `ped_body.desc_id`
//! finds the model path in the script array `ped_body` (`mesh`), read as
//! `MODELS/.../<name>.skin.xen` with textures in `<name>.tex.xen`.
//!
//! Animation: [`RidingPose`] plays the standing ride pose, the ground
//! tree's base `Sk8_Gnd_Stnd_Base_xx` with the difference clip
//! `Sk8_Gnd_Stnd_Slow_Idle01_xDx` applied (the `applydifference` node of
//! `OnGround_AnimBranch`), the idle looping. APPROXIMATE: the rest of the
//! tree (speed and crouch blends, other idles, air, tricks, stance flip)
//! is not translated yet, so this pose is shown in every state. Not used
//! yet: normal maps, the other texture layers and
//! the materials' blend modes (hair and eyelashes use alpha cut-outs here:
//! APPROXIMATE).
use std::path::{Path, PathBuf};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use p8_formats::anim::{self, Clip};
use p8_formats::qb::Value;
use p8_formats::scene::Scene;
use p8_formats::skeleton::Skeleton;
use p8_formats::texture::{self, DictTexture};
use p8_skater::Scripts;

/// The ped profile used for the player's skater (see the module notes).
pub const PROFILE: &str = "Pro_Hawk";

/// Find `rel` (a game path such as `models\Skater_pro\pro_hawk.skin`) under
/// `root`, matching each folder and file name without regard to case.
fn find_path(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|p| !p.is_empty()) {
        let want = part.to_ascii_lowercase();
        let hit = std::fs::read_dir(&at).ok()?.flatten().find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == want)?;
        at = hit.path();
    }
    Some(at)
}

/// What the profile names: the skeleton checksum and the model path.
fn profile_files(s: &Scripts, profile: &str) -> Result<(u32, String), String> {
    let k = p8_formats::qb_key;
    let p = s.global(profile).ok_or_else(|| format!("no {profile} in the scripts"))?;
    let skeleton = match p.get(k("skeletonname")) {
        Some(Value::Checksum(c)) => *c,
        _ => return Err(format!("{profile} has no skeletonname")),
    };
    let desc = match p.get(k("ped_body")).and_then(|b| b.get(k("desc_id"))) {
        Some(Value::Checksum(c)) => *c,
        _ => return Err(format!("{profile} has no ped_body desc_id")),
    };
    let Some(Value::Array(bodies)) = s.global("ped_body") else { return Err("no ped_body table".into()) };
    let mesh = bodies
        .iter()
        .find(|b| matches!(b.get(k("desc_id")), Some(Value::Checksum(c)) if *c == desc))
        .and_then(|b| match b.get(k("mesh")) {
            Some(Value::String(m)) => Some(m.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("no ped_body entry for {profile}"))?;
    Ok((skeleton, mesh))
}

fn image(t: &texture::Rgba8) -> Image {
    let mut img = Image::new(
        Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        t.pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

/// The looping ride pose (see the module notes) on the model's joints.
#[derive(Component)]
pub struct RidingPose {
    joints: Vec<Entity>,
    bind: Vec<Transform>,
    base: Clip,
    idle: Clip,
    time: f32,
}

/// Clip names of the ride pose: the base and the difference applied to it.
const RIDE_BASE: &str = "Sk8_Gnd_Stnd_Base_xx";
const RIDE_IDLE: &str = "Sk8_Gnd_Stnd_Slow_Idle01_xDx";

fn load_ride(compressed: &Path) -> Result<(Clip, Clip), String> {
    let data = compressed.parent().ok_or("no data folder")?;
    let std_q_path = find_path(data, "ANIMS/standardkeyQ.bin.xen").ok_or("ANIMS/standardkeyQ.bin.xen not found")?;
    let std_q = std::fs::read(&std_q_path).map_err(|e| format!("{}: {e}", std_q_path.display()))?;
    let pak = find_path(compressed, "PAK/perm_anims.pak.xen").ok_or("PAK/perm_anims.pak.xen not found")?;
    let keys = [p8_formats::qb_key(RIDE_BASE), p8_formats::qb_key(RIDE_IDLE)];
    let mut clips = anim::load_all(&pak, &keys, &std_q)?;
    let idle = clips.pop().ok_or("idle clip missing")?;
    let base = clips.pop().ok_or("base clip missing")?;
    Ok((base, idle))
}

/// Advance the ride pose and write it to the joints. The idle loops over
/// its duration (INFERRED from the tree's `cycle` use; the node's own
/// timing is not translated yet).
pub fn animate(time: Res<Time>, mut poses: Query<&mut RidingPose>, mut joints: Query<&mut Transform>) {
    for mut r in &mut poses {
        let d = r.idle.duration;
        r.time = if d > 0.0 { (r.time + time.delta_secs()) % d } else { 0.0 };
        let pose = anim::apply_difference(&r.base.sample(0.0), &r.idle.sample(r.time));
        for (i, &e) in r.joints.iter().enumerate() {
            let Ok(mut tf) = joints.get_mut(e) else { continue };
            let bind = r.bind[i];
            match pose.get(i) {
                Some(b) if b.weight > 0.0 => {
                    tf.rotation = Quat::from_array(b.bevy_rotation()).normalize();
                    tf.translation = b.t.map(Vec3::from).unwrap_or(bind.translation);
                }
                _ => *tf = bind,
            }
        }
    }
}

/// Build the model under `parent` (the skater's root). Returns a short
/// description, or why it could not be loaded.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    commands: &mut Commands,
    parent: Entity,
    s: &Scripts,
    compressed: &Path,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Result<String, String> {
    let (skel_key, mesh_path) = profile_files(s, PROFILE)?;
    let skeleton = Skeleton::load(&compressed.join("ZONES").join("global.pak.xen"), skel_key)?;
    let skin = find_path(compressed, &format!("{mesh_path}.xen")).ok_or_else(|| format!("{mesh_path}.xen not found"))?;
    let scene = Scene::load(&skin)?;
    let tex_path = PathBuf::from(skin.to_string_lossy().replace(".skin.xen", ".tex.xen"));
    let textures: Vec<DictTexture> = texture::load_dictionary(&tex_path).unwrap_or_default();

    // Joints: the bind pose hierarchy under a model root at the object's
    // origin (the model faces +Z like the physics matrix's third row).
    let model_root = commands.spawn((Transform::default(), Visibility::default(), ChildOf(parent))).id();
    let mut joints: Vec<Entity> = Vec::with_capacity(skeleton.bones.len());
    let mut world: Vec<Mat4> = Vec::with_capacity(skeleton.bones.len());
    let mut bind_local: Vec<Transform> = Vec::with_capacity(skeleton.bones.len());
    for b in &skeleton.bones {
        let local = Transform {
            translation: Vec3::from(b.position),
            rotation: Quat::from_array(b.rotation).normalize(),
            scale: Vec3::ONE,
        };
        let (up, up_world) = match b.parent {
            Some(p) if p < joints.len() => (joints[p], world[p]),
            _ => (model_root, Mat4::IDENTITY),
        };
        world.push(up_world * local.to_matrix());
        bind_local.push(local);
        joints.push(commands.spawn((local, ChildOf(up))).id());
    }
    let inverse: Vec<Mat4> = world.iter().map(|m| m.inverse()).collect();
    let bind = bindposes.add(SkinnedMeshInverseBindposes::from(inverse));

    let mut made = std::collections::HashMap::new();
    for m in &scene.meshes {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(m.joints.clone()));
        let weights: Vec<[f32; 4]> = m
            .weights
            .iter()
            .map(|w| {
                let sum: f32 = w.iter().sum();
                if sum > 0.0 { w.map(|x| x / sum) } else { [1.0, 0.0, 0.0, 0.0] }
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
        mesh.insert_indices(Indices::U32(m.indices.clone()));
        // The material's colour texture: the dictionary entry of type 0.
        let colour = scene
            .materials
            .iter()
            .find(|x| x.name == m.material)
            .and_then(|mat| mat.textures.iter().find_map(|t| textures.iter().find(|d| d.name == *t && d.kind == 0)));
        let material = match colour.and_then(|d| d.image.as_ref().ok().map(|i| (d.name, i))) {
            Some((name, img)) => made
                .entry(name)
                .or_insert_with(|| {
                    materials.add(StandardMaterial {
                        base_color_texture: Some(images.add(image(img))),
                        perceptual_roughness: 0.85,
                        alpha_mode: AlphaMode::Mask(0.5),
                        ..default()
                    })
                })
                .clone(),
            None => materials.add(Color::srgb(0.7, 0.7, 0.7)),
        };
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            SkinnedMesh { inverse_bindposes: bind.clone(), joints: joints.clone() },
            ChildOf(model_root),
        ));
    }
    let anim_note = match load_ride(compressed) {
        Ok((base, idle)) => {
            let note = format!("ride pose {RIDE_BASE} + {RIDE_IDLE}");
            commands.entity(model_root).insert(RidingPose { joints, bind: bind_local, base, idle, time: 0.0 });
            note
        }
        Err(e) => format!("no animation ({e})"),
    };
    Ok(format!("{PROFILE}: {} meshes, {} bones, {anim_note}", scene.meshes.len(), skeleton.bones.len()))
}
