//! Application state machine.
//!
//! * [`AppState`] — top level: main menu vs. an active run.
//! * [`Overlay`] — modal overlays (pause, settings) that can sit on top.
//! * [`Screen`] — computed from the two; every UI screen is spawned on
//!   `OnEnter(Screen::X)` and tagged `DespawnOnExit(Screen::X)`, so exactly one
//!   menu screen exists at a time.

use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Playing,
    /// One-frame hop used by "Retry" to rebuild the level from scratch.
    Restarting,
}

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Overlay {
    #[default]
    None,
    Pause,
    Settings,
    Credits,
    Play,
    Custom,
    Cutscenes,
    Admin,
    Dead,
    Summary,
    Achievements,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    MainMenu,
    Hud,
    Pause,
    Settings,
    Credits,
    Play,
    Custom,
    Cutscenes,
    Admin,
    Death,
    Summary,
    Achievements,
}

impl ComputedStates for Screen {
    type SourceStates = (AppState, Overlay);

    fn compute((app, overlay): (AppState, Overlay)) -> Option<Self> {
        Some(match (app, overlay) {
            (AppState::Restarting, _) => return None,
            (_, Overlay::Settings) => Screen::Settings,
            (AppState::MainMenu, Overlay::Credits) => Screen::Credits,
            (AppState::MainMenu, Overlay::Play) => Screen::Play,
            (AppState::MainMenu, Overlay::Custom) => Screen::Custom,
            (AppState::MainMenu, Overlay::Cutscenes) => Screen::Cutscenes,
            (AppState::MainMenu, Overlay::Achievements) => Screen::Achievements,
            (AppState::MainMenu, _) => Screen::MainMenu,
            (AppState::Playing, Overlay::Pause) => Screen::Pause,
            (AppState::Playing, Overlay::Admin) => Screen::Admin,
            (AppState::Playing, Overlay::Dead) => Screen::Death,
            (AppState::Playing, Overlay::Summary) => Screen::Summary,
            (AppState::Playing, _) => Screen::Hud,
        })
    }
}

/// True while the player is actually in control (in a run, no overlay open,
/// no cutscene playing).
pub fn gameplay_active(
    app: Res<State<AppState>>,
    overlay: Res<State<Overlay>>,
    cutscene: Option<Res<crate::cutscene::ActiveCutscene>>,
) -> bool {
    *app.get() == AppState::Playing && *overlay.get() == Overlay::None && !cutscene.is_some_and(|c| c.is_playing())
}

/// True while a run is on screen and not paused/dead (cutscenes included).
pub fn world_running(app: Res<State<AppState>>, overlay: Res<State<Overlay>>) -> bool {
    *app.get() == AppState::Playing && *overlay.get() == Overlay::None
}

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_state::<Overlay>()
            .add_computed_state::<Screen>()
            .add_systems(OnEnter(Overlay::Pause), pause_time)
            .add_systems(OnEnter(Overlay::None), resume_time)
            .add_systems(OnEnter(AppState::MainMenu), reset_overlay)
            .add_systems(OnEnter(AppState::Restarting), restart);
    }
}

fn pause_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

fn restart(mut next_app: ResMut<NextState<AppState>>, mut next_overlay: ResMut<NextState<Overlay>>) {
    next_overlay.set(Overlay::None);
    next_app.set(AppState::Playing);
}

pub fn reset_overlay(mut next: ResMut<NextState<Overlay>>) {
    next.set(Overlay::None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_mapping() {
        assert_eq!(Screen::compute((AppState::MainMenu, Overlay::None)), Some(Screen::MainMenu));
        assert_eq!(Screen::compute((AppState::MainMenu, Overlay::Settings)), Some(Screen::Settings));
        assert_eq!(Screen::compute((AppState::Playing, Overlay::None)), Some(Screen::Hud));
        assert_eq!(Screen::compute((AppState::Playing, Overlay::Pause)), Some(Screen::Pause));
        assert_eq!(Screen::compute((AppState::Playing, Overlay::Settings)), Some(Screen::Settings));
        assert_eq!(Screen::compute((AppState::Playing, Overlay::Dead)), Some(Screen::Death));
        assert_eq!(Screen::compute((AppState::MainMenu, Overlay::Credits)), Some(Screen::Credits));
        assert_eq!(Screen::compute((AppState::Restarting, Overlay::None)), None);
    }
}
