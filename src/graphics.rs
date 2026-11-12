//! Graphics quality (Settings → Graphics quality, or `--low` at start-up).
//! Cameras tagged [`QualityCamera`] get bloom / SSAO / anti-aliasing to match
//! the setting, and lamp shadows are switched off on Low.

use bevy::{
    anti_alias::{fxaa::Fxaa, taa::TemporalAntiAliasing},
    light::{DirectionalLightShadowMap, PointLightShadowMap},
    pbr::ScreenSpaceAmbientOcclusion,
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::settings::{Graphics, Settings};

/// A world camera whose post effects follow the quality setting.
#[derive(Component, Debug, Clone, Copy)]
pub struct QualityCamera {
    pub bloom: f32,
}

/// Remembers that a light had shadows before Low switched them off.
#[derive(Component)]
struct HadShadows;

/// The quality actually in use (the start-up override wins over settings).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveGraphics(pub Graphics);

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, track_setting).add_systems(PostUpdate, (apply_cameras, apply_lights));
    }

    fn finish(&self, app: &mut App) {
        let settings = app.world().get_resource::<Settings>().map(|s| s.graphics).unwrap_or_default();
        let active = Graphics::startup_override().unwrap_or(settings);
        app.insert_resource(ActiveGraphics(active));
        let (point, dir) = match active {
            Graphics::Low => (256, 512),
            Graphics::Medium => (512, 1024),
            Graphics::High => (1024, 2048),
        };
        app.insert_resource(PointLightShadowMap { size: point });
        app.insert_resource(DirectionalLightShadowMap { size: dir });
        info!("graphics quality: {active:?}");
    }
}

/// Changing the setting in the menu takes effect immediately.
fn track_setting(settings: Res<Settings>, mut active: ResMut<ActiveGraphics>, mut point: ResMut<PointLightShadowMap>) {
    if settings.is_changed() && !settings.is_added() && active.0 != settings.graphics {
        active.0 = settings.graphics;
        point.size = match active.0 {
            Graphics::Low => 256,
            Graphics::Medium => 512,
            Graphics::High => 1024,
        };
    }
}

fn apply_cameras(mut commands: Commands, active: Res<ActiveGraphics>, cams: Query<(Entity, &QualityCamera, Ref<QualityCamera>)>) {
    for (e, q, r) in &cams {
        if !(active.is_changed() || r.is_added()) {
            continue;
        }
        let mut c = commands.entity(e);
        match active.0 {
            Graphics::High => {
                c.remove::<Fxaa>().insert((Bloom { intensity: q.bloom, ..Bloom::NATURAL }, ScreenSpaceAmbientOcclusion::default(), TemporalAntiAliasing::default()));
            }
            Graphics::Medium => {
                c.remove::<(ScreenSpaceAmbientOcclusion, TemporalAntiAliasing)>().insert((Bloom { intensity: q.bloom, ..Bloom::NATURAL }, Fxaa::default()));
            }
            Graphics::Low => {
                c.remove::<(ScreenSpaceAmbientOcclusion, TemporalAntiAliasing, Bloom, Fxaa)>();
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn apply_lights(
    mut commands: Commands,
    active: Res<ActiveGraphics>,
    mut points: Query<(Entity, &mut PointLight, Has<HadShadows>)>,
    mut spots: Query<(Entity, &mut SpotLight, Has<HadShadows>), Without<PointLight>>,
) {
    let low = active.0 == Graphics::Low;
    for (e, mut l, had) in &mut points {
        if !(active.is_changed() || l.is_added()) {
            continue;
        }
        if low && l.shadow_maps_enabled {
            l.shadow_maps_enabled = false;
            commands.entity(e).insert(HadShadows);
        } else if !low && had {
            l.shadow_maps_enabled = true;
            commands.entity(e).remove::<HadShadows>();
        }
    }
    if active.is_changed() {
        for (e, mut l, had) in &mut spots {
            if low && l.shadow_maps_enabled {
                l.shadow_maps_enabled = false;
                commands.entity(e).insert(HadShadows);
            } else if !low && had {
                l.shadow_maps_enabled = true;
                commands.entity(e).remove::<HadShadows>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_steps() {
        assert_eq!(Graphics::Medium.step(1), Graphics::High);
        assert_eq!(Graphics::High.step(1), Graphics::High);
        assert_eq!(Graphics::Low.step(-1), Graphics::Low);
        assert_eq!(Graphics::High.step(0), Graphics::Low, "Enter cycles");
        assert_eq!(Graphics::default(), Graphics::Medium);
    }

    #[test]
    fn old_settings_files_get_medium() {
        let s = Settings::from_ron("(version: 1, master_volume: 50)");
        assert_eq!(s.graphics, Graphics::Medium);
        assert_eq!(s.master_volume, 50);
    }
}
