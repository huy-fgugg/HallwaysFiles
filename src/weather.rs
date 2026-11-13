//! Outdoor weather: rain streaks around the player, a looping rain sound
//! that fades in outside and out indoors, and distant lightning with thunder
//! (the flash is skipped with Reduce Flashing on).

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::audio::{encode_wav, PlaySfx, Sfx, SAMPLE_RATE};
use crate::floor::Floor;
use crate::player::Player;
use crate::rng::GameRng;
use crate::run::RunState;
use crate::settings::Settings;
use crate::state::AppState;

const DROPS: usize = 280;
const RADIUS: f32 = 9.0;
const TOP: f32 = 11.0;

#[derive(Component)]
struct Drop {
    speed: f32,
}

#[derive(Component)]
struct RainLoop;

#[derive(Component)]
struct Lightning;

#[derive(Resource)]
struct Weather {
    outside: f32,
    rng: GameRng,
    next_strike: f32,
    flash: f32,
    thunder_in: Option<f32>,
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), setup).add_systems(Update, (rain, sky_events).run_if(in_state(AppState::Playing)));
    }
}

/// Seamless 6 s loop of soft rain: filtered noise with random drips.
fn synth_rain() -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let len = (6.0 * sr) as usize;
    let mut rng = GameRng::new(0xA1_7A1);
    let mut lp = 0.0f32;
    let mut out: Vec<f32> = (0..len)
        .map(|_| {
            let n = rng.next_f32() * 2.0 - 1.0;
            lp += (n - lp) * 0.35;
            lp * 0.5
        })
        .collect();
    for _ in 0..240 {
        let at = (rng.next_f32() * (len - 400) as f32) as usize;
        let amp = rng.range_f32(0.05, 0.2);
        for k in 0..300 {
            out[at + k] += (k as f32 * 0.9).sin() * amp * (-(k as f32) / 60.0).exp();
        }
    }
    // Crossfade the ends so the loop has no click.
    let fade = 2000;
    for i in 0..fade {
        let a = i as f32 / fade as f32;
        let tail = out[len - fade + i];
        out[i] = out[i] * a + tail * (1.0 - a);
    }
    out.truncate(len - fade);
    out
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut audio: ResMut<Assets<AudioSource>>) {
    let mesh = meshes.add(Cuboid::new(0.012, 0.5, 0.012));
    let mat = mats.add(StandardMaterial { base_color: Color::srgba(0.7, 0.78, 0.9, 0.35), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
    let mut rng = GameRng::new(0x5_7A1A);
    for _ in 0..DROPS {
        let p = Vec3::new(rng.range_f32(-RADIUS, RADIUS), rng.range_f32(0.0, TOP), rng.range_f32(-RADIUS, RADIUS));
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(p), Visibility::Hidden, Drop { speed: rng.range_f32(12.0, 16.0) }, DespawnOnExit(AppState::Playing)));
    }
    let src = audio.add(AudioSource { bytes: encode_wav(&synth_rain(), SAMPLE_RATE).into() });
    commands.spawn((AudioPlayer(src), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)), RainLoop, DespawnOnExit(AppState::Playing)));
    commands.spawn((
        PointLight { color: Color::srgb(0.75, 0.85, 1.0), intensity: 0.0, range: 160.0, radius: 5.0, shadow_maps_enabled: false, ..default() },
        Transform::default(),
        Lightning,
        DespawnOnExit(AppState::Playing),
    ));
    commands.insert_resource(Weather { outside: 0.0, rng: GameRng::new(0x7_4D3), next_strike: 14.0, flash: 0.0, thunder_in: None });
}

/// True when the player stands in an outdoor room.
fn outdoors(run: &RunState, floor: Option<&Floor>) -> bool {
    floor.and_then(|f| f.0.room(run.current_room)).is_some_and(|r| r.area.outdoors())
}

#[allow(clippy::too_many_arguments)]
fn rain(
    time: Res<Time>,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    settings: Res<Settings>,
    mut weather: ResMut<Weather>,
    player: Query<&Transform, (With<Player>, Without<Drop>)>,
    mut drops: Query<(&Drop, &mut Transform, &mut Visibility)>,
    mut sinks: Query<&mut AudioSink, With<RainLoop>>,
) {
    let dt = time.delta_secs();
    let out = outdoors(&run, floor.as_deref());
    weather.outside += ((if out { 1.0 } else { 0.0 }) - weather.outside) * (dt * 1.5).min(1.0);
    for mut s in &mut sinks {
        s.set_volume(Volume::Linear(weather.outside * 0.55 * settings.sfx_gain()));
    }
    let Ok(p) = player.single() else { return };
    let center = p.translation;
    let vis = if out { Visibility::Inherited } else { Visibility::Hidden };
    for (d, mut tf, mut v) in &mut drops {
        if *v != vis {
            *v = vis;
        }
        if !out {
            continue;
        }
        tf.translation.y -= d.speed * dt;
        tf.translation.x += 1.2 * dt; // a little wind
        let off = tf.translation - center;
        if tf.translation.y < 0.0 || off.x.abs() > RADIUS || off.z.abs() > RADIUS {
            let k = tf.translation.x * 12.9898 + tf.translation.z * 78.233;
            let fx = (k.sin() * 43758.547).fract();
            let fz = ((k * 1.7).cos() * 24634.633).fract();
            tf.translation = Vec3::new(center.x + fx * 2.0 * RADIUS - RADIUS, center.y + TOP * (0.7 + 0.3 * fx.abs()), center.z + fz * 2.0 * RADIUS - RADIUS);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sky_events(
    time: Res<Time>,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    settings: Res<Settings>,
    mut weather: ResMut<Weather>,
    player: Query<&Transform, (With<Player>, Without<Lightning>)>,
    mut bolts: Query<(&mut PointLight, &mut Transform), With<Lightning>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let dt = time.delta_secs();
    if let Some(t) = weather.thunder_in.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            weather.thunder_in = None;
            sfx.write(PlaySfx(Sfx::Rumble));
        }
    }
    weather.flash = (weather.flash - dt).max(0.0);
    if outdoors(&run, floor.as_deref()) {
        weather.next_strike -= dt;
        if weather.next_strike <= 0.0 {
            weather.next_strike = weather.rng.range_f32(14.0, 32.0);
            weather.thunder_in = Some(weather.rng.range_f32(0.6, 2.2));
            if !settings.reduce_flashing {
                weather.flash = 0.35;
            }
        }
    }
    let Ok(p) = player.single() else { return };
    let f = weather.flash;
    // Double flicker: two bright pulses inside the 0.35 s flash.
    let pulse = if f > 0.25 || (0.08..0.15).contains(&f) { 1.0 } else { 0.0 };
    for (mut l, mut tf) in &mut bolts {
        tf.translation = p.translation + Vec3::new(-20.0, 45.0, -30.0);
        l.intensity = pulse * 60_000_000.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_loop_is_quiet_and_clean() {
        let s = synth_rain();
        assert!(s.len() > SAMPLE_RATE as usize * 5);
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 1.0, "rain clips: {peak}");
    }
}
