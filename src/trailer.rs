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
use crate::floor::plan::{Area, Encounter, LightState, RoomPlan};
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pick {
    Room(u32),
    Gardens,
}

const SHOTS: [(&str, Shot); 11] = [
    ("arrival", Shot::Cutscene { id: INTRO, from: 6.5, to: 12.4 }),
    ("guest_wing", Shot::Walk { pick: Pick::Room(4), secs: 3.2 }),
    ("archive", Shot::Walk { pick: Pick::Room(24), secs: 3.2 }),
    ("squad", Shot::Squad { secs: 3.6 }),
    ("gardens", Shot::Walk { pick: Pick::Gardens, secs: 4.0 }),
    ("fake_doors", Shot::FakeDoors { secs: 3.2 }),
    ("dark", Shot::Dark { secs: 3.0 }),
    ("gallery", Shot::Gallery { secs: 3.2 }),
    ("listener", Shot::Listener { secs: 4.2 }),
    ("hollow", Shot::Cutscene { id: THE_HOLLOW, from: 5.0, to: 12.6 }),
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
    log: Vec<String>,
}

pub struct TrailerPlugin;

impl Plugin for TrailerPlugin {
    fn build(&self, app: &mut App) {
        let Ok(dir) = std::env::var("HALLWAYS_TRAILER") else { return };
        let _ = std::fs::create_dir_all(&dir);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / FPS)))
            .insert_resource(Recorder { dir, shot: 0, phase: Phase::Setup, frame: 0, first: 0, room: 0, from: Vec3::ZERO, to: Vec3::ZERO, yaw: 0.0, log: vec![] })
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
        Shot::Cutscene { .. } => None,
    };
    r.cloned()
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
) {
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
        let path = format!("{}/segments.txt", rec.dir);
        let _ = std::fs::write(path, rec.log.join("\n"));
        exit.write(AppExit::Success);
        return;
    };

    match rec.phase {
        Phase::Setup => {
            active.stop();
            **mood = crate::world::Mood::default();
            **chase = crate::hollow::ChaseState::default();
            force.0 = None;
            peers.0.clear();
            **session = NetSession::default();
            inv.flashlight_on = false;
            p.crouching = false;
            suppress.0 = !matches!(shot, Shot::Cutscene { id: THE_HOLLOW, .. });
            match shot {
                Shot::Cutscene { id, .. } if id == INTRO => {
                    tf.translation = crate::world::elevator::PLAYER_START;
                    p.yaw = 0.0;
                    play.write(PlayCutscene(INTRO.into()));
                }
                Shot::Cutscene { id, .. } if id == THE_HOLLOW => {
                    tf.translation = Vec3::new(0.0, 0.0, -8.0);
                    p.yaw = 0.0;
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
                    }
                    let entry = Vec3::new(r.entry_x, 0.0, r.max.y - 0.7);
                    let goal = Vec3::new(r.exit_x, 0.0, r.min.y + r.depth() * 0.42);
                    let (from, to) = match shot {
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
                    rec.yaw = (-d.x).atan2(-d.z);
                    p.yaw = rec.yaw;
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
            let ready = match shot {
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
                Shot::Walk { secs, .. } | Shot::Squad { secs } | Shot::FakeDoors { secs } | Shot::Dark { secs } | Shot::Gallery { secs } | Shot::Listener { secs } => (secs, t >= secs),
            };
            if !matches!(shot, Shot::Cutscene { .. }) {
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
