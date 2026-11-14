//! The Door 40 chase: a second, longer encounter with The Hollow.
//!
//! Cue → counter: **you step through Door 041, the lights bleed red and the
//! floor rumbles** → The Hollow rises from the ink behind you and follows
//! you through every room up to Door 047. **Counter: run.** Sprinting is
//! faster than it is; the doors on the way swing open as you reach them,
//! and hiding does not help here. Reaching Room 047 makes it sink away.
//!
//! The rooms of the chase (41–46) are generated without locks, fake doors
//! or other entities (see `floor::plan`).

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{FloorPlan, PURSUIT_FIRST, PURSUIT_LAST, WALL_T};
use crate::floor::{Floor, FloorRuntime};
use crate::hollow::{DeathCause, Hollow, SuppressHollow};
use crate::interaction::{Door, Interactable};
use crate::physics::Collider;
use crate::player::Player;
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{gameplay_active, AppState, Overlay};
use crate::world::Mood;

/// Faster than walking, slower than sprinting.
pub const PURSUIT_SPEED: f32 = 4.1;
/// When it falls this far behind it hurries to keep the pressure on.
const CATCH_UP_GAP: f32 = 13.0;
const CATCH_UP_SPEED: f32 = 5.0;
pub const PURSUIT_CATCH: f32 = 1.15;
const CUE_SECS: f32 = 2.6;
const SINK_SECS: f32 = 2.2;
/// Doors on the chase route open by themselves within this distance.
const AUTO_DOOR: f32 = 2.6;

#[derive(Component)]
pub struct PursuitHollow;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PursuitPhase {
    #[default]
    Idle,
    Cue(f32),
    Chasing { seg: usize },
    Sinking(f32),
    Done,
}

#[derive(Resource, Debug, Default)]
pub struct Pursuit {
    pub phase: PursuitPhase,
    path: Vec<Vec3>,
    /// Door 047 has slammed shut behind the survivors.
    pub door_shut: bool,
}

impl Pursuit {
    /// The chase music plays while this is true.
    pub fn active(&self) -> bool {
        matches!(self.phase, PursuitPhase::Cue(_) | PursuitPhase::Chasing { .. })
    }
}

/// Ground-level route: rising right behind Door 041 (in Room 040), then
/// through every chase room's openings between the barricades, to just
/// inside Room 047.
pub fn pursuit_path(plan: &FloorPlan) -> Vec<Vec3> {
    let mut pts = vec![];
    if let Some(r) = plan.room(PURSUIT_FIRST - 1) {
        pts.push(Vec3::new(r.exit_x, 0.0, r.min.y + 1.2));
        pts.push(Vec3::new(r.exit_x, 0.0, r.min.y + 0.4));
    }
    for i in PURSUIT_FIRST..=PURSUIT_LAST + 1 {
        let Some(r) = plan.room(i) else { continue };
        // Through the doorway.
        pts.push(Vec3::new(r.entry_x, r.entry_height, r.max.y + WALL_T + 0.4));
        let route = r.route();
        if i == PURSUIT_LAST + 1 {
            pts.push(Vec3::new(route[0].x, r.entry_height, route[0].y));
            break;
        }
        for (k, p) in route.iter().enumerate() {
            let y = if k == 0 { r.entry_height } else { 0.0 };
            pts.push(Vec3::new(p.x, y, p.y));
        }
    }
    pts
}

/// How far along the route `p` is: the last route point not ahead of it
/// (the route only ever heads toward -Z). From there a straight line to the
/// player never crosses a barricade.
fn progress_index(path: &[Vec3], p: Vec3) -> usize {
    path.iter().rposition(|v| v.z >= p.z - 0.05).unwrap_or(0)
}

/// Arc length along `path` of the point nearest `p` on the stretch the player
/// has reached (see [`progress_index`]).
fn progress_along(path: &[Vec3], p: Vec3) -> f32 {
    let i = progress_index(path, p);
    let mut s = 0.0;
    for w in path.windows(2).take(i) {
        s += w[0].distance(w[1]);
    }
    if let (Some(a), Some(b)) = (path.get(i), path.get(i + 1)) {
        let seg = *b - *a;
        let len = seg.length();
        if len > 1e-4 {
            s += ((p - *a).dot(seg / len)).clamp(0.0, len);
        }
    }
    s
}

/// Point `dist` metres along `path`.
fn point_along(path: &[Vec3], dist: f32) -> Vec3 {
    let mut left = dist.max(0.0);
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        if left <= len && len > 1e-4 {
            return w[0].lerp(w[1], left / len);
        }
        left -= len;
    }
    path.last().copied().unwrap_or(Vec3::ZERO)
}

/// Red, dim, flickering: the lights during the chase.
pub const CHASE_MOOD: Mood = Mood { tint: Color::srgb(1.0, 0.12, 0.08), amount: 0.85, scale: 0.8, flicker: 0.35 };

/// How far ahead of the player the chase's guiding light flies.
pub const GUIDE_LEAD: f32 = 4.5;

/// The chase's guiding light: always a few metres ahead on the way out,
/// through each barricade gap and the real door (never a fake one).
#[derive(Component)]
pub struct ChaseGuide;

pub struct PursuitPlugin;

impl Plugin for PursuitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pursuit>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(Pursuit::default()))
            .add_systems(Update, (trigger, run_pursuit, auto_doors, shut_door_behind, guide_light, ink_warnings).chain().run_if(gameplay_active));
        // QA: HALLWAYS_WARP=40 starts the run at Room 040 (the opening chase counts as survived).
        if std::env::var_os("HALLWAYS_WARP").is_some() {
            app.add_systems(Update, dev_warp.run_if(gameplay_active));
        }
        // QA: HALLWAYS_BOT=1 sprints along each room's route (through every
        // opening), colliding with walls like a player would.
        if std::env::var_os("HALLWAYS_BOT").is_some() {
            app.add_systems(Update, dev_bot.after(crate::player::CameraSet).run_if(gameplay_active))
                .add_systems(Update, dev_bot_revive.run_if(in_state(AppState::Playing)));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn trigger(
    mut commands: Commands,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    suppress: Res<SuppressHollow>,
    mut pursuit: ResMut<Pursuit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut mood: ResMut<Mood>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut surge: (ResMut<crate::surge::SurgeState>, Query<Entity, With<crate::surge::SurgeBody>>),
) {
    if pursuit.phase != PursuitPhase::Idle || suppress.0 || std::env::var_os("HALLWAYS_CAPTURE").is_some() {
        return;
    }
    let Some(floor) = floor else { return };
    let Some(room) = floor.0.room(run.current_room) else { return };
    if !room.pursuit || run.current_room != PURSUIT_FIRST {
        return;
    }
    let path = pursuit_path(&floor.0);
    let Some(start) = path.first().copied() else { return };
    let e = crate::hollow::spawn_hollow(&mut commands, &mut meshes, &mut mats, start);
    commands.entity(e).insert(PursuitHollow);
    pursuit.path = path;
    pursuit.phase = PursuitPhase::Cue(0.0);
    // Nothing else hunts during the chase: a Surge or Undertow still making
    // passes from an earlier room is called off (it would also reset the lights).
    surge.0.0 = crate::surge::SurgePhase::Idle;
    for e in surge.1.iter() {
        commands.entity(e).try_despawn();
    }
    *mood = CHASE_MOOD;
    shake.add(0.5);
    sfx.write(PlaySfx(Sfx::Rumble));
    sfx.write(PlaySfx(Sfx::Ooze));
    subs.write(Subtitle::danger("[The lights bleed red. Something is rising behind you. RUN!]"));
}

#[allow(clippy::too_many_arguments)]
fn run_pursuit(
    mut commands: Commands,
    time: Res<Time>,
    mut pursuit: ResMut<Pursuit>,
    mut run: ResMut<RunState>,
    player: Query<&Transform, (With<Player>, Without<PursuitHollow>)>,
    mut hollow: Query<(Entity, &mut Hollow, &mut Transform), With<PursuitHollow>>,
    mut mood: ResMut<Mood>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut death: ResMut<DeathCause>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    let dt = time.delta_secs();
    if std::env::var_os("HALLWAYS_WARP").is_some() && time.elapsed_secs() % 1.0 < dt {
        info!("QA pursuit {:?} room {} pos {:?}", pursuit.phase, run.current_room, player.single().map(|p| p.translation).ok());
    }
    let Ok(p) = player.single() else { return };
    let Some((e, mut h, mut tf)) = hollow.iter_mut().next() else {
        // Lost its body (e.g. a revive reset): stand down cleanly.
        if pursuit.active() {
            pursuit.phase = PursuitPhase::Done;
            *mood = Mood::default();
        }
        return;
    };
    if pursuit.active() {
        // Hold the chase lighting even if something else touched it.
        *mood = CHASE_MOOD;
    }
    match pursuit.phase.clone() {
        PursuitPhase::Cue(t) => {
            let t = t + dt;
            h.emerge_rate = 1.0 / CUE_SECS;
            h.eyes_target = 1.0;
            let d = p.translation - tf.translation;
            h.facing = d.x.atan2(d.z);
            pursuit.phase = if t >= CUE_SECS { PursuitPhase::Chasing { seg: 0 } } else { PursuitPhase::Cue(t) };
        }
        PursuitPhase::Chasing { mut seg } => {
            if run.current_room > PURSUIT_LAST {
                pursuit.phase = PursuitPhase::Sinking(0.0);
                sfx.write(PlaySfx(Sfx::Ooze));
                subs.write(Subtitle::info("[Behind you, the shrieking stops. It's sinking back into the ink.]"));
                return;
            }
            let path = &pursuit.path;
            let player_idx = progress_index(path, p.translation);
            let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
            let gap = flat(p.translation).distance(flat(tf.translation));
            let speed = if gap > CATCH_UP_GAP { CATCH_UP_SPEED } else { PURSUIT_SPEED };
            let mut dist = speed * dt;
            // Follow the route until it reaches the player's stretch, then go straight for them.
            while dist > 0.0 {
                let target = if seg + 1 < path.len() && seg < player_idx { path[seg + 1] } else { p.translation };
                let to = flat(target) - flat(tf.translation);
                let len = to.length();
                if len < 1e-4 {
                    if seg + 1 < path.len() && seg < player_idx {
                        seg += 1;
                        continue;
                    }
                    break;
                }
                h.facing = to.x.atan2(to.z);
                if len > dist {
                    tf.translation += to / len * dist;
                    break;
                }
                tf.translation += to;
                dist -= len;
                if seg + 1 < path.len() && seg < player_idx {
                    seg += 1;
                } else {
                    break;
                }
            }
            pursuit.phase = PursuitPhase::Chasing { seg };
            if gap < 6.0 {
                shake.add(0.4 * dt);
            }
            if gap < PURSUIT_CATCH {
                pursuit.phase = PursuitPhase::Done;
                commands.entity(e).try_despawn();
                *mood = Mood::default();
                shake.add(1.0);
                sfx.write(PlaySfx(Sfx::Stinger));
                *death = DeathCause {
                    title: "THE HOLLOW CAUGHT UP".into(),
                    cause: format!("Run down in Room {:03} during the chase after Door 040.", run.current_room),
                    lesson: "When the lights bleed red after Door 040, hiding won't save you. Sprint and keep moving: the doors open as you reach them.".into(),
                };
                next_overlay.set(Overlay::Dead);
            }
        }
        PursuitPhase::Sinking(t) => {
            let t = t + dt;
            h.emerge_rate = 0.0;
            h.emerge = (1.0 - t / SINK_SECS).max(0.0);
            h.eyes_target = 0.0;
            pursuit.phase = PursuitPhase::Sinking(t);
            if t >= SINK_SECS {
                commands.entity(e).try_despawn();
                pursuit.phase = PursuitPhase::Done;
                *mood = Mood::default();
                run.lumens += 15;
                sfx.write(PlaySfx(Sfx::Pickup));
                subs.write(Subtitle { text: "You outran The Hollow (+15 Lumens).".into(), seconds: 5.0, danger: false });
            }
        }
        _ => {}
    }
}

fn dev_warp(
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    mut chase: ResMut<crate::hollow::ChaseState>,
    mut player: Query<&mut Transform, With<Player>>,
    mut done: Local<bool>,
) {
    if run.elapsed < 0.2 {
        *done = false;
    }
    let (Some(floor), Ok(mut tf)) = (floor, player.single_mut()) else { return };
    // A room number, or an entity name ("Listener") for the first room it haunts.
    let want = std::env::var("HALLWAYS_WARP").unwrap_or_default();
    let Some(room) = want.parse::<u32>().ok().or_else(|| floor.0.rooms.iter().find(|r| r.encounter.is_some_and(|e| format!("{e:?}").eq_ignore_ascii_case(&want)) || r.feature.is_some_and(|f| format!("{f:?}").to_lowercase().starts_with(&want.to_lowercase()))).map(|r| r.index)) else { return };
    if *done || run.elapsed < 1.0 {
        return;
    }
    let Some(r) = floor.0.room(room) else { return };
    *done = true;
    info!("QA warp to room {room} ({:?}, {:?}, {:?})", r.area, r.encounter, r.light);
    chase.phase = crate::hollow::ChasePhase::Done;
    chase.active = false;
    tf.translation = Vec3::new(r.entry_x, r.entry_height + 0.05, r.max.y - 0.8);
}

/// QA autopilot state: which room it is in and since when.
#[derive(Default)]
struct BotLog {
    room: u32,
    since: f32,
    finished: bool,
    /// Preferred sidestep direction (+1 / -1).
    side: f32,
}

/// QA autopilot (HALLWAYS_BOT=1): sprints through every room along its
/// route, opens (and unlocks) the doors on the way, can't die, and logs each
/// room reached plus anywhere it gets stuck, so a whole floor can be checked
/// for blocked paths and crashes without a person at the keyboard.
#[allow(clippy::too_many_arguments)]
fn dev_bot(
    mut commands: Commands,
    time: Res<Time>,
    mut run: ResMut<RunState>,
    floor: Option<Res<Floor>>,
    mut rt: Option<ResMut<FloorRuntime>>,
    colliders: Query<&Collider>,
    grounds: Query<&crate::physics::Ground>,
    mut doors: Query<(Entity, &mut Door, &GlobalTransform)>,
    mut player: Query<(&mut crate::player::Player, &mut Transform)>,
    mut log: Local<BotLog>,
) {
    let (Some(floor), Ok((mut pl, mut tf))) = (floor, player.single_mut()) else { return };
    run.vitality = 100.0;
    if run.elapsed < 1.5 {
        *log = BotLog::default();
        return;
    }
    let now = run.elapsed;
    let p = tf.translation;
    let room = run.current_room;
    if room != log.room {
        if log.room == 0 && room != 0 {
            info!("QA seed {:016X}", floor.0.seed);
        }
        info!("QA bot reached room {room:03} at {now:.1}s");
        log.room = room;
        log.since = now;
    }
    if now - log.since > 30.0 {
        // Stuck: report it and skip ahead so the rest of the floor still gets tested.
        warn!("QA STUCK in room {room:03} at {p:?}");
        log.since = now;
        if let Some(next) = floor.0.room(room + 1) {
            tf.translation = Vec3::new(next.entry_x, next.entry_height, next.max.y - 0.8);
        }
        return;
    }
    // Where to walk: the handcrafted lift room and corridor go straight to Door 002.
    let mut route: Vec<Vec2> = match floor.0.room(room) {
        Some(r) => {
            let mut v = r.route();
            v.push(Vec2::new(r.exit_x, r.min.y - WALL_T - 0.8));
            v
        }
        None => vec![Vec2::new(0.0, p.z.min(-3.0)), Vec2::new(0.0, crate::floor::plan::START_Z - 1.0)],
    };
    route.retain(|v| v.y < p.z - 0.25);
    // Open the next real door when close (unlocking it if needed).
    for (e, mut d, gt) in &mut doors {
        // The door ahead, or the one being stepped through (the room count
        // switches mid-doorway).
        if d.open || d.mimic || (d.number != room + 1 && d.number != room) {
            continue;
        }
        let g = gt.translation();
        if Vec2::new(g.x - p.x, g.z - p.z).length() > 2.4 {
            continue;
        }
        d.open = true;
        d.locked = false;
        d.key_room = None;
        if d.blocker != Entity::PLACEHOLDER {
            commands.entity(d.blocker).try_remove::<Collider>();
        }
        commands.entity(e).try_remove::<Interactable>();
        if let Some(rt) = rt.as_mut() {
            rt.opened.insert(d.number);
            rt.keys.insert(room);
        }
    }
    if room == floor.0.last_room && rt.as_ref().is_some_and(|rt| rt.opened.contains(&floor.0.final_door())) {
        if !log.finished {
            log.finished = true;
            info!("QA bot finished the floor at {now:.1}s");
        }
        return;
    }
    let Some(t) = route.first().copied() else { return };
    let to = Vec3::new(t.x - p.x, 0.0, t.y - p.z);
    if to.length() < 0.05 {
        return;
    }
    let len = crate::player::SPRINT_SPEED * time.delta_secs();
    let boxes: Vec<_> = colliders.iter().map(|c| c.0).collect();
    let floors: Vec<_> = grounds.iter().map(|g| g.0).collect();
    // Small steps that also follow stairs up and down (a long frame would
    // otherwise run into the next stair face before the height catches up).
    let walk = |dir: Vec3| {
        let step = dir * len;
        let n = ((step.length() / 0.1).ceil() as usize).max(1);
        let mut pos = p;
        for _ in 0..n {
            pos = crate::physics::resolve_cylinder(pos + step / n as f32, 0.3, pos.y + crate::player::STEP_UP, pos.y + 1.7, &boxes);
            pos.y = crate::physics::ground_height(pos, pos.y, crate::player::STEP_UP, &floors);
        }
        pos
    };
    // Straight at the next point; if furniture is in the way, sidestep like a
    // player would (keep the side that worked last time).
    let dir = to.normalize();
    let mut best = walk(dir);
    let progress = |q: Vec3| (q - p).dot(dir);
    if progress(best) < len * 0.3 {
        for a in [0.8f32, -0.8, 1.4, -1.4] {
            let a = a * if log.side == 0.0 { 1.0 } else { log.side };
            let cand = walk(Quat::from_rotation_y(a) * dir);
            if (cand - p).length() > len * 0.5 && progress(cand) > -0.05 {
                best = cand;
                log.side = a.signum();
                break;
            }
        }
    }
    tf.translation = best;
    pl.yaw = (-to.x).atan2(-to.z);
}

/// The bot can't die: a death screen is closed straight away.
fn dev_bot_revive(overlay: Res<State<Overlay>>, mut next: ResMut<NextState<Overlay>>, mut run: ResMut<RunState>) {
    if *overlay.get() == Overlay::Dead {
        run.vitality = 100.0;
        next.set(Overlay::None);
    }
}

#[allow(clippy::too_many_arguments)]
fn guide_light(
    mut commands: Commands,
    time: Res<Time>,
    pursuit: Res<Pursuit>,
    player: Query<&Transform, (With<Player>, Without<ChaseGuide>)>,
    mut guides: Query<(Entity, &mut Transform), With<ChaseGuide>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let chasing = matches!(pursuit.phase, PursuitPhase::Chasing { .. }) || (matches!(pursuit.phase, PursuitPhase::Cue(t) if t > 1.2));
    let Ok(p) = player.single() else { return };
    if !chasing || pursuit.path.len() < 2 {
        for (e, _) in &guides {
            commands.entity(e).try_despawn();
        }
        return;
    }
    let target = point_along(&pursuit.path, progress_along(&pursuit.path, p.translation) + GUIDE_LEAD) + Vec3::Y * 1.9;
    let t = time.elapsed_secs();
    if let Some((_, mut tf)) = guides.iter_mut().next() {
        let to = target - tf.translation;
        let k = (time.delta_secs() * 5.0).min(1.0);
        tf.translation += to * k;
        tf.translation.y = target.y + (t * 3.0).sin() * 0.08;
        return;
    }
    let glow = mats.add(StandardMaterial { base_color: Color::srgb(0.85, 0.95, 1.0), emissive: LinearRgba::rgb(10.0, 14.0, 18.0), unlit: true, ..default() });
    let body = meshes.add(Sphere::new(0.09).mesh().ico(2).unwrap_or_else(|_| Sphere::new(0.09).mesh().build()));
    commands
        .spawn((Name::new("Chase guiding light"), ChaseGuide, Transform::from_translation(p.translation + Vec3::Y * 1.9), Visibility::default(), DespawnOnExit(AppState::Playing)))
        .with_children(|c| {
            c.spawn((Mesh3d(body), MeshMaterial3d(glow)));
            // Cold light that reads against the red chase lighting.
            c.spawn(PointLight { color: Color::srgb(0.75, 0.9, 1.0), intensity: 60_000.0, range: 8.0, shadow_maps_enabled: false, ..default() });
        });
}

/// Rooms before Door 040 where ink starts seeping down the walls.
pub const WARNING_ROOMS: std::ops::RangeInclusive<u32> = (PURSUIT_FIRST - 3)..=(PURSUIT_FIRST - 1);

#[derive(Component)]
struct InkDrip {
    room: u32,
    /// 0..1 how far it has run down.
    t: f32,
    speed: f32,
    top: f32,
}

/// The build-up to the chase: in the three rooms before Door 040, black ink
/// runs down the walls, more of it in each room. Harmless; just a warning.
#[allow(clippy::too_many_arguments)]
fn ink_warnings(
    mut commands: Commands,
    time: Res<Time>,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    mut drips: Query<(Entity, &mut InkDrip, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut subs: MessageWriter<Subtitle>,
    mut done: Local<Vec<u32>>,
) {
    if run.elapsed < 0.2 {
        done.clear();
    }
    let dt = time.delta_secs();
    for (e, mut d, mut tf) in &mut drips {
        d.t = (d.t + d.speed * dt).min(1.0);
        // Grows downward from the top of the wall.
        let len = 0.2 + d.t * 1.8;
        tf.scale.y = len;
        tf.translation.y = d.top - len / 2.0;
        if run.current_room > d.room + 1 || run.current_room + 1 < d.room {
            commands.entity(e).try_despawn();
        }
    }
    let Some(floor) = floor else { return };
    let room = run.current_room;
    if !WARNING_ROOMS.contains(&room) || done.contains(&room) || !floor.0.room(PURSUIT_FIRST).is_some_and(|r| r.pursuit) {
        return;
    }
    let Some(r) = floor.0.room(room) else { return };
    done.push(room);
    let step = room + 1 - *WARNING_ROOMS.start(); // 1, 2, 3
    let ink = mats.add(StandardMaterial { base_color: Color::srgb(0.005, 0.004, 0.006), perceptual_roughness: 0.15, reflectance: 0.3, ..default() });
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut rng = crate::rng::GameRng::new(r.seed ^ 0x1A4);
    let top = r.height - 0.05;
    for _ in 0..(4 * step) {
        let east = rng.next_f32() < 0.5;
        let x = if east { r.max.x - 0.015 } else { r.min.x + 0.015 };
        let z = rng.range_f32(r.min.y + 0.5, r.max.y - 0.5);
        let w = rng.range_f32(0.04, 0.12);
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(ink.clone()),
            Transform::from_xyz(x, top, z).with_scale(Vec3::new(0.02, 0.2, w)),
            InkDrip { room, t: 0.0, speed: rng.range_f32(0.08, 0.25), top },
            DespawnOnExit(AppState::Playing),
            Name::new("Ink drip"),
        ));
    }
    let line = match step {
        1 => "[Something dark is seeping down the walls.]",
        2 => "[More ink. It's running faster now.]",
        _ => "[The walls are weeping ink. Something is close.]",
    };
    subs.write(Subtitle::danger(line));
}

/// Once every living player is through Door 047, it slams shut behind
/// them and stays locked: the chase is over for good.
#[allow(clippy::too_many_arguments)]
fn shut_door_behind(
    mut commands: Commands,
    mut pursuit: ResMut<Pursuit>,
    floor: Option<Res<Floor>>,
    run: Res<RunState>,
    peers: Res<crate::net::Peers>,
    player: Query<&Transform, With<Player>>,
    mut doors: Query<(Entity, &mut Door)>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
) {
    if pursuit.door_shut || pursuit.phase == PursuitPhase::Idle {
        return;
    }
    let (Some(floor), Ok(p)) = (floor, player.single()) else { return };
    let Some(r) = floor.0.room(PURSUIT_LAST) else { return };
    let door_z = r.min.y - WALL_T;
    if !door_should_shut(p.translation.z, run.vitality > 0.0, peers.0.values().map(|s| (s.p[2], s.alive)), door_z) {
        return;
    }
    let number = PURSUIT_LAST + 1;
    for (e, mut d) in &mut doors {
        if d.number != number || d.mimic {
            continue;
        }
        d.open = false;
        d.locked = true;
        d.key_room = None;
        if d.blocker != Entity::PLACEHOLDER {
            let (x0, x1) = (r.exit_x - crate::world::DOOR_W / 2.0, r.exit_x + crate::world::DOOR_W / 2.0);
            commands.entity(d.blocker).try_insert(Collider(crate::physics::Aabb {
                min: Vec3::new(x0, 0.0, r.min.y - WALL_T),
                max: Vec3::new(x1, crate::world::DOOR_H, r.min.y),
            }));
        }
        commands.entity(e).try_remove::<Interactable>();
    }
    if let Some(rt) = rt.as_mut() {
        rt.opened.remove(&number);
    }
    pursuit.door_shut = true;
    info!("Door {number:03} shut behind the survivors");
    shake.add(0.6);
    sfx.write(PlaySfx(Sfx::DoorLocked));
    sfx.write(PlaySfx(Sfx::Stinger));
    subs.write(Subtitle::info("[BANG] Door 047 slams shut behind you. Keep going."));
}

/// Pure rule: shut once the local player (if alive) and every living teammate
/// are at least a step past the door (rooms run toward -Z).
pub fn door_should_shut(me_z: f32, me_alive: bool, peers: impl Iterator<Item = (f32, bool)>, door_z: f32) -> bool {
    let past = |z: f32| z < door_z - 1.2;
    let mut anyone = false;
    if me_alive {
        if !past(me_z) {
            return false;
        }
        anyone = true;
    }
    for (z, alive) in peers {
        if alive {
            if !past(z) {
                return false;
            }
            anyone = true;
        }
    }
    anyone
}

/// During the chase, real doors on the route open as the player reaches them.
#[allow(clippy::type_complexity)]
fn auto_doors(
    mut commands: Commands,
    pursuit: Res<Pursuit>,
    player: Query<&Transform, With<Player>>,
    mut doors: Query<(Entity, &mut Door, &GlobalTransform)>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if !matches!(pursuit.phase, PursuitPhase::Chasing { .. } | PursuitPhase::Cue(_)) {
        return;
    }
    let Ok(p) = player.single() else { return };
    for (e, mut door, gt) in &mut doors {
        if door.open || door.mimic || !(PURSUIT_FIRST + 1..=PURSUIT_LAST + 1).contains(&door.number) {
            continue;
        }
        let d = gt.translation() - p.translation;
        if Vec2::new(d.x, d.z).length() > AUTO_DOOR {
            continue;
        }
        door.locked = false;
        door.key_room = None;
        door.open = true;
        if door.blocker != Entity::PLACEHOLDER {
            commands.entity(door.blocker).try_remove::<Collider>();
        }
        commands.entity(e).try_remove::<Interactable>();
        if let Some(rt) = rt.as_mut() {
            rt.opened.insert(door.number);
        }
        sfx.write(PlaySfx(Sfx::DoorOpen));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_runs_through_every_chase_room() {
        let plan = crate::floor::plan::generate(7);
        let path = pursuit_path(&plan);
        assert!(path.len() > 10);
        // Heads north (towards higher doors = more negative z).
        assert!(path.last().unwrap().z < path.first().unwrap().z);
        for w in path.windows(2) {
            assert!(w[1].z <= w[0].z + 0.01, "path doubles back: {w:?}");
        }
        // It weaves through the barricade openings, not straight lines.
        let turns = path.windows(2).filter(|w| (w[1].x - w[0].x).abs() > 2.0).count();
        assert!(turns >= 6, "only {turns} turns");
    }

    #[test]
    fn guide_leads_through_the_real_door() {
        let plan = crate::floor::plan::generate(11);
        let path = pursuit_path(&plan);
        // A player standing anywhere on the route sees the light ahead of them.
        for k in [3usize, 8, 14] {
            let p = path[k.min(path.len() - 1)];
            let s = progress_along(&path, p);
            let g = point_along(&path, s + GUIDE_LEAD);
            assert!(g.z <= p.z + 0.01, "light behind the player");
        }
        // In a choice room the route passes the real exit, never a fake door.
        for i in crate::floor::plan::CHASE_CHOICE_ROOMS {
            let r = plan.room(i).unwrap();
            let at_wall: Vec<_> = path.iter().filter(|v| (v.z - r.min.y).abs() < 1.0).collect();
            assert!(!at_wall.is_empty());
            for v in at_wall {
                assert!((v.x - r.exit_x).abs() < 0.2, "route at x {} but exit at {}", v.x, r.exit_x);
                for m in &r.mimics {
                    assert!((v.x - m.x).abs() > 1.0);
                }
            }
        }
    }

    #[test]
    fn door_waits_for_every_living_player() {
        let door = -100.0;
        assert!(!door_should_shut(-99.0, true, std::iter::empty(), door), "still in the doorway");
        assert!(door_should_shut(-102.0, true, std::iter::empty(), door));
        assert!(!door_should_shut(-102.0, true, [(-90.0, true)].into_iter(), door), "a teammate is still behind");
        assert!(door_should_shut(-102.0, true, [(-90.0, false)].into_iter(), door), "the dead don't hold the door");
        assert!(door_should_shut(-50.0, false, [(-103.0, true)].into_iter(), door), "spectating while friends got through");
        assert!(!door_should_shut(-50.0, false, std::iter::empty(), door), "nobody alive");
    }

    #[test]
    fn sprinting_outruns_it_walking_does_not() {
        assert!(PURSUIT_SPEED < crate::player::SPRINT_SPEED);
        assert!(PURSUIT_SPEED > crate::player::WALK_SPEED);
        assert!(CATCH_UP_SPEED < crate::player::SPRINT_SPEED);
    }
}
