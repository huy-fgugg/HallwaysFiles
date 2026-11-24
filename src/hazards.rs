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
//! * **Fallen shelves**: a ceiling beam has come down wall to wall with a
//!   toppled bookshelf on it and books spilled underneath. **Crouch** to get
//!   under (standing, you can't pass).
//! * **Burning chandelier**: a chandelier has crashed to the floor and is on
//!   fire; weave around it (its flames hurt).
//!
//! * **The hand hall**: the last room of each chase is a straight hall whose
//!   walls are lined with huge hands reaching out in waves, right up to the
//!   edges of a lit lane down the middle. Step out of the lane while they
//!   reach and they drag you into the wall.
//!
//! Every hand is a posed arm model ("Low Poly Arms (rigged)" by DevMop,
//! CC0, OpenGameArt), dyed inky black.
//!
//! Rule: hands and fire never span the whole room. There is always a free lane
//! at least [`LANE`] wide beside them, so the way through stays open and you
//! can choose to dodge or to time it. None of them have colliders.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{chases, door_zones, zones_overlap, Feature, FloorPlan, RoomPlan, Zone, LANDING_DEPTH, STEP_RISE, STEP_RUN};
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
    /// The hand hall: arms from both walls reaching `west` / `east` metres in,
    /// leaving the lane down the middle free.
    HandHall { west: f32, east: f32 },
    /// A fallen beam and bookshelf wall to wall: crouch under it.
    Fallen { tilt: f32 },
    /// A burning chandelier on the floor at `x`.
    Chandelier { x: f32 },
}

/// Underside of a fallen beam: crouching (head at ~1.1 m) gets under it.
pub const BEAM_BOTTOM: f32 = 1.28;
/// Burning chandelier: solid radius and how far its flames reach.
pub const CHANDELIER_SOLID: f32 = 0.45;
pub const CHANDELIER_BURN: f32 = 0.95;

/// Length of the arm model, shoulder to fingertips (metres).
const ARM_LEN: f32 = 2.6;
/// Seconds per wave of the hand hall.
const HALL_PERIOD: f32 = 2.4;

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
            HazardKind::Debris | HazardKind::HandHall { .. } | HazardKind::Fallen { .. } => (r.min.x, r.max.x),
            HazardKind::Chandelier { x } => (x - CHANDELIER_BURN, x + CHANDELIER_BURN),
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
    if r.feature == Some(Feature::HandHall) {
        // The lane runs straight between the doors (both at entry_x).
        let west = (r.entry_x - LANE / 2.0 - r.min.x).max(0.0);
        let east = (r.max.x - r.entry_x - LANE / 2.0).max(0.0);
        let half = (r.depth() / 2.0 - 2.2).max(1.0);
        return vec![Hazard { room: r.index, kind: HazardKind::HandHall { west, east }, z: (r.min.y + r.max.y) / 2.0, half, phase: 0.0 }];
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
        let kind = if roll < 0.3 && room_left >= 1.0 {
            HazardKind::Hands { east, reach: room_left.min(rng.range_f32(1.6, 3.2)) }
        } else if roll < 0.52 && room_left >= 1.0 {
            HazardKind::Fire { east, len: room_left.min(rng.range_f32(2.0, 4.5)) }
        } else if roll < 0.7 {
            HazardKind::Fallen { tilt: rng.range_f32(-0.05, 0.05) }
        } else if roll < 0.82 && width >= 2.0 * CHANDELIER_BURN + LANE + 0.4 {
            // Off to one side, leaving a lane on the other.
            let side = width - 2.0 * CHANDELIER_BURN - LANE;
            let off = rng.range_f32(0.2, side.max(0.2));
            let x = if east { r.max.x - CHANDELIER_BURN - off.min(side) } else { r.min.x + CHANDELIER_BURN + off.min(side) };
            HazardKind::Chandelier { x }
        } else if roll < 0.95 {
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
        "debris" => Some(0.9),
        "fallen" => Some(0.6),
        "chandelier" => Some(0.75),
        _ => None,
    }
}

/// All chase hazards on a floor.
pub fn floor_hazards(plan: &FloorPlan) -> Vec<Hazard> {
    chases(plan.last_room).flat_map(|(a, b)| a..=b).filter_map(|i| plan.room(i)).flat_map(|r| room_hazards(plan.seed, r)).collect()
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

/// One reaching arm (the arm model slides out of the wall).
#[derive(Component)]
struct Arm {
    wall: f32,
    dir: f32,
    reach: f32,
    /// Thickness scale (the hands are big).
    thick: f32,
    /// Hand hall: this arm's place in the travelling wave (seconds).
    wave: Option<f32>,
    z: f32,
    tilt: f32,
}

/// How far a hand-hall arm is out (0..1) and whether it's trembling just
/// before it lunges.
pub fn hall_out(t: f32, wave: f32) -> (f32, bool) {
    let k = ((t + wave) / HALL_PERIOD).rem_euclid(1.0);
    if k < 0.3 {
        (0.0, false)
    } else if k < 0.42 {
        (0.06, true)
    } else if k < 0.52 {
        ((k - 0.42) / 0.1, false)
    } else if k < 0.78 {
        (1.0, false)
    } else if k < 0.92 {
        (1.0 - (k - 0.78) / 0.14, false)
    } else {
        (0.0, false)
    }
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
    assets: Res<AssetServer>,
) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    let claw = crate::floor::scenery::prop(&assets, "claw_arm");
    let shelf = crate::floor::scenery::prop(&assets, "bookshelf");
    let books = crate::floor::scenery::prop(&assets, "books");
    let chandelier = crate::floor::scenery::prop(&assets, "chandelier");
    built.0.retain(|i| rt.loaded.contains(i));
    let rooms: Vec<u32> = chases(floor.0.last_room).flat_map(|(a, b)| a..=b).collect();
    for i in rooms {
        if built.0.contains(&i) || !rt.loaded.contains(&i) {
            continue;
        }
        let Some(r) = floor.0.room(i) else { continue };
        built.0.push(i);
        let ink = mats.add(StandardMaterial { base_color: Color::srgb(0.04, 0.02, 0.05), perceptual_roughness: 0.35, ..default() });
        let grate = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.11, 0.1), metallic: 0.8, perceptual_roughness: 0.5, ..default() });
        let flame_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.42, 0.08), emissive: LinearRgba::rgb(7.0, 2.0, 0.3), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
        let core_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.9, 0.45), emissive: LinearRgba::rgb(12.0, 9.0, 3.0), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
        let rock_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.35, 0.32, 0.3), perceptual_roughness: 0.95, ..default() });
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let blob = meshes.add(Sphere::new(0.5).mesh().uv(16, 10));
        let disc = meshes.add(Cylinder::new(1.0, 0.01));
        for h in room_hazards(floor.0.seed, r) {
            if std::env::var_os("HALLWAYS_WARP").is_some() {
                info!("QA hazard {:?} room {} z {:.1} x {:.1}..{:.1} entry_x {:.1}", h.kind, i, h.z, r.min.x, r.max.x, r.entry_x);
            }
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
                        for k in 0..4 {
                            let z = h.z - h.half + (k as f32 + 0.5) * (h.half * 2.0 / 4.0);
                            let y = 0.6 + rng.next_f32() * 1.2;
                            let len = reach * rng.range_f32(0.85, 1.0);
                            c.spawn((claw.clone(), Transform::from_xyz(wall - dir * 0.6, y, z), Arm { wall, dir, reach: len, thick: 1.8, wave: None, z, tilt: rng.range_f32(-0.35, 0.35) }));
                        }
                    });
                }
                HazardKind::HandHall { west, east } => {
                    let mut rng = GameRng::fork(r.seed, 78);
                    let lane = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.4), emissive: LinearRgba::rgb(2.2, 1.5, 0.6), unlit: true, ..default() });
                    root.with_children(|c| {
                        // Glowing edges of the safe lane, door to door.
                        for x in [r.entry_x - LANE / 2.0, r.entry_x + LANE / 2.0] {
                            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(lane.clone()), Transform::from_xyz(x, 0.012, h.z).with_scale(Vec3::new(0.06, 0.01, h.half * 2.0 + 1.6))));
                        }
                        for (wall, dir, reach) in [(r.min.x, 1.0f32, west), (r.max.x, -1.0, east)] {
                            // Ink streaming down the walls.
                            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(ink.clone()), Transform::from_xyz(wall + dir * 0.01, r.height / 2.0, h.z).with_scale(Vec3::new(0.02, r.height, h.half * 2.0 + 1.0))));
                            if reach < 0.3 {
                                continue;
                            }
                            let mut z = h.z + h.half;
                            while z > h.z - h.half {
                                for y in [rng.range_f32(0.5, 1.0), rng.range_f32(1.3, 2.0)] {
                                    let len = reach * rng.range_f32(0.92, 1.0);
                                    // The wave rolls down the hall toward the exit.
                                    let wave = -z * 0.45 + if dir > 0.0 { 0.0 } else { 0.6 } + rng.range_f32(0.0, 0.15);
                                    c.spawn((claw.clone(), Transform::from_xyz(wall - dir * 0.6, y, z), Arm { wall, dir, reach: len, thick: 1.7, wave: Some(wave), z: z + rng.range_f32(-0.15, 0.15), tilt: rng.range_f32(-0.3, 0.3) }));
                                }
                                z -= 0.85;
                            }
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
                HazardKind::Fallen { tilt } => {
                    let wood = mats.add(crate::world::materials::pbr_set(&assets, "dark_wood", Color::srgb(0.75, 0.65, 0.55)));
                    let paper = mats.add(StandardMaterial { base_color: Color::srgb(0.88, 0.85, 0.76), perceptual_roughness: 0.9, double_sided: true, cull_mode: None, ..default() });
                    let mut rng = GameRng::fork(r.seed, 79);
                    let w = r.width();
                    let cx = (r.min.x + r.max.x) / 2.0;
                    root.insert(crate::physics::Collider(crate::physics::Aabb {
                        min: Vec3::new(r.min.x, BEAM_BOTTOM, h.z - 1.1),
                        max: Vec3::new(r.max.x, r.height + 0.5, h.z + 0.95),
                    }));
                    root.with_children(|c| {
                        // The beam, wedged wall to wall (never lower than BEAM_BOTTOM).
                        let mid = BEAM_BOTTOM + 0.18 + tilt.abs() * w / 2.0;
                        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(cx, mid, h.z).with_rotation(Quat::from_rotation_z(tilt)).with_scale(Vec3::new(w + 0.1, 0.34, 0.4))));
                        // Toppled bookshelves lying across it, on their backs.
                        let n = (w / 1.4).floor().clamp(1.0, 4.0) as usize;
                        for k in 0..n {
                            let x = r.min.x + (k as f32 + 0.5) * w / n as f32 + rng.range_f32(-0.15, 0.15);
                            let yaw = rng.range_f32(-0.25, 0.25);
                            c.spawn((shelf.clone(), Transform::from_xyz(x, mid + 0.17, h.z - 0.85).with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(1.4)).with_scale(Vec3::splat(0.95))));
                        }
                        // Books and papers spilled on the floor on both sides.
                        for _ in 0..(3 + (w / 1.5) as usize) {
                            let x = rng.range_f32(r.min.x + 0.4, r.max.x - 0.4);
                            let z = h.z + rng.range_f32(-1.4, 1.4);
                            c.spawn((books.clone(), Transform::from_xyz(x, 0.0, z).with_rotation(Quat::from_rotation_y(rng.range_f32(0.0, 6.28)) * Quat::from_rotation_z(if rng.next_f32() < 0.5 { 1.57 } else { 0.0 })).with_scale(Vec3::splat(0.8))));
                        }
                        for _ in 0..12 {
                            let x = rng.range_f32(r.min.x + 0.3, r.max.x - 0.3);
                            let z = h.z + rng.range_f32(-1.8, 1.8);
                            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(paper.clone()), Transform::from_xyz(x, 0.006, z).with_rotation(Quat::from_rotation_y(rng.range_f32(0.0, 6.28))).with_scale(Vec3::new(0.21, 0.002, 0.3))));
                        }
                        // Rubble piled at the walls where the beam came down.
                        for x in [r.min.x + 0.3, r.max.x - 0.3] {
                            for _ in 0..3 {
                                c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(rock_mat.clone()), Transform::from_xyz(x + rng.range_f32(-0.2, 0.2), rng.range_f32(0.1, 0.3), h.z + rng.range_f32(-0.4, 0.4)).with_rotation(Quat::from_rotation_y(rng.range_f32(0.0, 3.0))).with_scale(Vec3::new(0.35, 0.25, 0.3))));
                            }
                        }
                    });
                }
                HazardKind::Chandelier { x } => {
                    root.insert(crate::physics::Collider(crate::physics::Aabb {
                        min: Vec3::new(x - CHANDELIER_SOLID, 0.0, h.z - CHANDELIER_SOLID),
                        max: Vec3::new(x + CHANDELIER_SOLID, 1.0, h.z + CHANDELIER_SOLID),
                    }));
                    let mut rng = GameRng::fork(r.seed, 80);
                    root.with_children(|c| {
                        // Crashed on its side, chain still hanging from the ceiling.
                        c.spawn((chandelier.clone(), Transform::from_xyz(x, 0.0, h.z).with_rotation(Quat::from_rotation_y(rng.range_f32(0.0, 6.28)) * Quat::from_rotation_z(0.5))));
                        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(grate.clone()), Transform::from_xyz(x, r.height - 0.6, h.z).with_scale(Vec3::new(0.03, 1.2, 0.03))));
                        for k in 0..10 {
                            let a = k as f32 / 10.0 * std::f32::consts::TAU;
                            let rad = rng.range_f32(0.3, CHANDELIER_BURN * 0.8);
                            let seed = k as f32 * 2.3;
                            let base = 0.6 + (k % 3) as f32 * 0.2;
                            let (fx, fz) = (x + a.cos() * rad, h.z + a.sin() * rad);
                            c.spawn((Mesh3d(blob.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(fx, 0.0, fz).with_scale(Vec3::new(0.28, 0.01, 0.28)), Flame { base, seed, core: false }));
                            c.spawn((Mesh3d(blob.clone()), MeshMaterial3d(core_mat.clone()), Transform::from_xyz(fx, 0.0, fz).with_scale(Vec3::new(0.14, 0.01, 0.14)), Flame { base: base * 0.55, seed: seed + 2.0, core: true }));
                        }
                        c.spawn((PointLight { color: Color::srgb(1.0, 0.5, 0.15), intensity: 0.0, range: 7.0, ..default() }, Transform::from_xyz(x, 0.9, h.z), FireLight));
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
                        place_arm(arm, &mut tf, out, false, t);
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
                    hurt = Some((35.0, "THE WALLS GRABBED YOU", "Hands reach out of the walls in waves during the chase. Keep to the far side, or slip past while they pull back."));
                }
            }
            HazardKind::HandHall { .. } => {
                let mut caught = false;
                let mut lunge_near = false;
                for c in children.iter() {
                    if let Ok((arm, mut tf)) = arms.get_mut(c) {
                        let (out, tremble) = if active { hall_out(t, arm.wave.unwrap_or(0.0)) } else { (0.0, false) };
                        place_arm(arm, &mut tf, out, tremble, t);
                        // A hand that's out catches you if you're between it and the wall.
                        if out > 0.45 && (arm.z - pp.y).abs() < 0.55 && p.translation.y < 1.6 {
                            let tip = arm.wall + arm.dir * arm.reach * out;
                            if (arm.dir > 0.0 && pp.x < tip - 0.1) || (arm.dir < 0.0 && pp.x > tip + 0.1) {
                                caught = true;
                            }
                            lunge_near = true;
                        }
                    }
                }
                if lunge_near && body.cooldown <= 0.0 {
                    body.cooldown = 1.2;
                    sfx.write(PlaySfx(Sfx::Whoosh));
                }
                if caught && active {
                    shake.add(0.9);
                    subs.write(Subtitle::danger("[The hands seize you and drag you into the wall!]"));
                    hurt = Some((1000.0, "THE HANDS TOOK YOU", "At the end of the chase, the walls are full of hands. Stay inside the glowing lane down the middle of the hall: they can't reach it."));
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
                    hurt = Some((12.0, "BURNED IN THE CHASE", "Fire vents flare in bursts during the chase. Watch for the flicker, then run past while they're out, or take the clear lane beside them."));
                }
            }
            HazardKind::Fallen { .. } => {
                // Tell anyone running at it standing up to crouch (once).
                if !body.done && pp.y > h.z && pp.y - h.z < 3.5 && pp.x > r.min.x && pp.x < r.max.x {
                    body.done = true;
                    subs.write(Subtitle::danger("[A beam and a bookshelf have come down!] CROUCH (C / Ctrl) to get under!"));
                }
            }
            HazardKind::Chandelier { x } => {
                // Always burning (the light flickers with the flames).
                for c in children.iter() {
                    if let Ok((flame, mut tf)) = flames.get_mut(c) {
                        let k = 0.75 + 0.25 * (t * 9.0 + flame.seed).sin() * (t * 5.3 + flame.seed * 2.0).cos();
                        let hgt = flame.base * k;
                        tf.scale = Vec3::new(if flame.core { 0.14 } else { 0.28 }, hgt, if flame.core { 0.14 } else { 0.28 });
                        tf.translation.y = hgt / 2.0;
                    }
                    if let Ok(mut l) = lights.get_mut(c) {
                        l.intensity = 220_000.0 * (0.8 + 0.2 * (t * 11.0).sin());
                    }
                }
                if Vec2::new(x, h.z).distance(pp) < CHANDELIER_BURN && p.translation.y < 1.0 && body.cooldown <= 0.0 {
                    body.cooldown = 1.0;
                    sfx.write(PlaySfx(Sfx::Whoosh));
                    subs.write(Subtitle::danger("[The burning chandelier scorches you!]"));
                    hurt = Some((15.0, "BURNED IN THE CHASE", "A fallen chandelier burns in the middle of the room. Keep to the open side and give the flames a wide berth."));
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
                            hurt = Some((35.0, "CRUSHED IN THE CHASE", "When the ceiling cracks during the chase, a shadow marks where it falls. Sidestep out of the shadow."));
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

/// Slides an arm model out of its wall: `out` 0 = hidden, 1 = full reach.
fn place_arm(arm: &Arm, tf: &mut Transform, out: f32, tremble: bool, t: f32) {
    // The model is ARM_LEN long along +X; it's stretched so that fully out
    // its fingertips reach `reach` from the wall, the rest hidden inside it.
    let len = arm.reach + 0.6;
    let sx = len / ARM_LEN;
    let ext = arm.reach * out;
    let shake = if tremble { (t * 47.0 + arm.z * 13.0).sin() * 0.04 } else { 0.0 };
    let wiggle = (t * 6.0 + arm.z * 7.0).sin() * 0.06 * out;
    tf.translation.x = arm.wall + arm.dir * (ext - len) + shake;
    tf.translation.z = arm.z;
    let face = Quat::from_rotation_y(if arm.dir > 0.0 { 0.0 } else { std::f32::consts::PI });
    tf.rotation = face * Quat::from_rotation_z(arm.tilt * 0.4 + wiggle) * Quat::from_rotation_x(arm.tilt + wiggle * 2.0);
    tf.scale = Vec3::new(sx, arm.thick, arm.thick);
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
        let mut kinds = [0; 5];
        for seed in 0..300 {
            let plan = generate(seed);
            for i in chases(plan.last_room).flat_map(|(a, b)| a..=b) {
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
                        HazardKind::Fallen { tilt } => {
                            kinds[3] += 1;
                            assert!(tilt.abs() < 0.06, "the beam stays high enough everywhere");
                        }
                        HazardKind::Chandelier { x } => {
                            kinds[4] += 1;
                            let free = (x - CHANDELIER_BURN - r.min.x).max(r.max.x - x - CHANDELIER_BURN);
                            assert!(free >= LANE - 1e-3, "seed {seed} room {i}: no way round the chandelier");
                        }
                        HazardKind::HandHall { west, east } => {
                            // The lane between the hands is free and lines up with both doors.
                            assert!(r.width() - west - east >= LANE - 1e-3, "seed {seed} room {i}: hand hall has no lane");
                            assert!((r.entry_x - r.exit_x).abs() < 1e-4, "seed {seed} room {i}: hand hall isn't straight");
                            assert!(west > 1.0 && east > 1.0, "seed {seed} room {i}: hands on both walls");
                            continue;
                        }
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
        assert!(kinds.iter().all(|&k| k > 60), "{kinds:?}");
    }

    #[test]
    fn every_chase_ends_in_a_hand_hall() {
        for seed in 0..60 {
            let plan = generate(seed);
            for (_, last) in chases(plan.last_room) {
                let r = plan.room(last).unwrap();
                assert_eq!(r.feature, Some(Feature::HandHall), "seed {seed}");
                assert!(r.dividers().is_empty() && r.containers == 0 && !r.hiding, "seed {seed}: the hall is bare");
                assert!(matches!(room_hazards(seed, r)[0].kind, HazardKind::HandHall { .. }));
            }
        }
        // The hands tremble, lunge, hold and pull back.
        let samples: Vec<(f32, bool)> = (0..48).map(|i| hall_out(i as f32 * HALL_PERIOD / 48.0, 0.0)).collect();
        assert!(samples.iter().any(|s| s.1) && samples.iter().any(|s| s.0 >= 1.0) && samples.iter().any(|s| s.0 == 0.0));
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
