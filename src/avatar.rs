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

/// Crouch pose: hips drop 0.33 m; thigh forward and knee bend chosen so the
/// feet stay on the floor: 0.44·cos(thigh) + 0.48·cos(knee − thigh) = 0.57.
pub const CROUCH_THIGH: f32 = 1.0;
pub const CROUCH_KNEE: f32 = 1.8;

#[derive(Component)]
struct Avatar {
    id: u8,
    shown: Vec3,
    yaw: f32,
    pitch: f32,
    phase: f32,
    crouch: f32,
    skin: crate::body::Skin,
}

#[derive(Component)]
struct NameTag(u8);

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
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
            PeerMsg::State(_) | PeerMsg::Admin { .. } => {}
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
            children![(Text::new(session.name_of(*id)), theme.font(true, 15.0), TextColor(color.mix(&Color::WHITE, 0.55)))],
        ));
    }
}

fn spawn_avatar(commands: &mut Commands, model: &crate::body::PlayerModel, id: u8, st: &PeerState) {
    let pos = Vec3::from_array(st.p);
    let skin = st.skin.unwrap_or_else(|| crate::body::Skin::for_id(id));
    commands
        .spawn((
            Avatar { id, shown: pos, yaw: st.yaw, pitch: st.pitch, phase: st.phase, crouch: 0.0, skin },
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
    let mut pose = BodyPose { drop: 0.33 * c, lean: 0.25 * c, head: -pitch * 0.8 - 0.2 * c, ..default() };
    for (i, sgn) in [(0usize, 1.0f32), (1, -1.0)] {
        let s = swing * sgn;
        pose.thigh[i] = s * (1.0 - 0.6 * c) + CROUCH_THIGH * c;
        pose.calf[i] = ((ph.sin() * sgn).max(0.0) * 0.7 * walk) * (1.0 - 0.6 * c) + CROUCH_KNEE * c;
        let held = if i == 0 { right } else { left };
        pose.arm[i] = if held { 0.75 } else { -s * 0.8 };
        pose.forearm[i] = if held { 0.6 } else { 0.2 };
    }
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
            pose.arm_roll = [-0.5 * e, 0.5 * e];
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
) {
    let dt = time.delta_secs();
    let k = (dt * 12.0).min(1.0);
    for (ent, mut av, mut tf, mut vis, mut pose, kids) in &mut avatars {
        let Some(st) = peers.0.get(&av.id) else { continue };
        *vis = if st.alive && !st.hiding { Visibility::Inherited } else { Visibility::Hidden };
        let target = Vec3::from_array(st.p);
        if target.distance(av.shown) > 4.0 {
            av.shown = target; // teleport (hiding spot, respawn)
        }
        av.shown = av.shown.lerp(target, k);
        let dyaw = (st.yaw - av.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        av.yaw += dyaw * k;
        av.pitch += (st.pitch - av.pitch) * k;
        av.phase += (st.phase - av.phase).clamp(-1.0, 1.0) * k;
        av.crouch += ((if st.crouch { 1.0 } else { 0.0 }) - av.crouch) * k;
        tf.translation = av.shown;
        tf.rotation = Quat::from_rotation_y(av.yaw);
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
        let head = av.shown + Vec3::Y * 2.05;
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
        // Thigh 0.44 m (hip to knee), shin and boot 0.48 m (knee to sole).
        let crouched = 0.9 - 0.33 - 0.44 * CROUCH_THIGH.cos() - 0.48 * (CROUCH_KNEE - CROUCH_THIGH).cos();
        assert!(crouched.abs() < 0.03, "crouched feet at {crouched}");
        let standing: f32 = 0.9 - 0.44 - 0.48;
        assert!(standing.abs() < 0.03, "standing feet at {standing}");
    }
}
