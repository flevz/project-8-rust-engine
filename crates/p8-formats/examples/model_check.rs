//! `cargo run -p p8-formats --example model_check -- <DATA/COMPRESSED folder> [pro_hawk] [Pros_Hawk_skel] [dump dir]`
//! Loads a pro skater's model, textures and skeleton and prints what it
//! found; with a dump folder, writes each colour texture as raw RGBA.
use p8_formats::{scene::Scene, skeleton::Skeleton, texture};
use std::path::PathBuf;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&a[1]);
    let model = a.get(2).map_or("pro_hawk", |s| s.as_str());
    let skel = a.get(3).map_or("Pros_Hawk_skel", |s| s.as_str());
    let dir = root.join("MODELS").join("SKATER_PRO");
    let scene = Scene::load(&dir.join(format!("{model}.skin.xen"))).unwrap();
    let tex = texture::load_dictionary(&dir.join(format!("{model}.tex.xen"))).unwrap();
    let ske = Skeleton::load(&root.join("ZONES").join("global.pak.xen"), p8_formats::qb_key(skel)).unwrap();
    println!("{} bones, {} materials, {} meshes, {} textures", ske.bones.len(), scene.materials.len(), scene.meshes.len(), tex.len());
    for m in &scene.meshes {
        let mat = scene.materials.iter().find(|x| x.name == m.material).unwrap();
        let colour = mat.textures.iter().find_map(|t| tex.iter().find(|d| d.name == *t && d.kind == 0));
        let max_joint = m.joints.iter().flatten().max().copied().unwrap_or(0);
        println!(
            "  mesh {:08x}: {} vertices, {} triangles, max bone {}, colour texture {}",
            m.material,
            m.positions.len(),
            m.indices.len() / 3,
            max_joint,
            colour.map_or("none".to_string(), |d| match &d.image {
                Ok(i) => format!("{:08x} {}x{}", d.name, i.width, i.height),
                Err(e) => e.clone(),
            })
        );
    }
    if let Some(out) = a.get(4) {
        for d in &tex {
            if let Ok(i) = &d.image {
                std::fs::write(format!("{out}/{:08x}_{}_{}x{}.rgba", d.name, d.kind, i.width, i.height), &i.pixels).unwrap();
            }
        }
    }
}
