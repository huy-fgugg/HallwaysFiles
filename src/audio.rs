//! Procedurally synthesised audio.
//!
//! The prototype ships no audio files: every sound is generated at startup as
//! a 16-bit PCM WAV in memory (deterministic noise, so builds sound identical).
//! Replace entries in [`SfxLibrary`] with licensed assets later.

use bevy::{audio::Volume, prelude::*};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;

use crate::rng::GameRng;
use crate::settings::Settings;

pub const SAMPLE_RATE: u32 = 22_050;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum Sfx {
    UiMove,
    UiSelect,
    DoorOpen,
    DoorLocked,
    DrawerOpen,
    DrawerClose,
    Pickup,
    ClosetEnter,
    ClosetExit,
    Footstep,
    Stinger,
    Rumble,
    Ooze,
    Whoosh,
    Ding,
    GateRattle,
    SurgeRoar,
}

impl Sfx {
    const ALL: [Sfx; 17] = [
        Sfx::UiMove,
        Sfx::UiSelect,
        Sfx::DoorOpen,
        Sfx::DoorLocked,
        Sfx::DrawerOpen,
        Sfx::DrawerClose,
        Sfx::Pickup,
        Sfx::ClosetEnter,
        Sfx::ClosetExit,
        Sfx::Footstep,
        Sfx::Stinger,
        Sfx::Rumble,
        Sfx::Ooze,
        Sfx::Whoosh,
        Sfx::Ding,
        Sfx::GateRattle,
        Sfx::SurgeRoar,
    ];

    fn is_stinger(self) -> bool {
        matches!(self, Sfx::Stinger | Sfx::Rumble | Sfx::SurgeRoar)
    }
}

/// Request to play a one-shot sound effect.
#[derive(Message, Debug, Clone, Copy)]
pub struct PlaySfx(pub Sfx);

#[derive(Resource, Default)]
pub struct SfxLibrary {
    sounds: HashMap<Sfx, Handle<AudioSource>>,
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .init_resource::<SfxLibrary>()
            .add_systems(Startup, build_library)
            .add_systems(Update, play_sfx);
    }
}

fn build_library(mut lib: ResMut<SfxLibrary>, mut sources: ResMut<Assets<AudioSource>>) {
    for sfx in Sfx::ALL {
        let samples = synth(sfx);
        let handle = sources.add(AudioSource { bytes: Arc::from(encode_wav(&samples, SAMPLE_RATE)) });
        lib.sounds.insert(sfx, handle);
    }
}

fn play_sfx(
    mut commands: Commands,
    mut reader: MessageReader<PlaySfx>,
    lib: Res<SfxLibrary>,
    settings: Res<Settings>,
) {
    for PlaySfx(sfx) in reader.read() {
        let Some(handle) = lib.sounds.get(sfx) else { continue };
        let gain = if sfx.is_stinger() { settings.stinger_gain() } else { settings.sfx_gain() };
        if gain <= 0.0 {
            continue;
        }
        commands.spawn((
            AudioPlayer(handle.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)),
        ));
    }
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

fn n_samples(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

/// Attack/decay envelope in 0..1.
fn env(t: f32, attack: f32, length: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (1.0 - (t - attack) / (length - attack)).max(0.0).powf(2.0)
    }
}

/// One-pole low-pass filtered white noise, deterministic.
fn noise(len: usize, seed: u64, smoothing: f32) -> Vec<f32> {
    let mut rng = GameRng::new(seed);
    let mut y = 0.0;
    (0..len)
        .map(|_| {
            let x = rng.next_f32() * 2.0 - 1.0;
            y += (x - y) * smoothing;
            y
        })
        .collect()
}

/// Synthesises an effect and limits its peak to 0.95 so nothing clips.
pub fn synth(sfx: Sfx) -> Vec<f32> {
    let mut v = synth_raw(sfx);
    let peak = v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.95 {
        for s in &mut v {
            *s *= 0.95 / peak;
        }
    }
    v
}

fn synth_raw(sfx: Sfx) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    match sfx {
        Sfx::UiMove => tone(0.05, |t| (TAU * 660.0 * t).sin() * env(t, 0.004, 0.05) * 0.25),
        Sfx::UiSelect => tone(0.14, |t| {
            let f = if t < 0.06 { 520.0 } else { 780.0 };
            (TAU * f * t).sin() * env(t, 0.005, 0.14) * 0.3
        }),
        Sfx::DoorOpen => {
            let len = n_samples(1.1);
            let nz = noise(len, 11, 0.08);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    // Creak: a wobbling low tone with rough noise, then a soft thud.
                    let f = 140.0 - 60.0 * (t / 1.1) + 12.0 * (TAU * 7.0 * t).sin();
                    let creak = (TAU * f * t).sin().signum() * 0.12 * env(t, 0.05, 0.9) * (0.6 + 0.4 * (TAU * 23.0 * t).sin());
                    let thud = if t > 0.85 { (TAU * 55.0 * t).sin() * env(t - 0.85, 0.005, 0.25) * 0.5 } else { 0.0 };
                    creak + nz[i] * 0.5 * env(t, 0.02, 1.1) + thud
                })
                .collect()
        }
        Sfx::DoorLocked => tone(0.3, |t| {
            let rattle = ((TAU * 18.0 * t).sin() > 0.0) as i32 as f32;
            (TAU * 210.0 * t).sin() * rattle * env(t, 0.002, 0.3) * 0.3
        }),
        Sfx::DrawerOpen | Sfx::DrawerClose => {
            let len = n_samples(0.4);
            let nz = noise(len, if sfx == Sfx::DrawerOpen { 21 } else { 22 }, 0.25);
            let closing = sfx == Sfx::DrawerClose;
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    let slide = nz[i] * 0.6 * env(t, 0.03, 0.35);
                    let knock_t = if closing { 0.3 } else { 0.0 };
                    let knock = if t >= knock_t { (TAU * 180.0 * t).sin() * env(t - knock_t, 0.002, 0.1) * 0.4 } else { 0.0 };
                    slide + knock
                })
                .collect()
        }
        Sfx::Pickup => tone(0.35, |t| {
            let f = 880.0 + 440.0 * (t * 8.0).floor().min(2.0);
            ((TAU * f * t).sin() * 0.6 + (TAU * f * 2.0 * t).sin() * 0.2) * env(t, 0.005, 0.35) * 0.3
        }),
        Sfx::ClosetEnter | Sfx::ClosetExit => {
            let len = n_samples(0.45);
            let nz = noise(len, 31, 0.15);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    let thump_t = if sfx == Sfx::ClosetEnter { 0.28 } else { 0.0 };
                    let thump = if t >= thump_t { (TAU * 70.0 * t).sin() * env(t - thump_t, 0.004, 0.15) * 0.6 } else { 0.0 };
                    nz[i] * 0.4 * env(t, 0.05, 0.4) + thump
                })
                .collect()
        }
        Sfx::Footstep => {
            let len = n_samples(0.12);
            let nz = noise(len, 41, 0.35);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    (nz[i] * 0.5 + (TAU * 90.0 * t).sin() * 0.3) * env(t, 0.003, 0.12)
                })
                .collect()
        }
        Sfx::Rumble | Sfx::Ooze | Sfx::Whoosh => extra(sfx),
        Sfx::SurgeRoar => {
            // A roar that builds for ~5 s: noise whose brightness rises, a low
            // growl underneath, and crackle bursts like shorting wiring.
            let len = n_samples(6.0);
            let mut rng = GameRng::new(111);
            let mut lp = 0.0f32;
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    let build = (t / 5.0).min(1.0).powf(1.6);
                    let x = rng.next_f32() * 2.0 - 1.0;
                    lp += (x - lp) * (0.02 + 0.5 * build);
                    let growl = (TAU * (42.0 + 30.0 * build) * t).sin() * 0.5;
                    let crackle = if rng.next_f32() < 0.002 + 0.02 * build { rng.next_f32() * 2.0 - 1.0 } else { 0.0 };
                    let tail = 1.0 - ((t - 5.4) / 0.6).clamp(0.0, 1.0);
                    (lp * 2.2 + growl * build + crackle * 0.6) * build * tail * 0.6
                })
                .collect()
        }
        Sfx::Ding => tone(2.2, |t| {
            // Small brass bell: inharmonic partials, long decay.
            [1.0f32, 2.76, 5.4, 8.9].iter().enumerate().map(|(i, h)| (TAU * 988.0 * h * t).sin() * (-t * (2.0 + i as f32 * 1.5)).exp() / (i as f32 + 1.0)).sum::<f32>() * 0.3
        }),
        Sfx::GateRattle => {
            let len = n_samples(1.4);
            let nz = noise(len, 101, 0.5);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    // Metal clatter: noise gated by fast irregular pulses.
                    let gate = ((TAU * 23.0 * t).sin() + (TAU * 37.0 * t).sin() > 0.6) as i32 as f32;
                    (nz[i] * gate * 0.7 + (TAU * 2200.0 * t).sin() * gate * 0.05) * env(t, 0.01, 1.4)
                })
                .collect()
        }
        Sfx::Stinger => {
            let len = n_samples(1.2);
            let nz = noise(len, 51, 0.6);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    // Dissonant cluster + noise burst.
                    let cluster = [311.0, 329.6, 466.2, 493.9]
                        .iter()
                        .map(|f| (TAU * f * t).sin())
                        .sum::<f32>()
                        * 0.18;
                    (cluster + nz[i] * 0.5) * env(t, 0.01, 1.2)
                })
                .collect()
        }
    }
}

fn extra(sfx: Sfx) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    match sfx {
        Sfx::Rumble => {
            let len = n_samples(3.0);
            let nz = noise(len, 71, 0.01);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    let swell = (t / 1.2).min(1.0) * (1.0 - ((t - 2.2) / 0.8).clamp(0.0, 1.0));
                    ((TAU * 38.0 * t).sin() * 0.5 + nz[i] * 3.0) * swell * 0.7
                })
                .collect()
        }
        Sfx::Ooze => {
            let len = n_samples(2.5);
            let mut rng = GameRng::new(81);
            let mut out = vec![0.0; len];
            // Wet bubbling: many short descending blips.
            for _ in 0..40 {
                let start = rng.below(len as u32 - 4000) as usize;
                let f0 = rng.range_f32(180.0, 420.0);
                for k in 0..3000 {
                    let t = k as f32 / sr;
                    let f = f0 * (1.0 - t * 4.0).max(0.3);
                    out[start + k] += (TAU * f * t).sin() * (1.0 - t / 0.136).max(0.0).powi(2) * 0.12;
                }
            }
            out
        }
        _ => {
            // Whoosh: band-limited noise swell.
            let len = n_samples(0.9);
            let nz = noise(len, 91, 0.3);
            (0..len)
                .map(|i| {
                    let t = i as f32 / sr;
                    nz[i] * (t / 0.9 * std::f32::consts::PI).sin().powi(2) * 0.9
                })
                .collect()
        }
    }
}

fn tone(seconds: f32, f: impl Fn(f32) -> f32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    (0..n_samples(seconds)).map(|i| f(i as f32 / sr)).collect()
}

/// A seamless 8 s loop: low electrical hum, slow "breathing" swell and air noise.
pub fn synth_ambience() -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let seconds = 8.0;
    let len = n_samples(seconds);
    let nz = noise(len, 61, 0.02);
    (0..len)
        .map(|i| {
            let t = i as f32 / sr;
            // All frequencies complete whole cycles in 8 s, so the loop is seamless.
            let swell = 0.6 + 0.4 * (TAU * t / seconds).sin();
            let hum = (TAU * 50.0 * t).sin() * 0.10 + (TAU * 100.0 * t).sin() * 0.04 + (TAU * 150.0 * t).sin() * 0.015;
            let fade = (t.min(seconds - t) / 0.05).min(1.0); // hide the noise seam
            hum * swell + nz[i] * 1.2 * swell * fade
        })
        .collect()
}

/// Encodes mono f32 samples (-1..1) as a 16-bit PCM WAV file.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let wav = encode_wav(&[0.0, 0.5, -0.5, 1.0], 22_050);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(wav.len(), 44 + 8);
    }

    #[test]
    fn all_sounds_are_finite_and_bounded() {
        for sfx in Sfx::ALL {
            let s = synth(sfx);
            assert!(!s.is_empty(), "{sfx:?} empty");
            assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 0.951), "{sfx:?} out of range");
        }
        assert!(synth_ambience().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn synthesis_is_deterministic() {
        assert_eq!(synth(Sfx::DoorOpen), synth(Sfx::DoorOpen));
    }
}
