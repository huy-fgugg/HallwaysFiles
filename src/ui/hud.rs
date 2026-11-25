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
#[derive(Component)]
struct ItemSlot(usize);
#[derive(Component)]
struct ItemLabel(usize);
#[derive(Component)]
struct ItemCount(usize);
#[derive(Component)]
struct ItemStripe(usize);
#[derive(Component)]
struct ItemIcon(usize);
#[derive(Component)]
struct BatteryBar;
#[derive(Component)]
struct StaminaRow;
#[derive(Component)]
struct StaminaFill;
#[derive(Component)]
struct BatteryFill;

#[derive(Resource, Default)]
struct ActiveSubtitle {
    text: String,
    danger: bool,
    remaining: f32,
}

#[derive(Component)]
struct DamageFlash;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveSubtitle>()
            .init_resource::<HideHud>()
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(
                Update,
                (update_labels, update_stamina, update_prompt, update_hide_overlay, update_subtitles, hud_visibility, update_items, update_damage_flash)
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
            crate::theme::PassThrough,
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

            // Damage flash / low-vitality pulse: red edges over the view.
            root.spawn((
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), border: UiRect::all(px(70)), ..default() },
                BorderColor::all(Color::NONE),
                BackgroundColor(Color::NONE),
                Pickable::IGNORE,
                DamageFlash,
            ));

            // Top-left: room / seed / timer.
            root.spawn(Node { position_type: PositionType::Absolute, top: px(24), left: px(28), flex_direction: FlexDirection::Column, row_gap: px(2), ..default() })
                .with_children(|c| {
                    // No room number on screen: the door signs are the only count.
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
                    // Stamina: shown while it's not full.
                    c.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() }, Visibility::Hidden, StaminaRow))
                        .with_children(|r| {
                            r.spawn((Node { width: px(186), height: px(6), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(t.panel.with_alpha(0.9))))
                                .with_child((Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(Color::srgb(0.55, 0.78, 0.95)), StaminaFill));
                            r.spawn((Text::new("STAMINA"), t.font(false, 12.0), TextColor(t.text_dim), TextLayout::no_wrap()));
                        });
                });

            // Bottom-centre: item slots (+ flashlight battery bar above them).
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                ..default()
            })
            .with_children(|col| {
                col.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() }, Visibility::Hidden, BatteryBar))
                    .with_children(|b| {
                        b.spawn((Text::new("BATTERY"), t.font(false, 13.0), TextColor(t.text_dim)));
                        b.spawn((Node { width: px(150), height: px(6), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(t.panel.with_alpha(0.9))))
                            .with_child((Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(crate::items::ItemKind::Flashlight.color()), BatteryFill));
                    });
                col.spawn(Node { column_gap: px(10), ..default() }).with_children(|c| {
                    for i in 0..crate::items::SLOTS {
                        c.spawn((
                            Node {
                                width: px(78),
                                height: px(84),
                                border: UiRect::all(px(t.border)),
                                border_radius: BorderRadius::all(px(t.radius)),
                                padding: UiRect::all(px(5)),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::SpaceBetween,
                                ..default()
                            },
                            BackgroundColor(t.panel.with_alpha(0.65)),
                            BorderColor::all(t.panel_border),
                            ItemSlot(i),
                        ))
                        .with_children(|slot| {
                            slot.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|top| {
                                top.spawn((Text::new((i + 1).to_string()), t.font(true, 13.0), TextColor(t.text_dim)));
                                top.spawn((Text::new(""), t.font(true, 13.0), TextColor(t.text), ItemCount(i)));
                            });
                            // Item picture, with its short name underneath.
                            slot.spawn(Node { width: percent(100), justify_content: JustifyContent::Center, ..default() })
                                .with_child((ImageNode::default(), Node { width: px(36), height: px(36), ..default() }, Visibility::Hidden, ItemIcon(i)));
                            slot.spawn((Text::new(""), t.font(true, 11.0), TextColor(t.text), ItemLabel(i), Node { align_self: AlignSelf::Center, ..default() }));
                            slot.spawn((Node { width: percent(100), height: px(4), border_radius: BorderRadius::all(px(2)), ..default() }, BackgroundColor(Color::NONE), ItemStripe(i)));
                        });
                    }
                });
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

            // Subtitles (above the item slots, battery bar and hide meter).
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(200),
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
/// Hides the whole HUD (the trailer recorder uses this for clean shots).
#[derive(Resource, Debug, Default)]
pub struct HideHud(pub bool);

fn hud_visibility(screen: Option<Res<State<Screen>>>, hide: Res<HideHud>, mut q: Query<&mut Visibility, With<HudRoot>>) {
    let show = screen.is_some_and(|s| *s.get() == Screen::Hud) && !hide.0;
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

fn update_stamina(
    player: Query<&crate::player::Player>,
    mut row: Query<&mut Visibility, With<StaminaRow>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<StaminaFill>>,
) {
    let Ok(p) = player.single() else { return };
    if let Ok(mut v) = row.single_mut() {
        let want = if p.stamina < 0.999 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if let Ok((mut n, mut c)) = fill.single_mut() {
        n.width = percent(p.stamina.clamp(0.0, 1.0) * 100.0);
        // Red while winded (can't sprint until it recovers).
        c.0 = if p.winded { Color::srgb(0.85, 0.35, 0.3) } else { Color::srgb(0.55, 0.78, 0.95) };
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

/// Red edge flash when vitality drops, and a slow pulse while it is low.
fn update_damage_flash(
    time: Res<Time>,
    theme: Res<UiTheme>,
    run: Res<RunState>,
    settings: Res<crate::settings::Settings>,
    mut q: Query<(&mut BorderColor, &mut BackgroundColor), With<DamageFlash>>,
    mut state: Local<(f32, f32)>,
) {
    let (last, mut flash) = *state;
    if run.vitality < last - 0.5 {
        flash = 1.0;
    }
    flash = (flash - time.delta_secs() * if settings.reduce_flashing { 0.8 } else { 1.6 }).max(0.0);
    *state = (run.vitality, flash);
    let low = if run.vitality > 0.0 && run.vitality < 35.0 {
        let k = 1.0 - run.vitality / 35.0;
        let speed = if settings.reduce_flashing { 1.5 } else { 3.2 };
        (0.5 + 0.5 * (time.elapsed_secs() * speed).sin()) * 0.22 * k + 0.08 * k
    } else {
        0.0
    };
    let peak = if settings.reduce_flashing { 0.35 } else { 0.6 };
    let edge = (flash * peak).max(low);
    for (mut b, mut bg) in &mut q {
        *b = BorderColor::all(theme.danger.with_alpha(edge));
        bg.0 = theme.danger.with_alpha(flash * peak * 0.25);
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
    cutscene: Res<crate::cutscene::ActiveCutscene>,
) {
    for s in reader.read() {
        // Cutscenes draw their own captions inside the letterbox.
        if cutscene.is_playing() {
            active.remaining = 0.0;
            continue;
        }
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

#[allow(clippy::type_complexity)]
fn update_items(
    theme: Res<UiTheme>,
    inv: Res<crate::items::Inventory>,
    mut slots: Query<(&ItemSlot, &mut BorderColor)>,
    mut texts: ParamSet<(Query<(&ItemLabel, &mut Text, &mut TextColor)>, Query<(&ItemCount, &mut Text)>)>,
    mut stripes: Query<(&ItemStripe, &mut BackgroundColor), Without<BatteryFill>>,
    mut bar: Query<&mut Visibility, With<BatteryBar>>,
    mut fill: Query<&mut Node, With<BatteryFill>>,
    icons: Res<crate::icons::ItemIcons>,
    mut images: Query<(&ItemIcon, &mut ImageNode, &mut Visibility), Without<BatteryBar>>,
) {
    if !inv.is_changed() {
        return;
    }
    for (slot, mut img, mut vis) in &mut images {
        match inv.slots[slot.0].and_then(|s| icons.0.get(&s.kind)) {
            Some(h) => {
                img.image = h.clone();
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
    for (slot, mut border) in &mut slots {
        let c = if slot.0 == inv.selected { theme.interact } else { theme.panel_border };
        *border = BorderColor::all(c);
    }
    for (l, mut text, mut color) in &mut texts.p0() {
        let st = inv.slots[l.0];
        text.0 = st.map(|s| s.kind.short().to_string()).unwrap_or_default();
        color.0 = st.map(|s| s.kind.color()).unwrap_or(theme.text);
    }
    for (c, mut text) in &mut texts.p1() {
        text.0 = match inv.slots[c.0] {
            Some(s) if s.count > 1 => format!("x{}", s.count),
            Some(s) if s.kind == crate::items::ItemKind::Flashlight && inv.flashlight_on => "ON".into(),
            _ => String::new(),
        };
    }
    for (s, mut bg) in &mut stripes {
        bg.0 = inv.slots[s.0].map(|st| st.kind.color().with_alpha(0.8)).unwrap_or(Color::NONE);
    }
    let has_light = inv.has(crate::items::ItemKind::Flashlight);
    for mut v in &mut bar {
        *v = if has_light { Visibility::Inherited } else { Visibility::Hidden };
    }
    for mut n in &mut fill {
        let f = if inv.battery_max > 0.0 { inv.battery / inv.battery_max } else { 0.0 };
        n.width = percent(f * 100.0);
    }
}
