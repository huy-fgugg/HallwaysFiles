//! Chase hazards: every run's Door 40 chase is different.
//!
//! Each chase room (41–46) gets seeded hazards between its barricades, picked
//! fresh from the run's seed:
//!
//! * **Grasping hands** burst out of one side wall in waves. A grab hurts and
//!   slows you for a moment (with The Hollow right behind you).
//! * **Fire vents** across part of the floor roar up in timed bursts.
//! * **Falling ceiling**: as you run in, a chunk of ceiling cracks above where
//!   you are; a shadow marks the spot just before it lands.
//!
//! Rule: hands and fire never span the whole room. There is always a free lane
//! at least [`LANE`] wide beside them, so the way through stays open and you
//! can choose to dodge or to time it. None of them have colliders.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{door_zones, zones_overlap, FloorPlan, RoomPlan, Zone, LANDING_DEPTH, PURSUIT_FIRST, PURSUIT_LAST, STEP_RISE, STEP_RUN};
use crate::floor::{Floor, FloorRuntime, RoomMember};
use crate::hollow::DeathCause;
use crate::player::Player;
use crate::pursuit::Pursuit;
use crate::rng::GameRng;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};

/// Free width always left beside hands and fire.
pub const LANE: f32 = 1.6;
const HANDS_PERIOD: f32 = 2.6;
const FIRE_ON: f32 = 1.3;
const FIRE_OFF: f32 = 1.5;
const FIRE_WARN: f32 = 0.45;
const DEBRIS_WARN: f32 = 0.9;
const DEBRIS_FALL: f32 = 0.35;
const DEBRIS_RADIUS: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HazardKind {
    /// Arms reaching `reach` metres out of the east or west wall.
    Hands { east: bool, reach: f32 },
    /// A strip of vents from one side wall, `len` metres long.
    Fire { east: bool, len: f32 },
    /// Ceiling collapse aimed at the player.
    Debris,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hazard {
    pub room: u32,
    pub kind: HazardKind,
    /// Centre line of the hazard band (world z).
    pub z: f32,
    /// Half depth of the band along z.
    pub half: f32,
    /// Cycle offset in seconds, so neighbouring hazards don't pulse together.
    pub phase: f32,
}

impl Hazard {
    /// Floor area the hazard can reach (x, z).
    pub fn zone(&self, r: &RoomPlan) -> Zone {
        let (lo, hi) = match self.kind {
            HazardKind::Hands { east, reach } | HazardKind::Fire { east, len: reach } => {
                if east { (r.max.x - reach, r.max.x) } else { (r.min.x, r.min.x + reach) }
            }
            HazardKind::Debris => (r.min.x, r.max.x),
        };
        (Vec2::new(lo, self.z - self.half), Vec2::new(hi, self.z + self.half))
    }
}

/// Stretches of a chase room between its barricades where hazards can go
/// (south z, north z), clear of the walls, barricades, doors and stairs.
fn stretches(r: &RoomPlan) -> Vec<(f32, f32)> {
    let stairs = if r.entry_height > 0.0 { LANDING_DEPTH + r.entry_height / STEP_RISE * STEP_RUN + 0.6 } else { 0.0 };
    let mut lines: Vec<f32> = vec![r.max.y - 1.6 - stairs];
    lines.extend(r.dividers().iter().map(|d| d.z));
    lines.push(r.min.y + 1.6);
    lines.sort_by(|a, b| b.total_cmp(a));
    lines.windows(2).map(|w| (w[0] - 0.9, w[1] + 0.9)).filter(|(s, n)| s - n >= 1.2).collect()
}

/// The hazards of one chase room on this seed.
pub fn room_hazards(seed: u64, r: &RoomPlan) -> Vec<Hazard> {
    if !r.pursuit {
        return vec![];
    }
    let mut rng = GameRng::fork(seed ^ 0xC4A5_E0B5, u64::from(r.index));
    let width = r.width();
    let doors = door_zones(r);
    let mut out = vec![];
    for (s, n) in stretches(r) {
        let z = (s + n) / 2.0;
        let half = ((s - n) / 2.0).min(0.8);
        let roll = rng.next_f32();
        let east = rng.next_f32() < 0.5;
        let phase = rng.range_f32(0.0, 2.5);
        let room_left = (width - LANE).max(0.0);
        // QA / trailer: HALLWAYS_HAZARDS=hands,fire,debris fixes the order.
        let roll = match forced_kind(out.len()) {
            Some(k) => k,
            None => roll,
        };
        let kind = if roll < 0.4 && room_left >= 1.0 {
            HazardKind::Hands { east, reach: room_left.min(rng.range_f32(1.6, 3.2)) }
        } else if roll < 0.75 && room_left >= 1.0 {
            HazardKind::Fire { east, len: room_left.min(rng.range_f32(2.0, 4.5)) }
        } else if roll < 0.92 {
            HazardKind::Debris
        } else {
            continue;
        };
        let h = Hazard { room: r.index, kind, z, half, phase };
        if doors.iter().any(|d| zones_overlap(&h.zone(r), d)) {
            continue;
        }
        out.push(h);
    }
    // Every chase room has at least something in it.
    if out.is_empty()
        && let Some((s, n)) = stretches(r).first().copied()
    {
        out.push(Hazard { room: r.index, kind: HazardKind::Debris, z: (s + n) / 2.0, half: ((s - n) / 2.0).min(0.8), phase: 0.0 });
    }
    out
}

/// A roll that picks the n-th hazard named in `HALLWAYS_HAZARDS` (cycling).
fn forced_kind(n: usize) -> Option<f32> {
    let list = std::env::var("HALLWAYS_HAZARDS").ok()?;
    let names: Vec<&str> = list.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    let name = names.get(n % names.len().max(1))?;
    match *name {
        "hands" => Some(0.1),
        "fire" => Some(0.5),
        "debris" => Some(0.8),
        _ => None,
    }
}

/// All chase hazards on a floor.
pub fn floor_hazards(plan: &FloorPlan) -> Vec<Hazard> {
    (PURSUIT_FIRST..=PURSUIT_LAST).filter_map(|i| plan.room(i)).flat_map(|r| room_hazards(plan.seed, r)).collect()
}

/// 0..1: how far the hands are out at time `t`.
pub fn hands_out(t: f32, phase: f32) -> f32 {
    let k = ((t + phase) / HANDS_PERIOD).fract();
    // Rest, burst out, hold, pull back.
    if k < 0.35 {
        0.0
    } else if k < 0.47 {
        (k - 0.35) / 0.12
    } else if k < 0.8 {
        1.0
    } else if k < 0.92 {
        1.0 - (k - 0.8) / 0.12
    } else {
        0.0
    }
}

/// Fire state at time `t`: (burning, warning flicker).
pub fn fire_state(t: f32, phase: f32) -> (bool, bool) {
    let k = (t + phase) % (FIRE_ON + FIRE_OFF);
    (k < FIRE_ON, k >= FIRE_ON + FIRE_OFF - FIRE_WARN)
}

#[derive(Component)]
struct HazardBody {
    h: Hazard,
    /// Debris: drop point (x) once triggered, and time since.
    drop: Option<(f32, f32)>,
    done: bool,
    cooldown: f32,
}

#[derive(Component)]
struct Arm {
    wall: f32,
    dir: f32,
    reach: f32,
    /// The hand (moves to the tip) rather than the forearm (stretches).
    hand: bool,
}

#[derive(Component)]
struct Flame {
    base: f32,
    seed: f32,
    /// The bright inner core (narrower).
    core: bool,
}

#[derive(Component)]
struct FireLight;

#[derive(Component)]
struct DebrisRock;

#[derive(Component)]
struct DebrisShadow;

/// Rooms whose hazards have been built.
#[derive(Resource, Default)]
struct Built(Vec<u32>);

/// Seconds of slowdown left after a grab (read by the player controller).
#[derive(Resource, Default)]
pub struct Grabbed(pub f32);

pub struct HazardPlugin;

impl Plugin for HazardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Built>()
            .init_resource::<Grabbed>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| {
                c.insert_resource(Built::default());
                c.insert_resource(Grabbed::default());
            })
            .add_systems(Update, spawn_hazards.run_if(in_state(AppState::Playing)))
            .add_systems(Update, run_hazards.run_if(gameplay_active));
    }
}

fn spawn_hazards(
    mut commands: Commands,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    mut built: ResMut<Built>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    built.0.retain(|i| rt.loaded.contains(i));
    for i in PURSUIT_FIRST..=PURSUIT_LAST {
        if built.0.contains(&i) || !rt.loaded.contains(&i) {
            continue;
        }
        let Some(r) = floor.0.room(i) else { continue };
        built.0.push(i);
        let ink = mats.add(StandardMaterial { base_color: Color::srgb(0.04, 0.02, 0.05), perceptual_roughness: 0.35, ..default() });
        let ink_glow = mats.add(StandardMaterial { base_color: Color::srgb(0.25, 0.05, 0.3), emissive: LinearRgba::rgb(0.5, 0.05, 0.6), ..default() });
        let grate = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.11, 0.1), metallic: 0.8, perceptual_roughness: 0.5, ..default() });
        let flame_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.42, 0.08), emissive: LinearRgba::rgb(7.0, 2.0, 0.3), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
        let core_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.9, 0.45), emissive: LinearRgba::rgb(12.0, 9.0, 3.0), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
        let rock_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.35, 0.32, 0.3), perceptual_roughness: 0.95, ..default() });
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let blob = meshes.add(Sphere::new(0.5).mesh().uv(16, 10));
        let disc = meshes.add(Cylinder::new(1.0, 0.01));
        for h in room_hazards(floor.0.seed, r) {
            // Each falling-stone shadow fades in on its own.
            let shadow_mat = mats.add(StandardMaterial { base_color: Color::srgba(0.0, 0.0, 0.0, 0.0), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
            let mut root = commands.spawn((
                Transform::default(),
                Visibility::default(),
                HazardBody { h, drop: None, done: false, cooldown: 0.0 },
                RoomMember(i),
                DespawnOnExit(AppState::Playing),
                Name::new(format!("Chase hazard {:?}", h.kind)),
            ));
            match h.kind {
                HazardKind::Hands { east, reach } => {
                    let wall = if east { r.max.x } else { r.min.x };
                    let dir = if east { -1.0 } else { 1.0 };
                    let mut rng = GameRng::fork(r.seed, 77);
                    root.with_children(|c| {
                        // Ink stain where the arms come through.
                        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink.clone()), Transform::from_xyz(wall + dir * 0.01, 1.2, h.z).with_scale(Vec3::new(0.02, 2.0, h.half * 2.2))));
                        for k in 0..5 {
                            let z = h.z - h.half + (k as f32 + 0.5) * (h.half * 2.0 / 5.0);
                            let y = 0.55 + rng.next_f32() * 1.3;
                            let len = reach * rng.range_f32(0.8, 1.0);
                            // Forearm (stretches out of the wall) and the clawed hand at its tip.
                            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink.clone()), Transform::from_xyz(wall, y, z).with_scale(Vec3::new(0.01, 0.1, 0.1)), Arm { wall, dir, reach: len, hand: false }));
                            c.spawn((Transform::from_xyz(wall, y, z), Visibility::default(), Arm { wall, dir, reach: len, hand: true })).with_children(|a| {
                                a.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink.clone()), Transform::from_xyz(0.07, 0.0, 0.0).with_scale(Vec3::new(0.16, 0.06, 0.15))));
                                for f in 0..4 {
                                    let fz = -0.055 + f as f32 * 0.037;
                                    a.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink_glow.clone()), Transform::from_xyz(0.22, -0.02, fz).with_rotation(Quat::from_rotation_z(-0.4)).with_scale(Vec3::new(0.16, 0.02, 0.02))));
                                }
                                a.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink_glow.clone()), Transform::from_xyz(0.12, 0.02, 0.09).with_rotation(Quat::from_rotation_y(-0.7)).with_scale(Vec3::new(0.12, 0.02, 0.02))));
                            });
                        }
                    });
                }
                HazardKind::Fire { east, len } => {
                    let (x0, x1) = if east { (r.max.x - len, r.max.x) } else { (r.min.x, r.min.x + len) };
                    root.with_children(|c| {
                        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(grate.clone()), Transform::from_xyz((x0 + x1) / 2.0, 0.02, h.z).with_scale(Vec3::new(x1 - x0, 0.04, h.half * 1.6))));
                        let n = ((x1 - x0) / 0.45).ceil().max(2.0) as usize;
                        for k in 0..n {
                            for row in [-0.35f32, 0.0, 0.35] {
                                let x = x0 + (k as f32 + 0.5) * (x1 - x0) / n as f32;
                                let seed = k as f32 * 1.7 + row * 5.0;
                                let base = 1.0 + (k % 3) as f32 * 0.25;
                                c.spawn((Mesh3d(blob.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(x, 0.0, h.z + row * h.half).with_scale(Vec3::new(0.3, 0.01, 0.3)), Flame { base, seed, core: false }));
                                c.spawn((Mesh3d(blob.clone()), MeshMaterial3d(core_mat.clone()), Transform::from_xyz(x, 0.0, h.z + row * h.half).with_scale(Vec3::new(0.15, 0.01, 0.15)), Flame { base: base * 0.55, seed: seed + 2.0, core: true }));
                            }
                        }
                        c.spawn((PointLight { color: Color::srgb(1.0, 0.5, 0.15), intensity: 0.0, range: 7.0, ..default() }, Transform::from_xyz((x0 + x1) / 2.0, 0.8, h.z), FireLight));
                    });
                }
                HazardKind::Debris => {
                    root.with_children(|c| {
                        c.spawn((Mesh3d(disc.clone()), MeshMaterial3d(shadow_mat.clone()), Transform::from_xyz(0.0, 0.015, h.z).with_scale(Vec3::splat(DEBRIS_RADIUS)), Visibility::Hidden, DebrisShadow));
                        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(rock_mat.clone()), Transform::from_xyz(0.0, r.height, h.z).with_scale(Vec3::new(0.9, 0.5, 0.8)), Visibility::Hidden, DebrisRock));
                    });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_hazards(
    time: Res<Time>,
    floor: Option<Res<Floor>>,
    pursuit: Res<Pursuit>,
    mut run: ResMut<RunState>,
    mut grabbed: ResMut<Grabbed>,
    player: Query<&Transform, (With<Player>, Without<Arm>, Without<Flame>, Without<DebrisRock>, Without<DebrisShadow>, Without<HazardBody>)>,
    mut bodies: Query<(&mut HazardBody, &Children)>,
    mut arms: Query<(&Arm, &mut Transform), (Without<Flame>, Without<DebrisRock>, Without<DebrisShadow>)>,
    mut flames: Query<(&Flame, &mut Transform), (Without<Arm>, Without<DebrisRock>, Without<DebrisShadow>)>,
    mut lights: Query<&mut PointLight, With<FireLight>>,
    mut rocks: Query<(&mut Transform, &mut Visibility), (With<DebrisRock>, Without<Arm>, Without<Flame>, Without<DebrisShadow>)>,
    mut shadows: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (With<DebrisShadow>, Without<Arm>, Without<Flame>, Without<DebrisRock>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    (mut shake, mut sfx, mut subs): (ResMut<CameraShake>, MessageWriter<PlaySfx>, MessageWriter<Subtitle>),
    (mut death, mut next_overlay): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    grabbed.0 = (grabbed.0 - dt).max(0.0);
    let Some(floor) = floor else { return };
    let Ok(p) = player.single() else { return };
    let active = pursuit.active();
    let pp = Vec2::new(p.translation.x, p.translation.z);
    let mut hurt: Option<(f32, &'static str, &'static str)> = None;
    for (mut body, children) in &mut bodies {
        body.cooldown = (body.cooldown - dt).max(0.0);
        let h = body.h;
        let Some(r) = floor.0.room(h.room) else { continue };
        let zone = h.zone(r);
        let inside = pp.x > zone.0.x && pp.x < zone.1.x && pp.y > zone.0.y - 0.2 && pp.y < zone.1.y + 0.2 && p.translation.y < 1.0;
        match h.kind {
            HazardKind::Hands { .. } => {
                let out = if active { hands_out(t, h.phase) } else { 0.0 };
                for c in children.iter() {
                    if let Ok((arm, mut tf)) = arms.get_mut(c) {
                        let len = (arm.reach * out).max(0.01);
                        let wiggle = (t * 9.0 + tf.translation.z * 7.0).sin() * 0.05;
                        let face = Quat::from_rotation_y(if arm.dir > 0.0 { 0.0 } else { std::f32::consts::PI });
                        if arm.hand {
                            tf.translation.x = arm.wall + arm.dir * len;
                            tf.rotation = face * Quat::from_rotation_z(wiggle * 3.0);
                            tf.scale = Vec3::splat(if out > 0.02 { 1.6 } else { 0.01 });
                        } else {
                            tf.translation.x = arm.wall + arm.dir * len / 2.0;
                            tf.rotation = face * Quat::from_rotation_z(wiggle);
                            tf.scale = Vec3::new(len, 0.1, 0.1);
                        }
                    }
                }
                // Reach measured from the wall: the hands catch you within their extension.
                let reach_now = match h.kind {
                    HazardKind::Hands { reach, .. } => reach * out,
                    _ => 0.0,
                };
                let from_wall = if matches!(h.kind, HazardKind::Hands { east: true, .. }) { r.max.x - pp.x } else { pp.x - r.min.x };
                if out > 0.6 && inside && from_wall < reach_now && body.cooldown <= 0.0 {
                    body.cooldown = 1.6;
                    grabbed.0 = 0.9;
                    shake.add(0.5);
                    sfx.write(PlaySfx(Sfx::Whoosh));
                    subs.write(Subtitle::danger("[Hands burst from the wall and claw at you!]"));
                    hurt = Some((15.0, "THE WALLS GRABBED YOU", "Hands reach out of the walls in waves during the chase. Keep to the far side, or slip past while they pull back."));
                }
            }
            HazardKind::Fire { .. } => {
                let (on, warn) = if active { fire_state(t, h.phase) } else { (false, false) };
                for c in children.iter() {
                    if let Ok((f, mut tf)) = flames.get_mut(c) {
                        let flick = 0.8 + 0.2 * (t * 23.0 + f.seed).sin();
                        let hgt = if on { 1.1 * f.base * flick } else if warn { 0.18 * flick } else { 0.01 };
                        let w = if on { 0.4 } else { 0.18 } * if f.core { 0.5 } else { 1.0 } * (0.9 + 0.1 * (t * 31.0 + f.seed).cos());
                        tf.scale = Vec3::new(w, hgt, w);
                        tf.translation.y = hgt * 0.42;
                    }
                    if let Ok(mut l) = lights.get_mut(c) {
                        l.intensity = if on { 260_000.0 * (0.85 + 0.15 * (t * 17.0).sin()) } else if warn { 25_000.0 } else { 0.0 };
                    }
                }
                if on && inside && body.cooldown <= 0.0 {
                    body.cooldown = 0.5;
                    shake.add(0.15);
                    if hurt.is_none() {
                        subs.write(Subtitle::danger("[The vents roar. You're burning!]"));
                    }
                    hurt = Some((8.0, "BURNED IN THE CHASE", "Fire vents flare in bursts during the chase. Watch for the flicker, then run past while they're out, or take the clear lane beside them."));
                }
            }
            HazardKind::Debris => {
                if body.done {
                    continue;
                }
                if body.drop.is_none() && active && (pp.y - h.z).abs() < 3.2 && pp.y > h.z - 0.5 {
                    // Aim at where the player is heading.
                    let x = pp.x.clamp(r.min.x + 0.6, r.max.x - 0.6);
                    body.drop = Some((x, 0.0));
                    sfx.write(PlaySfx(Sfx::Rumble));
                    shake.add(0.3);
                    subs.write(Subtitle::danger("[The ceiling cracks above you!]"));
                }
                if let Some((x, mut s)) = body.drop {
                    s += dt;
                    body.drop = Some((x, s));
                    for c in children.iter() {
                        if let Ok((mut tf, mut vis, m)) = shadows.get_mut(c) {
                            *vis = Visibility::Visible;
                            tf.translation.x = x;
                            let k = (s / DEBRIS_WARN).min(1.0);
                            tf.scale = Vec3::new(DEBRIS_RADIUS * (0.4 + 0.6 * k), 1.0, DEBRIS_RADIUS * (0.4 + 0.6 * k));
                            if let Some(mut mat) = mats.get_mut(&m.0) {
                                mat.base_color = Color::srgba(0.0, 0.0, 0.0, 0.25 + 0.5 * k);
                            }
                        }
                        if let Ok((mut tf, mut vis)) = rocks.get_mut(c) {
                            tf.translation.x = x;
                            if s > DEBRIS_WARN {
                                *vis = Visibility::Visible;
                                let k = ((s - DEBRIS_WARN) / DEBRIS_FALL).min(1.0);
                                tf.translation.y = r.height * (1.0 - k * k) + 0.25 * k * k;
                                tf.rotation = Quat::from_rotation_x(k * 1.3) * Quat::from_rotation_z(k * 0.7);
                            }
                        }
                    }
                    if s >= DEBRIS_WARN + DEBRIS_FALL {
                        body.done = true;
                        shake.add(0.6);
                        sfx.write(PlaySfx(Sfx::Rumble));
                        if Vec2::new(x, h.z).distance(pp) < DEBRIS_RADIUS && p.translation.y < 1.0 {
                            subs.write(Subtitle::danger("[Falling stone hits you!]"));
                            hurt = Some((25.0, "CRUSHED IN THE CHASE", "When the ceiling cracks during the chase, a shadow marks where it falls. Sidestep out of the shadow."));
                        }
                    }
                }
            }
        }
    }
    if let Some((dmg, title, lesson)) = hurt
        && run.damage(dmg)
    {
        *death = DeathCause { title: title.into(), cause: format!("Room {:03}, during the chase after Door 040.", run.current_room), lesson: lesson.into() };
        next_overlay.set(Overlay::Dead);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor::plan::generate;

    #[test]
    fn hazards_differ_between_runs_and_repeat_per_seed() {
        assert_eq!(floor_hazards(&generate(3)), floor_hazards(&generate(3)));
        let layouts: std::collections::HashSet<String> = (0..30).map(|s| format!("{:?}", floor_hazards(&generate(s)).iter().map(|h| (h.room, std::mem::discriminant(&h.kind))).collect::<Vec<_>>())).collect();
        assert!(layouts.len() > 20, "{} different chases", layouts.len());
    }

    #[test]
    fn every_chase_room_has_a_hazard_and_a_free_lane() {
        let mut kinds = [0; 3];
        for seed in 0..300 {
            let plan = generate(seed);
            for i in PURSUIT_FIRST..=PURSUIT_LAST {
                let r = plan.room(i).unwrap();
                let hs = room_hazards(seed, r);
                assert!(!hs.is_empty(), "seed {seed} room {i} has no hazard");
                for h in hs {
                    let zone = h.zone(r);
                    match h.kind {
                        HazardKind::Hands { reach, .. } | HazardKind::Fire { len: reach, .. } => {
                            assert!(r.width() - reach >= LANE - 1e-3, "seed {seed} room {i}: no free lane");
                            kinds[if matches!(h.kind, HazardKind::Hands { .. }) { 0 } else { 1 }] += 1;
                        }
                        HazardKind::Debris => kinds[2] += 1,
                    }
                    // Inside the room, clear of doors and barricade lines.
                    assert!(zone.0.y > r.min.y && zone.1.y < r.max.y, "seed {seed} room {i}: outside");
                    assert!(!door_zones(r).iter().any(|d| zones_overlap(&zone, d)), "seed {seed} room {i}: on a door");
                    for d in r.dividers() {
                        assert!((d.z - h.z).abs() >= h.half + 0.5, "seed {seed} room {i}: on a barricade");
                    }
                }
            }
        }
        assert!(kinds.iter().all(|&k| k > 100), "{kinds:?}");
    }

    #[test]
    fn hands_and_fire_cycle() {
        assert_eq!(hands_out(0.0, 0.0), 0.0);
        assert_eq!(hands_out(HANDS_PERIOD * 0.6, 0.0), 1.0);
        assert!(fire_state(0.1, 0.0).0);
        assert!(!fire_state(FIRE_ON + 0.1, 0.0).0);
        assert!(fire_state(FIRE_ON + FIRE_OFF - 0.1, 0.0).1);
    }
}
