//! The big message across the top of the screen (connection lost, removed
//! from the room, host changes) and admin announcements from the room.

use bevy::prelude::*;

use crate::net::{NetSession, PeerEvent, PeerMsg};
use crate::theme::UiTheme;

/// Longest announcement shown.
pub const MAX_LEN: usize = 120;

/// Cleans a typed or received message: printable characters, trimmed, capped.
pub fn clean(text: &str) -> String {
    let t: String = text.chars().filter(|c| !c.is_control()).take(MAX_LEN).collect();
    t.trim().to_string()
}

pub struct NoticePlugin;

impl Plugin for NoticePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (receive, draw_notice).chain());
    }
}

/// Announcements sent from the admin panel by someone in the room.
fn receive(mut events: MessageReader<PeerEvent>, session: Res<NetSession>, mut notice: ResMut<crate::net::NetNotice>) {
    for ev in events.read() {
        if let PeerMsg::Announce { text } = &ev.msg {
            let text = clean(text);
            if !text.is_empty() {
                notice.show(format!("ANNOUNCEMENT from {}:  {text}", session.name_of(ev.from)), Some(8.0), false);
            }
        }
    }
}

#[derive(Component)]
struct NoticeRoot;

/// The big message across the top (connection lost, removed, announcements).
fn draw_notice(mut commands: Commands, theme: Res<UiTheme>, notice: Res<crate::net::NetNotice>, roots: Query<Entity, With<NoticeRoot>>, mut drawn: Local<u32>) {
    if *drawn == notice.version && (notice.text.is_empty() == roots.is_empty()) {
        return;
    }
    *drawn = notice.version;
    for r in &roots {
        commands.entity(r).despawn();
    }
    if notice.text.is_empty() {
        return;
    }
    let (bg, edge) = if notice.danger { (Color::srgba(0.35, 0.04, 0.04, 0.92), Color::srgb(1.0, 0.4, 0.35)) } else { (Color::srgba(0.08, 0.1, 0.2, 0.92), Color::srgb(1.0, 0.82, 0.4)) };
    commands
        .spawn((Node { position_type: PositionType::Absolute, top: px(56), width: percent(100), justify_content: JustifyContent::Center, ..default() }, GlobalZIndex(60), Pickable::IGNORE, NoticeRoot))
        .with_children(|r| {
            r.spawn((Node { max_width: px(900), padding: UiRect::axes(px(18), px(10)), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(bg), BorderColor::all(edge), Pickable::IGNORE))
                .with_child((Text::new(notice.text.clone()), theme.font(true, theme.size_body), TextColor(Color::WHITE), TextLayout::justify(Justify::Center), Pickable::IGNORE));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcements_are_cleaned_and_capped() {
        assert_eq!(clean("  hi\u{7}there \n"), "hithere");
        assert_eq!(clean(&"x".repeat(500)).len(), MAX_LEN);
    }
}
