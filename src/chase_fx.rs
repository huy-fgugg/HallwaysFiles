//! Chase atmosphere (original, inspired by the reference the player gave):
//!
//! * When a chase starts the screen **flashes**, then the light turns
//!   blood-red and **ripples** for the whole chase (a pulsing red vignette
//!   and lamps that throb). With Reduce Flashing on, the flash is a soft red.
//! * **Eyes open in the walls** of the chase rooms and follow you as you run.
//! * When the door slams behind the survivors, a **red glow** bleeds out from
//!   under it and slowly fades.

use bevy::prelude::*;

use crate::floor::plan::{chases, Feature, WALL_T};
use crate::floor::{Floor, FloorRuntime, RoomMember};
use crate::player::Player;
use crate::pursuit::{Pursuit, PursuitPhase};
use crate::rng::GameRng;
use crate::run::Subtitle;
use crate::state::AppState;

const FLASH_SECS: f32 = 0.6;
const GLOW_SECS: f32 = 9.0;

#[derive(Component)]
struct Flash(f32);

#[derive(Component)]
struct Vignette;

#[derive(Component)]
struct VignetteLayer(usize);

const VIGNETTE_LAYERS: usize = 6;

#[derive(Component)]
struct Eye {
    room: u32,
    normal: Vec3,
    open: f32,
    seed: f32,
}

#[derive(Component)]
struct Iris;

#[derive(Component)]
struct DoorGlow(f32);

#[derive(Resource, Default)]
struct EyesBuilt(Vec<u32>);

pub struct ChaseFxPlugin;

impl Plugin for ChaseFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EyesBuilt>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(EyesBuilt::default()))
            .add_systems(Update, (flash_and_vignette, spawn_eyes, animate_eyes, door_glow).run_if(in_state(AppState::Playing)));
    }
}

/// Pulse of the chase light at time `t`: 0..1, a slow throb with a ripple on it.
pub fn ripple(t: f32) -> f32 {
    (0.5 + 0.35 * (t * 2.1).sin() + 0.15 * (t * 7.3).sin()).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn flash_and_vignette(
    mut commands: Commands,
    time: Res<Time>,
    pursuit: Res<Pursuit>,
    settings: Res<crate::settings::Settings>,
    mut prev: Local<bool>,
    mut flashes: Query<(Entity, &mut Flash, &mut BackgroundColor)>,
    vignettes: Query<Entity, With<Vignette>>,
    mut layers: Query<(&VignetteLayer, &mut BorderColor)>,
    mut subs: MessageWriter<Subtitle>,
) {
    let dt = time.delta_secs();
    let started = matches!(pursuit.phase, PursuitPhase::Cue(_));
    let active = pursuit.active();
    if started && !*prev {
        let c = if settings.reduce_flashing { Color::srgba(0.6, 0.0, 0.0, 0.35) } else { Color::srgba(1.0, 0.95, 0.9, 0.9) };
        commands.spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, BackgroundColor(c), GlobalZIndex(30), Pickable::IGNORE, Flash(0.0), DespawnOnExit(AppState::Playing)));
        // A soft red vignette: nested frames, darker toward the edges.
        commands
            .spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(5), Pickable::IGNORE, Vignette, DespawnOnExit(AppState::Playing)))
            .with_children(|v| {
                for k in 0..VIGNETTE_LAYERS {
                    let inset = k as f32 * 22.0;
                    v.spawn((
                        Node { position_type: PositionType::Absolute, left: px(inset), right: px(inset), top: px(inset), bottom: px(inset), border: UiRect::all(px(22)), border_radius: BorderRadius::all(px(120.0 + inset)), ..default() },
                        BorderColor::all(Color::NONE),
                        Pickable::IGNORE,
                        VignetteLayer(k),
                    ));
                }
            });
        subs.write(Subtitle::danger("[The eyes in the walls open.]"));
    }
    *prev = started || (active && *prev);
    for (e, mut f, mut bg) in &mut flashes {
        f.0 += dt;
        let a = bg.0.alpha();
        bg.0.set_alpha((a - dt / FLASH_SECS).max(0.0));
        if f.0 >= FLASH_SECS {
            commands.entity(e).try_despawn();
        }
    }
    let k = ripple(time.elapsed_secs());
    for e in &vignettes {
        if !active {
            commands.entity(e).try_despawn();
        }
    }
    for (l, mut border) in &mut layers {
        let fall = 1.0 - l.0 as f32 / VIGNETTE_LAYERS as f32;
        *border = BorderColor::all(Color::srgba(0.5, 0.0, 0.0, (0.1 + 0.16 * k) * fall));
    }
}

fn spawn_eyes(
    mut commands: Commands,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    mut built: ResMut<EyesBuilt>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    built.0.retain(|i| rt.loaded.contains(i));
    let rooms: Vec<u32> = chases(floor.0.last_room).flat_map(|(a, b)| a..=b).collect();
    for i in rooms {
        if built.0.contains(&i) || !rt.loaded.contains(&i) {
            continue;
        }
        let Some(r) = floor.0.room(i) else { continue };
        built.0.push(i);
        if r.feature == Some(Feature::HandHall) || !r.pursuit {
            continue;
        }
        let white = mats.add(StandardMaterial { base_color: Color::srgb(0.92, 0.88, 0.82), emissive: LinearRgba::rgb(0.25, 0.2, 0.18), perceptual_roughness: 0.25, ..default() });
        let iris = mats.add(StandardMaterial { base_color: Color::srgb(0.6, 0.02, 0.02), emissive: LinearRgba::rgb(3.0, 0.1, 0.05), ..default() });
        let pupil = mats.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
        let lid = mats.add(StandardMaterial { base_color: Color::srgb(0.08, 0.03, 0.04), perceptual_roughness: 0.4, ..default() });
        let ball = meshes.add(Sphere::new(0.5).mesh().uv(18, 10));
        let mut rng = GameRng::fork(r.seed, 0xE7E5);
        let dividers: Vec<f32> = r.dividers().iter().map(|d| d.z).collect();
        let n = ((r.depth() / 1.6) as usize).clamp(4, 14);
        for k in 0..n {
            let west = k % 2 == 0;
            let z = rng.range_f32(r.min.y + 1.0, r.max.y - 1.0);
            if dividers.iter().any(|d| (d - z).abs() < 0.5) {
                continue;
            }
            let y = r.entry_height.max(0.0) + rng.range_f32(0.9, 2.3);
            let (x, normal) = if west { (r.min.x + 0.01, Vec3::X) } else { (r.max.x - 0.01, -Vec3::X) };
            let size = rng.range_f32(0.7, 1.3);
            let rot = Quat::from_rotation_y(if west { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 });
            commands
                .spawn((
                    Transform::from_xyz(x, y, z).with_rotation(rot * Quat::from_rotation_z(rng.range_f32(-0.25, 0.25))).with_scale(Vec3::new(size, 0.02, size)),
                    Visibility::default(),
                    Eye { room: i, normal, open: 0.0, seed: rng.range_f32(0.0, 10.0) },
                    RoomMember(i),
                    DespawnOnExit(AppState::Playing),
                    Name::new("Wall eye"),
                ))
                .with_children(|e| {
                    // A dark socket, the white, and a red iris with a black pupil.
                    e.spawn((Mesh3d(ball.clone()), MeshMaterial3d(lid.clone()), Transform::from_scale(Vec3::new(0.42, 0.28, 0.05))));
                    e.spawn((Mesh3d(ball.clone()), MeshMaterial3d(white.clone()), Transform::from_xyz(0.0, 0.0, 0.01).with_scale(Vec3::new(0.36, 0.22, 0.08))));
                    e.spawn((Transform::from_xyz(0.0, 0.0, 0.045), Visibility::default(), Iris)).with_children(|i| {
                        i.spawn((Mesh3d(ball.clone()), MeshMaterial3d(iris.clone()), Transform::from_scale(Vec3::new(0.13, 0.13, 0.03))));
                        i.spawn((Mesh3d(ball.clone()), MeshMaterial3d(pupil.clone()), Transform::from_xyz(0.0, 0.0, 0.012).with_scale(Vec3::new(0.05, 0.07, 0.02))));
                    });
                });
        }
    }
}

fn animate_eyes(
    time: Res<Time>,
    pursuit: Res<Pursuit>,
    player: Query<&Transform, (With<Player>, Without<Eye>, Without<Iris>)>,
    mut eyes: Query<(&mut Eye, &mut Transform, &GlobalTransform, &Children), Without<Iris>>,
    mut irises: Query<&mut Transform, (With<Iris>, Without<Eye>)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let p = player.single().map(|p| p.translation + Vec3::Y * 1.5).ok();
    let (a, b) = pursuit.chase;
    for (mut eye, mut tf, gt, kids) in &mut eyes {
        let awake = pursuit.active() && eye.room >= a && eye.room <= b;
        // Blink now and then while open.
        let blink = ((t * 0.37 + eye.seed).fract() < 0.04) as u8 as f32;
        let target = if awake { 1.0 - blink } else { 0.0 };
        let rate = if awake { 3.0 } else { 1.5 };
        eye.open += (target - eye.open) * (dt * rate * 4.0).min(1.0);
        let s = tf.scale.x;
        tf.scale = Vec3::new(s, (eye.open * s).max(0.02), s);
        // The iris follows the player across the wall.
        if let Some(p) = p {
            let to = (p - gt.translation()).normalize_or_zero();
            let side = to - eye.normal * to.dot(eye.normal);
            let local = tf.rotation.inverse() * side;
            for k in kids.iter() {
                if let Ok(mut i) = irises.get_mut(k) {
                    i.translation.x = (local.x * 0.12).clamp(-0.11, 0.11);
                    i.translation.y = (local.y * 0.08).clamp(-0.06, 0.06);
                }
            }
        }
    }
}

fn door_glow(
    mut commands: Commands,
    time: Res<Time>,
    pursuit: Res<Pursuit>,
    floor: Option<Res<Floor>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut prev: Local<bool>,
    mut glows: Query<(Entity, &mut DoorGlow, &mut PointLight, &Children)>,
    mut strips: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    let shut = pursuit.door_shut;
    if shut
        && !*prev
        && let Some(r) = floor.as_ref().and_then(|f| f.0.room(pursuit.chase.1))
    {
        let z = r.min.y - WALL_T;
        let strip = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.1, 0.05), emissive: LinearRgba::rgb(14.0, 0.6, 0.2), unlit: true, ..default() });
        commands
            .spawn((
                Transform::from_xyz(r.exit_x, 0.15, z - 0.35),
                Visibility::default(),
                PointLight { color: Color::srgb(1.0, 0.08, 0.04), intensity: 180_000.0, range: 6.0, ..default() },
                DoorGlow(0.0),
                DespawnOnExit(AppState::Playing),
                Name::new("Red glow under the door"),
            ))
            .with_child((Mesh3d(meshes.add(Cuboid::new(crate::world::DOOR_W * 0.95, 0.02, 0.1))), MeshMaterial3d(strip), Transform::from_xyz(0.0, -0.14, 0.3)));
    }
    *prev = shut;
    let dt = time.delta_secs();
    for (e, mut g, mut light, kids) in &mut glows {
        g.0 += dt;
        let k = (1.0 - g.0 / GLOW_SECS).max(0.0);
        // It throbs as it fades.
        light.intensity = 180_000.0 * k * (0.75 + 0.25 * (g.0 * 6.0).sin());
        for c in kids.iter() {
            if let Ok(m) = strips.get_mut(c)
                && let Some(mut mat) = mats.get_mut(&m.0)
            {
                mat.emissive = LinearRgba::rgb(14.0 * k, 0.6 * k, 0.2 * k);
            }
        }
        if g.0 >= GLOW_SECS {
            commands.entity(e).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripple_stays_in_range_and_moves() {
        let v: Vec<f32> = (0..200).map(|i| ripple(i as f32 * 0.05)).collect();
        assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
        let (lo, hi) = v.iter().fold((1.0f32, 0.0f32), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(hi - lo > 0.5, "it visibly ripples");
    }
}
