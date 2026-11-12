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
use crate::interaction::HideState;
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
    Cue { room: u32, t: f32 },
    Rushing { room: u32, path: Vec<Vec3>, seg: usize, pos: Vec3, passed_close: bool },
}

#[derive(Resource, Debug)]
pub struct SurgeState(pub SurgePhase);

impl Default for SurgeState {
    fn default() -> Self {
        Self(SurgePhase::Idle)
    }
}

#[derive(Component)]
struct SurgeBody;

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
    let last = (room + 2).min(crate::floor::plan::LAST_ROOM);
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
            .add_systems(Update, animate_shards.run_if(in_state(AppState::Playing)));
    }
}

fn trigger(run: Res<RunState>, floor: Option<Res<Floor>>, rt: Option<Res<FloorRuntime>>, mut state: ResMut<SurgeState>, mut mood: ResMut<Mood>, mut sfx: MessageWriter<PlaySfx>, mut subs: MessageWriter<Subtitle>) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    if state.0 != SurgePhase::Idle {
        return;
    }
    let Some(r) = floor.0.room(run.current_room) else { return };
    if r.surge && !rt.surges_done.contains(&r.index) {
        state.0 = SurgePhase::Cue { room: r.index, t: 0.0 };
        *mood = Mood { tint: Color::srgb(0.75, 0.88, 1.0), amount: 0.35, scale: 1.0, flicker: 0.85 };
        sfx.write(PlaySfx(Sfx::SurgeRoar));
        subs.write(Subtitle::danger("[The lights strobe. A roar builds in the walls.] HIDE!"));
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
) {
    let dt = time.delta_secs();
    let Some(floor) = floor else { return };
    match &mut state.0 {
        SurgePhase::Idle => {}
        SurgePhase::Cue { room, t } => {
            *t += dt;
            if *t >= cue_seconds(*room) {
                let path = surge_path(&floor.0, *room);
                let start = path[0];
                spawn_surge(&mut commands, &mut meshes, &mut mats, start);
                subs.write(Subtitle::danger("[IT'S HERE]"));
                state.0 = SurgePhase::Rushing { room: *room, path, seg: 0, pos: start, passed_close: false };
            }
        }
        SurgePhase::Rushing { room, path, seg, pos, passed_close } => {
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
                if d < SURGE_KILL_RADIUS && !hide.is_hiding() {
                    shake.add(1.0);
                    sfx.write(PlaySfx(Sfx::Stinger));
                    *death = DeathCause {
                        title: "THE SURGE TORE THROUGH".into(),
                        cause: format!("Caught in the open in Room {:03}.", run.current_room),
                        lesson: "When the lights strobe and a roar builds, hide in a wardrobe or locker right away and stay in until the lights settle.".into(),
                    };
                    for (e, _) in &bodies {
                        commands.entity(e).despawn();
                    }
                    *mood = Mood::default();
                    state.0 = SurgePhase::Idle;
                    next_overlay.set(Overlay::Dead);
                    return;
                }
            }
            if done {
                let room = *room;
                for (e, _) in &bodies {
                    commands.entity(e).despawn();
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

fn spawn_surge(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, at: Vec3) {
    let shard_mat = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(9.0, 12.0, 18.0), unlit: true, ..default() });
    let dark_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.02, 0.02, 0.03), perceptual_roughness: 0.3, ..default() });
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut rng = GameRng::new(0x5u64 ^ (at.z.abs() as u64));
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            SurgeBody,
            PointLight { color: Color::srgb(0.75, 0.88, 1.0), intensity: 500_000.0, range: 14.0, ..default() },
            DespawnOnExit(AppState::Playing),
            Name::new("The Surge"),
        ))
        .with_children(|c| {
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
    fn surge_kill_time_fits_inside_hide_limit() {
        // Cue + full flight must be shorter than the wardrobe hide limit, so a
        // player who hides at the first cue never gets pushed out mid-pass.
        for seed in 0..30 {
            let plan = generate(seed);
            for r in plan.rooms.iter().filter(|r| r.surge) {
                let path = surge_path(&plan, r.index);
                let len: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
                assert!(cue_seconds(r.index) + len / SURGE_SPEED < crate::interaction::HIDE_LIMIT, "seed {seed} room {}", r.index);
            }
        }
    }
}
