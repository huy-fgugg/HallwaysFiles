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
/// Vitality lost for opening a fake door on a choice wall.
pub const FAKE_DOOR_DAMAGE: f32 = 25.0;
pub const HIDE_LIMIT: f32 = 10.0;
pub const HIDE_WARN: f32 = 7.0;
const HIDE_TRANSITION: f32 = 0.45;
const HIDE_COOLDOWN: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractKind {
    Door,
    Drawer,
    HidingSpot,
    /// The Skin table in the boarding hall: opens the customise screen.
    Skin,
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
    /// A fake door showing the wrong number: opening it hurts.
    pub mimic: bool,
    /// Locked until the key from this room's drawers is found.
    pub key_room: Option<u32>,
}

impl Door {
    pub fn new(number: u32, locked: bool, blocker: Entity) -> Self {
        Self { number, locked, open: false, t: 0.0, blocker, mimic: false, key_room: None }
    }
}

#[derive(Component, Debug)]
pub struct Drawer {
    pub open: bool,
    pub t: f32,
    pub loot: Option<u32>,
    pub searched: bool,
    /// Holds the key for this room's locked exit door.
    pub has_key: bool,
    /// (room, drawer id) for persistence on the generated floor.
    pub tag: Option<(u32, u8)>,
    /// An item found inside instead of (or as well as) Lumens.
    pub item: Option<crate::items::ItemKind>,
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

/// True while a rushing entity is on its way or passing; pauses the hide timer.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ThreatActive(pub bool);

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
    /// Which place inside the spot (0 = middle, 1 = beside a teammate).
    pub place: usize,
}

/// How many players fit in one wardrobe or locker.
pub const SPOT_CAPACITY: usize = 2;

/// Teammates already hiding in the spot whose inside point is `inside`.
pub fn spot_occupants(inside: Vec3, peers: impl Iterator<Item = (Vec3, bool)>) -> usize {
    peers.filter(|(p, hiding)| *hiding && p.distance(inside) < 0.7).count()
}

/// Where a player stands inside a spot: the middle, or shuffled to one side
/// when a teammate is already in there.
pub fn place_inside(inside: Vec3, yaw: f32, place: usize) -> Vec3 {
    inside + Quat::from_rotation_y(yaw) * Vec3::X * (0.26 * place as f32)
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
            if d.locked && d.key_room.is_some() {
                (format!("Door {:03}  (needs key)", d.number), true)
            } else if d.locked {
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
        InteractKind::Skin => ("Skin".into(), false),
    })
}

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractTarget>()
            .init_resource::<ThreatActive>()
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
    mut runtime: Option<ResMut<crate::floor::FloorRuntime>>,
    mimics: Query<&crate::floor::build::MimicOf>,
    (mut shake, mut death, mut next_overlay): (ResMut<CameraShake>, ResMut<crate::hollow::DeathCause>, ResMut<NextState<crate::state::Overlay>>),
    (mut inv, item_cfg, mut arms, peers): (ResMut<crate::items::Inventory>, Res<crate::data::ItemConfig>, MessageWriter<crate::viewmodel::ArmAnim>, Res<crate::net::Peers>),
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
    arms.write(crate::viewmodel::ArmAnim(crate::viewmodel::Gesture::Reach));
    match i.kind {
        InteractKind::Door => {
            let Ok(mut door) = doors.get_mut(e) else { return };
            if door.mimic {
                // Fake door: it lunges. Damage, scare, and a lesson.
                commands.entity(e).try_remove::<Interactable>();
                if let (Some(rt), Ok(m)) = (runtime.as_mut(), mimics.get(e)) {
                    rt.mimics_triggered.insert((m.0, m.1));
                }
                run.mimics_fooled += 1;
                shake.add(0.8);
                sfx.write(PlaySfx(Sfx::Stinger));
                subs.write(Subtitle::danger(format!("Wrong door! It said {:03}, not {:03}. -{} vitality", door.number, run.current_room + 1, FAKE_DOOR_DAMAGE as u32)));
                if run.damage(FAKE_DOOR_DAMAGE) {
                    *death = crate::hollow::DeathCause {
                        title: "YOU PICKED THE WRONG DOOR".into(),
                        cause: format!("You opened a fake door numbered {:03} in Room {:03}.", door.number, run.current_room),
                        lesson: "Real doors count up by one. Read the plaque before you open: if the number is wrong, it's a Mimic.".into(),
                    };
                    next_overlay.set(crate::state::Overlay::Dead);
                }
            } else if door.locked {
                let has_key = door.key_room.is_some_and(|k| runtime.as_ref().is_some_and(|rt| rt.keys.contains(&k)));
                if has_key {
                    door.locked = false;
                    door.key_room = None;
                    door.open = true;
                    if door.blocker != Entity::PLACEHOLDER {
                        commands.entity(door.blocker).try_remove::<Collider>();
                    }
                    commands.entity(e).try_remove::<Interactable>();
                    if let Some(rt) = runtime.as_mut() {
                        rt.opened.insert(door.number);
                    }
                    sfx.write(PlaySfx(Sfx::DoorOpen));
                    subs.write(Subtitle::info("[The key turns. The door creaks open]"));
                } else if door.key_room.is_some() && (inv.take(crate::items::ItemKind::SkeletonKey) || inv.take(crate::items::ItemKind::Lockpick)) {
                    door.locked = false;
                    door.key_room = None;
                    door.open = true;
                    if door.blocker != Entity::PLACEHOLDER {
                        commands.entity(door.blocker).try_remove::<Collider>();
                    }
                    commands.entity(e).try_remove::<Interactable>();
                    if let Some(rt) = runtime.as_mut() {
                        rt.opened.insert(door.number);
                    }
                    sfx.write(PlaySfx(Sfx::DoorLocked));
                    sfx.write(PlaySfx(Sfx::DoorOpen));
                    subs.write(Subtitle::info("[Click] You work the lock open."));
                } else if door.key_room.is_some() {
                    sfx.write(PlaySfx(Sfx::DoorLocked));
                    subs.write(Subtitle::info("Locked. The key is in one of this room's drawers (or use a Lockpick)."));
                } else {
                    sfx.write(PlaySfx(Sfx::DoorLocked));
                    subs.write(Subtitle::info("[Handle rattles] It won't budge. Not yet."));
                }
            } else if !door.open {
                door.open = true;
                if door.blocker != Entity::PLACEHOLDER {
                    commands.entity(door.blocker).try_remove::<Collider>();
                }
                commands.entity(e).try_remove::<Interactable>();
                if let Some(rt) = runtime.as_mut() {
                    rt.opened.insert(door.number);
                }
                sfx.write(PlaySfx(Sfx::DoorOpen));
                subs.write(Subtitle::info("[Door creaks open]"));
            }
        }
        InteractKind::Drawer => {
            let Ok(mut drawer) = drawers.get_mut(e) else { return };
            drawer.open = !drawer.open;
            if drawer.open {
                sfx.write(PlaySfx(Sfx::DrawerOpen));
                let had_item = drawer.item.is_some();
                if !drawer.searched {
                    drawer.searched = true;
                    if let (Some(rt), Some(tag)) = (runtime.as_mut(), drawer.tag) {
                        rt.searched.insert(tag);
                        if drawer.has_key {
                            rt.keys.insert(tag.0);
                        }
                    }
                    if drawer.has_key {
                        sfx.write(PlaySfx(Sfx::Pickup));
                        subs.write(Subtitle::info(format!("Found the key to Door {:03}", drawer.tag.map_or(0, |t| t.0 + 1))));
                    }
                    if let Some(item) = drawer.item.take() {
                        use crate::items::AddResult;
                        let name = item.tuning(&item_cfg).name.clone();
                        sfx.write(PlaySfx(Sfx::Pickup));
                        let msg = match inv.add(item, &item_cfg) {
                            AddResult::Recharged => "Found batteries: flashlight recharged.".to_string(),
                            AddResult::StackFull => format!("Found a {name}, but you can't carry more."),
                            AddResult::Replaced(old) => format!("Found a {name}. Dropped your {} to make room.", old.tuning(&item_cfg).name),
                            _ if item == crate::items::ItemKind::Flashlight => format!("Found a {name}. Press F to switch it on."),
                            _ if item == crate::items::ItemKind::LuckyCharm => format!("Found a {name}! It will save you once from dying."),
                            _ => format!("Found a {name}. Select it (1-3) and press Q to use."),
                        };
                        // Rare finds say so.
                        let msg = match item.rarity(&item_cfg) {
                            r @ (crate::items::Rarity::Rare | crate::items::Rarity::VeryRare) => format!("[{}] {msg}", r.label()),
                            _ => msg,
                        };
                        subs.write(Subtitle::info(msg));
                        // Keep the Lumens message from overwriting the item message.
                        if drawer.loot.is_none() {
                            return;
                        }
                    }
                    match drawer.loot.take() {
                        Some(n) => {
                            run.lumens += n;
                            sfx.write(PlaySfx(Sfx::Pickup));
                            subs.write(Subtitle::info(if n >= crate::floor::build::POUCH_MIN { format!("A pouch of Lumens! (+{n})") } else { format!("Found {n} Lumens") }));
                        }
                        None if !drawer.has_key && !had_item => {
                            subs.write(Subtitle::info("Empty."));
                        }
                        None => {}
                    }
                }
            } else {
                sfx.write(PlaySfx(Sfx::DrawerClose));
            }
        }
        InteractKind::Skin => next_overlay.set(crate::state::Overlay::Skin),
        InteractKind::HidingSpot => {
            let Ok(spot) = spots.get(e) else { return };
            if hide.cooldown > 0.0 {
                return;
            }
            let inside = spot_occupants(spot.inside, peers.0.values().map(|p| (Vec3::from_array(p.p), p.hiding)));
            if inside >= SPOT_CAPACITY {
                subs.write(Subtitle::info("[No room - two of you are already in there]"));
                return;
            }
            hide.place = inside;
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
    threat: Res<ThreatActive>,
    rules: Res<crate::presets::ModeRules>,
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
                tf.translation = place_inside(spot.inside, spot.inside_yaw, hide.place);
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
            // You hold your breath while something is out there: the timer pauses.
            let dt = if threat.0 { 0.0 } else { dt * rules.hide_rate };
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
        tf.rotation = Quat::from_rotation_y(door.t * crate::floor::plan::DOOR_SWING);
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
        // Two fit in a spot; the second stands beside the first.
        let inside = Vec3::new(1.0, 0.0, 2.0);
        assert_eq!(spot_occupants(inside, [(inside, true), (inside + Vec3::X * 0.26, true), (inside, false), (Vec3::ZERO, true)].into_iter()), 2);
        assert!(place_inside(inside, 0.3, 1).distance(inside) > 0.2);
        assert_eq!(place_inside(inside, 0.3, 0), inside);
        assert!((h.hidden_fraction().unwrap() - 0.5).abs() < 1e-6);
        assert!(HideState::default().hidden_fraction().is_none());
    }
}
