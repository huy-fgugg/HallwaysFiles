//! "The Hollow" — HALLWAYS' first original entity.
//!
//! Design: a stooped, 2.4 m figure of layered black tatters with impossibly
//! long arms and a cracked porcelain mask whose two thin eye-slits burn amber.
//! It rises out of a spreading pool of ink when the lights turn red.
//!
//! Cue → counter (learnable rule): **lights turn red + low rumble** → it is
//! coming down the hall. **Counter: break line of sight and hide in a
//! wardrobe.** It searches outside for a few seconds, then sinks away.

use bevy::prelude::*;
use serde::Deserialize;

use crate::audio::{PlaySfx, Sfx};
use crate::cutscene::{CutsceneEnded, EndAction, PlayCutscene, THE_HOLLOW};
use crate::interaction::{HideState, HidingSpot};
use crate::player::Player;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};
use crate::world::Mood;

pub const HOLLOW_SPEED: f32 = 3.3;
pub const CATCH_RADIUS: f32 = 0.9;
pub const SEARCH_SECS: f32 = 3.0;
const SINK_SECS: f32 = 2.5;

/// Commands that cutscene timelines (or gameplay) can send to The Hollow.
#[derive(Message, Debug, Clone, Deserialize)]
pub enum HollowCommand {
    /// Spawn (if needed) at `at` and rise from the ink over `seconds`.
    Emerge { at: (f32, f32, f32), seconds: f32 },
    /// Light the eyes.
    Ignite,
    /// Turn to face a world point.
    Face((f32, f32, f32)),
}

impl HollowCommand {
    /// Version of this command that completes immediately (used when skipping).
    pub fn instant(&self) -> Self {
        match self {
            HollowCommand::Emerge { at, .. } => HollowCommand::Emerge { at: *at, seconds: 0.0 },
            other => other.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ChasePhase {
    #[default]
    Dormant,
    Hunting,
    Searching(f32),
    Sinking(f32),
    Done,
}

#[derive(Resource, Debug, Default)]
pub struct ChaseState {
    pub active: bool,
    pub phase: ChasePhase,
    pub caught: bool,
}

/// Explains the last death on the death screen.
#[derive(Resource, Debug, Default, Clone)]
pub struct DeathCause {
    pub title: String,
    pub cause: String,
    pub lesson: String,
}

#[derive(Component, Debug)]
pub struct Hollow {
    /// 0 = beneath the floor, 1 = fully risen.
    pub emerge: f32,
    pub emerge_rate: f32,
    pub eyes: f32,
    pub eyes_target: f32,
    pub facing: f32,
    pool: Entity,
}

#[derive(Component)]
struct HollowBody;
#[derive(Component)]
struct HollowArm(f32);
#[derive(Component)]
struct HollowTatter(f32);
#[derive(Component)]
struct HollowEyes;
#[derive(Component)]
struct InkPool;

pub struct HollowPlugin;

impl Plugin for HollowPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<HollowCommand>()
            .init_resource::<ChaseState>()
            .init_resource::<DeathCause>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(ChaseState::default()))
            .add_systems(
                Update,
                (
                    trigger_encounter.run_if(gameplay_active),
                    handle_commands,
                    on_cutscene_end,
                    chase.run_if(gameplay_active),
                    animate_hollow,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Walking deep into the corridor (Room 001) wakes The Hollow once per run.
fn trigger_encounter(
    player: Query<&Transform, With<Player>>,
    chase: Res<ChaseState>,
    active: Res<crate::cutscene::ActiveCutscene>,
    mut play: MessageWriter<PlayCutscene>,
    mut fired: Local<bool>,
    run: Res<RunState>,
) {
    if run.elapsed < 0.2 {
        *fired = false;
    }
    let Ok(p) = player.single() else { return };
    if !*fired && chase.phase == ChasePhase::Dormant && !active.is_playing() && p.translation.z < -7.5 {
        *fired = true;
        play.write(PlayCutscene(THE_HOLLOW.into()));
    }
}

fn spawn_hollow(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, at: Vec3) -> Entity {
    // Wet ink: near-black with a tight highlight, low environment reflection
    // so it doesn't read as grey under the ambient light.
    let ink = mats.add(StandardMaterial {
        base_color: Color::srgb(0.004, 0.003, 0.004),
        perceptual_roughness: 0.12,
        reflectance: 0.25,
        clearcoat: 0.4,
        clearcoat_perceptual_roughness: 0.1,
        ..default()
    });
    let cloth = mats.add(StandardMaterial { base_color: Color::srgb(0.03, 0.028, 0.03), perceptual_roughness: 0.9, ..default() });
    let skin = mats.add(StandardMaterial { base_color: Color::srgb(0.07, 0.065, 0.07), perceptual_roughness: 0.35, clearcoat: 0.6, ..default() });
    let porcelain = mats.add(StandardMaterial { base_color: Color::srgb(0.82, 0.8, 0.76), perceptual_roughness: 0.25, clearcoat: 0.8, ..default() });
    let crack = mats.add(StandardMaterial { base_color: Color::srgb(0.05, 0.04, 0.04), unlit: true, ..default() });
    let eyes = mats.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::BLACK, unlit: false, ..default() });

    let pool = commands
        .spawn((
            Mesh3d(meshes.add(Cylinder::new(1.0, 0.012).mesh().resolution(48))),
            MeshMaterial3d(ink.clone()),
            Transform::from_translation(at + Vec3::Y * 0.008).with_scale(Vec3::new(0.001, 1.0, 0.001)),
            InkPool,
            DespawnOnExit(AppState::Playing),
            Name::new("Ink pool"),
        ))
        .id();

    let capsule = |r: f32, len: f32| Capsule3d::new(r, len);
    let root = commands
        .spawn((
            Transform::from_translation(at - Vec3::Y * 2.6),
            Visibility::default(),
            Hollow { emerge: 0.0, emerge_rate: 0.0, eyes: 0.0, eyes_target: 0.0, facing: 0.0, pool },
            DespawnOnExit(AppState::Playing),
            Name::new("The Hollow"),
        ))
        .id();

    commands.entity(root).with_children(|r| {
        // Body pivots at the hips so it can stoop and sway.
        r.spawn((Transform::from_xyz(0.0, 1.05, 0.0), Visibility::default(), HollowBody)).with_children(|b| {
            // Torso: a long, hunched column of ink.
            b.spawn((Mesh3d(meshes.add(capsule(0.2, 0.9))), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 0.55, -0.05).with_rotation(Quat::from_rotation_x(0.35))));
            b.spawn((Mesh3d(meshes.add(Sphere::new(0.26).mesh().uv(24, 16))), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 1.0, 0.12).with_scale(Vec3::new(1.4, 0.8, 1.0))));
            // Neck and head pushed forward.
            b.spawn((Mesh3d(meshes.add(capsule(0.06, 0.25))), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 1.17, 0.26).with_rotation(Quat::from_rotation_x(0.9))));
            b.spawn((Transform::from_xyz(0.0, 1.28, 0.38), Visibility::default())).with_children(|h| {
                // Cracked porcelain mask: elongated oval, slightly concave look via two layers.
                h.spawn((Mesh3d(meshes.add(Sphere::new(0.15).mesh().uv(32, 20))), MeshMaterial3d(porcelain.clone()), Transform::from_scale(Vec3::new(0.85, 1.3, 0.55))));
                h.spawn((Mesh3d(meshes.add(Sphere::new(0.16).mesh().uv(24, 16))), MeshMaterial3d(cloth.clone()), Transform::from_xyz(0.0, 0.02, -0.06).with_scale(Vec3::new(1.0, 1.35, 0.8))));
                // Eye slits (emissive when ignited).
                for x in [-0.045f32, 0.045] {
                    h.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.018, 0.075, 0.01))),
                        MeshMaterial3d(eyes.clone()),
                        Transform::from_xyz(x, 0.03, 0.081).with_rotation(Quat::from_rotation_z(x.signum() * -0.12)),
                        HollowEyes,
                    ));
                }
                // Cracks across the mask.
                for (x, y, rz, len) in [(0.02f32, 0.1f32, 0.5f32, 0.09f32), (-0.03, -0.07, -0.3, 0.11), (0.05, -0.1, 0.9, 0.06)] {
                    h.spawn((Mesh3d(meshes.add(Cuboid::new(0.004, len, 0.006))), MeshMaterial3d(crack.clone()), Transform::from_xyz(x, y, 0.078).with_rotation(Quat::from_rotation_z(rz))));
                }
            });
            // Arms: shoulder pivots with long upper/fore arms and splayed fingers that nearly drag.
            for side in [-1.0f32, 1.0] {
                b.spawn((Transform::from_xyz(side * 0.34, 1.02, 0.1), Visibility::default(), HollowArm(side))).with_children(|a| {
                    a.spawn((Mesh3d(meshes.add(capsule(0.045, 0.62))), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, -0.36, 0.0)));
                    a.spawn((Mesh3d(meshes.add(capsule(0.035, 0.7))), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, -1.08, 0.08).with_rotation(Quat::from_rotation_x(-0.15))));
                    for f in 0..4 {
                        let spread = (f as f32 - 1.5) * 0.035;
                        a.spawn((
                            Mesh3d(meshes.add(capsule(0.009, 0.24))),
                            MeshMaterial3d(skin.clone()),
                            Transform::from_xyz(spread, -1.6, 0.14).with_rotation(Quat::from_rotation_z(spread * 3.0) * Quat::from_rotation_x(-0.25)),
                        ));
                    }
                });
            }
            // Hanging tatters around the torso.
            for i in 0..14 {
                let a = i as f32 / 14.0 * std::f32::consts::TAU;
                let len = 0.7 + ((i * 7) % 5) as f32 * 0.12;
                b.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.09, len, 0.01))),
                    MeshMaterial3d(cloth.clone()),
                    Transform::from_xyz(a.sin() * 0.24, 0.3 - len / 2.0 + 0.4, a.cos() * 0.2).with_rotation(Quat::from_rotation_y(a)),
                    HollowTatter(a),
                ));
            }
        });
        // Lower body: a ragged robe of floor-length strips over a thin core,
        // trailing into the ink.
        r.spawn((Mesh3d(meshes.add(capsule(0.17, 0.75))), MeshMaterial3d(cloth.clone()), Transform::from_xyz(0.0, 0.75, -0.02)));
        r.spawn((Mesh3d(meshes.add(Cone { radius: 0.22, height: 0.6 })), MeshMaterial3d(ink.clone()), Transform::from_xyz(0.0, 0.3, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::PI))));
        for i in 0..18 {
            let a = i as f32 / 18.0 * std::f32::consts::TAU + 0.17;
            let len = 1.05 + ((i * 5) % 7) as f32 * 0.05;
            let width = 0.07 + ((i * 3) % 4) as f32 * 0.015;
            let lean = 0.08 + ((i * 11) % 5) as f32 * 0.03;
            r.spawn((
                Mesh3d(meshes.add(Cuboid::new(width, len, 0.012))),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(a.sin() * 0.24, 1.15 - len / 2.0, a.cos() * 0.2)
                    .with_rotation(Quat::from_rotation_y(a) * Quat::from_rotation_x(-lean)),
                HollowTatter(a),
            ));
        }
    });
    root
}

fn handle_commands(
    mut commands: Commands,
    mut reader: MessageReader<HollowCommand>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut hollows: Query<(&mut Hollow, &mut Transform)>,
    mut pending: Local<Vec<HollowCommand>>,
) {
    pending.extend(reader.read().cloned());
    if pending.is_empty() {
        return;
    }
    let cmds: Vec<_> = pending.drain(..).collect();
    for cmd in cmds {
        match cmd {
            HollowCommand::Emerge { at, seconds } => {
                let at = Vec3::from(at);
                if hollows.is_empty() {
                    spawn_hollow(&mut commands, &mut meshes, &mut mats, at);
                    // The new entity isn't queryable until commands apply; retry next frame.
                    pending.push(HollowCommand::Emerge { at: at.into(), seconds });
                    return;
                }
                for (mut h, _) in &mut hollows {
                    if seconds <= 0.0 {
                        h.emerge = 1.0;
                        h.emerge_rate = 0.0;
                    } else {
                        h.emerge_rate = 1.0 / seconds;
                    }
                }
            }
            HollowCommand::Ignite => {
                if hollows.is_empty() {
                    pending.push(HollowCommand::Ignite);
                    return;
                }
                for (mut h, _) in &mut hollows {
                    h.eyes_target = 1.0;
                }
            }
            HollowCommand::Face(p) => {
                for (mut h, tf) in &mut hollows {
                    let d = Vec3::from(p) - tf.translation;
                    h.facing = d.x.atan2(d.z);
                }
            }
        }
    }
}

fn on_cutscene_end(
    mut reader: MessageReader<CutsceneEnded>,
    mut chase: ResMut<ChaseState>,
    mut subs: MessageWriter<Subtitle>,
) {
    for e in reader.read() {
        if e.end == EndAction::StartChase {
            chase.active = true;
            chase.phase = ChasePhase::Hunting;
            subs.write(Subtitle::danger("[It's coming. Hide!]"));
        }
    }
}

/// Picks where The Hollow should head next: through the doorway between the
/// corridor (z < -5.1) and Room 000 when the target is on the other side.
pub fn next_waypoint(hollow: Vec3, target: Vec3) -> Vec3 {
    let door_room = Vec3::new(0.0, 0.0, -4.4);
    let door_corr = Vec3::new(0.0, 0.0, -5.9);
    let in_corr = |p: Vec3| p.z < -5.1;
    match (in_corr(hollow), in_corr(target)) {
        (true, false) => {
            if hollow.distance(door_corr) > 0.4 && hollow.z < -5.9 { door_corr } else { door_room }
        }
        (false, true) => {
            if hollow.distance(door_room) > 0.4 && hollow.z > -4.4 { door_room } else { door_corr }
        }
        _ => target,
    }
}

#[allow(clippy::too_many_arguments)]
fn chase(
    time: Res<Time>,
    mut chase: ResMut<ChaseState>,
    hide: Res<HideState>,
    spots: Query<&HidingSpot>,
    player: Query<&Transform, (With<Player>, Without<Hollow>)>,
    mut hollows: Query<(&mut Hollow, &mut Transform)>,
    mut mood: ResMut<Mood>,
    mut shake: ResMut<CameraShake>,
    mut run: ResMut<RunState>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut death: ResMut<DeathCause>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    if !chase.active {
        return;
    }
    let dt = time.delta_secs();
    let Ok(p) = player.single() else { return };
    let Ok((mut h, mut tf)) = hollows.single_mut() else { return };
    let hidden = hide.is_hiding();
    let spot_front = hide.spot.and_then(|s| spots.get(s).ok()).map(|s| s.exit);

    match chase.phase {
        ChasePhase::Hunting => {
            let goal = if hidden { spot_front.unwrap_or(p.translation) } else { p.translation };
            let wp = next_waypoint(tf.translation, goal);
            let mut d = wp - tf.translation;
            d.y = 0.0;
            let dist_goal = Vec3::new(goal.x - tf.translation.x, 0.0, goal.z - tf.translation.z).length();
            if d.length() > 0.05 {
                let step = d.normalize() * HOLLOW_SPEED * dt;
                tf.translation += if step.length() > d.length() { d } else { step };
                h.facing = d.x.atan2(d.z);
            }
            if !hidden && dist_goal < CATCH_RADIUS {
                chase.caught = true;
                chase.active = false;
                shake.add(1.0);
                sfx.write(PlaySfx(Sfx::Stinger));
                *death = DeathCause {
                    title: "THE HOLLOW FOUND YOU".into(),
                    cause: format!("Caught in Room {:03} after the lights turned red.", run.current_room),
                    lesson: "Red lights and a low rumble mean The Hollow is coming. Break line of sight and hide in a wardrobe; it can't see inside.".into(),
                };
                next_overlay.set(Overlay::Dead);
            } else if hidden && dist_goal < 0.6 {
                chase.phase = ChasePhase::Searching(0.0);
                subs.write(Subtitle::danger("[Something scrapes against the wardrobe door...]"));
                sfx.write(PlaySfx(Sfx::Ooze));
                shake.add(0.25);
            }
        }
        ChasePhase::Searching(t) => {
            if !hidden {
                chase.phase = ChasePhase::Hunting;
                return;
            }
            let t = t + dt;
            if let Some(s) = hide.spot.and_then(|s| spots.get(s).ok()) {
                let d = s.inside - tf.translation;
                h.facing = d.x.atan2(d.z);
            }
            chase.phase = if t >= SEARCH_SECS {
                sfx.write(PlaySfx(Sfx::Ooze));
                ChasePhase::Sinking(0.0)
            } else {
                ChasePhase::Searching(t)
            };
        }
        ChasePhase::Sinking(t) => {
            let t = t + dt;
            h.emerge = (1.0 - t / SINK_SECS).max(0.0);
            h.emerge_rate = 0.0;
            h.eyes_target = 0.0;
            if t >= SINK_SECS {
                chase.phase = ChasePhase::Done;
                chase.active = false;
                *mood = Mood::default();
                run.lumens += 10;
                sfx.write(PlaySfx(Sfx::Pickup));
                subs.write(Subtitle { text: "It sank away. You survived The Hollow (+10 Lumens). End of the Phase 1 test floor.".into(), seconds: 6.0, danger: false });
            }
        }
        _ => {}
    }
}

fn animate_hollow(
    time: Res<Time>,
    mut hollows: Query<(&mut Hollow, &mut Transform)>,
    mut pools: Query<&mut Transform, (With<InkPool>, Without<Hollow>, Without<HollowBody>, Without<HollowArm>, Without<HollowTatter>)>,
    mut bodies: Query<&mut Transform, (With<HollowBody>, Without<Hollow>, Without<HollowArm>, Without<HollowTatter>)>,
    mut arms: Query<(&HollowArm, &mut Transform), (Without<Hollow>, Without<HollowBody>, Without<HollowTatter>)>,
    mut tatters: Query<(&HollowTatter, &mut Transform), (Without<Hollow>, Without<HollowBody>, Without<HollowArm>)>,
    eyes: Query<&MeshMaterial3d<StandardMaterial>, With<HollowEyes>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut h, mut tf) in &mut hollows {
        h.emerge = (h.emerge + h.emerge_rate * dt).clamp(0.0, 1.0);
        h.eyes += (h.eyes_target - h.eyes) * (dt * 6.0).min(1.0);
        let e = h.emerge;
        // Pool spreads first, the body rises after.
        let rise = ((e - 0.25) / 0.75).clamp(0.0, 1.0);
        let rise = rise * rise * (3.0 - 2.0 * rise);
        let bob = (t * 1.3).sin() * 0.04 * rise;
        tf.translation.y = -2.6 * (1.0 - rise) + bob;
        let target_rot = Quat::from_rotation_y(h.facing);
        tf.rotation = tf.rotation.slerp(target_rot, (dt * 5.0).min(1.0));
        if let Ok(mut pool) = pools.get_mut(h.pool) {
            let r = (e / 0.3).clamp(0.0, 1.0) * 1.1 + (t * 2.0).sin() * 0.02 * e;
            let r = r.max(0.001);
            pool.scale = Vec3::new(r, 1.0, r * 0.85);
            pool.translation.x = tf.translation.x;
            pool.translation.z = tf.translation.z;
        }
        for mat in &eyes {
            if let Some(mut m) = mats.get_mut(&mat.0) {
                let pulse = 0.85 + 0.15 * (t * 9.0).sin();
                m.emissive = LinearRgba::rgb(9.0, 4.2, 0.8) * (h.eyes * pulse);
            }
        }
    }
    for mut b in &mut bodies {
        b.rotation = Quat::from_rotation_x(0.25 + (t * 0.9).sin() * 0.05) * Quat::from_rotation_z((t * 0.7).sin() * 0.06);
    }
    for (arm, mut a) in &mut arms {
        a.rotation = Quat::from_rotation_x(-0.15 + (t * 1.1 + arm.0).sin() * 0.12) * Quat::from_rotation_z(arm.0 * (0.08 + (t * 0.8).sin() * 0.04));
    }
    for (tat, mut tr) in &mut tatters {
        tr.rotation = Quat::from_rotation_y(tat.0) * Quat::from_rotation_x((t * 2.1 + tat.0 * 3.0).sin() * 0.15);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoint_routes_through_door() {
        // Hollow deep in the corridor, player in Room 000 → head for the corridor side of the door.
        assert_eq!(next_waypoint(Vec3::new(0.5, 0.0, -12.0), Vec3::new(3.0, 0.0, 1.0)), Vec3::new(0.0, 0.0, -5.9));
        // At the door → step into the room.
        assert_eq!(next_waypoint(Vec3::new(0.0, 0.0, -5.9), Vec3::new(3.0, 0.0, 1.0)), Vec3::new(0.0, 0.0, -4.4));
        // Same room → straight at the target.
        let target = Vec3::new(3.0, 0.0, 1.0);
        assert_eq!(next_waypoint(Vec3::new(0.0, 0.0, -3.0), target), target);
    }

    #[test]
    fn skipping_makes_emerge_instant() {
        let c = HollowCommand::Emerge { at: (0.0, 0.0, 0.0), seconds: 7.0 }.instant();
        assert!(matches!(c, HollowCommand::Emerge { seconds, .. } if seconds == 0.0));
    }

    #[test]
    fn hollow_is_slower_than_sprint() {
        assert!(HOLLOW_SPEED < crate::player::SPRINT_SPEED);
        assert!(HOLLOW_SPEED > crate::player::WALK_SPEED);
    }
}
