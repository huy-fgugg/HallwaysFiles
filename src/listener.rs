//! The Listener: a blind, root-and-bark figure that patrols the outdoor
//! gardens. It cannot see you; it hears footsteps. Walking is heard nearby,
//! sprinting from much farther. Crouching (slower) or standing still is
//! silent, and a shed hides you completely. If it walks into you, the run
//! ends.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{Encounter, RoomPlan};
use crate::floor::{Floor, FloorRuntime};
use crate::interaction::HideState;
use crate::player::Player;
use crate::rng::GameRng;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};

/// How far footsteps carry (metres).
pub const WALK_HEARD: f32 = 7.5;
pub const SPRINT_HEARD: f32 = 13.0;
const PATROL_SPEED: f32 = 1.2;
const HUNT_SPEED: f32 = 2.9;
const CATCH: f32 = 1.0;

/// Pure rule: how far the player's current movement can be heard.
pub fn noise_radius(speed: f32, crouching: bool, sprinting: bool, hiding: bool) -> f32 {
    if hiding || crouching || speed < 0.4 {
        0.0
    } else if sprinting {
        SPRINT_HEARD
    } else {
        WALK_HEARD
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Patrol,
    /// Heading for the last place it heard you.
    Hunt,
    /// Standing still, head cocked, listening.
    Listen(f32),
}

#[derive(Component)]
pub struct Listener {
    room: u32,
    mode: Mode,
    target: Vec3,
    phase: f32,
    step: f32,
    rng: GameRng,
    heard_cooldown: f32,
}

#[derive(Component, Clone, Copy)]
enum Part {
    Head,
    Arm(bool),
    Leg(bool),
    Torso,
}

#[derive(Resource, Default)]
struct HintShown(bool);

pub struct ListenerPlugin;

impl Plugin for ListenerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HintShown>().add_systems(
            Update,
            (spawn_when_entering, (think, animate).chain().run_if(gameplay_active), despawn_when_left).run_if(in_state(AppState::Playing)),
        );
    }
}

fn spawn_when_entering(
    mut commands: Commands,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    mut rt: Option<ResMut<FloorRuntime>>,
    force: Res<crate::encounters::ForceEncounter>,
    player: Query<&Transform, With<Player>>,
    existing: Query<&Listener>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut subs: MessageWriter<Subtitle>,
    mut sfx: MessageWriter<PlaySfx>,
    mut hint: ResMut<HintShown>,
) {
    let (Some(floor), Some(rt)) = (floor, rt.as_mut()) else { return };
    let Some(r) = floor.0.room(run.current_room) else { return };
    let forced = matches!(force.0, Some((room, Encounter::Listener)) if room == r.index);
    if !(r.encounter == Some(Encounter::Listener) || forced) || rt.encounters_done.contains(&r.index) || existing.iter().any(|l| l.room == r.index) {
        return;
    }
    let Ok(p) = player.single() else { return };
    // Wait until the player is properly inside.
    if p.translation.z > r.max.y - 1.2 {
        return;
    }
    rt.encounters_done.insert(r.index);
    let mut rng = GameRng::new(r.seed ^ 0x11_57E2);
    // Starts on the far side, away from the player.
    let start = Vec3::new(rng.range_f32(r.min.x + 1.5, r.max.x - 1.5), 0.0, r.min.y + 2.0);
    spawn_listener(&mut commands, &mut meshes, &mut mats, r, start, rng);
    sfx.write(PlaySfx(Sfx::Rumble));
    subs.write(Subtitle::danger("[Slow, heavy footsteps. Something out there is listening.]"));
    if !hint.0 {
        hint.0 = true;
        subs.write(Subtitle::info("It's blind but hears footsteps. Hold Ctrl to crouch and move silently, or hide in a shed."));
    }
}

fn spawn_listener(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, r: &RoomPlan, at: Vec3, rng: GameRng) {
    let m = |c: Color, rough: f32| StandardMaterial { base_color: c, perceptual_roughness: rough, ..default() };
    let bark = mats.add(m(Color::srgb(0.16, 0.11, 0.08), 0.95));
    let pale = mats.add(m(Color::srgb(0.72, 0.68, 0.6), 0.6));
    let moss = mats.add(m(Color::srgb(0.13, 0.2, 0.08), 1.0));
    let fungus = mats.add(m(Color::srgb(0.55, 0.42, 0.3), 0.8));
    let glow = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.2), emissive: LinearRgba::rgb(4.0, 1.8, 0.4), ..default() });
    let cyl = meshes.add(Cylinder::new(0.5, 1.0));
    let cap = meshes.add(Capsule3d::new(0.5, 1.0));
    let sph = meshes.add(Sphere::new(0.5).mesh().uv(16, 10));
    let cone = meshes.add(Cone { radius: 0.5, height: 1.0 });
    let piece = |c: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, pos: Vec3, rot: Quat, size: Vec3| {
        c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(pos).with_rotation(rot).with_scale(size), NotShadowCaster));
    };
    commands
        .spawn((
            Listener { room: r.index, mode: Mode::Patrol, target: at, phase: 0.0, step: 0.0, rng, heard_cooldown: 0.0 },
            Transform::from_translation(at),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Name::new("The Listener"),
        ))
        .with_children(|root| {
            // Long root-legs with knotted knees.
            for right in [true, false] {
                let x = if right { 0.16 } else { -0.16 };
                root.spawn((Part::Leg(right), Transform::from_xyz(x, 1.35, 0.0), Visibility::default())).with_children(|leg| {
                    piece(leg, &cyl, &bark, Vec3::new(0.0, -0.6, 0.0), Quat::IDENTITY, Vec3::new(0.12, 1.2, 0.12));
                    piece(leg, &sph, &bark, Vec3::new(0.0, -0.62, 0.02), Quat::IDENTITY, Vec3::splat(0.17));
                    // Thin roots winding down the leg.
                    for k in 0..4 {
                        let a = k as f32 * 1.6;
                        piece(leg, &cyl, &bark, Vec3::new(a.sin() * 0.055, -0.35 - k as f32 * 0.18, a.cos() * 0.055), Quat::from_rotation_y(a) * Quat::from_rotation_z(0.5), Vec3::new(0.025, 0.4, 0.025));
                    }
                    // Splayed root toes.
                    for a in [-0.6f32, 0.0, 0.6] {
                        piece(leg, &cone, &bark, Vec3::new(a.sin() * 0.12, -1.28, -0.12 * a.cos()), Quat::from_rotation_x(-1.3) * Quat::from_rotation_z(a), Vec3::new(0.07, 0.3, 0.07));
                    }
                });
            }
            root.spawn((Part::Torso, Transform::from_xyz(0.0, 1.35, 0.0), Visibility::default())).with_children(|torso| {
                // Gaunt, forward-leaning trunk wound with roots and moss.
                piece(torso, &cap, &bark, Vec3::new(0.0, 0.55, -0.05), Quat::from_rotation_x(0.18), Vec3::new(0.34, 0.5, 0.26));
                for (y, a) in [(0.3f32, 0.4f32), (0.6, -0.5), (0.85, 0.2)] {
                    piece(torso, &cyl, &bark, Vec3::new(0.0, y, -0.05), Quat::from_rotation_z(a) * Quat::from_rotation_x(0.18), Vec3::new(0.38, 0.05, 0.3));
                }
                piece(torso, &sph, &moss, Vec3::new(0.1, 0.9, -0.15), Quat::IDENTITY, Vec3::new(0.25, 0.16, 0.18));
                piece(torso, &sph, &moss, Vec3::new(-0.12, 0.4, 0.1), Quat::IDENTITY, Vec3::new(0.18, 0.12, 0.14));
                // A cage of roots over the chest, like ribs.
                for k in 0..4 {
                    let y = 0.45 + k as f32 * 0.12;
                    for sd in [-1.0f32, 1.0] {
                        piece(torso, &cyl, &bark, Vec3::new(sd * 0.1, y, -0.2), Quat::from_rotation_z(sd * 1.2) * Quat::from_rotation_x(0.18), Vec3::new(0.022, 0.22, 0.022));
                    }
                }
                // Shelf fungus on the shoulders and moss hanging off them.
                for sd in [-1.0f32, 1.0] {
                    piece(torso, &sph, &fungus, Vec3::new(sd * 0.27, 0.95, 0.0), Quat::from_rotation_z(sd * 0.4), Vec3::new(0.16, 0.04, 0.13));
                    for k in 0..3 {
                        piece(torso, &cyl, &moss, Vec3::new(sd * (0.2 + k as f32 * 0.04), 0.72 - k as f32 * 0.04, -0.05 + k as f32 * 0.05), Quat::from_rotation_z(sd * 0.1), Vec3::new(0.02, 0.35 + k as f32 * 0.08, 0.02));
                    }
                }
                // Head: a smooth pale knot with no eyes, a glowing split for a
                // mouth, shelf-fungus "ears" and a crown of twigs.
                torso.spawn((Part::Head, Transform::from_xyz(0.0, 1.18, -0.16), Visibility::default())).with_children(|h| {
                    piece(h, &sph, &pale, Vec3::new(0.0, 0.18, 0.0), Quat::IDENTITY, Vec3::new(0.3, 0.46, 0.32));
                    piece(h, &cyl, &glow, Vec3::new(0.0, 0.08, -0.155), Quat::IDENTITY, Vec3::new(0.025, 0.22, 0.01));
                    // A faint ember glow leaking from the split.
                    h.spawn((PointLight { color: Color::srgb(1.0, 0.55, 0.2), intensity: 6_000.0, range: 2.5, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 0.08, -0.25)));
                    // Bark knots on the pale skull.
                    for (x, y) in [(-0.09f32, 0.32f32), (0.07, 0.36), (0.11, 0.05)] {
                        piece(h, &sph, &bark, Vec3::new(x, y, -0.1), Quat::IDENTITY, Vec3::splat(0.05));
                    }
                    for s in [-1.0f32, 1.0] {
                        piece(h, &sph, &fungus, Vec3::new(s * 0.2, 0.2, 0.02), Quat::from_rotation_z(s * 0.3), Vec3::new(0.26, 0.06, 0.2));
                        piece(h, &sph, &fungus, Vec3::new(s * 0.18, 0.08, 0.04), Quat::from_rotation_z(s * 0.2), Vec3::new(0.18, 0.05, 0.14));
                    }
                    for (a, tilt) in [(-0.5f32, 0.4f32), (-0.15, 0.25), (0.2, 0.35), (0.5, 0.5), (0.0, 0.6)] {
                        piece(h, &cyl, &bark, Vec3::new(a * 0.25, 0.48, 0.05), Quat::from_rotation_z(-a) * Quat::from_rotation_x(tilt), Vec3::new(0.025, 0.42, 0.025));
                    }
                });
                // Arms that hang past the knees, ending in branch fingers.
                for right in [true, false] {
                    let x = if right { 0.26 } else { -0.26 };
                    torso.spawn((Part::Arm(right), Transform::from_xyz(x, 0.95, -0.08), Visibility::default())).with_children(|arm| {
                        piece(arm, &cyl, &bark, Vec3::new(0.0, -0.4, 0.0), Quat::IDENTITY, Vec3::new(0.08, 0.8, 0.08));
                        piece(arm, &sph, &bark, Vec3::new(0.0, -0.8, 0.0), Quat::IDENTITY, Vec3::splat(0.11));
                        piece(arm, &cyl, &bark, Vec3::new(0.0, -1.15, -0.04), Quat::from_rotation_x(0.15), Vec3::new(0.065, 0.7, 0.065));
                        // Five knotted branch fingers.
                        for a in [-0.5f32, -0.25, 0.0, 0.25, 0.5] {
                            piece(arm, &sph, &bark, Vec3::new(a * 0.09, -1.5, -0.05), Quat::IDENTITY, Vec3::splat(0.045));
                            piece(arm, &cone, &pale, Vec3::new(a * 0.14, -1.66, -0.06), Quat::from_rotation_x(std::f32::consts::PI) * Quat::from_rotation_z(a), Vec3::new(0.03, 0.34, 0.03));
                        }
                        // Moss trailing from the wrist.
                        piece(arm, &cyl, &moss, Vec3::new(0.03, -1.35, 0.03), Quat::IDENTITY, Vec3::new(0.018, 0.3, 0.018));
                    });
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn think(
    time: Res<Time>,
    floor: Option<Res<Floor>>,
    mut listeners: Query<(&mut Listener, &mut Transform), Without<Player>>,
    player: Query<(&Player, &Transform), Without<Listener>>,
    hide: Res<HideState>,
    mut run: ResMut<RunState>,
    mut death: ResMut<crate::hollow::DeathCause>,
    mut next: ResMut<NextState<Overlay>>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut shake: ResMut<CameraShake>,
    capture: Local<Option<bool>>,
) {
    let Some(floor) = floor else { return };
    let Ok((pl, ptf)) = player.single() else { return };
    let dt = time.delta_secs();
    let capture = capture.unwrap_or_else(|| std::env::var_os("HALLWAYS_CAPTURE").is_some());
    let hiding = hide.is_hiding();
    let ppos = Vec3::new(ptf.translation.x, 0.0, ptf.translation.z);
    for (mut l, mut tf) in &mut listeners {
        let Some(r) = floor.0.room(l.room) else { continue };
        let pos = Vec3::new(tf.translation.x, 0.0, tf.translation.z);
        let dist = pos.distance(ppos);
        l.heard_cooldown = (l.heard_cooldown - dt).max(0.0);
        // Hearing (only while you're in its garden).
        let in_room = ptf.translation.z <= r.max.y + 0.2 && ptf.translation.z >= r.min.y - 0.3;
        let radius = noise_radius(pl.speed, pl.crouching, pl.sprinting, hiding);
        if in_room && dist < radius {
            l.target = ppos;
            if l.mode != Mode::Hunt && l.heard_cooldown <= 0.0 {
                sfx.write(PlaySfx(Sfx::Whisper));
                subs.write(Subtitle::danger("[It heard you. It's coming.]"));
                shake.add(0.25);
                l.heard_cooldown = 4.0;
            }
            l.mode = Mode::Hunt;
        }
        // Movement.
        let (speed, arrived) = match l.mode {
            Mode::Patrol => (PATROL_SPEED, pos.distance(l.target) < 0.4),
            Mode::Hunt => (HUNT_SPEED, pos.distance(l.target) < 0.5),
            Mode::Listen(_) => (0.0, false),
        };
        if arrived {
            l.mode = match l.mode {
                Mode::Hunt => Mode::Listen(2.2),
                _ => Mode::Patrol,
            };
            if l.mode == Mode::Patrol {
                let x = l.rng.range_f32(r.min.x + 1.0, r.max.x - 1.0);
                let z = l.rng.range_f32(r.min.y + 1.0, r.max.y - 1.0);
                l.target = Vec3::new(x, 0.0, z);
            }
        }
        if let Mode::Listen(t) = l.mode {
            let t = t - dt;
            l.mode = if t <= 0.0 { Mode::Patrol } else { Mode::Listen(t) };
            if l.mode == Mode::Patrol {
                let x = l.rng.range_f32(r.min.x + 1.0, r.max.x - 1.0);
                let z = l.rng.range_f32(r.min.y + 1.0, r.max.y - 1.0);
                l.target = Vec3::new(x, 0.0, z);
            }
        }
        let to = l.target - pos;
        if speed > 0.0 && to.length() > 0.05 {
            let step = to.normalize() * (speed * dt).min(to.length());
            let next_pos = pos + step;
            tf.translation.x = next_pos.x.clamp(r.min.x + 0.5, r.max.x - 0.5);
            tf.translation.z = next_pos.z.clamp(r.min.y + 0.5, r.max.y - 0.5);
            let want = (-to.x).atan2(-to.z);
            let cur = tf.rotation.to_euler(EulerRot::YXZ).0;
            let d = (want - cur + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            tf.rotation = Quat::from_rotation_y(cur + d * (dt * 4.0).min(1.0));
            l.phase += speed * dt * 2.2;
            l.step += speed * dt;
            if l.step > 1.1 {
                l.step = 0.0;
                if dist < 16.0 {
                    sfx.write(PlaySfx(Sfx::Footstep));
                }
            }
        }
        // It walks into you: game over. (The QA camera is exempt.)
        if dist < CATCH && !hiding && !capture && run.vitality > 0.0 {
            run.vitality = 0.0;
            shake.add(1.0);
            sfx.write(PlaySfx(Sfx::SurgeRoar));
            *death = crate::hollow::DeathCause {
                title: "THE LISTENER FOUND YOU".into(),
                cause: format!("It walked straight into you in Room {:03}.", l.room),
                lesson: "It can't see. Walking makes noise nearby and sprinting carries far. Hold Ctrl to crouch: slower, but silent. A shed hides you completely.".into(),
            };
            next.set(Overlay::Dead);
        }
    }
}

fn animate(time: Res<Time>, listeners: Query<(&Listener, &Children)>, children: Query<&Children>, mut parts: Query<(&Part, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (l, kids) in &listeners {
        let moving = !matches!(l.mode, Mode::Listen(_));
        let swing = if moving { l.phase.sin() } else { 0.0 };
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok(ch) = children.get(e) {
                stack.extend(ch.iter());
            }
            let Ok((part, mut tf)) = parts.get_mut(e) else { continue };
            match *part {
                Part::Leg(right) => tf.rotation = Quat::from_rotation_x(if right { swing } else { -swing } * 0.45),
                Part::Arm(right) => {
                    let s = if right { -swing } else { swing };
                    tf.rotation = Quat::from_rotation_x(s * 0.35 + 0.1) * Quat::from_rotation_z(if right { 0.08 } else { -0.08 });
                }
                Part::Torso => {
                    tf.translation.y = 1.35 + (l.phase * 2.0).sin().abs() * 0.04;
                    tf.rotation = Quat::from_rotation_z((l.phase).sin() * 0.05);
                }
                Part::Head => {
                    // Listening: the head cocks to one side and twitches.
                    tf.rotation = match l.mode {
                        Mode::Listen(_) => Quat::from_rotation_z(0.45 + (t * 9.0).sin() * 0.04) * Quat::from_rotation_y((t * 1.3).sin() * 0.5),
                        Mode::Hunt => Quat::from_rotation_x(0.2),
                        Mode::Patrol => Quat::from_rotation_y((t * 0.7).sin() * 0.4),
                    };
                }
            }
        }
    }
}

fn despawn_when_left(mut commands: Commands, run: Res<RunState>, listeners: Query<(Entity, &Listener)>) {
    for (e, l) in &listeners {
        if run.current_room > l.room + 1 || run.current_room + 2 < l.room {
            commands.entity(e).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hearing_rules() {
        assert_eq!(noise_radius(3.0, false, false, false), WALK_HEARD);
        assert_eq!(noise_radius(5.0, false, true, false), SPRINT_HEARD);
        assert_eq!(noise_radius(1.6, true, false, false), 0.0, "crouching is silent");
        assert_eq!(noise_radius(0.0, false, false, false), 0.0, "standing still is silent");
        assert_eq!(noise_radius(3.0, false, false, true), 0.0, "hidden in a shed");
    }

    #[test]
    fn listener_lives_in_the_gardens() {
        use crate::floor::plan::{generate, Area};
        let mut seen = 0;
        for seed in 0..10 {
            for r in generate(seed).rooms {
                if r.encounter == Some(Encounter::Listener) {
                    seen += 1;
                    assert!(matches!(r.area, Area::Grounds | Area::Archive), "{:?}", r.area);
                }
            }
        }
        assert!(seen >= 10, "the Listener should be common outdoors: {seen}");
    }
}
