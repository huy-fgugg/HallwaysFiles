//! Phase 2: the 100-door procedural floor.
//!
//! * [`plan`] — pure, seeded generator (unit tested).
//! * [`build`] — turns room plans into geometry.
//! * Streaming — only rooms near the player exist; others are despawned and
//!   rebuilt identically on demand. [`FloorRuntime`] remembers what changed.

pub mod build;
pub mod plan;

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::hollow::{ChasePhase, ChaseState};
use crate::interaction::Door;
use crate::physics::Collider;
use crate::rng::RunSeed;
use crate::run::{RunState, Subtitle};
use crate::state::{AppState, Overlay};
use crate::world::materials::SurfaceMaterials;
use crate::world::Builder;
use build::{OpenOnSpawn, PropMaterials};
use plan::{FloorPlan, RoomKind, FINAL_DOOR, FIRST_ROOM};
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
                (stream_rooms, fix_open_doors, unlock_gate_after_hollow, announce_areas, finish_floor).run_if(in_state(AppState::Playing)),
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
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
) {
    use crate::profile::Modifier;
    let mut cfg = cfg.clone();
    if profile.has(Modifier::Restless) {
        for t in [&mut cfg.surge, &mut cfg.undertow, &mut cfg.whisperer, &mut cfg.gallery, &mut cfg.hush] {
            t.chance = (t.chance * 1.6).min(0.9);
            t.cooldown = t.cooldown.saturating_sub(t.cooldown / 3);
        }
    }
    commands.insert_resource(Floor(plan::generate_with(seed.0, &cfg)));
    let light_scale = if profile.has(Modifier::LightsLow) { 0.5 } else { 1.0 };
    commands.insert_resource(FloorRuntime { items: items.clone(), light_scale, ..default() });
    commands.insert_resource(PropMaterials::new(&mut std_mats, &mut meshes, &mut images));
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
        build::final_door_surround(&mut b, room);
        rt.loaded.insert(i);
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

/// Surviving The Hollow unlocks Door 002 into the generated floor.
fn unlock_gate_after_hollow(chase: Res<ChaseState>, mut doors: Query<&mut Door>, mut subs: MessageWriter<Subtitle>, mut done: Local<bool>, run: Res<RunState>) {
    if run.elapsed < 0.2 {
        *done = false;
    }
    if *done || chase.phase != ChasePhase::Done {
        return;
    }
    for mut d in &mut doors {
        if d.number == 2 && d.locked {
            d.locked = false;
            *done = true;
            subs.write(Subtitle { text: "[Somewhere ahead, a lock clicks open. Door 002.]".into(), seconds: 4.0, danger: false });
        }
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
fn finish_floor(time: Res<Time>, floor: Option<Res<Floor>>, mut rt: ResMut<FloorRuntime>, run: Res<RunState>, mut next: ResMut<NextState<Overlay>>) {
    let final_door = floor.map_or(FINAL_DOOR, |f| f.0.final_door());
    if rt.opened.contains(&final_door) && rt.finished_at.is_none() {
        rt.finished_at = Some(run.elapsed);
    }
    if let Some(t) = rt.finished_at
        && run.elapsed - t > 1.5
    {
        rt.finished_at = Some(f32::INFINITY);
        next.set(Overlay::Summary);
    }
    let _ = time;
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
