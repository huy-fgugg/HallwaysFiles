//! Scenery: garden sheds, little windmills and the props that fill empty
//! floor space in every area. Everything here keeps clear of doors, passages,
//! stairs and the walking route (the same zones the furniture respects).

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use super::build::{stair_zones, PropMaterials};
use super::plan::{door_zones, passage_zones, pillars, stream_bridge, zones_overlap, Area, RoomKind, RoomPlan, Zone, STREAM_HALF};
use crate::physics::{Aabb, Collider};
use crate::rng::GameRng;
use crate::world::{world_aabb, Builder};

/// Spins an entity around its local Z axis (windmill sails), in radians a second.
#[derive(Component)]
pub struct Spin(pub f32);

pub fn spin_props(time: Res<Time>, mut q: Query<(&Spin, &mut Transform)>) {
    for (s, mut t) in &mut q {
        t.rotate_local_z(s.0 * time.delta_secs());
    }
}

/// Footprint of a shed standing against a side wall (`east`), centred at depth `z`.
pub const SHED_DEPTH: f32 = 2.0;
pub const SHED_LEN: f32 = 2.6;

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

/// Plank garden shed with a pitched tin roof, a door facing the room and a
/// dimly lit window. Solid.
pub fn garden_shed(b: &mut Builder, props: &PropMaterials, at: Vec2, east: bool, rng: &mut GameRng) {
    // Local +Z is the door side; it faces away from the wall.
    let yaw = if east { -FRAC_PI_2 } else { FRAC_PI_2 };
    let tf = Transform::from_xyz(at.x, 0.0, at.y).with_rotation(Quat::from_rotation_y(yaw));
    let (w, d, wall_h, ridge) = (SHED_LEN, SHED_DEPTH, 2.1, 2.85);
    let col = world_aabb(Vec3::new(-w / 2.0, 0.0, -d / 2.0), Vec3::new(w / 2.0, ridge, d / 2.0), &tf);
    let root = b.commands.spawn((tf, Visibility::default(), Collider(col))).id();
    b.tag(root);
    let planks = b.mats.door_wood.clone();
    let trim = b.mats.trim_wood.clone();
    let roof = b.mats.corrugated.clone();
    let iron = b.mats.iron.clone();
    let cube = props.unit_cube.clone();
    let glass = props.glass.clone();
    let lit = rng.next_f32() < 0.6;
    let pitch = ((ridge - wall_h) / (d / 2.0)).atan();
    let slope = (d / 2.0 + 0.25) / pitch.cos();
    b.commands.entity(root).with_children(|s| {
        // Walls.
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(planks.clone()), Transform::from_xyz(0.0, wall_h / 2.0, 0.0).with_scale(Vec3::new(w, wall_h, d))));
        // Corner posts and a skirting board.
        for (x, z) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(x * (w / 2.0 + 0.02), wall_h / 2.0, z * (d / 2.0 + 0.02)).with_scale(Vec3::new(0.12, wall_h, 0.12))));
        }
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::new(w + 0.06, 0.16, d + 0.06))));
        // Vertical battens so the walls read as planks.
        let mut x = -w / 2.0 + 0.3;
        while x < w / 2.0 - 0.2 {
            for z in [-1.0f32, 1.0] {
                s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(x, wall_h / 2.0, z * (d / 2.0 + 0.01)).with_scale(Vec3::new(0.04, wall_h - 0.1, 0.03))));
            }
            x += 0.42;
        }
        // Gable: a diamond squashed into a triangle, filling the attic.
        s.spawn((Transform::from_xyz(0.0, wall_h, 0.0).with_scale(Vec3::new(1.0, (ridge - wall_h) / (d / 2.0), 1.0)), Visibility::default())).with_children(|g| {
            g.spawn((Mesh3d(cube.clone()), MeshMaterial3d(planks.clone()), Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_4)).with_scale(Vec3::new(w - 0.02, d / std::f32::consts::SQRT_2, d / std::f32::consts::SQRT_2))));
        });
        // Tin roof: two slabs with an overhang.
        for side in [-1.0f32, 1.0] {
            let cz = side * (d / 4.0 + 0.06);
            s.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(roof.clone()),
                Transform::from_xyz(0.0, (wall_h + ridge) / 2.0 + 0.05, cz).with_rotation(Quat::from_rotation_x(side * pitch)).with_scale(Vec3::new(w + 0.4, 0.06, slope)),
            ));
        }
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(0.0, ridge + 0.06, 0.0).with_scale(Vec3::new(w + 0.42, 0.08, 0.14))));
        // Door with a frame, cross brace and latch.
        let dz = d / 2.0 + 0.03;
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(0.35, 0.95, dz).with_scale(Vec3::new(0.95, 1.9, 0.05))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(planks.clone()), Transform::from_xyz(0.35, 0.92, dz + 0.02).with_scale(Vec3::new(0.82, 1.78, 0.04))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(0.35, 0.92, dz + 0.05).with_rotation(Quat::from_rotation_z(1.1)).with_scale(Vec3::new(1.9, 0.08, 0.03))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.68, 0.95, dz + 0.07).with_scale(Vec3::new(0.05, 0.16, 0.04))));
        // Window beside the door, with a cross bar.
        let win = if lit { glass.clone() } else { iron.clone() };
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(-0.7, 1.4, dz).with_scale(Vec3::new(0.62, 0.52, 0.05))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(win), Transform::from_xyz(-0.7, 1.4, dz + 0.015).with_scale(Vec3::new(0.5, 0.4, 0.04))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(-0.7, 1.4, dz + 0.04).with_scale(Vec3::new(0.5, 0.04, 0.02))));
        s.spawn((Mesh3d(cube.clone()), MeshMaterial3d(trim.clone()), Transform::from_xyz(-0.7, 1.4, dz + 0.04).with_scale(Vec3::new(0.04, 0.4, 0.02))));
    });
    // Leaning tools and a water butt beside it.
    let side = tf.transform_point(Vec3::new(w / 2.0 + 0.4, 0.0, 0.1));
    let butt = b
        .commands
        .spawn((
            Mesh3d(props.unit_cyl.clone()),
            MeshMaterial3d(b.mats.rusty_metal.clone()),
            Transform::from_xyz(side.x, 0.45, side.z).with_scale(Vec3::new(0.6, 0.9, 0.6)),
            Collider(Aabb { min: side + Vec3::new(-0.3, 0.0, -0.3), max: side + Vec3::new(0.3, 0.9, 0.3) }),
        ))
        .id();
    b.tag(butt);
    if lit {
        let p = tf.transform_point(Vec3::new(-0.7, 1.5, d / 2.0 + 0.6));
        let e = b.commands.spawn((PointLight { color: Color::srgb(1.0, 0.75, 0.45), intensity: 18_000.0, range: 4.0, ..default() }, Transform::from_translation(p))).id();
        b.tag(e);
    }
}

/// A little lattice windmill with turning sails. `k` scales it: 1.0 is a garden
/// ornament about 2.7 m tall, 2.5 a tall one beyond the walls.
pub fn windmill(b: &mut Builder, props: &PropMaterials, at: Vec3, yaw: f32, k: f32, speed: f32, solid: bool) {
    let mut e = b.commands.spawn((Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()));
    if solid {
        e.insert(Collider(Aabb { min: at + Vec3::new(-0.45, 0.0, -0.45) * k, max: at + Vec3::new(0.45, 2.6, 0.45) * k }));
    }
    let root = e.id();
    b.tag(root);
    let wood = b.mats.trim_wood.clone();
    let planks = b.mats.door_wood.clone();
    let canvas = props.canvas.clone();
    let cube = props.unit_cube.clone();
    let pyramid = props.pyramid.clone();
    let cyl = props.unit_cyl.clone();
    let top = 2.4 * k;
    b.commands.entity(root).with_children(|m| {
        // Four splayed legs.
        for (sx, sz) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let a = Vec3::new(sx * 0.45, 0.0, sz * 0.45) * k;
            let c = Vec3::new(sx * 0.14 * k, top, sz * 0.14 * k);
            let dir = c - a;
            m.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_translation((a + c) / 2.0).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir.normalize())).with_scale(Vec3::new(0.06 * k, dir.length(), 0.06 * k))));
        }
        // Cross rings.
        for (y, half) in [(0.8 * k, 0.36 * k), (1.6 * k, 0.26 * k)] {
            for (rot, off) in [(0.0f32, Vec3::Z), (FRAC_PI_2, Vec3::X)] {
                for s in [-1.0f32, 1.0] {
                    m.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_translation(Vec3::new(0.0, y, 0.0) + off * s * half).with_rotation(Quat::from_rotation_y(rot)).with_scale(Vec3::new(half * 2.0, 0.045 * k, 0.045 * k))));
                }
            }
        }
        // Housing and its little roof.
        m.spawn((Mesh3d(cube.clone()), MeshMaterial3d(planks.clone()), Transform::from_xyz(0.0, top + 0.18 * k, 0.0).with_scale(Vec3::new(0.38, 0.36, 0.56) * k)));
        m.spawn((Mesh3d(pyramid.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, top + 0.5 * k, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_4)).with_scale(Vec3::new(0.62, 0.3, 0.62) * k)));
        // Tail vane at the back.
        m.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, top + 0.2 * k, -0.6 * k).with_scale(Vec3::new(0.03, 0.03, 0.7) * k)));
        m.spawn((Mesh3d(cube.clone()), MeshMaterial3d(planks.clone()), Transform::from_xyz(0.0, top + 0.28 * k, -0.95 * k).with_scale(Vec3::new(0.02, 0.4, 0.36) * k)));
        // Hub and sails, turning.
        m.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, top + 0.18 * k, 0.34 * k).with_rotation(Quat::from_rotation_x(FRAC_PI_2)).with_scale(Vec3::new(0.12, 0.16, 0.12) * k)));
        m.spawn((Transform::from_xyz(0.0, top + 0.18 * k, 0.42 * k).with_rotation(Quat::from_rotation_z(yaw * 3.0)), Visibility::default(), Spin(speed))).with_children(|hub| {
            for i in 0..4 {
                let a = i as f32 * FRAC_PI_2;
                hub.spawn((Transform::from_rotation(Quat::from_rotation_z(a)), Visibility::default())).with_children(|arm| {
                    arm.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, 0.55 * k, 0.0).with_scale(Vec3::new(0.04, 1.1, 0.03) * k)));
                    arm.spawn((Mesh3d(cube.clone()), MeshMaterial3d(canvas.clone()), Transform::from_xyz(0.1 * k, 0.65 * k, 0.0).with_scale(Vec3::new(0.18, 0.8, 0.012) * k)));
                });
            }
        });
    });
}

/// A wooden bench: seat, back and two iron frames.
pub fn bench(b: &mut Builder, props: &PropMaterials, at: Vec3, yaw: f32) {
    let tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw));
    let col = world_aabb(Vec3::new(-0.8, 0.0, -0.3), Vec3::new(0.8, 0.9, 0.25), &tf);
    let root = b.commands.spawn((tf, Visibility::default(), Collider(col))).id();
    b.tag(root);
    let (wood, iron, cube) = (b.mats.door_wood.clone(), b.mats.iron.clone(), props.unit_cube.clone());
    b.commands.entity(root).with_children(|p| {
        for z in [-0.15f32, 0.0, 0.15] {
            p.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, 0.45, z).with_scale(Vec3::new(1.6, 0.04, 0.12))));
        }
        for y in [0.62f32, 0.8] {
            p.spawn((Mesh3d(cube.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(0.0, y, -0.24).with_rotation(Quat::from_rotation_x(-0.2)).with_scale(Vec3::new(1.6, 0.12, 0.04))));
        }
        for x in [-0.7f32, 0.7] {
            p.spawn((Mesh3d(cube.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(x, 0.22, 0.0).with_scale(Vec3::new(0.05, 0.45, 0.36))));
            p.spawn((Mesh3d(cube.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(x, 0.62, -0.22).with_scale(Vec3::new(0.05, 0.4, 0.04))));
        }
    });
}

/// Potted plant: terracotta pot with a leafy crown.
pub fn potted_plant(b: &mut Builder, props: &PropMaterials, at: Vec3, k: f32, rng: &mut GameRng) {
    let root = b.commands.spawn((Transform::from_translation(at), Visibility::default(), Collider(Aabb { min: at + Vec3::new(-0.25, 0.0, -0.25) * k, max: at + Vec3::new(0.25, 1.2, 0.25) * k }))).id();
    b.tag(root);
    let (cyl, sphere, cone, pot, leaf) = (props.unit_cyl.clone(), props.sphere.clone(), props.cone.clone(), props.terracotta.clone(), props.hedge.clone());
    let fronds: Vec<(f32, f32)> = (0..7).map(|_| (rng.range_f32(0.0, std::f32::consts::TAU), rng.range_f32(0.5, 0.9))).collect();
    b.commands.entity(root).with_children(|p| {
        // A flat-bottomed pot: narrow foot, wider body, a lip at the top.
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(pot.clone()), Transform::from_xyz(0.0, 0.04 * k, 0.0).with_scale(Vec3::new(0.3, 0.08, 0.3) * k)));
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(pot.clone()), Transform::from_xyz(0.0, 0.24 * k, 0.0).with_scale(Vec3::new(0.4, 0.34, 0.4) * k)));
        p.spawn((Mesh3d(cyl), MeshMaterial3d(pot), Transform::from_xyz(0.0, 0.43 * k, 0.0).with_scale(Vec3::new(0.48, 0.07, 0.48) * k)));
        let _ = cone;
        p.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(leaf.clone()), Transform::from_xyz(0.0, 0.75 * k, 0.0).with_scale(Vec3::new(0.55, 0.6, 0.55) * k)));
        for (a, tilt) in fronds {
            p.spawn((Mesh3d(sphere.clone()), MeshMaterial3d(leaf.clone()), Transform::from_xyz(a.cos() * 0.22 * k, 0.85 * k, a.sin() * 0.22 * k).with_rotation(Quat::from_rotation_y(-a) * Quat::from_rotation_z(tilt)).with_scale(Vec3::new(0.6, 0.08, 0.2) * k)));
        }
    });
}

/// Standing lamp with a warm shade (just glow, no light: it's decoration).
pub fn floor_lamp(b: &mut Builder, props: &PropMaterials, at: Vec3) {
    let root = b.commands.spawn((Transform::from_translation(at), Visibility::default(), Collider(Aabb { min: at + Vec3::new(-0.18, 0.0, -0.18), max: at + Vec3::new(0.18, 1.7, 0.18) }))).id();
    b.tag(root);
    let (cyl, cone, brass, shade) = (props.unit_cyl.clone(), props.cone.clone(), b.mats.brass.clone(), props.shade.clone());
    b.commands.entity(root).with_children(|p| {
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(brass.clone()), Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(0.34, 0.04, 0.34))));
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(brass), Transform::from_xyz(0.0, 0.78, 0.0).with_scale(Vec3::new(0.03, 1.5, 0.03))));
        // A drum shade, slightly flared with a smaller cap on top.
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(shade.clone()), Transform::from_xyz(0.0, 1.5, 0.0).with_scale(Vec3::new(0.42, 0.3, 0.42))));
        p.spawn((Mesh3d(cyl), MeshMaterial3d(shade), Transform::from_xyz(0.0, 1.67, 0.0).with_scale(Vec3::new(0.34, 0.05, 0.34))));
        let _ = cone;
    });
}

/// Iron-banded barrel.
pub fn barrel(b: &mut Builder, props: &PropMaterials, at: Vec3, tipped: bool) {
    let tf = if tipped {
        Transform::from_translation(at + Vec3::Y * 0.3).with_rotation(Quat::from_rotation_z(FRAC_PI_2))
    } else {
        Transform::from_translation(at + Vec3::Y * 0.45)
    };
    let col = Aabb { min: at + Vec3::new(-0.45, 0.0, -0.32), max: at + Vec3::new(0.45, if tipped { 0.6 } else { 0.9 }, 0.32) };
    let root = b.commands.spawn((tf, Visibility::default(), Collider(col))).id();
    b.tag(root);
    let (cyl, wood, iron) = (props.unit_cyl.clone(), b.mats.door_wood.clone(), b.mats.iron.clone());
    b.commands.entity(root).with_children(|p| {
        p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(wood), Transform::from_scale(Vec3::new(0.6, 0.9, 0.6))));
        for y in [-0.3f32, 0.0, 0.3] {
            p.spawn((Mesh3d(cyl.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, y, 0.0).with_scale(Vec3::new(0.63, 0.04, 0.63))));
        }
    });
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
pub fn dress_room(b: &mut Builder, props: &PropMaterials, r: &RoomPlan, extra: &[Zone], rng: &mut GameRng) -> Vec<Zone> {
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
                garden_shed(b, props, c, east, rng);
                take(&mut blocked, &mut taken, (c - Vec2::new(SHED_DEPTH / 2.0 + 0.5, SHED_LEN / 2.0 + 0.8), c + Vec2::new(SHED_DEPTH / 2.0 + 0.5, SHED_LEN / 2.0 + 0.8)));
            }
            walls.clear();
            let mut all = blocked.clone();
            all.extend(walls.iter().copied());
            if r.width() > 9.0 && rng.next_f32() < 0.75 {
                if let Some(p) = open_spot(r, &all, 0.9, rng) {
                    windmill(b, props, Vec3::new(p.x, 0.0, p.y), rng.range_f32(-0.6, 0.6), 1.0, rng.range_f32(0.6, 1.4), true);
                    take(&mut blocked, &mut taken, (p - Vec2::splat(0.9), p + Vec2::splat(0.9)));
                }
            }
            // A bench beside the path now and then.
            if rng.next_f32() < 0.5 {
                let mut all = blocked.clone();
                all.extend(walls.iter().copied());
                if let Some(p) = open_spot(r, &all, 1.0, rng) {
                    bench(b, props, Vec3::new(p.x, 0.0, p.y), rng.range_f32(0.0, 6.2));
                    take(&mut blocked, &mut taken, (p - Vec2::splat(1.0), p + Vec2::splat(1.0)));
                }
            }
            // Tall windmills on the hills beyond the walls, turning slowly.
            if rng.next_f32() < 0.55 {
                let east = rng.next_f32() < 0.5;
                let x = if east { r.max.x + rng.range_f32(6.0, 11.0) } else { r.min.x - rng.range_f32(6.0, 11.0) };
                let z = rng.range_f32(r.min.y, r.max.y);
                windmill(b, props, Vec3::new(x, 0.0, z), if east { -1.2 } else { 1.2 }, rng.range_f32(2.4, 3.2), rng.range_f32(0.25, 0.5), false);
            }
        }
        Area::Atrium if r.width() > 7.0 => {
            let mut all = blocked.clone();
            all.extend(walls.iter().copied());
            for _ in 0..3 {
                if let Some(p) = open_spot(r, &all, 0.95, rng) {
                    bench(b, props, Vec3::new(p.x, 0.0, p.y), if rng.next_f32() < 0.5 { 0.0 } else { std::f32::consts::PI });
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
                    barrel(b, props, Vec3::new(p.x, 0.0, p.y), rng.next_f32() < 0.25);
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
                if r.area == Area::Atrium || rng.next_f32() < 0.7 {
                    potted_plant(b, props, at, rng.range_f32(1.2, 1.8), rng);
                } else {
                    floor_lamp(b, props, at);
                }
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

/// Decor for a free side-wall slot in the Guest Wing / Atrium: plants and lamps.
pub fn wall_decor(b: &mut Builder, props: &PropMaterials, x: f32, inward: f32, z: f32, rng: &mut GameRng) {
    let at = Vec3::new(x + inward * 0.4, 0.0, z);
    if rng.next_f32() < 0.6 {
        potted_plant(b, props, at, rng.range_f32(1.0, 1.4), rng);
    } else {
        floor_lamp(b, props, at);
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
