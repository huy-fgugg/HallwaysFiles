//! Map Builder: lay out a floor room by room (area, size, type, lights,
//! monster, structure), see it from above, play-test it, host it online, and
//! save it as a `.hallwaysmap` file. Export puts a copy in your Downloads
//! folder to take to another computer; there, drop the file on the game
//! window (or use Import) to play or edit it.

use std::path::{Path, PathBuf};

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::window::FileDragAndDrop;

use super::focus::{spawn_button, Focusable, MenuAction, MenuActivated};
use super::play_menu::{field_row, random_seed, FieldKind};
use crate::floor::plan::{self, Area, Encounter, RoomKind};
use crate::mapfile::{starter_rooms, MapRoom, RoomField, MAP_EXTENSION, MAX_ROOMS};
use crate::net::NetRequest;
use crate::presets::{self, ActivePreset, Preset};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

/// The map being edited and the saved maps to browse.
#[derive(Resource, Debug)]
pub struct MapEditor {
    /// Saved maps (files).
    pub files: Vec<(PathBuf, Preset)>,
    /// Which file is open (None = a new, unsaved map).
    pub open: Option<usize>,
    pub map: Preset,
    pub selected: usize,
    /// Bumped on every change; the screen redraws when it moves.
    pub version: u32,
    pub status: String,
}

impl MapEditor {
    fn load() -> Self {
        let mut ed = Self { files: vec![], open: None, map: new_map("My Map"), selected: 0, version: 1, status: String::new() };
        ed.reload();
        if !ed.files.is_empty() {
            ed.open_file(0);
        } else {
            ed.map.name = "My Map".into();
        }
        ed
    }

    fn reload(&mut self) {
        self.files = presets::load_all(&presets::presets_dir()).into_iter().filter_map(|e| e.path.filter(|_| !e.preset.rooms.is_empty()).map(|p| (p, e.preset))).collect();
    }

    fn open_file(&mut self, i: usize) {
        if let Some((_, p)) = self.files.get(i) {
            self.map = p.clone();
            self.open = Some(i);
            self.selected = 0;
        }
        self.changed();
    }

    fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.selected = self.selected.min(self.map.rooms.len().saturating_sub(1));
    }

    fn room(&mut self) -> Option<&mut MapRoom> {
        let i = self.selected;
        self.map.rooms.get_mut(i)
    }

    fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.files.iter().any(|(_, p)| p.name.eq_ignore_ascii_case(n));
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).unwrap()
    }
}

fn new_map(name: &str) -> Preset {
    Preset { name: name.into(), rooms: starter_rooms(), ..Preset::default() }.sanitized()
}

#[derive(Component)]
struct BuilderRoot;

pub struct MapBuilderPlugin;

impl Plugin for MapBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::MapBuilder), open_builder)
            .add_systems(Update, (builder_actions, redraw, super::play_menu::room_ready).chain().run_if(in_state(Screen::MapBuilder)))
            .add_systems(Update, (open_from_menus, dropped_files).run_if(in_state(AppState::MainMenu)))
            // Player-made floors and maps say so on screen the whole run.
            .add_systems(OnEnter(AppState::Playing), spawn_player_made_banner)
            .add_systems(Update, show_player_made_banner.run_if(in_state(AppState::Playing)));
    }
}

fn open_builder(mut commands: Commands, old: Option<Res<MapEditor>>) {
    if old.is_none() {
        commands.insert_resource(MapEditor::load());
    }
}

fn open_from_menus(mut reader: MessageReader<MenuActivated>, mut next: ResMut<NextState<Overlay>>) {
    if reader.read().any(|MenuActivated(a)| *a == MenuAction::OpenMapBuilder) {
        next.set(Overlay::MapBuilder);
    }
}

/// Where Export puts a copy: your Downloads folder if there is one.
fn downloads_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let d = PathBuf::from(home).join("Downloads");
    d.is_dir().then_some(d)
}

/// Copies a `.hallwaysmap` (or `.hallways`) file into the maps folder and
/// returns its new path.
fn import_file(src: &Path) -> Result<PathBuf, String> {
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext != MAP_EXTENSION && ext != presets::EXTENSION {
        return Err(format!("{} isn't a .{MAP_EXTENSION} file", src.display()));
    }
    let text = std::fs::read_to_string(src).map_err(|e| e.to_string())?;
    let p = Preset::from_ron(&text).ok_or_else(|| "That file isn't a HALLWAYS map (it may be damaged).".to_string())?;
    presets::save(&presets::presets_dir(), &p).map_err(|e| e.to_string())
}

/// Dropping a map file on the window imports it and opens it in the builder.
fn dropped_files(mut drops: MessageReader<FileDragAndDrop>, mut commands: Commands, ed: Option<ResMut<MapEditor>>, mut next: ResMut<NextState<Overlay>>) {
    let mut ed = ed;
    for d in drops.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = d else { continue };
        let result = import_file(path_buf);
        let mut fresh = None;
        let e: &mut MapEditor = match ed.as_deref_mut() {
            Some(e) => e,
            None => fresh.insert(MapEditor::load()),
        };
        match result {
            Ok(path) => {
                e.reload();
                if let Some(i) = e.files.iter().position(|(p, _)| *p == path) {
                    e.open_file(i);
                }
                e.status = format!("Imported \"{}\". Play it, host it or edit it.", e.map.name);
            }
            Err(msg) => e.status = msg,
        }
        e.changed();
        if let Some(f) = fresh {
            commands.insert_resource(f);
        }
        next.set(Overlay::MapBuilder);
    }
}

#[allow(clippy::too_many_arguments)]
fn builder_actions(
    mut reader: MessageReader<MenuActivated>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ed: ResMut<MapEditor>,
    mut active: ResMut<ActivePreset>,
    mut seed: ResMut<RunSeed>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut net: MessageWriter<NetRequest>,
    profile: Res<crate::profile::Profile>,
    typing: Res<super::play_menu::Typing>,
) {
    if keys.just_pressed(KeyCode::Escape) && !typing.0 {
        next_overlay.set(Overlay::None);
    }
    for MenuActivated(a) in reader.read() {
        let mut status = None;
        match *a {
            MenuAction::CloseMapBuilder => next_overlay.set(Overlay::None),
            MenuAction::MapPrev | MenuAction::MapNext => {
                let n = ed.files.len();
                if n > 0 {
                    let cur = ed.open.unwrap_or(if *a == MenuAction::MapPrev { 0 } else { n - 1 });
                    let i = if *a == MenuAction::MapPrev { (cur + n - 1) % n } else { (cur + 1) % n };
                    ed.open_file(i);
                    status = Some(format!("Opened \"{}\".", ed.map.name));
                } else {
                    status = Some("No saved maps yet: build one and press Save.".into());
                }
            }
            MenuAction::MapNew => {
                let name = ed.unique_name("My Map");
                ed.map = new_map(&name);
                ed.open = None;
                ed.selected = 0;
                status = Some("A new map with a few rooms to start from.".into());
            }
            MenuAction::MapSelect(i) => ed.selected = i as usize,
            MenuAction::MapAdd => {
                if ed.map.rooms.len() < MAX_ROOMS {
                    let base = ed.map.rooms.get(ed.selected).copied().map(|r| MapRoom { monster: crate::mapfile::MapMonster::None, feature: crate::mapfile::MapFeature::None, ..r }).unwrap_or_default();
                    let at = (ed.selected + 1).min(ed.map.rooms.len());
                    ed.map.rooms.insert(at, base);
                    ed.selected = at;
                } else {
                    status = Some(format!("A map holds up to {MAX_ROOMS} rooms."));
                }
            }
            MenuAction::MapCopy => {
                if let Some(r) = ed.map.rooms.get(ed.selected).copied()
                    && ed.map.rooms.len() < MAX_ROOMS
                {
                    let at = ed.selected + 1;
                    ed.map.rooms.insert(at, r);
                    ed.selected = at;
                }
            }
            MenuAction::MapRemove => {
                if ed.map.rooms.len() > 1 {
                    let i = ed.selected;
                    ed.map.rooms.remove(i);
                } else {
                    status = Some("A map needs at least one room.".into());
                }
            }
            MenuAction::MapUp => {
                let i = ed.selected;
                if i > 0 {
                    ed.map.rooms.swap(i, i - 1);
                    ed.selected = i - 1;
                }
            }
            MenuAction::MapDown => {
                let i = ed.selected;
                if i + 1 < ed.map.rooms.len() {
                    ed.map.rooms.swap(i, i + 1);
                    ed.selected = i + 1;
                }
            }
            MenuAction::MapAdjust(f, d) => {
                if let Some(r) = ed.room() {
                    r.adjust(RoomField::ALL[f as usize % RoomField::ALL.len()], d as i32);
                }
            }
            MenuAction::MapPlay => {
                let p = ed.map.clone().sanitized();
                *seed = RunSeed(if p.seed.trim().is_empty() { random_seed() } else { RunSeed::from_text(&p.seed).0 });
                active.0 = Some(p);
                next_overlay.set(Overlay::None);
                next_app.set(AppState::Playing);
            }
            MenuAction::MapHost => {
                let p = ed.map.clone().sanitized();
                let s = if p.seed.trim().is_empty() { random_seed() } else { RunSeed::from_text(&p.seed).0 };
                net.write(NetRequest::Host { name: crate::profile::clean_name(&profile.name), seed: s, preset: Some(p) });
                status = Some("Creating a room for your map... (a sleeping free server can take up to a minute to wake up)".into());
            }
            MenuAction::MapSave | MenuAction::MapExport => {
                let p = ed.map.clone().sanitized();
                // Renamed: replace the old file.
                if let Some(old) = ed.open.and_then(|i| ed.files.get(i)).map(|(path, _)| path.clone())
                    && old.file_name().map(|f| f.to_string_lossy().to_string()) != Some(presets::file_name_for(&p))
                {
                    let _ = std::fs::remove_file(old);
                }
                match presets::save(&presets::presets_dir(), &p) {
                    Ok(path) => {
                        ed.reload();
                        ed.open = ed.files.iter().position(|(f, _)| *f == path);
                        ed.map = p.clone();
                        let mut msg = format!("Saved \"{}\".", p.name);
                        if *a == MenuAction::MapExport {
                            let file = path.file_name().map(PathBuf::from).unwrap_or_default();
                            match downloads_dir().map(|d| d.join(&file)) {
                                Some(dst) if std::fs::copy(&path, &dst).is_ok() => msg = format!("Exported to {}  -  copy it to another computer and drop it on the game window to play or edit it.", dst.display()),
                                _ => msg = format!("Exported to {}  -  copy it to another computer and drop it on the game window to play or edit it.", path.display()),
                            }
                        }
                        status = Some(msg);
                    }
                    Err(e) => status = Some(format!("Couldn't save: {e}")),
                }
            }
            MenuAction::MapImport => {
                // Anything new in Downloads comes in; then the folder opens for the rest.
                let mut n = 0;
                let mut last = None;
                if let Some(dir) = downloads_dir()
                    && let Ok(rd) = std::fs::read_dir(&dir)
                {
                    for e in rd.flatten() {
                        let path = e.path();
                        if path.extension().is_some_and(|x| x.eq_ignore_ascii_case(MAP_EXTENSION))
                            && !presets::presets_dir().join(path.file_name().unwrap_or_default()).exists()
                            && let Ok(p) = import_file(&path)
                        {
                            n += 1;
                            last = Some(p);
                        }
                    }
                }
                ed.reload();
                if let Some(p) = last
                    && let Some(i) = ed.files.iter().position(|(f, _)| *f == p)
                {
                    ed.open_file(i);
                }
                status = Some(if n > 0 {
                    format!("Imported {n} map(s) from your Downloads folder.")
                } else {
                    format!("No new maps in Downloads. Drop a .{MAP_EXTENSION} file on the game window, or put it in {}", presets::presets_dir().display())
                });
            }
            MenuAction::MapFolder => {
                let dir = presets::presets_dir();
                presets::open_folder(&dir);
                status = Some(format!("Your maps live in {}  (any .{MAP_EXTENSION} file there shows up here)", dir.display()));
            }
            MenuAction::MapDeleteFile => {
                if let Some(i) = ed.open {
                    if let Some((path, _)) = ed.files.get(i).cloned() {
                        let _ = std::fs::remove_file(path);
                    }
                    ed.reload();
                    ed.open = None;
                    status = Some(format!("Deleted \"{}\" (still open here: Save to keep it).", ed.map.name));
                } else {
                    status = Some("This map isn't saved yet.".into());
                }
            }
            _ => continue,
        }
        if let Some(s) = status {
            ed.status = s;
        }
        ed.changed();
    }
}

/// Redraws the screen whenever the map changes, keeping the focused control.
fn redraw(mut commands: Commands, theme: Res<UiTheme>, ed: Res<MapEditor>, roots: Query<Entity, With<BuilderRoot>>, focus: Res<InputFocus>, orders: Query<&Focusable>, mut drawn: Local<u32>) {
    if *drawn == ed.version && !roots.is_empty() {
        return;
    }
    *drawn = ed.version;
    let keep = focus.get().and_then(|e| orders.get(e).ok()).map(|f| f.0);
    for r in &roots {
        commands.entity(r).despawn();
    }
    spawn_screen(&mut commands, &theme, &ed, keep);
}

fn small(parent: &mut ChildSpawnerCommands, t: &UiTheme, label: &str, action: MenuAction, order: i32, w: f32, keep: Option<i32>) -> Entity {
    let e = spawn_button(parent, t, label, action, order, keep == Some(order));
    parent.commands().entity(e).entry::<Node>().and_modify(move |mut n| {
        n.width = px(w);
        n.min_width = px(w);
        n.min_height = px(34);
        n.padding = UiRect::axes(px(4), px(3));
    });
    e
}

fn area_color(a: Area) -> Color {
    match a {
        Area::GuestWing => Color::srgb(0.75, 0.55, 0.3),
        Area::Archive => Color::srgb(0.55, 0.38, 0.22),
        Area::ServiceTunnels => Color::srgb(0.45, 0.5, 0.55),
        Area::Grounds => Color::srgb(0.3, 0.55, 0.28),
        Area::QuarryLift => Color::srgb(0.55, 0.42, 0.3),
        Area::Atrium => Color::srgb(0.8, 0.78, 0.72),
    }
}

fn spawn_screen(commands: &mut Commands, t: &UiTheme, ed: &MapEditor, keep: Option<i32>) {
    let keep = keep.or(Some(100 + ed.selected as i32));
    let plan = plan::from_map(&ed.map.rooms, 1);
    commands
        .spawn((t.screen_root(true), GlobalZIndex(20), DespawnOnExit(Screen::MapBuilder), BuilderRoot, Name::new("Map builder")))
        .with_children(|root| {
            root.spawn(t.heading_text("MAP BUILDER"));
            // Saved maps: browse, or start a new one.
            root.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), ..default() }).with_children(|row| {
                small(row, t, "<", MenuAction::MapPrev, 0, 50.0, keep);
                let which = match ed.open {
                    Some(i) => format!("{} / {}", i + 1, ed.files.len()),
                    None => "new - not saved".into(),
                };
                row.spawn((Text::new(format!("{}  ({which})", ed.map.name)), t.font(true, t.size_body), TextColor(t.interact), Node { min_width: px(420), justify_content: JustifyContent::Center, ..default() }));
                small(row, t, ">", MenuAction::MapNext, 2, 50.0, keep);
                small(row, t, "New", MenuAction::MapNew, 3, 90.0, keep);
            });
            root.spawn(Node { column_gap: px(14), align_items: AlignItems::FlexStart, ..default() }).with_children(|cols| {
                // ---- Rooms ----
                cols.spawn(t.panel(380.0)).with_children(|p| {
                    field_row(p, t, FieldKind::MapName, &ed.map.name, 4);
                    p.spawn((Text::new(format!("ROOMS ({})  -  DOOR {:03} AT THE END", ed.map.rooms.len(), plan.final_door())), t.font(true, t.size_small), TextColor(t.interact)));
                    for (i, r) in ed.map.rooms.iter().enumerate() {
                        // The selected room is marked with arrows (the frame shows keyboard focus).
                        let mark = if i == ed.selected { ">" } else { " " };
                        let label = format!("{mark} {:03} {}", plan::FIRST_ROOM as usize + i, r.summary());
                        let e = small(p, t, &label, MenuAction::MapSelect(i as u16), 100 + i as i32, 360.0, keep);
                        let sel = i == ed.selected;
                        p.commands().entity(e).entry::<Node>().and_modify(|mut n| n.justify_content = JustifyContent::FlexStart);
                        if sel {
                            p.commands().entity(e).insert(BackgroundColor(t.interact.with_alpha(0.28)));
                        }
                    }
                });
                // ---- The selected room ----
                cols.spawn(t.panel(430.0)).with_children(|p| {
                    let n = plan::FIRST_ROOM as usize + ed.selected;
                    p.spawn((Text::new(format!("ROOM {n:03}")), t.font(true, t.size_small), TextColor(t.interact)));
                    if let Some(r) = ed.map.rooms.get(ed.selected) {
                        for (i, f) in RoomField::ALL.into_iter().enumerate() {
                            p.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                                row.spawn((Text::new(f.label()), t.font(false, t.size_small), TextColor(t.text), Node { width: px(96), ..default() }));
                                small(row, t, "<", MenuAction::MapAdjust(i as u8, -1), 10 + i as i32 * 2, 46.0, keep);
                                row.spawn((Text::new(r.value(f)), t.font(true, t.size_small), TextColor(t.text), Node { width: px(170), justify_content: JustifyContent::Center, ..default() }));
                                small(row, t, ">", MenuAction::MapAdjust(i as u8, 1), 11 + i as i32 * 2, 46.0, keep);
                            });
                        }
                        // What the game will actually build (sizes grow to fit structures).
                        if let Some(b) = plan.room(n as u32) {
                            let mut notes = vec![format!("Builds {:.0} x {:.0} m", b.width(), b.depth())];
                            if b.kind == RoomKind::KeyPuzzle {
                                notes.push("key in a drawer".into());
                            }
                            if b.encounter.is_some_and(Encounter::needs_hiding) {
                                notes.push("wardrobes to hide in".into());
                            }
                            p.spawn(t.dim_text(notes.join("  -  ")));
                        }
                    }
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|row| {
                        for (j, (label, action)) in [("Add", MenuAction::MapAdd), ("Copy", MenuAction::MapCopy), ("Remove", MenuAction::MapRemove), ("Up", MenuAction::MapUp), ("Down", MenuAction::MapDown)].into_iter().enumerate() {
                            small(row, t, label, action, 30 + j as i32, 76.0, keep);
                        }
                    });
                });
                // ---- From above ----
                cols.spawn(t.panel(260.0)).with_children(|p| {
                    p.spawn((Text::new("FROM ABOVE"), t.font(true, t.size_small), TextColor(t.interact)));
                    let (w, h) = (240.0f32, 400.0f32);
                    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
                    for r in &plan.rooms {
                        lo = lo.min(r.min);
                        hi = hi.max(r.max);
                    }
                    let span = (hi - lo).max(Vec2::splat(1.0));
                    let k = (w / span.x).min(h / span.y);
                    // Centred in the box.
                    let off = Vec2::new((w - span.x * k) / 2.0, (h - span.y * k) / 2.0);
                    p.spawn((Node { width: px(w), height: px(h), ..default() }, BackgroundColor(Color::srgb(0.04, 0.04, 0.05)))).with_children(|map| {
                        for (i, r) in plan.rooms.iter().enumerate() {
                            let sel = i == ed.selected;
                            let c = if r.kind == RoomKind::Final { Color::srgb(0.7, 0.2, 0.2) } else { area_color(r.area) };
                            map.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: px(off.x + (r.min.x - lo.x) * k),
                                    // North (lower z) is up.
                                    top: px(off.y + (r.min.y - lo.y) * k),
                                    width: px((r.width() * k).max(2.0)),
                                    height: px((r.depth() * k).max(2.0)),
                                    border: UiRect::all(px(if sel { 2.0 } else { 1.0 })),
                                    ..default()
                                },
                                BackgroundColor(c.with_alpha(if sel { 0.95 } else { 0.55 })),
                                BorderColor::all(if sel { Color::WHITE } else { Color::srgb(0.1, 0.1, 0.1) }),
                            ));
                        }
                    });
                    p.spawn(t.dim_text("Start at the bottom; the red hall holds the last door."));
                });
            });
            // ---- Actions ----
            root.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, max_width: px(1100), ..default() }).with_children(|row| {
                for (j, (label, action)) in [
                    ("Play Test", MenuAction::MapPlay),
                    ("Host Room", MenuAction::MapHost),
                    ("Save", MenuAction::MapSave),
                    ("Export", MenuAction::MapExport),
                    ("Import", MenuAction::MapImport),
                    ("Open Folder", MenuAction::MapFolder),
                    ("Delete", MenuAction::MapDeleteFile),
                    ("Back", MenuAction::CloseMapBuilder),
                ]
                .into_iter()
                .enumerate()
                {
                    small(row, t, label, action, 60 + j as i32, 150.0, keep);
                }
            });
            root.spawn((Text::new(ed.status.clone()), t.font(false, t.size_small), TextColor(t.text_dim), Node { max_width: px(1100), ..default() }));
        });
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

