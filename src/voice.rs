//! Voice chat for online rooms. Your microphone is captured on a background
//! thread (cpal), squeezed into small 4-bit ADPCM chunks (12 kHz, 100 ms
//! each, ~6 KB/s) and relayed by the room server like any other message.
//! Teammates' voices play from where they stand (quieter with distance), and
//! their character's mouth opens with how loud they're talking.
//!
//! Push to talk (hold V, or the TALK button on a phone) by default; Settings
//! can switch to an open mic (sends only while you're making sound) or off.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use bevy::audio::{AddAudioSource, Decodable, Volume};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::net::{NetSession, PeerMsg, SendPeer};

/// Sample rate of voice on the wire.
pub const RATE: u32 = 12_000;
/// Samples per chunk (100 ms).
pub const CHUNK: usize = 1_200;
/// Open mic only sends when the chunk is louder than this (RMS).
const OPEN_MIC_GATE: f32 = 0.018;
/// Playback waits for this much before starting (smooths network jitter)...
const START_BUFFER: usize = 1_800;
/// ...and drops the oldest audio past this, so the delay never builds up.
const MAX_BUFFER: usize = 7_200;
/// Voices fade out over this distance (metres).
const HEAR_RANGE: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VoiceMode {
    Off,
    #[default]
    PushToTalk,
    OpenMic,
}

impl VoiceMode {
    pub fn label(self) -> &'static str {
        match self {
            VoiceMode::Off => "OFF",
            VoiceMode::PushToTalk => "PUSH TO TALK (V)",
            VoiceMode::OpenMic => "OPEN MIC",
        }
    }
    pub fn step(self, d: i32) -> Self {
        let all = [VoiceMode::Off, VoiceMode::PushToTalk, VoiceMode::OpenMic];
        let i = all.iter().position(|m| *m == self).unwrap_or(1) as i32;
        all[(i + if d == 0 { 1 } else { d }).rem_euclid(3) as usize]
    }
}

// ---------------------------------------------------------------- codec ----

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026,
    4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn adpcm_step(code: u8, pred: &mut i32, index: &mut i32) {
    let step = STEPS[*index as usize];
    let mut diff = step >> 3;
    if code & 4 != 0 {
        diff += step;
    }
    if code & 2 != 0 {
        diff += step >> 1;
    }
    if code & 1 != 0 {
        diff += step >> 2;
    }
    *pred = if code & 8 != 0 { *pred - diff } else { *pred + diff }.clamp(-32768, 32767);
    *index = (*index + INDEX[code as usize]).clamp(0, 88);
}

/// IMA ADPCM: a 3-byte header (start value, step index), then 2 samples a byte.
pub fn encode(samples: &[f32]) -> Vec<u8> {
    let first = (samples.first().copied().unwrap_or(0.0).clamp(-1.0, 1.0) * 32767.0) as i32;
    let mut pred = first;
    let mut index = 0i32;
    let mut out = Vec::with_capacity(3 + samples.len() / 2 + 1);
    out.extend_from_slice(&(first as i16).to_le_bytes());
    out.push(0);
    let mut byte = 0u8;
    for (i, s) in samples.iter().enumerate() {
        let target = (s.clamp(-1.0, 1.0) * 32767.0) as i32;
        let step = STEPS[index as usize];
        let mut diff = target - pred;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut s2 = step;
        for bit in [4u8, 2, 1] {
            if diff >= s2 {
                code |= bit;
                diff -= s2;
            }
            s2 >>= 1;
        }
        adpcm_step(code, &mut pred, &mut index);
        if i % 2 == 0 {
            byte = code;
        } else {
            out.push(byte | (code << 4));
        }
    }
    if samples.len() % 2 == 1 {
        out.push(byte);
    }
    out
}

pub fn decode(data: &[u8]) -> Vec<f32> {
    if data.len() < 3 {
        return vec![];
    }
    let mut pred = i16::from_le_bytes([data[0], data[1]]) as i32;
    let mut index = (data[2] as i32).clamp(0, 88);
    let mut out = Vec::with_capacity((data.len() - 3) * 2);
    for b in &data[3..] {
        for code in [b & 0x0f, b >> 4] {
            adpcm_step(code, &mut pred, &mut index);
            out.push(pred as f32 / 32768.0);
        }
    }
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| B64.iter().position(|b| *b == c).map(|p| p as u32);
    let bytes: Vec<u32> = s.bytes().filter_map(val).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        let n = c.iter().enumerate().fold(0u32, |n, (i, v)| n | v << (18 - 6 * i));
        for i in 0..c.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

pub fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

// -------------------------------------------------------------- capture ----

/// The microphone thread: mono samples at [`RATE`].
struct Mic {
    rx: Receiver<Vec<f32>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn open_mic() -> Option<Mic> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    // QA: HALLWAYS_FAKE_MIC talks in syllable-like bursts instead of using a real microphone.
    if std::env::var_os("HALLWAYS_FAKE_MIC").is_some() {
        std::thread::spawn(move || {
            let mut t = 0usize;
            while !stop2.load(Ordering::Relaxed) {
                let chunk: Vec<f32> = (0..CHUNK)
                    .map(|i| {
                        let n = (t + i) as f32 / RATE as f32;
                        let syllable = ((n * 4.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5).powi(2);
                        ((n * 180.0 * std::f32::consts::TAU).sin() * 0.3 + (n * 410.0 * std::f32::consts::TAU).sin() * 0.1) * syllable
                    })
                    .collect();
                t += CHUNK;
                if tx.send(chunk).is_err() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        return Some(Mic { rx, stop });
    }
    #[cfg(target_os = "android")]
    request_mic_permission();
    std::thread::Builder::new()
        .name("voice-mic".into())
        .spawn(move || {
            let host = cpal::default_host();
            let Some(dev) = host.default_input_device() else {
                warn!("voice: no microphone");
                return;
            };
            let Ok(cfg) = dev.default_input_config() else {
                warn!("voice: microphone has no usable format");
                return;
            };
            let channels = cfg.channels().max(1) as usize;
            let rate = cfg.sample_rate() as f32;
            let format = cfg.sample_format();
            let config: cpal::StreamConfig = cfg.into();
            // Downmix to mono and resample to RATE (linear, good enough for speech).
            let mut pos = 0.0f32;
            let ratio = rate / RATE as f32;
            let mut last = 0.0f32;
            let mut push = move |mono: &mut dyn Iterator<Item = f32>| {
                let mut out = Vec::new();
                for s in mono {
                    while pos <= 1.0 {
                        out.push(last + (s - last) * pos);
                        pos += ratio;
                    }
                    pos -= 1.0;
                    last = s;
                }
                let _ = tx.send(out);
            };
            let err = |e| warn!("voice: microphone error {e}");
            let stream = match format {
                cpal::SampleFormat::F32 => dev.build_input_stream(&config, move |d: &[f32], _: &_| push(&mut d.chunks(channels).map(|c| c.iter().sum::<f32>() / channels as f32)), err, None),
                cpal::SampleFormat::I16 => dev.build_input_stream(&config, move |d: &[i16], _: &_| push(&mut d.chunks(channels).map(|c| c.iter().map(|v| *v as f32 / 32768.0).sum::<f32>() / channels as f32)), err, None),
                cpal::SampleFormat::U16 => dev.build_input_stream(&config, move |d: &[u16], _: &_| push(&mut d.chunks(channels).map(|c| c.iter().map(|v| (*v as f32 - 32768.0) / 32768.0).sum::<f32>() / channels as f32)), err, None),
                other => {
                    warn!("voice: unsupported microphone format {other:?}");
                    return;
                }
            };
            let Ok(stream) = stream else {
                warn!("voice: couldn't open the microphone (permission?)");
                return;
            };
            if stream.play().is_err() {
                return;
            }
            while !stop2.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            drop(stream);
        })
        .ok()?;
    Some(Mic { rx, stop })
}

/// Android asks for the microphone at runtime (the manifest lists it too).
#[cfg(target_os = "android")]
fn request_mic_permission() {
    let run = || -> Option<()> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let perm = env.new_string("android.permission.RECORD_AUDIO").ok()?;
        let arr = env.new_object_array(1, "java/lang/String", &perm).ok()?;
        env.call_method(&activity, "requestPermissions", "([Ljava/lang/String;I)V", &[jni::objects::JValue::Object(&arr), jni::objects::JValue::Int(7)]).ok()?;
        Some(())
    };
    if run().is_none() {
        warn!("voice: couldn't ask for the microphone permission");
    }
}

// ------------------------------------------------------------- playback ----

/// One teammate's voice: a buffer the network fills and the audio thread drains.
#[derive(Clone, Default)]
pub struct VoiceBuffer {
    samples: Arc<Mutex<VecDeque<f32>>>,
    /// Loudness of the latest chunk (f32 bits) and when it came, for the
    /// mouth. Taken from the network side, so mouths move even for players
    /// with no speakers.
    level: Arc<AtomicU32>,
    heard: Arc<Mutex<Option<std::time::Instant>>>,
}

impl VoiceBuffer {
    fn push(&self, s: &[f32]) {
        let mut q = self.samples.lock().unwrap();
        q.extend(s.iter().copied());
        while q.len() > MAX_BUFFER {
            q.pop_front();
        }
        self.level.store(rms(s).to_bits(), Ordering::Relaxed);
        *self.heard.lock().unwrap() = Some(std::time::Instant::now());
    }
    /// How loud they're talking now (0 once the chunks stop coming).
    pub fn level(&self) -> f32 {
        let fresh = self.heard.lock().unwrap().is_some_and(|t| t.elapsed().as_secs_f32() < 0.25);
        if fresh { f32::from_bits(self.level.load(Ordering::Relaxed)) } else { 0.0 }
    }
}

/// A never-ending audio source playing a [`VoiceBuffer`] (silence when empty).
#[derive(Asset, TypePath, Clone)]
pub struct VoiceStream(VoiceBuffer);

pub struct VoiceSource {
    buf: VoiceBuffer,
    playing: bool,
}

impl Iterator for VoiceSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = {
            let mut q = self.buf.samples.lock().unwrap();
            if !self.playing && q.len() >= START_BUFFER {
                self.playing = true;
            }
            if self.playing {
                match q.pop_front() {
                    Some(s) => s,
                    None => {
                        self.playing = false;
                        0.0
                    }
                }
            } else {
                0.0
            }
        };
        Some(s)
    }
}

impl bevy::audio::Source for VoiceSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> bevy::audio::ChannelCount {
        std::num::NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> bevy::audio::SampleRate {
        std::num::NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Decodable for VoiceStream {
    type Decoder = VoiceSource;
    fn decoder(&self) -> VoiceSource {
        VoiceSource { buf: self.0.clone(), playing: false }
    }
}

/// Everyone's voice buffers, by player id (the avatars read the levels).
#[derive(Resource, Default)]
pub struct Voices(pub HashMap<u8, VoiceBuffer>);

/// A teammate's voice player.
#[derive(Component)]
struct VoicePlayer(u8);

/// Whether you're sending your voice right now (for the HUD).
#[derive(Resource, Default)]
pub struct Transmitting(pub bool);

/// A voice chunk that arrived from a teammate.
#[derive(Message)]
pub struct VoiceIn {
    pub from: u8,
    pub data: String,
}

#[derive(Component)]
struct MicBadge;

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<VoiceStream>()
            .init_resource::<Voices>()
            .init_resource::<Transmitting>()
            .add_message::<VoiceIn>()
            .add_systems(Update, (send_voice, receive_voice, place_voices, mic_badge));
    }
}

#[allow(clippy::too_many_arguments)]
fn send_voice(
    session: Res<NetSession>,
    settings: Res<crate::settings::Settings>,
    keys: Res<ButtonInput<KeyCode>>,
    touch: Res<crate::touch::TouchTalk>,
    typing: Res<crate::ui::play_menu::Typing>,
    mut out: MessageWriter<SendPeer>,
    mut tx: ResMut<Transmitting>,
    mut mic: Local<Option<Mic>>,
    mut pending: Local<Vec<f32>>,
    mut seq: Local<u32>,
) {
    let mode = if std::env::var_os("HALLWAYS_FAKE_MIC").is_some() { VoiceMode::OpenMic } else { settings.voice };
    if !session.online() || mode == VoiceMode::Off {
        *mic = None;
        tx.0 = false;
        pending.clear();
        return;
    }
    let held = (keys.pressed(KeyCode::KeyV) && !typing.0) || touch.0;
    // Push to talk only switches the microphone on the first time you talk.
    if mic.is_none() && (mode == VoiceMode::OpenMic || held) {
        *mic = open_mic();
    }
    let Some(m) = mic.as_ref() else { return };
    while let Ok(s) = m.rx.try_recv() {
        pending.extend(s);
    }
    let mut sending = false;
    while pending.len() >= CHUNK {
        let chunk: Vec<f32> = pending.drain(..CHUNK).collect();
        let loud = rms(&chunk);
        let send = match mode {
            VoiceMode::PushToTalk => held,
            VoiceMode::OpenMic => loud > OPEN_MIC_GATE,
            VoiceMode::Off => false,
        };
        if send {
            *seq = seq.wrapping_add(1);
            out.write(SendPeer(PeerMsg::Voice { seq: *seq, data: b64_encode(&encode(&chunk)) }));
            sending = true;
        }
    }
    // Keep showing the badge between chunks while the key is held.
    tx.0 = sending || (mode == VoiceMode::PushToTalk && held);
}

fn receive_voice(mut commands: Commands, mut inbox: MessageReader<VoiceIn>, mut voices: ResMut<Voices>, mut streams: ResMut<Assets<VoiceStream>>, settings: Res<crate::settings::Settings>) {
    for v in inbox.read() {
        if settings.voice == VoiceMode::Off {
            continue;
        }
        let samples = decode(&b64_decode(&v.data));
        let buf = voices.0.entry(v.from).or_insert_with(|| {
            info!("voice: hearing player {}", v.from);
            let b = VoiceBuffer::default();
            let handle = streams.add(VoiceStream(b.clone()));
            commands.spawn((AudioPlayer(handle), PlaybackSettings::ONCE.with_volume(Volume::Linear(0.0)), VoicePlayer(v.from), Name::new("Voice")));
            b
        });
        buf.push(&samples);
    }
}

/// Voices are louder close by, quieter far away, silent past the hearing range.
fn place_voices(
    mut commands: Commands,
    session: Res<NetSession>,
    peers: Res<crate::net::Peers>,
    settings: Res<crate::settings::Settings>,
    me: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    mut sinks: Query<(Entity, &VoicePlayer, Option<&mut AudioSink>)>,
    mut voices: ResMut<Voices>,
) {
    let master = settings.master_volume as f32 / 100.0 * settings.voice_volume as f32 / 100.0;
    let here = me.single().map(|g| g.translation()).ok();
    for (e, vp, sink) in &mut sinks {
        let Some(st) = peers.0.get(&vp.0) else {
            // They left: stop their voice.
            commands.entity(e).despawn();
            voices.0.remove(&vp.0);
            continue;
        };
        let k = match here {
            // In the menus or the lobby UI: full volume.
            None => 1.0,
            Some(p) => {
                let d = p.distance(Vec3::from_array(st.p));
                (1.0 - (d - 2.0).max(0.0) / HEAR_RANGE).clamp(0.0, 1.0).powf(1.5)
            }
        };
        let k = if session.online() && st.alive { k } else { 0.0 };
        if let Some(mut s) = sink {
            s.set_volume(Volume::Linear(master * k * 1.6));
        }
    }
}

fn mic_badge(mut commands: Commands, tx: Res<Transmitting>, theme: Res<crate::theme::UiTheme>, badge: Query<Entity, With<MicBadge>>) {
    match (tx.0, badge.single().ok()) {
        (true, None) => {
            commands.spawn((
                Node { position_type: PositionType::Absolute, left: px(24), bottom: px(64), padding: UiRect::axes(px(10), px(4)), border_radius: BorderRadius::all(px(6)), ..default() },
                BackgroundColor(Color::srgba(0.6, 0.1, 0.1, 0.75)),
                GlobalZIndex(30),
                MicBadge,
                Pickable::IGNORE,
            ))
            .with_child((Text::new("* TALKING"), theme.font(true, theme.size_small), TextColor(Color::WHITE)));
        }
        (false, Some(e)) => commands.entity(e).despawn(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adpcm_round_trip_keeps_speech() {
        // A wobbling tone like a voice.
        let s: Vec<f32> = (0..CHUNK).map(|i| ((i as f32 * 0.07).sin() * 0.4 + (i as f32 * 0.31).sin() * 0.15) * (1.0 - i as f32 / CHUNK as f32 * 0.5)).collect();
        let bytes = encode(&s);
        assert_eq!(bytes.len(), 3 + CHUNK / 2, "4 bits a sample");
        let back = decode(&bytes);
        assert_eq!(back.len(), CHUNK);
        let err = (s.iter().zip(&back).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / CHUNK as f32).sqrt();
        assert!(err < 0.03, "error {err}");
        // Small enough for the server's 16 KB frames.
        assert!(b64_encode(&bytes).len() < 1_000);
    }

    #[test]
    fn base64_round_trips() {
        for n in 0..20 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)), data);
        }
        assert_eq!(b64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn voice_modes_cycle() {
        assert_eq!(VoiceMode::default(), VoiceMode::PushToTalk);
        assert_eq!(VoiceMode::OpenMic.step(1), VoiceMode::Off);
        assert_eq!(VoiceMode::Off.step(-1), VoiceMode::OpenMic);
    }
}
