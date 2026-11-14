//! Other players in an online room: sends the local player's state, opens
//! doors that teammates opened, and draws each teammate as an animated
//! figure (walk cycle, crouch, head look, held items, flashlight beam and the
//! same gestures as the first-person arms) with a floating name tag.

use std::collections::{HashMap, HashSet};

use bevy::light::NotShadowCaster;
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
use crate::viewmodel::{build_hand, build_item, grip_transform, BodyKit, FingerSet, Gesture, PlayerGesture};

const SEND_HZ: f32 = 15.0;

/// Jacket colours so teammates are easy to tell apart.
const COLORS: [Color; 6] = [
    Color::srgb(0.62, 0.16, 0.14),
    Color::srgb(0.14, 0.38, 0.58),
    Color::srgb(0.2, 0.46, 0.22),
    Color::srgb(0.58, 0.46, 0.12),
    Color::srgb(0.42, 0.2, 0.52),
    Color::srgb(0.12, 0.48, 0.48),
];

#[derive(Component)]
struct Avatar {
    id: u8,
    shown: Vec3,
    yaw: f32,
    pitch: f32,
    phase: f32,
    crouch: f32,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Limb {
    Hips,
    Head,
    Thigh(bool),
    Shin(bool),
    Arm(bool),
    Forearm(bool),
}

#[derive(Component)]
struct AvatarItem {
    kind: ItemKind,
    right: bool,
}

#[derive(Component)]
struct AvatarBeam;

#[derive(Component)]
struct NameTag(u8);

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (send_state, sync_doors, apply_peer_events, spawn_avatars, animate_avatars, place_name_tags).chain().run_if(in_state(AppState::Playing)),
        )
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

#[allow(clippy::too_many_arguments)]
fn spawn_avatars(
    mut commands: Commands,
    session: Res<NetSession>,
    peers: Res<Peers>,
    theme: Res<UiTheme>,
    avatars: Query<(Entity, &Avatar)>,
    tags: Query<(Entity, &NameTag)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
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
        let color = COLORS[*id as usize % COLORS.len()];
        let kit = BodyKit::new(&mut mats, &mut meshes, color);
        spawn_avatar(&mut commands, &kit, *id, st, &mut mats);
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

fn spawn_avatar(commands: &mut Commands, kit: &BodyKit, id: u8, st: &PeerState, mats: &mut Assets<StandardMaterial>) {
    let m = |mats: &mut Assets<StandardMaterial>, c: Color, rough: f32| mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, ..default() });
    let pants = m(mats, Color::srgb(0.12, 0.13, 0.16), 0.92);
    let patch = m(mats, Color::srgb(0.17, 0.18, 0.21), 0.95);
    let shoe = m(mats, Color::srgb(0.16, 0.1, 0.06), 0.55);
    let sole = m(mats, Color::srgb(0.03, 0.03, 0.03), 0.8);
    let hair = m(mats, [Color::srgb(0.12, 0.08, 0.05), Color::srgb(0.05, 0.04, 0.04), Color::srgb(0.42, 0.28, 0.14), Color::srgb(0.6, 0.48, 0.3)][id as usize % 4], 0.85);
    let white = m(mats, Color::srgb(0.93, 0.92, 0.9), 0.4);
    let pupil = m(mats, Color::srgb(0.03, 0.03, 0.04), 0.2);
    let lip = m(mats, Color::srgb(0.55, 0.3, 0.28), 0.6);
    let shirt = m(mats, Color::srgb(0.85, 0.84, 0.8), 0.9);
    let leather = m(mats, Color::srgb(0.2, 0.12, 0.07), 0.6);
    let metal = mats.add(StandardMaterial { base_color: Color::srgb(0.7, 0.68, 0.6), metallic: 0.9, perceptual_roughness: 0.35, ..default() });
    let pack = m(mats, Color::srgb(0.2, 0.24, 0.17), 0.9);
    let pos = Vec3::from_array(st.p);
    let piece = |c: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, at: Vec3, size: Vec3| {
        c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(size), NotShadowCaster));
    };
    let tilted = |c: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, at: Vec3, rot: Quat, size: Vec3| {
        c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_rotation(rot).with_scale(size), NotShadowCaster));
    };
    commands
        .spawn((
            Avatar { id, shown: pos, yaw: st.yaw, pitch: st.pitch, phase: st.phase, crouch: 0.0 },
            Transform::from_translation(pos),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Name::new(format!("Avatar {id}")),
        ))
        .with_children(|root| {
            // Legs hang from the hips; the upper body sits on top of them.
            for right in [true, false] {
                let x = if right { 0.1 } else { -0.1 };
                root.spawn((Limb::Thigh(right), Transform::from_xyz(x, 0.9, 0.0), Visibility::default())).with_children(|thigh| {
                    piece(thigh, &kit.capsule, &pants, Vec3::new(0.0, -0.22, 0.0), Vec3::new(0.16, 0.21, 0.16));
                    // Cargo pocket on the outside of the thigh.
                    piece(thigh, &kit.cube, &patch, Vec3::new(if right { 0.075 } else { -0.075 }, -0.24, 0.0), Vec3::new(0.025, 0.12, 0.1));
                    thigh.spawn((Limb::Shin(right), Transform::from_xyz(0.0, -0.44, 0.0), Visibility::default())).with_children(|shin| {
                        piece(shin, &kit.capsule, &pants, Vec3::new(0.0, -0.19, 0.0), Vec3::new(0.13, 0.19, 0.13));
                        // Knee patch.
                        piece(shin, &kit.cube, &patch, Vec3::new(0.0, -0.02, -0.06), Vec3::new(0.1, 0.09, 0.03));
                        // Boot: upper, toe cap, sole and laces.
                        piece(shin, &kit.capsule, &shoe, Vec3::new(0.0, -0.36, 0.0), Vec3::new(0.125, 0.09, 0.13));
                        piece(shin, &kit.cube, &shoe, Vec3::new(0.0, -0.42, -0.07), Vec3::new(0.115, 0.08, 0.2));
                        piece(shin, &kit.sphere, &shoe, Vec3::new(0.0, -0.425, -0.16), Vec3::new(0.115, 0.075, 0.08));
                        piece(shin, &kit.cube, &sole, Vec3::new(0.0, -0.468, -0.06), Vec3::new(0.125, 0.025, 0.29));
                        for k in 0..3 {
                            piece(shin, &kit.cube, &sole, Vec3::new(0.0, -0.385 + k as f32 * 0.025, -0.075 - k as f32 * 0.005), Vec3::new(0.07, 0.006, 0.01));
                        }
                    });
                });
            }
            root.spawn((Limb::Hips, Transform::from_xyz(0.0, 0.9, 0.0), Visibility::default())).with_children(|hips| {
                // Pelvis, belt and buckle.
                piece(hips, &kit.cube, &pants, Vec3::new(0.0, 0.02, 0.0), Vec3::new(0.34, 0.15, 0.21));
                piece(hips, &kit.cube, &leather, Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.355, 0.045, 0.225));
                piece(hips, &kit.cube, &metal, Vec3::new(0.0, 0.1, -0.114), Vec3::new(0.05, 0.035, 0.008));
                // Jacket: tapered torso (chest wider than waist), open collar over a shirt.
                piece(hips, &kit.cube, &kit.sleeve, Vec3::new(0.0, 0.22, 0.0), Vec3::new(0.34, 0.2, 0.21));
                piece(hips, &kit.capsule, &kit.sleeve, Vec3::new(0.0, 0.38, 0.0), Vec3::new(0.44, 0.2, 0.25));
                piece(hips, &kit.sphere, &kit.sleeve, Vec3::new(-0.15, 0.47, 0.0), Vec3::new(0.15, 0.13, 0.22));
                piece(hips, &kit.sphere, &kit.sleeve, Vec3::new(0.15, 0.47, 0.0), Vec3::new(0.15, 0.13, 0.22));
                piece(hips, &kit.cube, &shirt, Vec3::new(0.0, 0.5, -0.118), Vec3::new(0.09, 0.1, 0.01));
                for side in [-1.0f32, 1.0] {
                    // Collar flaps and chest pocket.
                    tilted(hips, &kit.cube, &kit.cuff, Vec3::new(side * 0.06, 0.52, -0.12), Quat::from_rotation_z(side * 0.5), Vec3::new(0.05, 0.1, 0.012));
                    piece(hips, &kit.cube, &kit.cuff, Vec3::new(side * 0.1, 0.18, -0.108), Vec3::new(0.09, 0.08, 0.01));
                }
                // Zip down the front.
                piece(hips, &kit.cube, &metal, Vec3::new(0.0, 0.3, -0.127), Vec3::new(0.008, 0.36, 0.006));
                // Backpack with straps over the shoulders.
                piece(hips, &kit.cube, &pack, Vec3::new(0.0, 0.33, 0.16), Vec3::new(0.27, 0.32, 0.12));
                piece(hips, &kit.cube, &pack, Vec3::new(0.0, 0.25, 0.225), Vec3::new(0.2, 0.12, 0.04));
                piece(hips, &kit.cube, &metal, Vec3::new(0.0, 0.45, 0.225), Vec3::new(0.12, 0.012, 0.006));
                for side in [-1.0f32, 1.0] {
                    piece(hips, &kit.cube, &leather, Vec3::new(side * 0.1, 0.36, -0.12), Vec3::new(0.035, 0.32, 0.01));
                    piece(hips, &kit.cube, &leather, Vec3::new(side * 0.1, 0.54, 0.02), Vec3::new(0.035, 0.012, 0.27));
                }
                // Neck.
                piece(hips, &kit.capsule, &kit.skin, Vec3::new(0.0, 0.6, 0.0), Vec3::new(0.09, 0.06, 0.09));
                hips.spawn((Limb::Head, Transform::from_xyz(0.0, 0.66, 0.0), Visibility::default())).with_children(|head| {
                    piece(head, &kit.sphere, &kit.skin, Vec3::new(0.0, 0.11, 0.0), Vec3::new(0.2, 0.25, 0.22));
                    // Jaw and chin.
                    piece(head, &kit.sphere, &kit.skin, Vec3::new(0.0, 0.035, -0.03), Vec3::new(0.16, 0.12, 0.16));
                    // Ears.
                    for x in [-0.1f32, 0.1] {
                        piece(head, &kit.sphere, &kit.skin, Vec3::new(x, 0.11, 0.0), Vec3::new(0.03, 0.06, 0.045));
                    }
                    // Hair: cap, back and a fringe over the forehead.
                    piece(head, &kit.sphere, &hair, Vec3::new(0.0, 0.18, 0.02), Vec3::new(0.215, 0.16, 0.23));
                    piece(head, &kit.sphere, &hair, Vec3::new(0.0, 0.12, 0.055), Vec3::new(0.205, 0.2, 0.15));
                    tilted(head, &kit.cube, &hair, Vec3::new(0.025, 0.2, -0.1), Quat::from_rotation_z(-0.3), Vec3::new(0.11, 0.035, 0.04));
                    // Eyes (whites + pupils), brows, nose and mouth.
                    for x in [-0.042f32, 0.042] {
                        piece(head, &kit.sphere, &white, Vec3::new(x, 0.125, -0.098), Vec3::new(0.034, 0.024, 0.02));
                        piece(head, &kit.sphere, &pupil, Vec3::new(x, 0.125, -0.107), Vec3::splat(0.014));
                        tilted(head, &kit.cube, &hair, Vec3::new(x, 0.152, -0.1), Quat::from_rotation_z(x.signum() * 0.12), Vec3::new(0.04, 0.008, 0.012));
                    }
                    piece(head, &kit.cube, &kit.skin, Vec3::new(0.0, 0.095, -0.112), Vec3::new(0.022, 0.045, 0.03));
                    piece(head, &kit.cube, &lip, Vec3::new(0.0, 0.052, -0.098), Vec3::new(0.045, 0.01, 0.01));
                });
                for right in [true, false] {
                    let x = if right { 0.25 } else { -0.25 };
                    hips.spawn((Limb::Arm(right), Transform::from_xyz(x, 0.52, 0.0), Visibility::default())).with_children(|arm| {
                        piece(arm, &kit.capsule, &kit.sleeve, Vec3::new(0.0, -0.15, 0.0), Vec3::new(0.115, 0.15, 0.115));
                        // Elbow patch.
                        piece(arm, &kit.sphere, &kit.cuff, Vec3::new(0.0, -0.29, 0.03), Vec3::new(0.08, 0.07, 0.06));
                        arm.spawn((Limb::Forearm(right), Transform::from_xyz(0.0, -0.3, 0.0), Visibility::default())).with_children(|fore| {
                            piece(fore, &kit.capsule, &kit.sleeve, Vec3::new(0.0, -0.13, 0.0), Vec3::new(0.1, 0.12, 0.1));
                            // Rolled cuff at the wrist.
                            piece(fore, &kit.cyl, &kit.cuff, Vec3::new(0.0, -0.245, 0.0), Vec3::new(0.105, 0.04, 0.105));
                            // Hand at the wrist, fingers pointing down the arm.
                            fore.spawn((Transform::from_xyz(0.0, -0.29, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), Visibility::default())).with_children(|hand| {
                                build_hand(hand, kit, right, 1.3);
                                for kind in ItemKind::ALL {
                                    hand.spawn((AvatarItem { kind, right }, grip_transform(right, 1.3, true), Visibility::Hidden)).with_children(|it| build_item(it, kit, kind, 1.3));
                                }
                                if !right {
                                    hand.spawn((
                                        AvatarBeam,
                                        SpotLight { intensity: 400_000.0, range: 14.0, outer_angle: 0.42, inner_angle: 0.25, shadow_maps_enabled: false, color: Color::srgb(1.0, 0.95, 0.85), ..default() },
                                        Transform::from_xyz(0.0, 0.03, -0.25),
                                        Visibility::Hidden,
                                    ));
                                }
                            });
                        });
                    });
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn animate_avatars(
    time: Res<Time>,
    peers: Res<Peers>,
    mut avatars: Query<(&mut Avatar, &mut Transform, &mut Visibility, &Children), (Without<Limb>, Without<AvatarItem>, Without<AvatarBeam>)>,
    mut limbs: Query<(&Limb, &mut Transform), Without<Avatar>>,
    mut items: Query<(&AvatarItem, &mut Visibility), (Without<Avatar>, Without<AvatarBeam>)>,
    mut beams: Query<&mut Visibility, (With<AvatarBeam>, Without<Avatar>, Without<AvatarItem>)>,
    mut fingers: Query<(&FingerSet, &mut Visibility), (Without<Avatar>, Without<AvatarItem>, Without<AvatarBeam>)>,
    children: Query<&Children>,
) {
    let dt = time.delta_secs();
    let k = (dt * 12.0).min(1.0);
    for (mut av, mut tf, mut vis, kids) in &mut avatars {
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

        let walk = (st.speed / crate::player::WALK_SPEED).min(1.6);
        let ph = av.phase * 0.5;
        let swing = ph.sin() * 0.55 * walk;
        let e = st.gesture.envelope(st.gt);
        let c = av.crouch;
        let holding_r = st.right.is_some();
        let holding_l = st.left.is_some();
        // Walk all descendants once and pose each limb.
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(ent) = stack.pop() {
            if let Ok(ch) = children.get(ent) {
                stack.extend(ch.iter());
            }
            if let Ok((limb, mut lt)) = limbs.get_mut(ent) {
                match *limb {
                    Limb::Hips => {
                        lt.translation.y = 0.9 - 0.33 * c;
                        lt.rotation = Quat::from_rotation_x(-0.25 * c);
                    }
                    Limb::Head => lt.rotation = Quat::from_rotation_x(av.pitch * 0.8 + 0.25 * c),
                    Limb::Thigh(right) => {
                        let s = if right { swing } else { -swing };
                        lt.translation.y = 0.9 - 0.33 * c;
                        lt.rotation = Quat::from_rotation_x(s - 1.0 * c);
                    }
                    Limb::Shin(right) => {
                        let s = if right { ph.sin() } else { -ph.sin() };
                        lt.rotation = Quat::from_rotation_x((s.max(0.0) * 0.7 * walk) + 1.5 * c);
                    }
                    Limb::Arm(right) => {
                        let s = if right { -swing } else { swing };
                        let held = if right { holding_r } else { holding_l };
                        let mut x = if held { -0.75 } else { s * 0.8 };
                        let mut z = if right { 0.08 } else { -0.08 };
                        match st.gesture {
                            Gesture::Reach if right => x -= 0.8 * e,
                            Gesture::Use if right => x -= 1.4 * e,
                            Gesture::Flinch => {
                                x -= 0.6 * e;
                                z *= 1.0 + 3.0 * e;
                            }
                            Gesture::Swap if right => x += 0.6 * e,
                            _ => {}
                        }
                        lt.rotation = Quat::from_rotation_x(x) * Quat::from_rotation_z(z);
                    }
                    Limb::Forearm(right) => {
                        let held = if right { holding_r } else { holding_l };
                        let mut bend = if held { -0.6 } else { -0.2 };
                        if right && st.gesture == Gesture::Use {
                            bend -= 1.0 * e;
                        }
                        lt.rotation = Quat::from_rotation_x(bend);
                    }
                }
            }
            if let Ok((it, mut v)) = items.get_mut(ent) {
                let held = if it.right { st.right } else { st.left };
                *v = if held == Some(it.kind) { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let Ok((f, mut v)) = fingers.get_mut(ent) {
                let held = if f.right { holding_r } else { holding_l };
                *v = if held == f.grip { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let Ok(mut v) = beams.get_mut(ent) {
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
