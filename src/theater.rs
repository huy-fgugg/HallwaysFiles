//! Cutscene viewer (main menu → Cutscenes): replays any cutscene. It starts a
//! quiet run in the background (no lobby), puts you where the cutscene
//! happens, plays it, and returns to the Cutscenes list when it ends or is
//! skipped. The HUD stays hidden throughout.

use bevy::prelude::*;

use crate::cutscene::{ActiveCutscene, CutsceneEnded, PlayCutscene, ENDING, INTRO, THE_HOLLOW};
use crate::floor::{Floor, FloorRuntime};
use crate::player::Player;
use crate::state::{AppState, Overlay};

/// (id, title, description) for every cutscene in the viewer.
pub const CUTSCENES: [(&str, &str, &str); 3] = [
    (INTRO, "Arrival", "The lift descends to the first floor."),
    (THE_HOLLOW, "The Hollow", "Something wakes at the end of the corridor."),
    (ENDING, "The Way Out", "The exit lift, and what waits in the shaft."),
];

/// The cutscene the viewer is showing (None = normal play).
#[derive(Resource, Debug, Default)]
pub struct Theater {
    pub id: Option<String>,
    started: bool,
    wait: f32,
}

impl Theater {
    pub fn active(&self) -> bool {
        self.id.is_some()
    }

    pub fn request(&mut self, id: &str) {
        self.id = Some(id.to_string());
        self.started = false;
        self.wait = 0.0;
    }
}

/// Set when a viewer run ends, so the main menu reopens the Cutscenes list.
#[derive(Resource, Debug, Default)]
pub struct BackToCutscenes(pub bool);

pub struct TheaterPlugin;

impl Plugin for TheaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Theater>()
            .init_resource::<BackToCutscenes>()
            .add_systems(Update, auto_from_env.run_if(in_state(AppState::MainMenu)))
            .add_systems(Update, (stage, finish, hide_hud).chain().run_if(in_state(AppState::Playing)))
            .add_systems(OnEnter(AppState::MainMenu), reopen_list.after(crate::state::reset_overlay));
    }
}

fn reopen_list(mut t: ResMut<Theater>, mut back: ResMut<BackToCutscenes>, mut next: ResMut<NextState<Overlay>>) {
    *t = Theater::default();
    if std::mem::take(&mut back.0) {
        next.set(Overlay::Cutscenes);
    }
}

/// No HUD (timer, hotbar, health) while the viewer shows a cutscene.
fn hide_hud(theater: Res<Theater>, mut hide: ResMut<crate::ui::hud::HideHud>, mut was: Local<bool>) {
    let on = theater.active();
    if on != *was {
        *was = on;
        hide.0 = on;
    }
}

/// QA / shortcut: `HALLWAYS_THEATER=03_ending` starts straight into a cutscene.
fn auto_from_env(mut theater: ResMut<Theater>, mut next: ResMut<NextState<AppState>>, mut skip: ResMut<crate::lobby::SkipLobby>, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Ok(id) = std::env::var("HALLWAYS_THEATER") {
        *done = true;
        theater.request(&id);
        skip.0 = true;
        next.set(AppState::Playing);
    }
}

/// Places the player and starts the cutscene once the world is ready.
#[allow(clippy::too_many_arguments)]
fn stage(
    time: Res<Time<Real>>,
    mut theater: ResMut<Theater>,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    mut player: Query<(&mut Player, &mut Transform)>,
    mut play: MessageWriter<PlayCutscene>,
    mut anchor: ResMut<crate::ending::CutsceneAnchor>,
    mut run: ResMut<crate::run::RunState>,
) {
    let Some(id) = theater.id.clone() else { return };
    if theater.started {
        return;
    }
    theater.wait += time.delta_secs();
    // Viewer runs never hurt you.
    run.vitality = 100.0;
    let Ok((mut p, mut tf)) = player.single_mut() else { return };
    match id.as_str() {
        x if x == INTRO => {
            if theater.wait > 0.6 {
                play.write(PlayCutscene(INTRO.into()));
                theater.started = true;
            }
        }
        x if x == THE_HOLLOW => {
            // In the corridor where it happens (the corridor itself no longer
            // wakes it since the opening chase was removed, so play it here).
            tf.translation = Vec3::new(0.0, 0.0, -8.0);
            p.yaw = 0.0;
            if theater.wait > 0.6 {
                play.write(PlayCutscene(THE_HOLLOW.into()));
                theater.started = true;
            }
        }
        _ => {
            // The ending needs the final hall built around the player.
            let (Some(floor), Some(rt)) = (floor, rt) else { return };
            let Some(r) = floor.0.room(floor.0.last_room) else { return };
            tf.translation = Vec3::new(r.exit_x, 0.0, r.min.y + 2.5);
            p.yaw = 0.0;
            if rt.loaded.contains(&r.index) && theater.wait > 1.5 {
                crate::ending::start_ending(&mut anchor, &mut play, &floor, &rt);
                theater.started = true;
            }
        }
    }
}

/// Back to the main menu when the cutscene ends (or is skipped).
fn finish(
    mut theater: ResMut<Theater>,
    mut ended: MessageReader<CutsceneEnded>,
    active: Res<ActiveCutscene>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut exit: MessageWriter<AppExit>,
    mut back: ResMut<BackToCutscenes>,
) {
    let mut done = false;
    for e in ended.read() {
        if theater.active() && theater.started && Some(&e.id) == theater.id.as_ref() {
            done = true;
        }
    }
    let _ = active;
    if done {
        theater.id = None;
        next_overlay.set(Overlay::None);
        if std::env::var_os("HALLWAYS_THEATER").is_some() {
            exit.write(AppExit::Success);
        } else {
            back.0 = true;
            next_app.set(AppState::MainMenu);
        }
    }
}
