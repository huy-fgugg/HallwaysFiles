//! Scrolling for menu panels that are taller than the screen: mouse wheel
//! over a panel, dragging on a touch screen, and keyboard/gamepad focus
//! (moving to an off-screen option scrolls it into view). A thin scrollbar
//! shows next to any panel that has more to see.

use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input::touch::Touches;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

/// Logical pixels per wheel "line".
const LINE: f32 = 48.0;

#[derive(Component)]
struct Thumb(Entity);
#[derive(Component)]
struct HasThumb;

pub struct ScrollPlugin;

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (wheel_and_touch, follow_focus, attach_thumbs, place_thumbs).chain());
    }
}

/// Max scroll (logical px) of a node.
fn max_scroll(node: &ComputedNode) -> f32 {
    ((node.content_size.y - node.size.y) * node.inverse_scale_factor).max(0.0)
}

fn scrolls(node: &Node) -> bool {
    node.overflow.y == OverflowAxis::Scroll
}

#[allow(clippy::type_complexity)]
fn wheel_and_touch(
    wheel: Res<AccumulatedMouseScroll>,
    touches: Res<Touches>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut panels: Query<(&Node, &ComputedNode, &UiGlobalTransform, &InheritedVisibility, &mut ScrollPosition)>,
) {
    let Ok(win) = window.single() else { return };
    let scale = win.scale_factor();
    let wheel_dy = match wheel.unit {
        MouseScrollUnit::Line => wheel.delta.y * LINE,
        MouseScrollUnit::Pixel => wheel.delta.y,
    };
    let cursor = win.cursor_position().map(|c| c * scale);
    // Touch: one finger dragging inside a panel scrolls it.
    let drag: Option<(Vec2, f32)> = touches.iter().find(|t| t.delta().y.abs() > 0.0).map(|t| (t.position() * scale, t.delta().y));
    // With a single scrollable panel on screen, the wheel scrolls it from anywhere.
    let only_one = panels.iter().filter(|(n, c, _, v, _)| scrolls(n) && v.get() && max_scroll(c) > 0.0).count() == 1;
    for (node, computed, tf, vis, mut pos) in &mut panels {
        if !scrolls(node) || !vis.get() {
            continue;
        }
        let max = max_scroll(computed);
        if max <= 0.0 {
            continue;
        }
        let mut dy = 0.0;
        if wheel_dy != 0.0 && (cursor.is_some_and(|c| computed.contains_point(*tf, c)) || only_one) {
            dy -= wheel_dy;
        }
        if let Some((p, d)) = drag {
            if computed.contains_point(*tf, p) {
                dy -= d;
            }
        }
        if dy != 0.0 {
            pos.0.y = (pos.0.y + dy).clamp(0.0, max);
        }
    }
}

/// Keeps the focused menu item visible inside its scrolling panel.
fn follow_focus(
    focus: Res<InputFocus>,
    parents: Query<&ChildOf>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut panels: Query<(&Node, &ComputedNode, &UiGlobalTransform, &mut ScrollPosition)>,
) {
    if !focus.is_changed() {
        return;
    }
    let Some(target) = focus.get() else { return };
    let Ok((tnode, ttf)) = nodes.get(target) else { return };
    // Nearest scrolling ancestor.
    let mut e = target;
    while let Ok(p) = parents.get(e) {
        e = p.parent();
        let Ok((node, pc, ptf, mut pos)) = panels.get_mut(e) else { continue };
        if !scrolls(node) {
            continue;
        }
        let k = pc.inverse_scale_factor;
        let item_top = (ttf.translation.y - tnode.size.y / 2.0) * k;
        let item_bottom = (ttf.translation.y + tnode.size.y / 2.0) * k;
        let pad = 12.0;
        let view_top = (ptf.translation.y - pc.size.y / 2.0) * k + pad;
        let view_bottom = (ptf.translation.y + pc.size.y / 2.0) * k - pad;
        let max = max_scroll(pc);
        if item_top < view_top {
            pos.0.y = (pos.0.y - (view_top - item_top)).clamp(0.0, max);
        } else if item_bottom > view_bottom {
            pos.0.y = (pos.0.y + (item_bottom - view_bottom)).clamp(0.0, max);
        }
        break;
    }
}

fn attach_thumbs(mut commands: Commands, panels: Query<(Entity, &Node), (With<ScrollPosition>, Without<HasThumb>)>, theme: Option<Res<crate::theme::UiTheme>>) {
    let Some(theme) = theme else { return };
    for (e, node) in &panels {
        if !scrolls(node) {
            continue;
        }
        commands.entity(e).insert(HasThumb);
        commands.spawn((
            Node { position_type: PositionType::Absolute, width: px(4), height: px(40), border_radius: BorderRadius::all(px(2)), ..default() },
            BackgroundColor(theme.interact.with_alpha(0.7)),
            GlobalZIndex(80),
            Visibility::Hidden,
            Pickable::IGNORE,
            Thumb(e),
        ));
    }
}

fn place_thumbs(
    mut commands: Commands,
    panels: Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility, &ScrollPosition)>,
    mut thumbs: Query<(Entity, &Thumb, &mut Node, &mut Visibility)>,
) {
    for (e, thumb, mut node, mut vis) in &mut thumbs {
        let Ok((c, tf, pvis, pos)) = panels.get(thumb.0) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let max = max_scroll(c);
        if max <= 0.0 || !pvis.get() {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        // Physical pixels → UI units (inverse_scale_factor already includes UiScale).
        let k = c.inverse_scale_factor;
        let h = c.size.y * k;
        let top = (tf.translation.y - c.size.y / 2.0) * k;
        let right_edge = (tf.translation.x + c.size.x / 2.0) * k;
        let view = h / (h + max);
        let bar_h = (h * view).max(24.0);
        let t = (pos.0.y / max).clamp(0.0, 1.0);
        node.height = px(bar_h - 8.0);
        node.top = px(top + 4.0 + (h - bar_h) * t);
        node.left = px(right_edge - 8.0);
    }
}
