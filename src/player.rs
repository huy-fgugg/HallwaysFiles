//! First-person controller: mouse/stick look, WASD movement, sprint, crouch,
//! head-bob, footsteps and camera shake.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use std::f32::consts::FRAC_PI_2;

use crate::audio::{PlaySfx, Sfx};
use crate::interaction::HideState;
use crate::physics::{resolve_cylinder, Collider};
use crate::run::CameraShake;
use crate::settings::Settings;
use crate::state::gameplay_active;

pub const WALK_SPEED: f32 = 3.0;
pub const SPRINT_SPEED: f32 = 5.2;
pub const CROUCH_SPEED: f32 = 1.6;
pub const STAND_EYE: f32 = 1.62;
pub const CROUCH_EYE: f32 = 1.0;
pub const RADIUS: f32 = 0.3;
/// Tallest ledge the player walks up without jumping (stair risers are 0.2).
/// Highest ledge walked straight up. Must exceed two stair rises: the player
/// (radius 0.3) touches the step after next while standing on a 0.3 m tread.
pub const STEP_UP: f32 = 0.45;
/// Longest move resolved against walls in one go (m).
const MAX_SUBSTEP: f32 = 0.12;
const GRAVITY: f32 = 18.0;
const BASE_MOUSE: f32 = 0.0022;
const BASE_STICK: f32 = 2.6;

#[derive(Component, Debug)]
pub struct Player {
    pub yaw: f32,
    pub pitch: f32,
    pub eye: f32,
    pub crouching: bool,
    pub sprinting: bool,
    pub speed: f32,
    pub bob_phase: f32,
    step_accum: f32,
    /// Vertical speed while dropping off a ledge.
    pub fall: f32,
}

impl Player {
    pub fn new(yaw: f32) -> Self {
        Self { yaw, pitch: 0.0, eye: STAND_EYE, crouching: false, sprinting: false, speed: 0.0, bob_phase: 0.0, step_accum: 0.0, fall: 0.0 }
    }
}

#[derive(Component)]
pub struct PlayerCamera;

/// Systems that write the camera transform; cutscenes run after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraSet;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeathFall>()
            .add_systems(OnEnter(crate::state::Overlay::Dead), |mut f: ResMut<DeathFall>| f.0 = Some(0.0))
            .add_systems(OnExit(crate::state::Overlay::Dead), |mut f: ResMut<DeathFall>| f.0 = None);
        app.add_systems(
            Update,
            ((look, movement).chain().run_if(gameplay_active), apply_camera).chain().in_set(CameraSet),
        );
    }
}

fn look(
    mouse: Res<AccumulatedMouseMotion>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    settings: Res<Settings>,
    mut player: Single<&mut Player>,
    mut touch: ResMut<crate::touch::TouchLook>,
) {
    let sens = settings.mouse_sensitivity;
    let invert = if settings.invert_y { -1.0 } else { 1.0 };
    // Touch drags count a little more than mouse pixels (phone screens are small).
    // (QA phone emulation: the mouse is a finger, so it doesn't also turn the camera.)
    let mouse_delta = if crate::qa::fake_touch() { Vec2::ZERO } else { mouse.delta };
    let mut delta = (mouse_delta + std::mem::take(&mut touch.0) * 1.8) * BASE_MOUSE * sens;
    for pad in &pads {
        let s = pad.right_stick();
        if s.length() > 0.15 {
            delta += Vec2::new(s.x, -s.y) * BASE_STICK * sens * time.delta_secs();
        }
    }
    player.yaw -= delta.x;
    player.pitch = (player.pitch - delta.y * invert).clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
}

/// Pure movement step, separated for testing: returns the planar velocity for
/// a given input direction (x = strafe right, y = forward) and yaw.
pub fn planar_velocity(input: Vec2, yaw: f32, speed: f32) -> Vec3 {
    let input = if input.length_squared() > 1.0 { input.normalize() } else { input };
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    (forward * input.y + right * input.x) * speed
}

#[allow(clippy::too_many_arguments)]
fn movement(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    hide: Res<HideState>,
    colliders: Query<&Collider>,
    grounds: Query<&crate::physics::Ground>,
    player_q: Single<(&mut Player, &mut Transform)>,
    mut sfx: MessageWriter<PlaySfx>,
    boost: Res<crate::items::SpeedBoost>,
    grabbed: Res<crate::hazards::Grabbed>,
) {
    let (mut player, mut transform) = player_q.into_inner();
    let dt = time.delta_secs();
    if hide.is_hiding() {
        player.speed = 0.0;
        return;
    }

    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }
    let mut sprint = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let mut crouch = keys.any_pressed([KeyCode::ControlLeft, KeyCode::KeyC]);
    for pad in &pads {
        let s = pad.left_stick();
        if s.length() > 0.15 {
            input += s;
        }
        sprint |= pad.pressed(GamepadButton::LeftThumb) || pad.pressed(GamepadButton::LeftTrigger);
        crouch |= pad.pressed(GamepadButton::East) || pad.pressed(GamepadButton::RightThumb);
    }

    player.crouching = crouch;
    player.sprinting = sprint && !crouch && input.y > 0.1;
    let speed = if crouch {
        CROUCH_SPEED
    } else if player.sprinting {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    let speed = if boost.0 > 0.0 { speed * 1.35 } else { speed };
    // Grabbed by the chase's wall hands: slowed for a moment.
    let speed = if grabbed.0 > 0.0 { speed * 0.5 } else { speed };
    let vel = planar_velocity(input, player.yaw, speed);

    let boxes: Vec<_> = colliders.iter().map(|c| c.0).collect();
    let eye_target = if crouch { CROUCH_EYE } else { STAND_EYE };
    player.eye += (eye_target - player.eye) * (dt * 10.0).min(1.0);
    let feet = transform.translation.y;
    // Low boxes (stair steps) are stepped over; ground snapping lifts the player.
    // Long frames (slow devices, hitches) move in short sub-steps so the player
    // can't skip through thin walls and obstacles.
    let travel = vel * dt;
    let steps = ((travel.length() / MAX_SUBSTEP).ceil() as usize).clamp(1, 12);
    let mut resolved = transform.translation;
    for _ in 0..steps {
        resolved = resolve_cylinder(resolved + travel / steps as f32, RADIUS, feet + STEP_UP, feet + player.eye + 0.1, &boxes);
    }
    let floors: Vec<_> = grounds.iter().map(|g| g.0).collect();
    let ground = crate::physics::ground_height(resolved, feet, STEP_UP, &floors);
    if ground >= feet {
        resolved.y = feet + (ground - feet) * (dt * 18.0).min(1.0);
        if ground - resolved.y < 0.01 {
            resolved.y = ground;
        }
        player.fall = 0.0;
    } else {
        player.fall += GRAVITY * dt;
        resolved.y = (feet - player.fall * dt).max(ground);
        if resolved.y <= ground {
            player.fall = 0.0;
        }
    }
    let moved = (Vec3::new(resolved.x, 0.0, resolved.z) - Vec3::new(transform.translation.x, 0.0, transform.translation.z)).length();
    transform.translation = resolved;
    player.speed = if dt > 0.0 { moved / dt } else { 0.0 };

    // Footsteps every ~0.55 m walking (sprinting is louder in later phases).
    if player.speed > 0.5 {
        player.bob_phase += moved * 4.2;
        player.step_accum += moved;
        let stride = if crouch { 0.75 } else { 0.6 };
        if player.step_accum > stride {
            player.step_accum = 0.0;
            if !crouch {
                sfx.write(PlaySfx(Sfx::Footstep));
            }
        }
    }
}

/// Writes yaw/pitch, eye height, head-bob and shake into the transforms.
/// Seconds since you died (the death animation), while the death screen is up.
#[derive(Resource, Default)]
pub struct DeathFall(pub Option<f32>);

/// How far the death collapse has got (0..1) after `t` seconds.
pub fn death_fall(t: f32) -> f32 {
    let k = (t / 0.95).clamp(0.0, 1.0);
    // Knees buckle, then a heavy drop.
    k * k * (3.0 - 2.0 * k)
}

fn apply_camera(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    mut shake: ResMut<CameraShake>,
    mut fall: ResMut<DeathFall>,
    player_q: Query<(&Player, &mut Transform), Without<PlayerCamera>>,
    mut cam_q: Query<&mut Transform, With<PlayerCamera>>,
) {
    let mut player_q = player_q;
    let Ok((player, mut body)) = player_q.single_mut() else { return };
    let Ok(mut cam) = cam_q.single_mut() else { return };
    body.rotation = Quat::from_rotation_y(player.yaw);

    let bob_amount = if settings.reduce_flashing { 0.0 } else { 0.035 };
    let bob = (player.bob_phase).sin() * bob_amount * (player.speed / WALK_SPEED).min(1.4);

    let t = time.elapsed_secs();
    let strength = shake.trauma * shake.trauma * settings.spook();
    let shake_off = Vec3::new((t * 37.0).sin(), (t * 43.0).cos(), 0.0) * 0.06 * strength;
    let shake_roll = (t * 29.0).sin() * 0.04 * strength;
    shake.trauma = (shake.trauma - time.delta_secs() * 1.2).max(0.0);

    cam.translation = Vec3::new(0.0, player.eye + bob, 0.0) + shake_off;
    cam.rotation = Quat::from_euler(EulerRot::YXZ, 0.0, player.pitch, shake_roll);
    // Death: you slump sideways and crumple to the floor, looking up.
    if let Some(t) = fall.0.as_mut() {
        // Capped per frame so the fall is always seen, even on a slow device.
        *t += time.delta_secs().min(1.0 / 20.0);
        let k = death_fall(*t);
        let twitch = if *t < 0.5 { (*t * 40.0).sin() * 0.03 * (1.0 - *t * 2.0) } else { 0.0 };
        cam.translation = Vec3::new(0.25 * k, player.eye + (0.22 - player.eye) * k, 0.0) + shake_off;
        cam.rotation = Quat::from_euler(EulerRot::YXZ, 0.0, player.pitch * (1.0 - k) + 0.35 * k + twitch, 1.35 * k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_at_zero_yaw_is_negative_z() {
        let v = planar_velocity(Vec2::new(0.0, 1.0), 0.0, 2.0);
        assert!((v - Vec3::new(0.0, 0.0, -2.0)).length() < 1e-5);
    }

    #[test]
    fn strafe_right_is_positive_x() {
        let v = planar_velocity(Vec2::new(1.0, 0.0), 0.0, 1.0);
        assert!((v - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn diagonal_is_not_faster() {
        let v = planar_velocity(Vec2::new(1.0, 1.0), 0.3, 3.0);
        assert!((v.length() - 3.0).abs() < 1e-4);
    }
}

#[cfg(test)]
mod stair_tests {
    use super::*;
    use crate::floor::plan::{STEP_RISE, STEP_RUN};

    #[test]
    fn stairs_never_trap_the_player() {
        // Standing on any step, the face within reach (one tread ahead of the
        // player's edge) must be low enough to step over.
        assert!(STEP_UP > 2.0 * STEP_RISE, "two rises within one radius");
        assert!(RADIUS <= STEP_RUN + 1e-6);
        // Still blocked by crates and barricades.
        assert!(STEP_UP < 0.7);
    }
}
