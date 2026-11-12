//! Scripted screenshot tour for visual QA, enabled by the `HALLWAYS_CAPTURE`
//! environment variable (value = output folder). Visits the main menu, then a
//! set of viewpoints in the test level, saves PNGs and exits. Inert otherwise.

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};

use crate::player::Player;
use crate::state::AppState;

/// Shots inside generated rooms: (label, room index, yaw). Positioned from the floor plan.
const ROOM_SHOTS: &[(&str, u32, f32)] = &[
    ("20_guest_wing", 4, 0.0),
    ("21_archive", 24, 0.0),
    ("22_tunnels", 44, 0.0),
    ("23_quarry", 64, 0.0),
    ("24_atrium", 84, 0.0),
    ("25_final_door", 99, 0.0),
    // Special shots: the room is chosen from the plan (see `pick_room`).
    ("26_gallery", 0, 0.0),
    ("27_hush", 0, 0.0),
    ("28_dark_flashlight", 0, 0.0),
    ("29_inventory", 4, 0.0),
    ("30_raised_exit", 0, 0.0),
    ("31_entry_landing", 0, 0.0),
    ("32_key_loft", 0, 0.0),
    ("33_fake_doors", 0, 0.0),
    ("35_admin_panel", 4, 0.0),
    ("36_jumpscare", 4, 0.0),
];

/// For special shots, find a suitable room in this seed's floor.
fn pick_room(label: &str, plan: &crate::floor::plan::FloorPlan) -> Option<(u32, Option<crate::floor::plan::Encounter>)> {
    use crate::floor::plan::{Encounter, Feature, LightState, RoomKind};
    let lit = |r: &&crate::floor::plan::RoomPlan| r.light != LightState::Dark && r.kind != RoomKind::Hall && r.encounter.is_none();
    match label {
        "26_gallery" => plan.rooms.iter().filter(|r| r.index >= 30).find(lit).map(|r| (r.index, Some(Encounter::Gallery))),
        "27_hush" => plan.rooms.iter().filter(|r| r.index >= 50).find(lit).map(|r| (r.index, Some(Encounter::Hush))),
        "28_dark_flashlight" => plan.rooms.iter().find(|r| r.light == LightState::Dark && r.index >= 10 && r.encounter.is_none()).map(|r| (r.index, None)),
        "30_raised_exit" => plan.rooms.iter().find(|r| matches!(r.feature, Some(Feature::RaisedExit { .. }))).map(|r| (r.index, None)),
        "31_entry_landing" => plan.rooms.iter().find(|r| r.entry_height > 0.0).map(|r| (r.index, None)),
        "32_key_loft" => plan.rooms.iter().find(|r| matches!(r.feature, Some(Feature::KeyLoft { .. }))).map(|r| (r.index, None)),
        "33_fake_doors" => plan.rooms.iter().filter(|r| r.mimics.len() >= 2).max_by_key(|r| r.mimics.len()).map(|r| (r.index, None)),
        _ => None,
    }
}

/// (label, feet position, yaw, pitch). `None` = main menu for the first
/// shot, and "hold position, keep the clock running" afterwards (used to
/// photograph cutscenes mid-play).
type Shot = (&'static str, Option<Vec3>, f32, f32);

const SHOTS: &[Shot] = &[
    ("01_main_menu", None, 0.0, 0.0),
    ("01b_play_menu", None, 0.0, 0.0),
    ("02_spawn_view", Some(Vec3::new(0.0, 0.0, 3.4)), 0.0, -0.05),
    ("02b_inside_lift", Some(Vec3::new(0.0, 0.0, 6.9)), 0.0, 0.05),
    ("02d_lobby", Some(Vec3::new(0.0, 0.0, 6.9)), 0.0, 0.05),
    ("02c_lift_from_room", Some(Vec3::new(0.4, 0.0, 1.6)), std::f32::consts::PI, 0.05),
    ("03_drawer_cabinet", Some(Vec3::new(-2.2, 0.0, -1.6)), 1.5708, -0.25),
    ("04_wardrobe", Some(Vec3::new(1.4, 0.0, 1.4)), -1.5708, 0.0),
    ("05_door_closeup", Some(Vec3::new(0.3, 0.0, -3.6)), 0.0, 0.05),
    ("06_corner_chair", Some(Vec3::new(0.5, 0.0, 1.0)), 2.3, -0.15),
    ("07_commode", Some(Vec3::new(2.4, 0.0, -2.9)), 0.0, -0.3),
    ("08_corridor", Some(Vec3::new(0.0, 0.0, -6.0)), 0.0, 0.0),
];

#[derive(Resource)]
struct Capture {
    dir: String,
    index: usize,
    timer: f32,
    shot_taken: bool,
    frames: u32,
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("HALLWAYS_CAPTURE") {
            let _ = std::fs::create_dir_all(&dir);
            app.insert_resource(Capture { dir, index: 0, timer: 0.0, shot_taken: false, frames: 0 })
                .add_systems(Update, run_capture);
        }
    }
}

fn run_capture(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cap: ResMut<Capture>,
    mut next_app: ResMut<NextState<AppState>>,
    app_state: Res<State<AppState>>,
    mut player: Query<(&mut Player, &mut Transform)>,
    mut exit: MessageWriter<AppExit>,
    floor: Option<Res<crate::floor::Floor>>,
    mut rt: Option<ResMut<crate::floor::FloorRuntime>>,
    (mut force, mut inv): (ResMut<crate::encounters::ForceEncounter>, ResMut<crate::items::Inventory>),
    (mut run, app_overlay, mut next_overlay): (ResMut<crate::run::RunState>, Res<State<crate::state::Overlay>>, ResMut<NextState<crate::state::Overlay>>),
    mut lobby: ResMut<crate::lobby::LiftLobby>,
    (mut admin, mut effects): (ResMut<crate::admin::AdminUnlocked>, MessageWriter<crate::admin::AdminEffect>),
) {
    cap.timer += time.delta_secs();
    cap.frames += 1;
    // HALLWAYS_CAPTURE_ONLY=28 photographs only shots whose label starts with "28".
    if let Ok(only) = std::env::var("HALLWAYS_CAPTURE_ONLY") {
        let label = SHOTS.get(cap.index).map(|s| s.0).or_else(|| ROOM_SHOTS.get(cap.index - SHOTS.len().min(cap.index)).map(|s| s.0));
        if let Some(l) = label
            && !l.starts_with(&only)
            && !l.starts_with("01")
        {
            cap.index += 1;
            cap.frames = 0;
            cap.timer = 0.0;
            return;
        }
    }
    // The Play screen shot opens that overlay; every other shot closes it.
    let cur = SHOTS.get(cap.index).map(|s| s.0);
    if cur == Some("01b_play_menu") {
        if *app_overlay.get() != crate::state::Overlay::Play {
            next_overlay.set(crate::state::Overlay::Play);
        }
    } else if *app_overlay.get() == crate::state::Overlay::Play {
        next_overlay.set(crate::state::Overlay::None);
    }
    let Some(&(label, pos, yaw, pitch)) = SHOTS.get(cap.index) else {
        // Generated-floor tour: stand near each room's south side looking at its exit.
        let k = cap.index - SHOTS.len();
        let Some(&(label, mut room, _)) = ROOM_SHOTS.get(k) else {
            if cap.timer > 2.0 {
                exit.write(AppExit::Success);
            }
            return;
        };
        if *app_state.get() != AppState::Playing {
            next_app.set(AppState::Playing);
            cap.frames = 0;
            return;
        }
        let (Some(floor), Ok((mut pl, mut tf))) = (floor, player.single_mut()) else { return };
        if room == 0 {
            let Some((r, enc)) = pick_room(label, &floor.0) else {
                cap.index += 1;
                return;
            };
            room = r;
            force.0 = enc.map(|e| (r, e));
            if enc.is_none() {
                // e.g. the flashlight shot: nothing should attack.
                force.0 = None;
            }
            if label.contains("flashlight") && cap.frames < 3 {
                let cfg = crate::data::ItemConfig::default();
                if !inv.has(crate::items::ItemKind::Flashlight) {
                    inv.add(crate::items::ItemKind::Flashlight, &cfg);
                }
                inv.flashlight_on = true;
            }
        }
        if room != 0 && !label.starts_with("26") && !label.starts_with("27") {
            force.0 = None;
        }
        if label == "29_inventory" && cap.frames < 3 {
            let cfg = crate::data::ItemConfig::default();
            *inv = crate::items::Inventory::default();
            for k in [crate::items::ItemKind::Flashlight, crate::items::ItemKind::Lockpick, crate::items::ItemKind::Lockpick, crate::items::ItemKind::Bandage] {
                inv.add(k, &cfg);
            }
            inv.selected = 1;
        }
        let Some(r) = floor.0.room(room) else { return };
        if let Some(rt) = rt.as_mut() {
            // Keep showcase shots clean: no natural encounters in these rooms.
            rt.surges_done.insert(room);
            if force.0.is_none() {
                rt.encounters_done.insert(room);
            }
        }
        // The capture camera can't look away or hide, so it never takes damage.
        run.vitality = 100.0;
        if label.starts_with("35") {
            admin.0 = true;
            if *app_overlay.get() != crate::state::Overlay::Admin && cap.frames > 8 {
                next_overlay.set(crate::state::Overlay::Admin);
            }
        } else if *app_overlay.get() == crate::state::Overlay::Admin {
            next_overlay.set(crate::state::Overlay::None);
        }
        if label.starts_with("36") && cap.frames > 20 {
            effects.write(crate::admin::AdminEffect { action: crate::admin::AdminAction::Jumpscare, by: "QA".into() });
        }
        if *app_overlay.get() == crate::state::Overlay::Dead {
            next_overlay.set(crate::state::Overlay::None);
        }
        if cap.frames < 6 {
            tf.translation = Vec3::new(r.entry_x, r.entry_height, r.max.y - 0.8);
            let mut target = Vec3::new(r.exit_x, 0.0, r.min.y);
            pl.pitch = 0.05;
            if label.starts_with("32") {
                if let Some(crate::floor::plan::Feature::KeyLoft { east }) = r.feature {
                    let wall = if east { r.max.x } else { r.min.x };
                    tf.translation = Vec3::new(r.center().x - (wall - r.center().x).signum() * 0.8, 0.0, r.max.y - 0.6);
                    target = Vec3::new(wall, 0.0, r.min.y + 3.0);
                    pl.pitch = 0.12;
                }
            }
            if label.starts_with("31") {
                pl.pitch = -0.55;
            }
            let d = target - tf.translation;
            pl.yaw = (-d.x).atan2(-d.z);
        }
        if cap.timer > 6.0 && cap.frames > 24 && !cap.shot_taken {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{}/{}.png", cap.dir, label)));
            cap.shot_taken = true;
            cap.frames = 12;
        }
        if cap.shot_taken && cap.frames > 18 {
            cap.index += 1;
            cap.timer = 0.0;
            cap.frames = 0;
            cap.shot_taken = false;
        }
        return;
    };
    // Show the elevator lobby panel for its shot only.
    lobby.waiting = label == "02d_lobby";
    // Settle time: longer for the first frames (shader compile + asset load).
    let settle = if cap.index <= 1 { 8.0 } else { 3.0 };
    if let Some(p) = pos {
        // Only request the transition once: re-setting the same state re-runs OnEnter.
        if *app_state.get() != AppState::Playing {
            next_app.set(AppState::Playing);
            cap.frames = 0;
            return;
        }
        if let (Ok((mut pl, mut tf)), true) = (player.single_mut(), cap.frames < 6) {
            tf.translation = p;
            pl.yaw = yaw;
            pl.pitch = pitch;
        }
    }
    // Wait for both wall time and frames (software renderers can run at ~1 fps).
    // Cutscene holds wait on game time instead, so they land on story beats.
    let ready = if pos.is_none() && cap.index > 1 { cap.timer > 2.0 && cap.frames > 8 } else { cap.timer > settle && cap.frames > 12 };
    if ready && !cap.shot_taken {
        let path = format!("{}/{}.png", cap.dir, label);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        cap.shot_taken = true;
        cap.frames = 12;
    }
    if cap.shot_taken && cap.frames > 18 {
        cap.index += 1;
        cap.timer = 0.0;
        cap.frames = 0;
        cap.shot_taken = false;
    }
}
