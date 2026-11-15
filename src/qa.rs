//! QA helpers, only active when switched on by environment variables.
//!
//! * `HALLWAYS_QA=1` logs every visible menu control with its label and
//!   on-screen position (`QA ui "LABEL" x y`, window pixels), so scripted
//!   playtests can click options by name like a player would.
//! * `HALLWAYS_FAKE_TOUCH=1` emulates a phone: the mouse becomes a finger
//!   (press = touch down, drag = touch move, release = lift). Mouse picking and
//!   mouse-look are turned off, so menus, the joystick, look-drag and the
//!   on-screen buttons all go through the same touch code as on a phone.

use std::collections::HashMap;

use bevy::input::mouse::MouseButtonInput;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::input::ButtonState;
use bevy::picking::input::PointerInputSettings;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::{CursorMoved, PrimaryWindow};

use crate::ui::focus::Focusable;

/// True when the mouse is standing in for a finger (QA phone emulation).
pub fn fake_touch() -> bool {
    std::env::var_os("HALLWAYS_FAKE_TOUCH").is_some()
}

/// Extra controls reported to scripted playtests (e.g. on-screen touch buttons).
#[derive(Component)]
pub struct QaTarget;

pub struct QaPlugin;

impl Plugin for QaPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("HALLWAYS_QA").is_some() || fake_touch() {
            app.add_systems(Update, report_controls);
        }
        if fake_touch() {
            app.insert_resource(PointerInputSettings { is_touch_enabled: true, is_mouse_enabled: false })
                .add_systems(PreUpdate, mouse_as_finger.before(bevy::input::InputSystems));
        }
    }
}

/// Text of the first `Text` under a control (its label).
fn label_of(e: Entity, children: &Query<&Children>, texts: &Query<&Text>) -> String {
    let mut stack = vec![e];
    while let Some(n) = stack.pop() {
        if let Ok(t) = texts.get(n) {
            if !t.0.trim().is_empty() {
                return t.0.trim().to_string();
            }
        }
        if let Ok(k) = children.get(n) {
            stack.extend(k.iter().rev());
        }
    }
    String::new()
}

#[allow(clippy::type_complexity)]
fn report_controls(
    window: Query<&Window, With<PrimaryWindow>>,
    controls: Query<(Entity, &ComputedNode, &UiGlobalTransform, &InheritedVisibility), Or<(With<Focusable>, With<QaTarget>)>>,
    children: Query<&Children>,
    texts: Query<&Text>,
    mut last: Local<HashMap<String, (i32, i32)>>,
    mut t: Local<f32>,
    time: Res<Time<Real>>,
) {
    *t += time.delta_secs();
    if *t < 0.5 {
        return;
    }
    *t = 0.0;
    let Ok(win) = window.single() else { return };
    let k = 1.0 / win.scale_factor();
    let mut now = HashMap::new();
    for (e, c, tf, vis) in &controls {
        if !vis.get() || c.size.x < 1.0 {
            continue;
        }
        let label = label_of(e, &children, &texts);
        let p = tf.translation * k;
        if p.x < 0.0 || p.y < 0.0 || p.x > win.width() || p.y > win.height() {
            continue;
        }
        let mut key = label.clone();
        let mut n = 2;
        while now.contains_key(&key) {
            key = format!("{label} #{n}");
            n += 1;
        }
        now.insert(key, (p.x.round() as i32, p.y.round() as i32));
    }
    if *last != now {
        let mut lines: Vec<_> = now.iter().map(|(l, (x, y))| format!("QA ui \"{l}\" {x} {y}")).collect();
        lines.sort();
        info!("QA screen ({} controls)\n{}", lines.len(), lines.join("\n"));
        *last = now;
    }
}

/// Phone emulation: turns left-mouse presses, drags and releases into finger
/// touches on the primary window.
fn mouse_as_finger(
    mut buttons: MessageReader<MouseButtonInput>,
    mut moves: MessageReader<CursorMoved>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut out: MessageWriter<TouchInput>,
    mut down: Local<bool>,
    mut pos: Local<Vec2>,
) {
    let Ok((win_e, win)) = window.single() else { return };
    if let Some(p) = win.cursor_position() {
        *pos = p;
    }
    let mut moved = false;
    for m in moves.read() {
        *pos = m.position;
        moved = true;
    }
    let touch = |phase| TouchInput { phase, position: *pos, window: win_e, force: None, id: 7 };
    for b in buttons.read() {
        if b.button != MouseButton::Left {
            continue;
        }
        match b.state {
            ButtonState::Pressed if !*down => {
                *down = true;
                out.write(touch(TouchPhase::Started));
            }
            ButtonState::Released if *down => {
                *down = false;
                out.write(touch(TouchPhase::Ended));
            }
            _ => {}
        }
    }
    if moved && *down {
        out.write(touch(TouchPhase::Moved));
    }
}
