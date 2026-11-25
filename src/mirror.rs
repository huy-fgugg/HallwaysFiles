//! Working mirrors. A mirror is a pane of glass showing what a camera sees
//! from the viewer's eye reflected through the glass. The camera looks
//! straight out of the glass with an off-axis frustum whose near plane *is*
//! the glass, so the reflection lines up exactly from any angle and nothing
//! behind the wall gets in. Only the nearest mirror in front of you renders.
//!
//! Your own character (normally not drawn in first person) is drawn for
//! mirrors only, on [`MIRROR_LAYER`], moving and crouching as you do.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraProjection, RenderTarget, SubCameraView};
use bevy::math::{Affine2, Vec3A};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::floor::plan::{Area, RoomPlan, DOOR_W};
use crate::rng::GameRng;
use crate::state::AppState;

/// Render layer of things only mirrors see (your own body).
pub const MIRROR_LAYER: usize = 4;
pub const GLASS_W: f32 = 0.9;
pub const GLASS_H: f32 = 1.7;
const VIEW_RANGE: f32 = 9.0;

/// A mirror's glass: centre, normal (out of the glass, into the room), the
/// direction "right" along it as seen from the front, and its camera.
#[derive(Component, Debug, Clone, Copy)]
pub struct Mirror {
    pub center: Vec3,
    pub normal: Vec3,
    pub right: Vec3,
    pub camera: Entity,
}

#[derive(Component)]
struct MirrorCamera;

/// Off-axis perspective through a rectangle on the near plane (view space,
/// looking down -Z), with an infinite reversed-Z far plane like Bevy's own.
#[derive(Debug, Clone, Copy)]
pub struct WindowProjection {
    pub l: f32,
    pub r: f32,
    pub b: f32,
    pub t: f32,
    pub near: f32,
}

impl Default for WindowProjection {
    fn default() -> Self {
        Self { l: -0.1, r: 0.1, b: -0.1, t: 0.1, near: 0.1 }
    }
}

const FAR: f32 = 200.0;

impl CameraProjection for WindowProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        let (l, r, b, t, n) = (self.l, self.r, self.b, self.t, self.near);
        Mat4::from_cols(
            Vec4::new(2.0 * n / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 * n / (t - b), 0.0, 0.0),
            Vec4::new((r + l) / (r - l), (t + b) / (t - b), 0.0, -1.0),
            Vec4::new(0.0, 0.0, n, 0.0),
        )
    }

    fn get_clip_from_view_for_sub(&self, _sub: &SubCameraView) -> Mat4 {
        self.get_clip_from_view()
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        FAR
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        let at = |z: f32| {
            let s = z.abs() / self.near;
            [Vec3A::new(self.r * s, self.b * s, z), Vec3A::new(self.r * s, self.t * s, z), Vec3A::new(self.l * s, self.t * s, z), Vec3A::new(self.l * s, self.b * s, z)]
        };
        let (a, f) = (at(z_near), at(z_far));
        [a[0], a[1], a[2], a[3], f[0], f[1], f[2], f[3]]
    }
}

/// Pure maths: the reflected camera for an eye in front of a mirror —
/// its transform and the window projection through the glass. None when the
/// eye is behind the glass or right against it.
pub fn reflect_view(eye: Vec3, m: &Mirror, half: Vec2) -> Option<(Transform, WindowProjection)> {
    let d = (eye - m.center).dot(m.normal);
    if d < 0.12 {
        return None;
    }
    let cam_pos = eye - 2.0 * d * m.normal;
    let tf = Transform::from_translation(cam_pos).looking_to(m.normal, Vec3::Y);
    let inv = tf.compute_affine().inverse();
    let up = m.normal.cross(m.right).normalize() * -1.0;
    let corners = [m.center - m.right * half.x - up * half.y, m.center + m.right * half.x + up * half.y];
    let (a, c) = (inv.transform_point3(corners[0]), inv.transform_point3(corners[1]));
    // A hair past the glass so the glass itself (and the wall) is clipped.
    let near = d + 0.01;
    let k = near / d;
    let (l, r) = (a.x.min(c.x) * k, a.x.max(c.x) * k);
    let (b, t) = (a.y.min(c.y) * k, a.y.max(c.y) * k);
    Some((tf, WindowProjection { l, r, b, t, near }))
}

/// Whether a room gets a mirror, and where on its south wall (glass centre x).
pub fn mirror_spot(r: &RoomPlan) -> Option<f32> {
    if !matches!(r.area, Area::GuestWing | Area::Archive | Area::Atrium) || r.pursuit || r.feature.is_some() || r.entry_height > 0.0 || r.kind == crate::floor::plan::RoomKind::Final {
        return None;
    }
    if GameRng::fork(r.seed, 0x3170).next_f32() > 0.3 {
        return None;
    }
    // Beside the entry door on its wider side, clear of the side-wall
    // furniture and the door casing.
    let left = (r.min.x + 0.9, r.entry_x - DOOR_W / 2.0 - 0.35);
    let right = (r.entry_x + DOOR_W / 2.0 + 0.35, r.max.x - 0.9);
    let (lo, hi) = if left.1 - left.0 >= right.1 - right.0 { left } else { right };
    (hi - lo >= GLASS_W + 0.2).then_some((lo + hi) / 2.0)
}

/// Builds the mirror on a room's south wall (called by the room builder).
pub fn spawn_mirror(b: &mut crate::world::Builder, r: &RoomPlan, x: f32, images: &mut Assets<Image>, std_mats: &mut Assets<StandardMaterial>) {
    spawn_mirror_at(b, x, r.max.y, images, std_mats);
}

/// Where the lobby's mirror hangs: the boarding hall's back wall, left of
/// the doorway (the skin table is on the right).
pub const LOBBY_MIRROR_X: f32 = -2.3;

/// A mirror on a wall facing south (-Z) at `wall`, glass centred on `x`.
pub fn spawn_mirror_at(b: &mut crate::world::Builder, x: f32, wall: f32, images: &mut Assets<Image>, std_mats: &mut Assets<StandardMaterial>) {
    let cy = 0.35 + GLASS_H / 2.0;
    // A carved wooden frame.
    let frame = b.mats.trim_wood.clone();
    b.block(Vec3::new(x - GLASS_W / 2.0 - 0.09, cy - GLASS_H / 2.0 - 0.09, wall - 0.05), Vec3::new(x + GLASS_W / 2.0 + 0.09, cy + GLASS_H / 2.0 + 0.09, wall), &frame, 0.8, false);
    let image = images.add(Image::new_target_texture(384, 720, TextureFormat::Bgra8UnormSrgb, None));
    let camera = b
        .commands
        .spawn((
            Camera3d::default(),
            Camera { order: -3, is_active: false, ..default() },
            RenderTarget::Image(image.clone().into()),
            Projection::custom(WindowProjection::default()),
            Transform::default(),
            RenderLayers::from_layers(&[0, MIRROR_LAYER]),
            MirrorCamera,
            Name::new("Mirror camera"),
        ))
        .id();
    b.tag(camera);
    // The glass shows the camera's picture, flipped left to right like a mirror.
    let glass = std_mats.add(StandardMaterial {
        base_color: Color::srgb(0.92, 0.95, 0.97),
        base_color_texture: Some(image),
        unlit: true,
        uv_transform: Affine2::from_scale_angle_translation(Vec2::new(-1.0, 1.0), 0.0, Vec2::new(1.0, 0.0)),
        ..default()
    });
    let mesh = b.meshes.add(Rectangle::new(GLASS_W, GLASS_H));
    let center = Vec3::new(x, cy, wall - 0.052);
    let e = b
        .commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(glass),
            Transform::from_translation(center).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Mirror { center, normal: Vec3::NEG_Z, right: Vec3::X, camera },
            Name::new("Mirror"),
        ))
        .id();
    b.tag(e);
}

/// Your character, drawn for mirrors only.
#[derive(Component)]
struct MirrorDouble(crate::body::Skin);

pub struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (drive_mirrors.after(crate::player::CameraSet), (spawn_double, move_double, tag_double).chain()).run_if(in_state(AppState::Playing)));
    }
}

fn drive_mirrors(
    eye: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    mirrors: Query<&Mirror>,
    mut cams: Query<(Entity, &mut Camera, &mut Transform, &mut Projection), With<MirrorCamera>>,
) {
    let Ok(eye) = eye.single() else { return };
    let eye = eye.translation();
    let half = Vec2::new(GLASS_W / 2.0, GLASS_H / 2.0);
    // The nearest mirror you're in front of, within range.
    let best = mirrors
        .iter()
        .filter(|m| (eye - m.center).dot(m.normal) > 0.12 && eye.distance(m.center) < VIEW_RANGE)
        .min_by(|a, b| eye.distance(a.center).total_cmp(&eye.distance(b.center)))
        .copied();
    for (e, mut cam, mut tf, mut proj) in &mut cams {
        let view = best.filter(|m| m.camera == e).and_then(|m| reflect_view(eye, &m, half));
        match view {
            Some((t, w)) => {
                if !cam.is_active {
                    cam.is_active = true;
                }
                *tf = t;
                *proj = Projection::custom(w);
            }
            None => {
                if cam.is_active {
                    cam.is_active = false;
                }
            }
        }
    }
}

fn spawn_double(mut commands: Commands, model: Option<Res<crate::body::PlayerModel>>, profile: Res<crate::profile::Profile>, q: Query<(Entity, &MirrorDouble)>) {
    let Some(model) = model else { return };
    if let Some((e, d)) = q.iter().next() {
        if d.0 != profile.skin {
            commands.entity(e).try_despawn();
        }
        return;
    }
    commands
        .spawn((
            MirrorDouble(profile.skin),
            crate::body::BodyPose::default(),
            Transform::default(),
            Visibility::default(),
            RenderLayers::layer(MIRROR_LAYER),
            DespawnOnExit(AppState::Playing),
            Name::new("Mirror double"),
        ))
        .with_children(|root| crate::body::spawn_body(root, &model, profile.skin, false));
}

/// Closest the double's feet may be to a mirror's glass: the big chibi head
/// (tilted forward when you look down) must stay in front of the glass, or
/// the part behind it is cut off and you see inside the head.
pub const DOUBLE_CLEARANCE: f32 = 0.62;

/// Where the double stands: where you are, pushed back from any mirror
/// you're pressed against (imperceptible in the reflection at that range).
pub fn double_position(feet: Vec3, mirrors: impl Iterator<Item = Mirror>) -> Vec3 {
    let mut at = feet;
    for m in mirrors {
        let d = (at - m.center).dot(m.normal);
        let side = (at - m.center).dot(m.right).abs();
        if d > -0.1 && d < DOUBLE_CLEARANCE && side < GLASS_W / 2.0 + 0.7 {
            at += m.normal * (DOUBLE_CLEARANCE - d);
        }
    }
    at
}

fn move_double(
    player: Query<(&crate::player::Player, &Transform), Without<MirrorDouble>>,
    hide: Res<crate::interaction::HideState>,
    inv: Res<crate::items::Inventory>,
    mirrors: Query<&Mirror>,
    mut q: Query<(&mut Transform, &mut crate::body::BodyPose, &mut Visibility), With<MirrorDouble>>,
) {
    let Ok((p, body)) = player.single() else { return };
    let (right, left) = crate::viewmodel::held_items(&inv);
    for (mut tf, mut pose, mut vis) in &mut q {
        tf.translation = double_position(body.translation, mirrors.iter().copied());
        tf.rotation = Quat::from_rotation_y(p.yaw);
        let crouch = ((crate::player::STAND_EYE - p.eye) / (crate::player::STAND_EYE - crate::player::CROUCH_EYE)).clamp(0.0, 1.0);
        *pose = crate::avatar::pose_for(p.speed, p.bob_phase, crouch, p.pitch, right.is_some(), left.is_some(), crate::viewmodel::Gesture::None, 0.0);
        let want = if hide.is_hiding() { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The double's parts load in later: put each on the mirror layer.
fn tag_double(mut commands: Commands, roots: Query<Entity, With<MirrorDouble>>, children: Query<&Children>, tagged: Query<(), With<RenderLayers>>) {
    for root in &roots {
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if let Ok(k) = children.get(e) {
                stack.extend(k.iter());
            }
            if tagged.get(e).is_err() {
                commands.entity(e).try_insert(RenderLayers::layer(MIRROR_LAYER));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mirror() -> Mirror {
        Mirror { center: Vec3::new(0.0, 1.2, 0.0), normal: Vec3::NEG_Z, right: Vec3::X, camera: Entity::PLACEHOLDER }
    }

    #[test]
    fn reflection_sits_behind_the_glass_and_frames_it_exactly() {
        let half = Vec2::new(0.45, 0.85);
        let eye = Vec3::new(0.3, 1.6, -2.0);
        let (tf, w) = reflect_view(eye, &mirror(), half).unwrap();
        assert!((tf.translation - Vec3::new(0.3, 1.6, 2.0)).length() < 1e-4, "{:?}", tf.translation);
        // Looking out of the glass into the room.
        assert!((tf.forward().as_vec3() - Vec3::NEG_Z).length() < 1e-4);
        assert!((w.near - 2.01).abs() < 1e-4);
        // The glass corners land exactly on the clip-space edges.
        let clip = w.get_clip_from_view() * Mat4::from(tf.compute_affine().inverse());
        for (corner, want) in [(Vec3::new(-0.45, 0.35, 0.0), Vec2::new(-1.0, -1.0)), (Vec3::new(0.45, 2.05, 0.0), Vec2::new(1.0, 1.0))] {
            let c = clip * corner.extend(1.0);
            let ndc = Vec2::new(c.x / c.w, c.y / c.w);
            // The camera faces back at you, so its left is your right.
            assert!((ndc.x.abs() - 1.0).abs() < 0.02 && (ndc.y - want.y).abs() < 0.02, "{corner:?} -> {ndc:?}");
        }
        // A point in the room in front of the glass is in front of the near plane (visible).
        let p = clip * Vec4::new(0.0, 1.2, -1.0, 1.0);
        assert!(p.w > 0.0 && p.z / p.w <= 1.0 && p.z / p.w >= 0.0);
        // Something behind the glass (inside the wall) is clipped away.
        let q = clip * Vec4::new(0.0, 1.2, 0.1, 1.0);
        assert!(q.z / q.w > 1.0, "behind the glass is clipped");
        // Behind the mirror: nothing to show.
        assert!(reflect_view(Vec3::new(0.0, 1.6, 1.0), &mirror(), half).is_none());
    }

    #[test]
    fn the_double_never_reaches_past_the_glass() {
        let m = mirror();
        // Pressed flat against the glass (player radius 0.3 from the wall).
        let at = double_position(Vec3::new(0.0, 0.0, -0.3), [m].into_iter());
        assert!(((at - m.center).dot(m.normal) - DOUBLE_CLEARANCE).abs() < 1e-4);
        // Further away: untouched.
        let far = Vec3::new(0.2, 0.0, -2.0);
        assert_eq!(double_position(far, [m].into_iter()), far);
        // Beside the mirror (well to the side): untouched.
        let side = Vec3::new(3.0, 0.0, -0.3);
        assert_eq!(double_position(side, [m].into_iter()), side);
    }

    #[test]
    fn some_rooms_get_a_mirror_beside_the_door() {
        let mut n = 0;
        for seed in 0..30 {
            let plan = crate::floor::plan::generate(seed);
            for r in &plan.rooms {
                if let Some(x) = mirror_spot(r) {
                    n += 1;
                    assert!(x - GLASS_W / 2.0 >= r.min.x + 0.9 - 1e-3 && x + GLASS_W / 2.0 <= r.max.x - 0.9 + 1e-3, "inside the wall");
                    assert!((x - r.entry_x).abs() >= DOOR_W / 2.0 + 0.35 + GLASS_W / 2.0 - 1e-3, "clear of the door");
                }
            }
        }
        assert!(n > 40, "{n} mirrors");
    }
}

#[cfg(test)]
mod find {
    /// `cargo test mirror_rooms -- --nocapture` lists mirror rooms for HALLWAYS_SEED (QA helper).
    #[test]
    fn mirror_rooms() {
        let Some(seed) = std::env::var("HALLWAYS_SEED").ok().and_then(|v| u64::from_str_radix(&v, 16).ok()) else { return };
        for r in &crate::floor::plan::generate(seed).rooms {
            if let Some(x) = super::mirror_spot(r) {
                println!("mirror room {} x {x:.1} entry_x {:.1} z {:.1}", r.index, r.entry_x, r.max.y);
            }
        }
    }

    #[test]
    fn lobby_mirror_fits_its_wall() {
        use crate::world::elevator::HALL_MIN;
        use crate::mirror::{GLASS_W, LOBBY_MIRROR_X};
        let (l, r) = (LOBBY_MIRROR_X - GLASS_W / 2.0 - 0.09, LOBBY_MIRROR_X + GLASS_W / 2.0 + 0.09);
        // Clear of the corner (and the bench on the west wall) and of the doorway (|x| < 1).
        assert!(l > HALL_MIN.x + 0.65 && r < -1.0 - 0.2, "{l}..{r}");
    }

}
