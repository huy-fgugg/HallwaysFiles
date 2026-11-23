//! The player body: a rigged low-poly character model ("Low Poly Dude",
//! CC0, see assets/models/player/LICENSE.txt) used for teammates in game,
//! players in cutscenes and the character preview in the menus.
//!
//! The model has no animations of its own: [`BodyPose`] on the body's root
//! entity is turned into bone rotations every frame (walk, run, crouch, reach,
//! panic, death), so every player body moves the same way. Clothes and skin
//! are recoloured from the player's [`Skin`].

use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use serde::{Deserialize, Serialize};

use crate::items::ItemKind;
use crate::viewmodel::{build_item, grip_transform, BodyKit};

/// A player's look: indices into the palettes below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Skin {
    pub shirt: u8,
    pub pants: u8,
    pub boots: u8,
    pub skin: u8,
    /// 0 = no cap.
    pub hat: u8,
}

impl Default for Skin {
    fn default() -> Self {
        Self { shirt: 0, pants: 0, boots: 0, skin: 1, hat: 1 }
    }
}

pub const SHIRTS: [(&str, [f32; 3]); 10] = [
    ("Ivory", [0.86, 0.84, 0.78]),
    ("Night blue", [0.16, 0.24, 0.42]),
    ("Moss", [0.24, 0.36, 0.2]),
    ("Brick", [0.55, 0.18, 0.14]),
    ("Mustard", [0.78, 0.6, 0.18]),
    ("Charcoal", [0.14, 0.14, 0.16]),
    ("Plum", [0.38, 0.17, 0.4]),
    ("Teal", [0.12, 0.42, 0.44]),
    ("Rose", [0.82, 0.45, 0.55]),
    ("Sky", [0.45, 0.62, 0.82]),
];
pub const PANTS: [(&str, [f32; 3]); 6] = [
    ("Denim", [0.2, 0.32, 0.56]),
    ("Black", [0.08, 0.08, 0.1]),
    ("Khaki", [0.52, 0.46, 0.32]),
    ("Grey", [0.36, 0.36, 0.38]),
    ("Brown", [0.32, 0.2, 0.12]),
    ("Olive", [0.3, 0.32, 0.18]),
];
pub const BOOTS: [(&str, [f32; 3]); 5] = [
    ("Brown", [0.28, 0.17, 0.09]),
    ("Black", [0.05, 0.05, 0.05]),
    ("Tan", [0.6, 0.45, 0.28]),
    ("White", [0.85, 0.85, 0.82]),
    ("Red", [0.55, 0.12, 0.1]),
];
pub const SKIN_TONES: [(&str, [f32; 3]); 6] = [
    ("Light", [0.95, 0.8, 0.68]),
    ("Fair", [0.88, 0.68, 0.54]),
    ("Medium", [0.72, 0.52, 0.38]),
    ("Tan", [0.6, 0.42, 0.29]),
    ("Brown", [0.44, 0.29, 0.19]),
    ("Deep", [0.28, 0.18, 0.12]),
];
/// Index 0 is "no cap".
pub const HATS: [(&str, [f32; 3]); 7] = [
    ("None", [0.0, 0.0, 0.0]),
    ("Red", [0.62, 0.12, 0.1]),
    ("Navy", [0.12, 0.16, 0.3]),
    ("Green", [0.18, 0.34, 0.18]),
    ("Black", [0.06, 0.06, 0.07]),
    ("Yellow", [0.85, 0.7, 0.15]),
    ("White", [0.88, 0.88, 0.86]),
];

/// The parts of a skin the customise screen can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinPart {
    Shirt,
    Pants,
    Boots,
    Skin,
    Hat,
}

impl SkinPart {
    pub const ALL: [SkinPart; 5] = [SkinPart::Shirt, SkinPart::Pants, SkinPart::Boots, SkinPart::Skin, SkinPart::Hat];
    pub fn label(self) -> &'static str {
        match self {
            SkinPart::Shirt => "Shirt",
            SkinPart::Pants => "Trousers",
            SkinPart::Boots => "Boots",
            SkinPart::Skin => "Skin",
            SkinPart::Hat => "Cap",
        }
    }
    fn len(self) -> usize {
        match self {
            SkinPart::Shirt => SHIRTS.len(),
            SkinPart::Pants => PANTS.len(),
            SkinPart::Boots => BOOTS.len(),
            SkinPart::Skin => SKIN_TONES.len(),
            SkinPart::Hat => HATS.len(),
        }
    }
}

impl Skin {
    /// A different look per player id, for players who never customised.
    pub fn for_id(id: u8) -> Self {
        let i = id as usize;
        Self { shirt: (i * 3 % SHIRTS.len()) as u8, pants: (i % PANTS.len()) as u8, boots: (i % BOOTS.len()) as u8, skin: (i * 2 % SKIN_TONES.len()) as u8, hat: ((i % (HATS.len() - 1)) + 1) as u8 }
    }

    pub fn get(&self, part: SkinPart) -> u8 {
        match part {
            SkinPart::Shirt => self.shirt,
            SkinPart::Pants => self.pants,
            SkinPart::Boots => self.boots,
            SkinPart::Skin => self.skin,
            SkinPart::Hat => self.hat,
        }
    }

    /// Steps one part to the next / previous colour (wrapping).
    pub fn step(&mut self, part: SkinPart, delta: i32) {
        let n = part.len() as i32;
        let v = (self.get(part) as i32 + delta).rem_euclid(n) as u8;
        match part {
            SkinPart::Shirt => self.shirt = v,
            SkinPart::Pants => self.pants = v,
            SkinPart::Boots => self.boots = v,
            SkinPart::Skin => self.skin = v,
            SkinPart::Hat => self.hat = v,
        }
    }

    pub fn name(&self, part: SkinPart) -> &'static str {
        let i = self.get(part) as usize;
        match part {
            SkinPart::Shirt => SHIRTS[i % SHIRTS.len()].0,
            SkinPart::Pants => PANTS[i % PANTS.len()].0,
            SkinPart::Boots => BOOTS[i % BOOTS.len()].0,
            SkinPart::Skin => SKIN_TONES[i % SKIN_TONES.len()].0,
            SkinPart::Hat => HATS[i % HATS.len()].0,
        }
    }

    pub fn shirt_color(&self) -> Color {
        rgb(SHIRTS[self.shirt as usize % SHIRTS.len()].1)
    }

    pub fn skin_color(&self) -> Color {
        rgb(SKIN_TONES[self.skin as usize % SKIN_TONES.len()].1)
    }

    fn color_for(&self, material: &str) -> Option<Color> {
        Some(match material {
            "Shirt" => self.shirt_color(),
            "Pants" => rgb(PANTS[self.pants as usize % PANTS.len()].1),
            "Boots" => rgb(BOOTS[self.boots as usize % BOOTS.len()].1),
            "Skin" => self.skin_color(),
            "Hat" => rgb(HATS[(self.hat as usize % HATS.len()).max(1)].1),
            _ => return None,
        })
    }
}

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// The loaded character model.
#[derive(Resource)]
pub struct PlayerModel(pub Handle<WorldAsset>);

/// Recoloured materials, shared by every body with the same skin.
#[derive(Resource, Default)]
struct SkinMaterials(HashMap<(Skin, &'static str), Handle<StandardMaterial>>);

/// On the model's scene entity until it's rigged.
#[derive(Component)]
pub struct BodyModel {
    pub skin: Skin,
    /// Give the hands holdable items and a flashlight beam (teammates).
    pub items: bool,
}

/// The pose of a body this frame (on the body's root entity: the avatar,
/// cutscene actor or preview). Angles are radians; index 0 is the right side.
/// Positive: legs and arms swing forward, knees and elbows bend, the upper
/// body leans forward, the head looks down.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BodyPose {
    /// Hips lowered by this much (crouch).
    pub drop: f32,
    /// Upper body leaning forward.
    pub lean: f32,
    pub head: f32,
    pub head_yaw: f32,
    pub thigh: [f32; 2],
    pub calf: [f32; 2],
    pub arm: [f32; 2],
    pub arm_roll: [f32; 2],
    pub forearm: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Hips,
    Spine,
    Head,
    Thigh(usize),
    Calf(usize),
    Bicep(usize),
    Forearm(usize),
    Hand(usize),
}

/// A posable bone: its rest transform and the body root's axes expressed in
/// its parent's space (so a rotation "about the body's X axis" can be applied
/// whatever the bone's own orientation).
#[derive(Component, Debug, Clone, Copy)]
struct Bone {
    part: Part,
    rest: Quat,
    rest_pos: Vec3,
    x: Vec3,
    y: Vec3,
    z: Vec3,
}

/// A body's mouth (scaled open by voice chat).
#[derive(Component)]
pub struct Mouth;

/// A held item on a body's hand.
#[derive(Component)]
pub struct HeldItem {
    pub kind: ItemKind,
    pub right: bool,
}

/// A body's flashlight beam.
#[derive(Component)]
pub struct HeldBeam;

fn part_for(name: &str) -> Option<Part> {
    let side = |s: &str| if s.ends_with(".R") { 0 } else { 1 };
    Some(match name {
        "Hips" => Part::Hips,
        "Spine1" => Part::Spine,
        "Head" => Part::Head,
        n if n.starts_with("Thigh.") => Part::Thigh(side(n)),
        n if n.starts_with("Calf.") => Part::Calf(side(n)),
        n if n.starts_with("Bicep.") => Part::Bicep(side(n)),
        n if n.starts_with("Forearm.") => Part::Forearm(side(n)),
        n if n.starts_with("Hand.") => Part::Hand(side(n)),
        _ => return None,
    })
}

pub struct BodyPlugin;

impl Plugin for BodyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkinMaterials>()
            .add_systems(Startup, |mut c: Commands, assets: Res<AssetServer>| c.insert_resource(PlayerModel(assets.load(GltfAssetLabel::Scene(0).from_asset("models/player/player.glb")))))
            .add_observer(rig_body)
            .add_systems(PostUpdate, apply_poses.before(TransformSystems::Propagate));
    }
}

/// Adds a body under `parent` (which should carry a [`BodyPose`]).
pub fn spawn_body(c: &mut ChildSpawnerCommands, model: &PlayerModel, skin: Skin, items: bool) {
    // The model faces +Z; bodies face -Z like the player.
    c.spawn((WorldAssetRoot(model.0.clone()), Transform::from_rotation(Quat::from_rotation_y(PI)), Visibility::default(), BodyModel { skin, items }));
}

/// Changes the skin of a body that's already rigged (or still loading).
pub fn reskin(commands: &mut Commands, body: Entity, skin: Skin) {
    commands.entity(body).insert(Reskin(skin));
}

#[derive(Component)]
struct Reskin(Skin);

#[allow(clippy::too_many_arguments)]
fn rig_body(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<(&BodyModel, &ChildOf)>,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    mat_names: Query<&GltfMaterialName>,
    mut skins: ResMut<SkinMaterials>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let scene = ready.entity;
    let Ok((model, root)) = models.get(scene) else { return };
    let root = root.parent();
    let skin = model.skin;
    let mut stack = vec![scene];
    let mut hands: Vec<(Entity, usize, Quat)> = vec![];
    let mut heads: Vec<(Entity, Quat)> = vec![];
    while let Some(e) = stack.pop() {
        if let Ok(k) = children.get(e) {
            stack.extend(k.iter());
        }
        if let Ok(m) = mat_names.get(e) {
            paint(&mut commands, e, &m.0, skin, &mut skins, &mut mats);
        }
        let Some(part) = names.get(e).ok().and_then(|n| part_for(n.as_str())) else { continue };
        let Ok(tf) = transforms.get(e) else { continue };
        // The parent's rest rotation relative to the body root.
        let mut rot = Quat::IDENTITY;
        let mut cur = parents.get(e).ok().map(|p| p.parent());
        while let Some(p) = cur {
            if p == root {
                break;
            }
            rot = transforms.get(p).map(|t| t.rotation).unwrap_or_default() * rot;
            cur = parents.get(p).ok().map(|q| q.parent());
        }
        let inv = rot.inverse();
        commands.entity(e).insert(Bone { part, rest: tf.rotation, rest_pos: tf.translation, x: inv * Vec3::X, y: inv * Vec3::Y, z: inv * Vec3::Z });
        if let Part::Hand(side) = part {
            hands.push((e, side, rot * tf.rotation));
        }
        if part == Part::Head {
            heads.push((e, rot * tf.rotation));
        }
    }
    commands.entity(scene).remove::<BodyModel>();
    // The model's head is blank: give it a face (laid out in the body's frame,
    // which faces -Z, with the head bone at the top of the neck).
    let face = FaceKit::new(&mut meshes, &mut mats);
    for (head, rot) in heads {
        let frame = Transform::from_rotation(rot.inverse());
        commands.entity(head).with_children(|h| {
            h.spawn((frame, Visibility::default())).with_children(|f| {
                for x in [-0.034f32, 0.034] {
                    f.spawn((Mesh3d(face.ball.clone()), MeshMaterial3d(face.white.clone()), Transform::from_xyz(x, 0.062, -0.094).with_scale(Vec3::new(0.024, 0.017, 0.012))));
                    f.spawn((Mesh3d(face.ball.clone()), MeshMaterial3d(face.dark.clone()), Transform::from_xyz(x, 0.062, -0.104).with_scale(Vec3::splat(0.01))));
                    f.spawn((Mesh3d(face.ball.clone()), MeshMaterial3d(face.dark.clone()), Transform::from_xyz(x, 0.087, -0.097).with_rotation(Quat::from_rotation_z(-x.signum() * 0.15)).with_scale(Vec3::new(0.028, 0.006, 0.01))));
                }
                // The mouth opens when the player talks (voice chat).
                f.spawn((Mesh3d(face.ball.clone()), MeshMaterial3d(face.dark.clone()), Transform::from_xyz(0.0, 0.008, -0.095).with_scale(Vec3::new(0.034, 0.008, 0.012)), Mouth));
            });
        });
    }
    if !model.items {
        return;
    }
    // Items and the flashlight beam ride on the hands, laid out in the body's frame.
    let kit = BodyKit::new(&mut mats, &mut meshes, skin.shirt_color());
    for (hand, side, hand_rot) in hands {
        let right = side == 0;
        let frame = Transform::from_rotation(hand_rot.inverse() * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_translation(hand_rot.inverse() * Vec3::new(0.0, -0.07, 0.0));
        commands.entity(hand).with_children(|h| {
            h.spawn((frame, Visibility::default())).with_children(|f| {
                for kind in ItemKind::ALL {
                    f.spawn((HeldItem { kind, right }, grip_transform(right, 1.0, true), Visibility::Hidden)).with_children(|it| build_item(it, &kit, kind, 1.0));
                }
                if !right {
                    f.spawn((
                        HeldBeam,
                        SpotLight { intensity: 400_000.0, range: 14.0, outer_angle: 0.42, inner_angle: 0.25, shadow_maps_enabled: false, color: Color::srgb(1.0, 0.95, 0.85), ..default() },
                        Transform::from_xyz(0.0, 0.03, -0.25),
                        Visibility::Hidden,
                    ));
                }
            });
        });
    }
}

struct FaceKit {
    ball: Handle<Mesh>,
    white: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
}

impl FaceKit {
    fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        Self {
            ball: meshes.add(Sphere::new(1.0).mesh().uv(12, 8)),
            white: mats.add(StandardMaterial { base_color: Color::srgb(0.93, 0.92, 0.9), perceptual_roughness: 0.3, ..default() }),
            dark: mats.add(StandardMaterial { base_color: Color::srgb(0.06, 0.05, 0.05), perceptual_roughness: 0.4, ..default() }),
        }
    }
}

fn paint(commands: &mut Commands, e: Entity, material: &str, skin: Skin, skins: &mut SkinMaterials, mats: &mut Assets<StandardMaterial>) {
    let key: &'static str = match material {
        "Shirt" => "Shirt",
        "Pants" => "Pants",
        "Boots" => "Boots",
        "Skin" => "Skin",
        "Hat" => "Hat",
        _ => return,
    };
    if key == "Hat" {
        commands.entity(e).insert(if skin.hat == 0 { Visibility::Hidden } else { Visibility::Inherited });
    }
    let Some(color) = skin.color_for(key) else { return };
    let handle = skins
        .0
        .entry((skin, key))
        .or_insert_with(|| mats.add(StandardMaterial { base_color: color, perceptual_roughness: if key == "Skin" { 0.6 } else { 0.9 }, ..default() }))
        .clone();
    commands.entity(e).insert(MeshMaterial3d(handle));
}

/// Turns each body's [`BodyPose`] into bone transforms, and applies skin changes.
#[allow(clippy::type_complexity)]
fn apply_poses(
    mut commands: Commands,
    poses: Query<(Entity, &BodyPose, &Children, Option<&Reskin>)>,
    children: Query<&Children>,
    mut bones: Query<(&Bone, &mut Transform)>,
    mat_names: Query<&GltfMaterialName>,
    mut skins: ResMut<SkinMaterials>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    // QA: HALLWAYS_POSE_TEST=rest shows bodies in their rest pose; =<part>
    // (thigh, calf, arm, forearm, lean, head) raises just that part.
    let test = std::env::var("HALLWAYS_POSE_TEST").ok();
    for (root, pose, kids, reskin) in &poses {
        let pose = &match test.as_deref() {
            Some("rest") => BodyPose::default(),
            Some("thigh") => BodyPose { thigh: [0.8, 0.0], ..default() },
            Some("calf") => BodyPose { calf: [0.8, 0.0], ..default() },
            Some("arm") => BodyPose { arm: [0.8, 0.0], ..default() },
            Some("forearm") => BodyPose { forearm: [0.8, 0.0], ..default() },
            Some("lean") => BodyPose { lean: 0.5, ..default() },
            Some("head") => BodyPose { head: 0.5, ..default() },
            _ => *pose,
        };
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok(k) = children.get(e) {
                stack.extend(k.iter());
            }
            if let Some(Reskin(skin)) = reskin
                && let Ok(m) = mat_names.get(e)
            {
                paint(&mut commands, e, &m.0, *skin, &mut skins, &mut mats);
            }
            let Ok((b, mut tf)) = bones.get_mut(e) else { continue };
            let turn = |pitch: f32, yaw: f32, roll: f32| Quat::from_axis_angle(b.x, pitch) * Quat::from_axis_angle(b.y, yaw) * Quat::from_axis_angle(b.z, roll) * b.rest;
            match b.part {
                Part::Hips => {
                    tf.translation = b.rest_pos - b.y * pose.drop;
                    tf.rotation = b.rest;
                }
                Part::Spine => tf.rotation = turn(-pose.lean, 0.0, 0.0),
                Part::Head => tf.rotation = turn(-pose.head, pose.head_yaw, 0.0),
                Part::Thigh(s) => tf.rotation = turn(pose.thigh[s], 0.0, 0.0),
                Part::Calf(s) => tf.rotation = turn(-pose.calf[s], 0.0, 0.0),
                Part::Bicep(s) => tf.rotation = turn(pose.arm[s], 0.0, pose.arm_roll[s]),
                Part::Forearm(s) => tf.rotation = turn(pose.forearm[s], 0.0, 0.0),
                Part::Hand(_) => {}
            }
        }
        if reskin.is_some() {
            commands.entity(root).remove::<Reskin>();
        }
    }
}

/// A standing, breathing idle pose (time in seconds) for previews.
pub fn idle_pose(t: f32) -> BodyPose {
    let breathe = (t * 1.6).sin() * 0.03;
    BodyPose { lean: breathe, head: -breathe, head_yaw: (t * 0.5).sin() * 0.25, arm: [0.05 + breathe, 0.05 - breathe], forearm: [0.15, 0.15], ..default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_step_and_wrap() {
        let mut s = Skin::default();
        for part in SkinPart::ALL {
            let n = part.len() as i32;
            let before = s.get(part);
            s.step(part, n);
            assert_eq!(s.get(part), before, "{part:?} wraps");
            s.step(part, -1);
            assert_ne!(s.get(part), before);
        }
        assert_eq!(SkinPart::Hat.len(), HATS.len());
        assert_eq!(HATS[0].0, "None");
    }

    #[test]
    fn every_model_part_has_a_colour() {
        let s = Skin::for_id(3);
        for m in ["Shirt", "Pants", "Boots", "Skin", "Hat"] {
            assert!(s.color_for(m).is_some(), "{m}");
        }
        assert_ne!(Skin::for_id(1), Skin::for_id(2));
    }

    #[test]
    fn bones_map_to_parts() {
        assert_eq!(part_for("Thigh.R"), Some(Part::Thigh(0)));
        assert_eq!(part_for("Bicep.L"), Some(Part::Bicep(1)));
        assert_eq!(part_for("Finger1.L"), None);
        assert!(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/player/player.glb")).exists());
    }
}
