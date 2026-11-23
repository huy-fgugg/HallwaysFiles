//! Boarding the lift. Every fresh run starts in the boarding hall behind the
//! lift with its rear gate open. Step into the car: once everyone is aboard it
//! leaves after a short beat; if anyone is missing, a 20-second countdown
//! starts as soon as the first player boards, and when it runs out the lift
//! leaves without them. The host can also start it at once (Enter / gamepad A).
//!
//! Solo runs board alone. Online (up to 4 players) only the host's game
//! decides when to go; everyone departs together when the server relays the
//! host's Start, and anyone not inside the car is left behind.

use bevy::prelude::*;

use crate::cutscene::{PlayCutscene, INTRO};
use crate::player::Player;
use crate::state::{AppState, Overlay};
use crate::theme::UiTheme;
use crate::world::elevator::{in_car, ElevatorCommand, CAR_START};

/// Seconds before departure once everyone is aboard.
pub const ALL_ABOARD_COUNTDOWN: f32 = 5.0;
/// Seconds before the lift leaves without the players who haven't boarded.
pub const BOARDING_COUNTDOWN: f32 = 20.0;
/// Seconds after the gate closes before a left-behind player gets the news.
const LEFT_BEHIND_SECS: f32 = 2.5;

#[derive(Resource, Debug, Clone)]
pub struct LiftLobby {
    pub waiting: bool,
    /// Players in the run (you included).
    pub players_joined: u32,
    /// Players standing inside the car.
    pub aboard: u32,
    pub is_host: bool,
    /// Counts down once someone is aboard.
    pub countdown: Option<f32>,
    /// Start inside the car with the rear gate shut (retry, solo custom floor).
    pub start_in_car: bool,
    /// The lift left without you: seconds since.
    pub left_behind: Option<f32>,
}

impl Default for LiftLobby {
    fn default() -> Self {
        Self { waiting: false, players_joined: 1, aboard: 0, is_host: true, countdown: None, start_in_car: false, left_behind: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyTick {
    Wait,
    Depart,
}

impl LiftLobby {
    /// Pure boarding rule: the host may force a start; otherwise nobody aboard
    /// means wait, someone aboard starts a 20 s countdown, and everyone aboard
    /// cuts it to 5 s.
    pub fn tick(&mut self, dt: f32, force_start: bool) -> LobbyTick {
        if !self.waiting {
            return LobbyTick::Wait;
        }
        if force_start && self.is_host {
            return LobbyTick::Depart;
        }
        if self.aboard == 0 {
            self.countdown = None;
            return LobbyTick::Wait;
        }
        let limit = if self.aboard >= self.players_joined { ALL_ABOARD_COUNTDOWN } else { BOARDING_COUNTDOWN };
        let c = self.countdown.get_or_insert(limit);
        if *c > limit {
            *c = limit;
        }
        *c -= dt;
        if *c <= 0.0 { LobbyTick::Depart } else { LobbyTick::Wait }
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

fn begin(
    mut lobby: ResMut<LiftLobby>,
    skip: Res<SkipLobby>,
    preset: Res<crate::presets::ActivePreset>,
    session: Res<crate::net::NetSession>,
    mut lift: MessageWriter<ElevatorCommand>,
    mut play: MessageWriter<PlayCutscene>,
) {
    let capture = std::env::var_os("HALLWAYS_CAPTURE").is_some() || std::env::var_os("HALLWAYS_TRAILER").is_some();
    // A solo custom floor starts straight in its first room (see floor::custom_quick_start).
    let custom_solo = preset.0.is_some() && !session.online();
    let waiting = !(skip.0 || capture || custom_solo);
    // Scripted QA runs (autopilot, warps, click tests) start aboard.
    let qa = ["HALLWAYS_BOT", "HALLWAYS_WARP", "HALLWAYS_QA", "HALLWAYS_FAKE_TOUCH"].iter().any(|v| std::env::var_os(v).is_some());
    // QA: HALLWAYS_QA_CUTSCENES keeps the retry / custom-floor openings in scripted runs.
    let qa_scenes = std::env::var_os("HALLWAYS_QA_CUTSCENES").is_some();
    *lobby = LiftLobby { waiting, start_in_car: !waiting || qa, ..default() };
    if !waiting {
        lift.write(ElevatorCommand::Needle { to: 1.0, seconds: 0.0 });
        lift.write(ElevatorCommand::CloseRear { seconds: 0.0 });
        if skip.0 && !capture && !custom_solo && (!qa || qa_scenes) {
            // Retry: a short scene in the lift; it opens the gate itself.
            play.write(PlayCutscene(crate::cutscene::RETRY.into()));
        } else {
            // Capture / solo custom floor: the gate is simply open.
            lift.write(ElevatorCommand::OpenGate { seconds: 0.0 });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_lobby(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut lobby: ResMut<LiftLobby>,
    mut play: MessageWriter<PlayCutscene>,
    session: Res<crate::net::NetSession>,
    peers: Res<crate::net::Peers>,
    mut start: MessageWriter<crate::net::RequestStart>,
    mut started: MessageReader<crate::net::DescentStarted>,
    mut lift: MessageWriter<ElevatorCommand>,
    mut player: Query<(&mut Player, &mut Transform)>,
    (mut death, mut next_overlay, mut run): (ResMut<crate::hollow::DeathCause>, ResMut<NextState<Overlay>>, ResMut<crate::run::RunState>),
    mut asked: Local<bool>,
) {
    let Ok((mut pl, mut tf)) = player.single_mut() else { return };
    if lobby.start_in_car {
        lobby.start_in_car = false;
        tf.translation = CAR_START;
        pl.yaw = 0.0;
    }
    // Left behind: the news a moment after the gate shuts.
    if let Some(t) = lobby.left_behind {
        let t = t + time.delta_secs();
        lobby.left_behind = Some(t);
        if t >= LEFT_BEHIND_SECS && run.vitality > 0.0 {
            run.vitality = 0.0;
            *death = crate::hollow::DeathCause {
                title: "THE LIFT LEFT WITHOUT YOU".into(),
                cause: "You weren't in the lift when its gate closed.".into(),
                lesson: "When someone steps into the lift, a 20-second countdown starts. Get in before it runs out.".into(),
            };
            next_overlay.set(Overlay::Dead);
        }
        return;
    }
    let online = session.online();
    lobby.players_joined = if online { session.players.len().max(1) as u32 } else { 1 };
    lobby.is_host = !online || session.is_host();
    lobby.aboard = u32::from(in_car(tf.translation)) + peers.0.values().filter(|p| p.alive && in_car(Vec3::from_array(p.p))).count() as u32;
    // The room's started flag (not just the one-off message) so a guest who was
    // still loading when the host started still leaves with everyone.
    started.clear();
    let mut depart = online && session.started && lobby.waiting;
    // The host can send the lift down early, but only from inside it.
    let force = (keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) || pads.iter().any(|p| p.just_pressed(GamepadButton::South))) && in_car(tf.translation);
    if lobby.tick(time.delta_secs(), force) == LobbyTick::Depart {
        if online {
            // Ask the server; everyone (us included) leaves when it says Start.
            if lobby.is_host && !*asked {
                start.write(crate::net::RequestStart);
                *asked = true;
            }
        } else {
            depart = true;
        }
    }
    if !lobby.waiting {
        *asked = false;
    }
    if depart {
        lobby.waiting = false;
        lobby.countdown = None;
        *asked = false;
        lift.write(ElevatorCommand::CloseRear { seconds: 1.2 });
        if in_car(tf.translation) {
            // The descent is the intro cutscene (skippable by holding Space).
            play.write(PlayCutscene(INTRO.into()));
        } else {
            lobby.left_behind = Some(0.0);
        }
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
    session: Res<crate::net::NetSession>,
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
        Some(c) if lobby.aboard >= lobby.players_joined => format!("EVERYONE ABOARD   -   GOING DOWN IN {}", c.ceil().max(0.0) as u32),
        Some(c) => format!("IN THE LIFT {} / {}   -   LEAVING IN {}", lobby.aboard, lobby.players_joined, c.ceil().max(0.0) as u32),
        None => format!("STEP INTO THE LIFT   {} / {} ABOARD", lobby.aboard, lobby.players_joined),
    };
    for mut t in &mut texts.p0() {
        if t.0 != status {
            t.0 = status.clone();
        }
    }
    let code = if session.online() { format!("ROOM CODE  {}   -   ", session.code) } else { String::new() };
    let hint = format!(
        "{code}{}",
        if lobby.aboard > 0 && lobby.aboard < lobby.players_joined {
            "The lift leaves without anyone who isn't inside when the countdown ends"
        } else if lobby.is_host {
            "Walk into the lift  -  host: [Enter] / [A] to go now"
        } else {
            "Walk into the lift"
        }
    );
    for mut t in &mut texts.p1() {
        if t.0 != hint {
            t.0 = hint.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby(joined: u32, aboard: u32) -> LiftLobby {
        LiftLobby { waiting: true, players_joined: joined, aboard, ..default() }
    }

    fn run_until_depart(l: &mut LiftLobby, max_secs: f32) -> Option<f32> {
        let mut t = 0.0;
        while t < max_secs {
            t += 0.1;
            if l.tick(0.1, false) == LobbyTick::Depart {
                return Some(t);
            }
        }
        None
    }

    #[test]
    fn nobody_aboard_waits_forever() {
        let mut l = lobby(3, 0);
        assert_eq!(run_until_depart(&mut l, 600.0), None);
        assert!(l.countdown.is_none());
    }

    #[test]
    fn someone_aboard_leaves_after_twenty_seconds() {
        let mut l = lobby(3, 1);
        let t = run_until_depart(&mut l, 60.0).unwrap();
        assert!((t - BOARDING_COUNTDOWN).abs() < 0.3, "{t}");
    }

    #[test]
    fn everyone_aboard_cuts_it_short() {
        let mut l = lobby(3, 1);
        assert_eq!(run_until_depart(&mut l, 3.0), None);
        l.aboard = 3;
        let t = run_until_depart(&mut l, 60.0).unwrap();
        assert!(t <= ALL_ABOARD_COUNTDOWN + 0.2, "{t}");
        // Solo: stepping in is everyone.
        let mut solo = lobby(1, 1);
        assert!(run_until_depart(&mut solo, 60.0).unwrap() <= ALL_ABOARD_COUNTDOWN + 0.2);
    }

    #[test]
    fn stepping_out_cancels_and_host_can_force() {
        let mut l = lobby(2, 1);
        l.tick(1.0, false);
        assert!(l.countdown.is_some());
        l.aboard = 0;
        l.tick(0.1, false);
        assert!(l.countdown.is_none());
        assert_eq!(l.tick(0.1, true), LobbyTick::Depart);
        let mut guest = LiftLobby { is_host: false, ..lobby(2, 0) };
        assert_eq!(guest.tick(0.1, true), LobbyTick::Wait);
    }
}
