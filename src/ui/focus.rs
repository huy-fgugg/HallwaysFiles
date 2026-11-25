//! Menu buttons with mouse, keyboard and controller support.
//!
//! * Every interactive element carries [`Focusable`] (tab order) and either a
//!   [`MenuButton`] (activates) or an [`Adjustable`] row (left/right changes a
//!   value, Enter toggles).
//! * Up/Down (arrows, W/S, D-pad, left stick) move focus; Enter/Space/gamepad
//!   South activate; Left/Right adjust; hovering with the mouse moves focus.
//! * The focused element always has a visible cyan border + brighter fill.

use bevy::{
    input_focus::InputFocus,
    picking::events::{PointerClick, PointerOver},
    prelude::*,
};

use crate::audio::{PlaySfx, Sfx};
use crate::theme::UiTheme;

/// Things a menu button can do. Each screen reacts to the actions it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    StartRun,
    OpenSettings,
    Quit,
    Resume,
    AbandonRun,
    CloseSettings,
    ResetSettings,
    OpenCredits,
    CloseCredits,
    OpenAchievements,
    CloseAchievements,
    Retry,
    OpenPlay,
    ClosePlay,
    PlaySolo,
    HostRoom,
    JoinRoom,
    Buy(crate::items::ItemKind),
    Refund,
    ToggleModifier(crate::profile::Modifier),
    OpenAdmin,
    CloseAdmin,
    AdminUnlock,
    AdminSpawn(crate::items::ItemKind),
    Admin(u8, crate::admin::AdminAction),
    /// Room host: remove this player.
    AdminKick(u8),
    /// Send the typed message to everyone in your room.
    AdminAnnounce,
    /// Server tools (need the server's admin key).
    AdminServerRooms,
    AdminServerAnnounce,
    AdminServerClose(u8),
    AdminServerKick(u8, u8),
    OpenCustom,
    CloseCustom,
    PresetPrev,
    PresetNext,
    PresetAdjust(crate::presets::PresetField, i8),
    PresetToggleArea(crate::floor::plan::Area),
    PresetToggleEntity(u8),
    PresetSave,
    PresetSaveNew,
    PresetDelete,
    PresetFolder,
    PresetPlay,
    PresetHost,
    OpenCutscenes,
    CloseCutscenes,
    OpenSkin,
    CloseSkin,
    /// Step a part of your look (index into `SkinPart::ALL`) by +/-1.
    SkinStep(u8, i8),
    OpenMapBuilder,
    CloseMapBuilder,
    MapPrev,
    MapNext,
    MapNew,
    MapSelect(u16),
    MapAdd,
    MapCopy,
    MapRemove,
    MapUp,
    MapDown,
    /// Change a setting of the selected room (index into `RoomField::ALL`).
    MapAdjust(u8, i8),
    MapPlay,
    MapHost,
    MapSave,
    MapExport,
    MapImport,
    MapFolder,
    MapDeleteFile,
    Revive,
    WatchCutscene(u8),
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MenuButton(pub MenuAction);

/// Position in the focus order of the current screen (lower = earlier).
#[derive(Component, Debug, Clone, Copy)]
pub struct Focusable(pub i32);

/// A settings row that reacts to left/right and Enter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adjustable(pub crate::ui::settings_menu::SettingField);

/// Fired when a [`MenuButton`] is activated by any input method.
#[derive(Message, Debug, Clone, Copy)]
pub struct MenuActivated(pub MenuAction);

/// Fired when an [`Adjustable`] row is changed. `delta` is -1, +1, or 0 for "toggle".
#[derive(Message, Debug, Clone, Copy)]
pub struct AdjustRequested {
    pub field: crate::ui::settings_menu::SettingField,
    pub delta: i32,
}

/// Small arrow buttons beside settings values for mouse users.
#[derive(Component, Debug, Clone, Copy)]
pub struct AdjustArrow {
    pub row: Entity,
    pub delta: i32,
}

/// Marks the element that should be focused when its screen opens.
#[derive(Component)]
pub struct InitialFocus;

pub struct FocusPlugin;

impl Plugin for FocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MenuActivated>()
            .add_message::<AdjustRequested>()
            .init_resource::<StickRepeat>()
            .add_observer(on_hover)
            .add_observer(on_click)
            .add_systems(
                Update,
                (apply_initial_focus, keyboard_and_gamepad_navigation, paint_focus).chain(),
            );
    }
}

fn apply_initial_focus(
    mut commands: Commands,
    mut focus: ResMut<InputFocus>,
    q: Query<Entity, Added<InitialFocus>>,
) {
    for e in &q {
        *focus = InputFocus::from_entity(e);
        commands.entity(e).remove::<InitialFocus>();
    }
}

fn on_hover(
    over: On<PointerOver>,
    q: Query<(), With<Focusable>>,
    mut focus: ResMut<InputFocus>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if q.contains(over.entity) && focus.get() != Some(over.entity) {
        *focus = InputFocus::from_entity(over.entity);
        sfx.write(PlaySfx(Sfx::UiMove));
    }
}

fn on_click(
    mut click: On<PointerClick>,
    buttons: Query<&MenuButton>,
    rows: Query<&Adjustable>,
    arrows: Query<&AdjustArrow>,
    mut activated: MessageWriter<MenuActivated>,
    mut adjust: MessageWriter<AdjustRequested>,
    mut focus: ResMut<InputFocus>,
    mut sfx: MessageWriter<PlaySfx>,
    fields: Query<(), With<crate::ui::play_menu::TextField>>,
) {
    let e = click.entity;
    if fields.contains(e) {
        // Tapping/clicking a text box selects it (touch screens have no hover).
        click.propagate(false);
        *focus = InputFocus::from_entity(e);
        return;
    }
    if let Ok(arrow) = arrows.get(e) {
        click.propagate(false);
        if let Ok(row) = rows.get(arrow.row) {
            *focus = InputFocus::from_entity(arrow.row);
            adjust.write(AdjustRequested { field: row.0, delta: arrow.delta });
            sfx.write(PlaySfx(Sfx::UiSelect));
        }
    } else if let Ok(button) = buttons.get(e) {
        click.propagate(false);
        activated.write(MenuActivated(button.0));
        sfx.write(PlaySfx(Sfx::UiSelect));
    } else if let Ok(row) = rows.get(e) {
        click.propagate(false);
        adjust.write(AdjustRequested { field: row.0, delta: 0 });
        sfx.write(PlaySfx(Sfx::UiSelect));
    }
}

/// Converts analog stick deflection into repeated discrete steps.
#[derive(Resource, Default)]
struct StickRepeat {
    timer: f32,
    last: IVec2,
}

#[allow(clippy::too_many_arguments)]
fn keyboard_and_gamepad_navigation(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    mut repeat: ResMut<StickRepeat>,
    mut focus: ResMut<InputFocus>,
    focusables: Query<(Entity, &Focusable, &InheritedVisibility)>,
    buttons: Query<&MenuButton>,
    rows: Query<&Adjustable>,
    mut activated: MessageWriter<MenuActivated>,
    mut adjust: MessageWriter<AdjustRequested>,
    mut sfx: MessageWriter<PlaySfx>,
    typing: Res<crate::ui::play_menu::Typing>,
) {
    let mut step = IVec2::ZERO;
    let mut activate = false;
    // While a text box has focus, letters and Space type instead of navigating.
    let letters = !typing.0;

    if keys.just_pressed(KeyCode::ArrowUp) || (letters && keys.just_pressed(KeyCode::KeyW)) {
        step.y -= 1;
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::Tab]) || (letters && keys.just_pressed(KeyCode::KeyS)) {
        step.y += 1;
    }
    if (letters && keys.just_pressed(KeyCode::KeyA)) || (letters && keys.just_pressed(KeyCode::ArrowLeft)) {
        step.x -= 1;
    }
    if (letters && keys.just_pressed(KeyCode::KeyD)) || (letters && keys.just_pressed(KeyCode::ArrowRight)) {
        step.x += 1;
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) || (letters && keys.just_pressed(KeyCode::Space)) {
        activate = true;
    }

    // Gamepad: D-pad + South, plus left stick with auto-repeat.
    let mut stick_dir = IVec2::ZERO;
    for pad in &gamepads {
        if pad.just_pressed(GamepadButton::DPadUp) {
            step.y -= 1;
        }
        if pad.just_pressed(GamepadButton::DPadDown) {
            step.y += 1;
        }
        if pad.just_pressed(GamepadButton::DPadLeft) {
            step.x -= 1;
        }
        if pad.just_pressed(GamepadButton::DPadRight) {
            step.x += 1;
        }
        if pad.just_pressed(GamepadButton::South) {
            activate = true;
        }
        let s = pad.left_stick();
        if s.y > 0.6 {
            stick_dir.y = -1;
        } else if s.y < -0.6 {
            stick_dir.y = 1;
        }
        if s.x > 0.6 {
            stick_dir.x = 1;
        } else if s.x < -0.6 {
            stick_dir.x = -1;
        }
    }
    if stick_dir != IVec2::ZERO {
        if stick_dir != repeat.last {
            step += stick_dir;
            repeat.timer = 0.35;
        } else {
            repeat.timer -= time.delta_secs();
            if repeat.timer <= 0.0 {
                step += stick_dir;
                repeat.timer = 0.12;
            }
        }
    }
    repeat.last = stick_dir;

    if step == IVec2::ZERO && !activate {
        return;
    }

    let mut order: Vec<(i32, Entity)> = focusables
        .iter()
        .filter(|(_, _, vis)| vis.get())
        .map(|(e, f, _)| (f.0, e))
        .collect();
    if order.is_empty() {
        return;
    }
    order.sort();
    let current = focus.get().and_then(|f| order.iter().position(|(_, e)| *e == f));

    if step.y != 0 {
        let len = order.len() as i32;
        let next = match current {
            Some(i) => (i as i32 + step.y).rem_euclid(len) as usize,
            None => 0,
        };
        *focus = InputFocus::from_entity(order[next].1);
        sfx.write(PlaySfx(Sfx::UiMove));
        return;
    }

    let Some(idx) = current else {
        *focus = InputFocus::from_entity(order[0].1);
        return;
    };
    let target = order[idx].1;
    if step.x != 0 {
        if let Ok(row) = rows.get(target) {
            adjust.write(AdjustRequested { field: row.0, delta: step.x.signum() });
            sfx.write(PlaySfx(Sfx::UiMove));
        }
    } else if activate {
        if let Ok(button) = buttons.get(target) {
            activated.write(MenuActivated(button.0));
            sfx.write(PlaySfx(Sfx::UiSelect));
        } else if let Ok(row) = rows.get(target) {
            adjust.write(AdjustRequested { field: row.0, delta: 0 });
            sfx.write(PlaySfx(Sfx::UiSelect));
        }
    }
}

/// Visible focus state: cyan border, brighter fill, and a ">" marker via text colour.
/// Own colours for a focusable (instead of the theme's): background,
/// border and label colour, normal and focused.
#[derive(Component, Debug, Clone, Copy)]
pub struct FocusPalette {
    pub bg: Color,
    pub border: Color,
    pub text: Color,
    pub bg_focus: Color,
    pub border_focus: Color,
    pub text_focus: Color,
}

fn paint_focus(
    theme: Res<UiTheme>,
    focus: Res<InputFocus>,
    mut q: Query<(Entity, &mut BackgroundColor, &mut BorderColor, Option<&Children>, Option<&FocusPalette>), With<Focusable>>,
    mut texts: Query<&mut TextColor>,
) {
    for (e, mut bg, mut border, children, pal) in &mut q {
        let focused = focus.get() == Some(e);
        let (want_bg, want_border, want_text) = match (pal, focused) {
            (Some(p), true) => (p.bg_focus, p.border_focus, p.text_focus),
            (Some(p), false) => (p.bg, p.border, p.text),
            (None, true) => (theme.button_bg_focus, theme.interact, theme.interact),
            (None, false) => (theme.button_bg, theme.panel_border, theme.text),
        };
        if bg.0 != want_bg {
            bg.0 = want_bg;
        }
        if *border != BorderColor::all(want_border) {
            *border = BorderColor::all(want_border);
        }
        if let Some(children) = children {
            // Only the first child (the label) follows focus colour.
            if let Some(&first) = children.first()
                && let Ok(mut c) = texts.get_mut(first)
                && c.0 != want_text
            {
                c.0 = want_text;
            }
        }
    }
}

/// Spawns a themed menu button as a child of `parent`.
pub fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    theme: &UiTheme,
    label: &str,
    action: MenuAction,
    order: i32,
    initial: bool,
) -> Entity {
    let mut e = parent.spawn((
        theme.primary_button(),
        MenuButton(action),
        Focusable(order),
        Name::new(format!("Button: {label}")),
    ));
    e.with_child((Text::new(label.to_uppercase()), theme.font(true, theme.size_body), TextColor(theme.text)));
    if initial {
        e.insert(InitialFocus);
    }
    e.id()
}
