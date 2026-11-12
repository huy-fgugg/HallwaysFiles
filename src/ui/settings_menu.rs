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
}

impl SettingField {
    pub const ALL: [SettingField; 10] = [
        SettingField::MouseSensitivity,
        SettingField::InvertY,
        SettingField::MasterVolume,
        SettingField::SfxVolume,
        SettingField::MusicVolume,
        SettingField::SpookyIntensity,
        SettingField::Subtitles,
        SettingField::ReduceFlashing,
        SettingField::LimitLoudSounds,
        SettingField::Fullscreen,
    ];

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
        }
    }
}

#[derive(Component)]
struct SettingValue(SettingField);

pub struct SettingsMenuPlugin;

impl Plugin for SettingsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Settings), spawn_settings)
            .add_systems(OnExit(Screen::Settings), persist)
            .add_systems(
                Update,
                (handle_adjust, handle_actions, refresh_values, back_on_escape).run_if(in_state(Screen::Settings)),
            );
    }
}

fn spawn_settings(mut commands: Commands, theme: Res<UiTheme>, settings: Res<Settings>, app: Res<State<AppState>>) {
    let opaque = *app.get() == AppState::MainMenu;
    commands
        .spawn((theme.screen_root(opaque), GlobalZIndex(20), DespawnOnExit(Screen::Settings), Name::new("Settings")))
        .with_children(|root| {
            root.spawn(theme.heading_text("SETTINGS"));
            root.spawn(theme.panel(640.0)).with_children(|panel| {
                for (i, field) in SettingField::ALL.iter().copied().enumerate() {
                    let mut row = panel.spawn((theme.row(), Focusable(i as i32), Adjustable(field), Name::new(field.label())));
                    if i == 0 {
                        row.insert(InitialFocus);
                    }
                    let row_id = row.id();
                    row.with_children(|r| {
                        r.spawn((Text::new(field.label()), theme.font(false, theme.size_body), TextColor(theme.text)));
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
                                    Text::new(field.value_text(&settings)),
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
            root.spawn(theme.dim_text("Arrows / D-pad to navigate  -  Left/Right to change  -  Esc to go back"));
        });
}

fn handle_adjust(mut reader: MessageReader<AdjustRequested>, mut settings: ResMut<Settings>) {
    for req in reader.read() {
        req.field.apply(&mut settings, req.delta);
    }
}

fn refresh_values(settings: Res<Settings>, mut q: Query<(&SettingValue, &mut Text)>) {
    if !settings.is_changed() {
        return;
    }
    for (v, mut text) in &mut q {
        let s = v.0.value_text(&settings);
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
            MenuAction::ResetSettings => *settings = Settings::default(),
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
