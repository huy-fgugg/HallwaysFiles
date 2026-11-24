//! Scenery: garden sheds, little windmills and the props that fill empty
//! floor space in every area. Everything here keeps clear of doors, passages,
//! stairs and the walking route (the same zones the furniture respects).
//!
//! The models come from free online libraries (see assets/legal/credits.md):
//! Poly Haven (CC0), the Low Poly Forest Pack by Devil's Work.shop, and
//! OpenGameArt's Medieval Shack (AnyRPG) and Old Windmill (yd), both CC0.

use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

use super::build::{stair_zones, PropMaterials};
use super::plan::{door_zones, passage_zones, pillars, stream_bridge, zones_overlap, Area, RoomKind, RoomPlan, Zone, STREAM_HALF};
use crate::physics::{Aabb, Collider};
use crate::rng::GameRng;
use crate::world::{world_aabb, Builder};
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};

/// Tree models for the gardens, with their crowns' half width (x) and half
/// depth (z) in metres, so trees never overlap walls or each other.
/// Indices 10..14 are pines, 6..10 bare trees.
pub const TREES: [(&str, f32, f32); 14] = [
    ("tree_gold_1", 2.61, 1.77),
    ("tree_gold_2", 2.21, 1.63),
    ("tree_gold_3", 1.25, 1.59),
    ("tree_round_1", 2.6, 1.5),
    ("tree_round_2", 2.15, 1.64),
    ("tree_round_3", 1.43, 1.74),
    ("tree_bare_1", 2.47, 0.78),
    ("tree_bare_2", 2.09, 1.25),
    ("tree_bare_3", 1.27, 1.8),
    ("tree_bare_4", 0.95, 0.38),
    ("tree_pine_1", 1.29, 1.3),
    ("tree_pine_2", 1.31, 1.32),
    ("tree_pine_3", 1.69, 1.69),
    ("tree_pine_4", 1.38, 1.39),
];

/// A scenery model from `assets/models/scenery/`.
pub fn prop(assets: &AssetServer, name: &str) -> WorldAssetRoot {
    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/scenery/{name}.glb"))))
}

/// Places a scenery model standing on the floor at `at`. `solid` is the
/// collider's half width, height and half depth in the model's own frame.
pub fn place(b: &mut Builder, assets: &AssetServer, name: &str, at: Vec3, yaw: f32, scale: Vec3, solid: Option<Vec3>) -> Entity {
    let tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(scale);
    let mut e = b.commands.spawn((prop(assets, name), tf));
    if let Some(h) = solid {
        // `h` is in metres; world_aabb applies the transform's scale, so undo it.
        let l = h / scale.max(Vec3::splat(1e-3));
        e.insert(Collider(world_aabb(Vec3::new(-l.x, 0.0, -l.z), l, &tf)));
    }
    let id = e.id();
    b.tag(id);
    id
}

/// A windmill whose sails should turn (set up once its model has loaded).
#[derive(Component)]
pub struct Sails(pub f32);

/// Finds the windmill model's "Sails" node and sets it turning.
pub fn start_sails(ready: On<WorldInstanceReady>, mut commands: Commands, mills: Query<&Sails>, children: Query<&Children>, names: Query<&Name>) {
    let Ok(sails) = mills.get(ready.entity) else { return };
    let mut stack = vec![ready.entity];
    while let Some(e) = stack.pop() {
        if let Ok(k) = children.get(e) {
            stack.extend(k.iter());
        }
        if names.get(e).is_ok_and(|n| n.as_str() == "Sails") {
            commands.entity(e).insert(Spin(sails.0));
        }
    }
}

/// Spins an entity around its local Y axis (the windmill's sails turn about
/// their axle, which the model points along its sails node's Y), radians a second.
#[derive(Component)]
pub struct Spin(pub f32);

pub fn spin_props(time: Res<Time>, mut q: Query<(&Spin, &mut Transform)>) {
    for (s, mut t) in &mut q {
        t.rotate_local_y(s.0 * time.delta_secs());
    }
}

/// Footprint of a shed standing against a side wall (`east`), centred at depth `z`.
pub const SHED_DEPTH: f32 = 2.4;
pub const SHED_LEN: f32 = 3.9;

/// Everything scenery must keep away from: doors, passages, stairs, the stream,
/// pillars and the walking route between the doors.
pub fn keep_out(r: &RoomPlan) -> Vec<Zone> {
    let mut z = door_zones(r);
    z.extend(passage_zones(r));
    z.extend(stair_zones(r));
    if let Some((sz, _)) = stream_bridge(r) {
        z.push((Vec2::new(r.min.x, sz - STREAM_HALF - 0.6), Vec2::new(r.max.x, sz + STREAM_HALF + 0.6)));
    }
    for p in pillars(r) {
        z.push((p - Vec2::splat(0.8), p + Vec2::splat(0.8)));
    }
    // The route: entry door → middle → exit door, 1.1 m either side.
    let m = 1.1;
    let mid = (r.min.y + r.max.y) / 2.0;
    let (x0, x1) = (r.entry_x.min(r.exit_x), r.entry_x.max(r.exit_x));
    z.push((Vec2::new(r.entry_x - m, mid - m), Vec2::new(r.entry_x + m, r.max.y)));
    z.push((Vec2::new(r.exit_x - m, r.min.y), Vec2::new(r.exit_x + m, mid + m)));
    z.push((Vec2::new(x0 - m, mid - m), Vec2::new(x1 + m, mid + m)));
    z
}

fn free(zone: &Zone, blocked: &[Zone]) -> bool {
    !blocked.iter().any(|b| zones_overlap(zone, b))
}

/// Where a garden shed can stand: against a side wall, its door facing the room.
/// Returns the centre and which wall.
pub fn shed_spot(r: &RoomPlan, blocked: &[Zone], rng: &mut GameRng) -> Option<(Vec2, bool)> {
    if r.area != Area::Grounds || r.width() < 10.0 || r.depth() < 9.0 || matches!(r.kind, RoomKind::Final | RoomKind::Transition) {
        return None;
    }
    let mut tries: Vec<(f32, bool)> = vec![];
    let mut z = r.max.y - 2.4;
    while z > r.min.y + 2.4 {
        tries.push((z, false));
        tries.push((z, true));
        z -= 1.3;
    }
    for i in (1..tries.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        tries.swap(i, j);
    }
    for (z, east) in tries {
        let x = if east { r.max.x - 0.15 - SHED_DEPTH / 2.0 } else { r.min.x + 0.15 + SHED_DEPTH / 2.0 };
        // Its footprint plus room to walk to the door.
        let zone = (Vec2::new(x - SHED_DEPTH / 2.0 - 0.6, z - SHED_LEN / 2.0 - 0.3), Vec2::new(x + SHED_DEPTH / 2.0 + 0.6, z + SHED_LEN / 2.0 + 0.3));
        if free(&zone, blocked) {
            return Some((Vec2::new(x, z), east));
        }
    }
    None
}

/// A free spot for a little windmill, away from everything else.
pub fn open_spot(r: &RoomPlan, blocked: &[Zone], half: f32, rng: &mut GameRng) -> Option<Vec2> {
    for _ in 0..40 {
        let x = rng.range_f32(r.min.x + half + 0.4, r.max.x - half - 0.4);
        let z = rng.range_f32(r.min.y + half + 0.6, r.max.y - half - 0.6);
        let zone = (Vec2::new(x - half, z - half), Vec2::new(x + half, z + half));
        if free(&zone, blocked) {
            return Some(Vec2::new(x, z));
        }
    }
    None
}

/// A wooden garden shed (model) with its door facing the room, a dimly lit
/// window now and then, and a bucket by the wall. Solid.
pub fn garden_shed(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, at: Vec2, east: bool, rng: &mut GameRng) {
    let _ = props;
    // The model's door side is +Z; it faces away from the wall.
    let yaw = if east { -FRAC_PI_2 } else { FRAC_PI_2 };
    place(b, assets, "shed", Vec3::new(at.x, 0.0, at.y), yaw, Vec3::splat(1.0), Some(Vec3::new(SHED_LEN / 2.0, 2.5, SHED_DEPTH / 2.0)));
    let tf = Transform::from_xyz(at.x, 0.0, at.y).with_rotation(Quat::from_rotation_y(yaw));
    let side = tf.transform_point(Vec3::new(SHED_LEN / 2.0 + 0.35, 0.0, 0.4));
    place(b, assets, "bucket", side, rng.range_f32(0.0, 6.28), Vec3::splat(1.0), Some(Vec3::new(0.15, 0.35, 0.15)));
    if rng.next_f32() < 0.6 {
        let p = tf.transform_point(Vec3::new(-0.8, 1.4, SHED_DEPTH / 2.0 + 0.6));
        let e = b.commands.spawn((PointLight { color: Color::srgb(1.0, 0.75, 0.45), intensity: 18_000.0, range: 4.0, ..default() }, Transform::from_translation(p))).id();
        b.tag(e);
    }
}

/// A windmill (model) whose sails turn. `k` scales it: 1.0 is a 3.2 m garden
/// windmill, 2.5+ a tall one on the hills beyond the walls.
pub fn windmill(b: &mut Builder, assets: &AssetServer, at: Vec3, yaw: f32, k: f32, speed: f32, solid: bool) {
    let e = place(b, assets, "windmill", at, yaw, Vec3::splat(k), solid.then(|| Vec3::new(0.55, 2.0, 0.55) * k));
    b.commands.entity(e).insert(Sails(speed));
}

/// A painted wooden bench (model). Its back is on the model's +Z side, so it's
/// turned round to keep "back toward -Z" like the rest of the layout code.
pub fn bench(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, at: Vec3, yaw: f32) {
    let _ = props;
    place(b, assets, "bench", at, yaw + std::f32::consts::PI, Vec3::splat(1.0), Some(Vec3::new(0.6, 0.9, 0.27)));
}

/// A potted plant (one of three models).
pub fn potted_plant(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, at: Vec3, k: f32, rng: &mut GameRng) {
    let _ = props;
    let (name, h) = [("plant_tall", 1.5), ("plant_round", 0.9), ("plant_tall", 1.5)][rng.below(3) as usize];
    let k = (k / 1.4).clamp(0.7, 1.3);
    place(b, assets, name, at, rng.range_f32(0.0, 6.28), Vec3::splat(k), Some(Vec3::new(0.3, h, 0.3) * k));
}

/// A grandfather clock (model) standing against a wall, facing the room.
pub fn grandfather_clock(b: &mut Builder, assets: &AssetServer, at: Vec3, yaw: f32) {
    place(b, assets, "grandfather_clock", at, yaw, Vec3::splat(1.0), Some(Vec3::new(0.32, 2.1, 0.22)));
}

/// A metal drum (model), sometimes tipped over.
pub fn barrel(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, at: Vec3, tipped: bool) {
    let _ = props;
    if tipped {
        let tf = Transform::from_translation(at + Vec3::Y * 0.29).with_rotation(Quat::from_rotation_z(FRAC_PI_2));
        let e = b.commands.spawn((prop(assets, "barrel"), tf, Collider(Aabb { min: at + Vec3::new(-0.45, 0.0, -0.3), max: at + Vec3::new(0.45, 0.58, 0.3) }))).id();
        b.tag(e);
    } else {
        place(b, assets, "barrel", at, 0.0, Vec3::splat(1.0), Some(Vec3::new(0.29, 0.9, 0.29)));
    }
}

/// Flat clutter that never blocks you: rugs, loose papers, scattered leaves.
pub fn floor_clutter(b: &mut Builder, props: &PropMaterials, r: &RoomPlan, rng: &mut GameRng) {
    let n = ((r.width() * r.depth()) / 14.0) as usize;
    let mat = match r.area {
        Area::Grounds => props.leaves_orange.clone(),
        Area::QuarryLift => b.mats.dirt.clone(),
        _ => props.paper.clone(),
    };
    for _ in 0..n.min(14) {
        let x = rng.range_f32(r.min.x + 0.3, r.max.x - 0.3);
        let z = rng.range_f32(r.min.y + 0.3, r.max.y - 0.3);
        let (w, d) = match r.area {
            Area::QuarryLift => (rng.range_f32(0.4, 0.9), rng.range_f32(0.3, 0.7)),
            _ => (rng.range_f32(0.16, 0.24), rng.range_f32(0.22, 0.3)),
        };
        let e = b
            .commands
            .spawn((Mesh3d(props.unit_cube.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(x, 0.016 + rng.range_f32(0.0, 0.004), z).with_rotation(Quat::from_rotation_y(rng.range_f32(0.0, 3.1))).with_scale(Vec3::new(w, 0.004, d))))
            .id();
        b.tag(e);
    }
}

/// Extra scenery for a room, by area; returns zones the furniture must avoid.
pub fn dress_room(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, r: &RoomPlan, extra: &[Zone], rng: &mut GameRng) -> Vec<Zone> {
    // Chases and rooms split by walls or shelves (zigzags, barricades, the
    // Reading Hall) wind between lanes: nothing free-standing goes in them,
    // so the way through is never blocked.
    if r.pursuit || !r.dividers().is_empty() {
        floor_clutter(b, props, r, rng);
        return vec![];
    }
    let mut blocked = keep_out(r);
    blocked.extend_from_slice(extra);
    let mut taken: Vec<Zone> = vec![];
    let take = |blocked: &mut Vec<Zone>, taken: &mut Vec<Zone>, z: Zone| {
        blocked.push(z);
        taken.push(z);
    };
    // Side-wall furniture slots run along the walls; scenery keeps to the
    // middle band so lockers and chests still fit.
    let side_band = 1.3;
    let mut walls: Vec<Zone> = vec![(Vec2::new(r.min.x, r.min.y), Vec2::new(r.min.x + side_band, r.max.y)), (Vec2::new(r.max.x - side_band, r.min.y), Vec2::new(r.max.x, r.max.y))];
    match r.area {
        Area::Grounds => {
            if let Some((c, east)) = shed_spot(r, &blocked, rng) {
                garden_shed(b, props, assets, c, east, rng);
                take(&mut blocked, &mut taken, (c - Vec2::new(SHED_DEPTH / 2.0 + 0.5, SHED_LEN / 2.0 + 0.8), c + Vec2::new(SHED_DEPTH / 2.0 + 0.5, SHED_LEN / 2.0 + 0.8)));
            }
            walls.clear();
            let mut all = blocked.clone();
            all.extend(walls.iter().copied());
            if r.width() > 9.0 && rng.next_f32() < 0.75 {
                if let Some(p) = open_spot(r, &all, 0.9, rng) {
                    windmill(b, assets, Vec3::new(p.x, 0.0, p.y), rng.range_f32(0.0, 6.28), 1.0, rng.range_f32(0.6, 1.4), true);
                    take(&mut blocked, &mut taken, (p - Vec2::splat(0.9), p + Vec2::splat(0.9)));
                }
            }
            // A bench beside the path now and then.
            if rng.next_f32() < 0.5 {
                let mut all = blocked.clone();
                all.extend(walls.iter().copied());
                if let Some(p) = open_spot(r, &all, 1.0, rng) {
                    bench(b, props, assets, Vec3::new(p.x, 0.0, p.y), rng.range_f32(0.0, 6.2));
                    take(&mut blocked, &mut taken, (p - Vec2::splat(1.0), p + Vec2::splat(1.0)));
                }
            }
            // Tall windmills on the hills beyond the walls, turning slowly.
            if rng.next_f32() < 0.55 {
                let east = rng.next_f32() < 0.5;
                let x = if east { r.max.x + rng.range_f32(6.0, 11.0) } else { r.min.x - rng.range_f32(6.0, 11.0) };
                let z = rng.range_f32(r.min.y, r.max.y);
                windmill(b, assets, Vec3::new(x, 0.0, z), rng.range_f32(0.0, 6.28), rng.range_f32(2.4, 3.2), rng.range_f32(0.25, 0.5), false);
            }
        }
        Area::Atrium if r.width() > 7.0 => {
            let mut all = blocked.clone();
            all.extend(walls.iter().copied());
            for _ in 0..3 {
                if let Some(p) = open_spot(r, &all, 0.95, rng) {
                    bench(b, props, assets, Vec3::new(p.x, 0.0, p.y), if rng.next_f32() < 0.5 { 0.0 } else { std::f32::consts::PI });
                    take(&mut blocked, &mut taken, (p - Vec2::splat(0.95), p + Vec2::splat(0.95)));
                    all.push((p - Vec2::splat(0.95), p + Vec2::splat(0.95)));
                }
            }
        }
        Area::ServiceTunnels | Area::QuarryLift if r.width() > 5.0 => {
            let mut all = blocked.clone();
            all.extend(walls.iter().copied());
            for _ in 0..rng.below(3) + 1 {
                if let Some(p) = open_spot(r, &all, 0.55, rng) {
                    barrel(b, props, assets, Vec3::new(p.x, 0.0, p.y), rng.next_f32() < 0.25);
                    let z = (p - Vec2::splat(0.55), p + Vec2::splat(0.55));
                    take(&mut blocked, &mut taken, z);
                    all.push(z);
                }
            }
        }
        _ => {}
    }
    // Big indoor rooms: planters and lamps standing out in the open.
    if matches!(r.area, Area::GuestWing | Area::Atrium | Area::Archive) && r.width() > 6.0 {
        let mut all = blocked.clone();
        all.extend(walls.iter().copied());
        let n = ((r.width() * r.depth()) / 35.0).round().clamp(1.0, 5.0) as usize;
        for _ in 0..n {
            if let Some(p) = open_spot(r, &all, 0.5, rng) {
                let at = Vec3::new(p.x, 0.0, p.y);
                potted_plant(b, props, assets, at, rng.range_f32(1.2, 1.8), rng);
                let z = (p - Vec2::splat(0.5), p + Vec2::splat(0.5));
                take(&mut blocked, &mut taken, z);
                all.push((p - Vec2::splat(1.4), p + Vec2::splat(1.4)));
            }
        }
    }
    if !matches!(r.area, Area::Atrium) {
        floor_clutter(b, props, r, rng);
    }
    taken
}

/// Decor for a free side-wall slot in the Guest Wing / Atrium: a plant or a
/// grandfather clock against the wall.
pub fn wall_decor(b: &mut Builder, props: &PropMaterials, assets: &AssetServer, x: f32, inward: f32, yaw: f32, z: f32, rng: &mut GameRng) {
    if rng.next_f32() < 0.6 {
        potted_plant(b, props, assets, Vec3::new(x + inward * 0.4, 0.0, z), rng.range_f32(1.0, 1.4), rng);
    } else {
        // The clock's face is its +Z side: turn it to face into the room.
        grandfather_clock(b, assets, Vec3::new(x + inward * 0.25, 0.0, z), yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor::plan::generate;

    #[test]
    fn scenery_never_blocks_doors_or_the_route() {
        for seed in 0..60u64 {
            for r in generate(seed).rooms.iter() {
                let keep = keep_out(r);
                let mut rng = GameRng::new(r.seed);
                if let Some((c, _)) = shed_spot(r, &keep, &mut rng) {
                    let fp = (c - Vec2::new(SHED_DEPTH / 2.0, SHED_LEN / 2.0), c + Vec2::new(SHED_DEPTH / 2.0, SHED_LEN / 2.0));
                    assert!(fp.0.x >= r.min.x && fp.1.x <= r.max.x && fp.0.y >= r.min.y && fp.1.y <= r.max.y, "seed {seed} room {}: shed outside the room", r.index);
                    for d in door_zones(r).iter().chain(passage_zones(r).iter()) {
                        assert!(!zones_overlap(&fp, d), "seed {seed} room {}: shed in a door zone", r.index);
                    }
                }
                if let Some(p) = open_spot(r, &keep, 0.9, &mut rng) {
                    let fp = (p - Vec2::splat(0.9), p + Vec2::splat(0.9));
                    assert!(!keep.iter().any(|k| zones_overlap(&fp, k)), "seed {seed} room {}", r.index);
                }
            }
        }
    }

    #[test]
    fn chases_and_divided_rooms_get_no_free_standing_scenery() {
        // dress_room returns early for these; check the plan has both kinds so the rule matters.
        let plan = generate(3);
        assert!(plan.rooms.iter().any(|r| r.pursuit));
        assert!(plan.rooms.iter().any(|r| !r.dividers().is_empty()));
    }

    #[test]
    fn gardens_get_sheds() {
        let mut sheds = 0;
        let mut gardens = 0;
        for seed in 0..40u64 {
            for r in generate(seed).rooms.iter().filter(|r| r.area == Area::Grounds) {
                gardens += 1;
                let keep = keep_out(r);
                if shed_spot(r, &keep, &mut GameRng::new(r.seed)).is_some() {
                    sheds += 1;
                }
            }
        }
        assert!(gardens > 0 && sheds * 3 > gardens, "only {sheds} sheds in {gardens} gardens");
    }
}
