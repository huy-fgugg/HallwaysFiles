//! Mode Creator: the easy way to make your own mode. Every setting is a big
//! card you tap to change (length, areas, each monster, lights, tricks,
//! supplies); a meter shows how hard it is; play it, host it, save it, or
//! share it with a short code. "Build Rooms" opens the room-by-room builder.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use super::focus::{spawn_button, FocusPalette, Focusable, MenuAction, MenuActivated};
use super::play_menu::{field_row, random_seed, FieldKind};
use crate::net::NetRequest;
use crate::presets::{self, ActivePreset, Entry, Preset, ENTITY_NAMES, KIT_NAMES, LEVEL_NAMES, MONSTER_COLORS};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

/// One setting row on the creator screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    Length,
    StartDoor,
    RoomSize,
    Structures,
    Locks,
    Chases,
    Order,
    Area(usize),
    Monster(usize),
    MonsterOn,
    MonsterName,
    MonsterColor,
    MonsterSpeedMin,
    MonsterSpeedMax,
    MonsterPasses,
    MonsterDamage,
    MonsterWarning,
    MonsterChance,
    MonsterFirst,
    Structure(usize),
    ReadingRoom,
    BuildRooms,
    Brightness,
    DarkRooms,
    FakeDoors,
    Health,
    Speed,
    HideTime,
    Revives,
    Sprint,
    Kit,
    Supplies,
    Lumens,
    Item(usize),
}

/// How a setting is shown and changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// A dark box with the value and a chevron: tap for the next choice.
    Select,
    /// A cream ON / dark OFF pill.
    Toggle,
    /// A text box.
    Field,
    /// A button that opens something.
    Button,
}

const LENGTHS: [u32; 7] = [15, 20, 30, 40, 60, 80, 100];
const STARTS: [u32; 10] = [0, 5, 10, 20, 30, 40, 50, 60, 70, 80];
const SIZES: [(u32, &str); 4] = [(70, "Small"), (100, "Normal"), (130, "Big"), (160, "Huge")];
const BRIGHT: [(u32, &str); 4] = [(140, "Bright"), (100, "Normal"), (60, "Dim"), (30, "Pitch black")];
const DARK: [(u32, &str); 4] = [(0, "None"), (100, "Some"), (200, "Many"), (300, "Most")];
const FAKES: [(u32, &str); 4] = [(0, "Off"), (100, "Some"), (200, "Many"), (300, "Everywhere")];
const STRUCT: [(u32, &str); 4] = [(0, "None"), (25, "Few"), (100, "Normal"), (250, "Many")];
const LOOT: [(u32, &str); 4] = [(0, "Empty"), (50, "Scarce"), (100, "Normal"), (200, "Plenty")];
const LOCKS: [(u32, &str); 4] = [(0, "Never"), (50, "Rarely"), (100, "Normal"), (250, "Often")];
const HEALTH: [(u32, &str); 6] = [(50, "50 HP"), (75, "75 HP"), (100, "100 HP"), (150, "150 HP"), (200, "200 HP"), (400, "400 HP")];
const SPEEDS: [(u32, &str); 5] = [(70, "Slow"), (85, "Slower"), (100, "Normal"), (115, "Faster"), (130, "Fast")];
const HIDES: [(u32, &str); 5] = [(50, "Short"), (100, "Normal"), (200, "Long"), (400, "Very long"), (0, "No limit")];
const LUMENS: [(u32, &str); 5] = [(0, "None"), (50, "Half"), (100, "Normal"), (200, "Double"), (400, "x4")];
const READING: [u32; 8] = [0, 15, 20, 30, 40, 50, 60, 80];
const M_SPEEDS: [u8; 7] = [8, 12, 16, 20, 24, 30, 36];
const M_DAMAGE: [u8; 5] = [25, 40, 60, 80, 100];
const M_WARN: [u8; 5] = [15, 25, 40, 60, 80];
const M_CHANCE: [u8; 7] = [5, 10, 15, 25, 40, 60, 100];
const M_FIRST: [u32; 6] = [5, 10, 20, 30, 40, 60];
const MONSTER_HINTS: [&str; 6] = ["Hide from it", "Stay hidden", "Look at it", "Look away", "Stand still", "Crouch, be quiet"];

/// Index of the option closest to `v`.
fn nearest(opts: &[(u32, &str)], v: u32) -> usize {
    opts.iter().enumerate().min_by_key(|(_, (x, _))| x.abs_diff(v)).map_or(0, |(i, _)| i)
}

fn nearest_n<T: Copy + Into<u32>>(opts: &[T], v: T) -> usize {
    let v: u32 = v.into();
    opts.iter().enumerate().min_by_key(|(_, x)| (**x).into().abs_diff(v)).map_or(0, |(i, _)| i)
}

fn step_n<T: Copy + Into<u32>>(opts: &[T], v: T, d: i32) -> T {
    opts[(nearest_n(opts, v) as i32 + d).rem_euclid(opts.len() as i32) as usize]
}

/// One tab: its name, a title and subtitle, and groups of settings.
pub struct Tab {
    pub name: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub groups: Vec<(&'static str, Vec<Setting>)>,
}

pub fn tabs() -> Vec<Tab> {
    use Setting::*;
    vec![
        Tab {
            name: "Floor",
            title: "Floor",
            subtitle: "How long the floor is, where you start and how the doors behave.",
            groups: vec![("Layout", vec![Length, StartDoor, RoomSize, Structures]), ("Doors & chases", vec![Locks, Chases, Order])],
        },
        Tab {
            name: "Areas",
            title: "Areas",
            subtitle: "Which parts of the hotel the floor goes through. They share the floor evenly.",
            groups: vec![("Areas in this mode", (0..6).map(Area).collect()), ("Order", vec![Order])],
        },
        Tab {
            name: "Monsters",
            title: "Monsters",
            subtitle: "How often each monster comes. Off takes it out completely.",
            groups: vec![("Monsters", (0..6).map(Monster).collect())],
        },
        Tab {
            name: "Your Monster",
            title: "Your Monster",
            subtitle: "Make your own monster. It rushes through rooms like the Surge: hide when it warns you.",
            groups: vec![
                ("Look", vec![MonsterOn, MonsterName, MonsterColor]),
                ("Behaviour", vec![MonsterSpeedMin, MonsterSpeedMax, MonsterPasses, MonsterDamage, MonsterWarning, MonsterChance, MonsterFirst]),
            ],
        },
        Tab {
            name: "Rooms",
            title: "Room Rules",
            subtitle: "How often each structure turns up, and where the Reading Hall is.",
            groups: vec![("Structures", (0..5).map(Structure).collect()), ("Special rooms", vec![ReadingRoom, BuildRooms])],
        },
        Tab {
            name: "Lights",
            title: "Lights & Tricks",
            subtitle: "How dark it gets, and the doors that lie.",
            groups: vec![("Lights", vec![Brightness, DarkRooms]), ("Tricks", vec![FakeDoors])],
        },
        Tab {
            name: "Player",
            title: "Player",
            subtitle: "Health, speed, hiding and second chances.",
            groups: vec![("Body", vec![Health, Speed, Sprint]), ("Survival", vec![HideTime, Revives, Kit])],
        },
        Tab {
            name: "Supplies",
            title: "Supplies",
            subtitle: "What you find in drawers, and the Lumens you take home.",
            groups: vec![("Loot", vec![Supplies, Lumens]), ("Item pool", (0..5).map(Item).collect())],
        },
        Tab { name: "Help", title: "Help", subtitle: "How the Mode Creator works.", groups: vec![] },
    ]
}

pub const HELP: [&str; 7] = [
    "1.  Pick a tab at the top. Each row is one setting: tap the box on the right to change it (the < goes back).",
    "2.  Cream pills are ON, dark pills are OFF. Tap a group bar to fold it away.",
    "3.  The meter at the top shows how hard your mode is.",
    "4.  PLAY tries it alone. HOST ROOM opens an online room with your mode for up to 4 players.",
    "5.  SAVE keeps it in Custom Floors. The SHARE CODE carries every setting: send it to a friend and they type it in and press Use Code.",
    "6.  YOUR MONSTER: switch it on, give it a name and colour, then set how fast, how often and how hard it hits.",
    "7.  BUILD ROOMS opens the Map Builder to lay out every room yourself. Player-made modes and maps always say so on screen.",
];

/// Every setting in tab order (the index used by [`MenuAction::CreatorCard`]).
pub fn all_settings() -> Vec<Setting> {
    tabs().into_iter().flat_map(|t| t.groups.into_iter().flat_map(|(_, s)| s)).collect()
}

/// Every group name in tab order (the index used by [`MenuAction::CreatorGroup`]).
fn all_groups() -> Vec<String> {
    tabs().into_iter().flat_map(|t| t.groups.into_iter().map(move |(g, _)| format!("{}/{g}", t.name))).collect()
}

fn item_name(i: usize) -> &'static str {
    ["Flashlight", "Lockpick", "Vitamins", "Bandage", "Skeleton key"][i]
}

impl Setting {
    pub fn control(self) -> Control {
        use Setting::*;
        match self {
            Chases | Area(_) | MonsterOn | Sprint | Item(_) => Control::Toggle,
            MonsterName => Control::Field,
            BuildRooms => Control::Button,
            _ => Control::Select,
        }
    }

    pub fn title(self) -> String {
        use Setting::*;
        match self {
            Length => "Length".into(),
            StartDoor => "Start at".into(),
            RoomSize => "Room size".into(),
            Structures => "Structures".into(),
            Locks => "Locked doors".into(),
            Chases => "Hollow chases".into(),
            Order => "Area order".into(),
            Area(i) => crate::floor::plan::Area::ALL[i].name().trim_start_matches("The ").to_string(),
            Monster(i) => ENTITY_NAMES[i].to_string(),
            MonsterOn => "Use my monster".into(),
            MonsterName => "Name".into(),
            MonsterColor => "Colour".into(),
            MonsterSpeedMin => "Slowest speed".into(),
            MonsterSpeedMax => "Fastest speed".into(),
            MonsterPasses => "Passes".into(),
            MonsterDamage => "Damage".into(),
            MonsterWarning => "Warning".into(),
            MonsterChance => "Spawn chance".into(),
            MonsterFirst => "First room".into(),
            Structure(i) => presets::STRUCTURE_NAMES[i].into(),
            ReadingRoom => "Reading Hall".into(),
            BuildRooms => "Hand-built rooms".into(),
            Brightness => "Lights".into(),
            DarkRooms => "Dark rooms".into(),
            FakeDoors => "Fake doors".into(),
            Health => "Health".into(),
            Speed => "Walk speed".into(),
            HideTime => "Hiding time".into(),
            Revives => "Revives".into(),
            Sprint => "Sprinting".into(),
            Kit => "Starting items".into(),
            Supplies => "Supplies".into(),
            Lumens => "Lumens earned".into(),
            Item(i) => item_name(i).into(),
        }
    }

    pub fn hint(self) -> &'static str {
        use Setting::*;
        match self {
            Length => "Doors to the end",
            StartDoor => "Skip the first doors",
            RoomSize => "How big rooms are",
            Structures => "Stairs, lofts, mazes overall",
            Locks => "Exits needing a key",
            Chases => "The Hollow hunts you",
            Order => "Shuffled each run, or as listed",
            Area(_) => "Include this area",
            Monster(i) => MONSTER_HINTS[i],
            MonsterOn => "Adds it to the floor",
            MonsterName => "Shown when it comes",
            MonsterColor => "Its glow and the room's tint",
            MonsterSpeedMin | MonsterSpeedMax => "Each visit picks in between",
            MonsterPasses => "Times it comes back",
            MonsterDamage => "When it catches you",
            MonsterWarning => "Time to find a hiding spot",
            MonsterChance => "Per room with a wardrobe",
            MonsterFirst => "Rooms before stay safe",
            Structure(_) => "How often it appears",
            ReadingRoom => "Room with tall shelves + key",
            BuildRooms => "Lay out every room yourself",
            Brightness => "Lamp brightness",
            DarkRooms => "Rooms with no lamp",
            FakeDoors => "Doors that hurt",
            Health => "Damage scales to match",
            Speed => "How fast you move",
            HideTime => "Before the cupboard kicks you out",
            Revives => "Get back up after dying",
            Sprint => "Allow running",
            Kit => "What you start with",
            Supplies => "Loot in drawers",
            Lumens => "Shop money from this mode",
            Item(_) => "Can turn up in drawers",
        }
    }

    pub fn value(self, p: &Preset) -> String {
        use Setting::*;
        let m = &p.custom;
        match self {
            Length => format!("{} doors", p.doors),
            StartDoor => if p.start_door == 0 { "The beginning".into() } else { format!("Door {:03}", p.start_door) },
            RoomSize => SIZES[nearest(&SIZES, p.room_size)].1.into(),
            Structures => STRUCT[nearest(&STRUCT, p.stair_rooms)].1.into(),
            Locks => LOCKS[nearest(&LOCKS, p.locks)].1.into(),
            Chases | MonsterOn | Sprint | Area(_) | Item(_) => if self.lit(p) { "ON".into() } else { "OFF".into() },
            Order => if p.shuffle_areas { "Shuffled".into() } else { "As listed".into() },
            Monster(i) => LEVEL_NAMES[p.level(i) as usize].into(),
            MonsterName => m.name.clone(),
            MonsterColor => MONSTER_COLORS[m.color as usize % MONSTER_COLORS.len()].0.into(),
            MonsterSpeedMin => format!("{} m/s", m.speed_min),
            MonsterSpeedMax => format!("{} m/s", m.speed_max),
            MonsterPasses => match m.passes {
                0 => "Once".into(),
                n => format!("Back {n}x"),
            },
            MonsterDamage => if m.damage >= 100 { "Kills".into() } else { format!("{} HP", m.damage) },
            MonsterWarning => format!("{:.1} s", f32::from(m.warning) / 10.0),
            MonsterChance => format!("{}%", m.chance),
            MonsterFirst => format!("Room {:03}", m.first_room),
            Structure(i) => LEVEL_NAMES[p.structures[i] as usize].into(),
            ReadingRoom => if p.reading_room == 0 { "Off".into() } else { format!("Room {:03}", p.reading_room) },
            BuildRooms => "Open Builder".into(),
            Brightness => BRIGHT[nearest(&BRIGHT, p.brightness)].1.into(),
            DarkRooms => DARK[nearest(&DARK, p.dark_rooms)].1.into(),
            FakeDoors => FAKES[nearest(&FAKES, p.fake_doors)].1.into(),
            Health => HEALTH[nearest(&HEALTH, p.health)].1.into(),
            Speed => SPEEDS[nearest(&SPEEDS, p.speed)].1.into(),
            HideTime => HIDES[nearest(&HIDES, p.hide_time)].1.into(),
            Revives => if p.revives == 0 { "None".into() } else { format!("{}", p.revives) },
            Kit => KIT_NAMES[p.start_kit as usize % KIT_NAMES.len()].into(),
            Supplies => LOOT[nearest(&LOOT, p.loot)].1.into(),
            Lumens => LUMENS[nearest(&LUMENS, p.lumens)].1.into(),
        }
    }

    /// Whether a toggle is on (other settings: always "lit").
    pub fn lit(self, p: &Preset) -> bool {
        use Setting::*;
        match self {
            Chases => p.chases,
            MonsterOn => p.custom.on,
            Sprint => p.sprint,
            Area(i) => p.areas.contains(&crate::floor::plan::Area::ALL[i]),
            Item(i) => p.items_off & (1 << i) == 0,
            Monster(i) => p.level(i) > 0,
            _ => true,
        }
    }

    /// Steps to the next (or previous) choice, wrapping.
    pub fn cycle(self, p: &mut Preset, d: i32) {
        use Setting::*;
        let step = |opts: &[(u32, &str)], v: u32| opts[(nearest(opts, v) as i32 + d).rem_euclid(opts.len() as i32) as usize].0;
        match self {
            Length => {
                p.doors = step_n(&LENGTHS, p.doors, d);
                p.start_door = p.start_door.min(p.doors.saturating_sub(5));
            }
            StartDoor => {
                let opts: Vec<u32> = STARTS.iter().copied().filter(|s| *s + 5 <= p.doors).collect();
                p.start_door = step_n(&opts, p.start_door, d);
            }
            RoomSize => p.room_size = step(&SIZES, p.room_size),
            Structures => p.stair_rooms = step(&STRUCT, p.stair_rooms),
            Locks => p.locks = step(&LOCKS, p.locks),
            Chases => p.chases = !p.chases,
            Order => p.shuffle_areas = !p.shuffle_areas,
            Area(i) => p.toggle_area(crate::floor::plan::Area::ALL[i]),
            Monster(i) => {
                let l = (p.level(i) as i32 + d).rem_euclid(LEVEL_NAMES.len() as i32) as u8;
                p.set_level(i, l);
            }
            MonsterOn => p.custom.on = !p.custom.on,
            MonsterName => {}
            MonsterColor => p.custom.color = (p.custom.color as i32 + d).rem_euclid(MONSTER_COLORS.len() as i32) as u8,
            MonsterSpeedMin => {
                p.custom.speed_min = step_n(&M_SPEEDS, p.custom.speed_min, d);
                p.custom.speed_max = p.custom.speed_max.max(p.custom.speed_min);
            }
            MonsterSpeedMax => {
                p.custom.speed_max = step_n(&M_SPEEDS, p.custom.speed_max, d);
                p.custom.speed_min = p.custom.speed_min.min(p.custom.speed_max);
            }
            MonsterPasses => p.custom.passes = (p.custom.passes as i32 + d).rem_euclid(6) as u8,
            MonsterDamage => p.custom.damage = step_n(&M_DAMAGE, p.custom.damage, d),
            MonsterWarning => p.custom.warning = step_n(&M_WARN, p.custom.warning, d),
            MonsterChance => p.custom.chance = step_n(&M_CHANCE, p.custom.chance, d),
            MonsterFirst => p.custom.first_room = step_n(&M_FIRST, p.custom.first_room, d),
            Structure(i) => p.structures[i] = (p.structures[i] as i32 + d).rem_euclid(LEVEL_NAMES.len() as i32) as u8,
            ReadingRoom => p.reading_room = step_n(&READING, p.reading_room, d),
            BuildRooms => {}
            Brightness => p.brightness = step(&BRIGHT, p.brightness),
            DarkRooms => p.dark_rooms = step(&DARK, p.dark_rooms),
            FakeDoors => p.fake_doors = step(&FAKES, p.fake_doors),
            Health => p.health = step(&HEALTH, p.health),
            Speed => p.speed = step(&SPEEDS, p.speed),
            HideTime => p.hide_time = step(&HIDES, p.hide_time),
            Revives => p.revives = (p.revives as i32 + d).rem_euclid(6) as u8,
            Sprint => p.sprint = !p.sprint,
            Kit => p.start_kit = (p.start_kit as i32 + d).rem_euclid(KIT_NAMES.len() as i32) as u8,
            Supplies => p.loot = step(&LOOT, p.loot),
            Lumens => p.lumens = step(&LUMENS, p.lumens),
            Item(i) => p.items_off ^= 1 << i,
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
    /// Open tab.
    pub tab: usize,
    /// Folded group bars ("Tab/Group").
    pub collapsed: Vec<String>,
}

impl ModeEditor {
    fn load() -> Self {
        let list: Vec<Entry> = presets::load_all(&presets::presets_dir()).into_iter().filter(|e| e.preset.rooms.is_empty()).collect();
        let mut mode = Preset { name: "My Mode".into(), ..Preset::default() };
        mode.doors = 40;
        let tab = std::env::var("HALLWAYS_CREATOR_TAB").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        Self { mode, list, index: 0, code: String::new(), status: "Pick a tab, tap a box to change it, then Play, Host or Save.".into(), version: 1, tab, collapsed: Vec::new() }
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
    let cards = all_settings();
    let groups = all_groups();
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::CloseModeCreator => next_overlay.set(Overlay::None),
            MenuAction::CreatorCard(i, d) => {
                if let Some(c) = cards.get(i as usize) {
                    c.cycle(&mut ed.mode, d as i32);
                }
            }
            MenuAction::CreatorTab(i) => ed.tab = (i as usize).min(tabs().len() - 1),
            MenuAction::CreatorGroup(i) => {
                if let Some(g) = groups.get(i as usize) {
                    if let Some(k) = ed.collapsed.iter().position(|c| c == g) {
                        ed.collapsed.remove(k);
                    } else {
                        ed.collapsed.push(g.clone());
                    }
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

/// The creator's brown and cream colours.
mod pal {
    use bevy::prelude::Color;
    pub const BG: Color = Color::srgb(0.15, 0.095, 0.065);
    pub const PANEL: Color = Color::srgb(0.27, 0.175, 0.12);
    pub const ROW: Color = Color::srgb(0.33, 0.22, 0.155);
    pub const GROUP: Color = Color::srgb(0.41, 0.275, 0.19);
    pub const BOX: Color = Color::srgb(0.11, 0.07, 0.045);
    pub const BOX_FOCUS: Color = Color::srgb(0.19, 0.12, 0.08);
    pub const EDGE: Color = Color::srgb(0.47, 0.33, 0.23);
    pub const CREAM: Color = Color::srgb(0.96, 0.86, 0.72);
    pub const CREAM_HI: Color = Color::srgb(1.0, 0.94, 0.84);
    pub const TEXT: Color = Color::srgb(0.97, 0.91, 0.83);
    pub const DIM: Color = Color::srgb(0.76, 0.64, 0.53);
    pub const INK: Color = Color::srgb(0.2, 0.12, 0.08);
}

fn dark_palette() -> FocusPalette {
    FocusPalette { bg: pal::BOX, border: pal::EDGE, text: pal::TEXT, bg_focus: pal::BOX_FOCUS, border_focus: pal::CREAM, text_focus: pal::CREAM_HI }
}

fn cream_palette() -> FocusPalette {
    FocusPalette { bg: pal::CREAM, border: pal::CREAM, text: pal::INK, bg_focus: pal::CREAM_HI, border_focus: Color::WHITE, text_focus: pal::INK }
}

fn group_palette() -> FocusPalette {
    FocusPalette { bg: pal::GROUP, border: pal::GROUP, text: pal::CREAM, bg_focus: Color::srgb(0.5, 0.34, 0.24), border_focus: pal::CREAM, text_focus: pal::CREAM_HI }
}

/// A rounded button in the creator's colours. The label is the first child
/// (it follows focus); `extra` adds more children after it.
#[allow(clippy::too_many_arguments)]
fn btn(parent: &mut ChildSpawnerCommands, t: &UiTheme, label: &str, size: f32, action: MenuAction, order: i32, keep: Option<i32>, w: f32, h: f32, palette: FocusPalette, extra: impl FnOnce(&mut ChildSpawnerCommands) + Send + Sync + 'static) -> Entity {
    let e = spawn_button(parent, t, "", action, order, keep == Some(order));
    let label = label.to_string();
    let font = t.font(true, size);
    parent.commands().entity(e).insert((palette, BackgroundColor(palette.bg), BorderColor::all(palette.border))).entry::<Node>().and_modify(move |mut n| {
        n.width = px(w);
        n.min_width = px(w);
        n.height = px(h);
        n.padding = UiRect::axes(px(12), px(2));
        n.column_gap = px(8);
        n.border = UiRect::all(px(2));
        n.border_radius = BorderRadius::all(px((h * 0.5).min(10.0)));
    });
    parent.commands().entity(e).despawn_related::<Children>().with_children(move |b| {
        b.spawn((Text::new(label), font, TextColor(palette.text), TextLayout::no_wrap(), Pickable::IGNORE));
        extra(b);
    });
    e
}

/// A small "v" chevron drawn from two bars (`open` = pointing down, else right).
fn chevron(b: &mut ChildSpawnerCommands, color: Color, open: bool) {
    let turn = if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 };
    b.spawn((Node { width: px(14), height: px(14), ..default() }, UiTransform { rotation: Rot2::radians(turn), ..default() }, Pickable::IGNORE)).with_children(|c| {
        for (x, a) in [(0.0f32, 0.7f32), (5.0, -0.7)] {
            c.spawn((
                Node { position_type: PositionType::Absolute, left: px(x), top: px(5), width: px(9), height: px(3), border_radius: BorderRadius::all(px(2)), ..default() },
                UiTransform { rotation: Rot2::radians(a), ..default() },
                BackgroundColor(color),
                Pickable::IGNORE,
            ));
        }
    });
}

const PANEL_W: f32 = 1440.0;
const COL_W: f32 = 700.0;

fn redraw(mut commands: Commands, theme: Res<UiTheme>, ed: Res<ModeEditor>, roots: Query<Entity, With<CreatorRoot>>, focus: Res<InputFocus>, orders: Query<&Focusable>, mut drawn: Local<u32>) {
    if *drawn == ed.version && !roots.is_empty() {
        return;
    }
    *drawn = ed.version;
    let keep = focus.get().and_then(|e| orders.get(e).ok()).map(|f| f.0).or(Some(10 + ed.tab as i32));
    for r in &roots {
        commands.entity(r).despawn();
    }
    let t = theme.clone();
    let p = &ed.mode;
    let all = tabs();
    let tab = &all[ed.tab.min(all.len() - 1)];
    // Flat indices of this tab's settings and groups.
    let first_setting: usize = all[..ed.tab].iter().map(|t| t.groups.iter().map(|(_, s)| s.len()).sum::<usize>()).sum();
    let first_group: usize = all[..ed.tab].iter().map(|t| t.groups.len()).sum();
    commands
        .spawn((
            Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: px(12), ..default() },
            BackgroundColor(pal::BG),
            crate::theme::ScreenRoot,
            GlobalZIndex(20),
            DespawnOnExit(Screen::ModeCreator),
            CreatorRoot,
            Name::new("Mode creator"),
        ))
        .with_children(|root| {
            // ---- Header: title, name, difficulty, template ----
            root.spawn(Node { width: px(PANEL_W), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: px(16), ..default() }).with_children(|row| {
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|h| {
                    h.spawn((Text::new("MODE CREATOR"), t.font(true, t.size_heading), TextColor(pal::CREAM)));
                    h.spawn((Text::new("Make a mode, play it, share it."), t.font(false, t.size_small), TextColor(pal::DIM)));
                });
                row.spawn(Node { column_gap: px(16), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    r.spawn(Node { width: px(360), ..default() }).with_children(|n| {
                        let f = field_row(n, &t, FieldKind::ModeName, &p.name, 1);
                        n.commands().entity(f).insert(dark_palette());
                    });
                    let d = p.difficulty();
                    let color = match d {
                        0..=20 => Color::srgb(0.5, 0.85, 0.45),
                        21..=40 => Color::srgb(0.95, 0.85, 0.45),
                        41..=60 => Color::srgb(1.0, 0.62, 0.3),
                        61..=80 => Color::srgb(0.95, 0.35, 0.25),
                        _ => Color::srgb(0.85, 0.3, 0.85),
                    };
                    r.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(5), width: px(230), ..default() }).with_children(|m| {
                        m.spawn((Text::new(format!("DIFFICULTY: {}", p.difficulty_name())), t.font(true, t.size_small), TextColor(color), TextLayout::no_wrap()));
                        m.spawn((Node { width: px(220), height: px(12), border_radius: BorderRadius::all(px(6)), ..default() }, BackgroundColor(pal::BOX))).with_children(|bar| {
                            bar.spawn((Node { width: percent(d.max(5) as f32), height: percent(100), border_radius: BorderRadius::all(px(6)), ..default() }, BackgroundColor(color)));
                        });
                    });
                    let from = ed.list.get(ed.index).map_or("Classic".to_string(), |e| e.preset.name.clone());
                    btn(r, &t, &format!("Start from: {from}"), t.size_small, MenuAction::CreatorTemplate(1), 2, keep, 330.0, 44.0, dark_palette(), |_| {});
                });
            });
            // ---- Tabs ----
            root.spawn(Node { width: px(PANEL_W), column_gap: px(6), ..default() }).with_children(|row| {
                let n = all.len() as f32;
                let w = (PANEL_W - 6.0 * (n - 1.0)) / n;
                for (i, tb) in all.iter().enumerate() {
                    let pal = if i == ed.tab { cream_palette() } else { FocusPalette { bg: pal::PANEL, border: pal::PANEL, ..dark_palette() } };
                    let e = btn(row, &t, &tb.name.to_uppercase(), t.size_small, MenuAction::CreatorTab(i as u8), 10 + i as i32, keep, w, 46.0, pal, |_| {});
                    row.commands().entity(e).entry::<Node>().and_modify(|mut n| {
                        n.justify_content = JustifyContent::Center;
                        n.padding = UiRect::axes(px(4), px(2));
                    });
                }
            });
            // ---- The panel ----
            root.spawn((
                Node { width: px(PANEL_W), height: px(600), flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::axes(px(20), px(16)), border_radius: BorderRadius::all(px(14)), overflow: Overflow::clip(), ..default() },
                BackgroundColor(pal::PANEL),
            ))
            .with_children(|panel| {
                panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), margin: UiRect::bottom(px(4)), ..default() }).with_children(|h| {
                    h.spawn((Text::new(tab.title), t.font(true, t.size_body * 1.25), TextColor(pal::CREAM)));
                    h.spawn((Text::new(tab.subtitle), t.font(false, t.size_small), TextColor(pal::DIM)));
                });
                if tab.groups.is_empty() {
                    for line in HELP {
                        panel.spawn((Text::new(line), t.font(false, t.size_small * 1.05), TextColor(pal::TEXT), Node { max_width: px(PANEL_W - 60.0), ..default() }));
                    }
                    panel.spawn((
                        Text::new("Your mode's settings are saved in its file and in its share code. Hand-built maps (Build Rooms) are saved as .hallwaysmap files you can send to friends."),
                        t.font(false, t.size_small),
                        TextColor(pal::DIM),
                        Node { max_width: px(PANEL_W - 60.0), margin: UiRect::top(px(10)), ..default() },
                    ));
                }
                let mut si = first_setting;
                for (gi, (group, settings)) in tab.groups.iter().enumerate() {
                    let key = format!("{}/{group}", tab.name);
                    let open = !ed.collapsed.contains(&key);
                    let order = 40 + (first_group + gi) as i32 * 30;
                    let gpal = group_palette();
                    let count = settings.len();
                    let small_font = t.font(false, t.size_small * 0.85);
                    let e = btn(panel, &t, &group.to_uppercase(), t.size_small, MenuAction::CreatorGroup((first_group + gi) as u8), order, keep, PANEL_W - 40.0, 34.0, gpal, move |b| {
                        b.spawn((Text::new(format!("{count} setting{}", if count == 1 { "" } else { "s" })), small_font.clone(), TextColor(pal::DIM), Node { margin: UiRect::left(Val::Auto), ..default() }, Pickable::IGNORE));
                        chevron(b, pal::CREAM, open);
                    });
                    panel.commands().entity(e).entry::<Node>().and_modify(|mut n| {
                        n.justify_content = JustifyContent::FlexStart;
                        n.border_radius = BorderRadius::all(px(8));
                    });
                    if !open {
                        si += settings.len();
                        continue;
                    }
                    panel.spawn(Node { width: px(PANEL_W - 40.0), flex_wrap: FlexWrap::Wrap, column_gap: px(PANEL_W - 40.0 - 2.0 * COL_W), row_gap: px(8), ..default() }).with_children(|grid| {
                        for (k, s) in settings.iter().enumerate() {
                            let order = order + 1 + k as i32 * 2;
                            setting_row(grid, &t, p, *s, si, order, keep);
                            si += 1;
                        }
                    });
                }
            });
            // ---- Share code and status ----
            root.spawn(Node { width: px(PANEL_W), column_gap: px(12), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn((
                    Node { padding: UiRect::axes(px(14), px(8)), border_radius: BorderRadius::all(px(10)), ..default() },
                    BackgroundColor(pal::PANEL),
                ))
                .with_children(|b| {
                    b.spawn((Text::new(format!("SHARE CODE:  {}", p.share_code())), t.font(true, t.size_small), TextColor(pal::CREAM), TextLayout::no_wrap()));
                });
                row.spawn(Node { width: px(330), ..default() }).with_children(|n| {
                    let f = field_row(n, &t, FieldKind::ShareCode, &ed.code, 600);
                    n.commands().entity(f).insert(dark_palette());
                });
                btn(row, &t, "Use Code", t.size_small, MenuAction::CreatorUseCode, 601, keep, 150.0, 44.0, dark_palette(), |_| {});
            });
            root.spawn((Text::new(ed.status.clone()), t.font(false, t.size_small), TextColor(pal::DIM), Node { width: px(PANEL_W), ..default() }));
            // ---- Actions ----
            root.spawn(Node { width: px(PANEL_W), column_gap: px(10), justify_content: JustifyContent::Center, ..default() }).with_children(|row| {
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
                    let palette = if j == 0 { cream_palette() } else { dark_palette() };
                    let e = btn(row, &t, &label.to_uppercase(), t.size_body, action, 700 + j as i32, keep, 220.0, 52.0, palette, |_| {});
                    row.commands().entity(e).entry::<Node>().and_modify(|mut n| n.justify_content = JustifyContent::Center);
                }
            });
        });
}

/// One setting: label and hint on the left, its control on the right.
fn setting_row(grid: &mut ChildSpawnerCommands, t: &UiTheme, p: &Preset, s: Setting, index: usize, order: i32, keep: Option<i32>) {
    grid.spawn((
        Node { width: px(COL_W), height: px(54), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::axes(px(16), px(4)), border_radius: BorderRadius::all(px(10)), ..default() },
        BackgroundColor(pal::ROW),
    ))
    .with_children(|row| {
        row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }).with_children(|l| {
            l.spawn((Text::new(s.title()), t.font(true, t.size_small * 1.05), TextColor(if s.lit(p) { pal::TEXT } else { pal::DIM }), TextLayout::no_wrap()));
            l.spawn((Text::new(s.hint()), t.font(false, t.size_small * 0.82), TextColor(pal::DIM), TextLayout::no_wrap()));
        });
        row.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|c| match s.control() {
            Control::Toggle => {
                let on = s.lit(p);
                let palette = if on { cream_palette() } else { dark_palette() };
                let e = btn(c, t, if on { "ON" } else { "OFF" }, t.size_small, MenuAction::CreatorCard(index as u8, 1), order, keep, 110.0, 38.0, palette, |_| {});
                c.commands().entity(e).entry::<Node>().and_modify(|mut n| {
                    n.justify_content = JustifyContent::Center;
                    n.border_radius = BorderRadius::MAX;
                });
            }
            Control::Field => {
                c.spawn(Node { width: px(300), ..default() }).with_children(|n| {
                    let f = field_row(n, t, FieldKind::MonsterName, &p.custom.name, order);
                    n.commands().entity(f).insert(dark_palette());
                });
            }
            Control::Button => {
                let e = btn(c, t, &s.value(p).to_uppercase(), t.size_small, MenuAction::CreatorRooms, order, keep, 300.0, 40.0, cream_palette(), |_| {});
                c.commands().entity(e).entry::<Node>().and_modify(|mut n| n.justify_content = JustifyContent::Center);
            }
            Control::Select => {
                let e = btn(c, t, "<", t.size_small, MenuAction::CreatorCard(index as u8, -1), order - 1, keep, 40.0, 40.0, dark_palette(), |_| {});
                c.commands().entity(e).entry::<Node>().and_modify(|mut n| {
                    n.justify_content = JustifyContent::Center;
                    n.padding = UiRect::ZERO;
                });
                let swatch = matches!(s, Setting::MonsterColor).then(|| p.custom.rgb());
                let e = btn(c, t, &s.value(p), t.size_small, MenuAction::CreatorCard(index as u8, 1), order, keep, 252.0, 40.0, dark_palette(), move |b| {
                    b.spawn(Node { flex_grow: 1.0, ..default() });
                    if let Some(col) = swatch {
                        b.spawn((Node { width: px(18), height: px(18), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(col), Pickable::IGNORE));
                    }
                    chevron(b, pal::CREAM, true);
                });
                c.commands().entity(e).entry::<Node>().and_modify(|mut n| n.justify_content = JustifyContent::FlexStart);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_cycles_back_to_where_it_started() {
        let base = Preset { name: "x".into(), doors: 40, ..Preset::default() };
        for s in all_settings() {
            if matches!(s.control(), Control::Field | Control::Button) {
                continue;
            }
            for d in [1, -1] {
                let mut p = base.clone();
                let start = s.value(&p);
                let mut seen = vec![start.clone()];
                for _ in 0..12 {
                    s.cycle(&mut p, d);
                    seen.push(s.value(&p));
                    if s.value(&p) == start {
                        break;
                    }
                }
                assert_eq!(s.value(&p), start, "{s:?} wraps: {seen:?}");
                assert!(seen.len() > 2, "{s:?} has choices");
            }
        }
    }

    #[test]
    fn every_tab_fits_the_panel() {
        // Each group bar is 34 px + 10 gap; each row of two settings 54 + 8.
        for tab in tabs() {
            let h: usize = 70 + tab.groups.iter().map(|(_, s)| 44 + s.len().div_ceil(2) * 62).sum::<usize>();
            assert!(h <= 600 - 32, "{} is {h} px tall", tab.name);
            for (_, s) in &tab.groups {
                assert!(s.len() <= 10);
            }
        }
        assert!(all_settings().len() < 255 && all_groups().len() < 30, "fits the u8 actions and focus orders");
    }
}
