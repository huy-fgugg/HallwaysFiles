//! Touch controls for phones and tablets (always on for Android; appears on
//! any device the first time the screen is touched).
//!
//! - Left half of the screen: a floating joystick to walk.
//! - Right half: drag to look around.
//! - Buttons: USE (interact / hide), CROUCH and RUN toggles, LIGHT, ITEM
//!   (next slot), USE ITEM, MENU; START in the lift, SKIP during cutscenes.
//!
//! The controls press the same keys a keyboard would, so every game system
//! works unchanged.

use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

use crate::state::{AppState, Screen};
use crate::theme::UiTheme;

/// Look movement from dragging, in logical pixels (consumed by the player).
#[derive(Resource, Debug, Default)]
pub struct TouchLook(pub Vec2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Interact,
    Crouch,
    Sprint,
    Light,
    NextItem,
    UseItem,
    Pause,
    Start,
    Skip,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Interact => "USE",
            Action::Crouch => "CROUCH",
            Action::Sprint => "RUN",
            Action::Light => "LIGHT",
            Action::NextItem => "ITEM",
            Action::UseItem => "USE ITEM",
            Action::Pause => "MENU",
            Action::Start => "START",
            Action::Skip => "HOLD TO SKIP",
        }
    }
}

#[derive(Component)]
struct TouchButton(Action);
#[derive(Component)]
struct TouchRoot;
#[derive(Component)]
struct StickBase;
#[derive(Component)]
struct StickKnob;

#[derive(Resource, Default)]
struct TouchState {
    enabled: bool,
    stick: Option<(u64, Vec2)>,
    stick_pos: Vec2,
    look: Option<u64>,
    crouch: bool,
    sprint: bool,
    /// Keys tapped this frame, released next frame.
    tapped: Vec<KeyCode>,
    pressed_buttons: Vec<(u64, Action)>,
}

const STICK_RADIUS: f32 = 70.0;
const MOVE_KEYS: [KeyCode; 4] = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD];

pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchLook>()
            .insert_resource(TouchState { enabled: cfg!(target_os = "android"), ..default() })
            .add_systems(OnEnter(AppState::Playing), spawn_controls)
            .add_systems(PreUpdate, handle_touches.after(bevy::input::InputSystems))
            .add_systems(Update, (update_controls.run_if(in_state(AppState::Playing)), soft_keyboard));
    }
}

fn button(parent: &mut ChildSpawnerCommands, theme: &UiTheme, action: Action, pos: (f32, f32), size: f32) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(pos.0),
                bottom: px(pos.1),
                width: px(size),
                height: px(size),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(theme.panel.with_alpha(0.55)),
            BorderColor::all(theme.interact.with_alpha(0.7)),
            TouchButton(action),
        ))
        .with_child((Text::new(action.label()), theme.font(true, if size > 100.0 { 18.0 } else { 13.0 }), TextColor(theme.text), Pickable::IGNORE));
}

fn spawn_controls(mut commands: Commands, theme: Res<UiTheme>) {
    let t = theme.clone();
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
            Visibility::Hidden,
            GlobalZIndex(15),
            Pickable::IGNORE,
            TouchRoot,
            DespawnOnExit(AppState::Playing),
        ))
        .with_children(|root| {
            button(root, &t, Action::Interact, (60.0, 70.0), 120.0);
            button(root, &t, Action::Crouch, (200.0, 40.0), 84.0);
            button(root, &t, Action::Sprint, (200.0, 140.0), 84.0);
            button(root, &t, Action::Light, (40.0, 210.0), 76.0);
            button(root, &t, Action::NextItem, (130.0, 240.0), 76.0);
            button(root, &t, Action::UseItem, (220.0, 240.0), 76.0);
            button(root, &t, Action::Pause, (24.0, 600.0), 70.0);
            button(root, &t, Action::Start, (560.0, 140.0), 110.0);
            button(root, &t, Action::Skip, (40.0, 360.0), 110.0);
            // Floating joystick (shown where the thumb lands).
            root.spawn((
                Node { position_type: PositionType::Absolute, width: px(STICK_RADIUS * 2.0), height: px(STICK_RADIUS * 2.0), border: UiRect::all(px(2)), border_radius: BorderRadius::MAX, ..default() },
                BackgroundColor(t.panel.with_alpha(0.3)),
                BorderColor::all(t.text_dim.with_alpha(0.6)),
                Visibility::Hidden,
                Pickable::IGNORE,
                StickBase,
            ));
            root.spawn((
                Node { position_type: PositionType::Absolute, width: px(56), height: px(56), border_radius: BorderRadius::MAX, ..default() },
                BackgroundColor(t.interact.with_alpha(0.6)),
                Visibility::Hidden,
                Pickable::IGNORE,
                StickKnob,
            ));
        });
}

/// Which touch button (if any) covers a point in physical pixels.
fn button_at(buttons: &Query<(&TouchButton, &ComputedNode, &UiGlobalTransform, &InheritedVisibility)>, p: Vec2) -> Option<Action> {
    buttons.iter().find(|(_, node, tf, vis)| vis.get() && node.contains_point(**tf, p)).map(|(b, ..)| b.0)
}

#[allow(clippy::too_many_arguments)]
fn handle_touches(
    touches: Res<Touches>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut state: ResMut<TouchState>,
    mut look: ResMut<TouchLook>,
    buttons: Query<(&TouchButton, &ComputedNode, &UiGlobalTransform, &InheritedVisibility)>,
    window: Query<&Window, With<PrimaryWindow>>,
    screen: Option<Res<State<Screen>>>,
    inv: Option<Res<crate::items::Inventory>>,
) {
    // The Android back button acts like Esc.
    if keys.just_pressed(KeyCode::BrowserBack) {
        keys.press(KeyCode::Escape);
    }
    if touches.iter().next().is_some() || touches.iter_just_pressed().next().is_some() {
        state.enabled = true;
    }
    for k in std::mem::take(&mut state.tapped) {
        keys.release(k);
    }
    if !state.enabled {
        return;
    }
    let in_game = screen.is_some_and(|s| *s.get() == Screen::Hud);
    let Ok(win) = window.single() else { return };
    let scale = win.scale_factor();
    let width = win.width();

    for t in touches.iter_just_pressed() {
        let phys = t.position() * scale;
        if let Some(a) = button_at(&buttons, phys) {
            state.pressed_buttons.push((t.id(), a));
            let tap = match a {
                Action::Interact => Some(KeyCode::KeyE),
                Action::Light => Some(KeyCode::KeyF),
                Action::UseItem => Some(KeyCode::KeyQ),
                Action::Pause => Some(KeyCode::Escape),
                Action::Start => Some(KeyCode::Enter),
                Action::NextItem => {
                    let next = inv.as_ref().map_or(0, |i| (i.selected + 1) % crate::items::SLOTS);
                    Some([KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3][next])
                }
                Action::Crouch => {
                    state.crouch = !state.crouch;
                    None
                }
                Action::Sprint => {
                    state.sprint = !state.sprint;
                    None
                }
                Action::Skip => None,
            };
            if let Some(k) = tap {
                keys.press(k);
                state.tapped.push(k);
            }
        } else if in_game && t.position().x < width * 0.45 && state.stick.is_none() {
            state.stick = Some((t.id(), t.position()));
            state.stick_pos = t.position();
        } else if in_game && state.look.is_none() {
            state.look = Some(t.id());
        }
    }
    for t in touches.iter_just_released().chain(touches.iter_just_canceled()) {
        state.pressed_buttons.retain(|(id, _)| *id != t.id());
        if state.stick.is_some_and(|(id, _)| id == t.id()) {
            state.stick = None;
        }
        if state.look == Some(t.id()) {
            state.look = None;
        }
    }
    // Joystick → WASD.
    let mut want: Vec<KeyCode> = vec![];
    if let Some((id, origin)) = state.stick {
        if let Some(t) = touches.get_pressed(id) {
            state.stick_pos = t.position();
            let d = (t.position() - origin).clamp_length_max(STICK_RADIUS) / STICK_RADIUS;
            if d.y < -0.35 {
                want.push(KeyCode::KeyW);
            }
            if d.y > 0.35 {
                want.push(KeyCode::KeyS);
            }
            if d.x < -0.35 {
                want.push(KeyCode::KeyA);
            }
            if d.x > 0.35 {
                want.push(KeyCode::KeyD);
            }
        }
    }
    for k in MOVE_KEYS {
        let on = want.contains(&k);
        if on && !keys.pressed(k) {
            keys.press(k);
        } else if !on && keys.pressed(k) && state.stick.is_some() {
            keys.release(k);
        }
    }
    if state.stick.is_none() && !in_game {
        for k in MOVE_KEYS {
            if keys.pressed(k) {
                keys.release(k);
            }
        }
    }
    // Drag to look.
    if let Some(id) = state.look {
        if let Some(t) = touches.get_pressed(id) {
            look.0 += t.delta();
        }
    }
    // Toggles and holds.
    let hold = |keys: &mut ButtonInput<KeyCode>, k: KeyCode, on: bool| {
        if on && !keys.pressed(k) {
            keys.press(k);
        } else if !on && keys.pressed(k) {
            keys.release(k);
        }
    };
    hold(&mut keys, KeyCode::KeyC, state.crouch && in_game);
    hold(&mut keys, KeyCode::ShiftLeft, state.sprint && in_game && !state.crouch);
    let skipping = state.pressed_buttons.iter().any(|(_, a)| *a == Action::Skip);
    hold(&mut keys, KeyCode::Space, skipping);
}

#[allow(clippy::too_many_arguments)]
fn update_controls(
    state: Res<TouchState>,
    theme: Res<UiTheme>,
    screen: Option<Res<State<Screen>>>,
    lobby: Res<crate::lobby::LiftLobby>,
    cutscene: Option<Res<crate::cutscene::ActiveCutscene>>,
    mut root: Query<&mut Visibility, With<TouchRoot>>,
    mut buttons: Query<(&TouchButton, &mut Visibility, &mut BackgroundColor), Without<TouchRoot>>,
    mut base: Query<(&mut Node, &mut Visibility), (With<StickBase>, Without<TouchRoot>, Without<TouchButton>, Without<StickKnob>)>,
    mut knob: Query<(&mut Node, &mut Visibility), (With<StickKnob>, Without<TouchRoot>, Without<TouchButton>, Without<StickBase>)>,
    ui_scale: Res<UiScale>,
) {
    let k = ui_scale.0.max(0.01);
    let in_game = screen.is_some_and(|s| *s.get() == Screen::Hud);
    let show = state.enabled && in_game;
    for mut v in &mut root {
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
    let playing_cutscene = cutscene.is_some_and(|c| c.is_playing());
    for (b, mut v, mut bg) in &mut buttons {
        let visible = match b.0 {
            Action::Start => lobby.waiting && lobby.is_host,
            Action::Skip => playing_cutscene,
            _ => !playing_cutscene,
        };
        *v = if visible { Visibility::Inherited } else { Visibility::Hidden };
        let lit = (b.0 == Action::Crouch && state.crouch) || (b.0 == Action::Sprint && state.sprint) || state.pressed_buttons.iter().any(|(_, a)| *a == b.0);
        bg.0 = if lit { theme.interact.with_alpha(0.6) } else { theme.panel.with_alpha(0.55) };
    }
    let active = state.stick.map(|(_, o)| o);
    for (mut n, mut v) in &mut base {
        *v = if active.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(o) = active {
            n.left = px(o.x / k - STICK_RADIUS);
            n.top = px(o.y / k - STICK_RADIUS);
        }
    }
    for (mut n, mut v) in &mut knob {
        *v = if active.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(o) = active {
            let p = o + (state.stick_pos - o).clamp_length_max(STICK_RADIUS);
            n.left = px(p.x / k - 28.0);
            n.top = px(p.y / k - 28.0);
        }
    }
}

/// Shows the phone's on-screen keyboard while a text box (name, seed, room
/// code) is selected.
fn soft_keyboard(typing: Res<crate::ui::play_menu::Typing>, mut was: Local<bool>) {
    if typing.0 == *was {
        return;
    }
    *was = typing.0;
    #[cfg(target_os = "android")]
    if let Some(app) = bevy::android::ANDROID_APP.get() {
        if typing.0 {
            app.show_soft_input(true);
        } else {
            app.hide_soft_input(true);
        }
    }
}
