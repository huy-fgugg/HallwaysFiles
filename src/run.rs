//! Per-run state shared by gameplay and HUD: room counter, currency, timer,
//! subtitles and camera shake requests.

use bevy::prelude::*;

use crate::state::{gameplay_active, AppState};

#[derive(Resource, Debug, Clone, Default)]
pub struct RunState {
    pub current_room: u32,
    pub lumens: u32,
    pub elapsed: f32,
    pub vitality: f32,
    pub damage_taken: f32,
    pub surges_survived: u32,
    pub mimics_fooled: u32,
    /// Highest room reached.
    pub best_room: u32,
}

impl RunState {
    pub fn new() -> Self {
        Self { vitality: 100.0, ..Default::default() }
    }

    /// Applies damage; returns true if this was fatal.
    pub fn damage(&mut self, amount: f32) -> bool {
        self.vitality = (self.vitality - amount).max(0.0);
        self.damage_taken += amount;
        self.vitality <= 0.0
    }

    pub fn timer_text(&self) -> String {
        let secs = self.elapsed.max(0.0) as u32;
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

/// A line of caption text. Every entity cue and important sound sends one.
#[derive(Message, Debug, Clone)]
pub struct Subtitle {
    pub text: String,
    pub seconds: f32,
    pub danger: bool,
}

impl Subtitle {
    pub fn info(text: impl Into<String>) -> Self {
        Self { text: text.into(), seconds: 2.5, danger: false }
    }
    pub fn danger(text: impl Into<String>) -> Self {
        Self { text: text.into(), seconds: 3.0, danger: true }
    }
}

/// Screen-shake "trauma" in 0..1; decays over time. Scaled by spooky intensity.
#[derive(Resource, Debug, Default)]
pub struct CameraShake {
    pub trauma: f32,
}

impl CameraShake {
    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }
}

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Subtitle>()
            .init_resource::<RunState>()
            .init_resource::<CameraShake>()
            .add_systems(OnEnter(AppState::Playing), |mut commands: Commands| {
                commands.insert_resource(RunState::new());
                commands.insert_resource(CameraShake::default());
            })
            .add_systems(Update, tick_timer.run_if(gameplay_active));
    }
}

fn tick_timer(time: Res<Time>, mut run: ResMut<RunState>) {
    run.elapsed += time.delta_secs();
    run.best_room = run.best_room.max(run.current_room);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_formatting() {
        let mut r = RunState::new();
        r.elapsed = 125.7;
        assert_eq!(r.timer_text(), "02:05");
    }

    #[test]
    fn damage_is_fatal_at_zero() {
        let mut r = RunState::new();
        assert!(!r.damage(40.0));
        assert!(!r.damage(40.0));
        assert!(r.damage(40.0));
        assert_eq!(r.vitality, 0.0);
        assert_eq!(r.damage_taken, 120.0);
    }

    #[test]
    fn shake_is_capped() {
        let mut s = CameraShake::default();
        s.add(0.7);
        s.add(0.7);
        assert_eq!(s.trauma, 1.0);
    }
}
