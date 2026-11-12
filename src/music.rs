//! Music director + original procedural score.
//!
//! Every track has a drop-in slot. If a file exists in the slot it is used,
//! otherwise the built-in procedural score plays:
//!
//! | Track            | Slot (any of .ogg / .mp3 / .wav)        | Plays          |
//! |------------------|-----------------------------------------|----------------|
//! | Menu             | `assets/music/menu`                     | loop           |
//! | Ambience         | `assets/music/ambience`                 | loop           |
//! | Chase            | `assets/music/chase`                    | loop           |
//! | Cutscene `<id>`  | `assets/cutscenes/<id>/music`           | once, synced   |
//!
//! See `assets/music/README.md` for length limits.

use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::{encode_wav, synth_ambience, SAMPLE_RATE};
use crate::rng::GameRng;
use crate::settings::Settings;
use crate::state::{AppState, Overlay};

/// Loop tracks: shortest/longest allowed length in seconds (documented for users).
pub const LOOP_MIN_SECS: f32 = 20.0;
pub const LOOP_MAX_SECS: f32 = 300.0;
/// Cutscene tracks may run at most this long; anything past the cutscene end is faded out.
pub const CUTSCENE_MUSIC_MAX_SECS: f32 = 60.0;
/// Fade used when switching tracks and when a cutscene ends.
pub const FADE_SECS: f32 = 1.5;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MusicTrack {
    Menu,
    Ambience,
    Chase,
    Cutscene(String),
}

impl MusicTrack {
    fn looping(&self) -> bool {
        !matches!(self, MusicTrack::Cutscene(_))
    }

    /// Asset path (without extension) of the drop-in slot.
    pub fn slot(&self) -> String {
        match self {
            MusicTrack::Menu => "music/menu".into(),
            MusicTrack::Ambience => "music/ambience".into(),
            MusicTrack::Chase => "music/chase".into(),
            MusicTrack::Cutscene(id) => format!("cutscenes/{id}/music"),
        }
    }

    /// Mixing level relative to the music volume setting.
    fn level(&self) -> f32 {
        match self {
            MusicTrack::Ambience => 0.6,
            _ => 0.9,
        }
    }
}

/// What should be playing right now. Gameplay systems write this.
#[derive(Resource, Debug, Default)]
pub struct MusicDirector {
    pub wanted: Option<MusicTrack>,
}

#[derive(Component)]
struct MusicPlayer {
    track: MusicTrack,
    /// 0..1 fade level; target 1 while wanted, 0 when fading out.
    fade: f32,
    leaving: bool,
}

#[derive(Resource, Default)]
struct ScoreCache(HashMap<MusicTrack, Handle<AudioSource>>);

/// Where Bevy looks for `assets/`: `BEVY_ASSET_ROOT`, else the Cargo project
/// (when launched via `cargo run`), else the folder containing the executable.
pub fn asset_root() -> PathBuf {
    if let Some(p) = std::env::var_os("BEVY_ASSET_ROOT") {
        return PathBuf::from(p).join("assets");
    }
    if let Some(p) = std::env::var_os("CARGO_MANIFEST_DIR") {
        return PathBuf::from(p).join("assets");
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("assets")))
        .unwrap_or_else(|| PathBuf::from("assets"))
}

/// Finds a user-supplied file for a slot, trying supported extensions.
pub fn find_override(slot: &str) -> Option<String> {
    let root = asset_root();
    for ext in ["ogg", "mp3", "wav"] {
        let rel = format!("{slot}.{ext}");
        if root.join(&rel).is_file() {
            return Some(rel);
        }
    }
    None
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicDirector>()
            .init_resource::<ScoreCache>()
            .add_systems(Update, (default_wants, apply_director, fade_players).chain());
    }
}

/// Baseline choice; cutscene/chase systems override it later in the frame.
fn default_wants(app: Res<State<AppState>>, mut director: ResMut<MusicDirector>, chase: Option<Res<crate::hollow::ChaseState>>, cut: Option<Res<crate::cutscene::ActiveCutscene>>) {
    let wanted = match app.get() {
        AppState::MainMenu => Some(MusicTrack::Menu),
        AppState::Playing => {
            if let Some(c) = cut.as_ref().filter(|c| c.def.is_some()) {
                Some(MusicTrack::Cutscene(c.id.clone()))
            } else if chase.is_some_and(|c| c.active) {
                Some(MusicTrack::Chase)
            } else {
                Some(MusicTrack::Ambience)
            }
        }
        _ => None,
    };
    if director.wanted != wanted {
        director.wanted = wanted;
    }
}

fn source_for(track: &MusicTrack, asset_server: &AssetServer, cache: &mut ScoreCache, sources: &mut Assets<AudioSource>) -> Handle<AudioSource> {
    if let Some(path) = find_override(&track.slot()) {
        info!("music: using drop-in file {path}");
        return asset_server.load(path);
    }
    cache
        .0
        .entry(track.clone())
        .or_insert_with(|| {
            let samples = match track {
                MusicTrack::Menu => synth_menu_theme(),
                MusicTrack::Ambience => synth_ambience(),
                MusicTrack::Chase => synth_chase_theme(),
                MusicTrack::Cutscene(id) if id.contains("hollow") => synth_hollow_cue(),
                MusicTrack::Cutscene(_) => synth_intro_cue(),
            };
            sources.add(AudioSource { bytes: Arc::from(encode_wav(&samples, SAMPLE_RATE)) })
        })
        .clone()
}

#[allow(clippy::too_many_arguments)]
fn apply_director(
    mut commands: Commands,
    director: Res<MusicDirector>,
    settings: Res<Settings>,
    asset_server: Res<AssetServer>,
    mut cache: ResMut<ScoreCache>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut players: Query<&mut MusicPlayer>,
) {
    let mut have_wanted = false;
    for mut p in &mut players {
        if Some(&p.track) == director.wanted.as_ref() && !p.leaving {
            have_wanted = true;
        } else {
            p.leaving = true;
        }
    }
    if let (Some(track), false) = (director.wanted.clone(), have_wanted) {
        let handle = source_for(&track, &asset_server, &mut cache, &mut sources);
        let mode = if track.looping() { PlaybackSettings::LOOP } else { PlaybackSettings::DESPAWN };
        // Cutscene cues start at full volume so they stay in sync with the picture.
        let fade = if track.looping() { 0.0 } else { 1.0 };
        commands.spawn((
            AudioPlayer(handle),
            mode.with_volume(Volume::Linear(settings.music_gain() * track.level() * fade)),
            MusicPlayer { track, fade, leaving: false },
            Name::new("Music"),
        ));
    }
}

fn fade_players(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    overlay: Res<State<Overlay>>,
    mut players: Query<(Entity, &mut MusicPlayer, Option<&mut AudioSink>)>,
) {
    let duck = if *overlay.get() == Overlay::Pause { 0.4 } else { 1.0 };
    let step = time.delta_secs() / FADE_SECS;
    for (e, mut p, sink) in &mut players {
        p.fade = if p.leaving { (p.fade - step).max(0.0) } else { (p.fade + step).min(1.0) };
        if p.leaving && p.fade <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(settings.music_gain() * p.track.level() * p.fade * duck));
        }
    }
}

// ---------------------------------------------------------------------------
// Original procedural score (composed for HALLWAYS)
// ---------------------------------------------------------------------------

const SR: f32 = SAMPLE_RATE as f32;

fn midi(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Simple Schroeder-style reverb: parallel combs + allpass. `mix` 0..1.
fn reverb(dry: &[f32], mix: f32, seconds_tail: f32) -> Vec<f32> {
    let combs = [1557usize, 1617, 1491, 1422].map(|d| d * SAMPLE_RATE as usize / 44_100);
    let fb = 0.001f32.powf(1.0 / (seconds_tail * SR / combs[0] as f32));
    let mut wet = vec![0.0; dry.len()];
    for &d in &combs {
        let mut buf = vec![0.0f32; d];
        let mut lp = 0.0f32;
        for i in 0..dry.len() {
            let idx = i % d;
            let out = buf[idx];
            lp = out * 0.6 + lp * 0.4;
            buf[idx] = dry[i] + lp * fb;
            wet[i] += out * 0.25;
        }
    }
    for &(d, g) in &[(225usize, 0.5f32), (556, 0.5)] {
        let mut buf = vec![0.0f32; d];
        for (i, s) in wet.iter_mut().enumerate() {
            let idx = i % d;
            let b = buf[idx];
            let y = -g * *s + b;
            buf[idx] = *s + g * y;
            *s = y;
        }
    }
    dry.iter().zip(&wet).map(|(d, w)| d * (1.0 - mix) + w * mix).collect()
}

/// Makes a buffer loop seamlessly by folding its last `tail` seconds onto the start.
fn fold_loop(mut v: Vec<f32>, loop_len: usize) -> Vec<f32> {
    if v.len() > loop_len {
        let extra: Vec<f32> = v.split_off(loop_len);
        for (i, s) in extra.into_iter().enumerate() {
            v[i % loop_len] += s;
        }
    }
    v
}

fn normalize(mut v: Vec<f32>, peak: f32) -> Vec<f32> {
    let max = v.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    for s in &mut v {
        *s *= peak / max;
    }
    v
}

fn add_note(buf: &mut [f32], start: f32, dur: f32, f: impl Fn(f32) -> f32) {
    let s0 = (start * SR) as usize;
    let n = (dur * SR) as usize;
    for k in 0..n {
        if let Some(b) = buf.get_mut(s0 + k) {
            *b += f(k as f32 / SR);
        }
    }
}

/// Soft detuned pad with slow attack/release.
fn pad(t: f32, freq: f32, dur: f32) -> f32 {
    let a = (t / 1.5).min(1.0);
    let r = ((dur - t) / 1.5).clamp(0.0, 1.0);
    let mut s = 0.0;
    for (i, det) in [-0.12f32, 0.0, 0.11].iter().enumerate() {
        let f = freq * 2f32.powf(det / 12.0);
        // Two harmonics only: dark and warm.
        s += (TAU * f * t + i as f32).sin() * 0.6 + (TAU * f * 2.0 * t).sin() * 0.15;
    }
    s * a * r * 0.12
}

/// Music-box style pluck with inharmonic shimmer.
fn pluck(t: f32, freq: f32) -> f32 {
    let env = (-t * 3.2).exp() * (t / 0.004).min(1.0);
    ((TAU * freq * t).sin() + (TAU * freq * 2.76 * t).sin() * 0.25 + (TAU * freq * 5.4 * t).sin() * 0.08) * env * 0.22
}

/// Main menu: slow, lonely music-box over a dark pad (A minor colour), 32 s loop.
pub fn synth_menu_theme() -> Vec<f32> {
    let loop_secs = 32.0;
    let len = (loop_secs * SR) as usize + (6.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    // Chord roots (MIDI) every 8 s: Am(add9) - Fmaj7 - Dm6 - E(sus4->E).
    let chords: [&[f32]; 4] = [&[45.0, 52.0, 59.0, 60.0], &[41.0, 48.0, 52.0, 57.0], &[38.0, 45.0, 53.0, 59.0], &[40.0, 47.0, 57.0, 56.0]];
    for (ci, chord) in chords.iter().enumerate() {
        for &n in chord.iter() {
            let f = midi(n);
            add_note(&mut buf, ci as f32 * 8.0, 9.5, |t| pad(t, f, 9.5));
        }
    }
    // Sparse melody (beats at 60 BPM).
    let melody: [(f32, f32); 14] = [
        (0.0, 76.0), (1.5, 72.0), (3.0, 71.0), (4.0, 69.0),
        (8.0, 72.0), (9.5, 69.0), (11.0, 67.0),
        (16.0, 74.0), (17.5, 72.0), (19.0, 69.0), (20.0, 65.0),
        (24.0, 71.0), (26.0, 68.0), (28.5, 64.0),
    ];
    for (start, n) in melody {
        let f = midi(n);
        add_note(&mut buf, start, 3.0, |t| pluck(t, f));
        // Faint echo an octave up, delayed.
        add_note(&mut buf, start + 0.75, 2.0, |t| pluck(t, f * 2.0) * 0.25);
    }
    // Low heartbeat-like thuds every 4 s.
    for k in 0..8 {
        add_note(&mut buf, k as f32 * 4.0 + 2.0, 0.5, |t| (TAU * 46.0 * t).sin() * (-t * 9.0).exp() * 0.3);
    }
    let wet = reverb(&buf, 0.45, 3.5);
    normalize(fold_loop(wet, (loop_secs * SR) as usize), 0.8)
}

/// Chase: 120 BPM, 8 bars (16 s) loop: pounding pulse, chromatic bass, shrieking cluster.
pub fn synth_chase_theme() -> Vec<f32> {
    let loop_secs = 16.0;
    let len = (loop_secs * SR) as usize + (2.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    let beat = 0.5;
    let mut rng = GameRng::new(1234);
    for b in 0..32 {
        let t0 = b as f32 * beat;
        // Kick on every beat, extra on the "and" of 4.
        add_note(&mut buf, t0, 0.35, |t| (TAU * (55.0 + 90.0 * (-t * 30.0).exp()) * t).sin() * (-t * 8.0).exp() * 0.8);
        if b % 4 == 3 {
            add_note(&mut buf, t0 + 0.25, 0.3, |t| (TAU * (55.0 + 80.0 * (-t * 30.0).exp()) * t).sin() * (-t * 9.0).exp() * 0.6);
        }
        // Metallic hit on 2 and 4.
        if b % 2 == 1 {
            let seed = rng.next_u64();
            add_note(&mut buf, t0, 0.25, move |t| {
                let mut r = GameRng::new(seed ^ (t * SR) as u64);
                (r.next_f32() * 2.0 - 1.0) * (-t * 18.0).exp() * 0.25 + (TAU * 1730.0 * t).sin() * (-t * 25.0).exp() * 0.08
            });
        }
    }
    // Bass ostinato in 16ths: A A Bb A | A A C B (MIDI 33 = A1).
    let pattern = [33.0, 33.0, 34.0, 33.0, 33.0, 33.0, 36.0, 35.0];
    for i in 0..128 {
        let n = pattern[i % 8] + if (i / 32) % 2 == 1 { 3.0 } else { 0.0 };
        let f = midi(n);
        add_note(&mut buf, i as f32 * beat / 4.0, 0.12, move |t| {
            let saw = 2.0 * ((f * t) % 1.0) - 1.0;
            (saw * 0.5 + (TAU * f * t).sin() * 0.5) * (-t * 14.0).exp() * 0.35
        });
    }
    // High tremolo cluster (minor 2nds), swelling every 2 bars.
    for bar in 0..4 {
        let start = bar as f32 * 4.0;
        for &n in &[81.0f32, 82.0, 88.0] {
            let f = midi(n);
            add_note(&mut buf, start, 4.0, move |t| (TAU * f * t).sin() * (0.5 + 0.5 * (TAU * 12.0 * t).sin()) * (t / 4.0).powi(2) * 0.06);
        }
    }
    let wet = reverb(&buf, 0.2, 1.2);
    normalize(fold_loop(wet, (loop_secs * SR) as usize), 0.85)
}

/// Arrival lift (14 s), synced to `assets/cutscenes/01_intro/cutscene.ron`:
/// low descent drone with mechanical throb 0-7 s, the bell lands at 7.0 s,
/// a cold open chord while the gate opens, a sparse music-box phrase for the title.
pub fn synth_intro_cue() -> Vec<f32> {
    let len = (15.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    add_note(&mut buf, 0.0, 7.2, |t| {
        let swell = (t / 2.0).min(1.0) * (1.0 - ((t - 6.6) / 0.6).clamp(0.0, 1.0));
        let throb = 0.6 + 0.4 * (TAU * 1.6 * t).sin();
        ((TAU * midi(28.0) * t).sin() + (TAU * midi(35.0) * t).sin() * 0.4) * swell * throb * 0.3
    });
    add_note(&mut buf, 7.0, 7.5, |t| {
        let a = (t / 1.5).min(1.0) * ((7.5 - t) / 2.0).clamp(0.0, 1.0);
        [45.0f32, 52.0, 55.0, 60.0].iter().map(|n| pad(t, midi(*n), 7.5) / 0.12 * 0.08).sum::<f32>() * a
    });
    for (start, n) in [(9.6f32, 76.0f32), (10.4, 72.0), (11.2, 71.0), (12.4, 69.0)] {
        let f = midi(n);
        add_note(&mut buf, start, 2.5, |t| pluck(t, f));
    }
    normalize(reverb(&buf, 0.45, 3.5), 0.8)
}

/// The Hollow emergence (16 s), synced to `assets/cutscenes/02_the_hollow/cutscene.ron`:
/// rumble 0–3.5 s, wet low cluster grows 3.5–11 s, silence beat, HIT at 11.3 s,
/// then the chase pulse fades in 13–16 s.
pub fn synth_hollow_cue() -> Vec<f32> {
    let len = (16.5 * SR) as usize;
    let mut buf = vec![0.0; len];
    add_note(&mut buf, 0.0, 11.0, |t| {
        let swell = (t / 10.5).powf(1.5) * (1.0 - ((t - 10.7) / 0.3).clamp(0.0, 1.0));
        let cluster = [33.0f32, 34.0, 39.0, 45.0, 46.0].iter().map(|n| (TAU * midi(*n) * t).sin()).sum::<f32>();
        cluster * swell * 0.12
    });
    add_note(&mut buf, 2.5, 8.5, |t| {
        // Rising string-like glissando.
        let f = midi(64.0 + t * 1.6);
        let saw = 2.0 * ((f * t) % 1.0) - 1.0;
        saw * (t / 8.5).powi(3) * 0.06 * (1.0 - ((t - 8.2) / 0.3).clamp(0.0, 1.0))
    });
    // The hit.
    add_note(&mut buf, 11.3, 3.5, |t| {
        let env = (-t * 1.6).exp();
        let body = [32.0f32, 38.0, 44.0, 51.0, 57.0, 58.0].iter().map(|n| (TAU * midi(*n) * t).sin()).sum::<f32>() * 0.18;
        let mut r = GameRng::new((t * SR) as u64);
        (body + (r.next_f32() * 2.0 - 1.0) * 0.6 * (-t * 6.0).exp()) * env
    });
    // Chase pulse lead-in.
    for b in 0..6 {
        let t0 = 13.0 + b as f32 * 0.5;
        let g = (b as f32 + 1.0) / 6.0;
        add_note(&mut buf, t0, 0.35, move |t| (TAU * (55.0 + 90.0 * (-t * 30.0).exp()) * t).sin() * (-t * 8.0).exp() * 0.7 * g);
    }
    normalize(reverb(&buf, 0.35, 2.5), 0.9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loops_have_documented_lengths() {
        let menu = synth_menu_theme();
        assert_eq!(menu.len(), (32.0 * SR) as usize);
        let chase = synth_chase_theme();
        assert_eq!(chase.len(), (16.0 * SR) as usize);
        assert!(menu.iter().chain(&chase).all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn cues_are_finite() {
        assert!(synth_intro_cue().iter().all(|s| s.is_finite()));
        assert!(synth_hollow_cue().iter().all(|s| s.is_finite()));
    }

    #[test]
    fn slots() {
        assert_eq!(MusicTrack::Cutscene("02_the_hollow".into()).slot(), "cutscenes/02_the_hollow/music");
        assert_eq!(MusicTrack::Chase.slot(), "music/chase");
    }
}
