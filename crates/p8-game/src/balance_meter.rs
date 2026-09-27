//! Draws the balance meter the translated skater shows
//! (`p8_skater::meter_display`) with the player's own HUD textures from
//! `DATA/COMPRESSED/ZONES/global.pak.xen`.
//!
//! The retail screen element system is not translated. How its 640 x 480
//! HUD coordinates reach the screen is APPROXIMATE: here they are scaled by
//! the window height and centred. Colours are the scripts' RGBA with 128 as
//! full strength (INFERRED: the scripts use [128 128 128] as the untinted
//! colour of textured sprites); the alpha is RGBA alpha / 128 (at most 1)
//! times the element's `alpha`.
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use p8_skater::meter_display::Sprite;

use crate::translated::Skater;

/// The HUD's screen size in retail units.
const HUD: Vec2 = Vec2::new(640.0, 480.0);

/// Where the meter's textures come from, and what went wrong reading them.
#[derive(Resource)]
pub struct MeterTextures {
    /// Texture name, its image and size in texels.
    loaded: Vec<(&'static str, Handle<Image>, Vec2)>,
    pub note: String,
}

/// One on-screen sprite of the meter, in the order of
/// `MeterDisplay::sprites`.
#[derive(Component)]
pub(crate) struct MeterSprite(usize);

const NAMES: [&str; 4] = ["balancemeter_bg", "balancemeter", "balancemeter_2", "balancearrow_glow"];

/// The player's `ZONES` folder, if known.
#[derive(Resource)]
pub struct ZonesDir(pub Option<PathBuf>);

/// Load the four textures (`zones` is the player's `ZONES` folder).
fn load(zones: Option<PathBuf>, images: &mut Assets<Image>) -> MeterTextures {
    let Some(dir) = zones else {
        return MeterTextures { loaded: Vec::new(), note: "no ZONES folder".into() };
    };
    let pak = dir.join("global.pak.xen");
    let mut loaded = Vec::new();
    let mut problems = Vec::new();
    for name in NAMES {
        match p8_formats::texture::load_img(&pak, name) {
            Ok(t) => {
                let size = Vec2::new(t.width as f32, t.height as f32);
                let image = Image::new(
                    Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
                    TextureDimension::D2,
                    t.pixels,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                loaded.push((name, images.add(image), size));
            }
            Err(e) => problems.push(e),
        }
    }
    let note = if problems.is_empty() { "balance meter textures loaded".into() } else { problems.join("; ") };
    MeterTextures { loaded, note }
}

pub fn setup(mut commands: Commands, zones: Res<ZonesDir>, mut images: ResMut<Assets<Image>>) {
    let textures = load(zones.0.clone(), &mut images);
    for i in 0..NAMES.len() {
        let image = textures.loaded.iter().find(|(n, ..)| *n == NAMES[i]).map(|(_, h, _)| h.clone());
        commands.spawn((
            MeterSprite(i),
            ImageNode { image: image.unwrap_or_default(), ..default() },
            Node { position_type: PositionType::Absolute, ..default() },
            UiTransform::default(),
            Visibility::Hidden,
            // z_priority: the background 1, the halves 2, the arrow 3.
            GlobalZIndex([1, 2, 2, 3][i]),
        ));
    }
    commands.insert_resource(textures);
}

fn colour(s: &Sprite) -> Color {
    let c = |v: u8| (v as f32 / 128.0).min(1.0);
    Color::srgba(c(s.rgba[0]), c(s.rgba[1]), c(s.rgba[2]), c(s.rgba[3]) * s.alpha)
}

pub fn draw(
    skater: Res<Skater>,
    textures: Option<Res<MeterTextures>>,
    window: Query<&Window>,
    mut sprites: Query<(&MeterSprite, &mut Node, &mut ImageNode, &mut UiTransform, &mut Visibility)>,
) {
    let (Ok(window), Some(textures)) = (window.single(), textures) else {
        return;
    };
    let display = &skater.object.physics.balance.display;
    let list = display.sprites(&skater.scripts);
    // 640 x 480 -> the window: scaled by height, centred (APPROXIMATE).
    let scale = window.height() / HUD.y;
    let left = (window.width() - HUD.x * scale) * 0.5;
    let turn = Rot2::degrees(display.container_rot);
    for (which, mut node, mut image, mut transform, mut visible) in &mut sprites {
        let Some(s) = list.get(which.0) else {
            *visible = Visibility::Hidden;
            continue;
        };
        let Some((_, _, size)) = textures.loaded.iter().find(|(n, ..)| *n == s.texture) else {
            *visible = Visibility::Hidden;
            continue;
        };
        let size = *size * s.scale;
        // The sprite's centre: `pos` is the justified point; the container
        // turns everything about its own position (its size is 0).
        let centre_in = s.pos - Vec2::new(s.just[0], s.just[1]) * size * 0.5;
        let centre = display.container_pos + turn * centre_in;
        let top_left = (centre - size * 0.5) * scale;
        node.left = px(left + top_left.x);
        node.top = px(top_left.y);
        node.width = px(size.x * scale);
        node.height = px(size.y * scale);
        // Retail angles turn clockwise on screen (INFERRED: the arrow then
        // leans the way the arc falls away), as UiTransform does.
        transform.rotation = Rot2::degrees(display.container_rot + s.rot);
        image.color = colour(s);
        *visible = Visibility::Inherited;
    }
}
