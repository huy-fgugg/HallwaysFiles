//! Main menu and pause menu.

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused},
};

use super::focus::{spawn_button, MenuAction, MenuActivated};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuClock>()
            .add_systems(OnEnter(Screen::MainMenu), (spawn_main_menu, |mut c: ResMut<MenuClock>| c.0 = 0.0))
            .add_systems(OnEnter(Screen::Credits), spawn_credits)
            .add_systems(OnEnter(Screen::Death), spawn_death_screen)
            .add_systems(OnEnter(Screen::Pause), spawn_pause_menu)
            .add_systems(
                Update,
                (
                    main_menu_actions.run_if(in_state(Screen::MainMenu)),
                    pause_actions.run_if(in_state(Screen::Pause)),
                    open_pause.run_if(in_state(Screen::Hud)),
                    close_pause.run_if(in_state(Screen::Pause)),
                    pause_on_focus_lost.run_if(in_state(Screen::Hud)),
                    manage_cursor,
                    animate_main_menu.run_if(in_state(Screen::MainMenu)),
                    credits_actions.run_if(in_state(Screen::Credits)),
                    death_actions.run_if(in_state(Screen::Death)),
                ),
            );
    }
}

/// Seconds since the main menu opened (drives the intro animation).
#[derive(Resource, Default)]
struct MenuClock(f32);

/// Fades text in after `delay` seconds.
#[derive(Component)]
struct FadeIn {
    delay: f32,
    base: Color,
}

/// Slides a node in from the left after `delay` seconds.
#[derive(Component)]
struct SlideIn(f32);

/// The title flickers like a failing bulb now and then.
#[derive(Component)]
struct TitleFlicker;

#[derive(Component)]
struct PulseLine;

fn spawn_main_menu(mut commands: Commands, theme: Res<UiTheme>, seed: Res<RunSeed>) {
    commands
        .spawn((
            Node { width: percent(100), height: percent(100), ..default() },
            DespawnOnExit(Screen::MainMenu),
            Name::new("Main menu"),
        ))
        .with_children(|root| {
            // Left-side shade so text stays readable over the animated hallway.
            root.spawn((
                Node {
                    width: percent(46),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::left(percent(7)),
                    row_gap: px(theme.gap),
                    ..default()
                },
                BackgroundColor(theme.bg.with_alpha(0.72)),
            ))
            .with_children(|col| {
                col.spawn((theme.title_text("HALLWAYS"), TitleFlicker, FadeIn { delay: 0.3, base: theme.text }));
                col.spawn((
                    Node { width: px(420), height: px(3), margin: UiRect::vertical(px(6)), ..default() },
                    BackgroundColor(theme.danger),
                    PulseLine,
                ));
                col.spawn((
                    theme.dim_text("Every door is numbered. Not every number is honest."),
                    FadeIn { delay: 1.2, base: theme.text_dim },
                    Node { margin: UiRect::bottom(px(34)), ..default() },
                ));
                for (i, (label, action)) in [
                    ("Start Run", MenuAction::StartRun),
                    ("Settings", MenuAction::OpenSettings),
                    ("Credits", MenuAction::OpenCredits),
                    ("Quit", MenuAction::Quit),
                ]
                .into_iter()
                .enumerate()
                {
                    let id = spawn_button(col, &theme, label, action, i as i32, i == 0);
                    col.commands().entity(id).insert(SlideIn(1.4 + i as f32 * 0.12));
                }
            });
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(18),
                left: px(24),
                right: px(24),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|footer| {
                footer.spawn(theme.dim_text(format!("SEED {}", seed.display())));
                footer.spawn(theme.dim_text(format!("PROTOTYPE v{}", env!("CARGO_PKG_VERSION"))));
            });
        });
}

#[allow(clippy::type_complexity)]
fn animate_main_menu(
    time: Res<Time<Real>>,
    settings: Res<crate::settings::Settings>,
    mut clock: ResMut<MenuClock>,
    mut fades: Query<(&FadeIn, &mut TextColor, Option<&TitleFlicker>)>,
    mut slides: Query<(&SlideIn, &mut Node), Without<PulseLine>>,
    mut lines: Query<(&mut BackgroundColor, &mut Node), With<PulseLine>>,
) {
    clock.0 += time.delta_secs();
    let t = clock.0;
    for (f, mut color, flicker) in &mut fades {
        let mut a = ((t - f.delay) / 1.2).clamp(0.0, 1.0);
        if flicker.is_some() && !settings.reduce_flashing {
            // Brief dips every few seconds, like a dying bulb.
            let phase = (t * 0.45).fract();
            if phase > 0.9 && ((t * 31.0).sin() > 0.2) {
                a *= 0.35;
            }
        }
        color.0 = f.base.with_alpha(a);
    }
    for (s, mut node) in &mut slides {
        let k = ((t - s.0) / 0.5).clamp(0.0, 1.0);
        let k = 1.0 - (1.0 - k).powi(3);
        node.left = px(-60.0 * (1.0 - k));
    }
    for (mut bg, mut node) in &mut lines {
        let grow = ((t - 0.6) / 1.0).clamp(0.0, 1.0);
        node.width = px(420.0 * (1.0 - (1.0 - grow).powi(3)));
        let pulse = 0.65 + 0.35 * (t * 1.6).sin();
        bg.0 = crate::theme::hex(0xE05A4E).with_alpha(pulse);
    }
}

fn main_menu_actions(
    mut reader: MessageReader<MenuActivated>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut exit: MessageWriter<AppExit>,
) {
    for MenuActivated(action) in reader.read() {
        match action {
            MenuAction::StartRun => next_app.set(AppState::Playing),
            MenuAction::OpenSettings => next_overlay.set(Overlay::Settings),
            MenuAction::OpenCredits => next_overlay.set(Overlay::Credits),
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

fn spawn_pause_menu(mut commands: Commands, theme: Res<UiTheme>, seed: Res<RunSeed>) {
    commands
        .spawn((theme.screen_root(false), GlobalZIndex(10), DespawnOnExit(Screen::Pause), Name::new("Pause menu")))
        .with_children(|root| {
            root.spawn(theme.panel(420.0)).with_children(|panel| {
                panel.spawn((theme.heading_text("PAUSED"), Node { align_self: AlignSelf::Center, ..default() }));
                panel.spawn((
                    theme.dim_text(format!("Seed {}", seed.display())),
                    Node { align_self: AlignSelf::Center, margin: UiRect::bottom(px(8)), ..default() },
                ));
                spawn_button(panel, &theme, "Resume", MenuAction::Resume, 0, true);
                spawn_button(panel, &theme, "Settings", MenuAction::OpenSettings, 1, false);
                spawn_button(panel, &theme, "Abandon Run", MenuAction::AbandonRun, 2, false);
            });
        });
}

fn pause_actions(
    mut reader: MessageReader<MenuActivated>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    for MenuActivated(action) in reader.read() {
        match action {
            MenuAction::Resume => next_overlay.set(Overlay::None),
            MenuAction::OpenSettings => next_overlay.set(Overlay::Settings),
            MenuAction::AbandonRun => {
                next_overlay.set(Overlay::None);
                next_app.set(AppState::MainMenu);
            }
            _ => {}
        }
    }
}

fn pause_pressed(keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>) -> bool {
    keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::Start))
}

fn open_pause(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut next: ResMut<NextState<Overlay>>) {
    if pause_pressed(&keys, &pads) {
        next.set(Overlay::Pause);
    }
}

fn close_pause(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut next: ResMut<NextState<Overlay>>) {
    if pause_pressed(&keys, &pads) || pads.iter().any(|p| p.just_pressed(GamepadButton::East)) {
        next.set(Overlay::None);
    }
}

/// Alt-tabbing out of a run pauses it, so nothing can kill you while away.
fn pause_on_focus_lost(mut reader: MessageReader<WindowFocused>, mut next: ResMut<NextState<Overlay>>) {
    for ev in reader.read() {
        if !ev.focused {
            next.set(Overlay::Pause);
        }
    }
}

/// Lock and hide the cursor only while the player is in control.
fn manage_cursor(
    screen: Option<Res<State<Screen>>>,
    mut q: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((window, mut cursor)) = q.single_mut() else { return };
    let in_control = screen.is_some_and(|s| *s.get() == Screen::Hud) && window.focused;
    let (grab, visible) = if in_control { (CursorGrabMode::Locked, false) } else { (CursorGrabMode::None, true) };
    if cursor.grab_mode != grab {
        cursor.grab_mode = grab;
    }
    if cursor.visible != visible {
        cursor.visible = visible;
    }
}

/// People and projects whose work is in the game. Third-party asset authors
/// come from Poly Haven's API; music credits are read from
/// `assets/music/CREDITS.txt` so drop-in tracks can be credited without code.
pub const CREDITS: &[(&str, &[&str])] = &[
    ("HALLWAYS", &["Design, code, models built from primitives, procedural score", "Made with Bevy (MIT / Apache-2.0)"]),
    (
        "3D MODELS  -  Poly Haven, CC0",
        &[
            "Gothic Cabinet, Gothic Commode, Arm Chair, Classic Nightstand: Kirill Sannikov",
            "Drawer Cabinet, Industrial Wall Sconce: Ulan Cabanilla",
            "Picture Frames, Modern Ceiling Lamp, Potted Plant: James Ray Cock",
            "Mantel Clock: Yann Kervran, Rico Cilliers",
        ],
    ),
    (
        "TEXTURES  -  Poly Haven, CC0",
        &[
            "Decrepit Wallpaper: Rob Tuytel     Old Wood Floor: Guillaume Monsergent",
            "Dark Wooden Planks, Painted Plaster Wall: Amal Kumar",
            "Peeling Painted Wall: Dimitrios Savva     Dirty Carpet: Rohit Seervi",
            "Wood Table 001: Dimitrios Savva, Rico Cilliers",
        ],
    ),
    ("FONT", &["IBM Plex Mono by IBM (SIL Open Font License 1.1)"]),
    ("INSPIRATION", &["Inspired by room-by-room horror games such as DOORS (LSPLASH).", "HALLWAYS is an independent project, not affiliated with or endorsed by them."]),
];

fn music_credit_lines() -> Vec<String> {
    let path = crate::music::asset_root().join("music").join("CREDITS.txt");
    std::fs::read_to_string(path)
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect())
        .unwrap_or_default()
}

fn spawn_credits(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((theme.screen_root(false), GlobalZIndex(20), DespawnOnExit(Screen::Credits), Name::new("Credits")))
        .with_children(|root| {
            root.spawn(theme.heading_text("CREDITS"));
            root.spawn(theme.panel(900.0)).with_children(|panel| {
                let mut section = |title: &str, lines: Vec<String>| {
                    panel.spawn((Text::new(title), theme.font(true, theme.size_small), TextColor(theme.interact)));
                    for l in lines {
                        panel.spawn((Text::new(l), theme.font(false, theme.size_small), TextColor(theme.text)));
                    }
                };
                for (title, lines) in CREDITS {
                    section(title, lines.iter().map(|s| s.to_string()).collect());
                }
                let music = music_credit_lines();
                if !music.is_empty() {
                    section("MUSIC", music);
                }
            });
            spawn_button(root, &theme, "Back", MenuAction::CloseCredits, 0, true);
        });
}

fn credits_actions(
    mut reader: MessageReader<MenuActivated>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    let back = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::East));
    if back || reader.read().any(|MenuActivated(a)| *a == MenuAction::CloseCredits) {
        next_overlay.set(Overlay::None);
    }
}

fn spawn_death_screen(mut commands: Commands, theme: Res<UiTheme>, cause: Res<crate::hollow::DeathCause>) {
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK.with_alpha(0.88)),
            GlobalZIndex(40),
            DespawnOnExit(Screen::Death),
            Name::new("Death screen"),
        ))
        .with_children(|root| {
            root.spawn(theme.danger_panel(720.0)).with_children(|p| {
                p.spawn((Text::new(cause.title.clone()), theme.font(true, theme.size_heading), TextColor(theme.danger)));
                p.spawn((Text::new(cause.cause.clone()), theme.font(false, theme.size_body), TextColor(theme.text)));
                p.spawn((
                    Node { max_width: px(640), margin: UiRect::vertical(px(10)), padding: UiRect::all(px(12)), border: UiRect::left(px(3)), ..default() },
                    BorderColor::all(theme.interact),
                ))
                .with_child((Text::new(format!("LESSON  -  {}", cause.lesson)), theme.font(false, theme.size_small), TextColor(theme.interact)));
                spawn_button(p, &theme, "Retry", MenuAction::Retry, 0, true);
                spawn_button(p, &theme, "Return to Main Menu", MenuAction::AbandonRun, 1, false);
            });
        });
}

fn death_actions(mut reader: MessageReader<MenuActivated>, mut next_app: ResMut<NextState<AppState>>) {
    for MenuActivated(a) in reader.read() {
        match a {
            MenuAction::Retry => next_app.set(AppState::Restarting),
            MenuAction::AbandonRun => next_app.set(AppState::MainMenu),
            _ => {}
        }
    }
}
