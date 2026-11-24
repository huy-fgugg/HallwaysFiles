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
    /// Fragile modifier: 50% more damage.
    pub fragile: bool,
    /// Mode rule: damage multiplier (from the mode's health setting).
    pub damage_scale: f32,
    /// Mode rule: Lumens earned multiplier.
    pub lumen_scale: f32,
    /// Lumens already banked (a revived player who dies again isn't paid twice).
    pub banked: bool,
}

impl RunState {
    pub fn new() -> Self {
        Self { vitality: 100.0, damage_scale: 1.0, lumen_scale: 1.0, ..Default::default() }
    }

    /// Applies damage; returns true if this was fatal.
    pub fn damage(&mut self, amount: f32) -> bool {
        let amount = if self.fragile { amount * 1.5 } else { amount };
        let amount = amount * if self.damage_scale > 0.0 { self.damage_scale } else { 1.0 };
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
            .init_resource::<crate::presets::ModeRules>()
            .add_systems(OnEnter(AppState::Playing), |mut commands: Commands, profile: Res<crate::profile::Profile>, active: Res<crate::presets::ActivePreset>| {
                let rules = crate::presets::ModeRules::of(active.0.as_ref());
                commands.insert_resource(rules);
                commands.insert_resource(RunState { fragile: profile.has(crate::profile::Modifier::Fragile), damage_scale: rules.damage, lumen_scale: rules.lumens, ..RunState::new() });
                commands.insert_resource(CameraShake::default());
            })
            .add_systems(Update, tick_timer.run_if(gameplay_active));
    }
}

fn tick_timer(time: Res<Time>, mut run: ResMut<RunState>, lobby: Option<Res<crate::lobby::LiftLobby>>) {
    // The clock starts when the lift leaves, not while players gather.
    if lobby.is_some_and(|l| l.waiting) {
        return;
    }
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
