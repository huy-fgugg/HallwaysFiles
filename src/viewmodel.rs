//! First-person arms: two hands built from primitives that hold the selected
//! item, sway with walking and looking, and play short gestures (reach to
//! open, use an item, flinch when hurt, lower/raise when switching items).
//! The same [`Gesture`] is sent to other players so their view of you moves
//! the same way (see `net` and `avatar`).

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::interaction::HideState;
use crate::items::{Inventory, ItemKind};
use crate::player::{Player, PlayerCamera};
use crate::run::RunState;
use crate::state::gameplay_active;

/// A one-shot arm animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Gesture {
    #[default]
    None,
    /// Reach forward to open a door, drawer or closet.
    Reach,
    /// Raise the held item (drink vitamins, wrap a bandage, click the torch).
    Use,
    /// Recoil after taking damage.
    Flinch,
    /// Lower and raise the hands while switching items.
    Swap,
}

impl Gesture {
    pub fn duration(self) -> f32 {
        match self {
            Gesture::None => 0.0,
            Gesture::Reach => 0.42,
            Gesture::Use => 0.85,
            Gesture::Flinch => 0.35,
            Gesture::Swap => 0.36,
        }
    }

    /// 0 → 1 → 0 envelope for a gesture at time `t`.
    pub fn envelope(self, t: f32) -> f32 {
        let d = self.duration();
        if d <= 0.0 || t >= d {
            return 0.0;
        }
        let x = (t / d).clamp(0.0, 1.0);
        // Fast in, slower out.
        if x < 0.35 { smooth(x / 0.35) } else { smooth(1.0 - (x - 0.35) / 0.65) }
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Ask the arms to play a gesture.
#[derive(Message, Debug, Clone, Copy)]
pub struct ArmAnim(pub Gesture);

/// The local player's current gesture (also replicated to other players).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PlayerGesture {
    pub gesture: Gesture,
    pub t: f32,
}

#[derive(Component)]
struct ViewRoot;

#[derive(Component, Clone, Copy, PartialEq)]
struct ViewArm {
    right: bool,
    base: Vec3,
}

/// An item mesh held in a hand; shown only while that item is held.
#[derive(Component, Clone, Copy)]
pub struct HeldItem {
    pub kind: ItemKind,
    pub right: bool,
}

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ArmAnim>()
            .init_resource::<PlayerGesture>()
            .add_systems(Update, (spawn_arms, (trigger_gestures, animate_arms).chain().after(crate::player::CameraSet)));
    }
}

/// Shared materials and meshes for hands and held items (also used by avatars).
pub struct BodyKit {
    pub skin: Handle<StandardMaterial>,
    pub sleeve: Handle<StandardMaterial>,
    pub cuff: Handle<StandardMaterial>,
    pub cube: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    /// Unit-radius capsule along Y (radius 0.5, total length 2).
    pub capsule: Handle<Mesh>,
    pub items: ItemMats,
}

pub struct ItemMats {
    pub black: Handle<StandardMaterial>,
    pub lens: Handle<StandardMaterial>,
    pub steel: Handle<StandardMaterial>,
    pub brass: Handle<StandardMaterial>,
    pub cloth: Handle<StandardMaterial>,
    pub orange: Handle<StandardMaterial>,
    pub white: Handle<StandardMaterial>,
}

impl BodyKit {
    pub fn new(mats: &mut Assets<StandardMaterial>, meshes: &mut Assets<Mesh>, sleeve: Color) -> Self {
        let m = |c: Color, rough: f32, metal: f32| StandardMaterial { base_color: c, perceptual_roughness: rough, metallic: metal, ..default() };
        Self {
            skin: mats.add(m(Color::srgb(0.78, 0.58, 0.45), 0.7, 0.0)),
            sleeve: mats.add(m(sleeve, 0.85, 0.0)),
            cuff: mats.add(m(Color::srgb(0.08, 0.08, 0.09), 0.8, 0.0)),
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            cyl: meshes.add(Cylinder::new(0.5, 1.0)),
            sphere: meshes.add(Sphere::new(0.5).mesh().uv(16, 10)),
            capsule: meshes.add(Capsule3d::new(0.5, 1.0).mesh().rings(4).latitudes(10).longitudes(14)),
            items: ItemMats {
                black: mats.add(m(Color::srgb(0.03, 0.03, 0.035), 0.55, 0.0)),
                lens: mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(6.0, 5.6, 4.6), ..default() }),
                steel: mats.add(m(Color::srgb(0.7, 0.72, 0.75), 0.3, 1.0)),
                brass: mats.add(m(Color::srgb(0.78, 0.6, 0.3), 0.35, 1.0)),
                cloth: mats.add(m(Color::srgb(0.92, 0.9, 0.84), 0.95, 0.0)),
                orange: mats.add(StandardMaterial { base_color: Color::srgba(0.9, 0.45, 0.1, 0.92), perceptual_roughness: 0.25, ..default() }),
                white: mats.add(m(Color::srgb(0.93, 0.93, 0.93), 0.5, 0.0)),
            },
        }
    }
}

fn part(c: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, pos: Vec3, size: Vec3) {
    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(pos).with_scale(size), NotShadowCaster));
}

/// A hand (palm, four fingers, thumb) at the origin, fingers toward -Z.
pub fn build_hand(c: &mut ChildSpawnerCommands, kit: &BodyKit, right: bool, scale: f32) {
    let s = scale;
    let side = if right { -1.0 } else { 1.0 };
    // Palm, with a rounded heel and knuckle ridge.
    part(c, &kit.cube, &kit.skin, Vec3::ZERO, Vec3::new(0.068, 0.026, 0.072) * s);
    part(c, &kit.sphere, &kit.skin, Vec3::new(0.0, 0.0, 0.03) * s, Vec3::new(0.068, 0.032, 0.04) * s);
    c.spawn((Mesh3d(kit.capsule.clone()), MeshMaterial3d(kit.skin.clone()), Transform::from_xyz(0.0, 0.004 * s, -0.036 * s).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.016 * s, 0.03 * s, 0.016 * s)), NotShadowCaster));
    for i in 0..4 {
        let x = (-0.024 + i as f32 * 0.016) * s;
        let len = [0.022, 0.026, 0.025, 0.02][i] * s;
        // Fingers curl down around a held item.
        c.spawn((
            Mesh3d(kit.capsule.clone()),
            MeshMaterial3d(kit.skin.clone()),
            Transform::from_xyz(x, -0.014 * s, -0.052 * s).with_rotation(Quat::from_rotation_x(-0.85 - std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.0145 * s, len, 0.0145 * s)),
            NotShadowCaster,
        ));
    }
    c.spawn((
        Mesh3d(kit.capsule.clone()),
        MeshMaterial3d(kit.skin.clone()),
        Transform::from_xyz(side * 0.038 * s, -0.006 * s, -0.018 * s)
            .with_rotation(Quat::from_rotation_y(side * 0.6) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::new(0.017 * s, 0.022 * s, 0.017 * s)),
        NotShadowCaster,
    ));
}

/// Builds a small model of `kind` (held in the palm, pointing -Z).
pub fn build_item(c: &mut ChildSpawnerCommands, kit: &BodyKit, kind: ItemKind, scale: f32) {
    let s = scale;
    let im = &kit.items;
    let lay = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let cyl = |c: &mut ChildSpawnerCommands, mat: &Handle<StandardMaterial>, pos: Vec3, r: f32, len: f32, rot: Quat| {
        c.spawn((Mesh3d(kit.cyl.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(pos * s).with_rotation(rot).with_scale(Vec3::new(r * 2.0, len, r * 2.0) * s), NotShadowCaster));
    };
    match kind {
        ItemKind::Flashlight => {
            cyl(c, &im.black, Vec3::new(0.0, 0.0, -0.05), 0.02, 0.17, lay);
            cyl(c, &im.black, Vec3::new(0.0, 0.0, -0.15), 0.029, 0.05, lay);
            cyl(c, &im.lens, Vec3::new(0.0, 0.0, -0.176), 0.024, 0.004, lay);
            part(c, &kit.cube, &im.steel, Vec3::new(0.0, 0.021, -0.07) * s, Vec3::new(0.012, 0.006, 0.02) * s);
        }
        ItemKind::Lockpick => {
            part(c, &kit.cube, &im.steel, Vec3::new(0.0, 0.0, -0.07) * s, Vec3::new(0.006, 0.004, 0.12) * s);
            part(c, &kit.cube, &im.steel, Vec3::new(0.012, 0.0, -0.06) * s, Vec3::new(0.005, 0.004, 0.1) * s);
            part(c, &kit.cube, &im.black, Vec3::new(0.006, 0.0, -0.005) * s, Vec3::new(0.03, 0.012, 0.035) * s);
        }
        ItemKind::Vitamins => {
            cyl(c, &im.orange, Vec3::new(0.0, 0.03, -0.035), 0.024, 0.075, Quat::IDENTITY);
            cyl(c, &im.white, Vec3::new(0.0, 0.074, -0.035), 0.026, 0.018, Quat::IDENTITY);
        }
        ItemKind::Bandage => {
            cyl(c, &im.cloth, Vec3::new(0.0, 0.025, -0.04), 0.034, 0.05, Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
            part(c, &kit.cube, &im.cloth, Vec3::new(0.0, -0.006, -0.08) * s, Vec3::new(0.045, 0.002, 0.05) * s);
        }
        ItemKind::SkeletonKey => {
            c.spawn((
                Mesh3d(kit.cyl.clone()),
                MeshMaterial3d(im.brass.clone()),
                Transform::from_xyz(0.0, 0.0, -0.02 * s).with_rotation(lay).with_scale(Vec3::new(0.045, 0.008, 0.045) * s),
                NotShadowCaster,
            ));
            part(c, &kit.cube, &im.brass, Vec3::new(0.0, 0.0, -0.09) * s, Vec3::new(0.008, 0.008, 0.11) * s);
            part(c, &kit.cube, &im.brass, Vec3::new(0.012, 0.0, -0.135) * s, Vec3::new(0.02, 0.008, 0.012) * s);
            part(c, &kit.cube, &im.brass, Vec3::new(0.01, 0.0, -0.115) * s, Vec3::new(0.016, 0.008, 0.008) * s);
        }
    }
}

const RIGHT_BASE: Vec3 = Vec3::new(0.19, -0.215, -0.44);
const LEFT_BASE: Vec3 = Vec3::new(-0.2, -0.235, -0.43);

fn spawn_arms(
    mut commands: Commands,
    cams: Query<Entity, Added<PlayerCamera>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for cam in &cams {
        let kit = BodyKit::new(&mut mats, &mut meshes, Color::srgb(0.16, 0.2, 0.26));
        let root = commands.spawn((ViewRoot, Transform::default(), Visibility::Hidden, ChildOf(cam), Name::new("View arms"))).id();
        for right in [true, false] {
            let base = if right { RIGHT_BASE } else { LEFT_BASE };
            commands
                .spawn((ViewArm { right, base }, Transform::from_translation(base), Visibility::default(), ChildOf(root)))
                .with_children(|arm| {
                    // Forearm sleeve reaching back past the screen edge, cuff, then the hand.
                    let along = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    arm.spawn((Mesh3d(kit.capsule.clone()), MeshMaterial3d(kit.sleeve.clone()), Transform::from_xyz(0.0, -0.012, 0.22).with_rotation(along).with_scale(Vec3::new(0.075, 0.17, 0.075)), NotShadowCaster));
                    arm.spawn((Mesh3d(kit.cyl.clone()), MeshMaterial3d(kit.cuff.clone()), Transform::from_xyz(0.0, -0.008, 0.06).with_rotation(along).with_scale(Vec3::new(0.08, 0.035, 0.08)), NotShadowCaster));
                    // Wrist.
                    arm.spawn((Mesh3d(kit.capsule.clone()), MeshMaterial3d(kit.skin.clone()), Transform::from_xyz(0.0, -0.004, 0.03).with_rotation(along).with_scale(Vec3::new(0.05, 0.03, 0.045)), NotShadowCaster));
                    arm.spawn((Transform::from_xyz(0.0, 0.0, -0.02), Visibility::default())).with_children(|hand| {
                        build_hand(hand, &kit, right, 1.0);
                        for kind in ItemKind::ALL {
                            hand.spawn((HeldItem { kind, right }, Transform::from_xyz(0.0, 0.03, -0.03), Visibility::Hidden)).with_children(|it| build_item(it, &kit, kind, 1.0));
                        }
                    });
                });
        }
    }
}

/// Which item each hand holds: the selected item in the right hand, and the
/// flashlight in the left while it's switched on (if not already in the right).
pub fn held_items(inv: &Inventory) -> (Option<ItemKind>, Option<ItemKind>) {
    let right = inv.slots[inv.selected].map(|s| s.kind);
    let left = (inv.flashlight_on && right != Some(ItemKind::Flashlight) && inv.has(ItemKind::Flashlight)).then_some(ItemKind::Flashlight);
    (right, left)
}

fn trigger_gestures(
    time: Res<Time>,
    mut anims: MessageReader<ArmAnim>,
    mut gesture: ResMut<PlayerGesture>,
    inv: Res<Inventory>,
    run: Res<RunState>,
    mut last: Local<(usize, f32, bool)>,
) {
    gesture.t += time.delta_secs();
    if gesture.t >= gesture.gesture.duration() {
        gesture.gesture = Gesture::None;
    }
    let start = |g: Gesture, gesture: &mut PlayerGesture| {
        gesture.gesture = g;
        gesture.t = 0.0;
    };
    for a in anims.read() {
        start(a.0, &mut gesture);
    }
    let (sel, vit, init) = *last;
    if init && inv.selected != sel && gesture.gesture == Gesture::None {
        start(Gesture::Swap, &mut gesture);
    }
    if init && run.vitality < vit - 0.5 {
        start(Gesture::Flinch, &mut gesture);
    }
    *last = (inv.selected, run.vitality, true);
}

#[allow(clippy::too_many_arguments)]
fn animate_arms(
    time: Res<Time>,
    gesture: Res<PlayerGesture>,
    inv: Res<Inventory>,
    hide: Res<HideState>,
    player: Query<&Player>,
    mut root: Query<&mut Visibility, (With<ViewRoot>, Without<HeldItem>)>,
    mut arms: Query<(&ViewArm, &mut Transform)>,
    mut items: Query<(&HeldItem, &mut Visibility), Without<ViewRoot>>,
    active: Option<Res<crate::cutscene::ActiveCutscene>>,
    state: (Res<State<crate::state::AppState>>, Res<State<crate::state::Overlay>>),
    mut sway: Local<(Vec2, f32, f32)>,
) {
    let Ok(p) = player.single() else { return };
    let playing = gameplay_active(state.0, state.1, active);
    let show = playing && !hide.is_hiding();
    for mut v in &mut root {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !show {
        return;
    }
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    // Look sway: hands lag behind fast turns.
    let (mut lag, last_yaw, last_pitch) = *sway;
    let d = Vec2::new(p.yaw - last_yaw, p.pitch - last_pitch);
    let d = if d.length() > 1.0 { Vec2::ZERO } else { d };
    lag = (lag + d * 0.6).clamp(Vec2::splat(-0.05), Vec2::splat(0.05));
    lag *= (1.0 - dt * 9.0).max(0.0);
    *sway = (lag, p.yaw, p.pitch);

    let speed_k = (p.speed / crate::player::WALK_SPEED).min(1.6);
    let phase = p.bob_phase;
    let sprint = if p.sprinting { 1.0 } else { 0.0 };
    let (held_r, held_l) = held_items(&inv);
    let g = gesture.gesture;
    let e = g.envelope(gesture.t);
    for (arm, mut tf) in &mut arms {
        let s = if arm.right { 1.0 } else { -1.0 };
        let mut pos = arm.base;
        let mut rot = Quat::IDENTITY;
        // Breathing and walking.
        pos.y += (t * 1.6).sin() * 0.004;
        pos.y += (phase * 2.0).sin().abs() * 0.012 * speed_k;
        pos.x += (phase).sin() * 0.008 * speed_k * s;
        // Sprinting: hands drop and pump.
        pos.y -= 0.05 * sprint;
        pos.z += (phase + if arm.right { 0.0 } else { std::f32::consts::PI }).sin() * 0.05 * sprint;
        rot *= Quat::from_rotation_x(-0.25 * sprint);
        // Crouch: hands come up a little.
        if p.crouching {
            pos.y += 0.02;
        }
        pos.x -= lag.x * 0.6;
        pos.y += lag.y * 0.4;
        // Empty left hand hangs lower.
        if !arm.right && held_l.is_none() {
            pos.y -= 0.2;
            pos.x -= 0.05;
        }
        match g {
            Gesture::Reach if arm.right => {
                pos += Vec3::new(-0.05, 0.08, -0.2) * e;
                rot *= Quat::from_rotation_x(0.35 * e);
            }
            Gesture::Use if arm.right => {
                pos += Vec3::new(-0.1, 0.13, 0.08) * e;
                rot *= Quat::from_rotation_x(0.9 * e) * Quat::from_rotation_z(0.4 * e);
            }
            Gesture::Flinch => {
                pos += Vec3::new(0.03 * s, -0.08, 0.06) * e;
                rot *= Quat::from_rotation_x(-0.3 * e);
            }
            Gesture::Swap if arm.right => {
                pos.y -= 0.28 * e;
                rot *= Quat::from_rotation_x(-0.6 * e);
            }
            _ => {}
        }
        tf.translation = pos;
        tf.rotation = rot;
    }
    for (h, mut v) in &mut items {
        let held = if h.right { held_r } else { held_l };
        *v = if held == Some(h.kind) { Visibility::Inherited } else { Visibility::Hidden };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestures_start_and_end_at_rest() {
        for g in [Gesture::Reach, Gesture::Use, Gesture::Flinch, Gesture::Swap] {
            assert!(g.envelope(0.0) < 0.01);
            assert!(g.envelope(g.duration()) == 0.0);
            let peak = (0..100).map(|i| g.envelope(g.duration() * i as f32 / 100.0)).fold(0.0, f32::max);
            assert!(peak > 0.95, "{g:?} never reaches full pose");
        }
        assert_eq!(Gesture::None.envelope(0.1), 0.0);
    }

    #[test]
    fn flashlight_moves_to_left_hand() {
        let cfg = crate::data::ItemConfig::default();
        let mut inv = Inventory::default();
        inv.add(ItemKind::Flashlight, &cfg);
        inv.add(ItemKind::Bandage, &cfg);
        inv.selected = 0;
        inv.flashlight_on = true;
        assert_eq!(held_items(&inv), (Some(ItemKind::Flashlight), None));
        inv.selected = 1;
        assert_eq!(held_items(&inv), (Some(ItemKind::Bandage), Some(ItemKind::Flashlight)));
        inv.flashlight_on = false;
        assert_eq!(held_items(&inv), (Some(ItemKind::Bandage), None));
    }
}
