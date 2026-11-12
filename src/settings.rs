//! Player settings: persisted to a small RON file in the user's config folder.

use bevy::{
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SETTINGS_VERSION: u32 = 1;

/// Rendering quality. Low is for weak or integrated GPUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Graphics {
    /// No ambient occlusion, anti-aliasing, bloom or lamp shadows; small
    /// shadow maps; conservative GPU limits. Runs on most integrated GPUs.
    Low,
    /// No ambient occlusion or temporal AA (cheap FXAA instead); bloom and
    /// one shadowed lamp per room.
    #[default]
    Medium,
    /// Everything on: SSAO, temporal AA, bloom, shadows.
    High,
}

impl Graphics {
    pub fn label(self) -> &'static str {
        match self {
            Graphics::Low => "LOW",
            Graphics::Medium => "MEDIUM",
            Graphics::High => "HIGH",
        }
    }

    pub fn step(self, delta: i32) -> Self {
        let order = [Graphics::Low, Graphics::Medium, Graphics::High];
        let i = order.iter().position(|g| *g == self).unwrap_or(1) as i32;
        let n = if delta == 0 { (i + 1) % 3 } else { (i + delta).clamp(0, 2) };
        order[n as usize]
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" | "--low" => Some(Graphics::Low),
            "medium" | "--medium" => Some(Graphics::Medium),
            "high" | "--high" => Some(Graphics::High),
            _ => None,
        }
    }

    /// Start-up override so a game that crashes on launch can still be opened:
    /// `hallways --low` or `HALLWAYS_GRAPHICS=low`.
    pub fn startup_override() -> Option<Self> {
        std::env::args().skip(1).find_map(|a| Graphics::parse(&a)).or_else(|| std::env::var("HALLWAYS_GRAPHICS").ok().and_then(|v| Graphics::parse(&v)))
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// Multiplier on the base mouse/stick look speed. 0.1 ..= 3.0
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    /// 0 ..= 100
    pub master_volume: u32,
    pub sfx_volume: u32,
    pub music_volume: u32,
    /// Scales cue volume, screen distortion, shake and jump-scare frequency. 0 ..= 100
    pub spooky_intensity: u32,
    /// Subtitles for entity cues and dialogue. On by default.
    pub subtitles: bool,
    /// Photosensitivity: replaces flicker/flash effects with slow fades.
    pub reduce_flashing: bool,
    /// Caps the loudest stingers and failure sounds.
    pub limit_loud_sounds: bool,
    pub fullscreen: bool,
    pub graphics: Graphics,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            mouse_sensitivity: 1.0,
            invert_y: false,
            master_volume: 80,
            sfx_volume: 80,
            music_volume: 60,
            spooky_intensity: 70,
            subtitles: true,
            reduce_flashing: false,
            limit_loud_sounds: false,
            fullscreen: false,
            graphics: Graphics::default(),
        }
    }
}

impl Settings {
    /// Clamp every field into its valid range (protects against hand-edited files).
    pub fn sanitized(mut self) -> Self {
        self.version = SETTINGS_VERSION;
        if !self.mouse_sensitivity.is_finite() {
            self.mouse_sensitivity = 1.0;
        }
        self.mouse_sensitivity = (self.mouse_sensitivity * 10.0).round() / 10.0;
        self.mouse_sensitivity = self.mouse_sensitivity.clamp(0.1, 3.0);
        self.master_volume = self.master_volume.min(100);
        self.sfx_volume = self.sfx_volume.min(100);
        self.music_volume = self.music_volume.min(100);
        self.spooky_intensity = self.spooky_intensity.min(100);
        self
    }

    pub fn sfx_gain(&self) -> f32 {
        (self.master_volume as f32 / 100.0) * (self.sfx_volume as f32 / 100.0)
    }

    pub fn music_gain(&self) -> f32 {
        (self.master_volume as f32 / 100.0) * (self.music_volume as f32 / 100.0)
    }

    /// 0..1 scale applied to scare effects (shake, distortion, cue loudness).
    pub fn spook(&self) -> f32 {
        self.spooky_intensity as f32 / 100.0
    }

    /// Gain for loud stingers, honouring the loud-sound limiter.
    pub fn stinger_gain(&self) -> f32 {
        let base = self.sfx_gain() * (0.5 + 0.5 * self.spook());
        if self.limit_loud_sounds { base.min(0.35) } else { base }
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    /// Parses settings, falling back to defaults on any error (corrupt file recovery).
    pub fn from_ron(text: &str) -> Self {
        match ron::from_str::<Settings>(text) {
            Ok(s) => s.sanitized(),
            Err(err) => {
                warn!("settings file unreadable ({err}); using defaults");
                Settings::default()
            }
        }
    }
}

/// Folder for settings/saves: `%APPDATA%\Hallways` on Windows,
/// `$XDG_CONFIG_HOME/hallways` or `~/.config/hallways` elsewhere, else the CWD.
pub fn config_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Hallways");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("hallways");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("hallways");
    }
    PathBuf::from(".")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.ron")
}

pub fn load_settings() -> Settings {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => Settings::from_ron(&text),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write to a temp file then rename, so a crash mid-write can't corrupt settings.
    let tmp = path.with_extension("ron.tmp");
    if std::fs::write(&tmp, settings.to_ron()).is_ok() {
        if let Err(err) = std::fs::rename(&tmp, &path) {
            warn!("could not save settings: {err}");
        }
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_settings())
            .add_systems(Update, apply_window_mode.run_if(resource_changed::<Settings>));
    }
}

fn apply_window_mode(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let wanted = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != wanted {
        window.mode = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_subtitles_on() {
        assert!(Settings::default().subtitles);
    }

    #[test]
    fn roundtrip_ron() {
        let s = Settings {
            mouse_sensitivity: 1.7,
            spooky_intensity: 20,
            fullscreen: true,
            ..default()
        };
        assert_eq!(Settings::from_ron(&s.to_ron()), s);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        assert_eq!(Settings::from_ron("{{ not ron"), Settings::default());
    }

    #[test]
    fn missing_fields_use_defaults_and_values_are_clamped() {
        let s = Settings::from_ron("(master_volume: 250, mouse_sensitivity: 9.0)");
        assert_eq!(s.master_volume, 100);
        assert_eq!(s.mouse_sensitivity, 3.0);
        assert!(s.subtitles);
    }

    #[test]
    fn loud_sound_limiter_caps_stingers() {
        let s = Settings { master_volume: 100, sfx_volume: 100, spooky_intensity: 100, limit_loud_sounds: true, ..default() };
        assert!(s.stinger_gain() <= 0.35);
    }
}
