//! The guiding light: a pale wisp that helps with key hunts.
//!
//! In a locked room (a key puzzle or the Reading Hall), if the key still
//! hasn't been found after a while, a small cold light gathers in front of
//! the player and drifts to the drawer that hides it, then hovers there,
//! pulsing, until the key is taken or the player leaves the room.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::floor::plan::{Feature, RoomKind};
use crate::floor::{Floor, FloorRuntime};
use crate::interaction::{Drawer, Interactable};
use crate::player::PlayerCamera;
use crate::run::{RunState, Subtitle};
use crate::state::{gameplay_active, AppState};

/// Seconds in a locked room before the light comes to help.
pub const GUIDE_AFTER: f32 = 35.0;
/// The Reading Hall is meant to be searched: the light waits longer.
pub const GUIDE_AFTER_HALL: f32 = 55.0;
const SPEED: f32 = 1.7;

#[derive(Component)]
struct Wisp {
    target: Vec3,
    mote: f32,
}
#[derive(Component)]
struct WispMote(f32);

/// Overrides the wait before the light appears (used by the trailer).
#[derive(Resource, Default, Debug)]
pub struct GuideOverride(pub Option<f32>);

#[derive(Resource, Default)]
struct GuideState {
    room: u32,
    t: f32,
    wisp: Option<Entity>,
}

pub struct GuidePlugin;

impl Plugin for GuidePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuideState>()
            .init_resource::<GuideOverride>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands| c.insert_resource(GuideState::default()))
            .add_systems(Update, (watch.run_if(gameplay_active), fly).chain().run_if(in_state(AppState::Playing)));
    }
}

/// How long to wait in this room before helping (None: no key to find).
pub fn wait_for(kind: RoomKind, feature: Option<Feature>) -> Option<f32> {
    match (kind, feature) {
        (RoomKind::KeyPuzzle, Some(Feature::ReadingHall)) => Some(GUIDE_AFTER_HALL),
        (RoomKind::KeyPuzzle, _) => Some(GUIDE_AFTER),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn watch(
    mut commands: Commands,
    time: Res<Time>,
    run: Res<RunState>,
    floor: Option<Res<Floor>>,
    rt: Option<Res<FloorRuntime>>,
    mut st: ResMut<GuideState>,
    over: Res<GuideOverride>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    drawers: Query<(&Drawer, &Interactable)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut subs: MessageWriter<Subtitle>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let (Some(floor), Some(rt)) = (floor, rt) else { return };
    let room = run.current_room;
    let solved = rt.keys.contains(&room);
    if room != st.room || solved {
        if let Some(e) = st.wisp.take() {
            commands.entity(e).try_despawn();
        }
        if room != st.room {
            st.room = room;
            st.t = 0.0;
        }
        if solved {
            return;
        }
    }
    let Some(r) = floor.0.room(room) else { return };
    let Some(wait) = wait_for(r.kind, r.feature) else { return };
    // QA: HALLWAYS_GUIDE_SECS shortens the wait.
    let wait = over.0.or_else(|| std::env::var("HALLWAYS_GUIDE_SECS").ok().and_then(|v| v.parse().ok())).unwrap_or(wait);
    st.t += time.delta_secs();
    if st.wisp.is_some() || st.t < wait {
        return;
    }
    let Some(target) = drawers.iter().find(|(d, _)| d.has_key && d.tag.is_some_and(|t| t.0 == room)).map(|(_, i)| (i.aabb.min + i.aabb.max) / 2.0) else { return };
    let Ok(cam) = cam.single() else { return };
    let start = cam.translation() + cam.forward().as_vec3() * 1.2;
    let body = meshes.add(Sphere::new(0.07).mesh().ico(2).unwrap_or_else(|_| Sphere::new(0.07).mesh().build()));
    let glow = mats.add(StandardMaterial { base_color: Color::srgb(0.8, 0.95, 1.0), emissive: LinearRgba::rgb(6.0, 9.0, 12.0), unlit: true, ..default() });
    let mote_mesh = meshes.add(Sphere::new(0.02).mesh().ico(1).unwrap_or_else(|_| Sphere::new(0.02).mesh().build()));
    let e = commands
        .spawn((
            Name::new("Guiding light"),
            Transform::from_translation(start),
            Visibility::default(),
            DespawnOnExit(AppState::Playing),
            Wisp { target: target + Vec3::Y * 0.55, mote: 0.0 },
        ))
        .with_children(|c| {
            c.spawn((Mesh3d(body), MeshMaterial3d(glow.clone())));
            c.spawn(PointLight { color: Color::srgb(0.7, 0.88, 1.0), intensity: 30_000.0, range: 6.0, shadow_maps_enabled: false, ..default() });
            for k in 0..3 {
                c.spawn((Mesh3d(mote_mesh.clone()), MeshMaterial3d(glow.clone()), Transform::default(), WispMote(k as f32 * std::f32::consts::TAU / 3.0)));
            }
        })
        .id();
    st.wisp = Some(e);
    sfx.write(PlaySfx(Sfx::Ding));
    subs.write(Subtitle::info("[A pale light gathers in front of you. It drifts off... follow it to the key.]"));
}

fn fly(time: Res<Time>, mut wisps: Query<(&mut Wisp, &mut Transform, &Children)>, mut motes: Query<(&WispMote, &mut Transform), Without<Wisp>>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut w, mut tf, kids) in &mut wisps {
        let to = w.target - tf.translation;
        let d = to.length();
        if d > 0.05 {
            tf.translation += to / d * (SPEED * dt).min(d);
        }
        // Hover and bob; a little sideways drift so it feels alive.
        let bob = Vec3::new((t * 1.7).sin() * 0.003, (t * 2.3).sin() * 0.004, 0.0);
        tf.translation += bob;
        w.mote += dt * 3.0;
        for k in kids.iter() {
            if let Ok((m, mut mt)) = motes.get_mut(k) {
                let a = w.mote + m.0;
                mt.translation = Vec3::new(a.cos() * 0.16, (a * 1.3).sin() * 0.06, a.sin() * 0.16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_locked_rooms_get_a_guide() {
        assert_eq!(wait_for(RoomKind::KeyPuzzle, None), Some(GUIDE_AFTER));
        assert_eq!(wait_for(RoomKind::KeyPuzzle, Some(Feature::ReadingHall)), Some(GUIDE_AFTER_HALL));
        assert_eq!(wait_for(RoomKind::Loot, None), None);
        assert!(GUIDE_AFTER_HALL > GUIDE_AFTER);
    }
}
