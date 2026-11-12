//! The 3D world for Phase 1: a handcrafted test room (Room 000) and a corridor
//! (Room 001), built from textured, trimmed room shells, a detailed panelled
//! door model, and animated CC0 glTF furniture.

pub mod elevator;
pub mod materials;
pub mod meshes;
pub mod plaque;

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    camera::Hdr,
    core_pipeline::tonemapping::Tonemapping,
    pbr::{DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};
use std::collections::HashMap;

use crate::interaction::{AnimatedPart, Door, Drawer, HidingSpot, InteractKind, Interactable, PartMotion};
use crate::physics::{Aabb, Collider};
use crate::player::{Player, PlayerCamera};
use crate::rng::{GameRng, RunSeed};
use crate::run::RunState;
use crate::settings::Settings;
use crate::state::{gameplay_active, AppState};
use materials::SurfaceMaterials;
use meshes::tiled_box;

pub const WALL_T: f32 = 0.2;
pub const DOOR_W: f32 = 1.0;
pub const DOOR_H: f32 = 2.1;

/// A named room region; entering it updates the HUD room counter.
#[derive(Component, Debug, Clone, Copy)]
pub struct RoomZone {
    pub number: u32,
    pub aabb: Aabb,
}

/// Smooth, seeded light flicker (never strobes; gentler with Reduce Flashing).
#[derive(Component, Debug, Clone, Copy)]
pub struct Flicker {
    pub base: f32,
    pub seed: f32,
    pub depth: f32,
}

/// Global lighting mood driven by cutscenes and entities (e.g. red alert).
#[derive(Resource, Debug, Clone, Copy)]
pub struct Mood {
    pub tint: Color,
    /// 0 = lights keep their own colour, 1 = fully `tint`.
    pub amount: f32,
    /// Brightness multiplier.
    pub scale: f32,
    /// Extra flicker depth 0..1 (strobing is suppressed with Reduce Flashing).
    pub flicker: f32,
}

impl Default for Mood {
    fn default() -> Self {
        Self { tint: Color::WHITE, amount: 0.0, scale: 1.0, flicker: 0.0 }
    }
}

/// Original colour of a light, so moods can tint and restore it.
#[derive(Component, Debug, Clone, Copy)]
pub struct BaseColor(pub Color);

/// Maps glTF node names under a model to logical owners and motions.
#[derive(Component, Debug, Clone)]
pub struct ModelBindings(pub Vec<PartBinding>);

#[derive(Debug, Clone)]
pub struct PartBinding {
    pub node: &'static str,
    pub owner: Entity,
    pub motion: PartMotion,
    /// Some source models ship doors slightly ajar; reset them to closed.
    pub reset_rotation: bool,
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight { color: Color::srgb(0.55, 0.62, 0.8), brightness: 340.0, ..default() })
            .init_resource::<Mood>()
            .add_plugins(elevator::ElevatorPlugin)
            .add_systems(OnEnter(AppState::Playing), (spawn_test_level, |mut m: ResMut<Mood>| *m = Mood::default()))
            .add_systems(OnEnter(AppState::MainMenu), spawn_menu_scene)
            .add_systems(Update, (bind_model_parts, flicker_lights, animate_menu_camera))
            .add_systems(Update, track_room.run_if(gameplay_active));
    }
}

/// Transforms a local-space box by `tf` and returns the enclosing world AABB.
pub fn world_aabb(local_min: Vec3, local_max: Vec3, tf: &Transform) -> Aabb {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { local_min.x } else { local_max.x },
            if i & 2 == 0 { local_min.y } else { local_max.y },
            if i & 4 == 0 { local_min.z } else { local_max.z },
        );
        let w = tf.transform_point(c);
        min = min.min(w);
        max = max.max(w);
    }
    Aabb { min, max }
}

/// Yaw that makes the player face along world direction `dir`.
pub fn yaw_facing(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    North,
    South,
    West,
    East,
}

pub struct Opening {
    pub side: Side,
    /// Centre along the wall (x for N/S walls, z for E/W walls).
    pub center: f32,
    pub width: f32,
    pub height: f32,
    /// Sill height above the floor (raised doors on stair landings).
    pub bottom: f32,
}

pub struct RoomStyle {
    pub wall: Handle<StandardMaterial>,
    pub wall_tile: f32,
    pub floor: Handle<StandardMaterial>,
    pub floor_tile: f32,
    pub wainscot: bool,
}

/// Small helper that owns the spawning context for level geometry.
pub struct Builder<'a, 'w, 's> {
    pub(crate) commands: &'a mut Commands<'w, 's>,
    pub(crate) meshes: &'a mut Assets<Mesh>,
    pub(crate) mats: SurfaceMaterials,
    pub(crate) cache: HashMap<[i32; 4], Handle<Mesh>>,
    /// Which state owns (and despawns) what we build.
    pub(crate) scope: AppState,
    /// When set, blocks are spawned as children of this entity in its local
    /// space, without colliders (used for decorative door assemblies).
    pub(crate) parent: Option<Entity>,
    /// Generated room this geometry belongs to (for streaming).
    pub(crate) member: Option<u32>,
    /// Floor height for doors built next (raised landings).
    pub(crate) base_y: f32,
}

impl Builder<'_, '_, '_> {
    /// Adds the despawn scope and room membership to a root entity.
    pub(crate) fn tag(&mut self, e: Entity) {
        let mut ec = self.commands.entity(e);
        ec.insert(DespawnOnExit(self.scope));
        if let Some(m) = self.member {
            ec.insert(crate::floor::RoomMember(m));
        }
    }

    pub(crate) fn mesh(&mut self, size: Vec3, tile: f32) -> Handle<Mesh> {
        let key = [size.x, size.y, size.z, tile].map(|v| (v * 1000.0).round() as i32);
        if let Some(h) = self.cache.get(&key) {
            return h.clone();
        }
        let h = self.meshes.add(tiled_box(size, tile));
        self.cache.insert(key, h.clone());
        h
    }

    /// Spawns a textured box from world-space min/max corners.
    pub(crate) fn block(&mut self, min: Vec3, max: Vec3, mat: &Handle<StandardMaterial>, tile: f32, solid: bool) -> Entity {
        let size = (max - min).abs();
        let center = (min + max) * 0.5;
        let mesh = self.mesh(size, tile);
        let mut e = self.commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::from_translation(center)));
        if let Some(p) = self.parent {
            e.insert(ChildOf(p));
            return e.id();
        }
        let id = e.id();
        self.tag(id);
        let mut e = self.commands.entity(id);
        if solid {
            e.insert(Collider(Aabb { min: min.min(max), max: min.max(max) }));
        }
        e.id()
    }

    /// Child box in a parent's local space (used for door details).
    pub(crate) fn part(&mut self, parent: Entity, center: Vec3, size: Vec3, mat: &Handle<StandardMaterial>, tile: f32) {
        let mesh = self.mesh(size, tile);
        self.commands.entity(parent).with_child((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::from_translation(center)));
    }

    /// Builds floor, ceiling, walls (with openings) and interior trim for a room.
    /// Sides in `shared` get trim only (the neighbouring room owns that wall).
    pub(crate) fn room(&mut self, min: Vec2, max: Vec2, height: f32, style: &RoomStyle, openings: &[Opening], shared: &[Side]) {
        let m = self.mats.clone();
        // Floor and ceiling.
        self.block(Vec3::new(min.x, -0.1, min.y), Vec3::new(max.x, 0.0, max.y), &style.floor, style.floor_tile, false);
        self.block(Vec3::new(min.x, height, min.y), Vec3::new(max.x, height + 0.1, max.y), &m.plaster_ceiling, 2.0, false);

        for side in [Side::North, Side::South, Side::West, Side::East] {
            // a = coordinate along the wall; the wall's interior face is at `line`.
            let (a0, a1, line, inward) = match side {
                Side::North => (min.x, max.x, min.y, 1.0),
                Side::South => (min.x, max.x, max.y, -1.0),
                Side::West => (min.y, max.y, min.x, 1.0),
                Side::East => (min.y, max.y, max.x, -1.0),
            };
            let along_x = matches!(side, Side::North | Side::South);
            // Box from (along range, y range, depth range) where depth > 0 goes into the room.
            let to_box = |a_lo: f32, a_hi: f32, y_lo: f32, y_hi: f32, d_lo: f32, d_hi: f32| -> (Vec3, Vec3) {
                let p_lo = line + inward * d_lo;
                let p_hi = line + inward * d_hi;
                if along_x {
                    (Vec3::new(a_lo, y_lo, p_lo), Vec3::new(a_hi, y_hi, p_hi))
                } else {
                    (Vec3::new(p_lo, y_lo, a_lo), Vec3::new(p_hi, y_hi, a_hi))
                }
            };
            let side_openings: Vec<&Opening> = openings.iter().filter(|o| o.side == side).collect();
            // Wall intervals (full height) between openings, plus lintels above them.
            let mut cuts: Vec<(f32, f32)> = side_openings.iter().map(|o| (o.center - o.width / 2.0, o.center + o.width / 2.0)).collect();
            cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
            let ext = if along_x { WALL_T } else { 0.0 }; // N/S walls cover the corners
            let mut spans = vec![];
            let mut cursor = a0 - ext;
            for (c0, c1) in &cuts {
                spans.push((cursor, *c0));
                cursor = *c1;
            }
            spans.push((cursor, a1 + ext));

            if !shared.contains(&side) {
                for (s0, s1) in &spans {
                    if s1 - s0 > 0.001 {
                        let (lo, hi) = to_box(*s0, *s1, 0.0, height, -WALL_T, 0.0);
                        self.block(lo, hi, &style.wall, style.wall_tile, true);
                    }
                }
                for o in &side_openings {
                    let (lo, hi) = to_box(o.center - o.width / 2.0, o.center + o.width / 2.0, o.bottom + o.height, height, -WALL_T, 0.0);
                    self.block(lo, hi, &style.wall, style.wall_tile, true);
                    if o.bottom > 0.001 {
                        let (lo, hi) = to_box(o.center - o.width / 2.0, o.center + o.width / 2.0, 0.0, o.bottom, -WALL_T, 0.0);
                        self.block(lo, hi, &style.wall, style.wall_tile, true);
                    }
                }
            }

            // Interior trim, broken at openings. E/W trims are inset to avoid
            // overlapping the N/S trims in the corners.
            let inset = if along_x { 0.0 } else { 0.075 };
            let mut trim_spans = vec![];
            let mut cursor = a0 + inset;
            for (c0, c1) in &cuts {
                trim_spans.push((cursor, c0 - 0.09)); // leave room for door casing
                cursor = c1 + 0.09;
            }
            trim_spans.push((cursor, a1 - inset));
            for (s0, s1) in trim_spans {
                if s1 - s0 < 0.02 {
                    continue;
                }
                if style.wainscot {
                    let (lo, hi) = to_box(s0, s1, 0.0, 0.95, 0.0, 0.018);
                    self.block(lo, hi, &m.panel_wood, 0.9, false);
                    let (lo, hi) = to_box(s0, s1, 0.93, 1.0, 0.0, 0.045);
                    self.block(lo, hi, &m.trim_wood, 0.8, false);
                    let (lo, hi) = to_box(s0, s1, 0.9, 0.93, 0.0, 0.03);
                    self.block(lo, hi, &m.trim_wood, 0.8, false);
                }
                let (lo, hi) = to_box(s0, s1, 0.0, 0.15, 0.0, 0.032);
                self.block(lo, hi, &m.trim_wood, 0.8, false);
                let (lo, hi) = to_box(s0, s1, 0.15, 0.17, 0.0, 0.022);
                self.block(lo, hi, &m.trim_wood, 0.8, false);
            }
            // Crown moulding (stepped, continuous).
            let (lo, hi) = to_box(a0 + inset, a1 - inset, height - 0.09, height, 0.0, 0.05);
            self.block(lo, hi, &m.trim_wood, 0.8, false);
            let (lo, hi) = to_box(a0 + inset, a1 - inset, height - 0.04, height, 0.05, 0.08);
            self.block(lo, hi, &m.trim_wood, 0.8, false);
        }
    }

    /// A four-panel door with casing, lining, threshold, brass knobs, hinges and
    /// an engraved number plaque, in a north-facing wall whose room-side face
    /// is at z = `line` (the door swings toward -Z).
    pub(crate) fn door(&mut self, cx: f32, line: f32, number: u32, locked: bool, images: &mut Assets<Image>, std_mats: &mut Assets<StandardMaterial>) -> Entity {
        let decor = self.parent.is_some();
        let m = self.mats.clone();
        let (w, h, t) = (DOOR_W, DOOR_H, WALL_T);
        let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);
        let (z_room, z_far) = (line, line - t);
        // Raised doors (stair landings) shift everything up by the builder's base height.
        let by = self.base_y;

        // Casing on both wall faces.
        for (zf, dir) in [(z_room, 1.0), (z_far, -1.0)] {
            let (za, zb) = (zf, zf + dir * 0.028);
            self.block(Vec3::new(x0 - 0.095, by, za), Vec3::new(x0, by + h + 0.095, zb), &m.trim_wood, 0.8, false);
            self.block(Vec3::new(x1, by, za), Vec3::new(x1 + 0.095, by + h + 0.095, zb), &m.trim_wood, 0.8, false);
            self.block(Vec3::new(x0 - 0.12, by + h + 0.095, za), Vec3::new(x1 + 0.12, by + h + 0.135, zb + dir * 0.012), &m.trim_wood, 0.8, false);
            self.block(Vec3::new(x0, by + h, za), Vec3::new(x1, by + h + 0.095, zb), &m.trim_wood, 0.8, false);
        }
        // Lining inside the wall thickness and a threshold.
        self.block(Vec3::new(x0, by, z_far), Vec3::new(x0 + 0.018, by + h, z_room), &m.trim_wood, 0.8, false);
        self.block(Vec3::new(x1 - 0.018, by, z_far), Vec3::new(x1, by + h, z_room), &m.trim_wood, 0.8, false);
        self.block(Vec3::new(x0, by + h - 0.018, z_far), Vec3::new(x1, by + h, z_room), &m.trim_wood, 0.8, false);
        self.block(Vec3::new(x0, by + -0.1, z_far - 0.01), Vec3::new(x1, by + 0.012, z_room + 0.01), &m.trim_wood, 0.6, false);

        // Big back-lit number sign on the wall above the frame, readable from across the room.
        let sign_img = images.add(plaque::sign_image(number));
        let sign_mat = std_mats.add(StandardMaterial {
            base_color_texture: Some(sign_img.clone()),
            emissive_texture: Some(sign_img),
            emissive: LinearRgba::rgb(2.2, 1.9, 1.5),
            perceptual_roughness: 0.5,
            ..default()
        });
        let sign_y = by + h + 0.33;
        self.block(Vec3::new(cx - 0.3, sign_y - 0.125, z_room), Vec3::new(cx + 0.3, sign_y + 0.125, z_room + 0.03), &m.brass, 1.0, false);
        let sign_quad = self.meshes.add(Rectangle::new(0.56, 0.21));
        let mut sign = self.commands.spawn((Mesh3d(sign_quad), MeshMaterial3d(sign_mat), Transform::from_xyz(cx, sign_y, z_room + 0.032)));
        if let Some(p) = self.parent {
            sign.insert(ChildOf(p));
        } else {
            let id = sign.id();
            self.tag(id);
        }

        // The blocker collider stays put while the leaf swings.
        let blocker = if decor {
            Entity::PLACEHOLDER
        } else {
            let id = self.commands.spawn((Transform::default(), Collider(Aabb { min: Vec3::new(x0, by, z_far), max: Vec3::new(x1, by + h, z_room) }))).id();
            self.tag(id);
            id
        };

        let leaf_w = w - 0.04;
        let leaf_h = h - 0.025;
        let hinge = Vec3::new(x0 + 0.02, by, line - t / 2.0);
        let mut root_cmd = self.commands.spawn((Transform::from_translation(hinge), Visibility::default(), Name::new(format!("Door {number:03}"))));
        if let Some(p) = self.parent {
            root_cmd.insert(ChildOf(p));
        } else {
            root_cmd.insert((
                Door::new(number, locked, blocker),
                Interactable {
                    kind: InteractKind::Door,
                    aabb: Aabb { min: Vec3::new(x0, by, z_far - 0.02), max: Vec3::new(x1, by + h, z_room + 0.05) },
                },
            ));
        }
        let root = root_cmd.id();
        if self.parent.is_none() {
            self.tag(root);
        }

        let slab_t = 0.042;
        let half = slab_t / 2.0;
        let mx = leaf_w / 2.0;
        self.part(root, Vec3::new(mx, leaf_h / 2.0 + 0.008, 0.0), Vec3::new(leaf_w, leaf_h, slab_t), &m.door_wood, 1.2);
        // Stiles, rails and mullions on both faces give the raised-panel look.
        let raise = 0.012;
        for s in [1.0f32, -1.0] {
            let z = s * (half + raise / 2.0);
            let y0 = 0.008;
            let stile = 0.11;
            self.part(root, Vec3::new(stile / 2.0, y0 + leaf_h / 2.0, z), Vec3::new(stile, leaf_h, raise), &m.panel_wood, 0.9);
            self.part(root, Vec3::new(leaf_w - stile / 2.0, y0 + leaf_h / 2.0, z), Vec3::new(stile, leaf_h, raise), &m.panel_wood, 0.9);
            let inner = leaf_w - 2.0 * stile;
            let rails = [(0.11, 0.22), (0.92, 0.16), (leaf_h - 0.055, 0.11)];
            for (cy, rh) in rails {
                self.part(root, Vec3::new(mx, y0 + cy, z), Vec3::new(inner, rh, raise), &m.panel_wood, 0.9);
            }
            for (lo, hi) in [(0.22, 0.84), (1.0, leaf_h - 0.11)] {
                self.part(root, Vec3::new(mx, y0 + (lo + hi) / 2.0, z), Vec3::new(0.09, hi - lo, raise), &m.panel_wood, 0.9);
            }
            // Thin bead around each panel for depth.
            for (cx_p, cy_p, pw, ph) in [
                (stile + (inner - 0.09) / 4.0, 0.53, (inner - 0.09) / 2.0, 0.62),
                (leaf_w - stile - (inner - 0.09) / 4.0, 0.53, (inner - 0.09) / 2.0, 0.62),
                (stile + (inner - 0.09) / 4.0, (1.0 + leaf_h - 0.11) / 2.0, (inner - 0.09) / 2.0, leaf_h - 0.11 - 1.0),
                (leaf_w - stile - (inner - 0.09) / 4.0, (1.0 + leaf_h - 0.11) / 2.0, (inner - 0.09) / 2.0, leaf_h - 0.11 - 1.0),
            ] {
                let bz = s * (half + 0.004);
                let b = 0.018;
                self.part(root, Vec3::new(cx_p, y0 + cy_p + ph / 2.0 - b / 2.0, bz), Vec3::new(pw, b, 0.008), &m.trim_wood, 0.5);
                self.part(root, Vec3::new(cx_p, y0 + cy_p - ph / 2.0 + b / 2.0, bz), Vec3::new(pw, b, 0.008), &m.trim_wood, 0.5);
                self.part(root, Vec3::new(cx_p - pw / 2.0 + b / 2.0, y0 + cy_p, bz), Vec3::new(b, ph, 0.008), &m.trim_wood, 0.5);
                self.part(root, Vec3::new(cx_p + pw / 2.0 - b / 2.0, y0 + cy_p, bz), Vec3::new(b, ph, 0.008), &m.trim_wood, 0.5);
            }
        }
        // Brass knobs with roses, and iron hinges.
        let knob_x = leaf_w - 0.075;
        let rose = self.meshes.add(Cylinder::new(0.032, 0.012));
        let neck = self.meshes.add(Cylinder::new(0.011, 0.045));
        let knob = self.meshes.add(Sphere::new(0.029).mesh().uv(32, 18));
        let plate = self.meshes.add(Cuboid::new(0.045, 0.16, 0.006));
        for s in [1.0f32, -1.0] {
            let face = s * (half + raise);
            let rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            self.commands.entity(root).with_children(|c| {
                c.spawn((Mesh3d(plate.clone()), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(knob_x, 0.95, face + s * 0.003)));
                c.spawn((Mesh3d(rose.clone()), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(knob_x, 0.98, face + s * 0.012).with_rotation(rot)));
                c.spawn((Mesh3d(neck.clone()), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(knob_x, 0.98, face + s * 0.035).with_rotation(rot)));
                c.spawn((Mesh3d(knob.clone()), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(knob_x, 0.98, face + s * 0.065)));
            });
        }
        for y in [0.25, 1.05, 1.85] {
            self.part(root, Vec3::new(-0.004, y, 0.0), Vec3::new(0.022, 0.1, slab_t + 0.012), &m.iron, 1.0);
            let pin = self.meshes.add(Cylinder::new(0.008, 0.12));
            self.commands.entity(root).with_child((Mesh3d(pin), MeshMaterial3d(m.iron.clone()), Transform::from_xyz(-0.016, y, 0.0)));
        }
        // Engraved number plaque on the room side.
        let img = images.add(plaque::plaque_image(number));
        let plaque_mat = std_mats.add(StandardMaterial {
            base_color_texture: Some(img),
            metallic: 0.8,
            perceptual_roughness: 0.38,
            ..default()
        });
        let quad = self.meshes.add(Rectangle::new(0.19, 0.079));
        let back = self.mesh(Vec3::new(0.205, 0.094, 0.006), 1.0);
        let pz = half + raise + 0.003;
        self.commands.entity(root).with_children(|c| {
            c.spawn((Mesh3d(back), MeshMaterial3d(m.brass.clone()), Transform::from_xyz(mx, 1.62, pz)));
            c.spawn((Mesh3d(quad), MeshMaterial3d(plaque_mat), Transform::from_xyz(mx, 1.62, pz + 0.0035)));
        });
        root
    }

    /// A non-interactive door assembly placed with an arbitrary transform
    /// (local +Z = the side the plaque faces).
    pub(crate) fn decor_door(&mut self, tf: Transform, number: u32, images: &mut Assets<Image>, std_mats: &mut Assets<StandardMaterial>) {
        let assembly = self.commands.spawn((tf, Visibility::default())).id();
        self.tag(assembly);
        self.parent = Some(assembly);
        self.door(0.0, 0.0, number, true, images, std_mats);
        self.parent = None;
    }
}

pub(crate) fn model(asset_server: &AssetServer, name: &str) -> WorldAssetRoot {
    WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{name}/{name}.gltf"))))
}

#[allow(clippy::too_many_arguments)]
fn spawn_test_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    seed: Res<RunSeed>,
) {
    let mats = materials::build(&asset_server, &mut std_mats);
    commands.insert_resource(mats.clone());
    let mut b = Builder { commands: &mut commands, meshes: &mut meshes, mats: mats.clone(), cache: HashMap::new(), scope: AppState::Playing, parent: None, member: None, base_y: 0.0 };
    let height = 3.0;

    // ---- Room 000: a faded sitting room -----------------------------------
    let room0 = RoomStyle { wall: mats.wallpaper.clone(), wall_tile: 1.6, floor: mats.wood_floor.clone(), floor_tile: 2.4, wainscot: true };
    b.room(
        Vec2::new(-4.0, -5.0),
        Vec2::new(4.0, 5.0),
        height,
        &room0,
        &[
            Opening { side: Side::North, center: 0.0, width: DOOR_W, height: DOOR_H, bottom: 0.0 },
            Opening { side: Side::South, center: 0.0, width: 2.0, height: 2.25, bottom: 0.0 },
        ],
        &[],
    );
    // Lift car behind the south wall, with a brass-trimmed surround on the room side.
    let blocker = elevator::spawn_elevator(b.commands, b.meshes, &mut std_mats, &mats);
    b.commands.insert_resource(elevator::reset_state(blocker));
    for (min, max) in [
        (Vec3::new(-1.12, 0.0, 4.97), Vec3::new(-1.0, 2.37, 5.0)),
        (Vec3::new(1.0, 0.0, 4.97), Vec3::new(1.12, 2.37, 5.0)),
        (Vec3::new(-1.18, 2.25, 4.96), Vec3::new(1.18, 2.42, 5.0)),
    ] {
        b.block(min, max, &mats.trim_wood, 0.8, false);
    }
    b.block(Vec3::new(-1.2, 2.42, 4.965), Vec3::new(1.2, 2.45, 5.0), &mats.brass, 1.0, false);
    b.block(Vec3::new(-1.0, -0.1, 5.0), Vec3::new(1.0, 0.01, 5.2), &mats.brass, 1.0, false);
    // ---- Room 001: a narrow, neglected corridor ---------------------------
    let corridor = RoomStyle { wall: mats.peeling_wall.clone(), wall_tile: 2.0, floor: mats.carpet.clone(), floor_tile: 1.8, wainscot: false };
    let c_min = Vec2::new(-1.4, -14.0);
    let c_max = Vec2::new(1.4, -5.0 - WALL_T);
    b.room(
        c_min,
        c_max,
        2.7,
        &corridor,
        &[
            Opening { side: Side::South, center: 0.0, width: DOOR_W, height: DOOR_H, bottom: 0.0 },
            Opening { side: Side::North, center: 0.0, width: DOOR_W, height: DOOR_H, bottom: 0.0 },
        ],
        &[Side::South],
    );

    // Doors: 001 opens into the corridor; 002 is locked until Phase 2.
    let _ = b.door(0.0, -5.0, 1, false, &mut images, &mut std_mats);
    let _ = b.door(0.0, c_min.y, 2, true, &mut images, &mut std_mats);

    // Rug.
    b.block(Vec3::new(-1.3, 0.0, -0.9), Vec3::new(1.3, 0.012, 2.6), &mats.carpet, 1.4, false);

    let rng_seed = seed.0;
    let loot_for = |id: u64| -> Option<u32> {
        let mut rng = GameRng::fork(rng_seed, 1000 + id);
        if rng.next_f32() < 0.55 { Some(2 + rng.below(7)) } else { None }
    };

    // ---- Drawer cabinet (west wall): four animated drawers -----------------
    let cab_tf = Transform::from_xyz(-4.0 + 0.25 + 0.02, 0.0, -1.6).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    let mut bindings = vec![];
    let drawer_ys = [(0.954, 1.103), (0.779, 0.928), (0.604, 0.753), (0.43, 0.578)];
    for (i, (y0, y1)) in drawer_ys.iter().enumerate() {
        let aabb = world_aabb(Vec3::new(-0.52, *y0, 0.15), Vec3::new(0.52, *y1, 0.56), &cab_tf);
        let mut loot = loot_for(i as u64);
        if i == 0 && loot.is_none() {
            loot = Some(3); // guarantee something to find
        }
        // Every run's first drawer holds vitamins (you already carry a flashlight).
        let item = (i == 0).then_some(crate::items::ItemKind::Vitamins);
        let owner = b
            .commands
            .spawn((Drawer { open: false, t: 0.0, loot, searched: false, has_key: false, tag: None, item }, Interactable { kind: InteractKind::Drawer, aabb }, DespawnOnExit(AppState::Playing)))
            .id();
        let node = ["drawer_cabinet_drawer_01", "drawer_cabinet_drawer_02", "drawer_cabinet_drawer_03", "drawer_cabinet_drawer_04"][i];
        bindings.push(PartBinding { node, owner, motion: PartMotion::Slide(Vec3::Z * 0.32), reset_rotation: false });
    }
    b.commands.spawn((
        model(&asset_server, "drawer_cabinet"),
        cab_tf,
        ModelBindings(bindings),
        Collider(world_aabb(Vec3::new(-0.571, 0.0, -0.248), Vec3::new(0.571, 1.881, 0.239), &cab_tf)),
        DespawnOnExit(AppState::Playing),
        Name::new("Drawer cabinet"),
    ));

    // ---- Gothic commode (north wall, right of the door): three drawers ------
    let com_tf = Transform::from_xyz(2.5, 0.0, -5.0 + 0.3);
    let mut bindings = vec![];
    for (i, (node, ty, tz)) in [("GothicCommode_01_door01", 0.987, 0.271), ("GothicCommode_01_door02", 0.667, 0.261), ("GothicCommode_01_door03", 0.356, 0.281)]
        .into_iter()
        .enumerate()
    {
        let aabb = world_aabb(Vec3::new(-0.5, ty - 0.125, tz - 0.08), Vec3::new(0.5, ty + 0.125, tz + 0.36), &com_tf);
        let owner = b
            .commands
            .spawn((
                Drawer { open: false, t: 0.0, loot: loot_for(10 + i as u64), searched: false, has_key: false, tag: None, item: None },
                Interactable { kind: InteractKind::Drawer, aabb },
                DespawnOnExit(AppState::Playing),
            ))
            .id();
        bindings.push(PartBinding { node, owner, motion: PartMotion::Slide(Vec3::Z * 0.3), reset_rotation: false });
    }
    b.commands.spawn((
        model(&asset_server, "GothicCommode_01"),
        com_tf,
        ModelBindings(bindings),
        Collider(world_aabb(Vec3::new(-0.6, 0.0, -0.29), Vec3::new(0.6, 1.21, 0.29), &com_tf)),
        DespawnOnExit(AppState::Playing),
        Name::new("Commode"),
    ));
    b.commands.spawn((model(&asset_server, "potted_plant_04"), Transform::from_xyz(2.15, 1.212, -4.75).with_scale(Vec3::splat(1.4)), DespawnOnExit(AppState::Playing)));
    b.commands.spawn((
        model(&asset_server, "mantel_clock_01"),
        Transform::from_xyz(2.75, 1.212, -4.78).with_rotation(Quat::from_rotation_y(-0.25)),
        DespawnOnExit(AppState::Playing),
    ));

    // ---- Gothic wardrobe (east wall): the hiding spot ----------------------
    let ward_tf = Transform::from_xyz(4.0 - 0.47, 0.0, 1.4).with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));
    let front_dir = ward_tf.rotation * Vec3::Z;
    let spot = b
        .commands
        .spawn((
            HidingSpot {
                inside: ward_tf.transform_point(Vec3::new(0.0, 0.0, 0.08)),
                inside_yaw: yaw_facing(front_dir),
                exit: ward_tf.transform_point(Vec3::new(0.0, 0.0, 1.15)),
                exit_yaw: yaw_facing(front_dir),
                doors_target: 0.0,
                doors_t: 0.0,
            },
            Interactable { kind: InteractKind::HidingSpot, aabb: world_aabb(Vec3::new(-0.86, 0.0, 0.3), Vec3::new(0.86, 2.3, 0.62), &ward_tf) },
            DespawnOnExit(AppState::Playing),
            Name::new("Wardrobe hiding spot"),
        ))
        .id();
    let hinge = |node: &'static str, angle: f32| PartBinding { node, owner: spot, motion: PartMotion::Hinge(angle), reset_rotation: true };
    b.commands.spawn((
        model(&asset_server, "GothicCabinet_01"),
        ward_tf,
        ModelBindings(vec![
            hinge("GothicCabinet_01_door1", -1.7),
            hinge("GothicCabinet_01_door3", -1.7),
            hinge("GothicCabinet_01_door2", 1.7),
            hinge("GothicCabinet_01_door4", 1.7),
        ]),
        Collider(world_aabb(Vec3::new(-0.86, 0.0, -0.457), Vec3::new(0.86, 2.36, 0.56), &ward_tf)),
        DespawnOnExit(AppState::Playing),
        Name::new("Wardrobe"),
    ));

    // ---- Set dressing -------------------------------------------------------
    let chair_tf = Transform::from_xyz(-3.05, 0.0, 3.9).with_rotation(Quat::from_rotation_y(2.4));
    b.commands.spawn((
        model(&asset_server, "ArmChair_01"),
        chair_tf,
        Collider(world_aabb(Vec3::new(-0.42, 0.0, -0.42), Vec3::new(0.42, 1.0, 0.36), &chair_tf)),
        DespawnOnExit(AppState::Playing),
    ));
    let stand_tf = Transform::from_xyz(-1.95, 0.0, 4.6).with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
    b.commands.spawn((
        model(&asset_server, "ClassicNightstand_01"),
        stand_tf,
        Collider(world_aabb(Vec3::new(-0.28, 0.0, -0.21), Vec3::new(0.28, 0.7, 0.21), &stand_tf)),
        DespawnOnExit(AppState::Playing),
    ));
    b.commands.spawn((
        model(&asset_server, "hanging_picture_frame_01"),
        Transform::from_xyz(2.9, 1.75, 5.0 - 0.001).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        DespawnOnExit(AppState::Playing),
    ));
    b.commands.spawn((
        model(&asset_server, "hanging_picture_frame_02"),
        Transform::from_xyz(-4.0 + 0.019, 1.75, 2.2).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::Playing),
    ));

    // ---- Lighting -----------------------------------------------------------
    b.commands.spawn((
        model(&asset_server, "modern_ceiling_lamp_01"),
        Transform::from_xyz(0.0, height - 1.17, 0.6),
        DespawnOnExit(AppState::Playing),
    ));
    b.commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.78, 0.52),
            intensity: 800_000.0,
            range: 16.0,
            radius: 0.05,
            shadow_maps_enabled: true,
            ..default()
        },
        Flicker { base: 800_000.0, seed: 1.3, depth: 0.05 },
        Transform::from_xyz(0.0, height - 1.0, 0.6),
        DespawnOnExit(AppState::Playing),
    ));
    // A faint cold spill from the open doorway side keeps corners readable.
    b.commands.spawn((
        PointLight { color: Color::srgb(0.55, 0.65, 0.9), intensity: 30_000.0, range: 9.0, shadow_maps_enabled: false, ..default() },
        Flicker { base: 30_000.0, seed: 0.0, depth: 0.0 },
        Transform::from_xyz(-2.5, 2.4, 3.5),
        DespawnOnExit(AppState::Playing),
    ));
    for (i, z) in [-8.0f32, -12.0].into_iter().enumerate() {
        let x = if i == 0 { c_min.x } else { c_max.x };
        let yaw = if i == 0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
        let tf = Transform::from_xyz(x, 1.75, z).with_rotation(Quat::from_rotation_y(yaw));
        b.commands.spawn((model(&asset_server, "industrial_wall_sconce"), tf, DespawnOnExit(AppState::Playing)));
        b.commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.86, 0.6),
                intensity: 120_000.0,
                range: 10.0,
                radius: 0.03,
                shadow_maps_enabled: true,
                ..default()
            },
            Flicker { base: 120_000.0, seed: 4.7 + i as f32 * 3.1, depth: 0.35 },
            Transform::from_translation(tf.transform_point(Vec3::new(0.0, 0.12, 0.26))),
            DespawnOnExit(AppState::Playing),
        ));
    }

    // Warm wall sconces in Room 000.
    for (i, (x, z, yaw)) in [(-4.0f32, 0.3f32, std::f32::consts::FRAC_PI_2), (4.0, -2.2, -std::f32::consts::FRAC_PI_2), (-1.8, 5.0, std::f32::consts::PI), (1.8, 5.0, std::f32::consts::PI)]
        .into_iter()
        .enumerate()
    {
        let tf = Transform::from_xyz(x, 1.9, z).with_rotation(Quat::from_rotation_y(yaw));
        b.commands.spawn((model(&asset_server, "industrial_wall_sconce"), tf, DespawnOnExit(AppState::Playing)));
        b.commands.spawn((
            PointLight { color: Color::srgb(1.0, 0.72, 0.45), intensity: 140_000.0, range: 9.0, radius: 0.03, shadow_maps_enabled: i < 2, ..default() },
            Flicker { base: 140_000.0, seed: 9.1 + i as f32, depth: 0.08 },
            Transform::from_translation(tf.transform_point(Vec3::new(0.0, 0.12, 0.26))),
            DespawnOnExit(AppState::Playing),
        ));
    }

    // ---- Room zones ---------------------------------------------------------
    b.commands.spawn((
        RoomZone { number: 0, aabb: Aabb { min: Vec3::new(-4.0, -1.0, -5.0), max: Vec3::new(4.0, 4.0, 5.0) } },
        DespawnOnExit(AppState::Playing),
    ));
    b.commands.spawn((
        RoomZone { number: 1, aabb: Aabb { min: Vec3::new(c_min.x, -1.0, c_min.y), max: Vec3::new(c_max.x, 4.0, -5.05) } },
        DespawnOnExit(AppState::Playing),
    ));

    // ---- Player + camera ----------------------------------------------------
    commands
        .spawn((
            Player::new(0.0),
            Transform::from_translation(elevator::PLAYER_START),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Name::new("Player"),
        ))
        .with_children(|p| {
            p.spawn((
                Camera3d::default(),
                PlayerCamera,
                Projection::from(PerspectiveProjection { fov: 72f32.to_radians(), near: 0.05, ..default() }),
                Transform::from_xyz(0.0, crate::player::STAND_EYE, 0.0),
                Hdr,
                Tonemapping::TonyMcMapface,
                Bloom { intensity: 0.12, ..Bloom::NATURAL },
                Msaa::Off,
                ScreenSpaceAmbientOcclusion::default(),
                TemporalAntiAliasing::default(),
                DistanceFog {
                    color: Color::srgb(0.02, 0.025, 0.035),
                    falloff: FogFalloff::Exponential { density: 0.07 },
                    ..default()
                },
            ));
        });
}

/// Marker for the slowly drifting main-menu camera.
#[derive(Component)]
pub struct MenuCamera;

/// The animated 3D backdrop behind the main menu: a long, lamplit guest-wing
/// hallway lined with numbered doors.
pub fn spawn_menu_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    let mats = materials::build(&asset_server, &mut std_mats);
    let scope = AppState::MainMenu;
    let mut b = Builder { commands: &mut commands, meshes: &mut meshes, mats: mats.clone(), cache: HashMap::new(), scope, parent: None, member: None, base_y: 0.0 };
    let (x0, x1, z0, z1, h) = (-1.7f32, 1.7f32, -42.0f32, 3.0f32, 3.0f32);
    let style = RoomStyle { wall: mats.wallpaper.clone(), wall_tile: 1.6, floor: mats.carpet.clone(), floor_tile: 1.8, wainscot: true };
    let door_zs: Vec<f32> = (0..6).map(|i| -4.0 - i as f32 * 6.0).collect();
    let mut openings = vec![];
    for &z in &door_zs {
        openings.push(Opening { side: Side::West, center: z, width: DOOR_W, height: DOOR_H, bottom: 0.0 });
        openings.push(Opening { side: Side::East, center: z - 3.0, width: DOOR_W, height: DOOR_H, bottom: 0.0 });
    }
    openings.push(Opening { side: Side::North, center: 0.0, width: DOOR_W, height: DOOR_H, bottom: 0.0 });
    b.room(Vec2::new(x0, z0), Vec2::new(x1, z1), h, &style, &openings, &[]);
    let mut number = 100;
    for &z in &door_zs {
        number += 1;
        b.decor_door(Transform::from_xyz(x0, 0.0, z).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)), number, &mut images, &mut std_mats);
        number += 1;
        b.decor_door(Transform::from_xyz(x1, 0.0, z - 3.0).with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)), number, &mut images, &mut std_mats);
    }
    b.decor_door(Transform::from_xyz(0.0, 0.0, z0), 100, &mut images, &mut std_mats);
    for (i, &z) in door_zs.iter().enumerate() {
        for (x, yaw) in [(x0, std::f32::consts::FRAC_PI_2), (x1, -std::f32::consts::FRAC_PI_2)] {
            let tf = Transform::from_xyz(x, 1.9, z - 1.5).with_rotation(Quat::from_rotation_y(yaw));
            b.commands.spawn((model(&asset_server, "industrial_wall_sconce"), tf, DespawnOnExit(scope)));
            b.commands.spawn((
                PointLight { color: Color::srgb(1.0, 0.72, 0.45), intensity: 60_000.0, range: 7.0, radius: 0.03, shadow_maps_enabled: i < 3, ..default() },
                Flicker { base: 60_000.0, seed: i as f32 * 2.3 + x, depth: if i == 4 { 0.6 } else { 0.08 } },
                Transform::from_translation(tf.transform_point(Vec3::new(0.0, 0.12, 0.26))),
                DespawnOnExit(scope),
            ));
        }
    }
    // A cold, red-tinged glow at the far end draws the eye down the hall.
    b.commands.spawn((
        PointLight { color: Color::srgb(1.0, 0.25, 0.2), intensity: 40_000.0, range: 10.0, ..default() },
        Flicker { base: 40_000.0, seed: 7.7, depth: 0.5 },
        Transform::from_xyz(0.0, 2.4, z0 + 1.5),
        DespawnOnExit(scope),
    ));
    commands.spawn((
        Camera3d::default(),
        MenuCamera,
        Projection::from(PerspectiveProjection { fov: 62f32.to_radians(), ..default() }),
        Transform::from_xyz(0.35, 1.6, 1.5).looking_at(Vec3::new(0.0, 1.4, -30.0), Vec3::Y),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom { intensity: 0.15, ..Bloom::NATURAL },
        DistanceFog { color: Color::srgb(0.02, 0.022, 0.03), falloff: FogFalloff::Exponential { density: 0.09 }, ..default() },
        DespawnOnExit(scope),
        Name::new("Menu camera"),
    ));
}

/// Slow forward drift with a gentle sway; loops back with the fog hiding the cut.
pub fn animate_menu_camera(time: Res<Time<Real>>, mut cam: Query<&mut Transform, With<MenuCamera>>) {
    let t = time.elapsed_secs();
    for mut tf in &mut cam {
        let z = 1.5 - (t * 0.35) % 18.0;
        tf.translation = Vec3::new(0.3 * (t * 0.13).sin(), 1.6 + (t * 0.9).sin() * 0.015, z);
        let look = Vec3::new(0.4 * (t * 0.07).sin(), 1.35, z - 20.0);
        *tf = tf.looking_at(look, Vec3::Y);
        tf.rotate_local_z((t * 0.21).sin() * 0.01);
    }
}

/// Finds glTF nodes by name under models with [`ModelBindings`] and attaches
/// [`AnimatedPart`] so they follow their logical owner.
fn bind_model_parts(
    mut commands: Commands,
    added: Query<(Entity, &Name, &Transform), Added<Name>>,
    parents: Query<&ChildOf>,
    bindings: Query<&ModelBindings>,
) {
    for (e, name, tf) in &added {
        let mut cur = e;
        for _ in 0..8 {
            let Ok(parent) = parents.get(cur) else { break };
            cur = parent.parent();
            if let Ok(b) = bindings.get(cur) {
                if let Some(binding) = b.0.iter().find(|pb| pb.node == name.as_str()) {
                    let mut base = *tf;
                    if binding.reset_rotation {
                        base.rotation = Quat::IDENTITY;
                    }
                    // The room may have streamed out while its glTF finished loading.
                    commands.entity(e).try_insert((AnimatedPart { owner: binding.owner, base, motion: binding.motion }, base));
                }
                break;
            }
        }
    }
}

fn flicker_lights(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<Settings>,
    mood: Res<Mood>,
    mut lights: Query<(Entity, &Flicker, &mut PointLight, Option<&BaseColor>)>,
) {
    let t = time.elapsed_secs();
    for (e, f, mut light, base) in &mut lights {
        let Some(base) = base else {
            commands.entity(e).insert(BaseColor(light.color));
            continue;
        };
        // Sum of incommensurate sines = smooth pseudo-noise.
        let n = ((t * 1.7 + f.seed).sin() * 0.5 + (t * 3.9 + f.seed * 2.0).sin() * 0.3 + (t * 0.6 + f.seed * 5.0).sin() * 0.2) * 0.5 + 0.5;
        let mut depth = f.depth;
        if !settings.reduce_flashing && f.depth > 0.2 && (t * 0.37 + f.seed).sin() > 0.97 {
            depth = 0.85;
        }
        if settings.reduce_flashing {
            depth *= 0.3;
        }
        let mut level = 1.0 - depth * n;
        if mood.flicker > 0.0 {
            if settings.reduce_flashing {
                // Slow breathing dim instead of a strobe.
                level *= 1.0 - mood.flicker * 0.4 * (0.5 + 0.5 * (t * 1.5 + f.seed).sin());
            } else {
                let strobe = ((t * 13.0 + f.seed * 7.0).sin() * (t * 7.3 + f.seed).sin()) > 0.1;
                level *= if strobe { 1.0 } else { 1.0 - mood.flicker };
            }
        }
        light.intensity = f.base * level * mood.scale;
        light.color = base.0.mix(&mood.tint, mood.amount);
    }
}

fn track_room(player: Query<&Transform, With<Player>>, zones: Query<&RoomZone>, floor: Option<Res<crate::floor::Floor>>, mut run: ResMut<RunState>) {
    let Ok(p) = player.single() else { return };
    if let Some(i) = floor.as_ref().and_then(|f| f.0.room_at_z(p.translation.z)) {
        if run.current_room != i {
            run.current_room = i;
        }
        return;
    }
    let probe = p.translation + Vec3::Y;
    for z in &zones {
        if z.aabb.contains(probe) && run.current_room != z.number {
            run.current_room = z.number;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_aabb_rotates_quarter_turn() {
        let tf = Transform::from_xyz(1.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let a = world_aabb(Vec3::new(-0.5, 0.0, -0.1), Vec3::new(0.5, 1.0, 0.1), &tf);
        assert!((a.min - Vec3::new(0.9, 0.0, -0.5)).length() < 1e-4, "{a:?}");
        assert!((a.max - Vec3::new(1.1, 1.0, 0.5)).length() < 1e-4, "{a:?}");
    }

    #[test]
    fn yaw_facing_matches_player_forward() {
        for dir in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            let yaw = yaw_facing(dir);
            let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            assert!((fwd - dir).length() < 1e-5, "{dir:?}");
        }
    }
}
