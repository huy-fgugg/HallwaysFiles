//! The Mode Creator's own monster: a rushing entity whose name, colour,
//! speed, passes, damage, warning and spawn chance come from the mode
//! ([`crate::presets::CustomMonster`]). It behaves like the Surge: a warning,
//! then it tears through the room along the door path. **Counter: hide.**
//! Only rooms with a hiding spot and no other monster can roll it.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::RoomKind;
use crate::floor::Floor;
use crate::hollow::DeathCause;
use crate::interaction::HideState;
use crate::player::Player;
use crate::presets::{ActivePreset, CustomMonster};
use crate::rng::GameRng;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};
use crate::surge::{advance, caught, surge_path};
use crate::world::Mood;

/// True while the custom monster is warning or rushing (hiding timers pause).
#[derive(Resource, Debug, Default)]
pub struct StrangerThreat(pub bool);

#[derive(Debug, Clone, PartialEq, Default)]
enum Phase {
    #[default]
    Idle,
    Cue { room: u32, t: f32 },
    Rushing { room: u32, path: Vec<Vec3>, seg: usize, pos: Vec3, speed: f32, passes_left: u8, pause: f32, hit: bool, close: bool },
}

#[derive(Resource, Debug, Default)]
struct Stranger {
    phase: Phase,
    /// Rooms already rolled (each room rolls once).
    rolled: HashSet<u32>,
}

#[derive(Component)]
struct StrangerBody;

/// Pure rule: does `room` roll the custom monster? Deterministic per room seed.
pub fn rolls(m: &CustomMonster, room: u32, seed: u64) -> bool {
    m.on && room >= m.first_room && GameRng::fork(seed, 0x5752).next_f32() * 100.0 < f32::from(m.chance)
}

pub struct StrangerPlugin;

impl Plugin for StrangerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stranger>()
            .init_resource::<StrangerThreat>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| {
                c.insert_resource(Stranger::default());
                c.insert_resource(StrangerThreat::default());
            })
            .add_systems(Update, run_stranger.run_if(gameplay_active))
            .add_systems(Update, animate.run_if(in_state(AppState::Playing)));
    }
}

#[allow(clippy::too_many_arguments)]
fn run_stranger(
    mut commands: Commands,
    time: Res<Time>,
    active: Res<ActivePreset>,
    floor: Option<Res<Floor>>,
    mut st: ResMut<Stranger>,
    mut threat: ResMut<StrangerThreat>,
    mut run: ResMut<RunState>,
    mut mood: ResMut<Mood>,
    (hide, surge, chase): (Res<HideState>, Res<crate::surge::SurgeState>, Res<crate::hollow::ChaseState>),
    player: Query<&Transform, (With<Player>, Without<StrangerBody>)>,
    mut bodies: Query<(Entity, &mut Transform), With<StrangerBody>>,
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    (mut shake, mut sfx, mut subs): (ResMut<CameraShake>, MessageWriter<PlaySfx>, MessageWriter<Subtitle>),
    (mut death, mut next_overlay): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
) {
    threat.0 = st.phase != Phase::Idle;
    let (Some(m), Some(floor)) = (active.0.as_ref().map(|p| &p.custom).filter(|m| m.on), floor) else { return };
    let dt = time.delta_secs();
    let tint = m.rgb();
    match &mut st.phase {
        Phase::Idle => {
            let room = run.current_room;
            if st.rolled.contains(&room) {
                return;
            }
            st.rolled.insert(room);
            let Some(r) = floor.0.room(room) else { return };
            let free = r.hiding && !r.pursuit && r.encounter.is_none() && r.kind != RoomKind::Final && room < floor.0.last_room;
            if !free || surge.0 != crate::surge::SurgePhase::Idle || chase.active || !rolls(m, room, r.seed) {
                return;
            }
            st.phase = Phase::Cue { room, t: 0.0 };
            *mood = Mood { tint, amount: 0.4, scale: 1.0, flicker: 0.85 };
            sfx.write(PlaySfx(Sfx::SurgeRoar));
            subs.write(Subtitle::danger(format!("[{} is coming...] HIDE!", m.name.to_uppercase())));
        }
        Phase::Cue { room, t } => {
            *t += dt;
            if *t >= f32::from(m.warning) / 10.0 {
                let room = *room;
                let path = surge_path(&floor.0, room);
                let start = path[0];
                spawn_body(&mut commands, &mut meshes, &mut mats, start, tint, &m.name);
                let k = floor.0.room(room).map_or(0.5, |r| GameRng::fork(r.seed, 0x5753).next_f32());
                let speed = f32::from(m.speed_min) + (f32::from(m.speed_max) - f32::from(m.speed_min)) * k;
                subs.write(Subtitle::danger(format!("[{} IS HERE]", m.name.to_uppercase())));
                st.phase = Phase::Rushing { room, path, seg: 0, pos: start, speed, passes_left: m.passes, pause: 0.0, hit: false, close: false };
            }
        }
        Phase::Rushing { room, path, seg, pos, speed, passes_left, pause, hit, close } => {
            if *pause > 0.0 {
                *pause -= dt;
                return;
            }
            let (np, ns, done) = advance(path, *seg, *pos, *speed * dt);
            *pos = np;
            *seg = ns;
            for (_, mut tf) in &mut bodies {
                tf.translation = np;
            }
            if let Ok(p) = player.single() {
                let d = Vec2::new(p.translation.x - np.x, p.translation.z - np.z).length();
                if d < 5.0 && !*close {
                    *close = true;
                    shake.add(0.5);
                    sfx.write(PlaySfx(Sfx::Whoosh));
                }
                let bounds = floor.0.room_at_z(np.z).and_then(|i| floor.0.room(i)).map(|r| (r.min.x, r.max.x));
                if !*hit && caught(p.translation, np, bounds) && !hide.is_hiding() {
                    *hit = true;
                    shake.add(1.0);
                    sfx.write(PlaySfx(Sfx::Stinger));
                    if run.damage(f32::from(m.damage)) || m.damage >= 100 {
                        run.vitality = 0.0;
                        *death = DeathCause {
                            title: format!("{} GOT YOU", m.name.to_uppercase()),
                            cause: format!("Caught in the open in Room {:03}.", run.current_room),
                            lesson: format!("{} is a monster from this player-made mode. When its warning shows, hide in a wardrobe or locker right away.", m.name),
                        };
                        for (e, _) in &bodies {
                            commands.entity(e).try_despawn();
                        }
                        *mood = Mood::default();
                        st.phase = Phase::Idle;
                        next_overlay.set(Overlay::Dead);
                        return;
                    }
                    subs.write(Subtitle::danger(format!("[{} tore past you!]", m.name)));
                }
            }
            if done && *passes_left > 0 {
                *passes_left -= 1;
                path.reverse();
                *seg = 0;
                *pos = path[0];
                *pause = 1.3;
                *hit = false;
                *close = false;
                sfx.write(PlaySfx(Sfx::Rumble));
                subs.write(Subtitle::danger("[It's turning back...]"));
                return;
            }
            if done {
                let _ = room;
                for (e, _) in &bodies {
                    commands.entity(e).try_despawn();
                }
                *mood = Mood::default();
                run.lumens += 5;
                subs.write(Subtitle::info(format!("[{} is gone.] (+5 Lumens)", m.name)));
                st.phase = Phase::Idle;
            }
        }
    }
}

#[derive(Component)]
struct Orbit(f32);

fn spawn_body(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, at: Vec3, tint: Color, name: &str) {
    let l = tint.to_linear();
    let glow = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(l.red * 14.0, l.green * 14.0, l.blue * 14.0), unlit: true, ..default() });
    let dark = mats.add(StandardMaterial { base_color: Color::srgb(0.02, 0.02, 0.02), perceptual_roughness: 0.4, ..default() });
    let ball = meshes.add(Sphere::new(0.5).mesh().uv(20, 12));
    let shard = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            StrangerBody,
            PointLight { color: tint, intensity: 450_000.0, range: 14.0, ..default() },
            DespawnOnExit(AppState::Playing),
            Name::new(format!("Custom monster: {name}")),
        ))
        .with_children(|c| {
            // A dark core with two blazing eyes, ringed by spinning shards of its colour.
            c.spawn((Mesh3d(ball.clone()), MeshMaterial3d(dark.clone()), Transform::from_scale(Vec3::splat(1.3))));
            for x in [-0.22f32, 0.22] {
                c.spawn((Mesh3d(ball.clone()), MeshMaterial3d(glow.clone()), Transform::from_xyz(x, 0.15, -0.6).with_scale(Vec3::new(0.26, 0.12, 0.1))));
            }
            for i in 0..14 {
                c.spawn((Mesh3d(shard.clone()), MeshMaterial3d(glow.clone()), Transform::from_scale(Vec3::new(0.08, 0.5, 0.08)), Orbit(i as f32)));
            }
        });
}

fn animate(time: Res<Time>, mut q: Query<(&Orbit, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (o, mut tf) in &mut q {
        let a = o.0 * 0.449 + t * (3.0 + o.0 % 3.0);
        let r = 0.95 + 0.2 * (t * 5.0 + o.0).sin();
        tf.translation = Vec3::new(a.cos() * r, (o.0 * 1.7 + t * 4.0).sin() * 0.6, a.sin() * r);
        tf.rotation = Quat::from_rotation_z(a) * Quat::from_rotation_x(t * 6.0 + o.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_respect_switch_first_room_and_chance() {
        let mut m = CustomMonster { on: true, chance: 100, first_room: 10, ..CustomMonster::default() };
        assert!(rolls(&m, 10, 1));
        assert!(!rolls(&m, 9, 1));
        m.on = false;
        assert!(!rolls(&m, 20, 1));
        m.on = true;
        m.chance = 30;
        let hits = (0..1000u64).filter(|s| rolls(&m, 20, *s)).count();
        assert!((200..400).contains(&hits), "{hits}");
    }
}
