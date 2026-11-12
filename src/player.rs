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
    bob_phase: f32,
    step_accum: f32,
}

impl Player {
    pub fn new(yaw: f32) -> Self {
        Self { yaw, pitch: 0.0, eye: STAND_EYE, crouching: false, sprinting: false, speed: 0.0, bob_phase: 0.0, step_accum: 0.0 }
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
) {
    let sens = settings.mouse_sensitivity;
    let invert = if settings.invert_y { -1.0 } else { 1.0 };
    let mut delta = mouse.delta * BASE_MOUSE * sens;
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
    player_q: Single<(&mut Player, &mut Transform)>,
    mut sfx: MessageWriter<PlaySfx>,
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
    let vel = planar_velocity(input, player.yaw, speed);

    let boxes: Vec<_> = colliders.iter().map(|c| c.0).collect();
    let target = transform.translation + vel * dt;
    let eye_target = if crouch { CROUCH_EYE } else { STAND_EYE };
    player.eye += (eye_target - player.eye) * (dt * 10.0).min(1.0);
    let resolved = resolve_cylinder(target, RADIUS, transform.translation.y, transform.translation.y + player.eye + 0.1, &boxes);
    let moved = (resolved - transform.translation).length();
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
fn apply_camera(
    time: Res<Time>,
    settings: Res<Settings>,
    mut shake: ResMut<CameraShake>,
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
