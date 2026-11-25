//! Settings screen (reachable from the main menu and the pause menu).

use bevy::prelude::*;

use super::focus::{
    spawn_button, AdjustArrow, AdjustRequested, Adjustable, Focusable, InitialFocus, MenuAction, MenuActivated,
};
use crate::settings::{save_settings, Settings};
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingField {
    MouseSensitivity,
    InvertY,
    MasterVolume,
    SfxVolume,
    MusicVolume,
    SpookyIntensity,
    Subtitles,
    ReduceFlashing,
    LimitLoudSounds,
    Fullscreen,
    Graphics,
    VoiceChat,
    VoiceVolume,
    /// Your microphone (M in game).
    Mic,
    /// Volume of the N-th other player in your online room (0 = muted).
    Player(u8),
}

/// Names of the other players in your online room (for the Player rows).
#[derive(Resource, Debug, Default, Clone)]
pub struct RoomNames(pub Vec<String>);

impl SettingField {
    pub const ALL: [SettingField; 14] = [
        SettingField::Graphics,
        SettingField::MouseSensitivity,
        SettingField::InvertY,
        SettingField::MasterVolume,
        SettingField::SfxVolume,
        SettingField::MusicVolume,
        SettingField::VoiceChat,
        SettingField::VoiceVolume,
        SettingField::Mic,
        SettingField::SpookyIntensity,
        SettingField::Subtitles,
        SettingField::ReduceFlashing,
        SettingField::LimitLoudSounds,
        SettingField::Fullscreen,
    ];

    /// Row label (players' rows show their name).
    pub fn label_for(self, names: &RoomNames) -> String {
        match self {
            SettingField::Player(i) => names.0.get(i as usize).map_or("Player".into(), |n| format!("  {n}")),
            f => f.label().into(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SettingField::MouseSensitivity => "Look sensitivity",
            SettingField::InvertY => "Invert look Y",
            SettingField::MasterVolume => "Master volume",
            SettingField::SfxVolume => "Effects volume",
            SettingField::MusicVolume => "Ambience volume",
            SettingField::SpookyIntensity => "Spooky intensity",
            SettingField::Subtitles => "Subtitles",
            SettingField::ReduceFlashing => "Reduce flashing",
            SettingField::LimitLoudSounds => "Limit loud sounds",
            SettingField::Fullscreen => "Display mode",
            SettingField::Graphics => "Graphics quality",
            SettingField::VoiceChat => "Voice chat",
            SettingField::VoiceVolume => "Voice volume",
            SettingField::Mic => "Microphone",
            SettingField::Player(_) => "Player",
        }
    }

    pub fn value_text_for(self, s: &Settings, names: &RoomNames) -> String {
        match self {
            SettingField::Player(i) => match names.0.get(i as usize) {
                Some(n) => match s.player_volume(n) {
                    0 => "MUTED".into(),
                    v => format!("{v}%"),
                },
                None => "-".into(),
            },
            f => f.value_text(s),
        }
    }

    pub fn value_text(self, s: &Settings) -> String {
        fn on_off(b: bool) -> String {
            if b { "ON".into() } else { "OFF".into() }
        }
        match self {
            SettingField::MouseSensitivity => format!("{:.1}x", s.mouse_sensitivity),
            SettingField::InvertY => on_off(s.invert_y),
            SettingField::MasterVolume => format!("{}%", s.master_volume),
            SettingField::SfxVolume => format!("{}%", s.sfx_volume),
            SettingField::MusicVolume => format!("{}%", s.music_volume),
            SettingField::SpookyIntensity => format!("{}%", s.spooky_intensity),
            SettingField::Subtitles => on_off(s.subtitles),
            SettingField::ReduceFlashing => on_off(s.reduce_flashing),
            SettingField::LimitLoudSounds => on_off(s.limit_loud_sounds),
            SettingField::Fullscreen => if s.fullscreen { "FULLSCREEN".into() } else { "WINDOWED".into() },
            SettingField::Graphics => s.graphics.label().into(),
            SettingField::VoiceChat => s.voice.label().into(),
            SettingField::VoiceVolume => format!("{}%", s.voice_volume),
            SettingField::Mic => if s.mic_muted { "MUTED".into() } else { "ON".into() },
            SettingField::Player(_) => "-".into(),
        }
    }

    /// Applies a change, with the room's names for the Player rows.
    pub fn apply_for(self, s: &mut Settings, delta: i32, names: &RoomNames) {
        match self {
            SettingField::Player(i) => {
                if let Some(n) = names.0.get(i as usize) {
                    s.step_player_volume(n, delta);
                }
            }
            f => f.apply(s, delta),
        }
    }

    /// Applies a change: `delta` is -1/+1 for steps, 0 for toggle/activate.
    pub fn apply(self, s: &mut Settings, delta: i32) {
        fn step_pct(v: &mut u32, delta: i32) {
            let d = if delta == 0 { 10 } else { delta * 10 };
            let mut n = *v as i32 + d;
            if delta == 0 && n > 100 {
                n = 0; // Enter cycles back to 0
            }
            *v = n.clamp(0, 100) as u32;
        }
        match self {
            SettingField::MouseSensitivity => {
                let d = if delta == 0 { 0.1 } else { delta as f32 * 0.1 };
                let mut v = s.mouse_sensitivity + d;
                if delta == 0 && v > 3.05 {
                    v = 0.1;
                }
                s.mouse_sensitivity = ((v * 10.0).round() / 10.0).clamp(0.1, 3.0);
            }
            SettingField::InvertY => s.invert_y = !s.invert_y,
            SettingField::MasterVolume => step_pct(&mut s.master_volume, delta),
            SettingField::SfxVolume => step_pct(&mut s.sfx_volume, delta),
            SettingField::MusicVolume => step_pct(&mut s.music_volume, delta),
            SettingField::SpookyIntensity => step_pct(&mut s.spooky_intensity, delta),
            SettingField::Subtitles => s.subtitles = !s.subtitles,
            SettingField::ReduceFlashing => s.reduce_flashing = !s.reduce_flashing,
            SettingField::LimitLoudSounds => s.limit_loud_sounds = !s.limit_loud_sounds,
            SettingField::Fullscreen => s.fullscreen = !s.fullscreen,
            SettingField::Graphics => s.graphics = s.graphics.step(delta),
            SettingField::VoiceChat => s.voice = s.voice.step(delta),
            SettingField::VoiceVolume => step_pct(&mut s.voice_volume, delta),
            SettingField::Mic => s.mic_muted = !s.mic_muted,
            SettingField::Player(_) => {}
        }
    }
}

#[derive(Component)]
struct SettingValue(SettingField);

pub struct SettingsMenuPlugin;

impl Plugin for SettingsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomNames>()
            .add_systems(OnEnter(Screen::Settings), spawn_settings)
            .add_systems(OnExit(Screen::Settings), persist)
            .add_systems(
                Update,
                (handle_adjust, handle_actions, refresh_values, back_on_escape).run_if(in_state(Screen::Settings)),
            );
    }
}

fn spawn_settings(mut commands: Commands, theme: Res<UiTheme>, settings: Res<Settings>, app: Res<State<AppState>>, session: Res<crate::net::NetSession>) {
    let opaque = *app.get() == AppState::MainMenu;
    // The other players in your online room, for their volume rows.
    let names = RoomNames(if session.online() { session.players.iter().filter(|p| p.id != session.my_id).map(|p| p.name.clone()).collect() } else { vec![] });
    let mut fields: Vec<SettingField> = SettingField::ALL.to_vec();
    let at = fields.iter().position(|f| *f == SettingField::Mic).map_or(fields.len(), |i| i + 1);
    for i in (0..names.0.len()).rev() {
        fields.insert(at, SettingField::Player(i as u8));
    }
    commands.insert_resource(names.clone());
    commands
        .spawn((theme.screen_root(opaque), GlobalZIndex(20), DespawnOnExit(Screen::Settings), Name::new("Settings")))
        .with_children(|root| {
            root.spawn(theme.heading_text("SETTINGS"));
            root.spawn(theme.panel(640.0)).with_children(|panel| {
                for (i, field) in fields.iter().copied().enumerate() {
                    if field == SettingField::Player(0) {
                        panel.spawn((Text::new("PLAYERS IN THIS ROOM  (their voice volume; MUTED also hides their chat)"), theme.font(true, theme.size_small * 0.85), TextColor(theme.interact), Node { margin: UiRect::top(px(6)), ..default() }));
                    }
                    let mut row = panel.spawn((theme.row(), Focusable(i as i32), Adjustable(field), Name::new(field.label())));
                    if i == 0 {
                        row.insert(InitialFocus);
                    }
                    let row_id = row.id();
                    row.with_children(|r| {
                        r.spawn((Text::new(field.label_for(&names)), theme.font(false, theme.size_body), TextColor(theme.text)));
                        r.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), ..default() })
                            .with_children(|v| {
                                v.spawn((
                                    Text::new("<"),
                                    theme.font(true, theme.size_body),
                                    TextColor(theme.text_dim),
                                    AdjustArrow { row: row_id, delta: -1 },
                                    Node { padding: UiRect::horizontal(px(8)), ..default() },
                                ));
                                v.spawn((
                                    Text::new(field.value_text_for(&settings, &names)),
                                    theme.font(true, theme.size_body),
                                    TextColor(theme.interact),
                                    Node { min_width: px(150), justify_content: JustifyContent::Center, ..default() },
                                    TextLayout { justify: Justify::Center, ..default() },
                                    SettingValue(field),
                                ));
                                v.spawn((
                                    Text::new(">"),
                                    theme.font(true, theme.size_body),
                                    TextColor(theme.text_dim),
                                    AdjustArrow { row: row_id, delta: 1 },
                                    Node { padding: UiRect::horizontal(px(8)), ..default() },
                                ));
                            });
                    });
                }
            });
            root.spawn(Node { column_gap: px(theme.gap), ..default() }).with_children(|row| {
                spawn_button(row, &theme, "Reset defaults", MenuAction::ResetSettings, 100, false);
                spawn_button(row, &theme, "Back", MenuAction::CloseSettings, 101, false);
            });
            if !session.online() {
                root.spawn(theme.dim_text("Join an online room to set other players' volumes here."));
            }
            root.spawn(theme.dim_text("Arrows / D-pad to navigate  -  Left/Right to change  -  Esc to go back"));
        });
}

fn handle_adjust(mut reader: MessageReader<AdjustRequested>, mut settings: ResMut<Settings>, names: Res<RoomNames>) {
    for req in reader.read() {
        req.field.apply_for(&mut settings, req.delta, &names);
    }
}

fn refresh_values(settings: Res<Settings>, names: Res<RoomNames>, mut q: Query<(&SettingValue, &mut Text)>) {
    if !settings.is_changed() {
        return;
    }
    for (v, mut text) in &mut q {
        let s = v.0.value_text_for(&settings, &names);
        if text.0 != s {
            text.0 = s;
        }
    }
}

fn close(app: &AppState, next: &mut NextState<Overlay>) {
    next.set(if *app == AppState::Playing { Overlay::Pause } else { Overlay::None });
}

fn handle_actions(
    mut reader: MessageReader<MenuActivated>,
    mut settings: ResMut<Settings>,
    app: Res<State<AppState>>,
    mut next: ResMut<NextState<Overlay>>,
) {
    for MenuActivated(action) in reader.read() {
        match action {
            MenuAction::CloseSettings => close(app.get(), &mut next),
            // Keeps your per-player volumes (those are about people, not options).
            MenuAction::ResetSettings => *settings = Settings { player_volumes: std::mem::take(&mut settings.player_volumes), ..Settings::default() },
            _ => {}
        }
    }
}

fn back_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    app: Res<State<AppState>>,
    mut next: ResMut<NextState<Overlay>>,
) {
    if keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::East)) {
        close(app.get(), &mut next);
    }
}

fn persist(settings: Res<Settings>) {
    save_settings(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_steps_clamp() {
        let mut s = Settings { master_volume: 95, ..default() };
        SettingField::MasterVolume.apply(&mut s, 1);
        assert_eq!(s.master_volume, 100);
        SettingField::MasterVolume.apply(&mut s, 1);
        assert_eq!(s.master_volume, 100);
        s.master_volume = 5;
        SettingField::MasterVolume.apply(&mut s, -1);
        assert_eq!(s.master_volume, 0);
    }

    #[test]
    fn enter_cycles_percentages() {
        let mut s = Settings { spooky_intensity: 100, ..default() };
        SettingField::SpookyIntensity.apply(&mut s, 0);
        assert_eq!(s.spooky_intensity, 0);
    }

    #[test]
    fn toggles_flip() {
        let mut s = Settings::default();
        SettingField::Subtitles.apply(&mut s, 1);
        assert!(!s.subtitles);
        SettingField::Subtitles.apply(&mut s, 0);
        assert!(s.subtitles);
    }

    #[test]
    fn sensitivity_bounds() {
        let mut s = Settings { mouse_sensitivity: 0.1, ..default() };
        SettingField::MouseSensitivity.apply(&mut s, -1);
        assert_eq!(s.mouse_sensitivity, 0.1);
        s.mouse_sensitivity = 3.0;
        SettingField::MouseSensitivity.apply(&mut s, 1);
        assert_eq!(s.mouse_sensitivity, 3.0);
    }
}
