//! Look-at interaction (E / gamepad X): doors, drawers and hiding spots.
//!
//! A ray from the camera is tested against every [`Interactable`] and every
//! [`Collider`] (as occluders); the nearest interactable in reach that isn't
//! behind a wall becomes the [`InteractTarget`] shown in the HUD prompt.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::physics::{Aabb, Collider};
use crate::player::{Player, PlayerCamera, CROUCH_EYE};
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState};

pub const REACH: f32 = 2.3;
/// Seconds a player may stay hidden before being forced out.
pub const HIDE_LIMIT: f32 = 10.0;
pub const HIDE_WARN: f32 = 7.0;
const HIDE_TRANSITION: f32 = 0.45;
const HIDE_COOLDOWN: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractKind {
    Door,
    Drawer,
    HidingSpot,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Interactable {
    pub kind: InteractKind,
    pub aabb: Aabb,
}

#[derive(Component, Debug)]
pub struct Door {
    pub number: u32,
    pub locked: bool,
    pub open: bool,
    /// 0 = closed, 1 = fully open.
    pub t: f32,
    /// Entity whose collider must be removed once the door opens.
    pub blocker: Entity,
}

#[derive(Component, Debug)]
pub struct Drawer {
    pub open: bool,
    pub t: f32,
    pub loot: Option<u32>,
    pub searched: bool,
}

#[derive(Component, Debug)]
pub struct HidingSpot {
    pub inside: Vec3,
    pub inside_yaw: f32,
    pub exit: Vec3,
    pub exit_yaw: f32,
    /// 0 closed .. 1 open: where the cabinet doors are heading...
    pub doors_target: f32,
    /// ...and where they currently are (eased in `animate_props`).
    pub doors_t: f32,
}

/// How a glTF sub-part follows its logical owner's `t` (0..1).
#[derive(Debug, Clone, Copy)]
pub enum PartMotion {
    /// Translate by this local offset at t = 1.
    Slide(Vec3),
    /// Rotate around local Y to this angle (radians) at t = 1, from closed.
    Hinge(f32),
}

/// Attached to a model sub-entity once it's found by name (see world module).
#[derive(Component, Debug, Clone, Copy)]
pub struct AnimatedPart {
    pub owner: Entity,
    pub base: Transform,
    pub motion: PartMotion,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct InteractTarget(pub Option<Entity>);

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum HidePhase {
    #[default]
    Out,
    Entering(f32),
    Hidden(f32),
    Exiting(f32),
}

#[derive(Resource, Debug, Default)]
pub struct HideState {
    pub phase: HidePhase,
    pub spot: Option<Entity>,
    pub cooldown: f32,
    warned: bool,
}

impl HideState {
    pub fn is_hiding(&self) -> bool {
        self.phase != HidePhase::Out
    }

    /// 0..1 fraction of hide time used (for the HUD meter).
    pub fn hidden_fraction(&self) -> Option<f32> {
        match self.phase {
            HidePhase::Hidden(t) => Some((t / HIDE_LIMIT).clamp(0.0, 1.0)),
            HidePhase::Entering(_) => Some(0.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HideTick {
    Nothing,
    Warn,
    ForceOut,
}

/// Pure hide-timer rule: warn once past [`HIDE_WARN`], force out at [`HIDE_LIMIT`].
pub fn hide_tick(elapsed_before: f32, dt: f32, already_warned: bool) -> HideTick {
    let now = elapsed_before + dt;
    if now >= HIDE_LIMIT {
        HideTick::ForceOut
    } else if now >= HIDE_WARN && !already_warned {
        HideTick::Warn
    } else {
        HideTick::Nothing
    }
}

/// Text for the HUD prompt, e.g. "Open Door 001".
pub fn prompt_text(
    target: Entity,
    interactables: &Query<&Interactable>,
    doors: &Query<&mut Door>,
    drawers: &Query<&mut Drawer>,
) -> Option<(String, bool)> {
    let kind = interactables.get(target).ok()?.kind;
    Some(match kind {
        InteractKind::Door => {
            let d = doors.get(target).ok()?;
            if d.locked {
                (format!("Try Door {:03}  (locked)", d.number), true)
            } else {
                (format!("Open Door {:03}", d.number), false)
            }
        }
        InteractKind::Drawer => {
            let d = drawers.get(target).ok()?;
            (if d.open { "Close drawer".into() } else { "Search drawer".into() }, false)
        }
        InteractKind::HidingSpot => ("Hide".into(), false),
    })
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractTarget>()
            .init_resource::<HideState>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| {
                c.insert_resource(HideState::default());
                c.insert_resource(InteractTarget::default());
            })
            .add_systems(
                Update,
                ((find_target, interact, update_hiding).chain().run_if(gameplay_active), animate_props).chain(),
            );
    }
}

fn find_target(
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    hide: Res<HideState>,
    interactables: Query<(Entity, &Interactable)>,
    colliders: Query<(Entity, &Collider)>,
    mut target: ResMut<InteractTarget>,
) {
    let Ok(cam) = cam.single() else { return };
    if hide.is_hiding() {
        target.0 = None;
        return;
    }
    let origin = cam.translation();
    let dir = cam.forward().as_vec3();
    let mut best: Option<(f32, Entity)> = None;
    for (e, i) in &interactables {
        if let Some(t) = i.aabb.ray_hit(origin, dir, REACH)
            && best.is_none_or(|(bt, _)| t < bt)
        {
            best = Some((t, e));
        }
    }
    // Occlusion: a wall closer than the target blocks it (ignore the target's own collider).
    if let Some((bt, be)) = best {
        for (e, c) in &colliders {
            if e == be {
                continue;
            }
            if let Some(t) = c.0.ray_hit(origin, dir, bt)
                && t + 0.05 < bt
                && !c.0.contains(origin)
            {
                best = None;
                break;
            }
        }
    }
    let new = best.map(|(_, e)| e);
    if target.0 != new {
        target.0 = new;
    }
}

fn interact_pressed(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> bool {
    keys.just_pressed(KeyCode::KeyE) || pads.iter().any(|p| p.just_pressed(GamepadButton::West))
}

#[allow(clippy::too_many_arguments)]
fn interact(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    target: Res<InteractTarget>,
    interactables: Query<&Interactable>,
    mut doors: Query<&mut Door>,
    mut drawers: Query<&mut Drawer>,
    spots: Query<&HidingSpot>,
    mut hide: ResMut<HideState>,
    mut run: ResMut<RunState>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
) {
    if !interact_pressed(&keys, &pads) {
        return;
    }
    // Leaving a hiding spot uses the same key.
    if let HidePhase::Hidden(_) = hide.phase {
        hide.phase = HidePhase::Exiting(0.0);
        sfx.write(PlaySfx(Sfx::ClosetExit));
        return;
    }
    if hide.is_hiding() {
        return;
    }
    let Some(e) = target.0 else { return };
    let Ok(i) = interactables.get(e) else { return };
    match i.kind {
        InteractKind::Door => {
            let Ok(mut door) = doors.get_mut(e) else { return };
            if door.locked {
                sfx.write(PlaySfx(Sfx::DoorLocked));
                subs.write(Subtitle::info("[Handle rattles] It won't budge. The rest of the floor isn't built yet."));
            } else if !door.open {
                door.open = true;
                commands.entity(door.blocker).remove::<Collider>();
                commands.entity(e).remove::<Interactable>();
                sfx.write(PlaySfx(Sfx::DoorOpen));
                subs.write(Subtitle::info("[Door creaks open]"));
            }
        }
        InteractKind::Drawer => {
            let Ok(mut drawer) = drawers.get_mut(e) else { return };
            drawer.open = !drawer.open;
            if drawer.open {
                sfx.write(PlaySfx(Sfx::DrawerOpen));
                if !drawer.searched {
                    drawer.searched = true;
                    match drawer.loot.take() {
                        Some(n) => {
                            run.lumens += n;
                            sfx.write(PlaySfx(Sfx::Pickup));
                            subs.write(Subtitle::info(format!("Found {n} Lumens")));
                        }
                        None => {
                            subs.write(Subtitle::info("Empty."));
                        }
                    }
                }
            } else {
                sfx.write(PlaySfx(Sfx::DrawerClose));
            }
        }
        InteractKind::HidingSpot => {
            if hide.cooldown > 0.0 || spots.get(e).is_err() {
                return;
            }
            hide.phase = HidePhase::Entering(0.0);
            hide.spot = Some(e);
            hide.warned = false;
            sfx.write(PlaySfx(Sfx::ClosetEnter));
            subs.write(Subtitle::info("[Hiding]  Press E to leave"));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hiding(
    time: Res<Time>,
    mut hide: ResMut<HideState>,
    mut spots: Query<&mut HidingSpot>,
    mut player: Query<(&mut Player, &mut Transform)>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
) {
    let dt = time.delta_secs();
    hide.cooldown = (hide.cooldown - dt).max(0.0);
    let Some(spot_e) = hide.spot else { return };
    let Ok(mut spot) = spots.get_mut(spot_e) else {
        hide.phase = HidePhase::Out;
        hide.spot = None;
        return;
    };
    let Ok((mut p, mut tf)) = player.single_mut() else { return };

    match hide.phase {
        HidePhase::Out => {
            spot.doors_target = 0.0;
        }
        HidePhase::Entering(t) => {
            let t = t + dt;
            spot.doors_target = 1.0;
            if t >= HIDE_TRANSITION {
                tf.translation = spot.inside;
                p.yaw = spot.inside_yaw;
                p.pitch = 0.0;
                p.eye = CROUCH_EYE + 0.25;
                hide.phase = HidePhase::Hidden(0.0);
            } else {
                hide.phase = HidePhase::Entering(t);
            }
        }
        HidePhase::Hidden(t) => {
            spot.doors_target = 0.06; // a crack to peek through
            match hide_tick(t, dt, hide.warned) {
                HideTick::Nothing => hide.phase = HidePhase::Hidden(t + dt),
                HideTick::Warn => {
                    hide.warned = true;
                    hide.phase = HidePhase::Hidden(t + dt);
                    subs.write(Subtitle::danger("[Your breathing is getting loud...]"));
                }
                HideTick::ForceOut => {
                    hide.phase = HidePhase::Exiting(0.0);
                    shake.add(0.6);
                    sfx.write(PlaySfx(Sfx::Stinger));
                    subs.write(Subtitle::danger("You can't stay hidden forever. Something shoved you out."));
                }
            }
        }
        HidePhase::Exiting(t) => {
            spot.doors_target = 1.0;
            let t = t + dt;
            if t >= HIDE_TRANSITION * 0.5 && tf.translation != spot.exit {
                tf.translation = spot.exit;
                p.yaw = spot.exit_yaw;
            }
            if t >= HIDE_TRANSITION {
                hide.phase = HidePhase::Out;
                hide.spot = None;
                hide.cooldown = HIDE_COOLDOWN;
                spot.doors_target = 0.0;
            } else {
                hide.phase = HidePhase::Exiting(t);
            }
        }
    }
}

/// Eases doors/drawers toward their targets and drives the model parts.
fn animate_props(
    time: Res<Time>,
    mut doors: Query<(&mut Door, &mut Transform), Without<AnimatedPart>>,
    mut drawers: Query<&mut Drawer>,
    mut spots: Query<&mut HidingSpot>,
    mut parts: Query<(&AnimatedPart, &mut Transform), Without<Door>>,
) {
    let dt = time.delta_secs();
    for (mut door, mut tf) in &mut doors {
        let target = if door.open { 1.0 } else { 0.0 };
        door.t += (target - door.t) * (dt * 2.2).min(1.0);
        // Doors swing away from the player into the next room (+Y rotation).
        tf.rotation = Quat::from_rotation_y(door.t * 1.75);
    }
    for mut d in &mut drawers {
        let target = if d.open { 1.0 } else { 0.0 };
        d.t += (target - d.t) * (dt * 9.0).min(1.0);
    }
    for mut s in &mut spots {
        s.doors_t += (s.doors_target - s.doors_t) * (dt * 10.0).min(1.0);
    }
    for (part, mut tf) in &mut parts {
        let t = if let Ok(d) = drawers.get(part.owner) {
            d.t
        } else if let Ok(s) = spots.get(part.owner) {
            s.doors_t
        } else {
            continue;
        };
        match part.motion {
            PartMotion::Slide(offset) => tf.translation = part.base.translation + offset * t,
            PartMotion::Hinge(angle) => tf.rotation = part.base.rotation * Quat::from_rotation_y(angle * t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_timer_warns_once_then_forces_out() {
        assert_eq!(hide_tick(0.0, 0.1, false), HideTick::Nothing);
        assert_eq!(hide_tick(HIDE_WARN - 0.05, 0.1, false), HideTick::Warn);
        assert_eq!(hide_tick(HIDE_WARN + 0.5, 0.1, true), HideTick::Nothing);
        assert_eq!(hide_tick(HIDE_LIMIT - 0.05, 0.1, true), HideTick::ForceOut);
    }

    #[test]
    fn hidden_fraction_reports_progress() {
        let h = HideState { phase: HidePhase::Hidden(HIDE_LIMIT / 2.0), ..default() };
        assert!((h.hidden_fraction().unwrap() - 0.5).abs() < 1e-6);
        assert!(HideState::default().hidden_fraction().is_none());
    }
}
