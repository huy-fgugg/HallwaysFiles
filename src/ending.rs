//! The ending cutscene's set and actors. When the final door opens, an exit
//! lift is built beyond it; your character (and online teammates) walk in,
//! the gate closes, the lift rises, something with long claws tears through
//! the roof, the cable snaps and the car drops. Cut to black, then victory.
//!
//! Everything is anchored to the far side of the final door, which moves
//! with every seed; camera shots use `CamPoint::Anchor`.

use bevy::prelude::*;
use serde::Deserialize;

use crate::cutscene::{ActiveCutscene, CutsceneEnded, EndAction, PlayCutscene, ENDING};
use crate::floor::{Floor, FloorRuntime};
use crate::net::{NetSession, Peers};
use crate::state::{AppState, Overlay};
use crate::viewmodel::BodyKit;

/// World position of the ending set (far side of the final door, floor level).
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct CutsceneAnchor(pub Vec3);

/// Timeline commands for the ending set (used in `03_ending/cutscene.ron`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum EndingCue {
    /// Build the exit lift and place the characters at the door.
    Build,
    /// Characters walk into the lift and turn around.
    Walk { seconds: f32 },
    CloseGate { seconds: f32 },
    Rise { meters: f32, seconds: f32 },
    /// Claws tear through the roof; red eyes in the shaft.
    Attack,
    /// Characters flail.
    Panic,
    /// A clawed arm lunges out of the dark and slashes the cable.
    Swipe,
    /// The cable parts: sparks, the top end whips away upward.
    Cut,
    /// The cable snaps and the car falls.
    Snap,
}

#[derive(Component)]
struct EndingSet;
#[derive(Component)]
struct LiftCar {
    rise: Option<(f32, f32, f32, f32)>, // from, to, t, seconds
    fall: Option<f32>,                  // speed
}
#[derive(Component)]
struct Gate {
    t: f32,
    seconds: f32,
    closing: bool,
}
#[derive(Component)]
struct Claw {
    t: f32,
    delay: f32,
}
#[derive(Component)]
struct Eye;
/// The lift cable (child of the car until it is cut).
#[derive(Component)]
struct Cable;
/// The severed top half of the cable, whipping upward.
#[derive(Component)]
struct CableTop {
    v: f32,
}
/// The slashing arm.
#[derive(Component)]
struct SlashArm {
    t: f32,
    start: Vec3,
}
#[derive(Component)]
struct Spark {
    v: Vec3,
    life: f32,
}

#[derive(Component)]
struct Actor {
    from: Vec3,
    to: Vec3,
    walk: Option<(f32, f32)>, // t, seconds
    panic: bool,
    phase: f32,
    /// Offset from the car once inside (actors ride the lift).
    riding: bool,
}

use crate::avatar::Limb;

/// Lift car position relative to the anchor.
const CAR: Vec3 = Vec3::new(0.0, 0.0, -4.4);

pub struct EndingPlugin;

impl Plugin for EndingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CutsceneAnchor>()
            .add_systems(Update, (run_cues, animate_set, animate_actors, animate_cut, victory).chain().run_if(in_state(AppState::Playing)))
            .add_systems(Update, capture_frames.run_if(in_state(AppState::Playing)));
    }
}

/// Called by the floor when the final door opens.
pub fn start_ending(anchor: &mut CutsceneAnchor, play: &mut MessageWriter<PlayCutscene>, floor: &Floor, rt: &FloorRuntime) {
    let Some(r) = floor.0.room(floor.0.last_room) else { return };
    let _ = rt;
    anchor.0 = Vec3::new(r.exit_x, 0.0, r.min.y - crate::floor::plan::WALL_T);
    play.write(PlayCutscene(ENDING.into()));
}

const SHIRTS: [Color; 6] = [
    Color::srgb(0.14, 0.38, 0.58),
    Color::srgb(0.62, 0.16, 0.14),
    Color::srgb(0.2, 0.46, 0.22),
    Color::srgb(0.58, 0.46, 0.12),
    Color::srgb(0.42, 0.2, 0.52),
    Color::srgb(0.12, 0.48, 0.48),
];

#[allow(clippy::too_many_arguments)]
fn run_cues(
    mut commands: Commands,
    mut active: ResMut<ActiveCutscene>,
    anchor: Res<CutsceneAnchor>,
    session: Res<NetSession>,
    peers: Res<Peers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut cars: Query<&mut LiftCar>,
    cars_tf: Query<&GlobalTransform, With<LiftCar>>,
    cables: Query<Entity, With<Cable>>,
    mut gates: Query<&mut Gate>,
    mut actors: Query<&mut Actor>,
    sets: Query<Entity, With<EndingSet>>,
    stand_in: Query<Entity, With<crate::floor::build::FinalVestibule>>,
) {
    let mut queue = std::mem::take(&mut active.ending).into_iter();
    while let Some((cue, instant)) = queue.next() {
        match cue {
            EndingCue::Build => {
                for e in sets.iter().chain(stand_in.iter()) {
                    commands.entity(e).try_despawn();
                }
                build_set(&mut commands, &mut meshes, &mut mats, anchor.0);
                // You plus everyone else in the room.
                let mut ids: Vec<u8> = vec![if session.online() { session.my_id } else { 0 }];
                ids.extend(peers.0.keys().copied());
                ids.sort();
                let n = ids.len().min(5);
                for (i, id) in ids.into_iter().take(5).enumerate() {
                    let lane = (i as f32 - (n as f32 - 1.0) / 2.0) * 0.6;
                    let from = anchor.0 + Vec3::new(lane, 0.0, 1.6 + i as f32 * 0.5);
                    let to = anchor.0 + CAR + Vec3::new(lane, 0.0, -0.3 + (i % 2) as f32 * 0.25);
                    spawn_actor(&mut commands, &mut meshes, &mut mats, id, SHIRTS[id as usize % SHIRTS.len()], from, to);
                }
                // The set only exists next frame: later cues (walk, gate...)
                // must wait for it, even when a slow frame fires several at once.
                active.ending.extend(queue);
                return;
            }
            EndingCue::Walk { seconds } => {
                for mut a in &mut actors {
                    a.walk = if instant { None } else { Some((0.0, seconds)) };
                    a.riding = true;
                }
            }
            EndingCue::CloseGate { seconds } => {
                for mut g in &mut gates {
                    g.closing = true;
                    g.t = if instant { seconds } else { 0.0 };
                    g.seconds = seconds;
                }
            }
            EndingCue::Rise { meters, seconds } => {
                for mut c in &mut cars {
                    c.rise = Some((0.0, meters, if instant { seconds } else { 0.0 }, seconds));
                }
            }
            EndingCue::Attack => {
                if let Ok(car) = cars.single().map(|_| ()) {
                    let _ = car;
                }
                spawn_monster(&mut commands, &mut meshes, &mut mats);
            }
            EndingCue::Panic => {
                for mut a in &mut actors {
                    a.panic = true;
                }
            }
            EndingCue::Swipe => spawn_slash(&mut commands, &mut meshes, &mut mats, &cars_tf),
            EndingCue::Cut => cut_cable(&mut commands, &mut meshes, &mut mats, &cars_tf, &cables),
            EndingCue::Snap => {
                for mut c in &mut cars {
                    c.rise = None;
                    c.fall = Some(0.0);
                }
            }
        }
    }
}

fn build_set(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, a: Vec3) {
    let m = |c: Color, rough: f32, metal: f32| StandardMaterial { base_color: c, perceptual_roughness: rough, metallic: metal, ..default() };
    let concrete = mats.add(m(Color::srgb(0.18, 0.17, 0.16), 0.95, 0.0));
    let brass = mats.add(m(Color::srgb(0.7, 0.52, 0.25), 0.35, 1.0));
    let wood = mats.add(m(Color::srgb(0.25, 0.13, 0.07), 0.6, 0.0));
    let floor_mat = mats.add(m(Color::srgb(0.12, 0.1, 0.09), 0.8, 0.0));
    let iron = mats.add(m(Color::srgb(0.08, 0.08, 0.09), 0.5, 0.8));
    let lamp = mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(8.0, 6.0, 3.0), ..default() });
    let panel = mats.add(m(Color::srgb(0.32, 0.17, 0.09), 0.5, 0.0));
    let dark_wood = mats.add(m(Color::srgb(0.13, 0.07, 0.04), 0.55, 0.0));
    let mirror = mats.add(StandardMaterial { base_color: Color::srgb(0.6, 0.62, 0.66), perceptual_roughness: 0.04, metallic: 1.0, ..default() });
    let tile_a = mats.add(m(Color::srgb(0.82, 0.78, 0.7), 0.3, 0.0));
    let tile_b = mats.add(m(Color::srgb(0.08, 0.07, 0.07), 0.3, 0.0));
    let amber = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.7, 0.3), emissive: LinearRgba::rgb(3.0, 1.6, 0.4), ..default() });
    let red_glow = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.2, 0.1), emissive: LinearRgba::rgb(4.0, 0.4, 0.2), ..default() });
    let dial = mats.add(m(Color::srgb(0.92, 0.88, 0.76), 0.6, 0.0));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let cyl = meshes.add(Cylinder::new(0.5, 1.0));
    let boxed = |c: &mut ChildSpawnerCommands, mat: &Handle<StandardMaterial>, lo: Vec3, hi: Vec3| {
        c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation((lo + hi) / 2.0).with_scale((hi - lo).abs())));
    };
    // Landing corridor and the shaft around the car (static).
    commands
        .spawn((EndingSet, Transform::from_translation(a), Visibility::default(), DespawnOnExit(AppState::Playing), Name::new("Ending set")))
        .with_children(|s| {
            boxed(s, &floor_mat, Vec3::new(-1.6, -0.1, -2.7), Vec3::new(1.6, 0.0, 0.0));
            boxed(s, &concrete, Vec3::new(-1.8, 0.0, -2.7), Vec3::new(-1.6, 3.2, 0.0));
            boxed(s, &concrete, Vec3::new(1.6, 0.0, -2.7), Vec3::new(1.8, 3.2, 0.0));
            boxed(s, &concrete, Vec3::new(-1.8, 3.2, -2.7), Vec3::new(1.8, 3.4, 0.0));
            // Shaft: tall walls and a deep pit.
            boxed(s, &concrete, Vec3::new(-1.8, -14.0, -6.4), Vec3::new(1.8, 30.0, -6.2));
            boxed(s, &concrete, Vec3::new(-1.8, -14.0, -6.2), Vec3::new(-1.6, 30.0, -2.7));
            boxed(s, &concrete, Vec3::new(1.6, -14.0, -6.2), Vec3::new(1.8, 30.0, -2.7));
            boxed(s, &concrete, Vec3::new(-1.8, 3.0, -2.75), Vec3::new(1.8, 30.0, -2.6));
            boxed(s, &concrete, Vec3::new(-1.8, -14.0, -2.75), Vec3::new(1.8, -0.1, -2.6));
            boxed(s, &floor_mat, Vec3::new(-1.6, -14.0, -6.2), Vec3::new(1.6, -13.9, -2.7));
            // Guide rails on the shaft walls and a pulley wheel high above.
            for x in [-1.55f32, 1.55] {
                boxed(s, &iron, Vec3::new(x - 0.04, -14.0, -4.5), Vec3::new(x + 0.04, 30.0, -4.3));
            }
            s.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 29.0, -4.4).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(1.2, 0.2, 1.2))));
            // Caged lights every few metres up the shaft (they slide past as you rise).
            for k in 0..6 {
                let y = 2.0 + k as f32 * 3.0;
                s.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(lamp.clone()), Transform::from_xyz(-1.5, y, -3.0).with_scale(Vec3::new(0.1, 0.14, 0.1))));
                s.spawn((PointLight { color: Color::srgb(1.0, 0.75, 0.45), intensity: 25_000.0, range: 4.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(-1.35, y, -3.1)));
            }
            // A caged bulb over the landing.
            s.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(lamp.clone()), Transform::from_xyz(0.0, 3.05, -1.3).with_scale(Vec3::new(0.14, 0.18, 0.14))));
            s.spawn((PointLight { color: Color::srgb(1.0, 0.8, 0.55), intensity: 120_000.0, range: 10.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 2.8, -1.3)));
        });
    // The car itself.
    commands
        .spawn((
            EndingSet,
            LiftCar { rise: None, fall: None },
            Transform::from_translation(a + CAR),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Name::new("Exit lift"),
        ))
        .with_children(|c| {
            // Checkerboard floor.
            for i in 0..6 {
                for j in 0..6 {
                    let mat = if (i + j) % 2 == 0 { &tile_a } else { &tile_b };
                    let (x0, z0) = (-1.2 + i as f32 * 0.4, -1.3 + j as f32 * (2.6 / 6.0));
                    boxed(c, mat, Vec3::new(x0, -0.02, z0), Vec3::new(x0 + 0.4, 0.0, z0 + 2.6 / 6.0));
                }
            }
            boxed(c, &wood, Vec3::new(-1.3, -0.2, -1.4), Vec3::new(1.3, -0.02, 1.4));
            // Walls: dark wood with raised panels and brass trim.
            boxed(c, &wood, Vec3::new(-1.3, 0.0, -1.4), Vec3::new(1.3, 2.7, -1.3));
            boxed(c, &wood, Vec3::new(-1.3, 0.0, -1.4), Vec3::new(-1.2, 2.7, 1.4));
            boxed(c, &wood, Vec3::new(1.2, 0.0, -1.4), Vec3::new(1.3, 2.7, 1.4));
            for (y0, y1) in [(0.12f32, 0.85f32), (1.1, 2.5)] {
                // Back wall: two panels either side of a mirror.
                for x in [-0.85f32, 0.85] {
                    boxed(c, &panel, Vec3::new(x - 0.3, y0, -1.3), Vec3::new(x + 0.3, y1, -1.27));
                }
                // Side walls: three panels each.
                for z in [-0.85f32, 0.0, 0.85] {
                    boxed(c, &panel, Vec3::new(-1.2, y0, z - 0.33), Vec3::new(-1.17, y1, z + 0.33));
                    boxed(c, &panel, Vec3::new(1.17, y0, z - 0.33), Vec3::new(1.2, y1, z + 0.33));
                }
            }
            boxed(c, &mirror, Vec3::new(-0.45, 1.1, -1.3), Vec3::new(0.45, 2.5, -1.285));
            for y in [0.02f32, 0.98, 2.56] {
                boxed(c, &brass, Vec3::new(-1.2, y, -1.3), Vec3::new(1.2, y + 0.05, -1.26));
                boxed(c, &brass, Vec3::new(-1.2, y, -1.3), Vec3::new(-1.16, y + 0.05, 1.4));
                boxed(c, &brass, Vec3::new(1.16, y, -1.3), Vec3::new(1.2, y + 0.05, 1.4));
            }
            // Corner posts with brass caps.
            for (x, z) in [(-1.25f32, -1.35f32), (1.25, -1.35), (-1.25, 1.35), (1.25, 1.35)] {
                boxed(c, &dark_wood, Vec3::new(x - 0.07, 0.0, z - 0.07), Vec3::new(x + 0.07, 2.7, z + 0.07));
                boxed(c, &brass, Vec3::new(x - 0.09, 2.62, z - 0.09), Vec3::new(x + 0.09, 2.7, z + 0.09));
            }
            // Coffered ceiling with a brass light fixture.
            boxed(c, &brass, Vec3::new(-1.3, 2.7, -1.4), Vec3::new(1.3, 2.8, 1.4));
            for x in [-0.6f32, 0.0, 0.6] {
                boxed(c, &dark_wood, Vec3::new(x - 0.04, 2.6, -1.3), Vec3::new(x + 0.04, 2.7, 1.3));
            }
            for z in [-0.65f32, 0.65] {
                boxed(c, &dark_wood, Vec3::new(-1.2, 2.6, z - 0.04), Vec3::new(1.2, 2.7, z + 0.04));
            }
            c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(0.0, 2.62, 0.0).with_scale(Vec3::new(0.46, 0.06, 0.46))));
            c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(lamp.clone()), Transform::from_xyz(0.0, 2.56, 0.0).with_scale(Vec3::new(0.32, 0.08, 0.32))));
            c.spawn((PointLight { color: Color::srgb(1.0, 0.82, 0.6), intensity: 90_000.0, range: 6.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 2.4, 0.0)));
            // Brass handrails on brackets.
            for x in [-1.12f32, 1.12] {
                boxed(c, &brass, Vec3::new(x - 0.025, 0.95, -1.2), Vec3::new(x + 0.025, 1.0, 1.2));
                for z in [-1.0f32, 0.0, 1.0] {
                    boxed(c, &brass, Vec3::new(x.min(x * 1.06), 0.96, z - 0.015), Vec3::new(x.max(x * 1.06), 0.99, z + 0.015));
                }
            }
            boxed(c, &brass, Vec3::new(-1.1, 0.95, -1.24), Vec3::new(1.1, 1.0, -1.19));
            // Button panel beside the gate: brass plate, glowing buttons.
            boxed(c, &brass, Vec3::new(1.14, 0.9, 1.0), Vec3::new(1.17, 1.6, 1.25));
            for k in 0..5 {
                let mat = if k == 4 { &red_glow } else { &amber };
                c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(1.13, 1.0 + k as f32 * 0.12, 1.12).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.05, 0.02, 0.05))));
            }
            // Floor indicator over the gate: an arc of numbers and a needle.
            boxed(c, &brass, Vec3::new(-0.45, 2.42, 1.33), Vec3::new(0.45, 2.66, 1.36));
            c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(dial.clone()), Transform::from_xyz(0.0, 2.54, 1.37).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.2, 0.01, 0.2))));
            c.spawn((Mesh3d(cube.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.03, 2.56, 1.385).with_rotation(Quat::from_rotation_z(-0.7)).with_scale(Vec3::new(0.01, 0.08, 0.005))));
            // Machinery on the roof: a winch housing and the cable up into the dark.
            boxed(c, &iron, Vec3::new(-0.35, 2.8, -0.3), Vec3::new(0.35, 3.05, 0.3));
            c.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 3.15, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.3, 0.5, 0.3))));
            c.spawn((Cable, Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 16.4, 0.0).with_scale(Vec3::new(0.06, 27.0, 0.06))));
            // Folding gate (starts open, raised into the header).
            c.spawn((Gate { t: 0.0, seconds: 1.0, closing: false }, Transform::from_xyz(0.0, 2.3, 1.36), Visibility::default())).with_children(|g| {
                for i in 0..13 {
                    let x = -1.2 + i as f32 * 0.2;
                    g.spawn((Mesh3d(cube.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(x, 1.3, 0.0).with_scale(Vec3::new(0.025, 2.6, 0.025))));
                }
                for y in [0.15f32, 1.3, 2.45] {
                    g.spawn((Mesh3d(cube.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(0.0, y, 0.0).with_scale(Vec3::new(2.45, 0.03, 0.03))));
                }
                // Criss-cross lattice between the bars.
                for i in 0..6 {
                    let x = -1.0 + i as f32 * 0.4;
                    for (y, a) in [(0.72f32, 0.75f32), (0.72, -0.75), (1.88, 0.75), (1.88, -0.75)] {
                        g.spawn((Mesh3d(cube.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(x, y, 0.01).with_rotation(Quat::from_rotation_z(a)).with_scale(Vec3::new(0.018, 1.45, 0.018))));
                    }
                }
            });
        });
}

/// A character in a coloured shirt with jointed arms and legs (same build
/// as online teammates).
fn spawn_actor(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, id: u8, shirt: Color, from: Vec3, to: Vec3) {
    // The same body as players in game (see avatar::build_body).
    let kit = BodyKit::new(mats, meshes, shirt);
    let bm = crate::avatar::BodyMats::new(mats, id);
    let dir = to - from;
    commands
        .spawn((
            EndingSet,
            Actor { from, to, walk: None, panic: false, phase: 0.0, riding: false },
            Transform::from_translation(from).with_rotation(Quat::from_rotation_y((-dir.x).atan2(-dir.z))),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
        ))
        .with_children(|root| crate::avatar::build_body(root, &kit, &bm));
}

/// Where the cable is cut: above the car roof (world space).
fn cut_point(cars: &Query<&GlobalTransform, With<LiftCar>>) -> Option<Vec3> {
    cars.iter().next().map(|g| g.translation() + Vec3::Y * 3.7)
}

/// A long black arm with three hooked claws, lunging in from the side of the
/// shaft to slash across the cable.
fn spawn_slash(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, cars: &Query<&GlobalTransform, With<LiftCar>>) {
    let Some(p) = cut_point(cars) else { return };
    let skin = mats.add(StandardMaterial { base_color: Color::srgb(0.04, 0.03, 0.03), perceptual_roughness: 0.6, ..default() });
    let claw = mats.add(StandardMaterial { base_color: Color::srgb(0.75, 0.7, 0.62), perceptual_roughness: 0.3, ..default() });
    let cyl = meshes.add(Cylinder::new(0.5, 1.0));
    let cone = meshes.add(Cone { radius: 0.5, height: 1.0 });
    let sph = meshes.add(Sphere::new(0.5).mesh().uv(12, 8));
    commands
        .spawn((EndingSet, SlashArm { t: 0.0, start: p + Vec3::new(-2.6, 0.3, 0.0) }, Transform::from_translation(p + Vec3::new(-2.6, 0.3, 0.0)), Visibility::default(), DespawnOnExit(AppState::Playing)))
        .with_children(|a| {
            // Forearm along +X, knuckled wrist, three long hooked claws.
            a.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(-0.6, 0.0, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.22, 1.6, 0.22))));
            a.spawn((Mesh3d(sph.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.25, 0.0, 0.0).with_scale(Vec3::new(0.36, 0.28, 0.4))));
            for (i, z) in [-0.12f32, 0.0, 0.12].into_iter().enumerate() {
                a.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.48, 0.02, z).with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2 + 0.3)).with_scale(Vec3::new(0.06, 0.32, 0.06))));
                a.spawn((Mesh3d(cone.clone()), MeshMaterial3d(claw.clone()), Transform::from_xyz(0.78, -0.08 - i as f32 * 0.02, z).with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2 - 0.5)).with_scale(Vec3::new(0.05, 0.42, 0.05))));
            }
        });
}

/// The cable parts at the cut point: a short stub stays on the car, the top
/// half whips upward, sparks fly and a red flash lights the shaft.
fn cut_cable(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    cars: &Query<&GlobalTransform, With<LiftCar>>,
    cables: &Query<Entity, With<Cable>>,
) {
    let Some(p) = cut_point(cars) else { return };
    let iron = mats.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.09), perceptual_roughness: 0.5, metallic: 0.8, ..default() });
    let spark = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.4), emissive: LinearRgba::rgb(20.0, 10.0, 3.0), ..default() });
    let cyl = meshes.add(Cylinder::new(0.5, 1.0));
    let dot = meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap());
    for e in cables {
        // The car keeps a frayed stub.
        commands.entity(e).try_insert(Transform::from_xyz(0.0, 3.25, 0.0).with_scale(Vec3::new(0.06, 0.9, 0.06)));
    }
    commands.spawn((EndingSet, CableTop { v: 6.0 }, Mesh3d(cyl), MeshMaterial3d(iron), Transform::from_translation(p + Vec3::Y * 13.5).with_scale(Vec3::new(0.06, 27.0, 0.06)), DespawnOnExit(AppState::Playing)));
    let mut rng = crate::rng::GameRng::new(0xC0FFEE);
    for _ in 0..28 {
        let v = Vec3::new(rng.range_f32(-1.0, 1.0), rng.range_f32(-0.2, 1.2), rng.range_f32(-1.0, 1.0)).normalize_or_zero() * rng.range_f32(2.0, 6.0);
        commands.spawn((EndingSet, Spark { v, life: rng.range_f32(0.4, 0.9) }, Mesh3d(dot.clone()), MeshMaterial3d(spark.clone()), Transform::from_translation(p).with_scale(Vec3::splat(0.04)), DespawnOnExit(AppState::Playing)));
    }
    commands.spawn((EndingSet, Spark { v: Vec3::ZERO, life: 0.25 }, PointLight { color: Color::srgb(1.0, 0.6, 0.3), intensity: 400_000.0, range: 10.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(p), DespawnOnExit(AppState::Playing)));
}

/// The monster: three long black claws punching down through the roof and a
/// pair of red eyes in the dark shaft above (children of the car).
fn spawn_monster(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) {
    let claw_mat = mats.add(StandardMaterial { base_color: Color::srgb(0.03, 0.02, 0.02), perceptual_roughness: 0.3, metallic: 0.4, ..default() });
    let eye_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.1, 0.05), emissive: LinearRgba::rgb(14.0, 1.0, 0.4), ..default() });
    let cone = meshes.add(Cone { radius: 0.5, height: 1.0 });
    let sph = meshes.add(Sphere::new(0.5).mesh().uv(12, 8));
    commands.queue(move |world: &mut World| {
        let Some(car) = world.query_filtered::<Entity, With<LiftCar>>().iter(world).next() else { return };
        world.entity_mut(car).with_children(|c| {
            for (i, x) in [-0.45f32, 0.0, 0.45].into_iter().enumerate() {
                c.spawn((
                    Claw { t: 0.0, delay: i as f32 * 0.12 },
                    Mesh3d(cone.clone()),
                    MeshMaterial3d(claw_mat.clone()),
                    Transform::from_xyz(x, 3.6, 0.3 - i as f32 * 0.2).with_rotation(Quat::from_rotation_x(std::f32::consts::PI) * Quat::from_rotation_z(x * 0.4)).with_scale(Vec3::new(0.09, 1.3, 0.09)),
                ));
            }
            for x in [-0.22f32, 0.22] {
                c.spawn((Eye, Mesh3d(sph.clone()), MeshMaterial3d(eye_mat.clone()), Transform::from_xyz(x, 5.6, 0.2).with_scale(Vec3::splat(0.16))));
            }
            c.spawn((PointLight { color: Color::srgb(1.0, 0.15, 0.08), intensity: 160_000.0, range: 8.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 4.6, 0.2)));
        });
    });
}

#[allow(clippy::type_complexity)]
fn animate_set(
    time: Res<Time>,
    anchor: Res<CutsceneAnchor>,
    mut cars: Query<(&mut LiftCar, &mut Transform), (Without<Gate>, Without<Claw>, Without<Actor>)>,
    mut gates: Query<(&mut Gate, &mut Transform), (Without<LiftCar>, Without<Claw>, Without<Actor>)>,
    mut claws: Query<(&mut Claw, &mut Transform), (Without<LiftCar>, Without<Gate>, Without<Actor>)>,
    mut actors: Query<(&Actor, &mut Transform), (Without<LiftCar>, Without<Gate>, Without<Claw>)>,
) {
    let dt = time.delta_secs();
    let mut car_y = None;
    for (mut c, mut tf) in &mut cars {
        let base = anchor.0 + CAR;
        if let Some((from, to, t, secs)) = c.rise.as_mut() {
            *t = (*t + dt).min(*secs);
            let k = (*t / secs.max(0.01)).clamp(0.0, 1.0);
            let k = k * k * (3.0 - 2.0 * k);
            tf.translation = base + Vec3::Y * (*from + (*to - *from) * k);
        }
        if let Some(v) = c.fall.as_mut() {
            *v += 22.0 * dt;
            tf.translation.y -= *v * dt;
            // A violent wobble as it drops.
            tf.rotation = Quat::from_rotation_z((time.elapsed_secs() * 31.0).sin() * 0.03);
        }
        car_y = Some(tf.translation.y);
    }
    for (mut g, mut tf) in &mut gates {
        if g.closing {
            g.t = (g.t + dt).min(g.seconds);
        }
        let k = if g.closing { g.t / g.seconds.max(0.01) } else { 0.0 };
        // Open = folded up in the header; closed = down across the doorway.
        tf.translation.y = 2.3 * (1.0 - k);
        tf.scale = Vec3::new(1.0, 0.08 + 0.92 * k, 1.0);
    }
    for (mut cl, mut tf) in &mut claws {
        cl.t += dt;
        let k = ((cl.t - cl.delay) / 0.25).clamp(0.0, 1.0);
        tf.translation.y = 3.6 - 1.3 * k + (cl.t * 20.0).sin() * 0.02 * k;
    }
    // Riders move up and down with the car.
    if let Some(y) = car_y {
        for (a, mut tf) in &mut actors {
            if a.riding && a.walk.is_none_or(|(t, s)| t >= s) {
                tf.translation.y = y - anchor.0.y;
            }
        }
    }
}

fn animate_actors(time: Res<Time>, mut actors: Query<(&mut Actor, &mut Transform, &Children)>, children: Query<&Children>, mut limbs: Query<(&Limb, &mut Transform), Without<Actor>>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut a, mut tf, kids) in &mut actors {
        let mut walking = false;
        if let Some((wt, secs)) = a.walk {
            let nt = (wt + dt).min(secs);
            a.walk = Some((nt, secs));
            let k = nt / secs.max(0.01);
            let p = a.from.lerp(a.to, k);
            tf.translation.x = p.x;
            tf.translation.z = p.z;
            walking = k < 1.0;
            if !walking {
                // Inside: turn to face the gate.
                tf.rotation = tf.rotation.slerp(Quat::IDENTITY * Quat::from_rotation_y(std::f32::consts::PI), 1.0);
            }
        }
        if walking {
            a.phase += dt * 7.0;
        }
        let swing = if walking { a.phase.sin() * 0.6 } else { 0.0 };
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok(ch) = children.get(e) {
                stack.extend(ch.iter());
            }
            let Ok((limb, mut lt)) = limbs.get_mut(e) else { continue };
            lt.rotation = match *limb {
                Limb::Hips => Quat::IDENTITY,
                Limb::Thigh(r) => Quat::from_rotation_x(if r { swing } else { -swing }),
                Limb::Shin(r) => Quat::from_rotation_x(if walking { ((if r { a.phase } else { -a.phase }).sin()).max(0.0) * 0.7 } else { 0.0 }),
                Limb::Arm(r) => {
                    if a.panic {
                        // Arms thrown up, flailing.
                        Quat::from_rotation_x(-2.6 + (t * 14.0 + if r { 0.0 } else { 1.7 }).sin() * 0.35) * Quat::from_rotation_z(if r { 0.35 } else { -0.35 })
                    } else {
                        Quat::from_rotation_x(if r { -swing } else { swing } * 0.8) * Quat::from_rotation_z(if r { 0.08 } else { -0.08 })
                    }
                }
                Limb::Forearm(_) => Quat::from_rotation_x(if a.panic { -0.4 } else { -0.2 }),
                Limb::Head => {
                    if a.panic {
                        Quat::from_rotation_x(-0.6 + (t * 9.0).sin() * 0.1)
                    } else {
                        Quat::from_rotation_y((t * 0.8 + a.from.x).sin() * 0.25)
                    }
                }
            };
        }
    }
}

#[allow(clippy::type_complexity)]
fn animate_cut(
    mut commands: Commands,
    time: Res<Time>,
    mut arms: Query<(&mut SlashArm, &mut Transform), (Without<CableTop>, Without<Spark>)>,
    mut tops: Query<(&mut CableTop, &mut Transform), (Without<SlashArm>, Without<Spark>)>,
    mut sparks: Query<(Entity, &mut Spark, &mut Transform), (Without<SlashArm>, Without<CableTop>)>,
) {
    let dt = time.delta_secs();
    for (mut a, mut tf) in &mut arms {
        a.t += dt;
        let k = a.t;
        // Wind up (slight pull back), a fast slash across the cable, then on
        // through and back into the dark.
        let (dx, dy, roll) = if k < 0.3 {
            (-0.3 * (k / 0.3), 0.25 * (k / 0.3), -0.5)
        } else if k < 0.55 {
            let u = (k - 0.3) / 0.25;
            (-0.3 + 3.4 * u, 0.25 - 0.55 * u, 0.6)
        } else {
            let u = ((k - 0.55) / 0.6).min(1.0);
            (3.1 - 3.5 * u, -0.3 + 0.6 * u, 0.2)
        };
        tf.translation = a.start + Vec3::new(dx, dy, 0.0);
        tf.rotation = Quat::from_rotation_z(roll) * Quat::from_rotation_y(-0.25);
    }
    for (mut c, mut tf) in &mut tops {
        c.v += 10.0 * dt;
        tf.translation.y += c.v * dt;
        tf.rotation = Quat::from_rotation_z((time.elapsed_secs() * 13.0).sin() * 0.08);
    }
    for (e, mut s, mut tf) in &mut sparks {
        s.life -= dt;
        s.v.y -= 9.8 * dt;
        tf.translation += s.v * dt;
        if s.life <= 0.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// When the ending finishes: the victory (results) screen.
fn victory(mut ended: MessageReader<CutsceneEnded>, mut next: ResMut<NextState<Overlay>>, theater: Res<crate::theater::Theater>) {
    for e in ended.read() {
        // Watching from the Cutscenes menu goes back to the menu instead.
        if e.end == EndAction::Victory && !theater.active() {
            next.set(Overlay::Summary);
        }
    }
}

/// QA: with `HALLWAYS_CAPTURE` (or `HALLWAYS_CUTSCENE_SHOTS`) set to a folder,
/// photographs key moments of each cutscene as it plays.
fn capture_frames(mut commands: Commands, active: Res<ActiveCutscene>, mut next_shot: Local<usize>) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let moments: &[(f32, &str)] = match active.id.as_str() {
        "01_intro" => &[(1.5, "intro_1_descent"), (5.5, "intro_2_dial"), (8.3, "intro_3_gate_opens"), (10.6, "intro_4_title"), (13.2, "intro_5_step_out")],
        "02_the_hollow" => &[(2.0, "hollow_1"), (5.0, "hollow_2"), (8.0, "hollow_3"), (11.5, "hollow_4"), (14.5, "hollow_5")],
        ENDING => &[
            (1.6, "ending_1_walk_through_door"),
            (4.6, "ending_2_into_the_lift"),
            (7.6, "ending_3_gate_closes"),
            (11.2, "ending_4_going_up"),
            (13.9, "ending_5_claws"),
            (15.05, "ending_6_claw_cuts_cable"),
            (16.4, "ending_7_lift_drops"),
            (19.2, "ending_8_black_title"),
        ],
        _ => &[],
    };
    let Some(dir) = std::env::var("HALLWAYS_CUTSCENE_SHOTS").ok().or_else(|| std::env::var("HALLWAYS_CAPTURE").ok()) else { return };
    if !active.is_playing() {
        *next_shot = 0;
        return;
    }
    if let Some((t, name)) = moments.get(*next_shot) {
        if active.t >= *t {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{dir}/{name}.png")));
            *next_shot += 1;
        }
    }
}
