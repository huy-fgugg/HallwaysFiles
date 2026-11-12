//! Minimal, deterministic collision and ray helpers.
//!
//! The level is built from axis-aligned boxes, so we don't need a full physics
//! engine: the player is a vertical cylinder resolved against wall AABBs on the
//! XZ plane, and interaction uses ray-vs-AABB slab tests. Keeping this pure
//! (no ECS) makes it trivially unit-testable.

use bevy::prelude::*;

/// World-space axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        let half = size * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }

    /// Returns the distance along the ray to the first intersection, if any
    /// within `max_dist`. `dir` must be normalized. A ray starting inside the
    /// box reports a hit at distance 0.
    pub fn ray_hit(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<f32> {
        let mut t_min = 0.0_f32;
        let mut t_max = max_dist;
        for axis in 0..3 {
            let o = origin[axis];
            let d = dir[axis];
            let (lo, hi) = (self.min[axis], self.max[axis]);
            if d.abs() < 1e-8 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let mut t0 = (lo - o) * inv;
                let mut t1 = (hi - o) * inv;
                if t0 > t1 {
                    std::mem::swap(&mut t0, &mut t1);
                }
                t_min = t_min.max(t0);
                t_max = t_max.min(t1);
                if t_min > t_max {
                    return None;
                }
            }
        }
        Some(t_min)
    }
}

/// Static solid geometry the player cannot walk through.
#[derive(Component, Debug, Clone, Copy)]
pub struct Collider(pub Aabb);

/// A walkable top surface (stairs, landings, lofts). The floor at y = 0 is implicit.
#[derive(Component, Debug, Clone, Copy)]
pub struct Ground(pub Aabb);

/// Highest walkable height under `pos` (XZ) that is at most `step` above `feet`.
pub fn ground_height(pos: Vec3, feet: f32, step: f32, grounds: &[Aabb]) -> f32 {
    grounds
        .iter()
        .filter(|g| pos.x >= g.min.x && pos.x <= g.max.x && pos.z >= g.min.z && pos.z <= g.max.z && g.max.y <= feet + step)
        .map(|g| g.max.y)
        .fold(0.0, f32::max)
}

/// Pushes a vertical cylinder (centre `pos` on the XZ plane, `radius`, spanning
/// `y_min..y_max`) out of every box it overlaps. Returns the corrected
/// position. Runs a few iterations so corners resolve cleanly.
pub fn resolve_cylinder(mut pos: Vec3, radius: f32, y_min: f32, y_max: f32, boxes: &[Aabb]) -> Vec3 {
    for _ in 0..4 {
        let mut moved = false;
        for b in boxes {
            if y_max <= b.min.y || y_min >= b.max.y {
                continue;
            }
            let closest_x = pos.x.clamp(b.min.x, b.max.x);
            let closest_z = pos.z.clamp(b.min.z, b.max.z);
            let dx = pos.x - closest_x;
            let dz = pos.z - closest_z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq >= radius * radius {
                continue;
            }
            if dist_sq > 1e-10 {
                let dist = dist_sq.sqrt();
                let push = radius - dist;
                pos.x += dx / dist * push;
                pos.z += dz / dist * push;
            } else {
                // Centre is inside the box: push out along the shallowest axis.
                let left = pos.x - b.min.x;
                let right = b.max.x - pos.x;
                let back = pos.z - b.min.z;
                let front = b.max.z - pos.z;
                let m = left.min(right).min(back).min(front);
                if m == left {
                    pos.x = b.min.x - radius;
                } else if m == right {
                    pos.x = b.max.x + radius;
                } else if m == back {
                    pos.z = b.min.z - radius;
                } else {
                    pos.z = b.max.z + radius;
                }
            }
            moved = true;
        }
        if !moved {
            break;
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_steps_up_but_not_onto_ledges() {
        let step = Aabb { min: Vec3::new(0.0, 0.0, 0.0), max: Vec3::new(1.0, 0.2, 1.0) };
        let ledge = Aabb { min: Vec3::new(2.0, 0.0, 0.0), max: Vec3::new(3.0, 1.6, 1.0) };
        let g = [step, ledge];
        assert_eq!(ground_height(Vec3::new(0.5, 0.0, 0.5), 0.0, 0.38, &g), 0.2);
        assert_eq!(ground_height(Vec3::new(2.5, 0.0, 0.5), 0.0, 0.38, &g), 0.0);
        assert_eq!(ground_height(Vec3::new(2.5, 0.0, 0.5), 1.6, 0.38, &g), 1.6);
        assert_eq!(ground_height(Vec3::new(5.0, 0.0, 0.5), 1.6, 0.38, &g), 0.0);
    }

    fn wall() -> Aabb {
        Aabb::from_center_size(Vec3::new(0.0, 1.5, 0.0), Vec3::new(4.0, 3.0, 0.2))
    }

    #[test]
    fn ray_hits_box_in_front() {
        let t = wall().ray_hit(Vec3::new(0.0, 1.5, 2.0), Vec3::NEG_Z, 10.0);
        assert!((t.unwrap() - 1.9).abs() < 1e-5);
    }

    #[test]
    fn ray_misses_box_behind_or_out_of_range() {
        assert!(wall().ray_hit(Vec3::new(0.0, 1.5, 2.0), Vec3::Z, 10.0).is_none());
        assert!(wall().ray_hit(Vec3::new(0.0, 1.5, 2.0), Vec3::NEG_Z, 1.0).is_none());
        assert!(wall().ray_hit(Vec3::new(5.0, 1.5, 2.0), Vec3::NEG_Z, 10.0).is_none());
    }

    #[test]
    fn ray_inside_box_hits_at_zero() {
        assert_eq!(wall().ray_hit(Vec3::new(0.0, 1.5, 0.0), Vec3::X, 1.0), Some(0.0));
    }

    #[test]
    fn cylinder_is_pushed_out_of_wall() {
        let p = resolve_cylinder(Vec3::new(0.0, 0.0, 0.25), 0.3, 0.0, 1.8, &[wall()]);
        assert!((p.z - 0.4).abs() < 1e-4, "{p:?}");
        assert!(p.x.abs() < 1e-6);
    }

    #[test]
    fn cylinder_inside_wall_exits_shallowest_side() {
        let p = resolve_cylinder(Vec3::new(0.0, 0.0, -0.05), 0.3, 0.0, 1.8, &[wall()]);
        assert!((p.z - (-0.4)).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn cylinder_ignores_boxes_above_head() {
        let beam = Aabb::from_center_size(Vec3::new(0.0, 2.5, 0.0), Vec3::new(4.0, 0.2, 4.0));
        let p = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(resolve_cylinder(p, 0.3, 0.0, 1.8, &[beam]), p);
    }

    #[test]
    fn corner_resolves_against_two_walls() {
        let a = Aabb::from_center_size(Vec3::new(0.0, 1.5, -1.0), Vec3::new(4.0, 3.0, 0.2));
        let b = Aabb::from_center_size(Vec3::new(-1.0, 1.5, 0.0), Vec3::new(0.2, 3.0, 4.0));
        let p = resolve_cylinder(Vec3::new(-0.85, 0.0, -0.85), 0.3, 0.0, 1.8, &[a, b]);
        assert!(p.x >= -0.9 + 0.3 - 1e-4 && p.z >= -0.9 + 0.3 - 1e-4, "{p:?}");
    }
}
