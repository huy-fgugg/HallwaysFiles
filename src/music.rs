//! Music director + original procedural score.
//!
//! Every track has a drop-in slot. If a file exists in the slot it is used,
//! otherwise the built-in procedural score plays:
//!
//! | Track            | Slot (any of .ogg / .mp3 / .wav)        | Plays          |
//! |------------------|-----------------------------------------|----------------|
//! | Menu             | `assets/music/menu`                     | loop           |
//! | Elevator lobby   | `assets/music/elevator`                 | loop           |
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
    /// Loops in the elevator while players gather.
    Lift,
    Ambience,
    Chase,
    /// The victory (results) screen after the ending.
    Victory,
    Cutscene(String),
}

impl MusicTrack {
    fn looping(&self) -> bool {
        !matches!(self, MusicTrack::Cutscene(_))
    }

    /// Folder under `assets/music/` where the player drops a song for this track.
    /// Any audio file inside works, whatever it is called.
    pub fn folder(&self) -> String {
        match self {
            MusicTrack::Menu => "music/menu".into(),
            MusicTrack::Lift => "music/elevator".into(),
            MusicTrack::Ambience => "music/ambience".into(),
            MusicTrack::Chase => "music/chase".into(),
            MusicTrack::Victory => "music/victory".into(),
            MusicTrack::Cutscene(id) => format!("music/cutscene_{}", cutscene_short_name(id)),
        }
    }

    /// Older single-file location (still honoured): `<slot>.ogg|mp3|wav`.
    pub fn slot(&self) -> String {
        match self {
            MusicTrack::Cutscene(id) => format!("cutscenes/{id}/music"),
            other => other.folder(),
        }
    }

    /// Friendly name for the credits/music status screen.
    pub fn label(&self) -> String {
        match self {
            MusicTrack::Menu => "Main menu".into(),
            MusicTrack::Lift => "Elevator lobby".into(),
            MusicTrack::Ambience => "Exploring".into(),
            MusicTrack::Chase => "Chase".into(),
            MusicTrack::Victory => "Victory screen".into(),
            MusicTrack::Cutscene(id) => format!("Cutscene: {}", cutscene_short_name(id)),
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

const AUDIO_EXTS: [&str; 3] = ["ogg", "mp3", "wav"];

/// "01_intro" -> "intro", "02_the_hollow" -> "hollow".
pub fn cutscene_short_name(id: &str) -> String {
    let rest = id.split_once('_').map_or(id, |(_, r)| r);
    rest.strip_prefix("the_").unwrap_or(rest).to_string()
}

fn is_audio(p: &std::path::Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// First audio file (alphabetically) inside `root/dir`, as an asset path.
fn first_audio_in(root: &std::path::Path, dir: &str) -> Option<String> {
    let mut files: Vec<_> = std::fs::read_dir(root.join(dir)).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && is_audio(p)).collect();
    files.sort();
    let name = files.first()?.file_name()?.to_str()?.to_string();
    Some(format!("{dir}/{name}"))
}

/// Finds a user-supplied file: any audio file in the track's music folder,
/// or the older fixed file name.
pub fn find_track_override(track: &MusicTrack) -> Option<String> {
    let root = asset_root();
    if let Some(p) = first_audio_in(&root, &track.folder()) {
        return Some(p);
    }
    find_override(&track.slot())
}

/// Finds a user-supplied file for a fixed slot path, trying supported extensions.
pub fn find_override(slot: &str) -> Option<String> {
    let root = asset_root();
    for ext in AUDIO_EXTS {
        let rel = format!("{slot}.{ext}");
        if root.join(&rel).is_file() {
            return Some(rel);
        }
    }
    None
}

/// Every music slot and the custom file it will use, if any (shown in Credits).
pub fn custom_music_report() -> Vec<(String, Option<String>)> {
    let mut tracks = vec![MusicTrack::Menu, MusicTrack::Lift, MusicTrack::Ambience, MusicTrack::Chase];
    tracks.push(MusicTrack::Cutscene(crate::cutscene::INTRO.into()));
    tracks.push(MusicTrack::Cutscene(crate::cutscene::THE_HOLLOW.into()));
    tracks.push(MusicTrack::Cutscene(crate::cutscene::ENDING.into()));
    tracks.push(MusicTrack::Victory);
    tracks.into_iter().map(|t| (t.label(), find_track_override(&t))).collect()
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
fn default_wants(
    app: Res<State<AppState>>,
    mut director: ResMut<MusicDirector>,
    chase: Option<Res<crate::hollow::ChaseState>>,
    pursuit: Option<Res<crate::pursuit::Pursuit>>,
    cut: Option<Res<crate::cutscene::ActiveCutscene>>,
    lobby: Option<Res<crate::lobby::LiftLobby>>,
    overlay: Option<Res<State<crate::state::Overlay>>>,
) {
    let wanted = match app.get() {
        AppState::MainMenu => Some(MusicTrack::Menu),
        AppState::Playing => {
            if overlay.is_some_and(|o| *o.get() == crate::state::Overlay::Summary) {
                Some(MusicTrack::Victory)
            } else if lobby.is_some_and(|l| l.waiting) {
                Some(MusicTrack::Lift)
            } else if let Some(c) = cut.as_ref().filter(|c| c.def.is_some()) {
                Some(MusicTrack::Cutscene(c.id.clone()))
            } else if chase.is_some_and(|c| c.active) || pursuit.is_some_and(|p| p.active()) {
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
    if let Some(path) = find_track_override(track) {
        info!("music: using drop-in file {path}");
        return asset_server.load(path);
    }
    cache
        .0
        .entry(track.clone())
        .or_insert_with(|| {
            let samples = match track {
                MusicTrack::Menu => synth_menu_theme(),
                MusicTrack::Lift => synth_lift_theme(),
                MusicTrack::Ambience => synth_ambience(),
                MusicTrack::Chase => synth_chase_theme(),
                MusicTrack::Victory => synth_victory_theme(),
                MusicTrack::Cutscene(id) if id.contains("ending") => synth_ending_cue(),
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

/// Elevator music (original): 24 s loop of mellow electric-piano seventh
/// chords and a walking bass at 80 BPM, with a slow tape wobble that makes it
/// slightly wrong. Calm on the surface, uneasy underneath.
pub fn synth_lift_theme() -> Vec<f32> {
    let loop_secs = 24.0;
    let len = (loop_secs * SR) as usize + (3.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    let beat = 60.0 / 80.0; // 0.75 s; 32 beats = 24 s
    // Tape wobble: a slow pitch drift applied to every note.
    let wobble = |t: f32, abs: f32| 1.0 + 0.004 * (TAU * 0.23 * (abs + t)).sin();
    let epiano = move |t: f32, f: f32, abs: f32| {
        let w = wobble(t, abs);
        let env = (-t * 1.4).exp() * (t / 0.006).min(1.0);
        let trem = 0.85 + 0.15 * (TAU * 4.5 * t).sin();
        ((TAU * f * w * t).sin() + (TAU * f * w * 2.0 * t).sin() * 0.25 * (-t * 4.0).exp() + (TAU * f * w * 7.1 * t).sin() * 0.04 * (-t * 9.0).exp()) * env * trem * 0.11
    };
    // Dmaj7 - Bm7 - Em9 - A13(sus), two bars each (8 beats).
    let chords: [&[f32]; 4] = [&[50.0, 57.0, 61.0, 66.0], &[47.0, 54.0, 57.0, 62.0], &[52.0, 59.0, 62.0, 66.0], &[45.0, 55.0, 59.0, 64.0]];
    let roots = [38.0f32, 35.0, 40.0, 33.0];
    for (ci, chord) in chords.iter().enumerate() {
        let bar0 = ci as f32 * 8.0 * beat;
        // Comp on beats 1 and the "and" of 2, both bars.
        for hit in [0.0f32, 1.5, 4.0, 5.5] {
            let start = bar0 + hit * beat;
            for &n in chord.iter() {
                let f = midi(n);
                add_note(&mut buf, start, 2.5, move |t| epiano(t, f, start));
            }
        }
        // Walking bass: root, fifth, octave, approach tone.
        for (k, off) in [0.0f32, 7.0, 12.0, 11.0, 0.0, 7.0, 5.0, 2.0].iter().enumerate() {
            let f = midi(roots[ci] + off);
            let start = bar0 + k as f32 * beat;
            add_note(&mut buf, start, beat * 0.95, move |t| (TAU * f * wobble(t, start) * t).sin() * (-t * 3.0).exp() * (t / 0.01).min(1.0) * 0.22);
        }
    }
    // Soft brushed hi-hat on 2 and 4.
    for b in 0..32 {
        if b % 2 == 1 {
            add_note(&mut buf, b as f32 * beat, 0.2, move |t| {
                let mut r = GameRng::new(((b as f32 + t) * SR) as u64);
                (r.next_f32() * 2.0 - 1.0) * (-t * 22.0).exp() * 0.05
            });
        }
    }
    // A high, slightly detuned music-box line that doesn't quite fit the chords.
    for (start, n) in [(3.0f32, 78.0f32), (9.0, 76.5), (15.0, 81.0), (21.0, 77.0)] {
        let f = midi(n);
        add_note(&mut buf, start, 2.5, move |t| pluck(t, f) * 0.5);
    }
    let wet = reverb(&buf, 0.3, 2.0);
    normalize(fold_loop(wet, (loop_secs * SR) as usize), 0.75)
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

/// Ending cue (22 s): the lift's hum and a hopeful pad 0–9 s, a slow climb
/// 9–12.5 s, dread 12.5–15.4 s, a crashing hit at 16.8 s, then silence.
pub fn synth_ending_cue() -> Vec<f32> {
    let len = (22.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    // Warm, hopeful pad (C major, then A minor).
    for (start, notes) in [(0.0f32, [48.0f32, 55.0, 60.0, 64.0]), (4.5, [45.0, 52.0, 57.0, 60.0])] {
        add_note(&mut buf, start, 5.0, move |t| {
            let env = (t / 1.2).min(1.0) * (1.0 - ((t - 4.2) / 0.8).clamp(0.0, 1.0));
            notes.iter().map(|n| (TAU * midi(*n) * t).sin() + 0.3 * (TAU * midi(*n) * 2.0 * t).sin()).sum::<f32>() * env * 0.05
        });
    }
    // Climb: a slow rising line under a ticking pulse.
    add_note(&mut buf, 9.0, 3.6, |t| {
        let f = midi(43.0 + t * 2.2);
        (TAU * f * t).sin() * (t / 3.6) * 0.12
    });
    for b in 0..7 {
        let t0 = 9.0 + b as f32 * 0.5;
        add_note(&mut buf, t0, 0.2, |t| (TAU * 1400.0 * t).sin() * (-t * 40.0).exp() * 0.15);
    }
    // Dread: a dissonant cluster swelling until the cable snaps.
    add_note(&mut buf, 12.5, 3.0, |t| {
        let swell = (t / 2.9).powf(1.4);
        [37.0f32, 38.0, 44.0, 49.0, 50.0].iter().map(|n| (TAU * midi(*n) * t).sin()).sum::<f32>() * swell * 0.11
    });
    // The crash.
    add_note(&mut buf, 16.8, 3.0, |t| {
        let mut r = GameRng::new((t * SR) as u64 ^ 77);
        let noise = (r.next_f32() * 2.0 - 1.0) * (-t * 3.5).exp();
        let boom = (TAU * (40.0 + 60.0 * (-t * 8.0).exp()) * t).sin() * (-t * 1.6).exp();
        noise * 0.7 + boom * 0.8
    });
    normalize(reverb(&buf, 0.3, 2.0), 0.9)
}

/// Victory theme (16 s loop): a bright, original brass-and-bells fanfare in
/// C major over a bouncing bass. All notes land on whole beats of 120 bpm.
pub fn synth_victory_theme() -> Vec<f32> {
    let len = (16.0 * SR) as usize;
    let mut buf = vec![0.0; len];
    let beat = 0.5f32;
    // Chord per bar: I  IV  V  I  vi  IV  V  I
    let bars: [[f32; 3]; 8] = [
        [60.0, 64.0, 67.0],
        [60.0, 65.0, 69.0],
        [62.0, 67.0, 71.0],
        [60.0, 64.0, 67.0],
        [57.0, 60.0, 64.0],
        [57.0, 60.0, 65.0],
        [59.0, 62.0, 67.0],
        [60.0, 64.0, 72.0],
    ];
    for (bar, chord) in bars.iter().enumerate() {
        let t0 = bar as f32 * 4.0 * beat;
        // Brassy stab on beat 1, held.
        let ch = *chord;
        add_note(&mut buf, t0, 1.6, move |t| {
            let env = (t / 0.03).min(1.0) * (-t * 1.4).exp();
            ch.iter().map(|n| {
                let f = midi(*n);
                (TAU * f * t).sin() + 0.5 * (TAU * f * 2.0 * t).sin() + 0.25 * (TAU * f * 3.0 * t).sin()
            }).sum::<f32>() * env * 0.07
        });
        // Bell arpeggio up the chord on the off-beats.
        for (k, n) in [ch[0] + 12.0, ch[1] + 12.0, ch[2] + 12.0, ch[1] + 12.0].into_iter().enumerate() {
            let ts = t0 + (k as f32 + 0.5) * beat;
            add_note(&mut buf, ts, 0.9, move |t| (TAU * midi(n) * t).sin() * (-t * 4.0).exp() * 0.09 + (TAU * midi(n) * 2.76 * t).sin() * (-t * 7.0).exp() * 0.03);
        }
        // Bouncing bass: root on 1 and 3, fifth on 2 and 4.
        for k in 0..4 {
            let n = if k % 2 == 0 { ch[0] - 24.0 } else { ch[0] - 17.0 };
            add_note(&mut buf, t0 + k as f32 * beat, 0.4, move |t| (TAU * midi(n) * t).sin() * (-t * 6.0).exp() * 0.22);
        }
    }
    // Kick on every beat, a little shimmer on the last bar.
    for b in 0..32 {
        add_note(&mut buf, b as f32 * beat, 0.25, |t| (TAU * (50.0 + 80.0 * (-t * 25.0).exp()) * t).sin() * (-t * 12.0).exp() * 0.35);
    }
    normalize(reverb(&buf, 0.2, 1.2), 0.85)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ending_and_victory_music() {
        let e = synth_ending_cue();
        assert_eq!(e.len(), (22.0 * SR) as usize);
        let v = synth_victory_theme();
        assert_eq!(v.len(), (16.0 * SR) as usize);
        assert!(v.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn loops_have_documented_lengths() {
        let menu = synth_menu_theme();
        assert_eq!(menu.len(), (32.0 * SR) as usize);
        let chase = synth_chase_theme();
        assert_eq!(chase.len(), (16.0 * SR) as usize);
        let lift = synth_lift_theme();
        assert_eq!(lift.len(), (24.0 * SR) as usize);
        assert!(lift.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
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
        assert_eq!(MusicTrack::Cutscene("02_the_hollow".into()).folder(), "music/cutscene_hollow");
        assert_eq!(MusicTrack::Cutscene("01_intro".into()).folder(), "music/cutscene_intro");
        assert!(is_audio(std::path::Path::new("My Song.MP3")));
        assert!(!is_audio(std::path::Path::new("PUT_YOUR_SONG_HERE.txt")));
        assert_eq!(MusicTrack::Chase.slot(), "music/chase");
    }
}
