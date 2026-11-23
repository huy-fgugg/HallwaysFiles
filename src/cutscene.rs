//! Data-driven, in-engine cutscenes.
//!
//! Each cutscene lives in `assets/cutscenes/<id>/`:
//! * `cutscene.ron` — timeline: camera shots, subtitles, lights, fades, sounds.
//! * `music.ogg|mp3|wav` — optional drop-in music, started at t = 0.
//!
//! If `cutscene.ron` is missing or invalid the copy compiled into the game is
//! used, so the .exe never breaks because of a bad edit.

use bevy::prelude::*;
use serde::Deserialize;

use crate::audio::{PlaySfx, Sfx};
use crate::hollow::HollowCommand;
use crate::music::asset_root;
use crate::player::{Player, PlayerCamera};
use crate::run::{CameraShake, Subtitle};
use crate::settings::Settings;
use crate::state::{world_running, AppState};
use crate::theme::UiTheme;
use crate::world::{elevator::ElevatorCommand, Mood};

pub const INTRO: &str = "01_intro";
pub const THE_HOLLOW: &str = "02_the_hollow";
pub const ENDING: &str = "03_ending";
/// Opening of a custom floor (and maps from the map builder).
pub const CUSTOM_START: &str = "04_custom_start";
/// Retry: back in the lift for a fresh floor.
pub const RETRY: &str = "05_retry";

/// Extra words for captions: `{floor}`, `{area}` and `{doors}`.
#[derive(Resource, Debug, Clone, Default)]
pub struct CutsceneVars {
    pub floor: String,
    pub area: String,
    pub doors: u32,
}

fn embedded(id: &str) -> Option<&'static str> {
    match id {
        INTRO => Some(include_str!("../assets/cutscenes/01_intro/cutscene.ron")),
        THE_HOLLOW => Some(include_str!("../assets/cutscenes/02_the_hollow/cutscene.ron")),
        ENDING => Some(include_str!("../assets/cutscenes/03_ending/cutscene.ron")),
        CUSTOM_START => Some(include_str!("../assets/cutscenes/04_custom_start/cutscene.ron")),
        RETRY => Some(include_str!("../assets/cutscenes/05_retry/cutscene.ron")),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CutsceneDef {
    pub title: String,
    pub duration: f32,
    #[serde(default = "yes")]
    pub letterbox: bool,
    #[serde(default = "yes")]
    pub skippable: bool,
    pub shots: Vec<CameraShot>,
    #[serde(default)]
    pub events: Vec<CueEvent>,
    #[serde(default)]
    pub end: EndAction,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum CamPoint {
    /// Absolute world position + look-at target.
    At { pos: (f32, f32, f32), look: (f32, f32, f32) },
    /// The player's current eye position and view direction.
    PlayerEye,
    /// Position + look-at relative to the cutscene anchor (the ending uses
    /// the far side of the final door, which moves with every seed).
    Anchor { pos: (f32, f32, f32), look: (f32, f32, f32) },
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub enum Ease {
    Linear,
    #[default]
    InOut,
    Out,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CameraShot {
    pub start: f32,
    pub end: f32,
    pub from: CamPoint,
    pub to: CamPoint,
    #[serde(default = "default_fov")]
    pub fov: f32,
    #[serde(default)]
    pub ease: Ease,
    /// Only for this many players (min, max), e.g. (1, 1) solo or (2, 5) groups.
    #[serde(default)]
    pub players: Option<(u8, u8)>,
}

fn default_fov() -> f32 {
    60.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct CueEvent {
    pub at: f32,
    #[serde(rename = "do")]
    pub action: CueAction,
    /// Only for this many players (min, max).
    #[serde(default)]
    pub players: Option<(u8, u8)>,
}

/// Wording for captions: `{n}` becomes the number of players, `{N}` the
/// number in words, and `{party}` "you" (solo) or "the N of you".
pub fn party_text(text: &str, players: u8) -> String {
    let words = ["ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE"];
    let small = ["zero", "one", "two", "three", "four", "five"];
    let n = players.clamp(1, 5) as usize;
    let party = if n == 1 { "you".to_string() } else { format!("the {} of you", small[n]) };
    text.replace("{n}", &n.to_string()).replace("{N}", words[n]).replace("{party}", &party)
}

impl CutsceneDef {
    /// The version of this cutscene for `players` players: shots and events
    /// meant for other group sizes are dropped and captions are filled in.
    pub fn for_players(self, players: u8) -> Self {
        self.with_vars(players, &CutsceneVars::default())
    }

    /// As [`Self::for_players`], also filling in `{floor}`, `{area}` and `{doors}`.
    pub fn with_vars(mut self, players: u8, vars: &CutsceneVars) -> Self {
        let fill = |t: &str| party_text(t, players).replace("{floor}", &vars.floor.to_uppercase()).replace("{area}", &vars.area.to_uppercase()).replace("{doors}", &vars.doors.to_string());
        let fits = |r: Option<(u8, u8)>| r.is_none_or(|(lo, hi)| (lo..=hi).contains(&players));
        self.shots.retain(|s| fits(s.players));
        self.events.retain(|e| fits(e.players));
        for e in &mut self.events {
            match &mut e.action {
                CueAction::Subtitle { text, .. } => *text = fill(text),
                CueAction::Title { text, sub, .. } => {
                    *text = fill(text);
                    *sub = fill(sub);
                }
                _ => {}
            }
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum CueAction {
    Subtitle { text: String, seconds: f32, #[serde(default)] danger: bool },
    /// Fade the screen overlay to `alpha` over `seconds` (white flashes become black with Reduce Flashing).
    Fade { alpha: f32, seconds: f32, #[serde(default)] white: bool },
    Title { text: String, #[serde(default)] sub: String, seconds: f32 },
    /// Tint every level light toward `color` by `amount`, scaling brightness.
    Lights { color: (f32, f32, f32), amount: f32, scale: f32, #[serde(default)] flicker: f32 },
    Sfx(Sfx),
    Shake(f32),
    Hollow(HollowCommand),
    Elevator(crate::world::elevator::ElevatorCommand),
    /// Commands for the ending set (exit lift, characters, monster).
    Ending(crate::ending::EndingCue),
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
pub enum EndAction {
    #[default]
    GiveControl,
    StartChase,
    /// Open the victory (results) screen.
    Victory,
}

pub fn parse(text: &str) -> Result<CutsceneDef, String> {
    ron::from_str::<CutsceneDef>(text).map_err(|e| e.to_string())
}

/// Loads `assets/cutscenes/<id>/cutscene.ron`, falling back to the built-in copy.
pub fn load_def(id: &str) -> Option<CutsceneDef> {
    let path = asset_root().join("cutscenes").join(id).join("cutscene.ron");
    if let Ok(text) = std::fs::read_to_string(&path) {
        match parse(&text) {
            Ok(def) => return Some(def),
            Err(e) => warn!("{}: {e} — using built-in version", path.display()),
        }
    }
    embedded(id).and_then(|t| parse(t).ok())
}

/// Request to start a cutscene by id.
#[derive(Message, Debug, Clone)]
pub struct PlayCutscene(pub String);

/// Sent when a cutscene finishes (or is skipped).
#[derive(Message, Debug, Clone)]
pub struct CutsceneEnded {
    pub id: String,
    pub end: EndAction,
}

#[derive(Resource, Debug, Default)]
pub struct ActiveCutscene {
    pub id: String,
    pub def: Option<CutsceneDef>,
    pub t: f32,
    fired: usize,
    skip_hold: f32,
    /// A skip press began during this cutscene (a key already held when it
    /// started, e.g. the Enter that started the run, doesn't count).
    skip_armed: bool,
    fade: FadeState,
    title: Option<(String, String, f32, f32)>,
    /// Ending-set commands waiting for `ending` to run them (cue, instant).
    pub ending: Vec<(crate::ending::EndingCue, bool)>,
}

impl ActiveCutscene {
    pub fn is_playing(&self) -> bool {
        self.def.is_some()
    }

    /// Stops immediately with no end action and a clear screen (trailer cuts).
    pub fn stop(&mut self) {
        self.def = None;
        self.title = None;
        self.ending.clear();
        self.fade = FadeState::default();
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct FadeState {
    alpha: f32,
    from: f32,
    to: f32,
    t: f32,
    seconds: f32,
    white: bool,
}

/// Tracks which story beats have already happened this session.
#[derive(Resource, Debug, Default)]
pub struct SeenCutscenes(pub Vec<String>);

#[derive(Component)]
struct CutsceneUi;
#[derive(Component)]
struct LetterboxBar;
#[derive(Component)]
struct FadeOverlay;
#[derive(Component)]
struct TitleText;
#[derive(Component)]
struct TitleSub;
#[derive(Component)]
struct SkipHint;
#[derive(Component)]
struct CutsceneSubtitle;

const SKIP_HOLD: f32 = 0.8;

pub struct CutscenePlugin;

impl Plugin for CutscenePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayCutscene>()
            .add_message::<CutsceneEnded>()
            .init_resource::<ActiveCutscene>()
            .init_resource::<CutsceneVars>()
            .init_resource::<SeenCutscenes>()
            .add_systems(OnEnter(AppState::Playing), (spawn_cutscene_ui, reset).chain())
            .add_systems(OnExit(AppState::Playing), |mut a: ResMut<ActiveCutscene>| *a = ActiveCutscene::default())
            .add_systems(
                Update,
                (start_cutscenes, (run_timeline, skip_input).run_if(world_running), drive_camera.after(crate::player::CameraSet), update_ui)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn reset(mut active: ResMut<ActiveCutscene>) {
    *active = ActiveCutscene::default();
}

fn start_cutscenes(
    mut reader: MessageReader<PlayCutscene>,
    mut active: ResMut<ActiveCutscene>,
    mut seen: ResMut<SeenCutscenes>,
    peers: Option<Res<crate::net::Peers>>,
    vars: Option<Res<CutsceneVars>>,
) {
    for PlayCutscene(id) in reader.read() {
        // You plus everyone else in the room.
        let players = 1 + peers.as_ref().map_or(0, |p| p.0.values().filter(|s| s.alive).count()) as u8;
        let vars = vars.as_deref().cloned().unwrap_or_default();
        match load_def(id).map(|d| d.with_vars(players.min(5), &vars)) {
            Some(def) => {
                info!("cutscene: {id} ({:.1}s)", def.duration);
                *active = ActiveCutscene { id: id.clone(), def: Some(def), ..default() };
                if !seen.0.contains(id) {
                    seen.0.push(id.clone());
                }
            }
            None => warn!("cutscene {id} not found"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_event(
    action: &CueAction,
    instant: bool,
    active: &mut ActiveCutscene,
    settings: &Settings,
    mood: &mut Mood,
    shake: &mut CameraShake,
    sfx: &mut MessageWriter<PlaySfx>,
    subs: &mut MessageWriter<Subtitle>,
    hollow: &mut MessageWriter<HollowCommand>,
    lift: &mut MessageWriter<ElevatorCommand>,
) {
    match action {
        CueAction::Subtitle { text, seconds, danger } if !instant => {
            subs.write(Subtitle { text: text.clone(), seconds: *seconds, danger: *danger });
        }
        CueAction::Fade { alpha, seconds, white } if !instant => {
            let white = *white && !settings.reduce_flashing;
            let seconds = if settings.reduce_flashing { seconds.max(0.4) } else { *seconds };
            active.fade = FadeState { alpha: active.fade.alpha, from: active.fade.alpha, to: *alpha, t: 0.0, seconds: seconds.max(0.001), white };
        }
        CueAction::Title { text, sub, seconds } if !instant => active.title = Some((text.clone(), sub.clone(), 0.0, *seconds)),
        CueAction::Lights { color, amount, scale, flicker } => {
            mood.tint = Color::srgb(color.0, color.1, color.2);
            mood.amount = amount.clamp(0.0, 1.0);
            mood.scale = scale.max(0.0);
            mood.flicker = *flicker;
        }
        CueAction::Sfx(s) if !instant => {
            sfx.write(PlaySfx(*s));
        }
        CueAction::Shake(a) if !instant => shake.add(*a),
        CueAction::Hollow(cmd) => {
            hollow.write(if instant { cmd.instant() } else { cmd.clone() });
        }
        CueAction::Elevator(cmd) => {
            lift.write(if instant { cmd.instant() } else { cmd.clone() });
        }
        CueAction::Ending(cue) => active.ending.push((cue.clone(), instant)),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn run_timeline(
    time: Res<Time>,
    settings: Res<Settings>,
    mut active: ResMut<ActiveCutscene>,
    mut mood: ResMut<Mood>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut hollow: MessageWriter<HollowCommand>,
    mut lift: MessageWriter<ElevatorCommand>,
    mut ended: MessageWriter<CutsceneEnded>,
) {
    let dt = time.delta_secs();
    // Fades and titles keep animating briefly after a cutscene ends.
    let f = &mut active.fade;
    if f.t < f.seconds {
        f.t = (f.t + dt).min(f.seconds);
        f.alpha = f.from + (f.to - f.from) * (f.t / f.seconds);
    }
    if let Some(title) = active.title.as_mut() {
        title.2 += dt;
        if title.2 > title.3 {
            active.title = None;
        }
    }
    let Some(def) = active.def.clone() else { return };
    active.t += dt;
    while active.fired < def.events.len() && def.events[active.fired].at <= active.t {
        let action = def.events[active.fired].action.clone();
        apply_event(&action, false, &mut active, &settings, &mut mood, &mut shake, &mut sfx, &mut subs, &mut hollow, &mut lift);
        active.fired += 1;
    }
    if active.t >= def.duration {
        finish(&mut active, &def, &settings, &mut mood, &mut shake, &mut sfx, &mut subs, &mut hollow, &mut lift, &mut ended);
    }
}

#[allow(clippy::too_many_arguments)]
fn finish(
    active: &mut ActiveCutscene,
    def: &CutsceneDef,
    settings: &Settings,
    mood: &mut Mood,
    shake: &mut CameraShake,
    sfx: &mut MessageWriter<PlaySfx>,
    subs: &mut MessageWriter<Subtitle>,
    hollow: &mut MessageWriter<HollowCommand>,
    lift: &mut MessageWriter<ElevatorCommand>,
    ended: &mut MessageWriter<CutsceneEnded>,
) {
    // Apply any state-changing events that were skipped so the world ends up consistent.
    while active.fired < def.events.len() {
        let action = def.events[active.fired].action.clone();
        apply_event(&action, true, active, settings, mood, shake, sfx, subs, hollow, lift);
        active.fired += 1;
    }
    active.fade = FadeState { alpha: active.fade.alpha, from: active.fade.alpha, to: 0.0, t: 0.0, seconds: 0.6, white: false };
    ended.write(CutsceneEnded { id: active.id.clone(), end: def.end });
    active.def = None;
}

#[allow(clippy::too_many_arguments)]
fn skip_input(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Res<Settings>,
    mut active: ResMut<ActiveCutscene>,
    mut mood: ResMut<Mood>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut hollow: MessageWriter<HollowCommand>,
    mut lift: MessageWriter<ElevatorCommand>,
    mut ended: MessageWriter<CutsceneEnded>,
) {
    let Some(def) = active.def.clone() else { return };
    if !def.skippable {
        return;
    }
    let held = keys.any_pressed([KeyCode::Space, KeyCode::Enter]) || pads.iter().any(|p| p.pressed(GamepadButton::South));
    let began = keys.any_just_pressed([KeyCode::Space, KeyCode::Enter]) || pads.iter().any(|p| p.just_pressed(GamepadButton::South));
    if began && active.t > 0.0 {
        active.skip_armed = true;
    }
    if !held {
        active.skip_armed = false;
    }
    // Count real holding time, at most 1/4 s per frame so one long frame
    // (a loading hitch on a slow device) can't skip the cutscene by itself.
    let dt = time.delta_secs().min(0.25);
    active.skip_hold = if held && active.skip_armed { active.skip_hold + dt } else { 0.0 };
    if active.skip_hold >= SKIP_HOLD {
        active.title = None;
        finish(&mut active, &def, &settings, &mut mood, &mut shake, &mut sfx, &mut subs, &mut hollow, &mut lift, &mut ended);
    }
}

fn ease(e: Ease, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    match e {
        Ease::Linear => x,
        Ease::InOut => x * x * (3.0 - 2.0 * x),
        Ease::Out => 1.0 - (1.0 - x).powi(3),
    }
}

/// Resolves a camera point to (position, look target) in world space.
pub fn resolve(point: CamPoint, player_pos: Vec3, yaw: f32, pitch: f32, eye: f32) -> (Vec3, Vec3) {
    resolve_anchored(point, player_pos, yaw, pitch, eye, Vec3::ZERO)
}

pub fn resolve_anchored(point: CamPoint, player_pos: Vec3, yaw: f32, pitch: f32, eye: f32, anchor: Vec3) -> (Vec3, Vec3) {
    match point {
        CamPoint::At { pos, look } => (Vec3::from(pos), Vec3::from(look)),
        CamPoint::Anchor { pos, look } => (anchor + Vec3::from(pos), anchor + Vec3::from(look)),
        CamPoint::PlayerEye => {
            let p = player_pos + Vec3::Y * eye;
            let dir = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::NEG_Z;
            (p, p + dir)
        }
    }
}

/// Picks the active shot for time `t` and returns interpolated (pos, look, fov).
pub fn sample_shots(shots: &[CameraShot], t: f32, player: (Vec3, f32, f32, f32)) -> Option<(Vec3, Vec3, f32)> {
    sample_shots_anchored(shots, t, player, Vec3::ZERO)
}

pub fn sample_shots_anchored(shots: &[CameraShot], t: f32, player: (Vec3, f32, f32, f32), anchor: Vec3) -> Option<(Vec3, Vec3, f32)> {
    let shot = shots.iter().rev().find(|s| t >= s.start).or(shots.first())?;
    let k = ease(shot.ease, (t - shot.start) / (shot.end - shot.start).max(0.001));
    let (p0, l0) = resolve_anchored(shot.from, player.0, player.1, player.2, player.3, anchor);
    let (p1, l1) = resolve_anchored(shot.to, player.0, player.1, player.2, player.3, anchor);
    Some((p0.lerp(p1, k), l0.lerp(l1, k), shot.fov))
}

fn drive_camera(
    active: Res<ActiveCutscene>,
    player: Query<(&Player, &Transform), Without<PlayerCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<PlayerCamera>>,
    shake: Res<CameraShake>,
    settings: Res<Settings>,
    time: Res<Time>,
    anchor: Res<crate::ending::CutsceneAnchor>,
) {
    let Ok((p, body)) = player.single() else { return };
    let Ok((mut tf, mut proj)) = cam.single_mut() else { return };
    let Projection::Perspective(persp) = proj.as_mut() else { return };
    let Some(def) = active.def.as_ref() else {
        let normal = 72f32.to_radians();
        if (persp.fov - normal).abs() > 1e-4 {
            persp.fov = normal;
        }
        return;
    };
    let Some((pos, look, fov)) = sample_shots_anchored(&def.shots, active.t, (body.translation, p.yaw, p.pitch, p.eye), anchor.0) else { return };
    // Camera is a child of the player body: convert world → local.
    let world = Transform::from_translation(pos).looking_at(look, Vec3::Y);
    let inv = body.compute_affine().inverse();
    let local_pos = inv.transform_point3(world.translation);
    let local_rot = body.rotation.inverse() * world.rotation;
    let t = time.elapsed_secs();
    let strength = shake.trauma * shake.trauma * settings.spook();
    tf.translation = local_pos + Vec3::new((t * 37.0).sin(), (t * 43.0).cos(), 0.0) * 0.05 * strength;
    tf.rotation = local_rot;
    persp.fov = fov.to_radians();
}

fn spawn_cutscene_ui(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
            GlobalZIndex(30),
            Pickable::IGNORE,
            crate::theme::PassThrough,
            CutsceneUi,
            DespawnOnExit(AppState::Playing),
            Name::new("Cutscene UI"),
        ))
        .with_children(|root| {
            for top in [true, false] {
                let mut node = Node { position_type: PositionType::Absolute, width: percent(100), height: percent(0), ..default() };
                if top {
                    node.top = px(0);
                } else {
                    node.bottom = px(0);
                }
                root.spawn((node, BackgroundColor(Color::BLACK), LetterboxBar));
            }
            root.spawn((
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
                BackgroundColor(Color::BLACK.with_alpha(0.0)),
                FadeOverlay,
            ));
            root.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            })
            .with_children(|c| {
                c.spawn((Text::new(""), theme.font(true, 72.0), TextColor(theme.text.with_alpha(0.0)), TitleText));
                c.spawn((Text::new(""), theme.font(false, theme.size_body), TextColor(theme.text_dim.with_alpha(0.0)), TitleSub));
            });
            root.spawn((
                Node { position_type: PositionType::Absolute, bottom: px(26), width: percent(100), justify_content: JustifyContent::Center, ..default() },
            ))
            .with_child((Text::new(""), theme.font(false, theme.size_body), TextColor(theme.text), TextLayout { justify: Justify::Center, ..default() }, CutsceneSubtitle));
            root.spawn((
                Node { position_type: PositionType::Absolute, top: px(22), right: px(28), ..default() },
            ))
            .with_child((Text::new(""), theme.font(false, theme.size_small), TextColor(theme.text_dim), SkipHint));
        });
}

#[derive(Default)]
struct SubLine {
    text: String,
    danger: bool,
    left: f32,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_ui(
    time: Res<Time>,
    theme: Res<UiTheme>,
    settings: Res<Settings>,
    active: Res<ActiveCutscene>,
    mut sub_reader: MessageReader<Subtitle>,
    mut line: Local<SubLine>,
    mut bars: Query<&mut Node, With<LetterboxBar>>,
    mut fade: Query<&mut BackgroundColor, With<FadeOverlay>>,
    mut texts: ParamSet<(
        Query<(&mut Text, &mut TextColor), With<TitleText>>,
        Query<(&mut Text, &mut TextColor), With<TitleSub>>,
        Query<(&mut Text, &mut TextColor), With<CutsceneSubtitle>>,
        Query<&mut Text, With<SkipHint>>,
    )>,
    mut bar_level: Local<f32>,
) {
    let dt = time.delta_secs();
    let playing = active.def.as_ref();
    let want = if playing.is_some_and(|d| d.letterbox) { 1.0 } else { 0.0 };
    *bar_level += (want - *bar_level) * (dt * 4.0).min(1.0);
    for mut n in &mut bars {
        n.height = percent(11.0 * *bar_level);
    }
    if let Ok(mut bg) = fade.single_mut() {
        let base = if active.fade.white { Color::WHITE } else { Color::BLACK };
        bg.0 = base.with_alpha(active.fade.alpha.clamp(0.0, 1.0));
    }
    // Title card with fade in/out.
    let (title, sub, alpha) = match &active.title {
        Some((t, s, el, total)) => (t.clone(), s.clone(), (el / 0.8).min(1.0).min(((total - el) / 0.8).max(0.0))),
        None => (String::new(), String::new(), 0.0),
    };
    if let Ok((mut t, mut c)) = texts.p0().single_mut() {
        if t.0 != title {
            t.0 = title;
        }
        c.0 = theme.text.with_alpha(alpha);
    }
    if let Ok((mut t, mut c)) = texts.p1().single_mut() {
        if t.0 != sub {
            t.0 = sub;
        }
        c.0 = theme.text_dim.with_alpha(alpha);
    }
    // Subtitles inside the letterbox.
    for s in sub_reader.read() {
        line.text = s.text.clone();
        line.danger = s.danger;
        line.left = s.seconds;
    }
    line.left -= dt;
    let show = playing.is_some() && settings.subtitles && line.left > 0.0;
    if let Ok((mut t, mut c)) = texts.p2().single_mut() {
        let s = if show { line.text.clone() } else { String::new() };
        if t.0 != s {
            t.0 = s;
        }
        c.0 = if line.danger { theme.danger } else { theme.text };
    }
    if let Ok(mut t) = texts.p3().single_mut() {
        let s = if playing.is_some_and(|d| d.skippable) { "Hold [Space] to skip".to_string() } else { String::new() };
        if t.0 != s {
            t.0 = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutscenes_adapt_to_the_number_of_players() {
        assert_eq!(party_text("behind {party}", 1), "behind you");
        assert_eq!(party_text("behind {party}", 3), "behind the three of you");
        assert_eq!(party_text("{N} OF YOU ({n})", 5), "FIVE OF YOU (5)");
        for id in [INTRO, ENDING] {
            let solo = load_def(id).unwrap().for_players(1);
            let group = load_def(id).unwrap().for_players(4);
            let subs = |d: &CutsceneDef| d.events.iter().filter_map(|e| if let CueAction::Subtitle { text, .. } = &e.action { Some(text.clone()) } else { None }).collect::<Vec<_>>();
            assert_ne!(subs(&solo), subs(&group), "{id}: solo and group play the same");
            assert!(subs(&group).iter().all(|t| !t.contains('{')), "{id}: unfilled placeholder");
            // Every moment still has a camera shot.
            for d in [&solo, &group] {
                for w in d.shots.windows(2) {
                    assert!(w[1].start <= w[0].end + 1e-3, "{id}: gap in shots");
                }
            }
        }
    }

    #[test]
    fn bundled_cutscenes_parse() {
        for id in [INTRO, THE_HOLLOW, ENDING, CUSTOM_START, RETRY] {
            let def = parse(embedded(id).unwrap()).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(def.duration > 0.0);
            assert!(!def.shots.is_empty());
            for s in &def.shots {
                assert!(s.end > s.start, "{id}: shot ends before it starts");
                assert!(s.end <= def.duration + 0.01, "{id}: shot past end");
            }
            for w in def.events.windows(2) {
                assert!(w[0].at <= w[1].at, "{id}: events must be in time order");
            }
        }
    }

    #[test]
    fn shot_sampling_interpolates() {
        let shots = vec![CameraShot {
            start: 0.0,
            end: 2.0,
            from: CamPoint::At { pos: (0.0, 0.0, 0.0), look: (0.0, 0.0, -1.0) },
            to: CamPoint::At { pos: (2.0, 0.0, 0.0), look: (2.0, 0.0, -1.0) },
            fov: 50.0,
            ease: Ease::Linear,
            players: None,
        }];
        let (p, _, fov) = sample_shots(&shots, 1.0, (Vec3::ZERO, 0.0, 0.0, 1.6)).unwrap();
        assert!((p.x - 1.0).abs() < 1e-5);
        assert_eq!(fov, 50.0);
    }

    #[test]
    fn player_eye_point_faces_forward() {
        let (p, l) = resolve(CamPoint::PlayerEye, Vec3::new(1.0, 0.0, 2.0), 0.0, 0.0, 1.6);
        assert_eq!(p, Vec3::new(1.0, 1.6, 2.0));
        assert!((l - p - Vec3::NEG_Z).length() < 1e-5);
    }
}
