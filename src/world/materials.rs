//! PBR materials built from CC0 Poly Haven texture sets (colour, normal and
//! AO/roughness/metal maps), plus a few solid materials.

use bevy::{
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};

#[derive(Resource, Clone)]
pub struct SurfaceMaterials {
    pub wallpaper: Handle<StandardMaterial>,
    pub peeling_wall: Handle<StandardMaterial>,
    pub plaster_ceiling: Handle<StandardMaterial>,
    pub wood_floor: Handle<StandardMaterial>,
    pub carpet: Handle<StandardMaterial>,
    pub trim_wood: Handle<StandardMaterial>,
    pub door_wood: Handle<StandardMaterial>,
    pub panel_wood: Handle<StandardMaterial>,
    pub brass: Handle<StandardMaterial>,
    pub iron: Handle<StandardMaterial>,
    pub void: Handle<StandardMaterial>,
    // Area materials (Phase 2).
    pub concrete_wall: Handle<StandardMaterial>,
    pub concrete_floor: Handle<StandardMaterial>,
    pub dirt: Handle<StandardMaterial>,
    pub marble: Handle<StandardMaterial>,
    pub corrugated: Handle<StandardMaterial>,
    pub brick: Handle<StandardMaterial>,
    pub rock: Handle<StandardMaterial>,
    pub rusty_metal: Handle<StandardMaterial>,
    pub metal_plate: Handle<StandardMaterial>,
    pub red_carpet: Handle<StandardMaterial>,
    pub archive_wall: Handle<StandardMaterial>,
}

fn repeat_sampler() -> ImageSampler {
    let mut d = ImageSamplerDescriptor::linear();
    d.address_mode_u = ImageAddressMode::Repeat;
    d.address_mode_v = ImageAddressMode::Repeat;
    d.set_anisotropic_filter(16);
    ImageSampler::Descriptor(d)
}

fn load_tex(asset_server: &AssetServer, path: String, srgb: bool) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = repeat_sampler();
        })
        .load(path)
}

/// Builds a material from `textures/<name>/<name>_{diff,nor,arm}.jpg`.
fn pbr_set(asset_server: &AssetServer, name: &str, tint: Color) -> StandardMaterial {
    let p = |suffix: &str| format!("textures/{name}/{name}_{suffix}.jpg");
    let arm = load_tex(asset_server, p("arm"), false);
    StandardMaterial {
        base_color: tint,
        base_color_texture: Some(load_tex(asset_server, p("diff"), true)),
        normal_map_texture: Some(load_tex(asset_server, p("nor"), false)),
        metallic_roughness_texture: Some(arm.clone()),
        occlusion_texture: Some(arm),
        perceptual_roughness: 1.0,
        metallic: 1.0, // scaled by the texture's blue channel
        ..default()
    }
}

pub fn build(asset_server: &AssetServer, materials: &mut Assets<StandardMaterial>) -> SurfaceMaterials {
    SurfaceMaterials {
        // Slightly desaturated tints push everything toward the game's cold palette.
        wallpaper: materials.add(pbr_set(asset_server, "decrepit_wallpaper", Color::srgb(0.85, 0.85, 0.8))),
        peeling_wall: materials.add(pbr_set(asset_server, "peeling_painted_wall", Color::srgb(0.7, 0.75, 0.75))),
        plaster_ceiling: materials.add(pbr_set(asset_server, "painted_plaster_wall", Color::srgb(0.85, 0.83, 0.8))),
        wood_floor: materials.add(pbr_set(asset_server, "old_wood_floor", Color::srgb(0.8, 0.75, 0.7))),
        carpet: materials.add(pbr_set(asset_server, "dirty_carpet", Color::srgb(0.75, 0.7, 0.7))),
        trim_wood: materials.add(pbr_set(asset_server, "wood_table_001", Color::srgb(0.55, 0.45, 0.4))),
        door_wood: materials.add(pbr_set(asset_server, "dark_wooden_planks", Color::srgb(0.9, 0.85, 0.8))),
        panel_wood: materials.add(pbr_set(asset_server, "wood_table_001", Color::srgb(0.42, 0.33, 0.28))),
        brass: materials.add(StandardMaterial {
            base_color: Color::srgb(0.78, 0.6, 0.32),
            metallic: 1.0,
            perceptual_roughness: 0.32,
            ..default()
        }),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.12, 0.13),
            metallic: 0.9,
            perceptual_roughness: 0.55,
            ..default()
        }),
        void: materials.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() }),
        concrete_wall: materials.add(pbr_set(asset_server, "concrete_wall_006", Color::srgb(0.75, 0.77, 0.75))),
        concrete_floor: materials.add(pbr_set(asset_server, "concrete_tiles", Color::srgb(0.7, 0.7, 0.68))),
        dirt: materials.add(pbr_set(asset_server, "dirt_floor", Color::srgb(0.8, 0.72, 0.62))),
        marble: materials.add(pbr_set(asset_server, "marble_01", Color::srgb(0.85, 0.83, 0.8))),
        corrugated: materials.add(pbr_set(asset_server, "corrugated_iron_02", Color::srgb(0.7, 0.72, 0.7))),
        brick: materials.add(pbr_set(asset_server, "dark_brick_wall", Color::srgb(0.8, 0.75, 0.72))),
        rock: materials.add(pbr_set(asset_server, "rock_wall_08", Color::srgb(0.8, 0.74, 0.66))),
        rusty_metal: materials.add(pbr_set(asset_server, "rusty_metal_02", Color::WHITE)),
        metal_plate: materials.add(pbr_set(asset_server, "metal_plate", Color::srgb(0.75, 0.77, 0.8))),
        red_carpet: materials.add(pbr_set(asset_server, "dirty_carpet", Color::srgb(0.75, 0.16, 0.14))),
        archive_wall: materials.add(pbr_set(asset_server, "dark_wooden_planks", Color::srgb(0.62, 0.52, 0.45))),
    }
}
