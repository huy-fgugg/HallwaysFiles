//! Text chat (T), microphone mute (M) and the speaker icon over people who
//! are talking.
//!
//! * **T** opens the chat line; type, **Enter** sends to everyone in the
//!   room, **Esc** closes. While it's open the game ignores the keyboard
//!   (no walking, no pause menu). Recent messages show bottom-left and fade.
//! * **M** mutes / unmutes your microphone (saved in Settings).
//! * Players you set to 0% in Settings (Players) are muted: no voice, and
//!   their chat messages are hidden.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::net::{NetSession, PeerEvent, PeerMsg, SendPeer};
use crate::settings::{save_settings, Settings};
use crate::state::{AppState, Overlay};
use crate::theme::UiTheme;

pub const MAX_LEN: usize = 120;
/// Lines shown at once, and how long a line stays when the chat is closed.
const SHOWN: usize = 7;
const FADE_AFTER: f32 = 12.0;

#[derive(Debug, Clone)]
pub struct Line {
    pub from: String,
    pub text: String,
    /// Seconds since it arrived.
    pub age: f32,
    pub mine: bool,
    /// A notice (announcement, mute) rather than someone talking.
    pub notice: bool,
}

#[derive(Resource, Debug, Default)]
pub struct Chat {
    pub open: bool,
    pub draft: String,
    pub lines: Vec<Line>,
    /// Bumped whenever what's on screen changes.
    pub version: u32,
}

impl Chat {
    pub fn push(&mut self, from: impl Into<String>, text: impl Into<String>, mine: bool, notice: bool) {
        self.lines.push(Line { from: from.into(), text: text.into(), age: 0.0, mine, notice });
        if self.lines.len() > 50 {
            self.lines.remove(0);
        }
        self.version = self.version.wrapping_add(1);
    }
}

/// Cleans a typed or received message: printable characters, trimmed, capped.
pub fn clean(text: &str) -> String {
    let t: String = text.chars().filter(|c| !c.is_control()).take(MAX_LEN).collect();
    t.trim().to_string()
}

/// An admin muted your chat: you can't send messages.
#[derive(Resource, Debug, Default)]
pub struct ChatMuted(pub bool);

#[derive(Component)]
struct ChatRoot;
#[derive(Component)]
struct MutedBadge;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chat>()
            .init_resource::<ChatMuted>()
            .add_systems(OnEnter(AppState::Playing), |mut chat: ResMut<Chat>| {
                chat.open = false;
                chat.draft.clear();
                chat.version = chat.version.wrapping_add(1);
            })
            .add_systems(OnExit(AppState::Playing), |mut chat: ResMut<Chat>, mut typing: ResMut<crate::ui::play_menu::Typing>| {
                if chat.open {
                    chat.open = false;
                    typing.0 = false;
                }
            })
            .add_systems(PreUpdate, chat_keys.after(bevy::input::InputSystems).after(crate::touch::TouchInput).run_if(in_state(AppState::Playing)))
            .add_systems(Update, (receive, age_lines, draw, muted_badge).chain().run_if(in_state(AppState::Playing)))
            .add_systems(Update, draw_notice);
    }
}

#[allow(clippy::too_many_arguments)]
fn chat_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    overlay: Res<State<Overlay>>,
    mut chat: ResMut<Chat>,
    mut typing: ResMut<crate::ui::play_menu::Typing>,
    mut settings: ResMut<Settings>,
    session: Res<NetSession>,
    mut out: MessageWriter<SendPeer>,
    mut subs: MessageWriter<crate::run::Subtitle>,
    muted_by_admin: Res<ChatMuted>,
) {
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();
    if !chat.open {
        // Only from the game itself (not menus, which have their own text boxes).
        if *overlay.get() != Overlay::None || typing.0 {
            return;
        }
        if keys.just_pressed(KeyCode::KeyT) {
            info!("chat opened");
            chat.open = true;
            chat.draft.clear();
            chat.version = chat.version.wrapping_add(1);
            typing.0 = true;
            keys.reset_all();
            return;
        }
        if keys.just_pressed(KeyCode::KeyM) {
            settings.mic_muted = !settings.mic_muted;
            save_settings(&settings);
            subs.write(crate::run::Subtitle::info(if settings.mic_muted { "Microphone muted (M to unmute)" } else { "Microphone on" }));
        }
        return;
    }
    if *overlay.get() != Overlay::None {
        chat.open = false;
        typing.0 = false;
        chat.version = chat.version.wrapping_add(1);
        return;
    }
    for e in events.iter().filter(|e| e.state.is_pressed()) {
        match &e.logical_key {
            Key::Enter => {
                let text = clean(&chat.draft);
                if !text.is_empty() && muted_by_admin.0 {
                    chat.push("*", "An admin muted your chat.", false, true);
                } else if !text.is_empty() {
                    if session.online() {
                        out.write(SendPeer(PeerMsg::Chat { text: text.clone() }));
                    }
                    info!("chat sent: {text}");
                    chat.push("You", text, true, false);
                }
                chat.draft.clear();
                chat.open = false;
                typing.0 = false;
            }
            Key::Escape => {
                chat.draft.clear();
                chat.open = false;
                typing.0 = false;
            }
            Key::Backspace => {
                chat.draft.pop();
            }
            Key::Space => {
                if chat.draft.chars().count() < MAX_LEN {
                    chat.draft.push(' ');
                }
            }
            Key::Character(s) => {
                for c in s.chars().filter(|c| !c.is_control()) {
                    if chat.draft.chars().count() < MAX_LEN {
                        chat.draft.push(c);
                    }
                }
            }
            _ => {}
        }
        chat.version = chat.version.wrapping_add(1);
    }
    // The game sees no keys while you type (no walking, no pause menu).
    keys.reset_all();
}

fn receive(mut events: MessageReader<PeerEvent>, session: Res<NetSession>, settings: Res<Settings>, mut chat: ResMut<Chat>, mut notice: ResMut<crate::net::NetNotice>) {
    for ev in events.read() {
        if let PeerMsg::Announce { text } = &ev.msg {
            let text = clean(text);
            if !text.is_empty() {
                notice.show(format!("ANNOUNCEMENT from {}:  {text}", session.name_of(ev.from)), Some(8.0), false);
                chat.push("ANNOUNCEMENT", text, false, true);
            }
            continue;
        }
        if let PeerMsg::Chat { text } = &ev.msg {
            let name = session.name_of(ev.from);
            let text = clean(text);
            if text.is_empty() || settings.is_muted(&name) {
                continue;
            }
            chat.push(name, text, false, false);
        }
    }
}

fn age_lines(time: Res<Time<Real>>, mut chat: ResMut<Chat>, mut last_shown: Local<usize>) {
    let dt = time.delta_secs();
    for l in &mut chat.lines {
        l.age += dt;
    }
    // Redraw when a line fades out.
    let shown = chat.lines.iter().filter(|l| l.age < FADE_AFTER).count();
    if shown != *last_shown {
        *last_shown = shown;
        chat.version = chat.version.wrapping_add(1);
    }
}

fn draw(mut commands: Commands, theme: Res<UiTheme>, chat: Res<Chat>, roots: Query<Entity, With<ChatRoot>>, time: Res<Time<Real>>, mut drawn: Local<(u32, bool)>) {
    // The caret blinks.
    let blink = (time.elapsed_secs() * 2.0) as i32 % 2 == 0;
    if drawn.0 == chat.version && drawn.1 == blink && !roots.is_empty() {
        return;
    }
    *drawn = (chat.version, blink);
    for r in &roots {
        commands.entity(r).despawn();
    }
    let visible: Vec<&Line> = chat.lines.iter().filter(|l| chat.open || l.age < FADE_AFTER).collect();
    let visible = &visible[visible.len().saturating_sub(SHOWN)..];
    if visible.is_empty() && !chat.open {
        return;
    }
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, left: px(24), bottom: px(150), width: px(520), flex_direction: FlexDirection::Column, row_gap: px(3), ..default() },
            GlobalZIndex(25),
            Pickable::IGNORE,
            ChatRoot,
            DespawnOnExit(AppState::Playing),
        ))
        .with_children(|c| {
            for l in visible {
                let name_color = if l.notice { Color::srgb(1.0, 0.8, 0.35) } else if l.mine { theme.interact } else { Color::srgb(0.6, 0.85, 1.0) };
                c.spawn((Node { padding: UiRect::axes(px(8), px(3)), border_radius: BorderRadius::all(px(4)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), Pickable::IGNORE)).with_children(|row| {
                    let who = if l.notice { format!("{}  ", l.from) } else { format!("{}:  ", l.from) };
                    row.spawn((Text::new(who), theme.font(true, theme.size_small), TextColor(name_color), Pickable::IGNORE)).with_child((TextSpan::new(l.text.clone()), theme.font(false, theme.size_small), TextColor(theme.text)));
                });
            }
            if chat.open {
                c.spawn((
                    Node { padding: UiRect::axes(px(10), px(6)), margin: UiRect::top(px(4)), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(6)), ..default() },
                    BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.85)),
                    BorderColor::all(theme.interact),
                    Pickable::IGNORE,
                ))
                .with_child((Text::new(format!("SAY:  {}{}", chat.draft, if blink { "_" } else { " " })), theme.font(true, theme.size_small), TextColor(theme.text), Pickable::IGNORE));
                c.spawn((Text::new("Enter to send  -  Esc to close"), theme.font(false, theme.size_small * 0.8), TextColor(theme.text_dim), Pickable::IGNORE));
            }
        });
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

/// "MIC MUTED" reminder while you're in an online room with your mic off.
fn muted_badge(mut commands: Commands, settings: Res<Settings>, session: Res<NetSession>, theme: Res<UiTheme>, assets: Res<AssetServer>, badge: Query<Entity, With<MutedBadge>>) {
    let want = settings.mic_muted && session.online();
    match (want, badge.single().ok()) {
        (true, None) => {
            commands
                .spawn((
                    Node { position_type: PositionType::Absolute, left: px(24), bottom: px(104), padding: UiRect::axes(px(8), px(3)), column_gap: px(6), align_items: AlignItems::Center, border_radius: BorderRadius::all(px(6)), ..default() },
                    BackgroundColor(Color::srgba(0.35, 0.05, 0.05, 0.7)),
                    GlobalZIndex(30),
                    MutedBadge,
                    Pickable::IGNORE,
                    DespawnOnExit(AppState::Playing),
                ))
                .with_children(|b| {
                    b.spawn((ImageNode::new(assets.load("ui/mic_off.png")), Node { width: px(22), height: px(22), ..default() }, Pickable::IGNORE));
                    b.spawn((Text::new("MIC MUTED (M)"), theme.font(true, theme.size_small * 0.85), TextColor(Color::WHITE), Pickable::IGNORE));
                });
        }
        (false, Some(e)) => commands.entity(e).despawn(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_cleaned_and_capped() {
        assert_eq!(clean("  hi\n\tthere  "), "hithere");
        assert_eq!(clean("  hello  "), "hello");
        assert_eq!(clean("a\u{7}b"), "ab");
        assert_eq!(clean(&"x".repeat(500)).len(), MAX_LEN);
        assert_eq!(clean("   "), "");
    }

    #[test]
    fn history_is_kept_short() {
        let mut c = Chat::default();
        for i in 0..80 {
            c.push("A", format!("{i}"), false, false);
        }
        assert_eq!(c.lines.len(), 50);
        assert_eq!(c.lines.last().unwrap().text, "79");
    }

    #[test]
    fn player_volumes_step_and_save_by_name() {
        let mut s = Settings::default();
        assert_eq!(s.player_volume("Ana"), 100);
        s.step_player_volume("Ana", 1);
        assert_eq!(s.player_volume("ana"), 150, "names ignore case");
        for _ in 0..2 {
            s.step_player_volume("Ana", 1);
        }
        assert!(s.is_muted("Ana"), "wraps round to muted");
        let back = Settings::from_ron(&s.to_ron());
        assert!(back.is_muted("ANA"), "saved in the settings file");
        s.step_player_volume("Ana", -1);
        s.step_player_volume("Ana", 1);
        assert!(s.is_muted("Ana"));
    }
}
