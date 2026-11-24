//! "The Surge": HALLWAYS' rushing entity (original design).
//!
//! Look: a roaring knot of crackling white-blue static shards with a cold
//! light at its heart and streaks trailing behind it. No face, no body.
//!
//! Cue → counter: **the lights strobe and a roar builds in the walls** →
//! within a few seconds it tears through every room along the door path.
//! **Counter: hide in a wardrobe or locker until it has passed.** Surge rooms
//! always contain at least one hiding spot (enforced by the generator).

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{FloorPlan, WALL_T};
use crate::floor::{Floor, FloorRuntime};
use crate::hollow::DeathCause;
use crate::floor::plan::Encounter;
use crate::interaction::{HideState, ThreatActive};
use crate::player::Player;
use crate::rng::GameRng;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};
use crate::world::Mood;

pub const SURGE_SPEED: f32 = 22.0;
pub const SURGE_KILL_RADIUS: f32 = 1.8;

/// Warning time before it arrives: shorter deeper into the floor, never under 3.2 s.
pub fn cue_seconds(room: u32) -> f32 {
    (5.0 - 1.6 * room as f32 / 100.0).max(3.2)
}

#[derive(Debug, Clone, PartialEq)]
pub enum SurgePhase {
    Idle,
    Cue { room: u32, t: f32, undertow: bool },
    Rushing { room: u32, path: Vec<Vec3>, seg: usize, pos: Vec3, passed_close: bool, passes_left: u32, pause: f32, undertow: bool },
}

#[derive(Resource, Debug)]
pub struct SurgeState(pub SurgePhase);

impl Default for SurgeState {
    fn default() -> Self {
        Self(SurgePhase::Idle)
    }
}

/// The Surge/Undertow's visible body (cleared when the Door 40 chase starts).
#[derive(Component)]
pub struct SurgeBody;

#[derive(Component)]
struct SurgeFace;
#[derive(Component)]
struct SurgeJaw;
#[derive(Component)]
struct SurgeEye;

#[derive(Component)]
struct Shard {
    seed: f32,
    radius: f32,
    streak: bool,
}

/// The route The Surge flies: from three rooms behind to two rooms ahead,
/// through every doorway, at chest height.
pub fn surge_path(plan: &FloorPlan, room: u32) -> Vec<Vec3> {
    let first = room.saturating_sub(3).max(crate::floor::plan::FIRST_ROOM);
    let last = (room + 2).min(plan.last_room);
    let y = 1.4;
    let mut pts = vec![];
    if let Some(r) = plan.room(first) {
        pts.push(Vec3::new(r.entry_x, y, r.max.y + 3.0));
    }
    for i in first..=last {
        let Some(r) = plan.room(i) else { continue };
        pts.push(Vec3::new(r.entry_x, y, r.max.y - 0.6));
        pts.push(Vec3::new(r.exit_x, y, r.min.y + 0.6));
        pts.push(Vec3::new(r.exit_x, y, r.min.y - WALL_T - 0.4));
    }
    pts
}

/// The Undertow sweeps a shorter stretch (two rooms back to one ahead) so
/// its repeated passes stay near the player.
pub fn undertow_path(plan: &FloorPlan, room: u32) -> Vec<Vec3> {
    let mut p = surge_path(plan, room);
    let first = room.saturating_sub(2).max(crate::floor::plan::FIRST_ROOM);
    let last = (room + 1).min(plan.last_room);
    if let (Some(a), Some(b)) = (plan.room(first), plan.room(last)) {
        p.retain(|v| v.z <= a.max.y + 3.1 && v.z >= b.min.y - 1.0);
    }
    p
}

/// Moves `pos` along `path` by `dist`; returns (new pos, new segment index, finished).
pub fn advance(path: &[Vec3], mut seg: usize, mut pos: Vec3, mut dist: f32) -> (Vec3, usize, bool) {
    while seg + 1 < path.len() {
        let target = path[seg + 1];
        let to = target - pos;
        let len = to.length();
        if len > dist {
            return (pos + to / len * dist, seg, false);
        }
        dist -= len;
        pos = target;
        seg += 1;
    }
    (pos, seg, true)
}

pub struct SurgePlugin;

impl Plugin for SurgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurgeState>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(SurgeState::default()))
            .add_systems(Update, (trigger, run_surge).chain().run_if(gameplay_active))
            .add_systems(Update, (animate_shards, animate_face).run_if(in_state(AppState::Playing)));
    }
}

#[allow(clippy::too_many_arguments)]
fn trigger(
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    force: Res<crate::encounters::ForceEncounter>,
    mut state: ResMut<SurgeState>,
    mut mood: ResMut<Mood>,
    mut threat: ResMut<ThreatActive>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    stranger: Res<crate::stranger::StrangerThreat>,
) {
    threat.0 = state.0 != SurgePhase::Idle || stranger.0;
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    if state.0 != SurgePhase::Idle {
        return;
    }
    let Some(r) = floor.0.room(run.current_room) else { return };
    let enc = match force.0 {
        Some((room, e)) if room == r.index => Some(e),
        _ => r.encounter,
    };
    if rt.surges_done.contains(&r.index) {
        return;
    }
    match enc {
        Some(Encounter::Surge) => {
            state.0 = SurgePhase::Cue { room: r.index, t: 0.0, undertow: false };
            *mood = Mood { tint: Color::srgb(0.75, 0.88, 1.0), amount: 0.35, scale: 1.0, flicker: 0.85 };
            sfx.write(PlaySfx(Sfx::SurgeRoar));
            subs.write(Subtitle::danger("[The lights strobe. A roar builds in the walls.] HIDE!"));
        }
        Some(Encounter::Undertow) => {
            state.0 = SurgePhase::Cue { room: r.index, t: 0.0, undertow: true };
            *mood = Mood { tint: Color::srgb(0.45, 0.75, 0.7), amount: 0.45, scale: 0.9, flicker: 0.8 };
            sfx.write(PlaySfx(Sfx::SurgeRoar));
            subs.write(Subtitle::danger("[A low, dragging roar... the lights pulse green.] HIDE, and stay hidden!"));
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn run_surge(
    mut commands: Commands,
    time: Res<Time>,
    floor: Option<Res<Floor>>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut state: ResMut<SurgeState>,
    mut mood: ResMut<Mood>,
    hide: Res<HideState>,
    player: Query<&Transform, (With<Player>, Without<SurgeBody>)>,
    mut bodies: Query<(Entity, &mut Transform), With<SurgeBody>>,
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut run: ResMut<RunState>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    (mut death, mut next_overlay): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
    cfg: Res<crate::data::EntityConfig>,
) {
    let dt = time.delta_secs();
    let Some(floor) = floor else { return };
    match &mut state.0 {
        SurgePhase::Idle => {}
        SurgePhase::Cue { room, t, undertow } => {
            *t += dt;
            let tune = if *undertow { cfg.undertow } else { cfg.surge };
            if *t >= tune.warn_for(*room) {
                let path = if *undertow { undertow_path(&floor.0, *room) } else { surge_path(&floor.0, *room) };
                let start = path[0];
                spawn_surge(&mut commands, &mut meshes, &mut mats, start, *undertow);
                subs.write(Subtitle::danger(if *undertow { "[IT'S HERE... and it isn't leaving]" } else { "[IT'S HERE]" }));
                let passes = if *undertow { 2 + (*room % 2) } else { 0 };
                state.0 = SurgePhase::Rushing { room: *room, path, seg: 0, pos: start, passed_close: false, passes_left: passes, pause: 0.0, undertow: *undertow };
            }
        }
        SurgePhase::Rushing { room, path, seg, pos, passed_close, passes_left, pause, undertow } => {
            if *pause > 0.0 {
                *pause -= dt;
                return;
            }
            let (np, ns, done) = advance(path, *seg, *pos, SURGE_SPEED * dt);
            *pos = np;
            *seg = ns;
            for (_, mut tf) in &mut bodies {
                tf.translation = np;
            }
            if let Ok(p) = player.single() {
                let d = Vec2::new(p.translation.x - np.x, p.translation.z - np.z).length();
                if d < 5.0 && !*passed_close {
                    *passed_close = true;
                    shake.add(0.5);
                    sfx.write(PlaySfx(Sfx::Whoosh));
                }
                // It fills the room wall to wall as it tears through: standing
                // off to the side doesn't save you, only hiding does.
                let bounds = floor.0.room_at_z(np.z).and_then(|i| floor.0.room(i)).map(|r| (r.min.x, r.max.x));
                if caught(p.translation, np, bounds) && !hide.is_hiding() {
                    shake.add(1.0);
                    sfx.write(PlaySfx(Sfx::Stinger));
                    *death = if *undertow {
                        DeathCause {
                            title: "THE UNDERTOW DRAGGED YOU OUT".into(),
                            cause: format!("Caught between passes in Room {:03}.", run.current_room),
                            lesson: "The Undertow comes back again and again. Stay hidden until the green flicker stops completely; your hide timer pauses while it's out there.".into(),
                        }
                    } else {
                        DeathCause {
                            title: "THE SURGE TORE THROUGH".into(),
                            cause: format!("Caught in the open in Room {:03}.", run.current_room),
                            lesson: "When the lights strobe and a roar builds, hide in a wardrobe or locker right away and stay in until the lights settle.".into(),
                        }
                    };
                    for (e, _) in &bodies {
                        commands.entity(e).try_despawn();
                    }
                    *mood = Mood::default();
                    state.0 = SurgePhase::Idle;
                    next_overlay.set(Overlay::Dead);
                    return;
                }
            }
            if done && *passes_left > 0 {
                // Undertow: turn around and come back after a short, quieter pause.
                *passes_left -= 1;
                path.reverse();
                *seg = 0;
                *pos = path[0];
                *pause = 1.3;
                *passed_close = false;
                sfx.write(PlaySfx(Sfx::Rumble));
                subs.write(Subtitle::danger("[It's turning back...]"));
                return;
            }
            if done {
                let room = *room;
                for (e, _) in &bodies {
                    commands.entity(e).try_despawn();
                }
                *mood = Mood::default();
                if let Some(rt) = rt.as_mut() {
                    rt.surges_done.insert(room);
                }
                run.surges_survived += 1;
                run.lumens += 5;
                subs.write(Subtitle::info("[The roar fades. The lights settle.] (+5 Lumens)"));
                state.0 = SurgePhase::Idle;
            }
        }
    }
}

/// Pure rule: the Surge catches you if you're close to it, or anywhere across
/// the room's width where its front is passing (within 1.4 m along the room).
pub fn caught(player: Vec3, surge: Vec3, room_x: Option<(f32, f32)>) -> bool {
    let d = Vec2::new(player.x - surge.x, player.z - surge.z).length();
    if d < SURGE_KILL_RADIUS {
        return true;
    }
    match room_x {
        Some((x0, x1)) => (player.z - surge.z).abs() < 1.4 && player.x > x0 - 0.3 && player.x < x1 + 0.3,
        None => false,
    }
}

pub(crate) fn spawn_surge(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, at: Vec3, undertow: bool) {
    // The Undertow is the same kind of thing, but sick green and heavier.
    let glow = if undertow { LinearRgba::rgb(4.0, 14.0, 9.0) } else { LinearRgba::rgb(9.0, 12.0, 18.0) };
    let shard_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: glow, unlit: true, ..default() });
    let dark_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.02, 0.02, 0.03), perceptual_roughness: 0.3, ..default() });
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut rng = GameRng::new(0x5u64 ^ (at.z.abs() as u64));
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            SurgeBody,
            PointLight { color: if undertow { Color::srgb(0.5, 1.0, 0.75) } else { Color::srgb(0.75, 0.88, 1.0) }, intensity: 500_000.0, range: 14.0, ..default() },
            DespawnOnExit(AppState::Playing),
            Name::new("The Surge"),
        ))
        .with_children(|c| {
            // A screaming face at the heart of the storm, facing where it's going (-Z).
            let eye_glow = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: if undertow { LinearRgba::rgb(6.0, 20.0, 10.0) } else { LinearRgba::rgb(20.0, 22.0, 26.0) }, unlit: true, ..default() });
            let void = mats.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
            let ball = meshes.add(Sphere::new(0.5).mesh().uv(20, 12));
            let tooth = meshes.add(Cone::new(0.5, 1.0));
            c.spawn((Transform::default(), Visibility::default(), SurgeFace)).with_children(|f| {
                f.spawn((Mesh3d(ball.clone()), MeshMaterial3d(dark_mat.clone()), Transform::from_scale(Vec3::new(0.95, 1.15, 0.8))));
                for x in [-0.2f32, 0.2] {
                    // Narrow, slanted, blazing eyes.
                    f.spawn((Mesh3d(ball.clone()), MeshMaterial3d(eye_glow.clone()), Transform::from_xyz(x, 0.22, -0.37).with_rotation(Quat::from_rotation_z(-x.signum() * 0.45)).with_scale(Vec3::new(0.3, 0.08, 0.08)), SurgeEye));
                }
                // The jaw: a gaping black mouth ringed with teeth; it flaps as it screams.
                f.spawn((Transform::from_xyz(0.0, -0.22, -0.34), Visibility::default(), SurgeJaw)).with_children(|j| {
                    j.spawn((Mesh3d(ball.clone()), MeshMaterial3d(void.clone()), Transform::from_scale(Vec3::new(0.56, 0.62, 0.18))));
                    for k in 0..18 {
                        let a = k as f32 / 18.0 * std::f32::consts::TAU;
                        j.spawn((
                            Mesh3d(tooth.clone()),
                            MeshMaterial3d(eye_glow.clone()),
                            Transform::from_xyz(a.sin() * 0.25, a.cos() * 0.29, -0.05).with_rotation(Quat::from_rotation_z(-a + std::f32::consts::PI)).with_scale(Vec3::new(0.04, 0.12 + 0.05 * ((k * 7) % 3) as f32, 0.02)),
                        ));
                    }
                });
            });
            for i in 0..56 {
                let streak = i >= 44;
                let dark = i % 5 == 0 && !streak;
                c.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(if dark { dark_mat.clone() } else { shard_mat.clone() }),
                    Transform::from_scale(Vec3::splat(0.1)),
                    Shard { seed: rng.range_f32(0.0, 100.0), radius: rng.range_f32(0.3, 1.1), streak },
                ));
            }
        });
}

/// The face judders, the jaw flaps open and shut in a scream, the eyes flicker.
#[allow(clippy::type_complexity)]
fn animate_face(
    time: Res<Time>,
    mut faces: Query<&mut Transform, (With<SurgeFace>, Without<SurgeJaw>, Without<SurgeEye>)>,
    mut jaws: Query<&mut Transform, (With<SurgeJaw>, Without<SurgeFace>, Without<SurgeEye>)>,
    mut eyes: Query<&mut Transform, (With<SurgeEye>, Without<SurgeFace>, Without<SurgeJaw>)>,
) {
    let t = time.elapsed_secs();
    for mut f in &mut faces {
        f.translation = Vec3::new((t * 37.0).sin() * 0.05, (t * 29.0).cos() * 0.05, 0.0);
        f.rotation = Quat::from_rotation_z((t * 13.0).sin() * 0.12);
    }
    for mut j in &mut jaws {
        let open = 0.75 + 0.45 * (t * 11.0).sin().abs();
        j.scale = Vec3::new(1.0 + 0.1 * (t * 7.0).sin(), open, 1.0);
    }
    for mut e in &mut eyes {
        let flicker = if (t * 9.0).sin() > 0.9 { 0.4 } else { 1.0 };
        e.scale = Vec3::new(0.3, 0.08 * flicker, 0.08);
    }
}

fn animate_shards(time: Res<Time>, mut shards: Query<(&Shard, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (s, mut tf) in &mut shards {
        let a = t * (6.0 + s.seed % 5.0) + s.seed;
        let b = t * (4.3 + s.seed % 3.0) + s.seed * 1.7;
        if s.streak {
            // Long streaks trail behind (+Z relative to travel toward -Z).
            tf.translation = Vec3::new(a.sin() * 0.5, b.cos() * 0.5, 1.2 + (s.seed % 1.0) * 1.6);
            tf.scale = Vec3::new(0.03, 0.03, 1.2 + (t * 9.0 + s.seed).sin().abs() * 0.8);
            tf.rotation = Quat::IDENTITY;
        } else {
            let jitter = ((t * 23.0 + s.seed).sin() * 0.5 + 0.5) * 0.25;
            let r = s.radius + jitter;
            tf.translation = Vec3::new(a.sin() * b.cos() * r, b.sin() * r * 0.8, a.cos() * b.cos() * r);
            tf.rotation = Quat::from_euler(EulerRot::XYZ, a * 2.0, b * 3.0, a + b);
            let size = 0.06 + ((t * 17.0 + s.seed).sin().abs()) * 0.16;
            tf.scale = Vec3::new(size, size * 0.4, size * 1.6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor::plan::generate;

    #[test]
    fn standing_to_the_side_does_not_save_you() {
        let room = Some((-6.0, 6.0));
        let surge = Vec3::new(0.0, 1.4, -20.0);
        // Far to the side of its path, in the same stretch of the room.
        assert!(caught(Vec3::new(5.5, 0.0, -20.5), surge, room));
        // Well ahead or behind it: not yet.
        assert!(!caught(Vec3::new(5.5, 0.0, -26.0), surge, room));
        // In the next room over (outside the walls): no.
        assert!(!caught(Vec3::new(9.0, 0.0, -20.0), surge, room));
    }

    #[test]
    fn cue_window_is_learnable() {
        assert!(cue_seconds(6) >= 4.5);
        assert!(cue_seconds(99) >= 3.2);
        assert!(cue_seconds(50) < cue_seconds(10));
    }

    #[test]
    fn path_runs_through_doorways_toward_negative_z() {
        let plan = generate(11);
        let path = surge_path(&plan, 20);
        assert!(path.len() > 6);
        for w in path.windows(2) {
            assert!(w[1].z <= w[0].z + 1e-4, "path must only head forward");
        }
    }

    #[test]
    fn advance_moves_exact_distance_and_finishes() {
        let path = vec![Vec3::ZERO, Vec3::new(0.0, 0.0, -10.0), Vec3::new(5.0, 0.0, -10.0)];
        let (p, seg, done) = advance(&path, 0, Vec3::ZERO, 12.0);
        assert!((p - Vec3::new(2.0, 0.0, -10.0)).length() < 1e-4);
        assert_eq!(seg, 1);
        assert!(!done);
        let (_, _, done) = advance(&path, seg, p, 100.0);
        assert!(done);
    }

    #[test]
    fn surge_cue_fits_inside_hide_limit() {
        // The hide timer pauses while a threat is active, but the warning
        // itself must still fit, so hiding at the first cue is always safe.
        let cfg = crate::data::EntityConfig::default();
        for room in 6..100 {
            assert!(cfg.surge.warn_for(room) < crate::interaction::HIDE_LIMIT);
            assert!(cfg.undertow.warn_for(room) < crate::interaction::HIDE_LIMIT);
        }
    }

    #[test]
    fn undertow_path_is_shorter_than_surge_path() {
        let plan = generate(4);
        let len = |p: &[Vec3]| p.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
        assert!(len(&undertow_path(&plan, 40)) < len(&surge_path(&plan, 40)));
        assert!(undertow_path(&plan, 40).len() >= 4);
    }
}
