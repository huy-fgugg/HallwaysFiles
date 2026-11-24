//! HALLWAYS — first-person procedural horror-survival (Bevy).
//!
//! The crate is a library plus a thin binary so gameplay logic can be unit
//! tested with `cargo test` without opening a window.

pub mod admin;
pub mod app;
pub mod audio;
pub mod avatar;
pub mod capture;
pub mod cutscene;
pub mod data;
pub mod encounters;
pub mod ending;
pub mod floor;
pub mod graphics;
pub mod guide;
pub mod hollow;
pub mod icons;
pub mod interaction;
pub mod items;
pub mod listener;
pub mod lobby;
pub mod music;
pub mod net;
pub mod physics;
pub mod profile;
pub mod pursuit;
pub mod qa;
pub mod body;
pub mod mapfile;
pub mod voice;
pub mod achievements;
pub mod hazards;
pub mod tap;
pub mod player;
pub mod presets;
pub mod rng;
pub mod run;
pub mod settings;
pub mod state;
pub mod chase_fx;
pub mod stranger;
pub mod surge;
pub mod theater;
pub mod theme;
pub mod touch;
pub mod trailer;
pub mod ui;
pub mod viewmodel;
pub mod weather;
pub mod world;

use bevy::prelude::*;

/// Everything game-specific. Add after `DefaultPlugins`.
pub struct HallwaysPlugin;

impl Plugin for HallwaysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<rng::RunSeed>().add_plugins(data::DataPlugin).add_plugins((
            settings::SettingsPlugin,
            state::StatePlugin,
            theme::ThemePlugin,
            audio::GameAudioPlugin,
            run::RunPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            interaction::InteractionPlugin,
            ui::UiPlugin,
            capture::CapturePlugin,
        ))
        .add_plugins((music::MusicPlugin, cutscene::CutscenePlugin, hollow::HollowPlugin, floor::FloorPlugin, surge::SurgePlugin, items::ItemsPlugin, encounters::EncountersPlugin, icons::IconsPlugin, lobby::LobbyPlugin, viewmodel::ViewModelPlugin));
    app.add_plugins((stranger::StrangerPlugin, chase_fx::ChaseFxPlugin));
        app.init_resource::<presets::ActivePreset>();
        app.add_plugins((profile::ProfilePlugin, net::NetPlugin, avatar::AvatarPlugin, admin::AdminPlugin, graphics::GraphicsPlugin, world::sky::SkyPlugin, listener::ListenerPlugin, weather::WeatherPlugin, touch::TouchPlugin, ending::EndingPlugin, theater::TheaterPlugin, trailer::TrailerPlugin));
        app.add_plugins((pursuit::PursuitPlugin, guide::GuidePlugin, qa::QaPlugin, tap::TapPlugin, hazards::HazardPlugin, achievements::AchievementsPlugin, body::BodyPlugin, voice::VoicePlugin));
    }
}
