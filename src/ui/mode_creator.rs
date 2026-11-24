//! Mode Creator: the easy way to make your own mode. Every setting is a big
//! card you tap to change (length, areas, each monster, lights, tricks,
//! supplies); a meter shows how hard it is; play it, host it, save it, or
//! share it with a short code. "Build Rooms" opens the room-by-room builder.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use super::focus::{spawn_button, Focusable, MenuAction, MenuActivated};
use super::play_menu::{field_row, random_seed, FieldKind};
use crate::floor::plan::Area;
use crate::net::NetRequest;
use crate::presets::{self, ActivePreset, Entry, Preset, ENTITY_NAMES, LEVEL_NAMES};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

/// One card on the creator screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Card {
    Length,
    RoomSize,
    Area(usize),
    Monster(usize),
    Brightness,
    DarkRooms,
    FakeDoors,
    Structures,
    Supplies,
}

const LENGTHS: [u32; 6] = [20, 30, 40, 60, 80, 100];
const SIZES: [(u32, &str); 4] = [(70, "Small"), (100, "Normal"), (130, "Big"), (160, "Huge")];
const BRIGHT: [(u32, &str); 4] = [(140, "Bright"), (100, "Normal"), (60, "Dim"), (30, "Pitch black")];
const DARK: [(u32, &str); 4] = [(0, "None"), (100, "Some"), (200, "Many"), (300, "Most")];
const FAKES: [(u32, &str); 4] = [(0, "Off"), (100, "Some"), (200, "Many"), (300, "Everywhere")];
const STRUCT: [(u32, &str); 3] = [(25, "Few"), (100, "Normal"), (250, "Many")];
const LOOT: [(u32, &str); 3] = [(50, "Scarce"), (100, "Normal"), (200, "Plenty")];
const MONSTER_HINTS: [&str; 6] = ["Hide from it", "Stay hidden", "Look at it", "Look away", "Stand still", "Crouch, be quiet"];

/// Index of the option closest to `v`.
fn nearest(opts: &[(u32, &str)], v: u32) -> usize {
    opts.iter().enumerate().min_by_key(|(_, (x, _))| x.abs_diff(v)).map_or(0, |(i, _)| i)
}

impl Card {
    pub fn all() -> Vec<(&'static str, Vec<Card>)> {
        vec![
            ("AREAS", (0..6).map(Card::Area).collect()),
            ("MONSTERS", (0..6).map(Card::Monster).collect()),
            ("FLOOR, LIGHTS & TRICKS", vec![Card::Length, Card::RoomSize, Card::Brightness, Card::DarkRooms, Card::FakeDoors, Card::Supplies]),
        ]
    }

    pub fn title(self) -> String {
        match self {
            Card::Length => "Length".into(),
            Card::RoomSize => "Room size".into(),
            Card::Area(i) => Area::ALL[i].name().trim_start_matches("The ").trim_start_matches("Service ").to_string(),
            Card::Monster(i) => ENTITY_NAMES[i].trim_start_matches("The ").to_string(),
            Card::Brightness => "Lights".into(),
            Card::DarkRooms => "Dark rooms".into(),
            Card::FakeDoors => "Fake doors".into(),
            Card::Structures => "Structures".into(),
            Card::Supplies => "Supplies".into(),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Card::Length => "Doors to the end",
            Card::RoomSize => "How big rooms are",
            Card::Area(_) => "Tap to include",
            Card::Monster(i) => MONSTER_HINTS[i],
            Card::Brightness => "Lamp brightness",
            Card::DarkRooms => "Rooms with no lamp",
            Card::FakeDoors => "Doors that lie",
            Card::Structures => "Stairs, lofts, mazes",
            Card::Supplies => "Loot in drawers",
        }
    }

    pub fn value(self, p: &Preset) -> String {
        match self {
            Card::Length => format!("{} doors", p.doors),
            Card::RoomSize => SIZES[nearest(&SIZES, p.room_size)].1.into(),
            Card::Area(i) => if p.areas.contains(&Area::ALL[i]) { "ON".into() } else { "off".into() },
            Card::Monster(i) => LEVEL_NAMES[p.level(i) as usize].into(),
            Card::Brightness => BRIGHT[nearest(&BRIGHT, p.brightness)].1.into(),
            Card::DarkRooms => DARK[nearest(&DARK, p.dark_rooms)].1.into(),
            Card::FakeDoors => FAKES[nearest(&FAKES, p.fake_doors)].1.into(),
            Card::Structures => STRUCT[nearest(&STRUCT, p.stair_rooms)].1.into(),
            Card::Supplies => LOOT[nearest(&LOOT, p.loot)].1.into(),
        }
    }

    /// Whether the card is "on" (lit) or "off" (dim).
    pub fn lit(self, p: &Preset) -> bool {
        match self {
            Card::Area(i) => p.areas.contains(&Area::ALL[i]),
            Card::Monster(i) => p.level(i) > 0,
            _ => true,
        }
    }

    /// Steps to the next (or previous) choice, wrapping.
    pub fn cycle(self, p: &mut Preset, d: i32) {
        let step = |opts: &[(u32, &str)], v: u32| opts[(nearest(opts, v) as i32 + d).rem_euclid(opts.len() as i32) as usize].0;
        match self {
            Card::Length => {
                let i = LENGTHS.iter().position(|x| *x >= p.doors).unwrap_or(0) as i32;
                p.doors = LENGTHS[(i + d).rem_euclid(LENGTHS.len() as i32) as usize];
            }
            Card::RoomSize => p.room_size = step(&SIZES, p.room_size),
            Card::Area(i) => p.toggle_area(Area::ALL[i]),
            Card::Monster(i) => {
                let l = (p.level(i) as i32 + d).rem_euclid(LEVEL_NAMES.len() as i32) as u8;
                p.set_level(i, l);
            }
            Card::Brightness => p.brightness = step(&BRIGHT, p.brightness),
            Card::DarkRooms => p.dark_rooms = step(&DARK, p.dark_rooms),
            Card::FakeDoors => p.fake_doors = step(&FAKES, p.fake_doors),
            Card::Structures => p.stair_rooms = step(&STRUCT, p.stair_rooms),
            Card::Supplies => p.loot = step(&LOOT, p.loot),
        }
    }
}

/// The mode being made, and the list to start from.
#[derive(Resource)]
pub struct ModeEditor {
    pub mode: Preset,
    list: Vec<Entry>,
    index: usize,
    pub code: String,
    pub status: String,
    pub version: u32,
}

impl ModeEditor {
    fn load() -> Self {
        let list: Vec<Entry> = presets::load_all(&presets::presets_dir()).into_iter().filter(|e| e.preset.rooms.is_empty()).collect();
        let mut mode = Preset { name: "My Mode".into(), ..Preset::default() };
        mode.doors = 40;
        Self { mode, list, index: 0, code: String::new(), status: "Tap a card to change it. Everything is saved when you press Save.".into(), version: 1 }
    }
}

#[derive(Component)]
struct CreatorRoot;

pub struct ModeCreatorPlugin;

impl Plugin for ModeCreatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::ModeCreator), |mut c: Commands, old: Option<Res<ModeEditor>>| {
            if old.is_none() {
                c.insert_resource(ModeEditor::load());
            }
        })
        .add_systems(Update, (creator_actions, redraw, super::play_menu::room_ready).chain().run_if(in_state(Screen::ModeCreator)))
        .add_systems(Update, open_creator.run_if(in_state(AppState::MainMenu)))
        .add_systems(OnEnter(AppState::Playing), spawn_player_made_banner)
        .add_systems(Update, show_player_made_banner.run_if(in_state(AppState::Playing)));
    }
}

/// "PLAYER-MADE MODE" banner shown through the whole run of a player's mode or map.
#[derive(Component)]
struct PlayerMadeBanner;

fn spawn_player_made_banner(mut commands: Commands, theme: Res<UiTheme>, active: Res<ActivePreset>) {
    let Some(p) = active.0.as_ref().filter(|p| p.player_made()) else { return };
    let kind = if p.rooms.is_empty() { "PLAYER-MADE MODE" } else { "PLAYER-MADE MAP" };
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, top: px(10), left: percent(50), margin: UiRect::left(px(-210)), width: px(420), justify_content: JustifyContent::Center, padding: UiRect::axes(px(12), px(4)), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(6)), ..default() },
            BackgroundColor(Color::srgba(0.35, 0.18, 0.02, 0.8)),
            BorderColor::all(Color::srgb(1.0, 0.75, 0.25)),
            GlobalZIndex(12),
            Pickable::IGNORE,
            PlayerMadeBanner,
            DespawnOnExit(AppState::Playing),
        ))
        .with_child((Text::new(format!("{kind}:  {}", p.name.to_uppercase())), theme.font(true, theme.size_small), TextColor(Color::srgb(1.0, 0.85, 0.45)), Pickable::IGNORE));
}

fn show_player_made_banner(screen: Res<State<Screen>>, cutscene: Res<crate::cutscene::ActiveCutscene>, mut q: Query<&mut Visibility, With<PlayerMadeBanner>>) {
    let show = matches!(screen.get(), Screen::Hud | Screen::Pause) && !cutscene.is_playing();
    for mut v in &mut q {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

fn open_creator(mut reader: MessageReader<MenuActivated>, mut next: ResMut<NextState<Overlay>>) {
    for MenuActivated(a) in reader.read() {
        match a {
            MenuAction::OpenModeCreator => next.set(Overlay::ModeCreator),
            MenuAction::CreatorRooms => next.set(Overlay::MapBuilder),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn creator_actions(
    mut reader: MessageReader<MenuActivated>,
    keys: Res<ButtonInput<KeyCode>>,
    typing: Res<super::play_menu::Typing>,
    mut ed: ResMut<ModeEditor>,
    mut active: ResMut<ActivePreset>,
    mut seed: ResMut<RunSeed>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut net: MessageWriter<NetRequest>,
    profile: Res<crate::profile::Profile>,
) {
    if keys.just_pressed(KeyCode::Escape) && !typing.0 {
        next_overlay.set(Overlay::None);
    }
    let cards: Vec<Card> = Card::all().into_iter().flat_map(|(_, c)| c).collect();
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::CloseModeCreator => next_overlay.set(Overlay::None),
            MenuAction::CreatorCard(i, d) => {
                if let Some(c) = cards.get(i as usize) {
                    c.cycle(&mut ed.mode, d as i32);
                }
            }
            MenuAction::CreatorTemplate(d) => {
                let n = ed.list.len().max(1);
                ed.index = (ed.index as i32 + d as i32).rem_euclid(n as i32) as usize;
                if let Some(e) = ed.list.get(ed.index).cloned() {
                    let name = if e.path.is_some() { e.preset.name.clone() } else { format!("My {}", e.preset.name) };
                    ed.mode = Preset { name, ..e.preset.clone() };
                    ed.status = format!("Started from \"{}\".", e.preset.name);
                }
            }
            MenuAction::CreatorPlay => {
                let p = ed.mode.clone().sanitized();
                *seed = RunSeed(random_seed());
                active.0 = Some(p);
                next_overlay.set(Overlay::None);
                next_app.set(AppState::Playing);
            }
            MenuAction::CreatorHost => {
                let p = ed.mode.clone().sanitized();
                net.write(NetRequest::Host { name: crate::profile::clean_name(&profile.name), seed: random_seed(), preset: Some(p) });
                ed.status = "Creating a room for your mode... (a sleeping free server can take up to a minute to wake up)".into();
            }
            MenuAction::CreatorSave => {
                let p = ed.mode.clone().sanitized();
                ed.status = match presets::save(&presets::presets_dir(), &p) {
                    Ok(path) => format!("Saved \"{}\" to {}  -  it's in Custom Floors too.", p.name, path.display()),
                    Err(e) => format!("Couldn't save: {e}"),
                };
            }
            MenuAction::CreatorUseCode => {
                let code = ed.code.clone();
                match Preset::from_share_code(&code) {
                    Some(p) => {
                        ed.status = format!("Loaded the shared mode ({}). Tap Play to try it.", p.difficulty_name());
                        ed.mode = p;
                    }
                    None => ed.status = "That code doesn't look right: check it letter by letter (it starts with HW-).".into(),
                }
            }
            _ => continue,
        }
        ed.version = ed.version.wrapping_add(1);
    }
}

fn redraw(mut commands: Commands, theme: Res<UiTheme>, ed: Res<ModeEditor>, roots: Query<Entity, With<CreatorRoot>>, focus: Res<InputFocus>, orders: Query<&Focusable>, mut drawn: Local<u32>) {
    if *drawn == ed.version && !roots.is_empty() {
        return;
    }
    *drawn = ed.version;
    let keep = focus.get().and_then(|e| orders.get(e).ok()).map(|f| f.0).or(Some(100));
    for r in &roots {
        commands.entity(r).despawn();
    }
    let t = theme.clone();
    let p = &ed.mode;
    commands
        .spawn((t.screen_root(true), GlobalZIndex(20), DespawnOnExit(Screen::ModeCreator), CreatorRoot, Name::new("Mode creator")))
        .with_children(|root| {
            root.spawn(t.heading_text("MODE CREATOR"));
            // Name, difficulty meter and a template to start from.
            root.spawn(Node { column_gap: px(16), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn(Node { width: px(340), ..default() }).with_children(|n| {
                    field_row(n, &t, FieldKind::ModeName, &p.name, 1);
                });
                let d = p.difficulty();
                let color = match d {
                    0..=20 => Color::srgb(0.4, 0.85, 0.5),
                    21..=40 => Color::srgb(0.9, 0.85, 0.4),
                    41..=60 => Color::srgb(1.0, 0.6, 0.25),
                    61..=80 => Color::srgb(0.95, 0.3, 0.25),
                    _ => Color::srgb(0.8, 0.2, 0.9),
                };
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|m| {
                    m.spawn((Text::new(format!("DIFFICULTY: {}", p.difficulty_name())), t.font(true, t.size_small), TextColor(color)));
                    m.spawn((Node { width: px(220), height: px(10), ..default() }, BackgroundColor(Color::srgb(0.15, 0.15, 0.18)))).with_children(|bar| {
                        bar.spawn((Node { width: percent(d.max(4) as f32), height: percent(100), ..default() }, BackgroundColor(color)));
                    });
                });
                let from = ed.list.get(ed.index).map_or("Classic".to_string(), |e| e.preset.name.clone());
                small(row, &t, &format!("Start from: {from}"), MenuAction::CreatorTemplate(1), 2, 330.0, keep);
            });
            // The cards.
            root.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|area| {
                let mut i = 0usize;
                for (group, cards) in Card::all() {
                    area.spawn((Text::new(group), t.font(true, t.size_small), TextColor(t.interact)));
                    area.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), width: px(1060), ..default() }).with_children(|grid| {
                        for c in cards {
                            let e = spawn_button(grid, &t, "", MenuAction::CreatorCard(i as u8, 1), 100 + i as i32, keep == Some(100 + i as i32));
                            let lit = c.lit(p);
                            grid.commands().entity(e).entry::<Node>().and_modify(|mut n| {
                                n.width = px(166);
                                n.min_width = px(166);
                                n.height = px(64);
                                n.flex_direction = FlexDirection::Column;
                                n.justify_content = JustifyContent::Center;
                                n.align_items = AlignItems::Center;
                                n.row_gap = px(2);
                                n.padding = UiRect::all(px(6));
                            });
                            grid.commands().entity(e).despawn_related::<Children>().with_children(|card| {
                                card.spawn((Text::new(c.title().to_uppercase()), t.font(true, t.size_small), TextColor(if lit { t.text } else { t.text_dim }), Pickable::IGNORE));
                                card.spawn((Text::new(c.value(p)), t.font(true, t.size_body), TextColor(if lit { t.interact } else { t.text_dim }), Pickable::IGNORE));
                                card.spawn((Text::new(c.hint()), t.font(false, t.size_small * 0.8), TextColor(t.text_dim), Pickable::IGNORE));
                            });
                            i += 1;
                        }
                    });
                }
            });
            // Share code.
            root.spawn(Node { column_gap: px(12), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn((Text::new(format!("SHARE CODE:  {}", p.share_code())), t.font(true, t.size_body), TextColor(Color::srgb(1.0, 0.85, 0.4))));
                row.spawn(Node { width: px(300), ..default() }).with_children(|n| {
                    field_row(n, &t, FieldKind::ShareCode, &ed.code, 60);
                });
                small(row, &t, "Use Code", MenuAction::CreatorUseCode, 61, 130.0, keep);
            });
            root.spawn((Text::new(ed.status.clone()), t.font(false, t.size_small), TextColor(t.text_dim), Node { max_width: px(1100), ..default() }));
            // Actions.
            root.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, max_width: px(1200), ..default() }).with_children(|row| {
                for (j, (label, action)) in [
                    ("Play", MenuAction::CreatorPlay),
                    ("Host Room", MenuAction::CreatorHost),
                    ("Save", MenuAction::CreatorSave),
                    ("Build Rooms", MenuAction::CreatorRooms),
                    ("Back", MenuAction::CloseModeCreator),
                ]
                .into_iter()
                .enumerate()
                {
                    small(row, &t, label, action, 70 + j as i32, 150.0, keep);
                }
            });
        });
}

fn small(parent: &mut ChildSpawnerCommands, t: &UiTheme, label: &str, action: MenuAction, order: i32, w: f32, keep: Option<i32>) {
    let e = spawn_button(parent, t, label, action, order, keep == Some(order));
    parent.commands().entity(e).entry::<Node>().and_modify(move |mut n| {
        n.width = px(w);
        n.min_width = px(w);
        n.min_height = px(36);
        n.padding = UiRect::axes(px(6), px(4));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_card_cycles_back_to_where_it_started() {
        let base = Preset { name: "x".into(), doors: 40, ..Preset::default() };
        for (_, cards) in Card::all() {
            for c in cards {
                let mut p = base.clone();
                let start = c.value(&p);
                let mut seen = vec![start.clone()];
                for _ in 0..8 {
                    c.cycle(&mut p, 1);
                    seen.push(c.value(&p));
                    if c.value(&p) == start {
                        break;
                    }
                }
                assert_eq!(c.value(&p), start, "{c:?} wraps: {seen:?}");
                assert!(seen.len() > 2 || matches!(c, Card::Area(_)), "{c:?} has choices");
            }
        }
    }
}
