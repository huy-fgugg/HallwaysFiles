//! Night sky for the outdoor area: a moon and a star field that follow the
//! player so they always sit "at infinity". Unlit and fog-free; indoors the
//! ceilings hide them.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::player::Player;
use crate::rng::GameRng;
use crate::state::AppState;

#[derive(Component)]
struct Sky;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_sky).add_systems(PostUpdate, follow_player.before(bevy::transform::TransformSystems::Propagate).run_if(in_state(AppState::Playing)));
    }
}

fn spawn_sky(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let unlit = |c: Color, mats: &mut Assets<StandardMaterial>| mats.add(StandardMaterial { base_color: c, unlit: true, fog_enabled: false, ..default() });
    let moon_mat = unlit(Color::srgb(0.92, 0.94, 1.0), &mut mats);
    let halo_mat = mats.add(StandardMaterial { base_color: Color::srgba(0.35, 0.4, 0.55, 0.035), unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Add, ..default() });
    let star_mats = [unlit(Color::srgb(0.9, 0.92, 1.0), &mut mats), unlit(Color::srgb(0.7, 0.75, 0.9), &mut mats), unlit(Color::srgb(1.0, 0.92, 0.8), &mut mats)];
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(24, 14));
    let star = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
    commands
        .spawn((Sky, Transform::default(), Visibility::default(), DespawnOnExit(AppState::Playing), Name::new("Night sky")))
        .with_children(|s| {
            let dir = Vec3::new(0.35, 0.55, -0.76).normalize();
            s.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(moon_mat), Transform::from_translation(dir * 140.0).with_scale(Vec3::splat(6.0)), NotShadowCaster));
            s.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(halo_mat), Transform::from_translation(dir * 141.0).with_scale(Vec3::splat(9.0)), NotShadowCaster));
            let mut rng = GameRng::new(0x5EED_57A2);
            for i in 0..220 {
                // Upper hemisphere, denser near the horizon band.
                let az = rng.range_f32(0.0, std::f32::consts::TAU);
                let el = rng.range_f32(0.12, 1.0).powf(1.6) * 1.45;
                let d = Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin());
                let size = rng.range_f32(0.18, 0.5);
                s.spawn((Mesh3d(star.clone()), MeshMaterial3d(star_mats[i % 3].clone()), Transform::from_translation(d * 150.0).with_scale(Vec3::splat(size)), NotShadowCaster));
            }
        });
}

fn follow_player(player: Query<&Transform, (With<Player>, Without<Sky>)>, mut sky: Query<&mut Transform, With<Sky>>) {
    let Ok(p) = player.single() else { return };
    for mut t in &mut sky {
        t.translation = p.translation;
    }
}
