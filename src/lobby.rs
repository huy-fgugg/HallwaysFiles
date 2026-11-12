//! The elevator lobby: every fresh run starts inside the lift with its own
//! looping music while players gather. When everyone has joined, a countdown
//! starts; the host can force-start at any time (Enter / gamepad A).
//!
//! Multiplayer isn't implemented yet, so the party is always 1 / 1. The
//! lobby is written so a future networking layer only has to update
//! `players_joined` / `players_expected` and `is_host`.

use bevy::prelude::*;

use crate::cutscene::{PlayCutscene, INTRO};
use crate::state::AppState;
use crate::theme::UiTheme;
use crate::world::elevator::ElevatorCommand;

/// Seconds of countdown once every expected player is in the lift.
pub const ALL_JOINED_COUNTDOWN: f32 = 15.0;

#[derive(Resource, Debug, Clone)]
pub struct LiftLobby {
    pub waiting: bool,
    pub players_joined: u32,
    pub players_expected: u32,
    pub is_host: bool,
    /// Counts down once everyone has joined.
    pub countdown: Option<f32>,
}

impl Default for LiftLobby {
    fn default() -> Self {
        Self { waiting: false, players_joined: 1, players_expected: 1, is_host: true, countdown: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyTick {
    Wait,
    Depart,
}

impl LiftLobby {
    /// Pure lobby rule: depart on host force-start, or when everyone has joined
    /// and the countdown has run out.
    pub fn tick(&mut self, dt: f32, force_start: bool) -> LobbyTick {
        if !self.waiting {
            return LobbyTick::Wait;
        }
        if force_start && self.is_host {
            return LobbyTick::Depart;
        }
        if self.players_joined >= self.players_expected {
            let c = self.countdown.get_or_insert(ALL_JOINED_COUNTDOWN);
            *c -= dt;
            if *c <= 0.0 {
                return LobbyTick::Depart;
            }
        } else {
            self.countdown = None;
        }
        LobbyTick::Wait
    }
}

/// Set by "Retry" so the restarted run skips the lobby and ride.
#[derive(Resource, Debug, Default)]
pub struct SkipLobby(pub bool);

#[derive(Component)]
struct LobbyPanel;
#[derive(Component)]
struct LobbyStatus;
#[derive(Component)]
struct LobbyHint;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LiftLobby>()
            .init_resource::<SkipLobby>()
            .add_systems(OnEnter(AppState::MainMenu), |mut s: ResMut<SkipLobby>| s.0 = false)
            .add_systems(OnEnter(AppState::Restarting), |mut s: ResMut<SkipLobby>| s.0 = true)
            .add_systems(OnEnter(AppState::Playing), (begin, spawn_panel))
            .add_systems(Update, (run_lobby, update_panel).chain().run_if(in_state(AppState::Playing)));
    }
}

fn begin(mut lobby: ResMut<LiftLobby>, skip: Res<SkipLobby>, mut lift: MessageWriter<ElevatorCommand>) {
    let capture = std::env::var_os("HALLWAYS_CAPTURE").is_some();
    *lobby = LiftLobby { waiting: !(skip.0 || capture), ..default() };
    if !lobby.waiting {
        // Retry / capture: you're already on the floor; the gate is open.
        lift.write(ElevatorCommand::Needle { to: 1.0, seconds: 0.0 });
        lift.write(ElevatorCommand::OpenGate { seconds: 0.0 });
    }
}

fn run_lobby(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut lobby: ResMut<LiftLobby>,
    mut play: MessageWriter<PlayCutscene>,
) {
    let force = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) || pads.iter().any(|p| p.just_pressed(GamepadButton::South));
    if lobby.tick(time.delta_secs(), force) == LobbyTick::Depart {
        lobby.waiting = false;
        lobby.countdown = None;
        // The descent is the intro cutscene (skippable by holding Space).
        play.write(PlayCutscene(INTRO.into()));
    }
}

fn spawn_panel(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, top: px(28), width: percent(100), justify_content: JustifyContent::Center, ..default() },
            GlobalZIndex(12),
            Pickable::IGNORE,
            Visibility::Hidden,
            LobbyPanel,
            DespawnOnExit(AppState::Playing),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(26), px(14)),
                    row_gap: px(6),
                    border: UiRect::all(px(theme.border)),
                    border_radius: BorderRadius::all(px(theme.radius)),
                    ..default()
                },
                BackgroundColor(theme.panel.with_alpha(0.88)),
                BorderColor::all(theme.interact),
            ))
            .with_children(|p| {
                p.spawn((Text::new("THE LIFT"), theme.font(true, theme.size_small), TextColor(theme.interact)));
                p.spawn((Text::new(""), theme.font(true, theme.size_heading), TextColor(theme.text), LobbyStatus));
                p.spawn((Text::new(""), theme.font(false, theme.size_small), TextColor(theme.text_dim), LobbyHint));
            });
        });
}

#[allow(clippy::type_complexity)]
fn update_panel(
    lobby: Res<LiftLobby>,
    mut panel: Query<&mut Visibility, With<LobbyPanel>>,
    mut texts: ParamSet<(Query<&mut Text, With<LobbyStatus>>, Query<&mut Text, With<LobbyHint>>)>,
) {
    for mut v in &mut panel {
        let want = if lobby.waiting { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !lobby.waiting {
        return;
    }
    let status = match lobby.countdown {
        Some(c) => format!("PLAYERS {} / {}   -   DESCENDING IN {}", lobby.players_joined, lobby.players_expected, c.ceil().max(0.0) as u32),
        None => format!("WAITING FOR PLAYERS   {} / {}", lobby.players_joined, lobby.players_expected),
    };
    for mut t in &mut texts.p0() {
        if t.0 != status {
            t.0 = status.clone();
        }
    }
    let hint = if lobby.is_host { "Press [Enter] / [A] to start now  -  look around while you wait" } else { "Waiting for the host to start" };
    for mut t in &mut texts.p1() {
        if t.0 != hint {
            t.0 = hint.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_can_force_start() {
        let mut l = LiftLobby { waiting: true, players_joined: 1, players_expected: 4, ..default() };
        assert_eq!(l.tick(0.1, false), LobbyTick::Wait);
        assert_eq!(l.tick(0.1, true), LobbyTick::Depart);
    }

    #[test]
    fn non_host_cannot_force_start() {
        let mut l = LiftLobby { waiting: true, players_joined: 1, players_expected: 4, is_host: false, ..default() };
        assert_eq!(l.tick(0.1, true), LobbyTick::Wait);
    }

    #[test]
    fn waits_for_everyone_then_counts_down() {
        let mut l = LiftLobby { waiting: true, players_joined: 2, players_expected: 3, ..default() };
        for _ in 0..1000 {
            assert_eq!(l.tick(0.1, false), LobbyTick::Wait, "must keep looping until all players join");
        }
        assert!(l.countdown.is_none());
        l.players_joined = 3;
        let mut departed = false;
        for _ in 0..((ALL_JOINED_COUNTDOWN / 0.1) as usize + 2) {
            if l.tick(0.1, false) == LobbyTick::Depart {
                departed = true;
                break;
            }
        }
        assert!(departed);
    }

    #[test]
    fn someone_leaving_cancels_countdown() {
        let mut l = LiftLobby { waiting: true, players_joined: 2, players_expected: 2, ..default() };
        l.tick(1.0, false);
        assert!(l.countdown.is_some());
        l.players_joined = 1;
        l.tick(0.1, false);
        assert!(l.countdown.is_none());
    }
}
