//! Turns a [`RoomPlan`] into 3D geometry: shell, doors, lights, furniture.
//! Everything is deterministic from the room's seed so rooms can be streamed
//! out and rebuilt identically (state like opened doors persists in
//! [`FloorRuntime`]).

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use super::plan::{door_zones, passage_zones, pillars, stream_bridge, zones_overlap, BRIDGE_HALF, PILLAR_R, STREAM_HALF, Zone, steps_for, Area, Feature, LightState, RoomKind, RoomPlan, LANDING_DEPTH, LOFT_HEIGHT, STEP_RUN, WALL_T};
use super::{FloorRuntime, RoomMember};
use crate::interaction::{AnimatedPart, Door, Drawer, HidingSpot, InteractKind, Interactable, PartMotion};
use crate::physics::{Aabb, Collider, Ground};
use crate::rng::GameRng;
use crate::world::{model, world_aabb, yaw_facing, Builder, Flicker, ModelBindings, Opening, PartBinding, RoomStyle, Side, DOOR_H, DOOR_W};

/// Shared handles used by the primitive props.
#[derive(Resource, Clone)]
pub struct PropMaterials {
    pub books: Vec<Handle<StandardMaterial>>,
    pub paper: Handle<StandardMaterial>,
    pub lamp_glow: Handle<StandardMaterial>,
    pub green_glass: Handle<StandardMaterial>,
    pub hazard: Handle<StandardMaterial>,
    pub unit_cube: Handle<Mesh>,
    pub unit_cyl: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    // Outdoors (The Grounds).
    pub grass: Handle<StandardMaterial>,
    pub hedge: Handle<StandardMaterial>,
    pub leaves: Handle<StandardMaterial>,
    pub bark: Handle<StandardMaterial>,
    pub gravel: Handle<StandardMaterial>,
    pub lantern: Handle<StandardMaterial>,
    pub water: Handle<StandardMaterial>,
    pub cone: Handle<Mesh>,
    pub leaves_red: Handle<StandardMaterial>,
    pub leaves_orange: Handle<StandardMaterial>,
    pub pine: Handle<StandardMaterial>,
    pub birch: Handle<StandardMaterial>,
    pub bramble: Handle<StandardMaterial>,
    pub stone: Handle<StandardMaterial>,
    pub glass: Handle<StandardMaterial>,
    pub rock_mesh: Handle<Mesh>,
    pub pyramid: Handle<Mesh>,
    pub canvas: Handle<StandardMaterial>,
    pub terracotta: Handle<StandardMaterial>,
    pub shade: Handle<StandardMaterial>,
}

/// Small tileable noise textures for the outdoor area (no image files needed).
fn noise_texture(images: &mut Assets<Image>, seed: u64, base: [f32; 3], spread: [f32; 3], blades: bool) -> Handle<Image> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 128;
    let mut rng = GameRng::new(seed);
    let mut px = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let n = rng.next_f32();
            // Vertical streaks read as grass blades / bark grain.
            let streak = if blades { (((x * 7919) ^ (y / 3 * 104729)) % 13) as f32 / 13.0 } else { 0.5 };
            let k = n * 0.6 + streak * 0.4;
            let i = (y * N + x) * 4;
            for c in 0..3 {
                px[i + c] = ((base[c] + (k - 0.5) * spread[c]).clamp(0.0, 1.0) * 255.0) as u8;
            }
            px[i + 3] = 255;
        }
    }
    images.add(Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default()))
}

impl PropMaterials {
    pub fn new(mats: &mut Assets<StandardMaterial>, meshes: &mut Assets<Mesh>, images: &mut Assets<Image>, assets: &AssetServer) -> Self {
        // Outdoor ground and stone: Poly Haven textures (ambientCG grass, Poly Haven gravel and mossy stone).
        let pbr = |name: &str| crate::world::materials::pbr_set(assets, name, Color::WHITE);
        let tex = |images: &mut Assets<Image>, seed, base, spread, blades| noise_texture(images, seed, base, spread, blades);
        let hedge_tex = tex(images, 12, [0.07, 0.17, 0.07], [0.08, 0.14, 0.07], false);
        let bark_tex = tex(images, 13, [0.22, 0.15, 0.1], [0.12, 0.08, 0.06], true);
        let book = |c: Color| StandardMaterial { base_color: c, perceptual_roughness: 0.75, ..default() };
        Self {
            books: [
                Color::srgb(0.35, 0.07, 0.06),
                Color::srgb(0.09, 0.2, 0.12),
                Color::srgb(0.3, 0.2, 0.1),
                Color::srgb(0.08, 0.1, 0.22),
                Color::srgb(0.45, 0.33, 0.12),
                Color::srgb(0.18, 0.16, 0.14),
                Color::srgb(0.5, 0.45, 0.38),
            ]
            .into_iter()
            .map(|c| mats.add(book(c)))
            .collect(),
            paper: mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.82, 0.72), perceptual_roughness: 0.9, ..default() }),
            lamp_glow: mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(14.0, 9.0, 4.5), ..default() }),
            green_glass: mats.add(StandardMaterial { base_color: Color::srgb(0.05, 0.3, 0.12), emissive: LinearRgba::rgb(0.6, 2.4, 0.9), perceptual_roughness: 0.2, ..default() }),
            hazard: mats.add(StandardMaterial { base_color: Color::srgb(0.75, 0.6, 0.08), perceptual_roughness: 0.6, ..default() }),
            unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            unit_cyl: meshes.add(Cylinder::new(0.5, 1.0)),
            sphere: meshes.add(Sphere::new(0.5).mesh().uv(20, 12)),
            grass: mats.add(pbr("grass_004")),
            hedge: mats.add(StandardMaterial { base_color_texture: Some(hedge_tex), perceptual_roughness: 0.9, ..default() }),
            leaves: mats.add(StandardMaterial { base_color: Color::srgb(0.6, 0.85, 0.55), base_color_texture: Some(tex(images, 15, [0.1, 0.2, 0.08], [0.1, 0.16, 0.08], false)), perceptual_roughness: 0.85, ..default() }),
            bark: mats.add(StandardMaterial { base_color_texture: Some(bark_tex), perceptual_roughness: 0.9, ..default() }),
            gravel: mats.add(pbr("gravel")),
            lantern: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.6), emissive: LinearRgba::rgb(9.0, 6.0, 3.0), ..default() }),
            water: mats.add(StandardMaterial { base_color: Color::srgb(0.03, 0.06, 0.09), perceptual_roughness: 0.05, reflectance: 0.8, ..default() }),
            cone: meshes.add(Cone { radius: 0.5, height: 1.0 }),
            leaves_red: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.45), base_color_texture: Some(tex(images, 16, [0.5, 0.12, 0.07], [0.2, 0.08, 0.05], false)), perceptual_roughness: 0.8, ..default() }),
            leaves_orange: mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.75, 0.5), base_color_texture: Some(tex(images, 17, [0.6, 0.28, 0.08], [0.2, 0.1, 0.05], false)), perceptual_roughness: 0.8, ..default() }),
            pine: mats.add(StandardMaterial { base_color_texture: Some(tex(images, 18, [0.05, 0.11, 0.07], [0.04, 0.08, 0.05], false)), perceptual_roughness: 0.9, ..default() }),
            birch: mats.add(StandardMaterial { base_color_texture: Some(tex(images, 19, [0.78, 0.76, 0.72], [0.3, 0.3, 0.3], true)), perceptual_roughness: 0.85, ..default() }),
            bramble: mats.add(StandardMaterial { base_color: Color::srgb(0.07, 0.09, 0.06), perceptual_roughness: 0.7, ..default() }),
            stone: mats.add(pbr("mossy_stone_wall")),
            glass: mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.85, 0.55, 0.5), emissive: LinearRgba::rgb(5.0, 3.2, 1.4), alpha_mode: AlphaMode::Blend, ..default() }),
            rock_mesh: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
            pyramid: meshes.add(Cone { radius: 0.5, height: 1.0 }.mesh().resolution(4)),
            canvas: mats.add(StandardMaterial { base_color: Color::srgb(0.78, 0.74, 0.64), perceptual_roughness: 0.95, cull_mode: None, double_sided: true, ..default() }),
            terracotta: mats.add(StandardMaterial { base_color: Color::srgb(0.55, 0.27, 0.16), perceptual_roughness: 0.85, ..default() }),
            shade: mats.add(StandardMaterial { base_color: Color::srgb(0.95, 0.8, 0.55), emissive: LinearRgba::rgb(2.2, 1.4, 0.6), cull_mode: None, double_sided: true, ..default() }),
        }
    }
}

fn style_for(area: Area, b: &Builder, props: &PropMaterials) -> RoomStyle {
    let m = &b.mats;
    match area {
        Area::GuestWing => RoomStyle { wall: m.wallpaper.clone(), wall_tile: 1.6, floor: m.wood_floor.clone(), floor_tile: 2.4, wainscot: true, sky: false },
        Area::Archive => RoomStyle { wall: m.archive_wall.clone(), wall_tile: 2.0, floor: m.wood_floor.clone(), floor_tile: 2.0, wainscot: true, sky: false },
        Area::ServiceTunnels => RoomStyle { wall: m.concrete_wall.clone(), wall_tile: 2.5, floor: m.concrete_floor.clone(), floor_tile: 2.0, wainscot: false, sky: false },
        Area::QuarryLift => RoomStyle { wall: m.rock.clone(), wall_tile: 3.0, floor: m.dirt.clone(), floor_tile: 2.5, wainscot: false, sky: false },
        Area::Atrium => RoomStyle { wall: m.brick.clone(), wall_tile: 2.2, floor: m.marble.clone(), floor_tile: 3.0, wainscot: true, sky: false },
        Area::Grounds => RoomStyle { wall: m.brick.clone(), wall_tile: 2.2, floor: props.grass.clone(), floor_tile: 1.5, wainscot: false, sky: true },
    }
}

/// Doors in the industrial areas use rusted metal instead of wood.
fn with_area_door_mats(b: &mut Builder, area: Area) -> crate::world::materials::SurfaceMaterials {
    let saved = b.mats.clone();
    if matches!(area, Area::ServiceTunnels | Area::QuarryLift) {
        b.mats.door_wood = saved.rusty_metal.clone();
        b.mats.panel_wood = saved.metal_plate.clone();
        b.mats.trim_wood = saved.metal_plate.clone();
    }
    saved
}

/// One furniture slot against a side wall: world position on the wall face and
/// the yaw that makes local +Z face into the room.
#[derive(Clone, Copy)]
struct Slot {
    x: f32,
    z: f32,
    yaw: f32,
    inward: f32,
    /// Floor height under the slot (lofts).
    y: f32,
}

/// Floor footprint of furniture standing in a wall slot (up to 1 m deep).
fn slot_footprint(s: &Slot) -> Zone {
    let (ax, bx) = (s.x, s.x + s.inward * 1.0);
    (Vec2::new(ax.min(bx), s.z - 0.65), Vec2::new(ax.max(bx), s.z + 0.65))
}

fn slot_free(s: &Slot, blocked: &[Zone]) -> bool {
    let f = slot_footprint(s);
    !blocked.iter().any(|b| zones_overlap(&f, b))
}

fn side_slots(r: &RoomPlan, rng: &mut GameRng) -> Vec<Slot> {
    let mut slots = vec![];
    let mut z = r.max.y - 1.1;
    while z > r.min.y + 1.1 {
        slots.push(Slot { x: r.min.x, z, yaw: FRAC_PI_2, inward: 1.0, y: 0.0 });
        slots.push(Slot { x: r.max.x, z, yaw: -FRAC_PI_2, inward: -1.0, y: 0.0 });
        z -= 1.95;
    }
    // Shuffle deterministically.
    for i in (1..slots.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        slots.swap(i, j);
    }
    slots
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_room(
    b: &mut Builder,
    r: &RoomPlan,
    rt: &FloorRuntime,
    props: &PropMaterials,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    std_mats: &mut Assets<StandardMaterial>,
) {
    b.member = Some(r.index);
    let mut rng = GameRng::new(r.seed);
    let style = style_for(r.area, b, props);
    let h = r.height;

    // ---- Shell with exit (and mimic) openings ----
    let raised = match r.feature {
        Some(Feature::RaisedExit { height }) => height,
        _ => 0.0,
    };
    let mut openings = vec![Opening { side: Side::North, center: r.exit_x, width: DOOR_W, height: DOOR_H, bottom: raised }];
    // Fake doors get real openings too (so their leaves show), bricked up behind.
    for m in &r.mimics {
        openings.push(Opening { side: Side::North, center: m.x, width: DOOR_W, height: DOOR_H, bottom: 0.0 });
    }
    b.room(r.min, r.max, h, &style, &openings, &[Side::South]);

    // ---- Exit door (and mimic) ----
    let saved = with_area_door_mats(b, r.area);
    let exit_no = r.exit_door();
    let opened = rt.opened.contains(&exit_no);
    b.base_y = raised;
    let door = b.door(r.exit_x, r.min.y, exit_no, false, images, std_mats);
    b.base_y = 0.0;
    let blocker = {
        let mut d = Door::new(exit_no, false, Entity::PLACEHOLDER);
        d.key_room = (r.kind == RoomKind::KeyPuzzle && !rt.keys.contains(&r.index)).then_some(r.index);
        d.locked = d.key_room.is_some();
        d
    };
    b.commands.entity(door).entry::<Door>().and_modify(move |mut d| {
        d.key_room = blocker.key_room;
        d.locked = blocker.locked;
    });
    if opened {
        b.commands.entity(door).entry::<Door>().and_modify(|mut d| {
            d.open = true;
            d.t = 1.0;
            d.locked = false;
        });
        b.commands.entity(door).remove::<Interactable>();
        // The blocker collider is a separate entity; find and clear it once spawned.
        b.commands.entity(door).insert(OpenOnSpawn);
    }
    for (slot, m) in r.mimics.iter().enumerate() {
        let slot = slot as u8;
        // A full door assembly set flush against the wall, with no opening behind it.
        let tf = Transform::from_xyz(m.x, 0.0, r.min.y);
        b.decor_door(tf, m.shown_number, images, std_mats);
        let back = style.wall.clone();
        let tile = style.wall_tile;
        b.block(Vec3::new(m.x - DOOR_W / 2.0, 0.0, r.min.y - WALL_T), Vec3::new(m.x + DOOR_W / 2.0, DOOR_H, r.min.y - WALL_T + 0.03), &back, tile, true);
        if !rt.mimics_triggered.contains(&(r.index, slot)) {
            let mut d = Door::new(m.shown_number, false, Entity::PLACEHOLDER);
            d.mimic = true;
            let e = b
                .commands
                .spawn((
                    Transform::from_xyz(m.x, 0.0, r.min.y),
                    d,
                    MimicOf(r.index, slot),
                    Interactable {
                        kind: InteractKind::Door,
                        aabb: Aabb { min: Vec3::new(m.x - 0.5, 0.0, r.min.y - 0.05), max: Vec3::new(m.x + 0.5, DOOR_H, r.min.y + 0.12) },
                    },
                ))
                .id();
            b.tag(e);
        }
    }
    b.mats = saved;

    // ---- Lighting ----
    let area_m2 = r.width() * r.depth();
    let base = 450_000.0 * (area_m2 / 40.0).clamp(0.8, 2.6) * if r.area == Area::Atrium { 1.8 } else { 1.0 };
    let (scale, depth) = match r.light {
        LightState::Bright => (1.0, 0.05),
        LightState::Dim => (0.62, 0.15),
        LightState::Dark => (0.07, 0.8),
    };
    let warm = match r.area {
        Area::ServiceTunnels => Color::srgb(0.9, 0.95, 1.0),
        Area::QuarryLift => Color::srgb(1.0, 0.7, 0.4),
        _ => Color::srgb(1.0, 0.78, 0.52),
    };
    let center = r.center();
    let lamp_y = if r.area == Area::Atrium { h - 1.5 } else { h - 0.9 };
    let lamp_points: Vec<Vec2> = if r.kind == RoomKind::Hall || r.depth() > 11.0 {
        // Garden lamp posts stand further apart than ceiling lamps.
        let n = (r.depth() / if r.area.outdoors() { 8.0 } else { 5.0 }).ceil() as usize;
        (0..n).map(|i| Vec2::new(center.x, r.max.y - (i as f32 + 0.5) * r.depth() / n as f32)).collect()
    } else if r.area == Area::Atrium {
        vec![Vec2::new(center.x, center.y + r.depth() * 0.22), Vec2::new(center.x, center.y - r.depth() * 0.22)]
    } else {
        vec![center]
    };
    let per_lamp = base * scale * rt.light_scale / lamp_points.len() as f32;
    // Outdoor lamp posts stand beside the gravel path, not on it.
    let post_x = |x: f32| {
        let right = x + 1.4;
        if (right - r.exit_x).abs() < 1.3 || (right - r.entry_x).abs() < 1.3 { x - 1.4 } else { right }
    };
    for (i, p) in lamp_points.iter().enumerate() {
        match r.area {
            Area::ServiceTunnels | Area::QuarryLift => caged_bulb(b, props, Vec3::new(p.x, h, p.y), 0.7),
            Area::Grounds => lamp_post(b, asset_server, Vec3::new(post_x(p.x), 0.0, p.y)),
            _ => {
                let e = b
                    .commands
                    .spawn((model(asset_server, "modern_ceiling_lamp_01"), Transform::from_xyz(p.x, h - 1.17, p.y)))
                    .id();
                b.tag(e);
            }
        }
        let e = b
            .commands
            .spawn((
                PointLight { color: warm, intensity: per_lamp, range: 18.0, radius: 0.05, shadow_maps_enabled: i == 0 && r.light != LightState::Dark, ..default() },
                Flicker { base: per_lamp, seed: r.index as f32 * 1.37 + i as f32, depth },
                if r.area.outdoors() { Transform::from_xyz(post_x(p.x), 3.1, p.y) } else { Transform::from_xyz(p.x, lamp_y, p.y) },
            ))
            .id();
        b.tag(e);
    }
    // Guide sconces flanking the exit door: always lit, even in dark rooms.
    for dx in [-0.95f32, 0.95] {
        let x = r.exit_x + dx;
        if x < r.min.x + 0.3 || x > r.max.x - 0.3 {
            continue;
        }
        let tf = Transform::from_xyz(x, 2.05 + raised, r.min.y);
        let e = b.commands.spawn((model(asset_server, "industrial_wall_sconce"), tf)).id();
        b.tag(e);
        let e = b
            .commands
            .spawn((
                PointLight { color: warm, intensity: 40_000.0, range: 6.0, ..default() },
                Flicker { base: 40_000.0, seed: r.index as f32 + dx, depth: 0.06 },
                Transform::from_translation(tf.transform_point(Vec3::new(0.0, 0.12, 0.26))),
            ))
            .id();
        b.tag(e);
    }

    // ---- Scenery (sheds, windmills, benches, barrels, clutter) ----
    let lamps: Vec<Zone> = if r.area.outdoors() { lamp_points.iter().map(|p| (Vec2::new(post_x(p.x), p.y) - Vec2::splat(0.7), Vec2::new(post_x(p.x), p.y) + Vec2::splat(0.7))).collect() } else { vec![] };
    let scenery = {
        let mut srng = GameRng::new(r.seed ^ 0x5CE7_E21E);
        super::scenery::dress_room(b, props, asset_server, r, &lamps, &mut srng)
    };
    // ---- Area architecture ----
    let arch_zones = match r.area {
        Area::QuarryLift => {
            quarry_architecture(b, props, r);
            vec![]
        }
        Area::ServiceTunnels => {
            tunnel_architecture(b, props, r);
            vec![]
        }
        Area::Atrium => {
            atrium_architecture(b, props, r);
            vec![]
        }
        Area::Grounds => grounds_architecture(b, props, asset_server, r, &[scenery.clone(), lamps.clone()].concat(), &mut rng),
        _ => vec![],
    };
    if r.area == Area::Atrium || (r.area == Area::GuestWing && r.kind != RoomKind::Hall && rng.next_f32() < 0.5) {
        let runner = b.mats.red_carpet.clone();
        let (z0, z1) = (r.min.y + 0.4, r.max.y - 0.2);
        let x = (r.entry_x + r.exit_x) / 2.0;
        b.block(Vec3::new(x - 0.8, 0.0, z0), Vec3::new(x + 0.8, 0.012, z1), &runner, 1.4, false);
    }

    // ---- Stairs, landings and lofts ----
    // Furniture against the walls keeps clear of the scenery and garden models too.
    let mut blocked: Vec<(Vec2, Vec2)> = [scenery.clone(), arch_zones].concat();
    let stair_mat = match r.area {
        Area::ServiceTunnels | Area::QuarryLift => b.mats.metal_plate.clone(),
        Area::Atrium => b.mats.marble.clone(),
        _ => b.mats.trim_wood.clone(),
    };
    if raised > 0.0 {
        let (x0, x1) = ((r.exit_x - 0.85).max(r.min.x), (r.exit_x + 0.85).min(r.max.x));
        landing(b, Vec3::new(x0, 0.0, r.min.y), Vec3::new(x1, raised, r.min.y + LANDING_DEPTH), &stair_mat, true);
        // Walkable strip through the wall to the next room's landing.
        ground_only(b, Vec3::new(r.exit_x - 0.5, raised - 0.05, r.min.y - WALL_T - 0.02), Vec3::new(r.exit_x + 0.5, raised, r.min.y));
        let end = stairs(b, r.exit_x - 0.6, r.exit_x + 0.6, r.min.y + LANDING_DEPTH, 1.0, raised, &stair_mat);
        railing(b, Vec3::new(x0, raised, r.min.y + 0.05), Vec3::new(x0, raised, r.min.y + LANDING_DEPTH));
        railing(b, Vec3::new(x1, raised, r.min.y + 0.05), Vec3::new(x1, raised, r.min.y + LANDING_DEPTH));
        blocked.push((Vec2::new(x0 - 0.2, r.min.y), Vec2::new(x1 + 0.2, end + 0.4)));
    }
    if r.entry_height > 0.0 {
        let hgt = r.entry_height;
        let (x0, x1) = ((r.entry_x - 0.85).max(r.min.x), (r.entry_x + 0.85).min(r.max.x));
        landing(b, Vec3::new(x0, 0.0, r.max.y - LANDING_DEPTH), Vec3::new(x1, hgt, r.max.y), &stair_mat, true);
        let end = stairs(b, r.entry_x - 0.6, r.entry_x + 0.6, r.max.y - LANDING_DEPTH, -1.0, hgt, &stair_mat);
        railing(b, Vec3::new(x0, hgt, r.max.y - LANDING_DEPTH), Vec3::new(x0, hgt, r.max.y - 0.05));
        railing(b, Vec3::new(x1, hgt, r.max.y - LANDING_DEPTH), Vec3::new(x1, hgt, r.max.y - 0.05));
        blocked.push((Vec2::new(x0 - 0.2, end - 0.4), Vec2::new(x1 + 0.2, r.max.y)));
    }
    let mut loft_slot = None;
    if let Some(Feature::KeyLoft { east }) = r.feature {
        let (wall, inward) = if east { (r.max.x, -1.0f32) } else { (r.min.x, 1.0f32) };
        let (z0, z1) = (r.min.y + 1.6, r.min.y + 3.8);
        let xa = wall + inward * 1.7;
        let (lx0, lx1) = (wall.min(xa), wall.max(xa));
        landing(b, Vec3::new(lx0, 0.0, z0), Vec3::new(lx1, LOFT_HEIGHT, z1), &stair_mat, false);
        let xb = wall + inward * 1.0;
        let end = stairs(b, wall.min(xb), wall.max(xb), z1, 1.0, LOFT_HEIGHT, &stair_mat);
        // Rails on the open edges of the loft.
        railing(b, Vec3::new(xa, LOFT_HEIGHT, z0), Vec3::new(xa, LOFT_HEIGHT, z1));
        railing(b, Vec3::new(lx0, LOFT_HEIGHT, z0), Vec3::new(lx1, LOFT_HEIGHT, z0));
        // A warm lamp so the loft reads as the place to go.
        let e = b
            .commands
            .spawn((PointLight { color: warm, intensity: 60_000.0, range: 5.0, ..default() }, Transform::from_xyz(wall + inward * 0.8, LOFT_HEIGHT + 1.6, (z0 + z1) / 2.0)))
            .id();
        b.tag(e);
        blocked.push((Vec2::new(lx0 - 0.1, z0 - 0.2), Vec2::new(lx1 + 0.1, end + 0.3)));
        loft_slot = Some(Slot { x: wall, z: (z0 + z1) / 2.0 - 0.3, yaw: if east { -FRAC_PI_2 } else { FRAC_PI_2 }, inward, y: LOFT_HEIGHT });
    }

    // ---- Dividers: zigzag walls, chase barricades, Reading Hall shelves ----
    for d in r.dividers() {
        for (x0, x1) in [(r.min.x, d.gap.0), (d.gap.1, r.max.x)] {
            if x1 - x0 < 0.3 {
                continue;
            }
            match r.feature {
                Some(Feature::ReadingHall) => shelf_row(b, props, x0, x1, d.z, &mut rng),
                _ if r.pursuit => barricade(b, x0, x1, d.z, &mut rng),
                _ => {
                    b.block(Vec3::new(x0, 0.0, d.z - 0.1), Vec3::new(x1, h, d.z + 0.1), &style.wall, style.wall_tile, true);
                    // A dark skirting line so the partition reads as a wall.
                    let trim = b.mats.trim_wood.clone();
                    b.block(Vec3::new(x0, 0.0, d.z - 0.12), Vec3::new(x1, 0.14, d.z + 0.12), &trim, 1.0, false);
                }
            }
        }
        blocked.push((Vec2::new(r.min.x, d.z - 0.6), Vec2::new(r.max.x, d.z + 0.6)));
    }

    // ---- Furniture ----
    // ---- Pillar halls and garden streams ----
    let pillar_mat = match r.area {
        Area::Atrium => b.mats.marble.clone(),
        Area::QuarryLift => b.mats.door_wood.clone(),
        Area::Archive => b.mats.panel_wood.clone(),
        _ => b.mats.trim_wood.clone(),
    };
    for p in pillars(r) {
        let e = b
            .commands
            .spawn((
                Mesh3d(props.unit_cyl.clone()),
                MeshMaterial3d(pillar_mat.clone()),
                Transform::from_xyz(p.x, h / 2.0, p.y).with_scale(Vec3::new(PILLAR_R * 2.0, h, PILLAR_R * 2.0)),
                Collider(Aabb { min: Vec3::new(p.x - PILLAR_R, 0.0, p.y - PILLAR_R), max: Vec3::new(p.x + PILLAR_R, h, p.y + PILLAR_R) }),
            ))
            .id();
        b.tag(e);
        let base = PILLAR_R + 0.1;
        b.block(Vec3::new(p.x - base, 0.0, p.y - base), Vec3::new(p.x + base, 0.22, p.y + base), &pillar_mat, 1.0, false);
        b.block(Vec3::new(p.x - base, h - 0.25, p.y - base), Vec3::new(p.x + base, h, p.y + base), &pillar_mat, 1.0, false);
        blocked.push((Vec2::new(p.x - base - 0.2, p.y - base - 0.2), Vec2::new(p.x + base + 0.2, p.y + base + 0.2)));
    }
    if let Some((z, bx)) = stream_bridge(r) {
        garden_stream(b, props, r, z, bx);
        blocked.push((Vec2::new(r.min.x, z - STREAM_HALF - 0.5), Vec2::new(r.max.x, z + STREAM_HALF + 0.5)));
    }
    // Doors must swing and be reached freely, and the openings in dividers stay clear.
    blocked.extend(door_zones(r));
    blocked.extend(passage_zones(r));
    let mut slots = side_slots(r, &mut rng);
    slots.retain(|s| slot_free(s, &blocked));
    // Atrium columns occupy the side walls; furniture stands between them.
    let narrow = r.width() < 4.0;
    let mut drawer_id: u8 = 0;
    let mut all_drawers: Vec<Entity> = vec![];
    if r.hiding {
        // Two players fit in each spot: make sure the whole party can hide.
        let n = hiding_spots_needed(r.kind == RoomKind::Hall || r.depth() > 10.0, rt.players);
        for _ in 0..n {
            if let Some(s) = slots.pop() {
                if r.area == Area::Grounds {
                    // Wooden garden sheds instead of steel lockers.
                    let saved = b.mats.clone();
                    b.mats.metal_plate = saved.door_wood.clone();
                    b.mats.iron = saved.trim_wood.clone();
                    locker(b, s);
                    b.mats = saved;
                } else if narrow || matches!(r.area, Area::ServiceTunnels | Area::QuarryLift) {
                    locker(b, s);
                } else {
                    wardrobe(b, asset_server, s);
                }
            }
        }
    }
    for c in 0..r.containers {
        let Some(s) = slots.pop() else { break };
        let ids = match r.area {
            Area::ServiceTunnels | Area::QuarryLift | Area::Grounds => tool_chest(b, s, r, rt, &mut drawer_id, c),
            _ => {
                if rng.next_f32() < 0.5 {
                    drawer_cabinet(b, asset_server, s, r, rt, &mut drawer_id)
                } else {
                    commode(b, asset_server, s, r, rt, &mut drawer_id)
                }
            }
        };
        all_drawers.extend(ids);
    }
    // The loft's chest always holds the key.
    if let Some(s) = loft_slot {
        let ids = tool_chest(b, s, r, rt, &mut drawer_id, 1);
        all_drawers = ids;
    }
    // Hide the key in one drawer for key puzzles.
    if r.kind == RoomKind::KeyPuzzle && !rt.keys.contains(&r.index) && !all_drawers.is_empty() {
        let k = all_drawers[rng.below(all_drawers.len() as u32) as usize];
        b.commands.entity(k).entry::<Drawer>().and_modify(|mut d| {
            d.has_key = true;
            d.loot = None;
            d.item = None;
        });
    }
    // Decor in the remaining slots.
    for s in slots.into_iter().take(6) {
        match r.area {
            Area::Archive => bookshelf(b, props, s, &mut rng),
            Area::QuarryLift | Area::ServiceTunnels => crate_stack(b, asset_server, s, &mut rng),
            Area::Grounds => hedge(b, asset_server, s, &mut rng),
            Area::GuestWing | Area::Atrium => {
                if narrow {
                    continue;
                }
                let pick = rng.below(3);
                let name = ["ArmChair_01", "ClassicNightstand_01", "GothicCommode_01"][pick as usize];
                if pick == 2 {
                    // Commodes stay lootable containers only: a plant or a lamp instead.
                    super::scenery::wall_decor(b, props, asset_server, s.x, s.inward, s.yaw, s.z, &mut rng);
                    continue;
                }
                let tf = Transform::from_xyz(s.x + s.inward * 0.45, 0.0, s.z).with_rotation(Quat::from_rotation_y(s.yaw));
                let col = if pick == 0 { world_aabb(Vec3::new(-0.42, 0.0, -0.42), Vec3::new(0.42, 1.0, 0.36), &tf) } else { world_aabb(Vec3::new(-0.28, 0.0, -0.21), Vec3::new(0.28, 0.7, 0.21), &tf) };
                let e = b.commands.spawn((model(asset_server, name), tf, Collider(col))).id();
                b.tag(e);
            }
        }
    }
    // Archive: reading table with a green banker's lamp in the middle of large rooms.
    let center_free = !blocked.iter().any(|(lo, hi)| center.x + 1.2 > lo.x && center.x - 1.2 < hi.x && center.y + 0.8 > lo.y && center.y - 0.8 < hi.y);
    if r.area == Area::Archive && r.width() > 6.5 && r.kind != RoomKind::Hall && center_free {
        reading_table(b, props, Vec3::new(center.x, 0.0, center.y), r.light != LightState::Dark);
    }
    // Pictures on the side walls in the Guest Wing.
    if r.area == Area::GuestWing && !narrow {
        let z = r.max.y - r.depth() * rng.range_f32(0.3, 0.7);
        let name = if rng.next_f32() < 0.5 { "hanging_picture_frame_01" } else { "hanging_picture_frame_02" };
        let e = b
            .commands
            .spawn((model(asset_server, name), Transform::from_xyz(r.min.x + 0.02, 1.8, z).with_rotation(Quat::from_rotation_y(FRAC_PI_2))))
            .id();
        b.tag(e);
    }
    b.member = None;
}

/// Hiding spots a room that needs them gets: 1 (2 in long rooms), and at
/// least one for every two players.
pub fn hiding_spots_needed(long: bool, players: u8) -> usize {
    (if long { 2 } else { 1 }).max((players as usize).div_ceil(crate::interaction::SPOT_CAPACITY))
}

/// A room's south side is the previous room's north wall. Where this room is
/// wider or taller than that wall, fill the gap so you never see out into the
/// void when you look back.
pub fn close_south_side(b: &mut Builder, r: &RoomPlan, prev: Option<&RoomPlan>, props: &PropMaterials) {
    let Some(p) = prev else { return };
    b.member = Some(r.index);
    let style = style_for(r.area, b, props);
    let (z0, z1) = (r.max.y, r.max.y + WALL_T);
    let h = r.height;
    if !r.area.outdoors() || !p.area.outdoors() {
        for (x0, x1) in [(r.min.x - WALL_T, p.min.x - WALL_T), (p.max.x + WALL_T, r.max.x + WALL_T)] {
            if x1 - x0 > 0.01 {
                b.block(Vec3::new(x0, 0.0, z0), Vec3::new(x1, h, z1), &style.wall, style.wall_tile, true);
            }
        }
        // Above the previous room's wall, if this room is taller.
        let (ox0, ox1) = (r.min.x.max(p.min.x) - WALL_T, r.max.x.min(p.max.x) + WALL_T);
        if h > p.height + 0.01 && ox1 > ox0 {
            b.block(Vec3::new(ox0, p.height, z0), Vec3::new(ox1, h, z1), &style.wall, style.wall_tile, false);
        }
    } else {
        // Both outdoors: a garden wall is enough (open sky above).
        for (x0, x1) in [(r.min.x - WALL_T, p.min.x - WALL_T), (p.max.x + WALL_T, r.max.x + WALL_T)] {
            if x1 - x0 > 0.01 {
                b.block(Vec3::new(x0, 0.0, z0), Vec3::new(x1, 3.2, z1), &style.wall, style.wall_tile, true);
            }
        }
    }
    b.member = None;
}

/// Marker on the temporary back wall of the oldest loaded room (see [`back_cap`]).
#[derive(Component)]
pub struct BackCap(pub u32);

/// A room has no south wall of its own: it shares the previous room's north
/// wall. Once the previous room is unloaded (it's two rooms behind you), that
/// wall disappears and looking back through the doors would show the void.
/// This fills the shared stretch with a wall and a closed door until the
/// previous room is loaded again.
pub fn back_cap(b: &mut Builder, r: &RoomPlan, prev: &RoomPlan, props: &PropMaterials, images: &mut Assets<Image>, std_mats: &mut Assets<StandardMaterial>) {
    b.member = Some(r.index);
    let style = style_for(prev.area, b, props);
    let (z0, z1) = (r.max.y, r.max.y + WALL_T);
    let (x0, x1) = (r.min.x.max(prev.min.x) - WALL_T, r.max.x.min(prev.max.x) + WALL_T);
    let top = r.height.min(prev.height);
    let eh = r.entry_height;
    let (dx0, dx1) = (r.entry_x - DOOR_W / 2.0, r.entry_x + DOOR_W / 2.0);
    let mut ids = vec![];
    for (a, c, y0, y1) in [(x0, dx0, 0.0, top), (dx1, x1, 0.0, top), (dx0, dx1, eh + DOOR_H, top), (dx0, dx1, 0.0, eh)] {
        if c - a > 0.005 && y1 - y0 > 0.005 {
            ids.push(b.block(Vec3::new(a, y0, z0), Vec3::new(c, y1, z1), &style.wall, style.wall_tile, false));
        }
    }
    // The door you came through, shut, seen from this room.
    let door_tf = Transform::from_xyz(r.entry_x, eh, z0).with_rotation(Quat::from_rotation_y(PI));
    let assembly = b.commands.spawn((door_tf, Visibility::default())).id();
    b.tag(assembly);
    b.parent = Some(assembly);
    b.door(0.0, 0.0, r.index, true, images, std_mats);
    b.parent = None;
    ids.push(assembly);
    for id in ids {
        b.commands.entity(id).insert(BackCap(r.index));
    }
    b.member = None;
}

/// Marker: this door was already open when its room was (re)built.
#[derive(Component)]
pub struct OpenOnSpawn;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_player_can_hide() {
        for players in 1..=4u8 {
            for long in [false, true] {
                assert!(super::hiding_spots_needed(long, players) * crate::interaction::SPOT_CAPACITY >= players as usize);
            }
        }
    }

    #[test]
    fn furniture_never_stands_in_a_door_swing() {
        for seed in 0..200 {
            let plan = super::super::plan::generate(seed);
            for r in &plan.rooms {
                let doors = door_zones(r);
                let mut rng = GameRng::new(r.seed);
                for s in side_slots(r, &mut rng).iter().filter(|s| slot_free(s, &doors)) {
                    let f = slot_footprint(s);
                    assert!(!doors.iter().any(|d| zones_overlap(&f, d)), "seed {seed} room {}", r.index);
                }
                // Divider openings stay clear of furniture in every area.
                let passages = passage_zones(r);
                for s in side_slots(r, &mut GameRng::new(r.seed)).iter().filter(|s| slot_free(s, &passages)) {
                    assert!(!passages.iter().any(|d| zones_overlap(&slot_footprint(s), d)));
                }
                assert!(passages.iter().all(|p| stair_zones(r).contains(p)), "columns and rocks must avoid divider openings");
                // Every door keeps its swing clear of columns and rocks too.
                let keep = stair_zones(r);
                assert!(doors.iter().all(|d| keep.contains(d)));
            }
        }
    }

    #[test]
    fn atrium_columns_leave_a_wide_path() {
        for seed in 0..40 {
            for r in super::super::plan::generate(seed).rooms.into_iter().filter(|r| r.area == Area::Atrium) {
                let inset = atrium_column_inset(&r);
                // Columns are 0.6 wide (plus a 0.8 base): the middle must stay open.
                let gap = (r.max.x - inset - 0.4) - (r.min.x + inset + 0.4);
                assert!(gap >= 1.4, "room {} width {} leaves {gap}", r.index, r.width());
            }
        }
    }

    #[test]
    fn atrium_columns_never_stand_on_stairs() {
        // Same placement rule as atrium_architecture, checked against every
        // stair, landing and loft on many floors.
        for seed in 0..40 {
            for r in super::super::plan::generate(seed).rooms {
                let zones = stair_zones(&r);
                if zones.is_empty() {
                    continue;
                }
                let mut z = r.max.y - 1.5;
                while z > r.min.y + 1.0 {
                    for x in [r.min.x + atrium_column_inset(&r), r.max.x - atrium_column_inset(&r)] {
                        let blocked = zones.iter().any(|(lo, hi)| x + 0.45 > lo.x && x - 0.45 < hi.x && z + 0.45 > lo.y && z - 0.45 < hi.y);
                        if let Some(Feature::RaisedExit { height }) = r.feature {
                            // The stairs run from the landing south; a column on them must be skipped.
                            let foot = r.min.y + LANDING_DEPTH + height / super::super::plan::STEP_RISE * STEP_RUN;
                            if (x - r.exit_x).abs() < 0.6 + 0.3 && z < foot + 0.3 {
                                assert!(blocked, "room {} column at {x},{z} on the stairs", r.index);
                            }
                        }
                    }
                    z -= 3.2;
                }
            }
        }
    }
}

/// Marks the logical entity of a fake door with its room index and slot.
#[derive(Component)]
pub struct MimicOf(pub u32, pub u8);

/// (Lumens, item) for a drawer, deterministic per room seed.
pub fn drawer_loot(r: &RoomPlan, id: u8, items: &crate::data::ItemConfig) -> (Option<u32>, Option<crate::items::ItemKind>) {
    let mut rng = GameRng::fork(r.seed, 500 + u64::from(id));
    let roll = rng.next_f32();
    let pick = rng.next_f32();
    let amount = 1 + rng.below(6);
    if roll < items.item_chance {
        (None, Some(crate::items::ItemKind::roll(items, pick)))
    } else if roll < items.item_chance + 0.45 {
        (Some(amount), None)
    } else {
        (None, None)
    }
}

fn logical_drawer(b: &mut Builder, r: &RoomPlan, rt: &FloorRuntime, id: u8, aabb: Aabb) -> Entity {
    let searched = rt.searched.contains(&(r.index, id));
    let (loot, item) = if searched { (None, None) } else { drawer_loot(r, id, &rt.items) };
    let e = b
        .commands
        .spawn((
            Drawer { open: false, t: 0.0, loot, searched, has_key: false, tag: Some((r.index, id)), item },
            Interactable { kind: InteractKind::Drawer, aabb },
        ))
        .id();
    b.tag(e);
    e
}

/// A solid block you can stand on.
fn landing(b: &mut Builder, min: Vec3, max: Vec3, mat: &Handle<StandardMaterial>, trim: bool) {
    let e = b.block(min, max, mat, 1.0, true);
    b.commands.entity(e).insert(Ground(Aabb { min, max }));
    if trim {
        // Darker nosing along the top edges.
        let iron = b.mats.iron.clone();
        b.block(Vec3::new(min.x, max.y - 0.03, max.z - 0.04), Vec3::new(max.x, max.y + 0.005, max.z + 0.005), &iron, 1.0, false);
    }
}

/// Invisible walkable strip (doorway thresholds between rooms).
fn ground_only(b: &mut Builder, min: Vec3, max: Vec3) {
    let e = b.commands.spawn((Transform::default(), Ground(Aabb { min, max }))).id();
    b.tag(e);
}

/// A straight flight of steps climbing to `height` at `z_top`; the flight
/// extends from `z_top` in direction `dir` (+1 = south / +Z). Returns the far z.
fn stairs(b: &mut Builder, x0: f32, x1: f32, z_top: f32, dir: f32, height: f32, mat: &Handle<StandardMaterial>) -> f32 {
    let n = steps_for(height);
    let rise = height / n as f32;
    let iron = b.mats.iron.clone();
    for k in 1..n {
        let top = height - k as f32 * rise;
        let za = z_top + dir * (k - 1) as f32 * STEP_RUN;
        let zb = z_top + dir * k as f32 * STEP_RUN;
        let (lo, hi) = (Vec3::new(x0, 0.0, za.min(zb)), Vec3::new(x1, top, za.max(zb)));
        let e = b.block(lo, hi, mat, 1.0, true);
        b.commands.entity(e).insert(Ground(Aabb { min: lo, max: hi }));
        // Nosing strip on the leading edge.
        let edge = za.max(zb) * (dir + 1.0) / 2.0 + za.min(zb) * (1.0 - dir) / 2.0;
        b.block(Vec3::new(x0, top - 0.025, edge - 0.03), Vec3::new(x1, top + 0.004, edge + 0.03), &iron, 1.0, false);
    }
    z_top + dir * (n - 1) as f32 * STEP_RUN
}

/// Iron handrail with posts between two points at floor height `a.y`.
fn railing(b: &mut Builder, a: Vec3, c: Vec3) {
    let iron = b.mats.iron.clone();
    let len = (c - a).length();
    if len < 0.05 {
        return;
    }
    let posts = (len / 0.9).ceil() as usize;
    for i in 0..=posts {
        let p = a.lerp(c, i as f32 / posts as f32);
        b.block(Vec3::new(p.x - 0.025, a.y, p.z - 0.025), Vec3::new(p.x + 0.025, a.y + 0.95, p.z + 0.025), &iron, 1.0, false);
    }
    let (lo, hi) = (a.min(c), a.max(c));
    b.block(Vec3::new(lo.x - 0.035, a.y + 0.92, lo.z - 0.035), Vec3::new(hi.x + 0.035, a.y + 0.98, hi.z + 0.035), &iron, 1.0, false);
    b.block(Vec3::new(lo.x - 0.015, a.y + 0.45, lo.z - 0.015), Vec3::new(hi.x + 0.015, a.y + 0.48, hi.z + 0.015), &iron, 1.0, false);
}

fn slot_tf(s: Slot, depth_half: f32) -> Transform {
    Transform::from_xyz(s.x + s.inward * (depth_half + 0.02), s.y, s.z).with_rotation(Quat::from_rotation_y(s.yaw))
}

fn drawer_cabinet(b: &mut Builder, asset_server: &AssetServer, s: Slot, r: &RoomPlan, rt: &FloorRuntime, next: &mut u8) -> Vec<Entity> {
    let tf = slot_tf(s, 0.25);
    let mut bindings = vec![];
    let mut out = vec![];
    let names = ["drawer_cabinet_drawer_01", "drawer_cabinet_drawer_02", "drawer_cabinet_drawer_03", "drawer_cabinet_drawer_04"];
    for (i, (y0, y1)) in [(0.954, 1.103), (0.779, 0.928), (0.604, 0.753), (0.43, 0.578)].into_iter().enumerate() {
        let aabb = world_aabb(Vec3::new(-0.52, y0, 0.15), Vec3::new(0.52, y1, 0.56), &tf);
        let owner = logical_drawer(b, r, rt, *next, aabb);
        *next += 1;
        out.push(owner);
        bindings.push(PartBinding { node: names[i], owner, motion: PartMotion::Slide(Vec3::Z * 0.32), reset_rotation: false });
    }
    let e = b
        .commands
        .spawn((model(asset_server, "drawer_cabinet"), tf, ModelBindings(bindings), Collider(world_aabb(Vec3::new(-0.571, 0.0, -0.248), Vec3::new(0.571, 1.881, 0.239), &tf))))
        .id();
    b.tag(e);
    out
}

fn commode(b: &mut Builder, asset_server: &AssetServer, s: Slot, r: &RoomPlan, rt: &FloorRuntime, next: &mut u8) -> Vec<Entity> {
    let tf = slot_tf(s, 0.29);
    let mut bindings = vec![];
    let mut out = vec![];
    for (node, ty, tz) in [("GothicCommode_01_door01", 0.987, 0.271), ("GothicCommode_01_door02", 0.667, 0.261), ("GothicCommode_01_door03", 0.356, 0.281)] {
        let aabb = world_aabb(Vec3::new(-0.5, ty - 0.125, tz - 0.08), Vec3::new(0.5, ty + 0.125, tz + 0.36), &tf);
        let owner = logical_drawer(b, r, rt, *next, aabb);
        *next += 1;
        out.push(owner);
        bindings.push(PartBinding { node, owner, motion: PartMotion::Slide(Vec3::Z * 0.3), reset_rotation: false });
    }
    let e = b
        .commands
        .spawn((model(asset_server, "GothicCommode_01"), tf, ModelBindings(bindings), Collider(world_aabb(Vec3::new(-0.6, 0.0, -0.29), Vec3::new(0.6, 1.21, 0.29), &tf))))
        .id();
    b.tag(e);
    out
}

/// Primitive steel tool chest: body, kick plate, three drawers with pulls.
fn tool_chest(b: &mut Builder, s: Slot, r: &RoomPlan, rt: &FloorRuntime, next: &mut u8, variant: u8) -> Vec<Entity> {
    let tf = slot_tf(s, 0.28);
    let metal = if variant % 2 == 0 { b.mats.rusty_metal.clone() } else { b.mats.metal_plate.clone() };
    let iron = b.mats.iron.clone();
    let root = b.commands.spawn((tf, Visibility::default(), Collider(world_aabb(Vec3::new(-0.45, 0.0, -0.28), Vec3::new(0.45, 1.05, 0.28), &tf)))).id();
    b.tag(root);
    let body = b.mesh(Vec3::new(0.9, 0.95, 0.56), 0.8);
    let kick = b.mesh(Vec3::new(0.86, 0.08, 0.5), 0.8);
    let top = b.mesh(Vec3::new(0.94, 0.04, 0.6), 0.8);
    b.commands.entity(root).with_children(|c| {
        c.spawn((Mesh3d(body), MeshMaterial3d(metal.clone()), Transform::from_xyz(0.0, 0.555, -0.01)));
        c.spawn((Mesh3d(kick), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 0.04, 0.0)));
        c.spawn((Mesh3d(top), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 1.05, 0.0)));
    });
    let front = b.mesh(Vec3::new(0.84, 0.27, 0.03), 0.6);
    let box_mesh = b.mesh(Vec3::new(0.8, 0.22, 0.48), 0.6);
    let pull = b.mesh(Vec3::new(0.5, 0.025, 0.03), 1.0);
    let mut out = vec![];
    for i in 0..3 {
        let y = 0.25 + i as f32 * 0.3;
        let aabb = world_aabb(Vec3::new(-0.42, y - 0.14, 0.2), Vec3::new(0.42, y + 0.14, 0.62), &tf);
        let owner = logical_drawer(b, r, rt, *next, aabb);
        *next += 1;
        out.push(owner);
        let base = Transform::from_xyz(0.0, y, 0.275);
        b.commands.entity(root).with_children(|c| {
            c.spawn((base, Visibility::default(), AnimatedPart { owner, base, motion: PartMotion::Slide(Vec3::Z * 0.32) })).with_children(|d| {
                d.spawn((Mesh3d(front.clone()), MeshMaterial3d(metal.clone()), Transform::default()));
                d.spawn((Mesh3d(box_mesh.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, -0.01, -0.25)));
                d.spawn((Mesh3d(pull.clone()), MeshMaterial3d(b.mats.brass.clone()), Transform::from_xyz(0.0, 0.06, 0.025)));
            });
        });
    }
    out
}

/// Hollow steel locker (hiding spot) with a hinged, vented door.
fn locker(b: &mut Builder, s: Slot) {
    let tf = slot_tf(s, 0.31);
    let metal = b.mats.metal_plate.clone();
    let iron = b.mats.iron.clone();
    let dark = b.mats.void.clone();
    let (w, h, d) = (0.9f32, 2.05f32, 0.62f32);
    let front = tf.rotation * Vec3::Z;
    let spot = b
        .commands
        .spawn((
            HidingSpot {
                inside: tf.transform_point(Vec3::new(0.0, 0.0, -0.02)),
                inside_yaw: yaw_facing(front),
                exit: tf.transform_point(Vec3::new(0.0, 0.0, 0.95)),
                exit_yaw: yaw_facing(front),
                doors_target: 0.0,
                doors_t: 0.0,
            },
            Interactable { kind: InteractKind::HidingSpot, aabb: world_aabb(Vec3::new(-w / 2.0, 0.0, d / 2.0 - 0.1), Vec3::new(w / 2.0, h, d / 2.0 + 0.25), &tf) },
        ))
        .id();
    b.tag(spot);
    let root = b.commands.spawn((tf, Visibility::default(), Collider(world_aabb(Vec3::new(-w / 2.0, 0.0, -d / 2.0), Vec3::new(w / 2.0, h, d / 2.0), &tf)))).id();
    b.tag(root);
    let t = 0.025;
    let side = b.mesh(Vec3::new(t, h, d), 0.8);
    let back = b.mesh(Vec3::new(w, h, t), 0.8);
    let cap = b.mesh(Vec3::new(w, t, d), 0.8);
    let leaf = b.mesh(Vec3::new(w - 0.02, h - 0.06, 0.02), 0.8);
    let slit = b.mesh(Vec3::new(0.5, 0.012, 0.004), 1.0);
    let handle = b.mesh(Vec3::new(0.02, 0.14, 0.03), 1.0);
    let base = Transform::from_xyz(-w / 2.0 + 0.01, 0.03, d / 2.0);
    b.commands.entity(root).with_children(|c| {
        c.spawn((Mesh3d(side.clone()), MeshMaterial3d(metal.clone()), Transform::from_xyz(-w / 2.0 + t / 2.0, h / 2.0, 0.0)));
        c.spawn((Mesh3d(side), MeshMaterial3d(metal.clone()), Transform::from_xyz(w / 2.0 - t / 2.0, h / 2.0, 0.0)));
        c.spawn((Mesh3d(back), MeshMaterial3d(metal.clone()), Transform::from_xyz(0.0, h / 2.0, -d / 2.0 + t / 2.0)));
        c.spawn((Mesh3d(cap.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, h - t / 2.0, 0.0)));
        c.spawn((Mesh3d(cap), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, t / 2.0, 0.0)));
        c.spawn((base, Visibility::default(), AnimatedPart { owner: spot, base, motion: PartMotion::Hinge(-1.7) })).with_children(|hinge| {
            let lx = (w - 0.02) / 2.0;
            hinge.spawn((Mesh3d(leaf), MeshMaterial3d(metal.clone()), Transform::from_xyz(lx, (h - 0.06) / 2.0, 0.0)));
            for k in 0..6 {
                hinge.spawn((Mesh3d(slit.clone()), MeshMaterial3d(dark.clone()), Transform::from_xyz(lx, h - 0.35 - k as f32 * 0.04, 0.012)));
            }
            hinge.spawn((Mesh3d(handle), MeshMaterial3d(iron.clone()), Transform::from_xyz(w - 0.12, 1.0, 0.025)));
        });
    });
}

fn wardrobe(b: &mut Builder, asset_server: &AssetServer, s: Slot) {
    let tf = Transform::from_xyz(s.x + s.inward * 0.48, 0.0, s.z).with_rotation(Quat::from_rotation_y(s.yaw));
    let front = tf.rotation * Vec3::Z;
    let spot = b
        .commands
        .spawn((
            HidingSpot {
                inside: tf.transform_point(Vec3::new(0.0, 0.0, 0.08)),
                inside_yaw: yaw_facing(front),
                exit: tf.transform_point(Vec3::new(0.0, 0.0, 1.15)),
                exit_yaw: yaw_facing(front),
                doors_target: 0.0,
                doors_t: 0.0,
            },
            Interactable { kind: InteractKind::HidingSpot, aabb: world_aabb(Vec3::new(-0.86, 0.0, 0.3), Vec3::new(0.86, 2.3, 0.62), &tf) },
        ))
        .id();
    b.tag(spot);
    let hinge = |node: &'static str, angle: f32| PartBinding { node, owner: spot, motion: PartMotion::Hinge(angle), reset_rotation: true };
    let e = b
        .commands
        .spawn((
            model(asset_server, "GothicCabinet_01"),
            tf,
            ModelBindings(vec![hinge("GothicCabinet_01_door1", -1.7), hinge("GothicCabinet_01_door3", -1.7), hinge("GothicCabinet_01_door2", 1.7), hinge("GothicCabinet_01_door4", 1.7)]),
            Collider(world_aabb(Vec3::new(-0.86, 0.0, -0.457), Vec3::new(0.86, 2.36, 0.56), &tf)),
        ))
        .id();
    b.tag(e);
}

/// Tall bookcase filled with individually sized, coloured books.
fn bookshelf(b: &mut Builder, props: &PropMaterials, s: Slot, rng: &mut GameRng) {
    let tf = slot_tf(s, 0.2);
    let wood = b.mats.trim_wood.clone();
    let (w, h, d) = (1.7f32, 2.6f32, 0.38f32);
    let root = b.commands.spawn((tf, Visibility::default(), Collider(world_aabb(Vec3::new(-w / 2.0, 0.0, -d / 2.0), Vec3::new(w / 2.0, h, d / 2.0), &tf)))).id();
    b.tag(root);
    let side = b.mesh(Vec3::new(0.05, h, d), 0.8);
    let board = b.mesh(Vec3::new(w, 0.035, d), 0.8);
    let back = b.mesh(Vec3::new(w, h, 0.02), 0.8);
    let crown = b.mesh(Vec3::new(w + 0.1, 0.08, d + 0.05), 0.8);
    let cube = props.unit_cube.clone();
    let shelves = 6;
    let mut books = vec![];
    for k in 0..shelves {
        let y0 = 0.1 + k as f32 * (h - 0.2) / shelves as f32;
        let mut x = -w / 2.0 + 0.06;
        while x < w / 2.0 - 0.1 {
            if rng.next_f32() < 0.06 {
                x += rng.range_f32(0.08, 0.2); // gap
                continue;
            }
            let bw = rng.range_f32(0.025, 0.065);
            let bh = rng.range_f32(0.2, (h - 0.2) / shelves as f32 - 0.06);
            let bd = rng.range_f32(0.2, 0.3);
            let lean = if rng.next_f32() < 0.05 { rng.range_f32(-0.25, 0.25) } else { 0.0 };
            let mat = props.books[rng.below(props.books.len() as u32) as usize].clone();
            books.push((Transform::from_xyz(x + bw / 2.0, y0 + 0.018 + bh / 2.0, d / 2.0 - bd / 2.0 - 0.03).with_rotation(Quat::from_rotation_z(lean)).with_scale(Vec3::new(bw, bh, bd)), mat));
            x += bw + 0.003;
        }
    }
    b.commands.entity(root).with_children(|c| {
        c.spawn((Mesh3d(side.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(-w / 2.0, h / 2.0, 0.0)));
        c.spawn((Mesh3d(side), MeshMaterial3d(wood.clone()), Transform::from_xyz(w / 2.0, h / 2.0, 0.0)));
        c.spawn((Mesh3d(back), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, h / 2.0, -d / 2.0 + 0.01)));
        c.spawn((Mesh3d(crown), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, h + 0.04, 0.0)));
        for k in 0..=shelves {
            let y = 0.1 + k as f32 * (h - 0.2) / shelves as f32;
            c.spawn((Mesh3d(board.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, y, 0.0)));
        }
        for (t, m) in books {
            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(m), t));
        }
    });
}

/// A double-sided row of tall shelves across the room (Reading Hall).
fn shelf_row(b: &mut Builder, props: &PropMaterials, x0: f32, x1: f32, z: f32, rng: &mut GameRng) {
    let n = ((x1 - x0) / 1.7).floor().max(1.0) as usize;
    let used = n as f32 * 1.7;
    let start = x0 + ((x1 - x0) - used) / 2.0 + 0.85;
    for i in 0..n {
        let x = start + i as f32 * 1.7;
        // Back to back: one faces south, one faces north.
        bookshelf(b, props, Slot { x, z: z + 0.22, yaw: 0.0, inward: 0.0, y: 0.0 }, rng);
        bookshelf(b, props, Slot { x, z: z - 0.22, yaw: PI, inward: 0.0, y: 0.0 }, rng);
    }
    // Fill any sliver at the ends so nobody slips through between shelves and wall.
    let wood = b.mats.trim_wood.clone();
    if start - 0.85 - x0 > 0.05 {
        b.block(Vec3::new(x0, 0.0, z - 0.45), Vec3::new(start - 0.85, 2.6, z + 0.45), &wood, 1.0, true);
    }
    let end = start - 0.85 + used;
    if x1 - end > 0.05 {
        b.block(Vec3::new(end, 0.0, z - 0.45), Vec3::new(x1, 2.6, z + 0.45), &wood, 1.0, true);
    }
}

/// Chase obstacles: a line of crates and toppled planks you have to run around.
fn barricade(b: &mut Builder, x0: f32, x1: f32, z: f32, rng: &mut GameRng) {
    let mut x = x0 + 0.05;
    while x < x1 - 0.3 {
        let size = rng.range_f32(0.75, 0.95).min(x1 - x - 0.02);
        if size < 0.3 {
            break;
        }
        let c = Vec3::new(x + size / 2.0, size / 2.0, z + rng.range_f32(-0.12, 0.12));
        crate_box(b, c, size, rng.range_f32(-0.15, 0.15), true);
        // Some stacks are two high.
        if rng.next_f32() < 0.4 {
            let s2 = size * 0.8;
            crate_box(b, Vec3::new(c.x, size + s2 / 2.0, c.z), s2, rng.range_f32(-0.3, 0.3), false);
        }
        x += size + 0.02;
    }
    // A solid band behind the crates so there are no gaps to squeeze through.
    let planks = b.mats.door_wood.clone();
    b.block(Vec3::new(x0, 0.0, z - 0.08), Vec3::new(x1, 1.1, z + 0.08), &planks, 1.0, true);
}

fn reading_table(b: &mut Builder, props: &PropMaterials, at: Vec3, lit: bool) {
    let wood = b.mats.trim_wood.clone();
    let root = b.commands.spawn((Transform::from_translation(at), Visibility::default(), Collider(Aabb { min: at + Vec3::new(-0.9, 0.0, -0.5), max: at + Vec3::new(0.9, 0.8, 0.5) }))).id();
    b.tag(root);
    let top = b.mesh(Vec3::new(1.8, 0.05, 1.0), 0.9);
    let leg = b.mesh(Vec3::new(0.07, 0.75, 0.07), 0.5);
    let apron = b.mesh(Vec3::new(1.65, 0.1, 0.03), 0.5);
    let cube = props.unit_cube.clone();
    let cyl = props.unit_cyl.clone();
    let brass = b.mats.brass.clone();
    b.commands.entity(root).with_children(|c| {
        c.spawn((Mesh3d(top), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, 0.775, 0.0)));
        for (x, z) in [(-0.82, -0.42), (0.82, -0.42), (-0.82, 0.42), (0.82, 0.42)] {
            c.spawn((Mesh3d(leg.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(x, 0.375, z)));
        }
        for z in [-0.45, 0.45] {
            c.spawn((Mesh3d(apron.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, 0.7, z)));
        }
        // Open book and loose papers.
        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(props.paper.clone()), Transform::from_xyz(-0.2, 0.81, 0.1).with_rotation(Quat::from_rotation_y(0.2)).with_scale(Vec3::new(0.42, 0.02, 0.3))));
        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(props.paper.clone()), Transform::from_xyz(0.35, 0.802, -0.2).with_rotation(Quat::from_rotation_y(-0.5)).with_scale(Vec3::new(0.21, 0.004, 0.3))));
        // Banker's lamp: brass base, stem, green glass shade.
        c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(0.55, 0.81, 0.15).with_scale(Vec3::new(0.16, 0.03, 0.16))));
        c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(0.55, 0.95, 0.15).with_scale(Vec3::new(0.02, 0.28, 0.02))));
        c.spawn((Mesh3d(cyl), MeshMaterial3d(props.green_glass.clone()), Transform::from_xyz(0.55, 1.1, 0.12).with_rotation(Quat::from_rotation_z(FRAC_PI_2)).with_scale(Vec3::new(0.1, 0.32, 0.1))));
    });
    if lit {
        let e = b.commands.spawn((PointLight { color: Color::srgb(0.85, 1.0, 0.8), intensity: 9_000.0, range: 4.0, ..default() }, Transform::from_translation(at + Vec3::new(0.55, 1.0, 0.12)))).id();
        b.tag(e);
    }
}

/// Outdoor architecture, an autumn night garden: brick walls with stone
/// pillars, a gravel path from door to door, red and orange trees, dark pines
/// beyond the walls, bramble patches, rock piles, a tiered fountain, and a
/// cool moonlit fill light.
fn grounds_architecture(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, r: &RoomPlan, taken: &[Zone], rng: &mut GameRng) -> Vec<Zone> {
    let h = r.height;
    let stone = props.stone.clone();
    // Capstones along the tops of the walls.
    for (lo, hi) in [
        (Vec3::new(r.min.x - WALL_T - 0.06, h, r.min.y - WALL_T - 0.06), Vec3::new(r.max.x + WALL_T + 0.06, h + 0.16, r.min.y + 0.06)),
        (Vec3::new(r.min.x - WALL_T - 0.06, h, r.max.y - 0.06), Vec3::new(r.max.x + WALL_T + 0.06, h + 0.16, r.max.y + WALL_T + 0.06)),
        (Vec3::new(r.min.x - WALL_T - 0.06, h, r.min.y), Vec3::new(r.min.x + 0.06, h + 0.16, r.max.y)),
        (Vec3::new(r.max.x - 0.06, h, r.min.y), Vec3::new(r.max.x + WALL_T + 0.06, h + 0.16, r.max.y)),
    ] {
        b.block(lo, hi, &stone, 1.0, false);
    }
    // Stone pillars along the side walls, with pyramid caps.
    let mut z = r.max.y - 0.2;
    while z > r.min.y + 0.2 {
        for x in [r.min.x, r.max.x] {
            let inward = if x == r.min.x { 1.0 } else { -1.0 };
            let (x0, x1) = (x - inward * 0.05, x + inward * 0.32);
            b.block(Vec3::new(x0.min(x1), 0.0, z - 0.2), Vec3::new(x0.max(x1), h + 0.35, z + 0.2), &stone, 1.0, false);
            let cap = b.commands.spawn((Mesh3d(props.pyramid.clone()), MeshMaterial3d(stone.clone()), Transform::from_xyz((x0 + x1) / 2.0, h + 0.55, z).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)).with_scale(Vec3::new(0.6, 0.4, 0.6)))).id();
            b.tag(cap);
        }
        z -= 3.2;
    }
    // Pillars and lanterns flanking the exit door, like a garden gate.
    for dx in [-0.85f32, 0.85] {
        let x = r.exit_x + dx;
        b.block(Vec3::new(x - 0.2, 0.0, r.min.y), Vec3::new(x + 0.2, h + 0.4, r.min.y + 0.35), &stone, 1.0, true);
        let cap = b.commands.spawn((Mesh3d(props.pyramid.clone()), MeshMaterial3d(stone.clone()), Transform::from_xyz(x, h + 0.62, r.min.y + 0.17).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)).with_scale(Vec3::new(0.62, 0.45, 0.62)))).id();
        b.tag(cap);
    }
    // Gravel path: entry → middle → exit, with dirt edges.
    let gravel = props.gravel.clone();
    let dirt = b.mats.dirt.clone();
    let mid = (r.max.y + r.min.y) / 2.0;
    let (x0, x1) = (r.entry_x.min(r.exit_x), r.entry_x.max(r.exit_x));
    for (w, y, mat) in [(1.05f32, 0.006f32, &dirt), (0.8, 0.012, &gravel)] {
        b.block(Vec3::new(r.entry_x - w, 0.0, mid - w), Vec3::new(r.entry_x + w, y, r.max.y), mat, 1.2, false);
        b.block(Vec3::new(r.exit_x - w, 0.0, r.min.y), Vec3::new(r.exit_x + w, y + 0.001, mid + w), mat, 1.2, false);
        b.block(Vec3::new(x0 - w, 0.0, mid - w), Vec3::new(x1 + w, y - 0.001, mid + w), mat, 1.2, false);
    }
    // "On the path" also covers the sheds, windmills and benches already placed.
    let on_path = |x: f32, z: f32, m: f32| {
        ((x - r.entry_x).abs() < m && z > mid - m)
            || ((x - r.exit_x).abs() < m && z < mid + m)
            || ((z - mid).abs() < m && x > x0 - m && x < x1 + m)
            || taken.iter().any(|(lo, hi)| x + m * 0.6 > lo.x && x - m * 0.6 < hi.x && z + m * 0.6 > lo.y && z - m * 0.6 < hi.y)
    };
    // Everything below claims its footprint here and only goes where nothing
    // else stands: no tree crown through a wall, a lamp or another tree, no
    // boulder in a stump. Yaws are quarter turns so footprints stay exact.
    let mut occ: Vec<Zone> = taken.to_vec();
    occ.extend(door_zones(r));
    occ.extend(passage_zones(r));
    occ.extend(stair_zones(r));
    let quarter = |rng: &mut GameRng| rng.below(4) as f32 * FRAC_PI_2;
    let footprint = |at: Vec2, half: Vec2, yaw: f32| {
        let h = if (yaw / FRAC_PI_2).round() as i32 % 2 == 0 { half } else { Vec2::new(half.y, half.x) };
        (at - h, at + h)
    };
    let inside = |z: &Zone, margin: f32| z.0.x >= r.min.x + margin && z.1.x <= r.max.x - margin && z.0.y >= r.min.y + margin && z.1.y <= r.max.y - margin;
    let path_zone = |z: &Zone, m: f32| {
        let c = (z.0 + z.1) / 2.0;
        on_path(c.x, c.y, m + ((z.1 - z.0).max_element() / 2.0))
    };
    // Planting plan. Each garden has one main tree and one accent tree:
    // the main ones stand in an evenly spaced avenue along the side walls
    // (in from the wall, leaving room for hedges, sheds and chests), the
    // accent ones in a small cluster in one corner with a stump, a boulder
    // or a fallen trunk at their feet. Nothing is scattered at random.
    let pick = |rng: &mut GameRng| super::scenery::TREES[rng.below(super::scenery::TREES.len() as u32) as usize];
    let main = pick(rng);
    let mut accent = pick(rng);
    for _ in 0..4 {
        if accent.0 != main.0 {
            break;
        }
        accent = pick(rng);
    }
    let k = rng.range_f32(0.9, 1.05);
    let plant = |b: &mut Builder, occ: &mut Vec<Zone>, (name, hx, hz): (&str, f32, f32), at: Vec2, yaw: f32, k: f32| -> bool {
        let z = footprint(at, Vec2::new(hx, hz) * k, yaw);
        if !inside(&z, 0.15) || path_zone(&z, 0.5) || occ.iter().any(|o| zones_overlap(&z, o)) {
            return false;
        }
        // Solid out to most of the crown: some crowns hang below eye height.
        super::scenery::place(b, assets, name, Vec3::new(at.x, 0.0, at.y), yaw, Vec3::splat(k), Some(Vec3::new(hx * 0.6 * k, 3.0, hz * 0.6 * k)));
        occ.push(z);
        true
    };
    // The avenue (wide gardens only): same yaw for every tree in the row.
    let (hx, hz) = (main.1 * k, main.2 * k);
    if r.width() > 2.0 * (1.3 + 2.0 * hx) + 3.0 {
        let step = 2.0 * hz + 1.4;
        for (wall, inward) in [(r.min.x, 1.0f32), (r.max.x, -1.0)] {
            let x = wall + inward * (1.3 + hx + 0.1);
            let mut z = r.max.y - hz - 1.2;
            while z > r.min.y + hz + 1.2 {
                plant(b, &mut occ, main, Vec2::new(x, z), 0.0, k);
                z -= step;
            }
        }
    }
    // The corner cluster.
    // Inset from the corner by the accent tree's crown, so its first tree
    // fits snugly in the corner and the others step out toward the middle.
    let reach = accent.1.max(accent.2) + 0.3;
    let cx = if rng.next_f32() < 0.5 { r.min.x + reach } else { r.max.x - reach };
    let cz = if rng.next_f32() < 0.5 { r.min.y + reach } else { r.max.y - reach };
    let corner = Vec2::new(cx, cz);
    let toward = (r.center() - corner).normalize_or_zero();
    let mut cluster: Vec<Vec2> = vec![];
    for i in 0..12 {
        if cluster.len() >= 3 {
            break;
        }
        let a = rng.range_f32(-0.8, 0.8);
        let d = if i == 0 { 0.0 } else { reach * 1.6 + (i as f32 - 1.0) * 0.6 };
        let dir = Vec2::new(toward.x * a.cos() - toward.y * a.sin(), toward.x * a.sin() + toward.y * a.cos());
        let at = corner + dir * d;
        if plant(b, &mut occ, accent, at, 0.0, 0.95) {
            cluster.push(at);
        }
    }
    // At the cluster's feet: a stump, a boulder or a fallen trunk.
    if let Some(&base) = cluster.first() {
        let props_here: [(&str, Vec2, f32); 3] = [("stump", Vec2::new(0.75, 0.84), 0.6), ("boulder", Vec2::new(0.58, 0.82), 0.9), ("log", Vec2::new(1.55, 0.2), 0.3)];
        for _ in 0..12 {
            let (name, half, h) = props_here[rng.below(3) as usize];
            let kk = if name == "log" { 1.0 } else { rng.range_f32(0.55, 0.8) };
            let yaw = quarter(rng);
            let at = base + Vec2::new(rng.range_f32(-3.0, 3.0), rng.range_f32(-3.0, 3.0));
            let z = footprint(at, half * kk, yaw);
            if inside(&z, 0.3) && !path_zone(&z, 0.9) && !occ.iter().any(|o| zones_overlap(&z, o)) {
                super::scenery::place(b, assets, name, Vec3::new(at.x, 0.0, at.y), yaw, Vec3::splat(kk), Some(Vec3::new(half.x * kk, h * kk, half.y * kk)));
                occ.push(z);
                break;
            }
        }
    }
    // A weathered statue in the middle of larger gardens.
    let c = r.center();
    let sz = footprint(c, Vec2::splat(1.05), 0.0);
    if r.width() > 11.0 && !path_zone(&sz, 1.0) && !occ.iter().any(|o| zones_overlap(&sz, o)) {
        super::scenery::place(b, assets, "statue", Vec3::new(c.x, 0.0, c.y), quarter(rng), Vec3::splat(1.0), Some(Vec3::new(0.9, 2.2, 0.9)));
        occ.push(sz);
    }
    // Tall trees beyond the walls fill the skyline (spaced so none overlap).
    let mut outside: Vec<Zone> = vec![];
    for _ in 0..((r.depth() / 2.5) as usize).max(4) {
        let side = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
        let (name, hx, hz) = [super::scenery::TREES[10], super::scenery::TREES[11], super::scenery::TREES[12], super::scenery::TREES[13], super::scenery::TREES[6], super::scenery::TREES[8]][rng.below(6) as usize];
        let k = rng.range_f32(1.3, 1.9);
        let half = Vec2::new(hx, hz) * k;
        let x = if side < 0.0 { r.min.x - WALL_T - half.max_element() - rng.range_f32(0.5, 4.0) } else { r.max.x + WALL_T + half.max_element() + rng.range_f32(0.5, 4.0) };
        let at = Vec2::new(x, rng.range_f32(r.min.y, r.max.y));
        let z = footprint(at, half, 0.0);
        if outside.iter().any(|o| zones_overlap(&z, o)) {
            continue;
        }
        super::scenery::place(b, assets, name, Vec3::new(at.x, 0.0, at.y), 0.0, Vec3::splat(k), None);
        outside.push(z);
    }
    let arch = occ.split_off(taken.len());
    // Moonlight: a cool, soft fill from high above.
    let e = b
        .commands
        .spawn((PointLight { color: Color::srgb(0.62, 0.72, 1.0), intensity: 260_000.0, range: 34.0, radius: 2.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(c.x, h + 7.0, c.y)))
        .id();
    b.tag(e);
    arch
}





/// A street lamp (the light itself is added by the caller).
fn lamp_post(b: &mut Builder, assets: &AssetServer, at: Vec3) {
    super::scenery::place(b, assets, "street_lamp", at, 0.0, Vec3::splat(1.0), Some(Vec3::new(0.15, 3.4, 0.15)));
}

/// A clipped hedge against the wall (outdoor decor slot).
fn hedge(b: &mut Builder, assets: &AssetServer, s: Slot, rng: &mut GameRng) {
    // The shrub model is a 3.5 m row: squeezed to fit a 1.6 m slot against the wall.
    let len = rng.range_f32(1.4, 1.8);
    let tf = slot_tf(s, 0.45);
    let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
    super::scenery::place(b, assets, "hedge", tf.translation, yaw, Vec3::new(len / 3.5, rng.range_f32(0.85, 1.1), 1.0), Some(Vec3::new(1.75, 1.4, 0.45)));
}

fn caged_bulb(b: &mut Builder, props: &PropMaterials, ceiling: Vec3, drop: f32) {
    let root = b.commands.spawn((Transform::from_translation(ceiling - Vec3::Y * drop), Visibility::default())).id();
    b.tag(root);
    let iron = b.mats.iron.clone();
    let cyl = props.unit_cyl.clone();
    b.commands.entity(root).with_children(|c| {
        c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, drop / 2.0, 0.0).with_scale(Vec3::new(0.012, drop, 0.012))));
        c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.09, 0.05, 0.09))));
        c.spawn((Mesh3d(props.sphere.clone()), MeshMaterial3d(props.lamp_glow.clone()), Transform::from_xyz(0.0, -0.1, 0.0).with_scale(Vec3::splat(0.09))));
        for k in 0..4 {
            let a = k as f32 * FRAC_PI_2;
            c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(a.sin() * 0.07, -0.1, a.cos() * 0.07).with_scale(Vec3::new(0.008, 0.2, 0.008))));
        }
        c.spawn((Mesh3d(cyl), MeshMaterial3d(iron), Transform::from_xyz(0.0, -0.2, 0.0).with_scale(Vec3::new(0.15, 0.008, 0.15))));
    });
}

/// Timber support frames, mine-cart rails and lanterns.
fn quarry_architecture(b: &mut Builder, props: &PropMaterials, r: &RoomPlan) {
    let timber = b.mats.door_wood.clone();
    let iron = b.mats.iron.clone();
    let h = r.height;
    let mut z = r.max.y - 1.2;
    while z > r.min.y + 0.8 {
        for x in [r.min.x + 0.12, r.max.x - 0.12] {
            let post = (Vec2::new(x - 0.12, z - 0.12), Vec2::new(x + 0.12, z + 0.12));
            if door_zones(r).iter().chain(passage_zones(r).iter()).any(|d| zones_overlap(&post, d)) {
                continue;
            }
            b.block(Vec3::new(x - 0.12, 0.0, z - 0.12), Vec3::new(x + 0.12, h, z + 0.12), &timber, 1.0, true);
        }
        b.block(Vec3::new(r.min.x, h - 0.3, z - 0.14), Vec3::new(r.max.x, h - 0.04, z + 0.14), &timber, 1.0, false);
        // Diagonal braces.
        for (x, s) in [(r.min.x + 0.45, 1.0f32), (r.max.x - 0.45, -1.0)] {
            let e = b
                .commands
                .spawn((Mesh3d(props.unit_cube.clone()), MeshMaterial3d(timber.clone()), Transform::from_xyz(x, h - 0.55, z).with_rotation(Quat::from_rotation_z(s * 0.8)).with_scale(Vec3::new(0.1, 0.8, 0.1))))
                .id();
            b.tag(e);
        }
        z -= 2.6;
    }
    // Rails from entry to exit with sleepers.
    let x = (r.entry_x + r.exit_x) / 2.0;
    let (z0, z1) = (r.min.y + 0.2, r.max.y - 0.1);
    for dx in [-0.36f32, 0.36] {
        b.block(Vec3::new(x + dx - 0.025, 0.0, z0), Vec3::new(x + dx + 0.025, 0.07, z1), &iron, 1.0, false);
    }
    let mut zz = z1 - 0.2;
    while zz > z0 {
        b.block(Vec3::new(x - 0.6, 0.0, zz - 0.09), Vec3::new(x + 0.6, 0.04, zz + 0.09), &timber, 0.6, false);
        zz -= 0.6;
    }
}

/// Ceiling pipes on brackets plus a hazard-striped floor edge.
fn tunnel_architecture(b: &mut Builder, props: &PropMaterials, r: &RoomPlan) {
    let rust = b.mats.rusty_metal.clone();
    let iron = b.mats.iron.clone();
    let h = r.height;
    let len = r.depth();
    let zc = (r.min.y + r.max.y) / 2.0;
    for (x, rad) in [(r.min.x + 0.25, 0.09f32), (r.min.x + 0.5, 0.05), (r.max.x - 0.3, 0.12)] {
        let e = b
            .commands
            .spawn((Mesh3d(props.unit_cyl.clone()), MeshMaterial3d(rust.clone()), Transform::from_xyz(x, h - 0.25, zc).with_rotation(Quat::from_rotation_x(FRAC_PI_2)).with_scale(Vec3::new(rad * 2.0, len, rad * 2.0))))
            .id();
        b.tag(e);
        let mut z = r.max.y - 0.5;
        while z > r.min.y {
            b.block(Vec3::new(x - 0.02, h - 0.25, z - 0.03), Vec3::new(x + 0.02, h, z + 0.03), &iron, 1.0, false);
            z -= 1.5;
        }
    }
    for x in [r.min.x, r.max.x - 0.12] {
        b.block(Vec3::new(x, 0.0, r.min.y), Vec3::new(x + 0.12, 0.015, r.max.y), &props.hazard, 1.0, false);
    }
    let _ = PI;
}

/// A stream across a garden: dark water between stone banks too high to step
/// over, crossed by a railed wooden footbridge on the path.
fn garden_stream(b: &mut Builder, props: &PropMaterials, r: &RoomPlan, z: f32, bx: f32) {
    let stone = props.stone.clone();
    let wood = b.mats.door_wood.clone();
    let rail = b.mats.trim_wood.clone();
    let (z0, z1) = (z - STREAM_HALF, z + STREAM_HALF);
    // Water, a touch below the grass, and the stream bed.
    b.block(Vec3::new(r.min.x, -0.06, z0), Vec3::new(r.max.x, 0.012, z1), &props.water, 1.0, false);
    // Banks on both sides, open only where the bridge lands.
    for zb in [z0 - 0.1, z1 + 0.1] {
        for (x0, x1) in [(r.min.x, bx - BRIDGE_HALF - 0.05), (bx + BRIDGE_HALF + 0.05, r.max.x)] {
            if x1 - x0 > 0.05 {
                b.block(Vec3::new(x0, 0.0, zb - 0.1), Vec3::new(x1, 0.62, zb + 0.1), &stone, 1.0, true);
            }
        }
    }
    // The footbridge: a deck with posts and handrails along both sides.
    b.block(Vec3::new(bx - BRIDGE_HALF, 0.0, z0 - 0.25), Vec3::new(bx + BRIDGE_HALF, 0.06, z1 + 0.25), &wood, 1.0, false);
    for x in [bx - BRIDGE_HALF, bx + BRIDGE_HALF] {
        b.block(Vec3::new(x - 0.04, 0.0, z0 - 0.2), Vec3::new(x + 0.04, 1.0, z1 + 0.2), &rail, 1.0, true);
        for zp in [z0 - 0.2, z, z1 + 0.2] {
            b.block(Vec3::new(x - 0.06, 0.0, zp - 0.06), Vec3::new(x + 0.06, 1.05, zp + 0.06), &wood, 1.0, false);
        }
        b.block(Vec3::new(x - 0.05, 0.98, z0 - 0.25), Vec3::new(x + 0.05, 1.06, z1 + 0.25), &wood, 1.0, false);
    }
}

/// Marble columns along both sides.
/// Floor areas taken by stairs, landings and lofts (x/z rectangles, with a
/// little clearance), so decorative columns never block them.
pub fn stair_zones(r: &RoomPlan) -> Vec<(Vec2, Vec2)> {
    let run = |h: f32| h / super::plan::STEP_RISE * STEP_RUN;
    // Door swings, the space in front of doors, divider openings, pillars and
    // streams count too.
    let mut z = door_zones(r);
    z.extend(passage_zones(r));
    for p in pillars(r) {
        z.push((p - Vec2::splat(PILLAR_R + 0.5), p + Vec2::splat(PILLAR_R + 0.5)));
    }
    if let Some((sz, _)) = stream_bridge(r) {
        z.push((Vec2::new(r.min.x, sz - STREAM_HALF - 0.6), Vec2::new(r.max.x, sz + STREAM_HALF + 0.6)));
    }
    if let Some(Feature::RaisedExit { height }) = r.feature {
        z.push((Vec2::new(r.exit_x - 1.3, r.min.y), Vec2::new(r.exit_x + 1.3, r.min.y + LANDING_DEPTH + run(height) + 0.8)));
    }
    if r.entry_height > 0.0 {
        z.push((Vec2::new(r.entry_x - 1.3, r.max.y - LANDING_DEPTH - run(r.entry_height) - 0.8), Vec2::new(r.entry_x + 1.3, r.max.y)));
    }
    if let Some(Feature::KeyLoft { east }) = r.feature {
        let (x0, x1) = if east { (r.max.x - 2.2, r.max.x) } else { (r.min.x, r.min.x + 2.2) };
        z.push((Vec2::new(x0, r.min.y + 1.2), Vec2::new(x1, r.min.y + 3.8 + run(LOFT_HEIGHT) + 0.6)));
    }
    z
}

/// How far Atrium columns stand from the side walls: flush in narrow rooms so
/// they never close the way through.
pub fn atrium_column_inset(r: &RoomPlan) -> f32 {
    if r.width() < 5.5 { 0.32 } else { 0.9 }
}

fn atrium_architecture(b: &mut Builder, props: &PropMaterials, r: &RoomPlan) {
    let marble = b.mats.marble.clone();
    let h = r.height;
    let keep_clear = stair_zones(r);
    let inset = atrium_column_inset(r);
    let mut z = r.max.y - 1.5;
    while z > r.min.y + 1.0 {
        for x in [r.min.x + inset, r.max.x - inset] {
            if keep_clear.iter().any(|(lo, hi)| x + 0.45 > lo.x && x - 0.45 < hi.x && z + 0.45 > lo.y && z - 0.45 < hi.y) {
                continue;
            }
            let e = b
                .commands
                .spawn((Mesh3d(props.unit_cyl.clone()), MeshMaterial3d(marble.clone()), Transform::from_xyz(x, h / 2.0, z).with_scale(Vec3::new(0.55, h, 0.55)), Collider(Aabb { min: Vec3::new(x - 0.3, 0.0, z - 0.3), max: Vec3::new(x + 0.3, h, z + 0.3) })))
                .id();
            b.tag(e);
            b.block(Vec3::new(x - 0.4, 0.0, z - 0.4), Vec3::new(x + 0.4, 0.25, z + 0.4), &marble, 1.0, false);
            b.block(Vec3::new(x - 0.42, h - 0.3, z - 0.42), Vec3::new(x + 0.42, h, z + 0.42), &marble, 1.0, false);
        }
        z -= 3.2;
    }
}

fn crate_stack(b: &mut Builder, assets: &AssetServer, s: Slot, rng: &mut GameRng) {
    // Wooden crates (and a barrel now and then) against the wall.
    let n = 1 + rng.below(3);
    for k in 0..n {
        let along = (k as f32 - 1.0) * 0.75;
        let at = Vec3::new(s.x + s.inward * 0.45, 0.0, s.z + along);
        match rng.below(3) {
            0 => super::scenery::place(b, assets, "crate_a", at, s.yaw + rng.range_f32(-0.2, 0.2), Vec3::splat(1.0), Some(Vec3::new(0.4, 0.6, 0.4))),
            1 => super::scenery::place(b, assets, "crate_b", at, s.yaw + FRAC_PI_2 + rng.range_f32(-0.2, 0.2), Vec3::splat(1.0), Some(Vec3::new(0.6, 0.5, 0.3))),
            _ => super::scenery::place(b, assets, "barrel", at, rng.range_f32(0.0, 6.28), Vec3::splat(1.0), Some(Vec3::new(0.3, 0.9, 0.3))),
        };
    }
}

/// Plank crate with edge battens.
fn crate_box(b: &mut Builder, c: Vec3, size: f32, yaw: f32, solid: bool) {
    let planks = b.mats.door_wood.clone();
    let trim = b.mats.trim_wood.clone();
    let mut e = b.commands.spawn((Transform::from_translation(c).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()));
    if solid {
        e.insert(Collider(Aabb { min: c - Vec3::splat(size / 2.0), max: c + Vec3::splat(size / 2.0) }));
    }
    let root = e.id();
    b.tag(root);
    let body = b.mesh(Vec3::splat(size - 0.04), 0.7);
    let edge = b.mesh(Vec3::new(size, 0.06, 0.06), 0.5);
    let s = size / 2.0 - 0.03;
    b.commands.entity(root).with_children(|p| {
        p.spawn((Mesh3d(body), MeshMaterial3d(planks), Transform::default()));
        for (y, z) in [(-s, -s), (-s, s), (s, -s), (s, s)] {
            p.spawn((Mesh3d(edge.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(0.0, y, z)));
            p.spawn((Mesh3d(edge.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(y, z, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2))));
            p.spawn((Mesh3d(edge.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(y, 0.0, z).with_rotation(Quat::from_rotation_z(FRAC_PI_2))));
        }
    });
}

/// Door 100 gets a grand surround: marble pilasters, a lintel and red lamps.
pub fn final_door_surround(b: &mut Builder, r: &RoomPlan) {
    if r.kind != RoomKind::Final {
        return;
    }
    b.member = Some(r.index);
    let marble = b.mats.marble.clone();
    let brass = b.mats.brass.clone();
    let (x, z) = (r.exit_x, r.min.y);
    for dx in [-0.85f32, 0.85] {
        b.block(Vec3::new(x + dx - 0.18, 0.0, z), Vec3::new(x + dx + 0.18, 3.2, z + 0.25), &marble, 1.0, true);
    }
    b.block(Vec3::new(x - 1.15, 3.2, z), Vec3::new(x + 1.15, 3.6, z + 0.3), &marble, 1.0, false);
    b.block(Vec3::new(x - 1.15, 3.6, z), Vec3::new(x + 1.15, 3.66, z + 0.32), &brass, 1.0, false);
    let e = b
        .commands
        .spawn((PointLight { color: Color::srgb(1.0, 0.3, 0.2), intensity: 60_000.0, range: 8.0, ..default() }, Flicker { base: 60_000.0, seed: 99.0, depth: 0.3 }, Transform::from_xyz(x, 3.0, z + 1.0)))
        .id();
    b.tag(e);
    // A dim landing corridor behind Door 100 (the ending's lift set replaces
    // it when the cutscene starts), so the open door never shows the void.
    let a = Vec3::new(x, 0.0, z - WALL_T);
    let concrete = b.mats.concrete_wall.clone();
    let floor_mat = b.mats.concrete_floor.clone();
    let mut ids = vec![
        b.block(a + Vec3::new(-1.6, -0.1, -2.7), a + Vec3::new(1.6, 0.0, 0.0), &floor_mat, 1.0, true),
        b.block(a + Vec3::new(-1.8, 0.0, -2.7), a + Vec3::new(-1.6, 3.2, 0.0), &concrete, 1.0, true),
        b.block(a + Vec3::new(1.6, 0.0, -2.7), a + Vec3::new(1.8, 3.2, 0.0), &concrete, 1.0, true),
        b.block(a + Vec3::new(-1.8, 3.2, -2.9), a + Vec3::new(1.8, 3.4, 0.0), &concrete, 1.0, false),
        b.block(a + Vec3::new(-1.8, 0.0, -2.9), a + Vec3::new(1.8, 3.2, -2.7), &concrete, 1.0, true),
    ];
    ids.push(b.commands.spawn((PointLight { color: Color::srgb(1.0, 0.75, 0.45), intensity: 8_000.0, range: 4.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(a + Vec3::new(0.0, 2.8, -1.4)))).id());
    for id in ids {
        b.tag(id);
        b.commands.entity(id).insert(FinalVestibule);
    }
    b.member = None;
}

/// The stand-in corridor behind Door 100 (see [`final_door_surround`]).
#[derive(Component)]
pub struct FinalVestibule;

#[allow(dead_code)]
fn member_of(e: Entity, q: &Query<&RoomMember>) -> Option<u32> {
    q.get(e).ok().map(|m| m.0)
}
