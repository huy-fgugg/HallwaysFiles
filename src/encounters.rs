//! Phase 3 entities that you counter with how you *look* or *move*:
//!
//! * **The Whisperer** (dark rooms): whispering starts behind you, then a pale,
//!   elongated head with hollow sockets drifts out of the dark. **Look straight
//!   at it** before the window closes and it scatters.
//! * **The Gallery**: the lights turn violet and clusters of eyes open in the
//!   walls. **Don't look at them**: vitality drains while you do. They close
//!   when you leave the room.
//! * **The Hush**: the air goes heavy, a violet vignette closes in, then
//!   "STAY STILL". **Don't move** until it lifts.
//!
//! Timings and damage come from `assets/data/entities.ron`.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::data::EntityConfig;
use crate::floor::plan::{Encounter, RoomPlan};
use crate::floor::{Floor, FloorRuntime};
use crate::hollow::DeathCause;
use crate::interaction::HideState;
use crate::player::{Player, PlayerCamera};
use crate::rng::GameRng;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};
use crate::theme::UiTheme;
use crate::world::Mood;

/// cos(16°): how directly you must face a target for it to count as "looking".
pub const LOOK_COS: f32 = 0.96;

/// True if a camera at `cam` facing `forward` (normalised) is looking at `target`.
pub fn is_looking_at(cam: Vec3, forward: Vec3, target: Vec3, cos: f32) -> bool {
    let to = target - cam;
    let d = to.length();
    d > 0.01 && forward.dot(to / d) >= cos
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Active {
    #[default]
    None,
    Whisperer { room: u32, t: f32, spawned: bool },
    Gallery { room: u32, t: f32, look_time: f32, beat: f32, warned: bool },
    Hush { room: u32, t: f32 },
}

#[derive(Resource, Debug, Default)]
pub struct EncounterState(pub Active);

/// Lets the capture tour force an encounter for screenshots.
#[derive(Resource, Debug, Default)]
pub struct ForceEncounter(pub Option<(u32, Encounter)>);

#[derive(Component)]
struct WhispererBody;
/// One of the Whisperer's reaching arms (-1 left, 1 right).
#[derive(Component)]
struct WhisperArm(f32);
/// A trailing wisp: its angle around the body and length.
#[derive(Component)]
struct WhisperWisp {
    angle: f32,
    len: f32,
}
/// The Hush: a tall faceless figure that appears in a corner and holds a
/// finger to where its lips should be while you must stay still.
#[derive(Component)]
struct HushFigure {
    room: u32,
    /// Seconds since it appeared, and how visible it is (0..1).
    t: f32,
    shown: f32,
}
#[derive(Component)]
struct HushHead;
#[derive(Component)]
struct HushArm;
#[derive(Component)]
struct GalleryEye {
    open: f32,
    seed: f32,
}
#[derive(Component)]
struct GalleryIris;
#[derive(Component)]
struct HushOverlay;
#[derive(Component)]
struct HushText;

pub struct EncountersPlugin;

impl Plugin for EncountersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EncounterState>()
            .init_resource::<ForceEncounter>()
            .add_systems(OnEnter(AppState::Playing), (spawn_hush_ui, |mut c: Commands| c.insert_resource(EncounterState::default())))
            .add_systems(Update, (trigger, whisperer, gallery, hush).chain().run_if(gameplay_active))
            .add_systems(Update, (animate_whisperer, animate_eyes, hush_ui, hush_figure).run_if(in_state(AppState::Playing)));
        // QA: HALLWAYS_MONSTER_VIEW=whisperer|hush|surge|undertow stands that
        // monster in front of you to check its look and animation.
        if std::env::var_os("HALLWAYS_MONSTER_VIEW").is_some() {
            app.add_systems(Update, monster_view.run_if(in_state(AppState::Playing)));
        }
    }
}

fn room_encounter(r: &RoomPlan, force: &ForceEncounter) -> Option<Encounter> {
    match force.0 {
        Some((room, e)) if room == r.index => Some(e),
        _ => r.encounter,
    }
}

#[allow(clippy::too_many_arguments)]
fn trigger(
    mut commands: Commands,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    force: Res<ForceEncounter>,
    mut state: ResMut<EncounterState>,
    mut mood: ResMut<Mood>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    if state.0 != Active::None {
        return;
    }
    let Some(r) = floor.0.room(run.current_room) else { return };
    if rt.encounters_done.contains(&r.index) {
        return;
    }
    match room_encounter(r, &force) {
        Some(Encounter::Whisperer) => {
            state.0 = Active::Whisperer { room: r.index, t: 0.0, spawned: false };
            sfx.write(PlaySfx(Sfx::Whisper));
            subs.write(Subtitle::danger("[Whispering... somewhere behind you]"));
        }
        Some(Encounter::Gallery) => {
            state.0 = Active::Gallery { room: r.index, t: 0.0, look_time: 0.0, beat: 0.0, warned: false };
            *mood = Mood { tint: Color::srgb(0.6, 0.35, 1.0), amount: 0.55, scale: 0.85, flicker: 0.0 };
            spawn_eyes(&mut commands, &mut meshes, &mut mats, r);
            sfx.write(PlaySfx(Sfx::Heartbeat));
            subs.write(Subtitle::danger("[The walls open their eyes] Don't look at them."));
        }
        Some(Encounter::Hush) => {
            state.0 = Active::Hush { room: r.index, t: 0.0 };
            sfx.write(PlaySfx(Sfx::Rumble));
            subs.write(Subtitle::danger("[The air goes heavy. Your ears pop.]"));
        }
        _ => {}
    }
}

fn fail(run: &mut RunState, damage: f32, death: &mut DeathCause, title: &str, lesson: &str, next: &mut NextState<Overlay>) {
    if run.damage(damage) {
        *death = DeathCause { title: title.into(), cause: format!("Room {:03}.", run.current_room), lesson: lesson.into() };
        next.set(Overlay::Dead);
    }
}

#[allow(clippy::too_many_arguments)]
fn whisperer(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<EntityConfig>,
    floor: Option<Res<Floor>>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut state: ResMut<EncounterState>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut bodies: Query<(Entity, &mut Transform), With<WhispererBody>>,
    (mut meshes, mut mats): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut run: ResMut<RunState>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    (mut death, mut next): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
) {
    let Active::Whisperer { room, t, spawned } = &mut state.0 else { return };
    let (room, dt) = (*room, time.delta_secs());
    *t += dt;
    let tune = cfg.whisperer;
    let Ok(cam) = cam.single() else { return };
    let (cpos, fwd) = (cam.translation(), cam.forward().as_vec3());
    if !*spawned && *t >= tune.warn_for(room) {
        // Appear behind the player, kept inside the room.
        let r = floor.as_ref().and_then(|f| f.0.room(room));
        let mut p = whisperer_spawn(cpos, fwd, r.map(|r| (r.min, r.max)));
        p.y = 1.5;
        spawn_whisperer(&mut commands, &mut meshes, &mut mats, p);
        *spawned = true;
        *t = 0.0;
        sfx.write(PlaySfx(Sfx::Whisper));
        return;
    }
    if !*spawned {
        return;
    }
    let Ok((e, mut tf)) = bodies.single_mut() else { return };
    // Drift slowly toward the player.
    let to = cpos - tf.translation;
    tf.translation += Vec3::new(to.x, 0.0, to.z).normalize_or_zero() * 0.5 * dt;
    let y = tf.translation.y;
    tf.look_at(Vec3::new(cpos.x, y, cpos.z), Vec3::Y);
    let finished = if is_looking_at(cpos, fwd, tf.translation + Vec3::Y * 0.15, LOOK_COS) {
        sfx.write(PlaySfx(Sfx::Whoosh));
        run.lumens += 3;
        subs.write(Subtitle::info("It shrieks and scatters into the dark. (+3 Lumens)"));
        true
    } else if *t >= tune.window {
        shake.add(0.8);
        sfx.write(PlaySfx(Sfx::Stinger));
        subs.write(Subtitle::danger(format!("It reached you. (-{:.0} vitality)", tune.damage)));
        fail(&mut run, tune.damage, &mut death, "THE WHISPERER REACHED YOU", "When you hear whispering in a dark room, turn around and look straight at it before it gets close.", &mut next);
        true
    } else {
        false
    };
    if finished {
        commands.entity(e).try_despawn();
        if let Some(rt) = rt.as_mut() {
            rt.encounters_done.insert(room);
        }
        state.0 = Active::None;
    }
}

#[allow(clippy::too_many_arguments)]
fn gallery(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<EntityConfig>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut state: ResMut<EncounterState>,
    hide: Res<HideState>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut eyes: Query<(Entity, &GlobalTransform, &mut GalleryEye)>,
    mut mood: ResMut<Mood>,
    mut run: ResMut<RunState>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    (mut death, mut next): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
) {
    let Active::Gallery { room, t, look_time, beat, warned } = &mut state.0 else { return };
    let dt = time.delta_secs();
    *t += dt;
    let tune = cfg.gallery;
    // Leaving the room ends it.
    if run.current_room != *room {
        for (e, _, _) in &eyes {
            commands.entity(e).try_despawn();
        }
        *mood = Mood::default();
        if let Some(rt) = rt.as_mut() {
            rt.encounters_done.insert(*room);
        }
        subs.write(Subtitle::info("[Behind you, the eyes close one by one]"));
        state.0 = Active::None;
        return;
    }
    let open = (*t / tune.warn_for(*room)).min(1.0);
    for (_, _, mut eye) in &mut eyes {
        eye.open = open;
    }
    if open < 1.0 || hide.is_hiding() {
        return;
    }
    let Ok(cam) = cam.single() else { return };
    let (cpos, fwd) = (cam.translation(), cam.forward().as_vec3());
    let looking = eyes.iter().any(|(_, g, _)| cpos.distance(g.translation()) < 14.0 && is_looking_at(cpos, fwd, g.translation(), 0.93));
    if looking {
        *look_time += dt;
        *beat -= dt;
        if *beat <= 0.0 {
            *beat = 0.7;
            sfx.write(PlaySfx(Sfx::Heartbeat));
        }
        if !*warned {
            *warned = true;
            subs.write(Subtitle::danger("They're draining you. LOOK AWAY."));
        }
        fail(&mut run, tune.damage * dt, &mut death, "THE GALLERY STARED BACK", "When eyes open in the walls, look at the floor and walk out. Every second you meet their gaze costs vitality.", &mut next);
    }
}

#[allow(clippy::too_many_arguments)]
fn hush(
    time: Res<Time>,
    cfg: Res<EntityConfig>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut state: ResMut<EncounterState>,
    player: Query<&Player>,
    mut run: ResMut<RunState>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    (mut death, mut next): (ResMut<DeathCause>, ResMut<NextState<Overlay>>),
) {
    let Active::Hush { room, t } = &mut state.0 else { return };
    let room = *room;
    *t += time.delta_secs();
    let tune = cfg.hush;
    let warn = tune.warn_for(room);
    if *t < warn {
        return;
    }
    let moving = player.single().is_ok_and(|p| p.speed > 0.35);
    let finished = if moving {
        shake.add(0.9);
        sfx.write(PlaySfx(Sfx::Stinger));
        subs.write(Subtitle::danger(format!("You moved. The pressure crushes in. (-{:.0} vitality)", tune.damage)));
        fail(&mut run, tune.damage, &mut death, "THE HUSH CRUSHED YOU", "When the vignette closes in and STAY STILL appears, take your hands off the keys until it lifts.", &mut next);
        true
    } else if *t >= warn + tune.window {
        sfx.write(PlaySfx(Sfx::Whoosh));
        subs.write(Subtitle::info("[The pressure lifts]"));
        true
    } else {
        false
    };
    if finished {
        if let Some(rt) = rt.as_mut() {
            rt.encounters_done.insert(room);
        }
        state.0 = Active::None;
    }
}

/// Whether the Hush overlay should be showing, and how strongly: (alpha, show text).
pub fn hush_visual(state: &Active, cfg: &EntityConfig) -> (f32, bool) {
    match state {
        Active::Hush { room, t } => {
            let warn = cfg.hush.warn_for(*room);
            ((t / warn).min(1.0), *t >= warn)
        }
        _ => (0.0, false),
    }
}

fn spawn_hush_ui(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), align_items: AlignItems::Center, justify_content: JustifyContent::Center, border: UiRect::all(px(140)), ..default() },
            BorderColor::all(Color::srgba(0.25, 0.05, 0.35, 0.0)),
            GlobalZIndex(15),
            Pickable::IGNORE,
            crate::theme::PassThrough,
            HushOverlay,
            DespawnOnExit(AppState::Playing),
        ))
        .with_child((Text::new(""), theme.font(true, 56.0), TextColor(theme.danger), HushText));
}

fn hush_ui(time: Res<Time>, state: Res<EncounterState>, cfg: Res<EntityConfig>, mut overlay: Query<&mut BorderColor, With<HushOverlay>>, mut text: Query<&mut Text, With<HushText>>) {
    let (a, show) = hush_visual(&state.0, &cfg);
    let pulse = 0.85 + 0.15 * (time.elapsed_secs() * 4.0).sin();
    for mut b in &mut overlay {
        *b = BorderColor::all(Color::srgba(0.22, 0.04, 0.32, 0.85 * a * pulse));
    }
    for mut t in &mut text {
        let want = if show { "STAY STILL" } else { "" };
        if t.0 != want {
            t.0 = want.into();
        }
    }
}

/// Where The Whisperer appears: behind the player, inside the room. With
/// the player's back to a wall, it picks the behind-left/right spot that
/// is farthest from them, so it never pops up in their face or in view.
pub fn whisperer_spawn(cpos: Vec3, fwd: Vec3, bounds: Option<(Vec2, Vec2)>) -> Vec3 {
    let f = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::NEG_Z);
    let side = Vec3::new(-f.z, 0.0, f.x);
    let clamp = |mut p: Vec3| {
        if let Some((min, max)) = bounds {
            p.x = p.x.clamp(min.x + 0.5, max.x - 0.5);
            p.z = p.z.clamp(min.y + 0.5, max.y - 0.5);
        }
        p
    };
    let flat = |p: Vec3| Vec3::new(p.x - cpos.x, 0.0, p.z - cpos.z);
    // Score: far enough away, not in front, ideally straight behind.
    let score = |p: Vec3| {
        let d = flat(p);
        let ahead = d.normalize_or_zero().dot(f);
        d.length().min(2.6) - 4.0 * ahead.max(0.0) - 0.5 * ahead
    };
    [-f * 2.6, (-f + side).normalize() * 2.6, (-f - side).normalize() * 2.6, side * 2.8, -side * 2.8]
        .into_iter()
        .map(|o| clamp(cpos + o))
        .max_by(|a, b| score(*a).total_cmp(&score(*b)))
        .unwrap_or(cpos - f * 2.6)
}

fn spawn_whisperer(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, at: Vec3) {
    let pale = mats.add(StandardMaterial { base_color: Color::srgb(0.78, 0.8, 0.82), emissive: LinearRgba::rgb(0.08, 0.09, 0.11), perceptual_roughness: 0.6, ..default() });
    let socket = mats.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
    let wisp = mats.add(StandardMaterial { base_color: Color::srgba(0.4, 0.42, 0.45, 0.55), alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() });
    let head = meshes.add(Sphere::new(0.2).mesh().uv(28, 18));
    let eye = meshes.add(Sphere::new(0.045).mesh().uv(12, 8));
    let strip = meshes.add(Cuboid::new(0.07, 1.0, 0.01));
    let mut rng = GameRng::new(at.x.to_bits() as u64 ^ at.z.to_bits() as u64);
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            WhispererBody,
            PointLight { color: Color::srgb(0.7, 0.8, 1.0), intensity: 3_500.0, range: 4.0, ..default() },
            DespawnOnExit(AppState::Playing),
            Name::new("The Whisperer"),
        ))
        .with_children(|c| {
            c.spawn((Mesh3d(head), MeshMaterial3d(pale.clone()), Transform::from_scale(Vec3::new(0.85, 1.45, 0.9))));
            for x in [-0.07f32, 0.07] {
                c.spawn((Mesh3d(eye.clone()), MeshMaterial3d(socket.clone()), Transform::from_xyz(x, 0.06, -0.16).with_scale(Vec3::new(1.0, 1.6, 0.6))));
            }
            // A long, slack mouth hanging open.
            c.spawn((Mesh3d(eye.clone()), MeshMaterial3d(socket.clone()), Transform::from_xyz(0.0, -0.16, -0.16).with_scale(Vec3::new(1.4, 1.5, 0.5))));
            // Hollow cheeks.
            for x in [-0.1f32, 0.1] {
                c.spawn((Mesh3d(eye.clone()), MeshMaterial3d(socket.clone()), Transform::from_xyz(x, -0.08, -0.13).with_scale(Vec3::new(0.7, 1.2, 0.3))));
            }
            // Two thin arms reaching for you, with long splayed fingers.
            let bone = meshes.add(Capsule3d::new(0.012, 0.32));
            let finger = meshes.add(Capsule3d::new(0.005, 0.11));
            for side in [-1.0f32, 1.0] {
                c.spawn((Transform::from_xyz(side * 0.13, -0.3, -0.05).with_rotation(Quat::from_rotation_x(-1.1) * Quat::from_rotation_z(side * 0.25)), Visibility::default(), WhisperArm(side))).with_children(|arm| {
                    arm.spawn((Mesh3d(bone.clone()), MeshMaterial3d(pale.clone()), Transform::from_xyz(0.0, -0.18, 0.0)));
                    arm.spawn((Mesh3d(bone.clone()), MeshMaterial3d(pale.clone()), Transform::from_xyz(0.0, -0.5, 0.03).with_rotation(Quat::from_rotation_x(-0.2))));
                    for f in 0..4 {
                        let sp = (f as f32 - 1.5) * 0.25;
                        arm.spawn((Mesh3d(finger.clone()), MeshMaterial3d(pale.clone()), Transform::from_xyz(sp * 0.04, -0.75, 0.06).with_rotation(Quat::from_rotation_z(sp) * Quat::from_rotation_x(-0.3))));
                    }
                });
            }
            for i in 0..20 {
                let a = i as f32 / 20.0 * std::f32::consts::TAU;
                let len = rng.range_f32(0.6, 1.3);
                c.spawn((
                    Mesh3d(strip.clone()),
                    MeshMaterial3d(wisp.clone()),
                    Transform::from_xyz(a.sin() * 0.14, -0.2 - len / 2.0, a.cos() * 0.12).with_rotation(Quat::from_rotation_y(a)).with_scale(Vec3::new(1.0, len, 1.0)),
                    WhisperWisp { angle: a, len },
                ));
            }
        });
}

/// The Whisperer drifts and sways, its head lolls, its arms grope toward you
/// and the rags of its body swirl around it.
#[allow(clippy::type_complexity)]
fn animate_whisperer(
    time: Res<Time>,
    mut q: Query<&mut Transform, (With<WhispererBody>, Without<WhisperArm>, Without<WhisperWisp>)>,
    mut arms: Query<(&WhisperArm, &mut Transform), (Without<WhispererBody>, Without<WhisperWisp>)>,
    mut wisps: Query<(&WhisperWisp, &mut Transform), (Without<WhispererBody>, Without<WhisperArm>)>,
) {
    let t = time.elapsed_secs();
    for mut tf in &mut q {
        tf.translation.y = 1.5 + (t * 1.7).sin() * 0.06;
        let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z((t * 0.9).sin() * 0.12) * Quat::from_rotation_x((t * 0.6).sin() * 0.06);
    }
    for (arm, mut tf) in &mut arms {
        // Slow, uneven grabbing: one arm reaches while the other draws back.
        let reach = (t * 1.3 + arm.0 * 1.4).sin();
        tf.rotation = Quat::from_rotation_x(-1.1 - 0.35 * reach) * Quat::from_rotation_z(arm.0 * (0.25 + 0.12 * (t * 2.1).sin()));
    }
    for (w, mut tf) in &mut wisps {
        let a = w.angle + t * 0.8;
        let flutter = 1.0 + 0.15 * (t * 3.0 + w.angle * 3.0).sin();
        tf.translation = Vec3::new(a.sin() * 0.15, -0.2 - w.len * flutter / 2.0, a.cos() * 0.13);
        tf.rotation = Quat::from_rotation_y(a) * Quat::from_rotation_x((t * 2.0 + w.angle).sin() * 0.25);
        tf.scale = Vec3::new(1.0, w.len * flutter, 1.0);
    }
}

/// The Hush appears once its warning is over: in the far corner of the room,
/// finger raised, its blank head slowly turning to face you. It fades away
/// when the pressure lifts.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hush_figure(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<EncounterState>,
    cfg: Res<EntityConfig>,
    floor: Option<Res<Floor>>,
    player: Query<&Transform, (With<Player>, Without<HushFigure>, Without<HushHead>, Without<HushArm>)>,
    mut figures: Query<(Entity, &mut HushFigure, &mut Transform, &Children), (Without<HushHead>, Without<HushArm>)>,
    mut heads: Query<&mut Transform, (With<HushHead>, Without<HushFigure>, Without<HushArm>)>,
    mut arms: Query<&mut Transform, (With<HushArm>, Without<HushFigure>, Without<HushHead>)>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let active = match state.0 {
        Active::Hush { room, t } if t >= cfg.hush.warn_for(room) => Some(room),
        _ => None,
    };
    let ppos = player.single().map(|p| p.translation).unwrap_or_default();
    if let (Some(room), true) = (active, figures.is_empty())
        && let Some(r) = floor.as_ref().and_then(|f| f.0.room(room))
    {
        // The corner farthest from you.
        let corners = [Vec2::new(r.min.x + 0.7, r.min.y + 0.7), Vec2::new(r.max.x - 0.7, r.min.y + 0.7), Vec2::new(r.min.x + 0.7, r.max.y - 0.7), Vec2::new(r.max.x - 0.7, r.max.y - 0.7)];
        let at = corners.into_iter().max_by(|a, b| a.distance(ppos.xz()).total_cmp(&b.distance(ppos.xz()))).unwrap_or(r.center());
        spawn_hush(&mut commands, &mut meshes, &mut mats, room, Vec3::new(at.x, 0.0, at.y));
    }
    let t_now = time.elapsed_secs();
    for (e, mut f, mut tf, kids) in &mut figures {
        f.t += dt;
        let want = if active == Some(f.room) || f.room == 0 { 1.0 } else { 0.0 };
        f.shown += (want - f.shown) * (dt * 2.5).min(1.0);
        if want == 0.0 && f.shown < 0.03 {
            commands.entity(e).try_despawn();
            continue;
        }
        // It rises out of the floor as it appears and sinks as it fades.
        tf.scale = Vec3::new(1.0, f.shown.max(0.01), 1.0);
        // The whole body turns toward you, slowly.
        let to = ppos - tf.translation;
        let want_yaw = (-to.x).atan2(-to.z);
        let cur = tf.rotation.to_euler(EulerRot::YXZ).0;
        let d = (want_yaw - cur + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        tf.rotation = Quat::from_rotation_y(cur + d * (dt * 0.6).min(1.0));
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(c) = stack.pop() {
            if let Ok(ch) = children.get(c) {
                stack.extend(ch.iter());
            }
            if let Ok(mut h) = heads.get_mut(c) {
                // Head cocks to one side and twitches.
                h.rotation = Quat::from_rotation_z(0.35 + (t_now * 7.0).sin() * 0.03) * Quat::from_rotation_x(0.15);
            }
            if let Ok(mut a) = arms.get_mut(c) {
                // Finger raised to its blank face: "shh".
                let lift = (f.t / 0.8).min(1.0);
                let raised = Quat::from_rotation_arc(Vec3::NEG_Y, (HUSH_LIPS - HUSH_SHOULDER).normalize());
                a.rotation = Quat::IDENTITY.slerp(raised, lift);
            }
        }
    }
}

fn monster_view(
    mut commands: Commands,
    run: Res<RunState>,
    player: Query<(&Player, &Transform)>,
    mut done: Local<bool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut bodies: Query<&mut Transform, (Or<(With<WhispererBody>, With<crate::surge::SurgeBody>)>, Without<Player>)>,
) {
    if run.elapsed < 4.0 {
        return;
    }
    let Ok((p, tf)) = player.single() else { return };
    let fwd = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
    let at = tf.translation + fwd * 3.0;
    if !*done {
        *done = true;
        match std::env::var("HALLWAYS_MONSTER_VIEW").unwrap_or_default().as_str() {
            "whisperer" => spawn_whisperer(&mut commands, &mut meshes, &mut mats, at + Vec3::Y * 1.5),
            "hush" => spawn_hush(&mut commands, &mut meshes, &mut mats, 0, at + fwd * 1.5),
            "surge" => crate::surge::spawn_surge(&mut commands, &mut meshes, &mut mats, at + Vec3::Y * 1.4, false),
            "undertow" => crate::surge::spawn_surge(&mut commands, &mut meshes, &mut mats, at + Vec3::Y * 1.4, true),
            _ => {}
        }
        return;
    }
    // Face the camera.
    for mut b in &mut bodies {
        let tilt = b.rotation.to_euler(EulerRot::YXZ);
        b.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw + std::f32::consts::PI, tilt.1, tilt.2);
    }
}

/// The Hush's raised shoulder and where its finger rests (its blank "lips").
const HUSH_SHOULDER: Vec3 = Vec3::new(0.26, 2.12, 0.0);
const HUSH_LIPS: Vec3 = Vec3::new(0.02, 2.47, -0.2);

fn spawn_hush(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, room: u32, at: Vec3) {
    let skin = mats.add(StandardMaterial { base_color: Color::srgb(0.82, 0.8, 0.78), perceptual_roughness: 0.9, ..default() });
    let robe = mats.add(StandardMaterial { base_color: Color::srgb(0.05, 0.05, 0.07), perceptual_roughness: 1.0, ..default() });
    let cap = meshes.add(Capsule3d::new(0.5, 1.0));
    let ball = meshes.add(Sphere::new(0.5).mesh().uv(20, 12));
    commands
        .spawn((Transform::from_translation(at), Visibility::default(), HushFigure { room, t: 0.0, shown: 0.0 }, DespawnOnExit(AppState::Playing), Name::new("The Hush")))
        .with_children(|c| {
            // A tall, gaunt body in a long dark shroud.
            // Shroud from the floor to the shoulders (capsule half-height = scale.y).
            // A shroud that flares out at the hem, under a narrow torso.
            let skirt = meshes.add(Cone::new(0.42, 1.5));
            c.spawn((Mesh3d(skirt), MeshMaterial3d(robe.clone()), Transform::from_xyz(0.0, 0.75, 0.0).with_scale(Vec3::new(1.0, 1.0, 0.75))));
            c.spawn((Mesh3d(cap.clone()), MeshMaterial3d(robe.clone()), Transform::from_xyz(0.0, 1.72, 0.0).with_scale(Vec3::new(0.46, 0.5, 0.3))));
            c.spawn((Mesh3d(cap.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 2.24, 0.0).with_scale(Vec3::new(0.12, 0.1, 0.12))));
            // A smooth, featureless head.
            c.spawn((Transform::from_xyz(0.0, 2.35, 0.0), Visibility::default(), HushHead)).with_child((Mesh3d(ball.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.3, 0.42, 0.32))));
            // One long arm, hanging; the other raises a finger to the face.
            c.spawn((Mesh3d(cap.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(-0.27, 1.62, 0.0).with_rotation(Quat::from_rotation_z(-0.08)).with_scale(Vec3::new(0.075, 0.5, 0.075))));
            c.spawn((Transform::from_xyz(HUSH_SHOULDER.x, HUSH_SHOULDER.y, HUSH_SHOULDER.z), Visibility::default(), HushArm)).with_children(|a| {
                a.spawn((Mesh3d(cap.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, -0.22, 0.0).with_scale(Vec3::new(0.075, 0.22, 0.075))));
                a.spawn((Mesh3d(ball.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, -0.47, 0.0).with_scale(Vec3::new(0.08, 0.1, 0.05))));
                a.spawn((Mesh3d(cap.clone()), MeshMaterial3d(skin.clone()), Transform::from_xyz(0.0, -0.56, 0.0).with_scale(Vec3::new(0.022, 0.06, 0.022))));
            });
        });
}

fn spawn_eyes(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, r: &RoomPlan) {
    let sclera = mats.add(StandardMaterial { base_color: Color::srgb(0.92, 0.88, 0.82), perceptual_roughness: 0.15, clearcoat: 1.0, ..default() });
    let iris = mats.add(StandardMaterial { base_color: Color::srgb(0.45, 0.08, 0.5), emissive: LinearRgba::rgb(0.9, 0.1, 1.2), perceptual_roughness: 0.2, ..default() });
    let pupil = mats.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
    let lid = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.05, 0.08), perceptual_roughness: 0.8, ..default() });
    let ball = meshes.add(Sphere::new(0.5).mesh().uv(24, 16));
    let ring = meshes.add(Torus::new(0.48, 0.58));
    let mut rng = GameRng::fork(r.seed, 777);
    let h = r.height;
    let clusters = 5 + rng.below(3);
    for _ in 0..clusters {
        // Pick a wall (west, east or north) and a spot on it.
        let wall = rng.below(3);
        let (base, yaw) = match wall {
            0 => (Vec3::new(r.min.x + 0.02, 0.0, rng.range_f32(r.min.y + 0.8, r.max.y - 0.8)), std::f32::consts::FRAC_PI_2),
            1 => (Vec3::new(r.max.x - 0.02, 0.0, rng.range_f32(r.min.y + 0.8, r.max.y - 0.8)), -std::f32::consts::FRAC_PI_2),
            _ => (Vec3::new(rng.range_f32(r.min.x + 0.8, r.max.x - 0.8), 0.0, r.min.y + 0.02), 0.0),
        };
        let n = 2 + rng.below(3);
        for _ in 0..n {
            let size = rng.range_f32(0.14, 0.32);
            let local = Vec3::new(rng.range_f32(-0.5, 0.5), rng.range_f32(1.0, (h - 0.4).min(2.8)), 0.0);
            let pos = base + Quat::from_rotation_y(yaw) * local;
            commands
                .spawn((
                    Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
                    Visibility::default(),
                    GalleryEye { open: 0.0, seed: rng.range_f32(0.0, 50.0) },
                    DespawnOnExit(AppState::Playing),
                ))
                .with_children(|c| {
                    c.spawn((Mesh3d(ring.clone()), MeshMaterial3d(lid.clone()), Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(size * 1.1, size * 0.6, size * 0.7))));
                    c.spawn((Mesh3d(ball.clone()), MeshMaterial3d(sclera.clone()), Transform::from_scale(Vec3::new(size, size * 0.62, size * 0.35))));
                    c.spawn((Mesh3d(ball.clone()), MeshMaterial3d(iris.clone()), Transform::from_xyz(0.0, 0.0, size * 0.16).with_scale(Vec3::new(size * 0.42, size * 0.42, size * 0.06)), GalleryIris))
                        .with_child((Mesh3d(ball.clone()), MeshMaterial3d(pupil.clone()), Transform::from_xyz(0.0, 0.0, 0.3).with_scale(Vec3::new(0.45, 0.45, 0.5))));
                });
        }
    }
}

fn animate_eyes(time: Res<Time>, player: Query<&Transform, (With<Player>, Without<GalleryEye>, Without<GalleryIris>)>, mut eyes: Query<(&GalleryEye, &mut Transform, &GlobalTransform, &Children)>, mut irises: Query<&mut Transform, (With<GalleryIris>, Without<GalleryEye>)>) {
    let t = time.elapsed_secs();
    let target = player.single().map(|p| p.translation + Vec3::Y * 1.6).unwrap_or_default();
    for (eye, mut tf, g, children) in &mut eyes {
        // Occasional blink; open amount scales the eye vertically.
        let blink = if (t * 0.6 + eye.seed).sin() > 0.985 { 0.1 } else { 1.0 };
        tf.scale = Vec3::new(1.0, (eye.open * blink).max(0.02), 1.0);
        // Irises track the player.
        let local = g.affine().inverse().transform_point3(target);
        let dir = local.normalize_or_zero();
        for c in children.iter() {
            if let Ok(mut it) = irises.get_mut(c) {
                it.translation.x = dir.x * 0.06;
                it.translation.y = dir.y * 0.04;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisperer_never_spawns_in_your_face() {
        // Back to the south wall of a 6 x 10 room, looking north.
        let bounds = Some((Vec2::new(-3.0, -10.0), Vec2::new(3.0, 0.0)));
        let cam = Vec3::new(0.0, 1.6, -0.6);
        let p = whisperer_spawn(cam, Vec3::NEG_Z, bounds);
        let d = Vec3::new(p.x - cam.x, 0.0, p.z - cam.z);
        assert!(d.length() > 1.8, "too close: {d:?}");
        assert!(!is_looking_at(cam, Vec3::NEG_Z, Vec3::new(p.x, 1.65, p.z), LOOK_COS), "spawned in view");
        // In the open it is straight behind.
        let p = whisperer_spawn(Vec3::new(0.0, 1.6, -5.0), Vec3::NEG_Z, bounds);
        assert!((p.z - (-5.0 + 2.6)).abs() < 1e-3 && p.x.abs() < 1e-3);
    }

    #[test]
    fn looking_requires_facing_target() {
        let cam = Vec3::ZERO;
        assert!(is_looking_at(cam, Vec3::NEG_Z, Vec3::new(0.1, 0.0, -5.0), LOOK_COS));
        assert!(!is_looking_at(cam, Vec3::NEG_Z, Vec3::new(3.0, 0.0, -2.0), LOOK_COS));
        assert!(!is_looking_at(cam, Vec3::NEG_Z, Vec3::new(0.0, 0.0, 5.0), LOOK_COS));
    }

    #[test]
    fn hush_overlay_ramps_then_shows_text() {
        let cfg = EntityConfig::default();
        let warn = cfg.hush.warn_for(20);
        let (a, show) = hush_visual(&Active::Hush { room: 20, t: warn * 0.5 }, &cfg);
        assert!((a - 0.5).abs() < 1e-4 && !show);
        let (a, show) = hush_visual(&Active::Hush { room: 20, t: warn + 0.1 }, &cfg);
        assert!(a == 1.0 && show);
        assert_eq!(hush_visual(&Active::None, &cfg), (0.0, false));
    }

    #[test]
    fn whisperer_window_is_fair() {
        let cfg = EntityConfig::default();
        assert!(cfg.whisperer.window >= 2.0, "players need time to turn around");
        assert!(cfg.hush.window >= 2.0);
    }
}
