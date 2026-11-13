//! Custom Floors screen: pick a preset (built-in or one of your `.hallways`
//! files), tweak it, save as many as you like, and play it solo or online.

use bevy::prelude::*;

use super::focus::{spawn_button, MenuAction, MenuActivated};
use super::play_menu::{field_row, random_seed, FieldKind, TextField};
use crate::floor::plan::Area;
use crate::net::NetRequest;
use crate::presets::{self, ActivePreset, Entry, Preset, PresetField, ENTITY_NAMES};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

/// The preset list and the copy being edited.
#[derive(Resource, Debug)]
pub struct CustomEditor {
    pub list: Vec<Entry>,
    pub index: usize,
    pub edit: Preset,
    /// Text boxes need refreshing (a different preset was selected).
    refresh_fields: bool,
}

impl CustomEditor {
    fn load(index: usize) -> Self {
        let list = presets::load_all(&presets::presets_dir());
        let index = index.min(list.len().saturating_sub(1));
        let edit = list[index].preset.clone();
        Self { list, index, edit, refresh_fields: true }
    }

    fn select(&mut self, index: usize) {
        self.index = index % self.list.len().max(1);
        self.edit = self.list[self.index].preset.clone();
        self.refresh_fields = true;
    }
}

#[derive(Component)]
struct PresetTitle;
#[derive(Component)]
struct FieldValue(PresetField);
#[derive(Component)]
struct AreaLabel(Area);
#[derive(Component)]
struct EntityLabel(u8);
#[derive(Component)]
struct CustomStatus;

pub struct CustomMenuPlugin;

impl Plugin for CustomMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Custom), (open_editor, spawn_screen).chain())
            .add_systems(Update, (custom_actions, refresh, super::play_menu::room_ready).chain().run_if(in_state(Screen::Custom)));
    }
}

fn open_editor(mut commands: Commands, old: Option<Res<CustomEditor>>) {
    commands.insert_resource(CustomEditor::load(old.map_or(0, |o| o.index)));
}

fn small(parent: &mut ChildSpawnerCommands, t: &UiTheme, label: &str, action: MenuAction, order: i32, w: f32) {
    small_focus(parent, t, label, action, order, w, false);
}

fn small_focus(parent: &mut ChildSpawnerCommands, t: &UiTheme, label: &str, action: MenuAction, order: i32, w: f32, initial: bool) {
    let e = spawn_button(parent, t, label, action, order, initial);
    parent.commands().entity(e).entry::<Node>().and_modify(move |mut n| {
        n.width = px(w);
        n.min_width = px(w);
        n.min_height = px(36);
        n.padding = UiRect::axes(px(6), px(4));
    });
}

fn spawn_screen(mut commands: Commands, theme: Res<UiTheme>) {
    let t = theme.clone();
    commands
        .spawn((t.screen_root(false), GlobalZIndex(20), DespawnOnExit(Screen::Custom), Name::new("Custom floors")))
        .with_children(|root| {
            root.spawn(t.heading_text("CUSTOM FLOORS"));
            // Preset picker.
            root.spawn(Node { align_items: AlignItems::Center, column_gap: px(12), ..default() }).with_children(|row| {
                small(row, &t, "<", MenuAction::PresetPrev, 0, 60.0);
                row.spawn((Text::new(""), t.font(true, t.size_body), TextColor(t.interact), PresetTitle, Node { min_width: px(520), justify_content: JustifyContent::Center, ..default() }));
                small(row, &t, ">", MenuAction::PresetNext, 1, 60.0);
            });
            root.spawn(Node { column_gap: px(18), align_items: AlignItems::FlexStart, ..default() }).with_children(|cols| {
                // Numbers.
                cols.spawn(t.panel(470.0)).with_children(|p| {
                    field_row(p, &t, FieldKind::PresetName, "", 2);
                    field_row(p, &t, FieldKind::PresetSeed, "", 3);
                    for (i, f) in PresetField::ALL.into_iter().enumerate() {
                        p.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                            r.spawn((Text::new(f.label()), t.font(false, t.size_small), TextColor(t.text), Node { width: px(170), ..default() }));
                            small(r, &t, "-", MenuAction::PresetAdjust(f, -1), 10 + i as i32 * 2, 48.0);
                            r.spawn((Text::new(""), t.font(true, t.size_small), TextColor(t.text), FieldValue(f), Node { width: px(80), justify_content: JustifyContent::Center, ..default() }));
                            small(r, &t, "+", MenuAction::PresetAdjust(f, 1), 11 + i as i32 * 2, 48.0);
                        });
                    }
                });
                // Switches.
                cols.spawn(t.panel(380.0)).with_children(|p| {
                    p.spawn((Text::new("AREAS (IN ORDER)"), t.font(true, t.size_small), TextColor(t.interact)));
                    for (i, a) in Area::ALL.into_iter().enumerate() {
                        let e = spawn_button(p, &t, "", MenuAction::PresetToggleArea(a), 40 + i as i32, false);
                        p.commands().entity(e).insert(AreaLabel(a)).entry::<Node>().and_modify(|mut n| n.min_height = px(34));
                    }
                    p.spawn((Text::new("ENTITIES"), t.font(true, t.size_small), TextColor(t.interact), Node { margin: UiRect::top(px(8)), ..default() }));
                    for i in 0..ENTITY_NAMES.len() {
                        let e = spawn_button(p, &t, "", MenuAction::PresetToggleEntity(i as u8), 50 + i as i32, false);
                        p.commands().entity(e).insert(EntityLabel(i as u8)).entry::<Node>().and_modify(|mut n| n.min_height = px(34));
                    }
                });
            });
            // Actions.
            root.spawn(Node { column_gap: px(10), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, max_width: px(900), row_gap: px(8), ..default() }).with_children(|row| {
                for (i, (label, action)) in [
                    ("Play Solo", MenuAction::PresetPlay),
                    ("Host Room", MenuAction::PresetHost),
                    ("Save", MenuAction::PresetSave),
                    ("Save As New", MenuAction::PresetSaveNew),
                    ("Delete", MenuAction::PresetDelete),
                    ("Open Folder", MenuAction::PresetFolder),
                    ("Back", MenuAction::CloseCustom),
                ]
                .into_iter()
                .enumerate()
                {
                    // Play is focused on open: pick a floor with < >, then press it.
                    small_focus(row, &t, label, action, 70 + i as i32, 160.0, i == 0);
                }
            });
            root.spawn((Text::new(""), t.font(false, t.size_small), TextColor(t.text_dim), CustomStatus, Node { max_width: px(900), ..default() }));
        });
}

#[allow(clippy::too_many_arguments)]
fn custom_actions(
    mut reader: MessageReader<MenuActivated>,
    mut ed: ResMut<CustomEditor>,
    mut status: Query<&mut Text, With<CustomStatus>>,
    mut active: ResMut<ActivePreset>,
    mut seed: ResMut<RunSeed>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut net: MessageWriter<NetRequest>,
    profile: Res<crate::profile::Profile>,
) {
    let mut say = |s: String| {
        for mut t in &mut status {
            t.0 = s.clone();
        }
    };
    let dir = presets::presets_dir();
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::CloseCustom => next_overlay.set(Overlay::Play),
            MenuAction::PresetPrev => {
                let n = ed.list.len();
                let i = (ed.index + n - 1) % n;
                ed.select(i);
            }
            MenuAction::PresetNext => {
                let i = ed.index + 1;
                ed.select(i);
            }
            MenuAction::PresetAdjust(f, d) => f.adjust(&mut ed.edit, d as i32),
            MenuAction::PresetToggleArea(area) => ed.edit.toggle_area(area),
            MenuAction::PresetToggleEntity(i) => ed.edit.toggle_entity(i as usize),
            MenuAction::PresetSave | MenuAction::PresetSaveNew => {
                let mut p = ed.edit.clone().sanitized();
                // Built-ins can't be overwritten: saving one makes your own copy.
                let is_builtin = ed.list[ed.index].path.is_none();
                if *a == MenuAction::PresetSaveNew || is_builtin {
                    let base = if *a == MenuAction::PresetSaveNew || p.name == ed.list[ed.index].preset.name { format!("{} copy", p.name) } else { p.name.clone() };
                    p.name = presets::unique_name(&ed.list, &base);
                } else if let Some(old) = ed.list[ed.index].path.clone() {
                    // Renamed: replace the old file.
                    if old.file_name().map(|f| f.to_string_lossy().to_string()) != Some(presets::file_name(&p.name)) {
                        let _ = std::fs::remove_file(old);
                    }
                }
                match presets::save(&dir, &p) {
                    Ok(path) => {
                        let name = p.name.clone();
                        *ed = CustomEditor::load(0);
                        if let Some(i) = ed.list.iter().position(|e| e.path.as_deref() == Some(path.as_path())) {
                            ed.select(i);
                        }
                        say(format!("Saved \"{name}\" to {}", path.display()));
                    }
                    Err(e) => say(format!("Couldn't save: {e}")),
                }
            }
            MenuAction::PresetDelete => {
                if let Some(path) = ed.list[ed.index].path.clone() {
                    let _ = std::fs::remove_file(&path);
                    let i = ed.index.saturating_sub(1);
                    *ed = CustomEditor::load(i);
                    say("Deleted.".into());
                } else {
                    say("Built-in presets can't be deleted. Save As New to make your own.".into());
                }
            }
            MenuAction::PresetFolder => {
                presets::open_folder(&dir);
                say(format!("Your presets live in {}  (any .hallways file there shows up here)", dir.display()));
            }
            MenuAction::PresetPlay => {
                let p = ed.edit.clone().sanitized();
                *seed = RunSeed(if p.seed.trim().is_empty() { random_seed() } else { RunSeed::from_text(&p.seed).0 });
                active.0 = Some(p);
                next_overlay.set(Overlay::None);
                next_app.set(AppState::Playing);
            }
            MenuAction::PresetHost => {
                let p = ed.edit.clone().sanitized();
                let s = if p.seed.trim().is_empty() { random_seed() } else { RunSeed::from_text(&p.seed).0 };
                net.write(NetRequest::Host { name: crate::profile::clean_name(&profile.name), seed: s, preset: Some(p) });
                say("Creating a room... (a sleeping free server can take up to a minute to wake up)".into());
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn refresh(
    mut ed: ResMut<CustomEditor>,
    mut fields: Query<&mut TextField>,
    mut texts: ParamSet<(
        Query<&mut Text, With<PresetTitle>>,
        Query<(&FieldValue, &mut Text)>,
        Query<(&AreaLabel, &Children)>,
        Query<(&EntityLabel, &Children)>,
        Query<&mut Text>,
    )>,
    session: Res<crate::net::NetSession>,
    status_ids: Query<Entity, With<CustomStatus>>,
) {
    if ed.refresh_fields {
        ed.refresh_fields = false;
        for mut f in &mut fields {
            match f.kind {
                FieldKind::PresetName => f.value = ed.edit.name.clone(),
                FieldKind::PresetSeed => f.value = ed.edit.seed.clone(),
                _ => {}
            }
        }
    }
    let entry = &ed.list[ed.index];
    let kind = if entry.path.is_some() { "your preset" } else { "built-in" };
    let title = format!("{}  ({kind}, {} / {})", ed.edit.name, ed.index + 1, ed.list.len());
    for mut t in &mut texts.p0() {
        if t.0 != title {
            t.0 = title.clone();
        }
    }
    for (f, mut t) in &mut texts.p1() {
        let v = f.0.value_text(&ed.edit);
        if t.0 != v {
            t.0 = v;
        }
    }
    let area_labels: Vec<(Entity, String)> = texts
        .p2()
        .iter()
        .filter_map(|(a, ch)| ch.first().map(|c| (*c, format!("{} {}", if ed.edit.areas.contains(&a.0) { "[ON] " } else { "[OFF]" }, a.0.name().to_uppercase()))))
        .collect();
    let entity_labels: Vec<(Entity, String)> = texts
        .p3()
        .iter()
        .filter_map(|(e, ch)| ch.first().map(|c| (*c, format!("{} {}", if ed.edit.entity_on(e.0 as usize) { "[ON] " } else { "[OFF]" }, ENTITY_NAMES[e.0 as usize].to_uppercase()))))
        .collect();
    let mut q = texts.p4();
    for (e, s) in area_labels.into_iter().chain(entity_labels) {
        if let Ok(mut t) = q.get_mut(e) {
            if t.0 != s {
                t.0 = s;
            }
        }
    }
    if let crate::net::NetStatus::Error(e) = &session.status {
        for id in &status_ids {
            if let Ok(mut t) = q.get_mut(id) {
                if !t.0.contains(e.as_str()) && t.0.starts_with("Creating") {
                    t.0 = e.clone();
                }
            }
        }
    }
}
