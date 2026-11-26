//! Other players in an online room: sends the local player's state, opens
//! doors that teammates opened, and draws each teammate as an animated
//! figure (walk cycle, crouch, head look, held items, flashlight beam and the
//! same gestures as the first-person arms) with a floating name tag.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::floor::FloorRuntime;
use crate::interaction::{Door, HideState, Interactable};
use crate::items::{Inventory, ItemKind};
use crate::net::{NetSession, PeerEvent, PeerMsg, PeerState, Peers, SendPeer};
use crate::physics::Collider;
use crate::player::{Player, PlayerCamera};
use crate::run::RunState;
use crate::state::AppState;
use crate::theme::UiTheme;
use crate::body::{BodyPose, HeldBeam, HeldItem};
use crate::viewmodel::{Gesture, PlayerGesture};

const SEND_HZ: f32 = 15.0;

/// Jacket colours so teammates are easy to tell apart.

/// Crouch pose: both legs swing forward into a squat. The chibi legs have no
/// knees, so the hips drop exactly as far as the legs rise (see
/// [`hip_drop`]) and the feet stay on the floor.
pub const CROUCH_THIGH: f32 = 1.0;

/// How far the hips drop so the lower foot rests on the floor, for the two
/// legs' swing angles (the angle the body actually turns them by).
pub fn hip_drop(swing: [f32; 2]) -> f32 {
    crate::body::LEG_LENGTH * (1.0 - swing[0].cos().max(swing[1].cos()))
}

/// Angle a leg is really turned by (see `body::apply_poses`: a bent calf
/// bends the whole leg a little less).
fn leg_swing(pose: &BodyPose, i: usize) -> f32 {
    pose.thigh[i] - pose.calf[i] * 0.35
}

#[derive(Component)]
struct Avatar {
    id: u8,
    shown: Vec3,
    yaw: f32,
    pitch: f32,
    phase: f32,
    crouch: f32,
    skin: crate::body::Skin,
    /// Seconds since this player died (their body falls, then fades out).
    dead: f32,
}

#[derive(Component)]
struct NameTag(u8);

/// Where living, visible teammates stand: the local player collides with them.
#[derive(Resource, Default)]
pub struct PeerBodies(pub Vec<Vec3>);

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PeerBodies>().add_systems(
            Update,
            (send_state, sync_doors, apply_peer_events, spawn_avatars, animate_avatars, place_name_tags).chain().run_if(in_state(AppState::Playing)),
        );
        // QA: HALLWAYS_AVATAR_VIEW stands three teammates in front of you
        // (still, walking, crouching) to check the models up close.
        if std::env::var_os("HALLWAYS_AVATAR_VIEW").is_some() {
            app.add_systems(Update, avatar_view.before(spawn_avatars).run_if(in_state(AppState::Playing)));
        }
        app
        .add_systems(OnExit(AppState::Playing), |mut c: Commands, q: Query<Entity, Or<(With<Avatar>, With<NameTag>)>>| {
            for e in &q {
                c.entity(e).try_despawn();
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn send_state(
    time: Res<Time>,
    session: Res<NetSession>,
    player: Query<(&Player, &Transform)>,
    gesture: Res<PlayerGesture>,
    inv: Res<Inventory>,
    hide: Res<HideState>,
    run: Res<RunState>,
    overlay: Res<State<crate::state::Overlay>>,
    profile: Res<crate::profile::Profile>,
    mut out: MessageWriter<SendPeer>,
    mut acc: Local<f32>,
) {
    if !session.online() {
        return;
    }
    *acc += time.delta_secs();
    if *acc < 1.0 / SEND_HZ {
        return;
    }
    *acc = 0.0;
    let Ok((p, tf)) = player.single() else { return };
    let (right, left) = crate::viewmodel::held_items(&inv);
    out.write(SendPeer(PeerMsg::State(PeerState {
        p: tf.translation.to_array(),
        yaw: p.yaw,
        pitch: p.pitch,
        crouch: p.crouching,
        speed: p.speed,
        phase: p.bob_phase,
        gesture: gesture.gesture,
        gt: gesture.t,
        right,
        left,
        light: inv.flashlight_on && inv.has(ItemKind::Flashlight),
        hiding: hide.is_hiding(),
        alive: *overlay.get() != crate::state::Overlay::Dead && run.vitality > 0.0,
        room: run.current_room,
        skin: Some(profile.skin),
    })));
}

/// Broadcasts doors this player opened.
fn sync_doors(session: Res<NetSession>, rt: Option<Res<FloorRuntime>>, mut sent: Local<HashSet<u32>>, mut out: MessageWriter<SendPeer>) {
    let Some(rt) = rt else { return };
    if rt.is_added() {
        sent.clear();
    }
    for n in rt.opened.difference(&sent.clone()).copied().collect::<Vec<_>>() {
        sent.insert(n);
        if session.online() {
            out.write(SendPeer(PeerMsg::Door { number: n }));
        }
    }
}

fn apply_peer_events(
    mut commands: Commands,
    session: Res<NetSession>,
    mut events: MessageReader<PeerEvent>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut doors: Query<(Entity, &mut Door)>,
    mut subs: MessageWriter<crate::run::Subtitle>,
) {
    for ev in events.read() {
        match &ev.msg {
            PeerMsg::Door { number } => {
                if let Some(rt) = rt.as_mut() {
                    rt.opened.insert(*number);
                }
                for (e, mut d) in &mut doors {
                    if d.number == *number && !d.mimic && !d.open {
                        d.open = true;
                        d.locked = false;
                        d.key_room = None;
                        if d.blocker != Entity::PLACEHOLDER {
                            commands.entity(d.blocker).try_remove::<Collider>();
                        }
                        commands.entity(e).try_remove::<Interactable>();
                    }
                }
            }
            PeerMsg::Notice { text } => {
                subs.write(crate::run::Subtitle::info(format!("{}: {}", session.name_of(ev.from), text)));
            }
            _ => {}
        }
    }
}

fn avatar_view(time: Res<Time>, player: Query<(&Player, &Transform)>, mut peers: ResMut<Peers>) {
    let Ok((p, tf)) = player.single() else { return };
    let fwd = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let t = time.elapsed_secs();
    for (k, id) in [1u8, 2, 3].into_iter().enumerate() {
        let at = tf.translation + fwd * 2.6 + right * ((k as f32 - 1.0) * 0.9);
        peers.0.insert(
            id,
            PeerState {
                p: at.to_array(),
                yaw: p.yaw + std::f32::consts::PI + (k as f32 - 1.0) * 0.4,
                pitch: 0.0,
                crouch: id == 3,
                speed: if id == 2 { crate::player::WALK_SPEED } else { 0.0 },
                phase: t * 4.2,
                gesture: crate::viewmodel::Gesture::None,
                gt: 0.0,
                right: if id == 1 { Some(ItemKind::Flashlight) } else { None },
                left: None,
                light: false,
                hiding: false,
                alive: true,
                room: 0,
                skin: None,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_avatars(
    mut commands: Commands,
    session: Res<NetSession>,
    peers: Res<Peers>,
    theme: Res<UiTheme>,
    avatars: Query<(Entity, &Avatar)>,
    tags: Query<(Entity, &NameTag)>,
    model: Option<Res<crate::body::PlayerModel>>,
) {
    let Some(model) = model else { return };
    let have: HashMap<u8, Entity> = avatars.iter().map(|(e, a)| (a.id, e)).collect();
    for (id, e) in &have {
        if !peers.0.contains_key(id) {
            commands.entity(*e).try_despawn();
        }
    }
    for (e, t) in &tags {
        if !peers.0.contains_key(&t.0) {
            commands.entity(e).try_despawn();
        }
    }
    for (id, st) in &peers.0 {
        if have.contains_key(id) {
            continue;
        }
        let skin = st.skin.unwrap_or_else(|| crate::body::Skin::for_id(*id));
        let color = skin.shirt_color();
        spawn_avatar(&mut commands, &model, *id, st);
        commands.spawn((
            Node { position_type: PositionType::Absolute, padding: UiRect::axes(px(8), px(2)), border_radius: BorderRadius::all(px(4)), ..default() },
            BackgroundColor(theme.panel.with_alpha(0.8)),
            Visibility::Hidden,
            NameTag(*id),
            Pickable::IGNORE,
            DespawnOnExit(AppState::Playing),
            children![
                (Text::new(session.name_of(*id)), theme.font(true, 15.0), TextColor(color.mix(&Color::WHITE, 0.55)))
            ],
        ));
    }
}

fn spawn_avatar(commands: &mut Commands, model: &crate::body::PlayerModel, id: u8, st: &PeerState) {
    let pos = Vec3::from_array(st.p);
    let skin = st.skin.unwrap_or_else(|| crate::body::Skin::for_id(id));
    commands
        .spawn((
            Avatar { id, shown: pos, yaw: st.yaw, pitch: st.pitch, phase: st.phase, crouch: 0.0, skin, dead: 0.0 },
            BodyPose::default(),
            Transform::from_translation(pos),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Name::new(format!("Avatar {id}")),
        ))
        .with_children(|root| crate::body::spawn_body(root, model, skin, true));
}

/// Pose for a player body from its shared state: walking, running,
/// crouching, holding items and gestures. Also used by cutscene actors.
pub fn pose_for(speed: f32, phase: f32, crouch: f32, pitch: f32, right: bool, left: bool, gesture: Gesture, envelope: f32) -> BodyPose {
    let walk = (speed / crate::player::WALK_SPEED).min(1.6);
    let ph = phase * 0.5;
    let swing = ph.sin() * 0.55 * walk;
    let c = crouch;
    let mut pose = BodyPose { lean: 0.35 * c, head: -pitch * 0.8 - 0.2 * c, ..default() };
    for (i, sgn) in [(0usize, 1.0f32), (1, -1.0)] {
        let s = swing * sgn;
        pose.thigh[i] = s * (1.0 - 0.6 * c) + CROUCH_THIGH * c;
        pose.calf[i] = ((ph.sin() * sgn).max(0.0) * 0.7 * walk) * (1.0 - c);
        let held = if i == 0 { right } else { left };
        pose.arm[i] = if held { 0.75 } else { -s * 0.8 };
        pose.forearm[i] = if held { 0.6 } else { 0.2 };
    }
    // Feet on the floor whatever the legs are doing (squatting, striding).
    pose.drop = hip_drop([leg_swing(&pose, 0), leg_swing(&pose, 1)]);
    let e = envelope;
    match gesture {
        Gesture::Reach => pose.arm[0] += 0.8 * e,
        Gesture::Use => {
            pose.arm[0] += 1.4 * e;
            pose.forearm[0] += 1.0 * e;
        }
        Gesture::Flinch => {
            pose.arm[0] += 0.6 * e;
            pose.arm[1] += 0.6 * e;
            pose.arm_roll = [0.3 * e, 0.3 * e];
        }
        Gesture::Swap => pose.arm[0] -= 0.6 * e,
        _ => {}
    }
    pose
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn animate_avatars(
    mut commands: Commands,
    time: Res<Time>,
    peers: Res<Peers>,
    mut avatars: Query<(Entity, &mut Avatar, &mut Transform, &mut Visibility, &mut BodyPose, &Children), (Without<HeldItem>, Without<HeldBeam>)>,
    mut items: Query<(&HeldItem, &mut Visibility), (Without<Avatar>, Without<HeldBeam>)>,
    mut beams: Query<&mut Visibility, (With<HeldBeam>, Without<Avatar>, Without<HeldItem>)>,
    children: Query<&Children>,
    colliders: Query<&crate::physics::Collider>,
    mut solid: ResMut<PeerBodies>,
) {
    let dt = time.delta_secs();
    let k = (dt * 12.0).min(1.0);
    let boxes: Vec<_> = colliders.iter().map(|c| c.0).collect();
    solid.0.clear();
    for (ent, mut av, mut tf, mut vis, mut pose, kids) in &mut avatars {
        let Some(st) = peers.0.get(&av.id) else { continue };
        av.dead = if st.alive { 0.0 } else { av.dead + dt };
        // A teammate who dies collapses where they stood, then fades from view.
        let fallen = !st.alive && av.dead < 4.0;
        *vis = if (st.alive && !st.hiding) || fallen { Visibility::Inherited } else { Visibility::Hidden };
        let target = Vec3::from_array(st.p);
        if target.distance(av.shown) > 4.0 {
            av.shown = target; // teleport (hiding spot, respawn)
        }
        av.shown = av.shown.lerp(target, k);
        // The body is wider than a player's collision cylinder (and smoothing
        // can cut corners): keep it out of walls so it never pokes through.
        if st.alive && !st.hiding {
            av.shown = crate::physics::resolve_cylinder(av.shown, crate::body::BODY_REACH, av.shown.y + crate::player::STEP_UP, av.shown.y + 1.3, &boxes);
            solid.0.push(av.shown);
        }
        let dyaw = (st.yaw - av.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        av.yaw += dyaw * k;
        av.pitch += (st.pitch - av.pitch) * k;
        av.phase += (st.phase - av.phase).clamp(-1.0, 1.0) * k;
        av.crouch += ((if st.crouch { 1.0 } else { 0.0 }) - av.crouch) * k;
        tf.translation = av.shown;
        tf.rotation = Quat::from_rotation_y(av.yaw);
        if fallen {
            let k = crate::player::death_fall(av.dead);
            tf.rotation = Quat::from_rotation_y(av.yaw) * Quat::from_rotation_x(1.5 * k);
            tf.translation.y += 0.12 * k;
            *pose = BodyPose { arm: [0.6 * k, 0.4 * k], arm_roll: [1.0 * k, 1.0 * k], thigh: [0.25 * k, 0.05], calf: [0.4 * k, 0.1], head: -0.3 * k, ..default() };
            continue;
        }
        *pose = pose_for(st.speed, av.phase, av.crouch, av.pitch, st.right.is_some(), st.left.is_some(), st.gesture, st.gesture.envelope(st.gt));
        // A teammate who changed their look.
        if let Some(skin) = st.skin
            && skin != av.skin
        {
            av.skin = skin;
            crate::body::reskin(&mut commands, ent, skin);
        }
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok(ch) = children.get(e) {
                stack.extend(ch.iter());
            }
            if let Ok((it, mut v)) = items.get_mut(e) {
                let held = if it.right { st.right } else { st.left };
                *v = if held == Some(it.kind) { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let Ok(mut v) = beams.get_mut(e) {
                *v = if st.light { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

fn place_name_tags(
    cam: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    avatars: Query<(&Avatar, &Visibility)>,
    mut tags: Query<(&NameTag, &mut Node, &mut Visibility, &ComputedNode), Without<Avatar>>,
    ui_scale: Res<UiScale>,
) {
    let k = ui_scale.0.max(0.01);
    let Ok((camera, cam_tf)) = cam.single() else { return };
    for (tag, mut node, mut vis, computed) in &mut tags {
        let Some((av, av_vis)) = avatars.iter().find(|(a, _)| a.id == tag.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let head = av.shown + Vec3::Y * 1.75;
        let dist = cam_tf.translation().distance(head);
        let on_screen = camera.world_to_viewport(cam_tf, head).ok().filter(|_| *av_vis != Visibility::Hidden && dist < 30.0);
        match on_screen {
            Some(p) => {
                // Node offsets are in UI units (scaled by UiScale); viewport points are logical pixels.
                let size = computed.size() * computed.inverse_scale_factor();
                node.left = px(p.x / k - size.x / 2.0);
                node.top = px(p.y / k - size.y);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feet_stay_on_the_floor() {
        // Crouched, standing and mid-stride: the lower sole is at floor level
        // (hips at LEG_LENGTH - drop, the foot LEG_LENGTH·cos(swing) below).
        for (speed, phase, crouch) in [(0.0, 0.0, 1.0), (0.0, 0.0, 0.0), (4.0, 1.3, 0.0), (2.0, 2.0, 1.0), (1.0, 0.7, 0.5)] {
            let pose = pose_for(speed, phase, crouch, 0.0, false, false, Gesture::None, 0.0);
            let hips = crate::body::LEG_LENGTH - pose.drop;
            let lowest = (0..2).map(|i| hips - crate::body::LEG_LENGTH * leg_swing(&pose, i).cos()).fold(f32::INFINITY, f32::min);
            assert!(lowest.abs() < 1e-4, "speed {speed} crouch {crouch}: sole at {lowest}");
            assert!(pose.drop < crate::body::LEG_LENGTH * 0.6, "the body stays above the floor");
        }
    }
}
