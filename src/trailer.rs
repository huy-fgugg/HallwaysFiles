//! Trailer recorder. `HALLWAYS_TRAILER=<folder>` plays a scripted shot list
//! through the real game at a fixed 24 frames per second of game time and
//! saves every frame as a PNG (so even a slow software renderer produces
//! smooth footage). `segments.txt` in the folder lists each shot's frames;
//! `packaging/trailer/make_trailer.py` adds title cards and music.

use std::time::Duration;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::time::TimeUpdateStrategy;

use crate::cutscene::{ActiveCutscene, PlayCutscene, ENDING, INTRO, THE_HOLLOW};
use crate::floor::plan::{Area, Encounter, Feature, LightState, RoomPlan, PURSUIT_FIRST, PURSUIT_LAST, READING_HALL};
use crate::interaction::Door;
use crate::pursuit::{Pursuit, PursuitHollow, PursuitPhase};
use crate::surge::{SurgePhase, SurgeState};
use crate::floor::{Floor, FloorRuntime};
use crate::net::{NetSession, PeerState, Peers};
use crate::player::Player;
use crate::state::{AppState, Overlay};

pub const FPS: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shot {
    /// Record a cutscene between two of its timestamps.
    Cutscene { id: &'static str, from: f32, to: f32 },
    /// Walk through a room toward its exit.
    Walk { pick: Pick, secs: f32 },
    /// Walk with two teammates (online avatars) beside you.
    Squad { secs: f32 },
    /// Pan across a choice wall of look-alike doors.
    FakeDoors { secs: f32 },
    /// A dark room by flashlight.
    Dark { secs: f32 },
    /// The Gallery opens its eyes.
    Gallery { secs: f32 },
    /// Crouched and silent while The Listener patrols.
    Listener { secs: f32 },
    /// Walk a room's route (through every opening) at `speed`.
    Route { pick: Pick, secs: f32, speed: f32 },
    /// Stand aside while The Surge tears through the room.
    Surge { secs: f32 },
    /// Door 041: look back as The Hollow rises, then run the barricades.
    Chase { secs: f32 },
    /// Door 047 slams shut behind you.
    Slam { secs: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pick {
    Room(u32),
    Gardens,
    Zigzag,
    ReadingHall,
}

/// The chase shot looks back this long before turning to run.
const LOOK_BACK: f32 = 2.3;

const SHOTS: [(&str, Shot); 16] = [
    ("arrival", Shot::Cutscene { id: INTRO, from: 6.5, to: 12.4 }),
    ("guest_wing", Shot::Walk { pick: Pick::Room(4), secs: 3.2 }),
    ("zigzag", Shot::Route { pick: Pick::Zigzag, secs: 4.6, speed: 2.6 }),
    ("archive", Shot::Walk { pick: Pick::Room(24), secs: 3.2 }),
    ("reading_hall", Shot::Route { pick: Pick::ReadingHall, secs: 6.0, speed: 1.5 }),
    ("squad", Shot::Squad { secs: 3.6 }),
    ("gardens", Shot::Walk { pick: Pick::Gardens, secs: 4.0 }),
    ("fake_doors", Shot::FakeDoors { secs: 3.2 }),
    ("dark", Shot::Dark { secs: 3.0 }),
    ("gallery", Shot::Gallery { secs: 3.2 }),
    ("listener", Shot::Listener { secs: 4.2 }),
    ("surge", Shot::Surge { secs: 3.4 }),
    ("hollow", Shot::Cutscene { id: THE_HOLLOW, from: 5.0, to: 12.6 }),
    ("chase", Shot::Chase { secs: 8.5 }),
    ("slam", Shot::Slam { secs: 3.2 }),
    ("ending", Shot::Cutscene { id: ENDING, from: 12.3, to: 17.3 }),
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Setup,
    Warmup(f32),
    Record(f32),
}

#[derive(Resource)]
struct Recorder {
    dir: String,
    shot: usize,
    phase: Phase,
    frame: u32,
    first: u32,
    room: u32,
    from: Vec3,
    to: Vec3,
    yaw: f32,
    /// Route for Route/Chase shots, and how far along it the camera is.
    path: Vec<Vec3>,
    along: f32,
    /// Frames waited after the last shot.
    tail: u32,
    log: Vec<String>,
}

pub struct TrailerPlugin;

impl Plugin for TrailerPlugin {
    fn build(&self, app: &mut App) {
        let Ok(dir) = std::env::var("HALLWAYS_TRAILER") else { return };
        let _ = std::fs::create_dir_all(&dir);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / FPS)))
            .insert_resource(Recorder { dir, shot: 0, phase: Phase::Setup, frame: 0, first: 0, room: 0, from: Vec3::ZERO, to: Vec3::ZERO, yaw: 0.0, path: vec![], along: 0.0, tail: 0, log: vec![] })
            .add_systems(Update, start.run_if(in_state(AppState::MainMenu)))
            .add_systems(Update, record.run_if(in_state(AppState::Playing)));
    }
}

fn start(mut next: ResMut<NextState<AppState>>, mut skip: ResMut<crate::lobby::SkipLobby>, mut done: Local<bool>) {
    if !*done {
        *done = true;
        skip.0 = true;
        next.set(AppState::Playing);
    }
}

fn pick_room(plan: &crate::floor::plan::FloorPlan, shot: Shot) -> Option<RoomPlan> {
    let lit = |r: &&RoomPlan| r.light != LightState::Dark && r.encounter.is_none();
    let r = match shot {
        Shot::Walk { pick: Pick::Room(i), .. } => plan.room(i),
        Shot::Walk { pick: Pick::Gardens, .. } => plan.rooms.iter().find(|r| r.area == Area::Grounds && r.light != LightState::Dark && r.width() > 12.0),
        Shot::Squad { .. } => plan.room(7),
        Shot::FakeDoors { .. } => plan.rooms.iter().filter(|r| r.mimics.len() >= 2).max_by_key(|r| r.mimics.len()),
        Shot::Dark { .. } => plan.rooms.iter().find(|r| r.light == LightState::Dark && r.index >= 10 && r.encounter.is_none()),
        Shot::Gallery { .. } => plan.rooms.iter().filter(|r| r.index >= 30).find(lit),
        Shot::Listener { .. } => plan.rooms.iter().find(|r| r.area == Area::Grounds && r.light != LightState::Dark && r.depth() > 12.0 && r.index > 52),
        Shot::Route { pick: Pick::Zigzag, .. } => plan.rooms.iter().find(|r| r.feature == Some(Feature::Zigzag) && r.light != LightState::Dark && r.encounter.is_none()),
        Shot::Route { pick: Pick::ReadingHall, .. } => plan.room(READING_HALL),
        Shot::Route { .. } | Shot::Walk { .. } => None,
        Shot::Surge { .. } => plan.rooms.iter().find(|r| r.encounter == Some(Encounter::Surge) && r.width() >= 5.5 && r.light != LightState::Dark && r.feature.is_none()),
        Shot::Chase { .. } => plan.room(PURSUIT_FIRST),
        Shot::Slam { .. } => plan.room(PURSUIT_LAST + 1),
        Shot::Cutscene { .. } => None,
    };
    r.cloned()
}

/// Moves `along` metres down a polyline; returns the point and travel direction.
fn along_path(path: &[Vec3], along: f32) -> (Vec3, Vec3) {
    let mut left = along;
    for w in path.windows(2) {
        let seg = w[1] - w[0];
        let len = seg.length();
        if len > 1e-4 && left <= len {
            return (w[0] + seg / len * left, seg / len);
        }
        left -= len;
    }
    let n = path.len();
    let dir = if n >= 2 { (path[n - 1] - path[n - 2]).normalize_or(Vec3::NEG_Z) } else { Vec3::NEG_Z };
    (path.last().copied().unwrap_or(Vec3::ZERO), dir)
}

fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

/// Turns `from` toward `to` by at most `k` (0..1) of the way, the short way round.
fn turn(from: f32, to: f32, k: f32) -> f32 {
    let d = (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + d * k.clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn record(
    mut commands: Commands,
    mut rec: ResMut<Recorder>,
    time: Res<Time>,
    floor: Option<Res<Floor>>,
    mut rt: Option<ResMut<FloorRuntime>>,
    mut player: Query<(&mut Player, &mut Transform)>,
    mut active: ResMut<ActiveCutscene>,
    mut play: MessageWriter<PlayCutscene>,
    mut world: (
        ResMut<crate::encounters::ForceEncounter>,
        ResMut<crate::hollow::ChaseState>,
        ResMut<crate::hollow::SuppressHollow>,
        ResMut<crate::world::Mood>,
        ResMut<crate::run::RunState>,
        ResMut<crate::ui::hud::HideHud>,
    ),
    mut extra: (
        ResMut<crate::items::Inventory>,
        ResMut<Peers>,
        ResMut<NetSession>,
        ResMut<crate::ending::CutsceneAnchor>,
        Res<State<Overlay>>,
        ResMut<NextState<Overlay>>,
        MessageWriter<AppExit>,
    ),
    mut chase_kit: (ResMut<Pursuit>, ResMut<crate::guide::GuideOverride>, Res<SurgeState>, Query<(Entity, &GlobalTransform), With<PursuitHollow>>, Query<&mut Door>, Query<&GlobalTransform, With<crate::guide::Wisp>>),
) {
    let (pursuit, guide, surge_state, hollows, doors, wisps) = (&mut chase_kit.0, &mut chase_kit.1, &chase_kit.2, &chase_kit.3, &mut chase_kit.4, &chase_kit.5);
    let (force, chase, suppress, mood, run, hide) = (&mut world.0, &mut world.1, &mut world.2, &mut world.3, &mut world.4, &mut world.5);
    let (inv, peers, session, anchor, overlay, next_overlay, exit) = (&mut extra.0, &mut extra.1, &mut extra.2, &mut extra.3, &extra.4, &mut extra.5, &mut extra.6);
    let Some(floor) = floor else { return };
    let dt = time.delta_secs();
    hide.0 = true;
    // The camera crew never dies.
    run.vitality = 100.0;
    if *overlay.get() != Overlay::None {
        next_overlay.set(Overlay::None);
    }
    let Ok((mut p, mut tf)) = player.single_mut() else { return };
    let Some(&(name, shot)) = SHOTS.get(rec.shot) else {
        // Give the last screenshots a few frames to reach the disk.
        rec.tail += 1;
        if rec.tail == 1 {
            let path = format!("{}/segments.txt", rec.dir);
            let _ = std::fs::write(path, rec.log.join("\n"));
        }
        if rec.tail > 8 {
            exit.write(AppExit::Success);
        }
        return;
    };

    match rec.phase {
        Phase::Setup => {
            // HALLWAYS_TRAILER_ONLY=chase,slam records just those shots.
            if let Ok(only) = std::env::var("HALLWAYS_TRAILER_ONLY")
                && !only.split(',').any(|o| o.trim() == name)
            {
                rec.shot += 1;
                return;
            }
            active.stop();
            **mood = crate::world::Mood::default();
            **chase = crate::hollow::ChaseState::default();
            force.0 = None;
            peers.0.clear();
            **session = NetSession::default();
            inv.flashlight_on = false;
            p.crouching = false;
            suppress.0 = !matches!(shot, Shot::Cutscene { id: THE_HOLLOW, .. });
            **pursuit = Pursuit::default();
            for (e, _) in hollows.iter() {
                commands.entity(e).try_despawn();
            }
            guide.0 = Some(1.0e6);
            rec.path.clear();
            rec.along = 0.0;
            if matches!(shot, Shot::Chase { .. } | Shot::Slam { .. }) {
                // The opening corridor chase counts as survived.
                chase.phase = crate::hollow::ChasePhase::Done;
            }
            match shot {
                Shot::Cutscene { id, .. } if id == INTRO => {
                    tf.translation = crate::world::elevator::PLAYER_START;
                    p.yaw = 0.0;
                    play.write(PlayCutscene(INTRO.into()));
                }
                Shot::Cutscene { id, .. } if id == THE_HOLLOW => {
                    // The corridor no longer wakes it (the old chase is gone): play it directly.
                    tf.translation = Vec3::new(0.0, 0.0, -8.0);
                    p.yaw = 0.0;
                    play.write(PlayCutscene(THE_HOLLOW.into()));
                }
                Shot::Cutscene { .. } => {
                    if let Some(r) = floor.0.room(floor.0.last_room) {
                        rec.room = r.index;
                        tf.translation = Vec3::new(r.exit_x, 0.0, r.min.y + 2.5);
                    }
                }
                _ => {
                    let Some(r) = pick_room(&floor.0, shot) else {
                        warn!("trailer: no room for {name}; skipping");
                        rec.shot += 1;
                        return;
                    };
                    rec.room = r.index;
                    if let Some(rt) = rt.as_mut() {
                        // No surprise entities, except the one this shot is about.
                        rt.surges_done.insert(r.index);
                        if matches!(shot, Shot::Gallery { .. } | Shot::Listener { .. }) {
                            rt.encounters_done.remove(&r.index);
                        } else {
                            rt.encounters_done.insert(r.index);
                        }
                        if matches!(shot, Shot::Surge { .. }) {
                            rt.surges_done.remove(&r.index);
                        }
                        rt.keys.remove(&r.index);
                        match shot {
                            Shot::Chase { .. } => {
                                rt.opened.insert(PURSUIT_FIRST);
                            }
                            Shot::Slam { .. } => {
                                rt.opened.insert(PURSUIT_LAST + 1);
                            }
                            _ => {}
                        }
                    }
                    let entry = Vec3::new(r.entry_x, 0.0, r.max.y - 0.7);
                    let goal = Vec3::new(r.exit_x, 0.0, r.min.y + r.depth() * 0.42);
                    let (from, to) = match shot {
                        Shot::Route { .. } => {
                            let mut route: Vec<Vec3> = r.route().iter().map(|v| Vec3::new(v.x, 0.0, v.y)).collect();
                            // Start at the foot of the entry stairs, not up on the landing.
                            if r.entry_height > 0.0 {
                                let foot = r.max.y - crate::floor::plan::LANDING_DEPTH - r.entry_height / crate::floor::plan::STEP_RISE * crate::floor::plan::STEP_RUN - 0.3;
                                route.retain(|v| v.z < foot);
                                route.insert(0, Vec3::new(r.entry_x, 0.0, foot));
                            }
                            // Stay clear of furniture along the side walls (collision would
                            // shove the camera through the wall).
                            for v in &mut route {
                                v.x = v.x.clamp(r.min.x + 1.25, r.max.x - 1.25);
                            }
                            rec.path = route;
                            (rec.path[0], rec.path[0])
                        }
                        Shot::Surge { .. } => {
                            // Just out of reach of the line it flies along, mid-room.
                            let z = r.min.y + r.depth() * 0.55;
                            let k = (r.max.y - z) / r.depth();
                            let line_x = r.entry_x + (r.exit_x - r.entry_x) * k;
                            let east = r.max.x - line_x > line_x - r.min.x;
                            let x = (if east { line_x + 2.5 } else { line_x - 2.5 }).clamp(r.min.x + 1.3, r.max.x - 1.3);
                            let spot = Vec3::new(x, 0.0, z);
                            (spot, spot)
                        }
                        Shot::Chase { .. } => {
                            let spot = Vec3::new(r.entry_x, 0.0, r.max.y - 1.8);
                            let mut path = vec![spot];
                            path.extend(crate::pursuit::pursuit_path(&floor.0).into_iter().filter(|v| v.z < spot.z - 0.3));
                            rec.path = path;
                            (spot, spot)
                        }
                        Shot::Slam { .. } => {
                            let spot = Vec3::new(r.entry_x, 0.0, r.max.y - 1.9);
                            (spot, spot)
                        }
                        Shot::FakeDoors { .. } => {
                            let mid = Vec3::new(r.center().x, 0.0, r.min.y + r.depth() * 0.62);
                            (mid, mid)
                        }
                        Shot::Listener { .. } => {
                            let spot = Vec3::new(r.entry_x, 0.0, r.max.y - 2.0);
                            (spot, spot + Vec3::new(0.0, 0.0, -0.4))
                        }
                        _ => (entry, goal),
                    };
                    rec.from = from;
                    rec.to = to;
                    tf.translation = from;
                    let d = (goal - entry).normalize_or_zero();
                    rec.yaw = match shot {
                        // Look back toward the door you just came through.
                        Shot::Chase { .. } | Shot::Slam { .. } => std::f32::consts::PI,
                        Shot::Route { .. } if rec.path.len() >= 2 => yaw_of((rec.path[1] - rec.path[0]).normalize_or(Vec3::NEG_Z)),
                        // Toward the door it bursts through.
                        Shot::Surge { .. } => yaw_of((Vec3::new(r.entry_x, 0.0, r.max.y) - from).normalize_or(Vec3::NEG_Z)),
                        _ => (-d.x).atan2(-d.z),
                    };
                    p.yaw = rec.yaw;
                    if matches!(shot, Shot::Slam { .. }) {
                        // Red light and an open door, with the slam held back until recording.
                        pursuit.phase = PursuitPhase::Done;
                        pursuit.door_shut = true;
                    }
                    match shot {
                        Shot::Dark { .. } => {
                            let cfg = crate::data::ItemConfig::default();
                            if !inv.has(crate::items::ItemKind::Flashlight) {
                                inv.add(crate::items::ItemKind::Flashlight, &cfg);
                            }
                            inv.flashlight_on = true;
                        }
                        Shot::Gallery { .. } => force.0 = Some((r.index, Encounter::Gallery)),
                        Shot::Listener { .. } => {
                            force.0 = Some((r.index, Encounter::Listener));
                            p.crouching = true;
                        }
                        _ => {}
                    }
                }
            }
            rec.phase = Phase::Warmup(0.0);
        }
        Phase::Warmup(t) => {
            let t = t + dt;
            rec.phase = Phase::Warmup(t);
            // Keep the player planted while the room streams in.
            if !matches!(shot, Shot::Cutscene { .. }) {
                tf.translation = rec.from;
                p.yaw = rec.yaw;
            }
            // Chase: The Hollow must not rise until the camera rolls.
            if matches!(shot, Shot::Chase { .. }) {
                suppress.0 = true;
            }
            let held_open = match shot {
                Shot::Chase { .. } => Some(PURSUIT_FIRST),
                Shot::Slam { .. } => Some(PURSUIT_LAST + 1),
                _ => None,
            };
            if let Some(n) = held_open {
                for mut d in doors.iter_mut() {
                    if d.number == n && !d.mimic {
                        d.open = true;
                        d.locked = false;
                        d.t = 1.0;
                    }
                }
            }
            if matches!(shot, Shot::Slam { .. }) {
                **mood = crate::world::Mood { tint: Color::srgb(1.0, 0.12, 0.08), amount: 0.85, scale: 0.8, flicker: 0.35 };
            }
            let ready = match shot {
                Shot::Surge { .. } => match &surge_state.0 {
                    SurgePhase::Cue { t, room, .. } => *t >= crate::surge::cue_seconds(*room) - 1.3,
                    SurgePhase::Rushing { .. } => true,
                    SurgePhase::Idle => false,
                },
                Shot::Cutscene { id, from, .. } => {
                    if id == ENDING && !active.is_playing() && t > 1.2 && rt.as_ref().is_some_and(|rt| rt.loaded.contains(&rec.room)) {
                        if let Some(rt) = rt.as_ref() {
                            crate::ending::start_ending(anchor, &mut play, &floor, rt);
                        }
                    }
                    active.id == id && active.is_playing() && active.t >= from
                }
                // The Listener needs a moment to walk into view.
                Shot::Listener { .. } => t > 3.0,
                Shot::Gallery { .. } => t > 2.2,
                _ => t > 1.4 && rt.as_ref().is_some_and(|rt| rt.loaded.contains(&rec.room)),
            };
            if ready {
                rec.first = rec.frame;
                rec.phase = Phase::Record(0.0);
                match shot {
                    Shot::Chase { .. } => suppress.0 = false,
                    // The guiding light gathers in front of the camera.
                    Shot::Route { pick: Pick::ReadingHall, .. } => guide.0 = Some(0.0),
                    _ => {}
                }
            }
        }
        Phase::Record(t) => {
            let t = t + dt;
            rec.phase = Phase::Record(t);
            let dir = rec.dir.clone();
            let n = rec.frame;
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{dir}/frame_{n:05}.png")));
            rec.frame += 1;
            let (secs, done) = match shot {
                Shot::Cutscene { to, .. } => (0.0, active.t >= to || !active.is_playing()),
                Shot::Walk { secs, .. }
                | Shot::Squad { secs }
                | Shot::FakeDoors { secs }
                | Shot::Dark { secs }
                | Shot::Gallery { secs }
                | Shot::Listener { secs }
                | Shot::Route { secs, .. }
                | Shot::Surge { secs }
                | Shot::Chase { secs }
                | Shot::Slam { secs } => (secs, t >= secs),
            };
            let route_speed = match shot {
                Shot::Route { speed, .. } => Some(speed),
                Shot::Chase { .. } if t > LOOK_BACK => Some(crate::player::SPRINT_SPEED),
                Shot::Chase { .. } => Some(0.0),
                _ => None,
            };
            if let Some(speed) = route_speed {
                let before = tf.translation;
                rec.along += speed * dt;
                let (pos, dir) = along_path(&rec.path, rec.along);
                tf.translation = pos;
                let step = (pos - before).length();
                p.speed = step / dt.max(1e-3);
                p.bob_phase += step * 4.2;
                p.sprinting = matches!(shot, Shot::Chase { .. }) && speed > 0.0;
                // Look well ahead so turns lead the camera instead of facing walls.
                let (ahead, _) = along_path(&rec.path, rec.along + 3.4);
                let look = (ahead - pos).normalize_or(dir);
                let is_chase = matches!(shot, Shot::Chase { .. });
                let hollow = hollows.iter().next().map(|(_, g)| g.translation());
                let wisp = wisps.iter().next().map(|g| g.translation());
                let mut pitch = if is_chase { -0.06 } else { 0.0 };
                let want = if is_chase && t <= LOOK_BACK {
                    rec.yaw
                } else if is_chase && t > secs - 1.9 && hollow.is_some() {
                    // Glance back over the shoulder at it.
                    let h = hollow.unwrap_or(pos);
                    yaw_of((Vec3::new(h.x, 0.0, h.z) - Vec3::new(pos.x, 0.0, pos.z)).normalize_or(dir))
                } else if let Some(w) = wisp.filter(|_| matches!(shot, Shot::Route { pick: Pick::ReadingHall, .. })) {
                    // Follow the guiding light.
                    let flat = Vec3::new(w.x - pos.x, 0.0, w.z - pos.z);
                    pitch = ((w.y - (pos.y + p.eye)).atan2(flat.length().max(0.5))).clamp(-0.3, 0.45);
                    yaw_of(flat.normalize_or(look))
                } else {
                    yaw_of(look)
                };
                let rate = if is_chase { 6.0 } else { 3.0 };
                p.yaw = turn(p.yaw, want, dt * rate) + if is_chase && t <= LOOK_BACK { (t * 7.0).sin() * 0.004 } else { 0.0 };
                p.pitch += (pitch - p.pitch) * (dt * 4.0).min(1.0);
            } else if matches!(shot, Shot::Slam { .. }) {
                tf.translation = rec.from;
                p.yaw = rec.yaw;
                p.pitch = 0.05;
                // Slam at 0.8 s; the red fades a beat later.
                if t <= 0.8 {
                    **mood = crate::world::Mood { tint: Color::srgb(1.0, 0.12, 0.08), amount: 0.85, scale: 0.8, flicker: 0.35 };
                } else if t < 0.9 {
                    pursuit.door_shut = false;
                } else if t > 1.7 {
                    let k = ((t - 1.7) / 0.8).min(1.0);
                    **mood = crate::world::Mood { tint: Color::srgb(1.0, 0.12, 0.08), amount: 0.85 * (1.0 - k), scale: 0.8 + 0.2 * k, flicker: 0.0 };
                }
            } else if matches!(shot, Shot::Surge { .. }) {
                tf.translation = rec.from;
                p.yaw = rec.yaw + (t * 0.8).sin() * 0.03;
                p.pitch = 0.04;
            } else if !matches!(shot, Shot::Cutscene { .. }) {
                let k = (t / secs.max(0.01)).clamp(0.0, 1.0);
                let pos = rec.from.lerp(rec.to, k);
                let step = (pos - tf.translation).length();
                tf.translation = pos;
                p.speed = if step > 0.0 { step / dt.max(1e-3) } else { 0.0 };
                p.bob_phase += step * 4.2;
                p.pitch = 0.04;
                p.yaw = match shot {
                    // Slow pan across the row of doors.
                    Shot::FakeDoors { .. } => rec.yaw + 0.55 - 1.1 * k,
                    Shot::Listener { .. } => rec.yaw + 0.25 * (k * std::f32::consts::PI).sin(),
                    _ => rec.yaw + (t * 0.9).sin() * 0.05,
                };
                if let Shot::Squad { .. } = shot {
                    session.players = vec![
                        crate::net::proto::PlayerInfo { id: 2, name: "Maya".into() },
                        crate::net::proto::PlayerInfo { id: 3, name: "Theo".into() },
                    ];
                    let fwd = Quat::from_rotation_y(rec.yaw) * Vec3::NEG_Z;
                    let right = Quat::from_rotation_y(rec.yaw) * Vec3::X;
                    for (id, side, ahead, light) in [(2u8, -0.9f32, 1.6f32, true), (3, 0.9, 2.4, false)] {
                        let at = pos + fwd * ahead + right * side;
                        peers.0.insert(
                            id,
                            PeerState {
                                p: at.to_array(),
                                yaw: rec.yaw,
                                pitch: 0.0,
                                crouch: false,
                                speed: 2.6,
                                phase: p.bob_phase + id as f32,
                                gesture: crate::viewmodel::Gesture::None,
                                gt: 0.0,
                                right: if light { Some(crate::items::ItemKind::Flashlight) } else { Some(crate::items::ItemKind::Lockpick) },
                                left: None,
                                light,
                                hiding: false,
                                alive: true,
                                room: rec.room,
                            },
                        );
                    }
                }
            }
            if done {
                let line = format!("{name} {} {}", rec.first, rec.frame.saturating_sub(1));
                info!("trailer: {line}");
                rec.log.push(line);
                rec.shot += 1;
                rec.phase = Phase::Setup;
            }
        }
    }
}
