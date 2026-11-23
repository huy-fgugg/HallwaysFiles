//! The arrival lift: an old brass-cage service elevator set into the south
//! wall of Room 000. Every run starts inside it (see `01_intro` cutscene).
//! Built entirely from primitives: panelled car, brass trim and handrail,
//! button panel, floor-indicator dial with a moving needle, ceiling lamp and
//! a folding lattice gate whose bars bunch to the sides as it opens.

use bevy::prelude::*;
use serde::Deserialize;

use super::{materials::SurfaceMaterials, meshes::tiled_box, Flicker};
use crate::audio::{PlaySfx, Sfx};
use crate::physics::{Aabb, Collider};
use crate::state::AppState;

/// Car interior bounds (world space).
pub const CAR_MIN: Vec3 = Vec3::new(-1.0, 0.0, 5.2);
pub const CAR_MAX: Vec3 = Vec3::new(1.0, 2.5, 8.2);
pub const GATE_Z: f32 = 5.32;
/// The rear gate, between the car and the boarding hall behind it.
pub const REAR_GATE_Z: f32 = CAR_MAX.z - 0.12;
/// Where a run starts when there's no boarding (retry, solo custom floor).
pub const CAR_START: Vec3 = Vec3::new(0.0, 0.0, 6.9);
/// The boarding hall behind the lift (x, z bounds).
pub const HALL_MIN: Vec2 = Vec2::new(-3.6, CAR_MAX.z + 0.08);
pub const HALL_MAX: Vec2 = Vec2::new(3.6, CAR_MAX.z + 8.5);
/// Where the player stands at the start of a run: in the boarding hall.
pub const PLAYER_START: Vec3 = Vec3::new(0.0, 0.0, CAR_MAX.z + 5.0);

/// True when a point (feet) is inside the lift car, past both gates.
pub fn in_car(p: Vec3) -> bool {
    p.x > CAR_MIN.x + 0.1 && p.x < CAR_MAX.x - 0.1 && p.z > GATE_Z + 0.15 && p.z < REAR_GATE_Z - 0.15 && p.y < 1.0
}
const GATE_BARS: usize = 11;

#[derive(Message, Debug, Clone, Deserialize)]
pub enum ElevatorCommand {
    /// Sweep the floor-indicator needle to `to` (0 = top floor, 1 = arrival) over `seconds`.
    Needle { to: f32, seconds: f32 },
    /// Open the gate over `seconds` (0 = instantly).
    OpenGate { seconds: f32 },
    /// Close the rear (boarding) gate over `seconds` (0 = instantly).
    CloseRear { seconds: f32 },
}

impl ElevatorCommand {
    pub fn instant(&self) -> Self {
        match self {
            ElevatorCommand::Needle { to, .. } => ElevatorCommand::Needle { to: *to, seconds: 0.0 },
            ElevatorCommand::OpenGate { .. } => ElevatorCommand::OpenGate { seconds: 0.0 },
            ElevatorCommand::CloseRear { .. } => ElevatorCommand::CloseRear { seconds: 0.0 },
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct ElevatorState {
    pub gate: f32,
    gate_rate: f32,
    pub needle: f32,
    needle_from: f32,
    needle_to: f32,
    needle_t: f32,
    needle_secs: f32,
    gate_blocker: Option<Entity>,
    /// Rear gate: 1 = open (boarding), 0 = closed.
    pub rear: f32,
    rear_rate: f32,
}

#[derive(Component)]
struct GateBar {
    closed_x: f32,
    open_x: f32,
    rear: bool,
}

/// Collider for the rear gate, added when it closes.
#[derive(Component)]
struct RearBlocker;

#[derive(Component)]
struct Needle;

pub struct ElevatorPlugin;

impl Plugin for ElevatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ElevatorCommand>()
            .init_resource::<ElevatorState>()
            .add_systems(Update, (handle_commands, animate).chain().run_if(in_state(AppState::Playing)));
    }
}

fn handle_commands(
    mut commands: Commands,
    mut reader: MessageReader<ElevatorCommand>,
    mut state: ResMut<ElevatorState>,
    mut sfx: MessageWriter<PlaySfx>,
    rear: Query<Entity, With<RearBlocker>>,
) {
    for cmd in reader.read() {
        match cmd {
            ElevatorCommand::Needle { to, seconds } => {
                state.needle_from = state.needle;
                state.needle_to = *to;
                state.needle_t = 0.0;
                state.needle_secs = *seconds;
                if *seconds <= 0.0 {
                    state.needle = *to;
                }
            }
            ElevatorCommand::OpenGate { seconds } => {
                if *seconds <= 0.0 {
                    state.gate = 1.0;
                    state.gate_rate = 0.0;
                } else {
                    state.gate_rate = 1.0 / seconds;
                    sfx.write(PlaySfx(Sfx::GateRattle));
                }
                if let Some(b) = state.gate_blocker.take() {
                    commands.entity(b).remove::<Collider>();
                }
            }
            ElevatorCommand::CloseRear { seconds } => {
                if *seconds <= 0.0 {
                    state.rear = 0.0;
                    state.rear_rate = 0.0;
                } else {
                    state.rear_rate = -1.0 / seconds;
                    sfx.write(PlaySfx(Sfx::GateRattle));
                }
                for e in &rear {
                    commands.entity(e).insert(Collider(Aabb { min: Vec3::new(CAR_MIN.x, 0.0, REAR_GATE_Z - 0.05), max: Vec3::new(CAR_MAX.x, 2.2, REAR_GATE_Z + 0.05) }));
                }
            }
        }
    }
}

fn animate(time: Res<Time>, mut state: ResMut<ElevatorState>, mut bars: Query<(&GateBar, &mut Transform), Without<Needle>>, mut needle: Query<&mut Transform, With<Needle>>) {
    let dt = time.delta_secs();
    state.gate = (state.gate + state.gate_rate * dt).min(1.0);
    state.rear = (state.rear + state.rear_rate * dt).clamp(0.0, 1.0);
    if state.needle_secs > 0.0 && state.needle_t < state.needle_secs {
        state.needle_t = (state.needle_t + dt).min(state.needle_secs);
        let k = state.needle_t / state.needle_secs;
        let k = k * k * (3.0 - 2.0 * k);
        state.needle = state.needle_from + (state.needle_to - state.needle_from) * k;
    }
    // Ease-out with a slight jitter, like an old mechanism catching.
    let ease = |v: f32| 1.0 - (1.0 - v).powi(3);
    let jitter = |v: f32| if v > 0.0 && v < 1.0 { (time.elapsed_secs() * 40.0).sin() * 0.004 } else { 0.0 };
    for (bar, mut tf) in &mut bars {
        let v = if bar.rear { state.rear } else { state.gate };
        tf.translation.x = bar.closed_x + (bar.open_x - bar.closed_x) * ease(v) + jitter(v);
    }
    for mut tf in &mut needle {
        // Needle sweeps from right (+70°) to left (-70°) as the car descends.
        tf.rotation = Quat::from_rotation_z((70.0 - 140.0 * state.needle).to_radians());
    }
}

/// Builds the lift car. `opening` is the doorway cut in Room 000's south wall.
pub fn spawn_elevator(commands: &mut Commands, meshes: &mut Assets<Mesh>, std_mats: &mut Assets<StandardMaterial>, mats: &SurfaceMaterials) -> Entity {
    let scope = DespawnOnExit(AppState::Playing);
    let solid = |commands: &mut Commands, meshes: &mut Assets<Mesh>, min: Vec3, max: Vec3, mat: &Handle<StandardMaterial>, tile: f32, collide: bool| {
        let size = max - min;
        let mut e = commands.spawn((Mesh3d(meshes.add(tiled_box(size, tile))), MeshMaterial3d(mat.clone()), Transform::from_translation((min + max) / 2.0), scope.clone()));
        if collide {
            e.insert(Collider(Aabb { min, max }));
        }
        e.id()
    };
    let (x0, x1, z0, z1, h) = (CAR_MIN.x, CAR_MAX.x, CAR_MIN.z, CAR_MAX.z, CAR_MAX.y);
    let t = 0.08;
    let dark_metal = std_mats.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.085), metallic: 0.8, perceptual_roughness: 0.45, ..default() });

    // Shell: floor, ceiling, back and side walls.
    solid(commands, meshes, Vec3::new(x0 - t, -0.1, z0), Vec3::new(x1 + t, 0.0, z1 + t), &mats.door_wood, 0.9, false);
    solid(commands, meshes, Vec3::new(x0 - t, h, z0), Vec3::new(x1 + t, h + 0.1, z1 + t), &mats.panel_wood, 1.0, false);
    // The back is open: a rear gate to the boarding hall, with a header above it.
    solid(commands, meshes, Vec3::new(x0 - t, 2.25, z1), Vec3::new(x1 + t, h, z1 + t), &mats.panel_wood, 0.9, false);
    solid(commands, meshes, Vec3::new(x0 - t, 0.0, z0), Vec3::new(x0, h, z1), &mats.panel_wood, 0.9, true);
    solid(commands, meshes, Vec3::new(x1, 0.0, z0), Vec3::new(x1 + t, h, z1), &mats.panel_wood, 0.9, true);

    // Raised panels with brass beading on the three inner walls.
    let bead = 0.012;
    for (axis_x, fixed, dir) in [(false, x0, 1.0f32), (false, x1, -1.0)] {
        let (a0, a1) = if axis_x { (x0, x1) } else { (z0 + 0.15, z1) };
        let n = 2;
        let w = (a1 - a0) / n as f32;
        for i in 0..n {
            let c = a0 + w * (i as f32 + 0.5);
            for (y0, y1) in [(0.2f32, 0.95f32), (1.1, 2.25)] {
                let pw = w - 0.16;
                let (min, max) = if axis_x {
                    (Vec3::new(c - pw / 2.0, y0, fixed + dir * 0.02), Vec3::new(c + pw / 2.0, y1, fixed))
                } else {
                    (Vec3::new(fixed, y0, c - pw / 2.0), Vec3::new(fixed + dir * 0.02, y1, c + pw / 2.0))
                };
                solid(commands, meshes, min.min(max), min.max(max), &mats.trim_wood, 0.6, false);
                // Brass bead along the top and bottom of each panel.
                for y in [y0, y1 - bead] {
                    let (bmin, bmax) = if axis_x {
                        (Vec3::new(c - pw / 2.0, y, fixed + dir * 0.03), Vec3::new(c + pw / 2.0, y + bead, fixed))
                    } else {
                        (Vec3::new(fixed, y, c - pw / 2.0), Vec3::new(fixed + dir * 0.03, y + bead, c + pw / 2.0))
                    };
                    solid(commands, meshes, bmin.min(bmax), bmin.max(bmax), &mats.brass, 1.0, false);
                }
            }
        }
    }
    // Handrails along both side walls.
    let rail = meshes.add(Cylinder::new(0.022, z1 - z0 - 0.6));
    for x in [x0 + 0.09, x1 - 0.09] {
        commands.spawn((Mesh3d(rail.clone()), MeshMaterial3d(mats.brass.clone()), Transform::from_xyz(x, 0.95, (z0 + z1) / 2.0 + 0.15).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), scope.clone()));
    }
    // Button panel on the right wall: brass plate, round buttons, one lit.
    solid(commands, meshes, Vec3::new(x1 - 0.015, 1.0, z0 + 0.35), Vec3::new(x1, 1.6, z0 + 0.6), &mats.brass, 1.0, false);
    let button = meshes.add(Cylinder::new(0.022, 0.012));
    let lit = std_mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.3), emissive: LinearRgba::rgb(6.0, 2.4, 0.6), ..default() });
    let ivory = std_mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.82, 0.74), perceptual_roughness: 0.3, ..default() });
    for row in 0..5 {
        for col in 0..2 {
            let mat = if row == 4 && col == 0 { lit.clone() } else { ivory.clone() };
            commands.spawn((
                Mesh3d(button.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(x1 - 0.02, 1.48 - row as f32 * 0.1, z0 + 0.42 + col as f32 * 0.11).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                scope.clone(),
            ));
        }
    }
    // Front header above the gate (inside the car) carrying the floor dial.
    solid(commands, meshes, Vec3::new(x0, 2.25, z0), Vec3::new(x1, h, z0 + 0.06), &mats.panel_wood, 0.9, false);
    let dial = meshes.add(Circle::new(0.17));
    let dial_mat = std_mats.add(StandardMaterial { base_color: Color::srgb(0.9, 0.85, 0.7), perceptual_roughness: 0.4, ..default() });
    commands.spawn((Mesh3d(dial), MeshMaterial3d(dial_mat), Transform::from_xyz(0.0, 2.37, z0 + 0.065), scope.clone()));
    let ring = meshes.add(Torus::new(0.165, 0.185));
    commands.spawn((Mesh3d(ring), MeshMaterial3d(mats.brass.clone()), Transform::from_xyz(0.0, 2.37, z0 + 0.066).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), scope.clone()));
    // Tick marks around the top of the dial.
    let tick = meshes.add(Cuboid::new(0.006, 0.03, 0.004));
    for i in 0..9 {
        let a = (70.0 - 17.5 * i as f32).to_radians();
        commands.spawn((
            Mesh3d(tick.clone()),
            MeshMaterial3d(dark_metal.clone()),
            Transform::from_xyz(-a.sin() * 0.13, 2.37 + a.cos() * 0.13, z0 + 0.068).with_rotation(Quat::from_rotation_z(a)),
            scope.clone(),
        ));
    }
    let needle_mesh = meshes.add(Cuboid::new(0.008, 0.13, 0.004));
    commands
        .spawn((Transform::from_xyz(0.0, 2.37, z0 + 0.072), Visibility::default(), Needle, scope.clone()))
        .with_child((Mesh3d(needle_mesh), MeshMaterial3d(dark_metal.clone()), Transform::from_xyz(0.0, 0.06, 0.0)));
    // Ceiling lamp: brass cup + glowing bulb + light.
    let cup = meshes.add(Cylinder::new(0.12, 0.05));
    commands.spawn((Mesh3d(cup), MeshMaterial3d(mats.brass.clone()), Transform::from_xyz(0.0, h - 0.025, (z0 + z1) / 2.0), scope.clone()));
    let bulb = meshes.add(Sphere::new(0.05).mesh().uv(20, 12));
    let glow = std_mats.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(12.0, 8.0, 4.5), ..default() });
    commands.spawn((Mesh3d(bulb), MeshMaterial3d(glow), Transform::from_xyz(0.0, h - 0.07, (z0 + z1) / 2.0), scope.clone()));
    commands.spawn((
        PointLight { color: Color::srgb(1.0, 0.78, 0.5), intensity: 60_000.0, range: 6.0, radius: 0.05, shadow_maps_enabled: true, ..default() },
        Flicker { base: 60_000.0, seed: 3.3, depth: 0.05 },
        Transform::from_xyz(0.0, h - 0.15, (z0 + z1) / 2.0),
        scope.clone(),
    ));

    // Folding lattice gates (front to the floor, rear to the boarding hall):
    // top and bottom tracks plus bars that bunch to the sides.
    let bar_mesh = meshes.add(Cuboid::new(0.018, 2.16, 0.018));
    let strap = meshes.add(Cuboid::new(0.09, 0.012, 0.008));
    let span = x1 - x0 - 0.08;
    for (rear, gz) in [(false, GATE_Z), (true, REAR_GATE_Z)] {
    solid(commands, meshes, Vec3::new(x0, 2.18, gz - 0.03), Vec3::new(x1, 2.25, gz + 0.03), &dark_metal, 1.0, false);
    solid(commands, meshes, Vec3::new(x0, 0.0, gz - 0.03), Vec3::new(x1, 0.02, gz + 0.03), &dark_metal, 1.0, false);
    for i in 0..GATE_BARS {
        let f = i as f32 / (GATE_BARS - 1) as f32;
        let closed_x = x0 + 0.04 + span * f;
        // Left half bunches to the left wall, right half to the right.
        let open_x = if f < 0.5 { x0 + 0.04 + f * 0.25 } else { x1 - 0.04 - (1.0 - f) * 0.25 };
        commands
            .spawn((
                Mesh3d(bar_mesh.clone()),
                MeshMaterial3d(mats.brass.clone()),
                Transform::from_xyz(if rear { open_x } else { closed_x }, 1.09, gz),
                GateBar { closed_x, open_x, rear },
                scope.clone(),
            ))
            .with_children(|c| {
                // Diagonal lattice straps give the scissor-gate look.
                for k in 0..6 {
                    let y = -0.9 + k as f32 * 0.36;
                    let sign = if (i + k) % 2 == 0 { 1.0 } else { -1.0 };
                    c.spawn((Mesh3d(strap.clone()), MeshMaterial3d(mats.brass.clone()), Transform::from_xyz(0.04, y, 0.012).with_rotation(Quat::from_rotation_z(sign * 0.7))));
                }
            });
    }
    }
    // The rear gate starts open; its collider is added when it closes.
    commands.spawn((Transform::default(), RearBlocker, scope.clone()));
    // Collider for the closed gate; removed when it opens.
    commands.spawn((Transform::default(), Collider(Aabb { min: Vec3::new(x0, 0.0, GATE_Z - 0.05), max: Vec3::new(x1, 2.2, GATE_Z + 0.05) }), scope)).id()
}

pub fn reset_state(blocker: Entity) -> ElevatorState {
    ElevatorState { gate_blocker: Some(blocker), rear: 1.0, ..default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_commands_have_zero_duration() {
        assert!(matches!(ElevatorCommand::OpenGate { seconds: 2.0 }.instant(), ElevatorCommand::OpenGate { seconds } if seconds == 0.0));
        assert!(matches!(ElevatorCommand::Needle { to: 1.0, seconds: 5.0 }.instant(), ElevatorCommand::Needle { seconds, .. } if seconds == 0.0));
    }

    #[test]
    fn players_board_from_the_hall() {
        // A run starts in the boarding hall, outside the car; a retry starts inside it.
        assert!(!in_car(PLAYER_START));
        assert!(PLAYER_START.z > HALL_MIN.y && PLAYER_START.z < HALL_MAX.y);
        assert!(in_car(CAR_START));
        // Five players fit (about 0.5 m² each).
        let floor = (CAR_MAX.x - CAR_MIN.x) * (REAR_GATE_Z - GATE_Z);
        assert!(floor / 5.0 >= 0.5, "{floor}");
        let _ = Aabb { min: CAR_MIN, max: CAR_MAX };
    }
}
