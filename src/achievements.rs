//! Achievements and pop-up notifications.
//!
//! * Achievements are saved in the profile. Unlocking one shows a pop-up in
//!   the top-right corner; the main menu's Achievements screen lists them all.
//! * Online, a pop-up says when a player joins or leaves the room.

use bevy::prelude::*;

use crate::floor::plan::Encounter;
use crate::floor::{Floor, FloorRuntime};
use crate::profile::{save_profile, Profile};
use crate::pursuit::{Pursuit, PursuitPhase};
use crate::run::RunState;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;
use crate::ui::focus::{spawn_button, MenuAction, MenuActivated};

pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub how: &'static str,
}

pub const ALL: [Achievement; 14] = [
    Achievement { id: "first_steps", name: "FIRST STEPS", how: "Walk through Door 002." },
    Achievement { id: "halfway", name: "HALFWAY DOWN", how: "Reach Door 050." },
    Achievement { id: "checkout", name: "CHECKOUT", how: "Open Door 100." },
    Achievement { id: "untouchable", name: "UNTOUCHABLE", how: "Open Door 100 without losing any vitality." },
    Achievement { id: "outrun", name: "OUTRUN", how: "Survive a chase by The Hollow." },
    Achievement { id: "twice_shy", name: "TWICE SHY", how: "Survive both of The Hollow's chases in one run." },
    Achievement { id: "closet_case", name: "CLOSET CASE", how: "Hide from The Surge." },
    Achievement { id: "still_as_stone", name: "STILL AS STONE", how: "Stay still through The Hush." },
    Achievement { id: "tiptoe", name: "TIPTOE", how: "Get past The Listener." },
    Achievement { id: "wrong_number", name: "WRONG NUMBER", how: "Open a fake door." },
    Achievement { id: "locksmith", name: "LOCKSMITH", how: "Find a key in a drawer." },
    Achievement { id: "left_behind", name: "LEFT BEHIND", how: "Miss the lift." },
    Achievement { id: "team_player", name: "TEAM PLAYER", how: "Play an online run with 3 or more players." },
    Achievement { id: "full_house", name: "FULL HOUSE", how: "Play an online run with a full room of 4 players." },
];

pub fn by_id(id: &str) -> Option<&'static Achievement> {
    ALL.iter().find(|a| a.id == id)
}

/// Unlock an achievement (ignored if it's already unlocked).
#[derive(Message, Debug, Clone, Copy)]
pub struct Unlock(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Achievement,
    Joined,
    Left,
}

/// A pop-up notification in the top-right corner.
#[derive(Message, Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
}

const TOAST_SECS: f32 = 4.5;

#[derive(Component)]
struct ToastStack;
#[derive(Component)]
struct ToastCard {
    t: f32,
}

pub struct AchievementsPlugin;

impl Plugin for AchievementsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Unlock>()
            .add_message::<Toast>()
            .add_systems(Startup, spawn_stack)
            .add_systems(Update, (detect.run_if(in_state(AppState::Playing)).run_if(real_run), join_toasts, apply_unlocks, show_toasts, animate_toasts).chain())
            .add_systems(OnEnter(Overlay::Summary), |run: Res<RunState>, theater: Res<crate::theater::Theater>, mut out: MessageWriter<Unlock>| {
                if theater.active() {
                    return;
                }
                out.write(Unlock("checkout"));
                if run.damage_taken <= 0.0 {
                    out.write(Unlock("untouchable"));
                }
            })
            .add_systems(OnEnter(Screen::Achievements), spawn_screen)
            .add_systems(Update, screen_actions.run_if(in_state(Screen::Achievements)));
    }
}

/// Achievements only count in real runs, not in the cutscene viewer, the
/// trailer recorder or scripted QA warps.
fn real_run(theater: Res<crate::theater::Theater>) -> bool {
    !theater.active() && std::env::var_os("HALLWAYS_TRAILER").is_none() && std::env::var_os("HALLWAYS_WARP").is_none() && std::env::var_os("HALLWAYS_CAPTURE").is_none()
}

/// Watches the run for achievement moments.
#[allow(clippy::too_many_arguments)]
fn detect(
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    pursuit: Res<Pursuit>,
    lobby: Res<crate::lobby::LiftLobby>,
    session: Res<crate::net::NetSession>,
    mut out: MessageWriter<Unlock>,
    mut was_sinking: Local<bool>,
    mut survived: Local<(u32, u64)>,
) {
    if run.current_room >= 2 {
        out.write(Unlock("first_steps"));
    }
    if run.current_room >= 50 {
        out.write(Unlock("halfway"));
    }
    if run.surges_survived >= 1 {
        out.write(Unlock("closet_case"));
    }
    if run.mimics_fooled >= 1 {
        out.write(Unlock("wrong_number"));
    }
    if rt.as_ref().is_some_and(|rt| !rt.keys.is_empty()) {
        out.write(Unlock("locksmith"));
    }
    if lobby.left_behind.is_some() {
        out.write(Unlock("left_behind"));
    }
    if session.online() && session.players.len() >= 3 {
        out.write(Unlock("team_player"));
    }
    if session.online() && session.players.len() >= usize::from(crate::net::proto::MAX_PLAYERS) {
        out.write(Unlock("full_house"));
    }
    // Past a Listener's garden and still alive.
    if let Some(f) = floor.as_ref() {
        let prev = run.current_room.saturating_sub(1);
        let met = rt.as_ref().is_some_and(|rt| rt.encounters_done.contains(&prev));
        if run.vitality > 0.0 && met && f.0.room(prev).is_some_and(|r| r.encounter == Some(Encounter::Listener)) {
            out.write(Unlock("tiptoe"));
        }
        // Chases survived this run (reset when a new floor starts).
        if survived.1 != f.0.seed {
            *survived = (0, f.0.seed);
        }
    }
    let sinking = matches!(pursuit.phase, PursuitPhase::Sinking(_));
    if *was_sinking && pursuit.phase == PursuitPhase::Done {
        survived.0 += 1;
        out.write(Unlock("outrun"));
        if survived.0 >= 2 {
            out.write(Unlock("twice_shy"));
        }
    }
    *was_sinking = sinking;
}

/// Online: a pop-up when someone joins or leaves the room.
fn join_toasts(session: Res<crate::net::NetSession>, mut seen: Local<Option<Vec<(u8, String)>>>, mut out: MessageWriter<Toast>) {
    if !session.online() {
        *seen = None;
        return;
    }
    let now: Vec<(u8, String)> = session.players.iter().map(|p| (p.id, p.name.clone())).collect();
    if let Some(before) = seen.as_ref() {
        for (id, name) in &now {
            if *id != session.my_id && !before.iter().any(|(b, _)| b == id) {
                out.write(Toast { kind: ToastKind::Joined, title: "PLAYER JOINED".into(), body: format!("{name} joined the room ({} / {})", now.len(), session.max.max(1)) });
            }
        }
        for (id, name) in before {
            if !now.iter().any(|(n, _)| n == id) {
                out.write(Toast { kind: ToastKind::Left, title: "PLAYER LEFT".into(), body: format!("{name} left the room") });
            }
        }
    }
    *seen = Some(now);
}

fn apply_unlocks(mut unlocks: MessageReader<Unlock>, mut profile: ResMut<Profile>, mut toasts: MessageWriter<Toast>) {
    let mut changed = false;
    for Unlock(id) in unlocks.read() {
        if profile.achievements.iter().any(|a| a == id) {
            continue;
        }
        let Some(a) = by_id(id) else { continue };
        profile.achievements.push(id.to_string());
        changed = true;
        info!("Achievement unlocked: {}", a.name);
        toasts.write(Toast { kind: ToastKind::Achievement, title: format!("ACHIEVEMENT  -  {}", a.name), body: a.how.to_string() });
    }
    if changed {
        save_profile(&profile);
    }
}

fn spawn_stack(mut commands: Commands) {
    commands.spawn((
        Node { position_type: PositionType::Absolute, top: px(64), right: px(16), flex_direction: FlexDirection::Column, row_gap: px(8), align_items: AlignItems::End, ..default() },
        GlobalZIndex(250),
        Pickable::IGNORE,
        crate::theme::PassThrough,
        ToastStack,
        Name::new("Toasts"),
    ));
}

fn show_toasts(mut commands: Commands, mut toasts: MessageReader<Toast>, stack: Query<Entity, With<ToastStack>>, theme: Res<UiTheme>) {
    let Ok(stack) = stack.single() else { return };
    for toast in toasts.read() {
        let (accent, icon) = match toast.kind {
            ToastKind::Achievement => (Color::srgb(1.0, 0.8, 0.3), "*"),
            ToastKind::Joined => (theme.interact, "+"),
            ToastKind::Left => (theme.danger, "-"),
        };
        let card = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(12),
                    padding: UiRect::axes(px(14), px(10)),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(theme.radius)),
                    margin: UiRect::right(px(-420)),
                    max_width: px(400),
                    ..default()
                },
                BackgroundColor(theme.panel.with_alpha(0.94)),
                BorderColor::all(accent),
                Pickable::IGNORE,
                ToastCard { t: 0.0 },
                ChildOf(stack),
            ))
            .id();
        commands.entity(card).with_children(|c| {
            c.spawn((Text::new(icon), theme.font(true, 30.0), TextColor(accent), Pickable::IGNORE));
            c.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }, Pickable::IGNORE)).with_children(|col| {
                col.spawn((Text::new(toast.title.clone()), theme.font(true, theme.size_small), TextColor(accent), Pickable::IGNORE));
                col.spawn((Text::new(toast.body.clone()), theme.font(false, theme.size_small), TextColor(theme.text), Pickable::IGNORE));
            });
        });
    }
}

/// Slides in from the right, holds, then slides back out.
fn animate_toasts(mut commands: Commands, time: Res<Time<Real>>, mut cards: Query<(Entity, &mut ToastCard, &mut Node)>) {
    for (e, mut card, mut node) in &mut cards {
        card.t += time.delta_secs().min(0.1);
        let k = toast_offset(card.t);
        node.margin.right = px(-420.0 * k);
        if card.t >= TOAST_SECS {
            commands.entity(e).try_despawn();
        }
    }
}

/// 1 = fully off-screen to the right, 0 = in place.
pub fn toast_offset(t: f32) -> f32 {
    let ease = |x: f32| 1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3);
    if t < 0.35 {
        1.0 - ease(t / 0.35)
    } else if t < TOAST_SECS - 0.4 {
        0.0
    } else {
        ease((t - (TOAST_SECS - 0.4)) / 0.4)
    }
}

fn spawn_screen(mut commands: Commands, theme: Res<UiTheme>, profile: Res<Profile>) {
    let done = ALL.iter().filter(|a| profile.achievements.iter().any(|x| x == a.id)).count();
    commands
        .spawn((theme.screen_root(false), GlobalZIndex(20), DespawnOnExit(Screen::Achievements), Name::new("Achievements")))
        .with_children(|root| {
            root.spawn(theme.heading_text("ACHIEVEMENTS"));
            root.spawn(theme.dim_text(format!("{done} / {} unlocked", ALL.len())));
            root.spawn(theme.panel(700.0)).with_children(|panel| {
                for a in &ALL {
                    let got = profile.achievements.iter().any(|x| x == a.id);
                    panel.spawn(Node { column_gap: px(14), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        row.spawn((Text::new(if got { "*" } else { "-" }), theme.font(true, theme.size_body), TextColor(if got { Color::srgb(1.0, 0.8, 0.3) } else { theme.text_dim })));
                        row.spawn((Text::new(a.name), theme.font(true, theme.size_small), TextColor(if got { theme.text } else { theme.text_dim }), Node { width: px(200), ..default() }));
                        row.spawn((Text::new(a.how), theme.font(false, theme.size_small), TextColor(theme.text_dim)));
                    });
                }
            });
            spawn_button(root, &theme, "Back", MenuAction::CloseAchievements, 0, true);
        });
}

fn screen_actions(mut reader: MessageReader<MenuActivated>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut next: ResMut<NextState<Overlay>>) {
    let back = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::East));
    if back || reader.read().any(|MenuActivated(a)| *a == MenuAction::CloseAchievements) {
        next.set(Overlay::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn achievements_have_unique_ids() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(ALL.iter().skip(i + 1).all(|b| b.id != a.id), "{}", a.id);
            assert!(by_id(a.id).is_some());
        }
    }

    #[test]
    fn toasts_slide_in_and_out() {
        assert_eq!(toast_offset(0.0), 1.0);
        assert_eq!(toast_offset(1.0), 0.0);
        assert!(toast_offset(TOAST_SECS) > 0.99);
    }
}
