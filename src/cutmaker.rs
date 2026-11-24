//! Cutscene Maker: make your own cutscenes for a mode as a simple list of
//! shots. Each shot is a camera move picked from a list, how long it lasts,
//! a caption and an effect (lights, flash, shake, sounds). A cutscene plays
//! at the start of the mode or when you walk through a chosen door. They are
//! turned into the same timelines as the game's own cutscenes
//! ([`crate::cutscene::CutsceneDef`]), so they look and skip the same.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::Sfx;
use crate::cutscene::{CamPoint, CameraShot, CueAction, CueEvent, CutsceneDef, Ease, EndAction};
use crate::presets::ActivePreset;
use crate::run::RunState;
use crate::state::AppState;

pub const MAX_CUTSCENES: usize = 6;
pub const MAX_SHOTS: usize = 8;
/// Id prefix for a mode's own cutscene ("mode:2").
pub const MODE_PREFIX: &str = "mode:";

/// When a cutscene plays: at the start (0) or on walking through door `n`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeCutscene {
    pub door: u32,
    pub shots: Vec<ModeShot>,
}

impl Default for ModeCutscene {
    fn default() -> Self {
        Self { door: 0, shots: vec![ModeShot { camera: 1, seconds: 3, text: "Something is waiting in the dark...".into(), effect: 1 }, ModeShot { camera: 7, seconds: 2, text: String::new(), effect: 0 }] }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeShot {
    /// Index into [`CAMERAS`].
    pub camera: u8,
    /// Seconds, 1..=8.
    pub seconds: u8,
    /// Caption shown during the shot (empty = none).
    pub text: String,
    /// Index into [`EFFECTS`].
    pub effect: u8,
}

impl Default for ModeShot {
    fn default() -> Self {
        Self { camera: 0, seconds: 3, text: String::new(), effect: 0 }
    }
}

/// Camera moves: name, and (from, to) as anchor offsets (x across, y up,
/// z negative = into the room from its entry door) or the player's eyes.
pub const CAMERAS: [&str; 8] = ["Doorway push-in", "High corner", "Low and wide", "Look back at door", "From the far end", "Ceiling, looking down", "Pan across", "Your eyes"];
pub const EFFECTS: [&str; 10] = ["None", "Lights flicker", "Red lights", "Lights out", "White flash", "Shake + rumble", "Scream", "Whispers", "Heartbeat", "Title card"];
pub const DOORS: [u32; 12] = [0, 2, 5, 10, 15, 20, 30, 40, 50, 60, 80, 99];

fn cam(i: u8) -> (CamPoint, CamPoint, f32) {
    let a = |p: (f32, f32, f32), l: (f32, f32, f32)| CamPoint::Anchor { pos: p, look: l };
    match i {
        0 => (a((0.0, 1.7, -0.3), (0.0, 1.5, -10.0)), a((0.0, 1.65, -2.2), (0.0, 1.4, -10.0)), 66.0),
        1 => (a((1.8, 2.6, -0.8), (0.0, 1.0, -9.0)), a((1.0, 2.3, -1.8), (0.0, 1.2, -10.0)), 62.0),
        2 => (a((-1.6, 0.6, -2.4), (0.6, 1.3, -10.0)), a((-1.0, 0.7, -3.2), (0.4, 1.4, -11.0)), 72.0),
        3 => (a((0.3, 1.6, -5.5), (0.0, 1.3, 0.0)), a((0.0, 1.5, -4.2), (0.0, 1.3, 0.0)), 60.0),
        4 => (a((0.6, 1.9, -9.0), (0.0, 1.2, 0.0)), a((-0.6, 1.8, -8.2), (0.0, 1.2, 0.0)), 58.0),
        5 => (a((0.0, 2.7, -3.0), (0.0, 0.0, -3.6)), a((0.0, 2.6, -4.4), (0.0, 0.0, -5.0)), 70.0),
        6 => (a((0.0, 1.6, -2.5), (3.0, 1.4, -7.0)), a((0.0, 1.6, -2.5), (-3.0, 1.4, -7.0)), 64.0),
        _ => (CamPoint::PlayerEye, CamPoint::PlayerEye, 72.0),
    }
}

impl ModeCutscene {
    pub fn duration(&self) -> f32 {
        self.shots.iter().map(|s| s.seconds.clamp(1, 8) as f32).sum::<f32>() + 0.8
    }

    pub fn trigger_name(&self) -> String {
        if self.door == 0 { "At the start".into() } else { format!("Through door {:03}", self.door) }
    }

    pub fn sanitized(mut self) -> Self {
        self.shots.truncate(MAX_SHOTS);
        if self.shots.is_empty() {
            self.shots.push(ModeShot::default());
        }
        for s in &mut self.shots {
            s.camera %= CAMERAS.len() as u8;
            s.effect %= EFFECTS.len() as u8;
            s.seconds = s.seconds.clamp(1, 8);
            s.text = s.text.chars().filter(|c| !c.is_control()).take(60).collect();
        }
        self.door = self.door.min(99);
        self
    }

    /// The playable timeline. `title` is the mode's name (for title cards).
    pub fn to_def(&self, title: &str) -> CutsceneDef {
        let mut shots = vec![];
        let mut events = vec![CueEvent { at: 0.0, action: CueAction::Fade { alpha: 1.0, seconds: 0.0, white: false }, players: None }, CueEvent { at: 0.05, action: CueAction::Fade { alpha: 0.0, seconds: 0.8, white: false }, players: None }];
        let mut t = 0.0f32;
        for s in &self.shots {
            let secs = s.seconds.clamp(1, 8) as f32;
            let (from, to, fov) = cam(s.camera);
            shots.push(CameraShot { start: t, end: t + secs, from, to, fov, ease: Ease::InOut, players: None });
            let ev = |at: f32, action: CueAction| CueEvent { at, action, players: None };
            let danger = matches!(s.effect, 2 | 6);
            match s.effect {
                1 => events.push(ev(t, CueAction::Lights { color: (1.0, 1.0, 1.0), amount: 0.0, scale: 0.8, flicker: 0.85 })),
                2 => {
                    events.push(ev(t, CueAction::Lights { color: (1.0, 0.1, 0.08), amount: 0.85, scale: 0.8, flicker: 0.3 }));
                    events.push(ev(t, CueAction::Sfx(Sfx::Rumble)));
                }
                3 => events.push(ev(t, CueAction::Lights { color: (1.0, 1.0, 1.0), amount: 0.0, scale: 0.04, flicker: 0.0 })),
                4 => {
                    events.push(ev(t, CueAction::Fade { alpha: 1.0, seconds: 0.0, white: true }));
                    events.push(ev(t + 0.05, CueAction::Fade { alpha: 0.0, seconds: 0.7, white: true }));
                }
                5 => {
                    events.push(ev(t, CueAction::Shake(0.8)));
                    events.push(ev(t, CueAction::Sfx(Sfx::Rumble)));
                }
                6 => {
                    events.push(ev(t, CueAction::Sfx(Sfx::Stinger)));
                    events.push(ev(t, CueAction::Shake(0.5)));
                }
                7 => events.push(ev(t, CueAction::Sfx(Sfx::Whisper))),
                8 => events.push(ev(t, CueAction::Sfx(Sfx::Heartbeat))),
                _ => {}
            }
            if !s.text.trim().is_empty() {
                if s.effect == 9 {
                    events.push(ev(t + 0.2, CueAction::Title { text: s.text.clone(), sub: title.to_uppercase(), seconds: secs }));
                } else {
                    events.push(ev(t + 0.2, CueAction::Subtitle { text: s.text.clone(), seconds: secs - 0.2, danger }));
                }
            }
            // Lights changed by an effect come back for the next shot.
            if matches!(s.effect, 1..=3) {
                events.push(ev(t + secs - 0.01, CueAction::Lights { color: (1.0, 1.0, 1.0), amount: 0.0, scale: 1.0, flicker: 0.0 }));
            }
            t += secs;
        }
        // Ease back into your eyes so control is handed over smoothly.
        let last_to = shots.last().map_or(CamPoint::PlayerEye, |s| s.to);
        shots.push(CameraShot { start: t, end: t + 0.8, from: last_to, to: CamPoint::PlayerEye, fov: 72.0, ease: Ease::InOut, players: None });
        events.sort_by(|a, b| a.at.total_cmp(&b.at));
        CutsceneDef { title: title.to_string(), duration: t + 0.8, letterbox: true, skippable: true, shots, events, end: EndAction::GiveControl }
    }
}

/// The mode's cutscene with this id ("mode:N"), as a timeline.
pub fn mode_def(id: &str, active: &ActivePreset) -> Option<CutsceneDef> {
    let i: usize = id.strip_prefix(MODE_PREFIX)?.parse().ok()?;
    let p = active.0.as_ref()?;
    p.cutscenes.get(i).map(|c| c.to_def(&p.name))
}

/// Index of the mode's start cutscene, if it has one.
pub fn start_cutscene(active: &ActivePreset) -> Option<usize> {
    active.0.as_ref()?.cutscenes.iter().position(|c| c.door == 0)
}

#[derive(Resource, Default)]
struct Played(Vec<usize>, u32);

pub struct CutMakerPlugin;

impl Plugin for CutMakerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Played>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(Played::default()))
            .add_systems(Update, door_cutscenes.run_if(crate::state::gameplay_active));
    }
}

/// Plays a mode's door cutscenes when you walk into that room.
fn door_cutscenes(
    run: Res<RunState>,
    active: Res<ActivePreset>,
    floor: Option<Res<crate::floor::Floor>>,
    pursuit: Res<crate::pursuit::Pursuit>,
    mut played: ResMut<Played>,
    mut anchor: ResMut<crate::ending::CutsceneAnchor>,
    mut play: MessageWriter<crate::cutscene::PlayCutscene>,
) {
    if played.1 == run.current_room {
        return;
    }
    played.1 = run.current_room;
    let (Some(p), Some(floor)) = (active.0.as_ref(), floor) else { return };
    if pursuit.active() {
        return;
    }
    for (i, c) in p.cutscenes.iter().enumerate() {
        if c.door == 0 || c.door != run.current_room || played.0.contains(&i) {
            continue;
        }
        let Some(r) = floor.0.room(run.current_room) else { continue };
        played.0.push(i);
        anchor.0 = Vec3::new(r.entry_x, r.entry_height, r.max.y);
        play.write(crate::cutscene::PlayCutscene(format!("{MODE_PREFIX}{i}")));
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_become_a_timeline_that_ends_in_your_eyes() {
        let c = ModeCutscene { door: 5, shots: vec![ModeShot { camera: 1, seconds: 3, text: "Hello".into(), effect: 2 }, ModeShot { camera: 6, seconds: 2, text: "Big".into(), effect: 9 }] };
        let d = c.to_def("My Mode");
        assert!((d.duration - 5.8).abs() < 1e-4);
        assert_eq!(d.shots.len(), 3);
        assert!(matches!(d.shots.last().unwrap().to, CamPoint::PlayerEye));
        assert!(d.events.windows(2).all(|w| w[0].at <= w[1].at));
        assert!(d.events.iter().any(|e| matches!(&e.action, CueAction::Subtitle { text, danger: true, .. } if text == "Hello")));
        assert!(d.events.iter().any(|e| matches!(&e.action, CueAction::Title { text, .. } if text == "Big")));
        // Red lights are put back afterwards.
        assert!(d.events.iter().any(|e| matches!(e.action, CueAction::Lights { scale, .. } if scale == 1.0)));
        // Every camera and effect works.
        for camera in 0..CAMERAS.len() as u8 {
            for effect in 0..EFFECTS.len() as u8 {
                let d = ModeCutscene { door: 0, shots: vec![ModeShot { camera, seconds: 1, text: "x".into(), effect }] }.to_def("m");
                assert!(d.duration > 1.0 && !d.shots.is_empty());
            }
        }
    }

    #[test]
    fn sanitizing_keeps_it_playable() {
        let c = ModeCutscene { door: 500, shots: vec![] }.sanitized();
        assert_eq!(c.door, 99);
        assert_eq!(c.shots.len(), 1);
        let long = ModeCutscene { door: 0, shots: vec![ModeShot { camera: 200, seconds: 0, text: "a".repeat(200), effect: 99 }; 20] }.sanitized();
        assert_eq!(long.shots.len(), MAX_SHOTS);
        assert!(long.shots.iter().all(|s| (s.camera as usize) < CAMERAS.len() && s.seconds >= 1 && s.text.len() <= 60));
    }
}
