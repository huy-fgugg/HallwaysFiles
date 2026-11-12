//! Scripted screenshot tour for visual QA, enabled by the `HALLWAYS_CAPTURE`
//! environment variable (value = output folder). Visits the main menu, then a
//! set of viewpoints in the test level, saves PNGs and exits. Inert otherwise.

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};

use crate::player::Player;
use crate::state::AppState;

/// (label, feet position, yaw, pitch). `None` = main menu for the first
/// shot, and "hold position, keep the clock running" afterwards (used to
/// photograph cutscenes mid-play).
type Shot = (&'static str, Option<Vec3>, f32, f32);

const SHOTS: &[Shot] = &[
    ("01_main_menu", None, 0.0, 0.0),
    ("02_spawn_view", Some(Vec3::new(0.0, 0.0, 3.4)), 0.0, -0.05),
    ("02b_inside_lift", Some(Vec3::new(0.0, 0.0, 6.9)), 0.0, 0.05),
    ("02c_lift_from_room", Some(Vec3::new(0.4, 0.0, 1.6)), std::f32::consts::PI, 0.05),
    ("03_drawer_cabinet", Some(Vec3::new(-2.2, 0.0, -1.6)), 1.5708, -0.25),
    ("04_wardrobe", Some(Vec3::new(1.4, 0.0, 1.4)), -1.5708, 0.0),
    ("05_door_closeup", Some(Vec3::new(0.3, 0.0, -3.6)), 0.0, 0.05),
    ("06_corner_chair", Some(Vec3::new(0.5, 0.0, 1.0)), 2.3, -0.15),
    ("07_commode", Some(Vec3::new(2.4, 0.0, -2.9)), 0.0, -0.3),
    ("08_corridor", Some(Vec3::new(0.0, 0.0, -6.0)), 0.0, 0.0),
    // Walking past z = -7.5 triggers The Hollow cutscene; then hold and shoot.
    ("09_hollow_trigger", Some(Vec3::new(0.0, 0.0, -7.8)), 0.0, 0.0),
    ("10_hollow_pool", None, 0.0, 0.0),
    ("11_hollow_rising", None, 0.0, 0.0),
    ("12_hollow_mask", None, 0.0, 0.0),
    ("13_hollow_end", None, 0.0, 0.0),
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
) {
    cap.timer += time.delta_secs();
    cap.frames += 1;
    let Some(&(label, pos, yaw, pitch)) = SHOTS.get(cap.index) else {
        if cap.timer > 2.0 {
            exit.write(AppExit::Success);
        }
        return;
    };
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
    let ready = if pos.is_none() && cap.index > 0 { cap.timer > 2.0 && cap.frames > 8 } else { cap.timer > settle && cap.frames > 12 };
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
