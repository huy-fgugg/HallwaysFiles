//! In-run HUD: room number, timer, currency, vitality, item slots, crosshair,
//! interaction prompt, hiding overlay and subtitles.

use bevy::prelude::*;

use crate::interaction::{prompt_text, Door, Drawer, HideState, InteractTarget, Interactable, HIDE_WARN, HIDE_LIMIT};
use crate::rng::RunSeed;
use crate::run::{RunState, Subtitle};
use crate::settings::Settings;
use crate::state::{AppState, Screen};
use crate::theme::UiTheme;

#[derive(Component)]
struct RoomLabel;
#[derive(Component)]
struct TimerLabel;
#[derive(Component)]
struct LumensLabel;
#[derive(Component)]
struct VitalityFill;
#[derive(Component)]
struct Crosshair;
#[derive(Component)]
struct PromptBox;
#[derive(Component)]
struct PromptText;
#[derive(Component)]
struct HideOverlay;
#[derive(Component)]
struct HideFill;
#[derive(Component)]
struct SubtitleBox;
#[derive(Component)]
struct SubtitleText;
#[derive(Component)]
struct HudRoot;

#[derive(Resource, Default)]
struct ActiveSubtitle {
    text: String,
    danger: bool,
    remaining: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveSubtitle>()
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(
                Update,
                (update_labels, update_prompt, update_hide_overlay, update_subtitles, hud_visibility)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn spawn_hud(mut commands: Commands, theme: Res<UiTheme>, seed: Res<RunSeed>) {
    commands.insert_resource(ActiveSubtitle::default());
    let t = theme.clone();
    commands
        .spawn((
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            Pickable::IGNORE,
            HudRoot,
            DespawnOnExit(AppState::Playing),
            Name::new("HUD"),
        ))
        .with_children(|root| {
            // Hiding overlay: dark vignette with wardrobe-door slats (below everything else).
            root.spawn((
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), flex_direction: FlexDirection::Row, ..default() },
                Visibility::Hidden,
                HideOverlay,
            ))
            .with_children(|o| {
                o.spawn((Node { width: percent(46), height: percent(100), ..default() }, BackgroundColor(Color::BLACK.with_alpha(0.93))));
                o.spawn((Node { width: percent(8), height: percent(100), ..default() }, BackgroundColor(Color::BLACK.with_alpha(0.25))));
                o.spawn((Node { width: percent(46), height: percent(100), ..default() }, BackgroundColor(Color::BLACK.with_alpha(0.93))));
            });

            // Top-left: room / seed / timer.
            root.spawn(Node { position_type: PositionType::Absolute, top: px(24), left: px(28), flex_direction: FlexDirection::Column, row_gap: px(2), ..default() })
                .with_children(|c| {
                    c.spawn((t.hud_text_sized("ROOM 000", true, 40.0), RoomLabel));
                    c.spawn((Text::new("00:00"), t.font(false, t.size_body), TextColor(t.text_dim), TimerLabel));
                    c.spawn(t.dim_text(format!("SEED {}", seed.display())));
                });

            // Top-right: currency.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: px(24),
                right: px(28),
                align_items: AlignItems::Center,
                column_gap: px(10),
                padding: UiRect::axes(px(14), px(8)),
                border: UiRect::all(px(t.border)),
                border_radius: BorderRadius::all(px(t.radius)),
                ..default()
            })
            .insert((BackgroundColor(t.panel.with_alpha(0.75)), BorderColor::all(t.panel_border)))
            .with_children(|c| {
                c.spawn((Node { width: px(12), height: px(12), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(t.interact)));
                c.spawn((t.hud_text("0"), LumensLabel));
                c.spawn(t.dim_text("LUMENS"));
            });

            // Bottom-left: vitality.
            root.spawn(Node { position_type: PositionType::Absolute, bottom: px(28), left: px(28), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() })
                .with_children(|c| {
                    c.spawn(t.dim_text("VITALITY"));
                    c.spawn((
                        Node { width: px(260), height: px(12), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() },
                        BackgroundColor(t.panel.with_alpha(0.8)),
                        BorderColor::all(t.panel_border),
                    ))
                    .with_child((Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(px(2)), ..default() }, BackgroundColor(t.text.with_alpha(0.85)), VitalityFill));
                });

            // Bottom-centre: item slots.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                width: percent(100),
                justify_content: JustifyContent::Center,
                column_gap: px(10),
                ..default()
            })
            .with_children(|c| {
                for i in 1..=3 {
                    c.spawn((
                        Node {
                            width: px(64),
                            height: px(64),
                            border: UiRect::all(px(t.border)),
                            border_radius: BorderRadius::all(px(t.radius)),
                            padding: UiRect::all(px(4)),
                            ..default()
                        },
                        BackgroundColor(t.panel.with_alpha(0.6)),
                        BorderColor::all(t.panel_border),
                    ))
                    .with_child((Text::new(i.to_string()), t.font(true, t.size_small), TextColor(t.text_dim)));
                }
            });

            // Centre: crosshair + prompt.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    Node { width: px(6), height: px(6), border_radius: BorderRadius::MAX, ..default() },
                    BackgroundColor(t.text.with_alpha(0.6)),
                    Crosshair,
                ));
                // Fixed-height spacer keeps the crosshair centred.
                c.spawn(Node { height: px(0), ..default() }).with_children(|s| {
                    s.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(36),
                            left: px(-300),
                            width: px(600),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Visibility::Hidden,
                        PromptBox,
                    ))
                    .with_children(|pb| {
                        pb.spawn((
                            Node {
                                padding: UiRect::axes(px(14), px(8)),
                                border: UiRect::all(px(t.border)),
                                border_radius: BorderRadius::all(px(t.radius)),
                                column_gap: px(10),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(t.panel.with_alpha(0.85)),
                            BorderColor::all(t.interact),
                        ))
                        .with_children(|p| {
                            p.spawn((
                                Node {
                                    padding: UiRect::axes(px(8), px(2)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(3)),
                                    ..default()
                                },
                                BorderColor::all(t.interact),
                            ))
                            .with_child((Text::new("E"), t.font(true, t.size_body), TextColor(t.interact)));
                            p.spawn((Text::new(""), t.font(false, t.size_body), TextColor(t.text), PromptText));
                        });
                    });
                });
            });

            // Hide meter (above the slots).
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(150),
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(6),
                    ..default()
                },
                Visibility::Hidden,
                HideOverlay,
            ))
            .with_children(|c| {
                c.spawn(t.hud_text_sized("HIDING  -  [E] LEAVE", true, t.size_small));
                c.spawn((
                    Node { width: px(320), height: px(8), border_radius: BorderRadius::all(px(3)), ..default() },
                    BackgroundColor(t.panel.with_alpha(0.9)),
                ))
                .with_child((Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(t.interact), HideFill));
            });

            // Subtitles.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(110),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    Node { padding: UiRect::axes(px(16), px(8)), border_radius: BorderRadius::all(px(t.radius)), max_width: px(900), ..default() },
                    BackgroundColor(Color::BLACK.with_alpha(0.7)),
                    Visibility::Hidden,
                    SubtitleBox,
                ))
                .with_child((Text::new(""), t.font(false, t.size_body), TextColor(t.text), TextLayout { justify: Justify::Center, ..default() }, SubtitleText));
            });
        });
}

/// Hide the HUD while menus are open on top of the run.
fn hud_visibility(screen: Option<Res<State<Screen>>>, mut q: Query<&mut Visibility, With<HudRoot>>) {
    let show = screen.is_some_and(|s| *s.get() == Screen::Hud);
    for mut v in &mut q {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

fn update_labels(
    run: Res<RunState>,
    mut room: Query<&mut Text, (With<RoomLabel>, Without<TimerLabel>, Without<LumensLabel>)>,
    mut timer: Query<&mut Text, (With<TimerLabel>, Without<RoomLabel>, Without<LumensLabel>)>,
    mut lumens: Query<&mut Text, (With<LumensLabel>, Without<RoomLabel>, Without<TimerLabel>)>,
    mut vit: Query<&mut Node, With<VitalityFill>>,
) {
    let set = |t: &mut Text, s: String| {
        if t.0 != s {
            t.0 = s;
        }
    };
    if let Ok(mut t) = room.single_mut() {
        set(&mut t, format!("ROOM {:03}", run.current_room));
    }
    if let Ok(mut t) = timer.single_mut() {
        set(&mut t, run.timer_text());
    }
    if let Ok(mut t) = lumens.single_mut() {
        set(&mut t, run.lumens.to_string());
    }
    if let Ok(mut n) = vit.single_mut() {
        n.width = percent(run.vitality.clamp(0.0, 100.0));
    }
}

#[allow(clippy::too_many_arguments)]
fn update_prompt(
    theme: Res<UiTheme>,
    target: Res<InteractTarget>,
    interactables: Query<&Interactable>,
    doors: Query<&mut Door>,
    drawers: Query<&mut Drawer>,
    mut boxq: Query<&mut Visibility, With<PromptBox>>,
    mut textq: Query<(&mut Text, &mut TextColor), With<PromptText>>,
    mut cross: Query<&mut BackgroundColor, With<Crosshair>>,
) {
    let prompt = target.0.and_then(|e| prompt_text(e, &interactables, &doors, &drawers));
    if let Ok(mut v) = boxq.single_mut() {
        let want = if prompt.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if let Ok(mut c) = cross.single_mut() {
        c.0 = if prompt.is_some() { theme.interact } else { theme.text.with_alpha(0.6) };
    }
    if let (Some((s, warn)), Ok((mut text, mut color))) = (prompt, textq.single_mut()) {
        if text.0 != s {
            text.0 = s;
        }
        color.0 = if warn { theme.danger } else { theme.text };
    }
}

fn update_hide_overlay(
    theme: Res<UiTheme>,
    hide: Res<HideState>,
    mut overlays: Query<&mut Visibility, With<HideOverlay>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<HideFill>>,
) {
    let frac = hide.hidden_fraction();
    for mut v in &mut overlays {
        let want = if frac.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if let (Some(f), Ok((mut n, mut c))) = (frac, fill.single_mut()) {
        n.width = percent((1.0 - f) * 100.0);
        c.0 = if f * HIDE_LIMIT >= HIDE_WARN { theme.danger } else { theme.interact };
    }
}

fn update_subtitles(
    time: Res<Time>,
    theme: Res<UiTheme>,
    settings: Res<Settings>,
    mut reader: MessageReader<Subtitle>,
    mut active: ResMut<ActiveSubtitle>,
    mut boxq: Query<&mut Visibility, With<SubtitleBox>>,
    mut textq: Query<(&mut Text, &mut TextColor), With<SubtitleText>>,
) {
    for s in reader.read() {
        active.text = s.text.clone();
        active.danger = s.danger;
        active.remaining = s.seconds;
    }
    active.remaining -= time.delta_secs();
    let show = settings.subtitles && active.remaining > 0.0 && !active.text.is_empty();
    if let Ok(mut v) = boxq.single_mut() {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if show && let Ok((mut text, mut color)) = textq.single_mut() {
        if text.0 != active.text {
            text.0 = active.text.clone();
        }
        color.0 = if active.danger { theme.danger } else { theme.text };
    }
}
