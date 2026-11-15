//! Touch input for menus (bevy_picking), robust to slow frames.
//!
//! Bevy matches a touch release against what was under the finger on the
//! *previous* frame. A quick tap that starts and ends within one frame (easy on
//! a phone running at 15-20 fps) therefore never clicks anything. This replaces
//! Bevy's touch-to-pointer step with the same logic, except that a release in
//! the same frame as its press is sent one frame later, so every tap counts.

use bevy::camera::RenderTarget;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::picking::input::PointerInputSettings;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation};
use bevy::picking::PickingSystems;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent, WindowRef};

pub struct TapPlugin;

impl Plugin for TapPlugin {
    fn build(&self, app: &mut App) {
        let mouse = !crate::qa::fake_touch();
        app.insert_resource(PointerInputSettings { is_touch_enabled: false, is_mouse_enabled: mouse })
            .init_resource::<FinishedTouches>()
            .add_systems(First, touch_pointers.in_set(PickingSystems::Input))
            .add_systems(Last, remove_finished_pointers);
    }
}

/// Touch ids whose pointer can be removed at the end of this frame.
#[derive(Resource, Default)]
struct FinishedTouches(Vec<u64>);

#[derive(Default)]
struct TapState {
    last: HashMap<u64, TouchInput>,
    /// Releases held back one frame (pressed and released in the same frame).
    held: Vec<(u64, Location)>,
}

fn touch_pointers(
    mut events: MessageReader<WindowEvent>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut state: Local<TapState>,
    mut finished: ResMut<FinishedTouches>,
    mut commands: Commands,
    mut out: MessageWriter<PointerInput>,
) {
    for (id, location) in std::mem::take(&mut state.held) {
        out.write(PointerInput::new(PointerId::Touch(id), location, PointerAction::Release(PointerButton::Primary)));
        finished.0.push(id);
    }
    let mut started_now: HashSet<u64> = HashSet::default();
    for event in events.read() {
        let WindowEvent::TouchInput(touch) = event else { continue };
        let pointer = PointerId::Touch(touch.id);
        let Some(target) = RenderTarget::Window(WindowRef::Entity(touch.window)).normalize(primary.single().ok()) else { continue };
        let location = Location { target, position: touch.position };
        match touch.phase {
            TouchPhase::Started => {
                commands.spawn((pointer, PointerLocation::new(location.clone())));
                out.write(PointerInput::new(pointer, location, PointerAction::Press(PointerButton::Primary)));
                state.last.insert(touch.id, *touch);
                started_now.insert(touch.id);
            }
            TouchPhase::Moved => {
                if let Some(last) = state.last.get(&touch.id) {
                    if last == touch {
                        continue;
                    }
                    let delta = touch.position - last.position;
                    out.write(PointerInput::new(pointer, location, PointerAction::Move { delta }));
                }
                state.last.insert(touch.id, *touch);
            }
            TouchPhase::Ended => {
                state.last.remove(&touch.id);
                if started_now.contains(&touch.id) {
                    state.held.push((touch.id, location));
                } else {
                    out.write(PointerInput::new(pointer, location, PointerAction::Release(PointerButton::Primary)));
                    finished.0.push(touch.id);
                }
            }
            TouchPhase::Canceled => {
                state.last.remove(&touch.id);
                out.write(PointerInput::new(pointer, location, PointerAction::Cancel));
                finished.0.push(touch.id);
            }
        }
    }
}

fn remove_finished_pointers(mut commands: Commands, mut finished: ResMut<FinishedTouches>, pointers: Query<(Entity, &PointerId)>) {
    for id in finished.0.drain(..) {
        for (e, p) in &pointers {
            if p.get_touch_id() == Some(id) {
                commands.entity(e).try_despawn();
            }
        }
    }
}
