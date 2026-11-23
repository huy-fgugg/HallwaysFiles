//! Customise your character: a live 3D preview you can drag to turn (mouse or
//! touch), and arrows to pick the character, hat and back piece. Opened from
//! the main menu's Customise button or the Skin table in the boarding hall.
//! The main menu also shows the preview, floating beside the buttons.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use super::focus::{spawn_button, MenuAction, MenuActivated};
use crate::body::{idle_pose, spawn_body, BodyPose, PlayerModel, SkinPart};
use crate::profile::{save_profile, Profile};
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

/// Render layer of the preview stage (its own camera and lights).
pub const PREVIEW_LAYER: usize = 2;
/// The preview stage stands far below everything else.
const STAGE: Vec3 = Vec3::new(0.0, -400.0, 0.0);

pub struct SkinMenuPlugin;

impl Plugin for SkinMenuPlugin {
    fn build(&self, app: &mut App) {
        // The image must exist before the first screen (the main menu) spawns,
        // which happens before any startup system runs.
        let handle = app.world_mut().get_resource_mut::<Assets<Image>>().map(|mut images| images.add(preview_image()));
        if let Some(h) = handle {
            app.insert_resource(PreviewImage(h));
        }
        app.init_resource::<PreviewSpin>()
            .add_systems(PreStartup, setup_stage)
            .add_systems(OnEnter(Screen::Skin), spawn_screen)
            .add_systems(Update, (skin_actions.run_if(in_state(Screen::Skin)), open_from_main_menu.run_if(in_state(Screen::MainMenu)), update_values, run_stage, tag_preview_layers));
    }
}

/// The image the preview camera draws into.
#[derive(Resource)]
pub struct PreviewImage(pub Handle<Image>);

/// The preview figure's turn (radians) and how long since it was dragged.
#[derive(Resource, Default)]
pub struct PreviewSpin {
    pub yaw: f32,
    pub idle: f32,
    pub t: f32,
}

#[derive(Component)]
struct PreviewCamera;

/// The figure on the preview stage.
#[derive(Component)]
struct PreviewDoll;

/// A UI node showing the preview (draggable).
#[derive(Component)]
pub struct PreviewView;

#[derive(Component)]
struct SkinValue(SkinPart);

fn preview_image() -> Image {
    Image::new_target_texture(560, 720, TextureFormat::Bgra8UnormSrgb, None)
}

fn setup_stage(mut commands: Commands, mut images: ResMut<Assets<Image>>, existing: Option<Res<PreviewImage>>) {
    let handle = match existing {
        Some(p) => p.0.clone(),
        None => {
            let h = images.add(preview_image());
            commands.insert_resource(PreviewImage(h.clone()));
            h
        }
    };
    commands.spawn((
        Camera3d::default(),
        Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: false, ..default() },
        RenderTarget::Image(handle.into()),
        Projection::from(PerspectiveProjection { fov: 30f32.to_radians(), ..default() }),
        Transform::from_translation(STAGE + Vec3::new(0.0, 1.0, -4.4)).looking_at(STAGE + Vec3::new(0.0, 0.82, 0.0), Vec3::Y),
        RenderLayers::layer(PREVIEW_LAYER),
        PreviewCamera,
        Name::new("Character preview camera"),
    ));
    // Warm key light from the front-left and a cool rim from behind.
    for (pos, color, intensity) in [(Vec3::new(-1.6, 2.4, -2.6), Color::srgb(1.0, 0.86, 0.7), 260_000.0), (Vec3::new(1.4, 2.0, 2.2), Color::srgb(0.55, 0.65, 1.0), 160_000.0)] {
        commands.spawn((PointLight { color, intensity, range: 12.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(STAGE + pos), RenderLayers::layer(PREVIEW_LAYER)));
    }
}

/// Keeps the doll in step with your look, turns it, and runs the camera only
/// while a preview is on screen.
#[allow(clippy::too_many_arguments)]
fn run_stage(
    mut commands: Commands,
    time: Res<Time<Real>>,
    profile: Res<Profile>,
    model: Option<Res<PlayerModel>>,
    mut spin: ResMut<PreviewSpin>,
    views: Query<&InheritedVisibility, With<PreviewView>>,
    mut cams: Query<&mut Camera, With<PreviewCamera>>,
    mut dolls: Query<(Entity, &mut Transform, &mut BodyPose), With<PreviewDoll>>,
    mut shown: Local<Option<crate::body::Skin>>,
) {
    let active = views.iter().any(|v| v.get());
    for mut c in &mut cams {
        if c.is_active != active {
            c.is_active = active;
        }
    }
    let Some(model) = model else { return };
    let dt = time.delta_secs();
    spin.t += dt;
    spin.idle += dt;
    // Turns slowly on its own unless you've just dragged it.
    if spin.idle > 2.5 {
        spin.yaw += dt * 0.5;
    }
    if dolls.is_empty() {
        commands
            .spawn((Transform::from_translation(STAGE), Visibility::default(), BodyPose::default(), PreviewDoll, Name::new("Preview doll")))
            .with_children(|c| spawn_body(c, &model, profile.skin, false));
        *shown = Some(profile.skin);
        return;
    }
    for (e, mut tf, mut pose) in &mut dolls {
        // Floats gently, facing the camera (bodies face -Z; the camera looks +Z).
        tf.translation = STAGE + Vec3::Y * (0.05 + (spin.t * 1.4).sin() * 0.04);
        tf.rotation = Quat::from_rotation_y(std::f32::consts::PI + spin.yaw);
        *pose = idle_pose(spin.t);
        if *shown != Some(profile.skin) {
            crate::body::reskin(&mut commands, e, profile.skin);
            *shown = Some(profile.skin);
        }
    }
}

/// Puts everything on the preview stage on its own render layer (the body's
/// parts load in after it's spawned, so this runs every frame).
fn tag_preview_layers(mut commands: Commands, roots: Query<Entity, With<PreviewDoll>>, children: Query<&Children>, tagged: Query<(), With<RenderLayers>>) {
    for root in &roots {
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if !tagged.contains(e) {
                commands.entity(e).try_insert(RenderLayers::layer(PREVIEW_LAYER));
            }
            if let Ok(ch) = children.get(e) {
                stack.extend(ch.iter());
            }
        }
    }
}

/// A draggable preview of your character, `w` x `h` pixels.
pub fn spawn_preview(parent: &mut ChildSpawnerCommands, image: &PreviewImage, w: f32, h: f32) -> Entity {
    parent
        .spawn((ImageNode::new(image.0.clone()), Node { width: px(w), height: px(h), ..default() }, PreviewView, Name::new("Character preview")))
        .observe(|drag: On<PointerDrag>, mut spin: ResMut<PreviewSpin>| {
            spin.yaw += drag.delta.x * 0.012;
            spin.idle = 0.0;
        })
        .id()
}

fn spawn_screen(mut commands: Commands, theme: Res<UiTheme>, image: Res<PreviewImage>, profile: Res<Profile>) {
    commands
        .spawn((theme.screen_root(false), GlobalZIndex(25), DespawnOnExit(Screen::Skin), Name::new("Customise")))
        .with_children(|root| {
            root.spawn(theme.heading_text("CUSTOMISE"));
            root.spawn(Node { column_gap: px(28), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(4), ..default() }, BackgroundColor(theme.bg.with_alpha(0.55)))).with_children(|left| {
                    spawn_preview(left, &image, 300.0, 386.0);
                    left.spawn(theme.dim_text("Drag to turn"));
                });
                row.spawn(theme.panel(430.0)).with_children(|panel| {
                    let mut order = 0;
                    for (i, part) in SkinPart::ALL.into_iter().enumerate() {
                        panel.spawn(Node { column_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                            r.spawn((Text::new(part.label()), theme.font(true, theme.size_small), TextColor(theme.text_dim), Node { width: px(110), ..default() }));
                            spawn_button(r, &theme, "<", MenuAction::SkinStep(i as u8, -1), order, order == 0);
                            r.spawn((Text::new(profile.skin.name(part)), theme.font(true, theme.size_body), TextColor(theme.text), Node { width: px(140), justify_content: JustifyContent::Center, ..default() }, SkinValue(part)));
                            spawn_button(r, &theme, ">", MenuAction::SkinStep(i as u8, 1), order + 1, false);
                        });
                        order += 2;
                    }
                    panel.spawn(Node { height: px(10), ..default() });
                    spawn_button(panel, &theme, "Done", MenuAction::CloseSkin, order, false);
                });
            });
            root.spawn(theme.dim_text("Your look is saved, and your teammates see it online."));
        });
}

fn update_values(profile: Res<Profile>, mut q: Query<(&SkinValue, &mut Text)>) {
    if !profile.is_changed() {
        return;
    }
    for (v, mut t) in &mut q {
        let name = profile.skin.name(v.0);
        if t.0 != name {
            t.0 = name.into();
        }
    }
}

fn skin_actions(
    mut reader: MessageReader<MenuActivated>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut profile: ResMut<Profile>,
    mut next: ResMut<NextState<Overlay>>,
) {
    let back = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::East));
    let mut close = back;
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::SkinStep(part, delta) => {
                profile.skin.step(SkinPart::ALL[part as usize % SkinPart::ALL.len()], delta as i32);
                save_profile(&profile);
            }
            MenuAction::CloseSkin => close = true,
            _ => {}
        }
    }
    if close {
        next.set(Overlay::None);
    }
}

fn open_from_main_menu(mut reader: MessageReader<MenuActivated>, mut next: ResMut<NextState<Overlay>>) {
    if reader.read().any(|MenuActivated(a)| *a == MenuAction::OpenSkin) {
        next.set(Overlay::Skin);
    }
}

/// True outside of runs too: used to keep the preview stage out of the way.
pub fn in_menu(app: Res<State<AppState>>) -> bool {
    *app.get() == AppState::MainMenu
}
