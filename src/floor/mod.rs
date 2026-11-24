//! Phase 2: the 100-door procedural floor.
//!
//! * [`plan`] — pure, seeded generator (unit tested).
//! * [`build`] — turns room plans into geometry.
//! * Streaming — only rooms near the player exist; others are despawned and
//!   rebuilt identically on demand. [`FloorRuntime`] remembers what changed.

pub mod build;
pub mod plan;
pub mod scenery;

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::hollow::{ChasePhase, ChaseState};
use crate::interaction::Door;
use crate::physics::Collider;
use crate::rng::RunSeed;
use crate::run::{RunState, Subtitle};
use crate::state::AppState;
use crate::world::materials::SurfaceMaterials;
use crate::world::Builder;
use build::{BackCap, OpenOnSpawn, PropMaterials};
use plan::{FloorPlan, RoomKind, FIRST_ROOM};
#[cfg(test)]
use plan::LAST_ROOM;

/// Tags every root entity of a generated room (for streaming).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomMember(pub u32);

#[derive(Resource, Debug, Clone)]
pub struct Floor(pub FloorPlan);

/// Everything the player changed on this floor, so streamed rooms come back the same.
#[derive(Resource, Debug)]
pub struct FloorRuntime {
    pub opened: HashSet<u32>,
    pub searched: HashSet<(u32, u8)>,
    pub keys: HashSet<u32>,
    /// Fake doors already opened: (room, slot).
    pub mimics_triggered: HashSet<(u32, u8)>,
    /// Lamp brightness multiplier (Lights Low modifier).
    pub light_scale: f32,
    pub surges_done: HashSet<u32>,
    pub loaded: HashSet<u32>,
    /// When Door 100 was opened (for the summary fade).
    pub finished_at: Option<f32>,
    /// Item tuning (copied in at run start) used when building drawers.
    pub items: crate::data::ItemConfig,
    pub encounters_done: HashSet<u32>,
    /// Players in the run (rooms that need hiding get a spot for every two).
    pub players: u8,
}

impl Default for FloorRuntime {
    fn default() -> Self {
        Self {
            opened: HashSet::new(),
            searched: HashSet::new(),
            keys: HashSet::new(),
            mimics_triggered: HashSet::new(),
            surges_done: HashSet::new(),
            loaded: HashSet::new(),
            finished_at: None,
            items: crate::data::ItemConfig::default(),
            encounters_done: HashSet::new(),
            light_scale: 1.0,
            players: 1,
        }
    }
}

/// How many rooms behind/ahead of the player stay loaded.
pub const KEEP_BEHIND: u32 = 1;
pub const KEEP_AHEAD: u32 = 3;

/// Pure helper: which generated rooms should exist when the player is in `current`.
pub fn wanted_rooms(current: u32, gate_open: bool, last: u32) -> std::ops::RangeInclusive<u32> {
    let lo = current.saturating_sub(KEEP_BEHIND).max(FIRST_ROOM);
    let ahead = if gate_open || current >= FIRST_ROOM { KEEP_AHEAD } else { 1 };
    let hi = (current.max(1) + ahead).min(last);
    lo..=hi
}

pub struct FloorPlugin;

impl Plugin for FloorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloorRuntime>()
            .add_systems(OnEnter(AppState::Playing), setup_floor)
            .add_systems(
                Update,
                (custom_quick_start, stream_rooms, fix_open_doors, announce_areas, finish_floor, scenery::spin_props).run_if(in_state(AppState::Playing)),
            )
            .add_observer(scenery::start_sails
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_floor(
    mut commands: Commands,
    seed: Res<RunSeed>,
    cfg: Res<crate::data::EntityConfig>,
    items: Res<crate::data::ItemConfig>,
    profile: Res<crate::profile::Profile>,
    preset: Res<crate::presets::ActivePreset>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    peers: Res<crate::net::Peers>,
    asset_server: Res<AssetServer>,
) {
    use crate::profile::Modifier;
    let mut cfg = cfg.clone();
    if profile.has(Modifier::Restless) {
        for t in [&mut cfg.surge, &mut cfg.undertow, &mut cfg.whisperer, &mut cfg.gallery, &mut cfg.hush, &mut cfg.listener] {
            t.chance = (t.chance * 1.6).min(0.9);
            t.cooldown = t.cooldown.saturating_sub(t.cooldown / 3);
        }
    }
    // A custom preset (from Custom Floors) shapes the whole floor.
    let opts = match &preset.0 {
        Some(p) => {
            p.apply_entities(&mut cfg);
            p.gen_options()
        }
        None => plan::GenOptions::default(),
    };
    // A hand-built map lays out its own rooms; anything else is generated.
    let plan = match preset.0.as_ref().filter(|p| !p.rooms.is_empty()) {
        Some(p) => plan::from_map(&p.rooms, seed.0),
        None => plan::generate_custom(seed.0, &cfg, &opts),
    };
    commands.insert_resource(Floor(plan));
    let custom_light = preset.0.as_ref().map_or(1.0, |p| p.light_scale());
    let light_scale = custom_light * if profile.has(Modifier::LightsLow) { 0.5 } else { 1.0 };
    let players = (peers.0.len() + 1).min(crate::net::proto::MAX_PLAYERS as usize) as u8;
    // A mode can take items out of the drawers.
    let mut items = items.clone();
    if let Some(p) = preset.0.as_ref().filter(|p| p.items_off != 0) {
        for (i, k) in crate::items::ItemKind::ALL.iter().enumerate() {
            if p.items_off & (1 << i) != 0 {
                k.tuning_mut(&mut items).weight = 0.0;
            }
        }
        if p.items_off & 0b1_1111 == 0b1_1111 {
            items.item_chance = 0.0;
        }
    }
    commands.insert_resource(FloorRuntime { items, light_scale, players, ..default() });
    commands.insert_resource(PropMaterials::new(&mut std_mats, &mut meshes, &mut images, &asset_server));
}

#[allow(clippy::too_many_arguments)]
fn stream_rooms(
    mut commands: Commands,
    floor: Option<Res<Floor>>,
    mut rt: ResMut<FloorRuntime>,
    run: Res<RunState>,
    chase: Res<ChaseState>,
    mats: Option<Res<SurfaceMaterials>>,
    props: Option<Res<PropMaterials>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    members: Query<(Entity, &RoomMember)>,
    caps: Query<(Entity, &BackCap)>,
) {
    let (Some(floor), Some(mats), Some(props)) = (floor, mats, props) else { return };
    let gate_open = chase.phase == ChasePhase::Done;
    let wanted = wanted_rooms(run.current_room, gate_open, floor.0.last_room);
    // Unload rooms that are out of range.
    let stale: Vec<u32> = rt.loaded.iter().copied().filter(|i| !wanted.contains(i)).collect();
    if !stale.is_empty() {
        for (e, m) in &members {
            if stale.contains(&m.0) {
                commands.entity(e).try_despawn();
            }
        }
        for i in stale {
            rt.loaded.remove(&i);
        }
    }
    // Build at most one room per frame to avoid hitches.
    if let Some(i) = wanted.clone().find(|i| !rt.loaded.contains(i)) {
        let Some(room) = floor.0.room(i) else { return };
        let mut b = Builder { commands: &mut commands, meshes: &mut meshes, mats: mats.clone(), cache: HashMap::new(), scope: AppState::Playing, parent: None, member: None, base_y: 0.0 };
        build::spawn_room(&mut b, room, &rt, &props, &asset_server, &mut images, &mut std_mats);
        build::close_south_side(&mut b, room, floor.0.room(i.wrapping_sub(1)), &props);
        build::final_door_surround(&mut b, room);
        rt.loaded.insert(i);
    }
    // The oldest loaded room gets a back wall while the room behind it is unloaded.
    let lo = *wanted.start();
    for (e, c) in &caps {
        if c.0 != lo || rt.loaded.contains(&(c.0 - 1)) {
            commands.entity(e).try_despawn();
        }
    }
    let capped = caps.iter().any(|(_, c)| c.0 == lo);
    if lo > FIRST_ROOM && rt.loaded.contains(&lo) && !rt.loaded.contains(&(lo - 1)) && !capped
        && let (Some(r), Some(prev)) = (floor.0.room(lo), floor.0.room(lo - 1))
    {
        let mut b = Builder { commands: &mut commands, meshes: &mut meshes, mats: mats.clone(), cache: HashMap::new(), scope: AppState::Playing, parent: None, member: None, base_y: 0.0 };
        build::back_cap(&mut b, r, prev, &props, &mut images, &mut std_mats);
    }
}

/// Custom floors skip the opening (lift, Room 000 and The Hollow's corridor):
/// once the lift has left (online) or straight away (solo), the player stands
/// in the first room of the floor they picked.
#[allow(clippy::too_many_arguments)]
fn custom_quick_start(
    preset: Res<crate::presets::ActivePreset>,
    floor: Option<Res<Floor>>,
    run: Res<RunState>,
    lobby: Res<crate::lobby::LiftLobby>,
    cutscene: Res<crate::cutscene::ActiveCutscene>,
    mut chase: ResMut<ChaseState>,
    mut player: Query<(&mut crate::player::Player, &mut Transform)>,
    mut subs: MessageWriter<Subtitle>,
    mut done: Local<bool>,
    (mut anchor, mut vars, mut play): (ResMut<crate::ending::CutsceneAnchor>, ResMut<crate::cutscene::CutsceneVars>, MessageWriter<crate::cutscene::PlayCutscene>),
) {
    if run.elapsed < 0.2 {
        *done = false;
    }
    let (Some(p), Some(floor)) = (preset.0.as_ref(), floor) else { return };
    if *done || lobby.waiting || cutscene.is_playing() || std::env::var_os("HALLWAYS_CAPTURE").is_some() {
        return;
    }
    let Ok((mut pl, mut tf)) = player.single_mut() else { return };
    // A floor can start you further in.
    let first = (p.start_door + 1).clamp(FIRST_ROOM, floor.0.last_room);
    let Some(r) = floor.0.room(first) else { return };
    *done = true;
    chase.phase = ChasePhase::Done;
    chase.active = false;
    tf.translation = Vec3::new(r.entry_x, r.entry_height, r.max.y - 0.9);
    pl.yaw = 0.0;
    pl.pitch = 0.0;
    // A proper opening: a sweep over the first room under the floor's title.
    let qa = ["HALLWAYS_BOT", "HALLWAYS_WARP", "HALLWAYS_QA", "HALLWAYS_TRAILER"].iter().any(|v| std::env::var_os(v).is_some()) && std::env::var_os("HALLWAYS_QA_CUTSCENES").is_none();
    if qa {
        subs.write(Subtitle { text: format!("-  {}  -   {}", p.name.to_uppercase(), r.area.name().to_uppercase()), seconds: 4.0, danger: false });
    } else {
        anchor.0 = Vec3::new(r.entry_x, r.entry_height, r.max.y);
        // Player-made modes and maps say so on the title card.
        let area = if p.player_made() { format!("PLAYER-MADE  -  {}", r.area.name()) } else { r.area.name().to_string() };
        *vars = crate::cutscene::CutsceneVars { floor: p.name.clone(), area, doors: floor.0.final_door() };
        play.write(crate::cutscene::PlayCutscene(crate::cutscene::CUSTOM_START.into()));
    }
}

/// Doors rebuilt in the "already open" state lose their blocking collider.
fn fix_open_doors(mut commands: Commands, doors: Query<(Entity, &Door), Added<OpenOnSpawn>>) {
    for (e, d) in &doors {
        if d.blocker != Entity::PLACEHOLDER {
            commands.entity(d.blocker).try_remove::<Collider>();
        }
        commands.entity(e).try_remove::<OpenOnSpawn>();
    }
}

fn announce_areas(run: Res<RunState>, floor: Option<Res<Floor>>, mut last: Local<u32>, mut subs: MessageWriter<Subtitle>) {
    if run.current_room == *last {
        return;
    }
    *last = run.current_room;
    let Some(floor) = floor else { return };
    if let Some(r) = floor.0.room(run.current_room) {
        match r.kind {
            RoomKind::Transition => {
                subs.write(Subtitle { text: format!("-  {}  -", r.area.name().to_uppercase()), seconds: 3.5, danger: false });
            }
            RoomKind::KeyPuzzle if r.feature == Some(plan::Feature::ReadingHall) => {
                subs.write(Subtitle { text: format!("-  THE READING HALL  -   Door {:03} is locked. The key is somewhere among the shelves.", r.exit_door()), seconds: 6.0, danger: false });
            }
            RoomKind::KeyPuzzle => {
                subs.write(Subtitle::info(format!("Door {:03} is locked. The key must be in here somewhere.", r.exit_door())));
            }
            RoomKind::Final => {
                subs.write(Subtitle { text: "Door 100. The way out.".into(), seconds: 3.5, danger: false });
            }
            _ => {}
        }
    }
}

/// Opening Door 100 ends the floor: fade, then the run summary.
fn finish_floor(
    floor: Option<Res<Floor>>,
    mut rt: ResMut<FloorRuntime>,
    run: Res<RunState>,
    mut anchor: ResMut<crate::ending::CutsceneAnchor>,
    mut play: MessageWriter<crate::cutscene::PlayCutscene>,
) {
    let Some(floor) = floor else { return };
    if rt.opened.contains(&floor.0.final_door()) && rt.finished_at.is_none() {
        rt.finished_at = Some(run.elapsed);
    }
    // A beat after the final door swings open, the ending cutscene plays;
    // the victory screen follows when it ends (see `ending`).
    if let Some(t) = rt.finished_at
        && run.elapsed - t > 0.8
        && t.is_finite()
    {
        rt.finished_at = Some(f32::INFINITY);
        crate::ending::start_ending(&mut anchor, &mut play, &floor, &rt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_window() {
        // Before the gate opens only the first generated room is preloaded.
        assert_eq!(wanted_rooms(0, false, LAST_ROOM), 2..=2);
        assert_eq!(wanted_rooms(1, true, LAST_ROOM), 2..=4);
        assert_eq!(wanted_rooms(10, true, LAST_ROOM), 9..=13);
        assert_eq!(wanted_rooms(23, true, 24), 22..=24);
        assert_eq!(wanted_rooms(99, true, LAST_ROOM), 98..=99);
    }
}
