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
            .add_systems(OnEnter(Screen::MainMenu), spawn_main_menu)
            // The intro animation plays when the menu first appears (launch, or
            // back from a run), not every time you close Settings/Credits/etc.:
            // buttons sliding in again made quick clicks miss.
            .add_systems(OnEnter(AppState::MainMenu), |mut c: ResMut<MenuClock>| c.0 = 0.0)
            .add_systems(OnEnter(Screen::Credits), spawn_credits)
            .add_systems(OnEnter(Screen::Cutscenes), spawn_cutscenes)
            .add_systems(OnEnter(Screen::Death), spawn_death_screen)
            .add_systems(OnEnter(Screen::Summary), spawn_summary)
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
                    cutscene_actions.run_if(in_state(Screen::Cutscenes)),
                    death_actions.run_if(in_state(Screen::Death)),
                    reveal_death_screen.run_if(in_state(Screen::Death)),
                    death_actions.run_if(in_state(Screen::Summary)),
                    animate_victory.run_if(in_state(Screen::Summary)),
                ),
            );
    }
}

/// Seconds since the main menu opened (drives the intro animation).
#[derive(Resource, Default)]
pub(crate) struct MenuClock(pub(crate) f32);

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

fn spawn_main_menu(mut commands: Commands, theme: Res<UiTheme>, seed: Res<RunSeed>, preview: Option<Res<super::skin_menu::PreviewImage>>) {
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
                // Shrinks the UI until the whole menu fits (short phone screens).
                crate::theme::ScreenRoot,
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
                // Grouped: play first, then extras and options, quit apart at the bottom.
                let groups: [(&str, &[(&str, MenuAction)]); 3] = [
                    ("", &[("Play", MenuAction::OpenPlay), ("Customise", MenuAction::OpenSkin)]),
                    ("EXTRAS", &[("Map Builder", MenuAction::OpenMapBuilder), ("Cutscenes", MenuAction::OpenCutscenes), ("Achievements", MenuAction::OpenAchievements), ("Settings", MenuAction::OpenSettings), ("Credits", MenuAction::OpenCredits)]),
                    ("", &[("Quit", MenuAction::Quit)]),
                ];
                let mut i = 0;
                for (g, (heading, items)) in groups.into_iter().enumerate() {
                    if g > 0 {
                        col.spawn(Node { height: px(14), ..default() });
                    }
                    if !heading.is_empty() {
                        col.spawn((Text::new(heading), theme.font(true, theme.size_small), TextColor(theme.interact), FadeIn { delay: 1.3, base: theme.interact }));
                    }
                    for (label, action) in items.iter().copied() {
                        let id = spawn_button(col, &theme, label, action, i, i == 0);
                        col.commands().entity(id).insert(SlideIn(1.4 + i as f32 * 0.12));
                        i += 1;
                    }
                }
            });
            // Your character, floating on the right: drag it to turn it around.
            root.spawn((
                Node { position_type: PositionType::Absolute, right: percent(9), top: percent(16), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(2), ..default() },
                FadeIn { delay: 1.6, base: Color::WHITE },
            ))
            .with_children(|c| {
                if let Some(p) = &preview {
                    super::skin_menu::spawn_preview(c, p, 320.0, 412.0);
                }
                c.spawn((theme.dim_text("Drag to turn  -  Customise to change"), FadeIn { delay: 1.8, base: theme.text_dim }));
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
                footer.spawn(theme.dim_text(format!("v{}", env!("CARGO_PKG_VERSION"))));
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

fn spawn_cutscenes(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((theme.screen_root(false), GlobalZIndex(20), DespawnOnExit(Screen::Cutscenes), Name::new("Cutscenes")))
        .with_children(|root| {
            root.spawn(theme.heading_text("CUTSCENES"));
            root.spawn(theme.panel(620.0)).with_children(|p| {
                for (i, (_, title, blurb)) in crate::theater::CUTSCENES.iter().enumerate() {
                    spawn_button(p, &theme, title, MenuAction::WatchCutscene(i as u8), i as i32, i == 0);
                    p.spawn((theme.dim_text(*blurb), Node { margin: UiRect::bottom(px(8)), ..default() }));
                }
                p.spawn(theme.dim_text("Hold Space (or A) to skip. You return here when it ends."));
                spawn_button(p, &theme, "Back", MenuAction::CloseCutscenes, 10, false);
            });
        });
}

fn cutscene_actions(
    mut reader: MessageReader<MenuActivated>,
    mut theater: ResMut<crate::theater::Theater>,
    mut skip: ResMut<crate::lobby::SkipLobby>,
    mut active: ResMut<crate::presets::ActivePreset>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::CloseCutscenes => next_overlay.set(Overlay::None),
            MenuAction::WatchCutscene(i) => {
                if let Some((id, ..)) = crate::theater::CUTSCENES.get(i as usize) {
                    theater.request(id);
                    skip.0 = true;
                    active.0 = None;
                    next_overlay.set(Overlay::None);
                    next_app.set(AppState::Playing);
                }
            }
            _ => {}
        }
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
            MenuAction::OpenPlay => next_overlay.set(Overlay::Play),
            MenuAction::OpenCutscenes => next_overlay.set(Overlay::Cutscenes),
            MenuAction::OpenSettings => next_overlay.set(Overlay::Settings),
            MenuAction::OpenCredits => next_overlay.set(Overlay::Credits),
            MenuAction::OpenAchievements => next_overlay.set(Overlay::Achievements),
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
                spawn_button(panel, &theme, "Admin Panel", MenuAction::OpenAdmin, 2, false);
                spawn_button(panel, &theme, "Abandon Run", MenuAction::AbandonRun, 3, false);
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
            MenuAction::OpenAdmin => next_overlay.set(Overlay::Admin),
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
    // Automated QA runs on a virtual display that never has focus.
    if std::env::var_os("HALLWAYS_THEATER").is_some() || std::env::var_os("HALLWAYS_CAPTURE").is_some() {
        reader.clear();
        return;
    }
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
    let in_control = screen.is_some_and(|s| *s.get() == Screen::Hud) && window.focused && !crate::qa::fake_touch();
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
    ("A GAME BY", &["Donut Studios"]),
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
            "Concrete Wall 006: Dario Barresi, Charlotte Baglioni     Concrete Tiles: Dimitrios Savva",
            "Marble 01, Rusty Metal 02, Metal Plate: Rob Tuytel     Rock Wall 08: Amal Kumar",
            "Dirt Floor: eye-candy.xyz     Dark Brick Wall: Dario Barresi, Dimitrios Savva",
            "Corrugated Iron 02: Jenelle van Heerden, Sergej Majboroda",
        ],
    ),
    ("FONT", &["IBM Plex Mono by IBM (SIL Open Font License 1.1)"]),
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

/// The death screen waits for the death animation, then fades in.
#[derive(Component)]
struct DeathReveal(f32);

const DEATH_ANIM: f32 = 1.6;

fn reveal_death_screen(
    time: Res<Time<Real>>,
    mut q: Query<(&mut DeathReveal, &mut Visibility, &mut BackgroundColor, &Children)>,
    mut panel: Query<&mut Visibility, Without<DeathReveal>>,
) {
    for (mut r, mut v, mut bg, kids) in &mut q {
        r.0 += time.delta_secs().min(1.0 / 20.0);
        for c in kids.iter() {
            if let Ok(mut pv) = panel.get_mut(c) {
                *pv = if r.0 < DEATH_ANIM { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
        // A red wash while you fall, then the panel.
        let k = ((r.0 - DEATH_ANIM) / 0.5).clamp(0.0, 1.0);
        if r.0 < DEATH_ANIM {
            *v = Visibility::Visible;
            bg.0 = Color::srgba(0.35, 0.0, 0.0, (r.0 / DEATH_ANIM) * 0.45);
        } else {
            bg.0 = Color::srgba(0.35 * (1.0 - k), 0.0, 0.0, 0.45 + 0.43 * k);
        }
    }
}

fn spawn_death_screen(mut commands: Commands, theme: Res<UiTheme>, cause: Res<crate::hollow::DeathCause>, run: Res<crate::run::RunState>, profile: Res<crate::profile::Profile>) {
    let earned = (run.lumens as f32 * profile.lumen_multiplier()).round() as u32;
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
            BackgroundColor(Color::BLACK.with_alpha(0.0)),
            GlobalZIndex(40),
            DespawnOnExit(Screen::Death),
            Name::new("Death screen"),
            DeathReveal(0.0),
            Visibility::Hidden,
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
                p.spawn((
                    Text::new(format!("Reached Room {:03}   -   {} Lumens banked for the shop", run.best_room.max(run.current_room), earned)),
                    theme.font(false, theme.size_small),
                    TextColor(theme.text_dim),
                ));
                spawn_button(p, &theme, "Retry", MenuAction::Retry, 0, true);
                spawn_button(p, &theme, "Return to Main Menu", MenuAction::AbandonRun, 1, false);
            });
        });
}

fn death_actions(
    mut reader: MessageReader<MenuActivated>,
    mut next_app: ResMut<NextState<AppState>>,
    mut seed: ResMut<RunSeed>,
    profile: Res<crate::profile::Profile>,
    preset: Res<crate::presets::ActivePreset>,
    session: Res<crate::net::NetSession>,
) {
    for MenuActivated(a) in reader.read() {
        match a {
            MenuAction::Retry => {
                // A retry is a new floor (new rooms, monsters and chase), unless the
                // player typed a seed to replay or the room's host picked it.
                let typed = !profile.seed_text.trim().is_empty() || preset.0.as_ref().is_some_and(|p| !p.seed.trim().is_empty());
                if !typed && !session.online() {
                    *seed = RunSeed(crate::ui::play_menu::random_seed());
                }
                next_app.set(AppState::Restarting);
            }
            MenuAction::AbandonRun => next_app.set(AppState::MainMenu),
            _ => {}
        }
    }
}

#[derive(Component)]
struct VictoryTitle;
#[derive(Component)]
struct VictorySparkle(f32);

/// Victory screen motion: the title pulses and glows, sparkles drift upward.
fn animate_victory(time: Res<Time<Real>>, mut title: Query<(&mut TextColor, &mut TextFont), With<VictoryTitle>>, mut sparks: Query<(&VictorySparkle, &mut Node, &mut BackgroundColor)>) {
    let t = time.elapsed_secs();
    for (mut c, mut f) in &mut title {
        let k = 0.5 + 0.5 * (t * 2.4).sin();
        c.0 = Color::srgb(1.0, 0.75 + 0.15 * k, 0.25 + 0.2 * k);
        f.font_size = bevy::text::FontSize::Px(92.0 + 8.0 * k);
    }
    for (s, mut n, mut bg) in &mut sparks {
        let y = (s.0 * 53.0 - t * (6.0 + s.0 % 5.0)).rem_euclid(100.0);
        n.top = percent(y);
        n.left = percent((s.0 * 37.0 + (t * 0.7 + s.0).sin() * 2.0).rem_euclid(100.0));
        bg.0 = Color::srgba(1.0, 0.82, 0.35, 0.25 + 0.35 * (0.5 + 0.5 * (t * 3.0 + s.0).sin()));
    }
}

fn spawn_summary(mut commands: Commands, theme: Res<UiTheme>, run: Res<crate::run::RunState>, seed: Res<RunSeed>, profile: Res<crate::profile::Profile>, floor: Option<Res<crate::floor::Floor>>) {
    let final_door = floor.map_or(100, |f| f.0.final_door());
    commands
        .spawn((
            Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
            BackgroundColor(Color::BLACK.with_alpha(0.92)),
            GlobalZIndex(40),
            DespawnOnExit(Screen::Summary),
            Name::new("Run summary"),
        ))
        .with_children(|root| {
            // Drifting gold sparkles behind the panel.
            for i in 0..36 {
                let x = (i as f32 * 37.0) % 100.0;
                let y = (i as f32 * 53.0) % 100.0;
                root.spawn((
                    Node { position_type: PositionType::Absolute, left: percent(x), top: percent(y), width: px(4.0 + (i % 3) as f32 * 2.0), height: px(4.0 + (i % 3) as f32 * 2.0), border_radius: BorderRadius::MAX, ..default() },
                    BackgroundColor(Color::srgba(1.0, 0.82, 0.35, 0.5)),
                    VictorySparkle(i as f32),
                ));
            }
            root.spawn((Text::new("VICTORY"), theme.font(true, 96.0), TextColor(Color::srgb(1.0, 0.8, 0.3)), VictoryTitle, Node { margin: UiRect::bottom(px(12)), ..default() }));
            root.spawn(theme.panel(640.0)).with_children(|p| {
                p.spawn((Text::new("FLOOR 1 COMPLETE"), theme.font(true, theme.size_heading), TextColor(theme.interact)));
                p.spawn((Text::new(format!("You walked out through Door {:03}.", final_door)), theme.font(false, theme.size_body), TextColor(theme.text_dim)));
                let rows = [
                    ("Time", run.timer_text()),
                    ("Doors opened", format!("{final_door} / {final_door}")),
                    ("Lumens collected", run.lumens.to_string()),
                    ("Lumens banked (with modifiers)", format!("{}", (run.lumens as f32 * profile.lumen_multiplier()).round() as u32)),
                    ("Surges survived", run.surges_survived.to_string()),
                    ("Mimics that fooled you", run.mimics_fooled.to_string()),
                    ("Damage taken", format!("{:.0}", run.damage_taken)),
                    ("Seed", seed.display()),
                ];
                for (label, value) in rows {
                    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, column_gap: px(40), ..default() }).with_children(|r| {
                        r.spawn((Text::new(label), theme.font(false, theme.size_body), TextColor(theme.text)));
                        r.spawn((Text::new(value), theme.font(true, theme.size_body), TextColor(theme.interact)));
                    });
                }
                spawn_button(p, &theme, "Play Again", MenuAction::Retry, 0, true);
                spawn_button(p, &theme, "Return to Main Menu", MenuAction::AbandonRun, 1, false);
            });
        });
}
