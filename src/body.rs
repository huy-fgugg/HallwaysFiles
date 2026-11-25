//! The player body: a chunky voxel chibi adventurer ("Voxel Chibi
//! Adventurers" free tier by zaddy99999, CC0, see
//! assets/models/chibi/LICENSE.txt). Used for teammates in game, players in
//! cutscenes and the character preview in the menus.
//!
//! Each hero comes split into head, torso, arms and legs; they hang on joints
//! here, and [`BodyPose`] on the body's root entity turns into joint rotations
//! every frame (walk, run, crouch, reach, panic, death), so every player body
//! moves the same way. A mouth opens on the face when the player talks.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use serde::{Deserialize, Serialize};

use crate::items::ItemKind;
use crate::viewmodel::{build_item, grip_transform, BodyKit};

/// A player's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Skin {
    /// Index into [`HEROES`].
    pub hero: u8,
    /// 0 = nothing, 1 = crown.
    pub hat: u8,
    /// 0 = nothing, 1 = adventurer's backpack.
    pub pack: u8,
}

impl Default for Skin {
    fn default() -> Self {
        Self { hero: 0, hat: 0, pack: 0 }
    }
}

/// The heroes: file prefix, name and a signature colour (used for sleeves on
/// held items).
pub const HEROES: [(&str, &str, [f32; 3]); 4] = [
    ("knight", "Knight", [0.5, 0.14, 0.16]),
    ("wizard", "Wizard", [0.38, 0.17, 0.4]),
    ("cleric", "Cleric", [0.85, 0.55, 0.15]),
    ("ranger", "Ranger", [0.24, 0.42, 0.2]),
];
pub const HATS: [&str; 2] = ["None", "Crown"];
pub const PACKS: [&str; 2] = ["None", "Backpack"];

/// The parts of a look the customise screen can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinPart {
    Hero,
    Hat,
    Pack,
}

impl SkinPart {
    pub const ALL: [SkinPart; 3] = [SkinPart::Hero, SkinPart::Hat, SkinPart::Pack];
    pub fn label(self) -> &'static str {
        match self {
            SkinPart::Hero => "Character",
            SkinPart::Hat => "Hat",
            SkinPart::Pack => "Back",
        }
    }
    fn len(self) -> usize {
        match self {
            SkinPart::Hero => HEROES.len(),
            SkinPart::Hat => HATS.len(),
            SkinPart::Pack => PACKS.len(),
        }
    }
}

impl Skin {
    /// A different look per player id, for players who never customised.
    pub fn for_id(id: u8) -> Self {
        let i = id as usize;
        Self { hero: (i % HEROES.len()) as u8, hat: ((i / HEROES.len()) % HATS.len()) as u8, pack: ((i / 2) % PACKS.len()) as u8 }
    }

    pub fn get(&self, part: SkinPart) -> u8 {
        match part {
            SkinPart::Hero => self.hero,
            SkinPart::Hat => self.hat,
            SkinPart::Pack => self.pack,
        }
    }

    /// Steps one part to the next / previous choice (wrapping).
    pub fn step(&mut self, part: SkinPart, delta: i32) {
        let n = part.len() as i32;
        let v = (self.get(part) as i32 + delta).rem_euclid(n) as u8;
        match part {
            SkinPart::Hero => self.hero = v,
            SkinPart::Hat => self.hat = v,
            SkinPart::Pack => self.pack = v,
        }
    }

    pub fn name(&self, part: SkinPart) -> &'static str {
        let i = self.get(part) as usize;
        match part {
            SkinPart::Hero => HEROES[i % HEROES.len()].1,
            SkinPart::Hat => HATS[i % HATS.len()],
            SkinPart::Pack => PACKS[i % PACKS.len()],
        }
    }

    fn hero_index(&self) -> usize {
        self.hero as usize % HEROES.len()
    }

    pub fn shirt_color(&self) -> Color {
        let c = HEROES[self.hero_index()].2;
        Color::srgb(c[0], c[1], c[2])
    }
}

/// How far a body reaches out from its centre (the widest head, or the arms):
/// bodies are kept at least this far from walls so nothing pokes through.
pub const BODY_REACH: f32 = 0.47;

/// Overall size: the heroes are 2.4-2.7 m tall at the pack's scale.
pub const BODY_SCALE: f32 = 0.58;
/// Hip height above the soles, in metres (the chibi legs hang straight down).
pub const LEG_LENGTH: f32 = HIPS.y * BODY_SCALE;
/// The big chibi heads are shrunk a little more so they fit through doors.
const HEAD_SCALE: f32 = 0.78;

// Joints in the hero grid, as metres in the body frame (facing -Z, right = +X):
// hips (torso pivot), neck, shoulders and hips for the legs.
const HIPS: Vec3 = Vec3::new(0.0, 0.6, 0.0);
const NECK: Vec3 = Vec3::new(0.0, 0.6, 0.0); // above the hips
// Tucked in a little from the pack's 0.65 so the arms stay inside BODY_REACH.
const SHOULDER: Vec3 = Vec3::new(0.55, 0.5, 0.0); // above the hips; x mirrored for the left
const LEG: Vec3 = Vec3::new(0.25, 0.0, 0.05);
const HAND: Vec3 = Vec3::new(0.0, -0.5, 0.0); // below the shoulder
/// Where the mouth sits on each hero's face, from the neck (measured from the
/// meshes: just in front of the face surface).
const MOUTHS: [Vec3; 4] = [Vec3::new(0.0, 0.25, -0.615), Vec3::new(0.0, 0.25, -0.615), Vec3::new(0.0, 0.15, -0.715), Vec3::new(0.0, 0.25, -0.615)];

const PART_FILES: [&str; 6] = ["torso", "head", "arm_r", "arm_l", "leg_r", "leg_l"];

/// Mesh offsets from each joint (metres at full size, body frame facing -Z), generated from the pack's manifest.
const HERO_PARTS: [[[f32; 3]; 6]; 4] = [
    [[-0.0, -0.3, -0.1], [-0.0, 0.0, -0.0], [-0.0, -0.5, -0.0], [-0.0, -0.5, -0.0], [-0.0, -0.6, -0.05], [-0.0, -0.6, -0.05]], // knight
    [[-0.0, -0.5, -0.05], [-0.0, -0.3, -0.0], [-0.0, -0.6, -0.0], [-0.0, -0.6, -0.0], [-0.0, -0.6, -0.05], [-0.0, -0.6, -0.05]], // wizard
    [[-0.0, -0.5, -0.05], [-0.0, -0.1, -0.05], [-0.0, -0.5, -0.0], [-0.0, -0.5, -0.0], [-0.0, -0.6, -0.05], [-0.0, -0.6, -0.05]], // cleric
    [[-0.3, -0.3, 0.1], [-0.0, -0.1, 0.05], [-0.0, -0.5, -0.0], [-0.0, -0.5, -0.0], [-0.0, -0.6, -0.05], [-0.0, -0.6, -0.05]], // ranger
];
const CROWN_OFFSET: [f32; 3] = [0.05, 0.9, 0.05];
const PACK_OFFSET: [f32; 3] = [-0.0, -0.3, 0.4];

/// The loaded hero parts and accessories.
#[derive(Resource, Clone)]
pub struct PlayerModel {
    parts: Vec<[Handle<WorldAsset>; 6]>,
    crown: Handle<WorldAsset>,
    pack: Handle<WorldAsset>,
    mouth_mesh: Handle<Mesh>,
    mouth_mat: Handle<StandardMaterial>,
}

impl PlayerModel {
    /// A hero's right or left arm model (first-person view).
    pub fn arm(&self, skin: Skin, right: bool) -> Handle<WorldAsset> {
        self.parts[skin.hero_index()][if right { 2 } else { 3 }].clone()
    }

    fn load(assets: &AssetServer, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        let scene = |name: &str| assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/chibi/{name}.glb")));
        Self {
            parts: HEROES.iter().map(|(h, _, _)| PART_FILES.map(|p| scene(&format!("{h}__{p}")))).collect(),
            crown: scene("crown"),
            pack: scene("adventurer_backpack"),
            mouth_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            mouth_mat: mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.03, 0.04), perceptual_roughness: 0.8, ..default() }),
        }
    }
}

/// The body under a [`BodyPose`] entity.
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
    Bicep(usize),
}

/// A posable joint and its rest position. Joints rest unrotated, in the body's
/// frame (facing -Z), so a pose angle is a plain rotation about X / Y / Z.
#[derive(Component, Debug, Clone, Copy)]
struct Bone {
    part: Part,
    rest_pos: Vec3,
}

/// A body's mouth (opened by voice chat).
#[derive(Component)]
pub struct Mouth;

/// How open a body's mouth is, 0..1 (set by voice chat on the body's root).
#[derive(Component, Default, Clone, Copy)]
pub struct Talking(pub f32);

/// A held item on a body's hand.
#[derive(Component)]
pub struct HeldItem {
    pub kind: ItemKind,
    pub right: bool,
}

/// A body's flashlight beam.
#[derive(Component)]
pub struct HeldBeam;

/// Marks a hand, where items are held.
#[derive(Component)]
struct Hand(usize);

pub struct BodyPlugin;

impl Plugin for BodyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, |mut c: Commands, assets: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>| {
            c.insert_resource(PlayerModel::load(&assets, &mut meshes, &mut mats))
        })
        .add_systems(Update, (apply_reskins, rig_bodies).chain())
        .add_systems(PostUpdate, apply_poses.before(TransformSystems::Propagate));
    }
}

/// Adds a body under `parent` (which should carry a [`BodyPose`]).
pub fn spawn_body(c: &mut ChildSpawnerCommands, model: &PlayerModel, skin: Skin, items: bool) {
    let mut e = c.spawn((Transform::from_scale(Vec3::splat(BODY_SCALE)), Visibility::default(), BodyModel { skin, items }));
    build_hero(&mut e, model, skin);
}

/// Changes the look of a body (on its [`BodyPose`] root entity).
pub fn reskin(commands: &mut Commands, body: Entity, skin: Skin) {
    commands.entity(body).insert(Reskin(skin));
}

#[derive(Component)]
struct Reskin(Skin);

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

/// A part's model under its joint, turned to face -Z.
fn part(c: &mut ChildSpawnerCommands, scene: &Handle<WorldAsset>, at: Vec3) {
    c.spawn((WorldAssetRoot(scene.clone()), Transform::from_translation(at).with_rotation(Quat::from_rotation_y(PI)), Visibility::default()));
}

fn build_hero(root: &mut EntityCommands, m: &PlayerModel, skin: Skin) {
    let h = skin.hero_index();
    let files = m.parts[h].clone();
    let offs = HERO_PARTS[h];
    let (crown, pack) = (m.crown.clone(), m.pack.clone());
    let (mouth_mesh, mouth_mat) = (m.mouth_mesh.clone(), m.mouth_mat.clone());
    root.with_children(|b| {
        b.spawn((Transform::from_translation(HIPS), Visibility::default(), Bone { part: Part::Hips, rest_pos: HIPS })).with_children(|hips| {
            for (s, x) in [(0usize, 1.0f32), (1, -1.0)] {
                let at = LEG * Vec3::new(x, 1.0, 1.0);
                hips.spawn((Transform::from_translation(at), Visibility::default(), Bone { part: Part::Thigh(s), rest_pos: at })).with_children(|leg| part(leg, &files[4 + s], v(offs[4 + s])));
            }
            hips.spawn((Transform::default(), Visibility::default(), Bone { part: Part::Spine, rest_pos: Vec3::ZERO })).with_children(|sp| {
                part(sp, &files[0], v(offs[0]));
                if skin.pack == 1 {
                    part(sp, &pack, v(PACK_OFFSET));
                }
                sp.spawn((Transform::from_translation(NECK).with_scale(Vec3::splat(HEAD_SCALE)), Visibility::default(), Bone { part: Part::Head, rest_pos: NECK })).with_children(|hd| {
                    part(hd, &files[1], v(offs[1]));
                    if skin.hat == 1 {
                        part(hd, &crown, v(CROWN_OFFSET));
                    }
                    hd.spawn((Mesh3d(mouth_mesh.clone()), MeshMaterial3d(mouth_mat.clone()), Transform::from_translation(MOUTHS[h]).with_scale(Vec3::new(0.16, 0.0, 0.02)), Visibility::Hidden, Mouth));
                });
                for (s, x) in [(0usize, 1.0f32), (1, -1.0)] {
                    let at = SHOULDER * Vec3::new(x, 1.0, 1.0);
                    sp.spawn((Transform::from_translation(at), Visibility::default(), Bone { part: Part::Bicep(s), rest_pos: at })).with_children(|arm| {
                        part(arm, &files[2 + s], v(offs[2 + s]));
                        arm.spawn((Transform::from_translation(HAND), Visibility::default(), Hand(s)));
                    });
                }
            });
        });
    });
}

/// Rebuilds bodies whose look changed.
fn apply_reskins(mut commands: Commands, model: Option<Res<PlayerModel>>, q: Query<(Entity, &Reskin, &Children)>, bodies: Query<&BodyModel>) {
    let Some(model) = model else { return };
    for (root, Reskin(skin), kids) in &q {
        for k in kids.iter() {
            if let Ok(b) = bodies.get(k) {
                let items = b.items;
                commands.entity(k).despawn();
                commands.entity(root).with_children(|c| spawn_body(c, &model, *skin, items));
            }
        }
        commands.entity(root).remove::<Reskin>();
    }
}

/// Gives new bodies their held items and flashlight beam.
fn rig_bodies(
    mut commands: Commands,
    bodies: Query<(Entity, &BodyModel), Added<BodyModel>>,
    children: Query<&Children>,
    hands: Query<&Hand>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (body, model) in &bodies {
        if !model.items {
            continue;
        }
        let kit = BodyKit::new(&mut mats, &mut meshes, model.skin.shirt_color());
        let mut stack = vec![body];
        while let Some(e) = stack.pop() {
            if let Ok(k) = children.get(e) {
                stack.extend(k.iter());
            }
            let Ok(Hand(side)) = hands.get(e) else { continue };
            let right = *side == 0;
            commands.entity(e).with_children(|h| {
                h.spawn((Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)).with_scale(Vec3::splat(1.0 / BODY_SCALE)), Visibility::default())).with_children(|f| {
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
}

/// The mouth's open height (body units) when talking at full volume.
pub const MOUTH_OPEN: f32 = 0.09;

/// Turns each body's [`BodyPose`] into joint transforms and opens talking mouths.
#[allow(clippy::type_complexity)]
fn apply_poses(
    poses: Query<(&BodyPose, &Children, Option<&Talking>)>,
    children: Query<&Children>,
    mut bones: Query<(&Bone, &mut Transform), Without<Mouth>>,
    mut mouths: Query<(&mut Transform, &mut Visibility), With<Mouth>>,
) {
    // QA: HALLWAYS_POSE_TEST=rest shows bodies in their rest pose; =<part>
    // (thigh, arm, lean, head, roll, talk) raises just that part.
    let test = std::env::var("HALLWAYS_POSE_TEST").ok();
    for (pose, kids, talking) in &poses {
        let pose = &match test.as_deref() {
            Some("rest") | Some("talk") => BodyPose::default(),
            Some("thigh") => BodyPose { thigh: [0.8, 0.0], ..default() },
            Some("arm") => BodyPose { arm: [0.8, 0.0], ..default() },
            Some("roll") => BodyPose { arm_roll: [0.8, 0.0], ..default() },
            Some("lean") => BodyPose { lean: 0.5, ..default() },
            Some("head") => BodyPose { head: 0.5, ..default() },
            _ => *pose,
        };
        let open = if test.as_deref() == Some("talk") { 1.0 } else { talking.map(|t| t.0).unwrap_or(0.0).clamp(0.0, 1.0) };
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok(k) = children.get(e) {
                stack.extend(k.iter());
            }
            if let Ok((mut tf, mut vis)) = mouths.get_mut(e) {
                tf.scale.y = MOUTH_OPEN * open;
                *vis = if open > 0.05 { Visibility::Inherited } else { Visibility::Hidden };
                continue;
            }
            let Ok((b, mut tf)) = bones.get_mut(e) else { continue };
            let turn = |pitch: f32, yaw: f32, roll: f32| Quat::from_rotation_x(pitch) * Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
            match b.part {
                Part::Hips => {
                    tf.translation = b.rest_pos - Vec3::Y * pose.drop / BODY_SCALE;
                    tf.rotation = Quat::IDENTITY;
                }
                Part::Spine => tf.rotation = turn(-pose.lean, 0.0, 0.0),
                Part::Head => tf.rotation = turn(-pose.head, pose.head_yaw, 0.0),
                // No knees on a chibi: a bent calf just bends the thigh a little less.
                Part::Thigh(s) => tf.rotation = turn(pose.thigh[s] - pose.calf[s] * 0.35, 0.0, 0.0),
                // Positive roll lifts the arm out to the side.
                Part::Bicep(s) => tf.rotation = turn(pose.arm[s] + pose.forearm[s] * 0.3, 0.0, if s == 0 { pose.arm_roll[s] } else { -pose.arm_roll[s] }),
            }
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
            s.step(part, 1);
            assert_ne!(s.get(part), before);
        }
        assert_ne!(Skin::for_id(1), Skin::for_id(2));
    }

    #[test]
    fn every_hero_part_is_shipped() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/chibi/");
        for (h, _, _) in HEROES {
            for p in PART_FILES {
                assert!(std::path::Path::new(&format!("{dir}{h}__{p}.glb")).exists(), "{h}__{p}");
            }
        }
        for f in ["crown", "adventurer_backpack", "LICENSE"] {
            let ext = if f == "LICENSE" { "txt" } else { "glb" };
            assert!(std::path::Path::new(&format!("{dir}{f}.{ext}")).exists(), "{f}");
        }
    }

    #[test]
    fn bodies_fit_inside_their_reach() {
        // Widest head (20 voxels) and the outside of the arms (shoulder 0.65 + 0.25).
        assert!(2.0 * 0.5 * BODY_SCALE * HEAD_SCALE <= BODY_REACH);
        assert!((SHOULDER.x + 0.25) * BODY_SCALE <= BODY_REACH + 0.01);
        // ...and still fit through a door, the tallest hat included (the wizard's
        // head is 25 voxels tall above the neck).
        assert!(BODY_REACH * 2.0 < crate::floor::plan::DOOR_W);
        let tallest = (HIPS.y + NECK.y) * BODY_SCALE + 2.5 * BODY_SCALE * HEAD_SCALE;
        assert!(tallest < crate::world::DOOR_H - 0.1, "{tallest}");
    }

    #[test]
    fn feet_stand_on_the_floor() {
        // Legs hang 0.6 m below the hips (at 0.6 m), so the soles are at 0.
        for h in HERO_PARTS {
            assert!((HIPS.y + LEG.y + h[4][1]).abs() < 0.01);
        }
    }
}
