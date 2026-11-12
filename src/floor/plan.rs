//! Seeded floor generator: a pure function from seed → 100-door floor plan.
//!
//! Layout: Room 000 (lift) and Room 001 (corridor) are handcrafted. Door 002
//! opens into generated rooms 2..=99, laid out as a chain heading toward -Z.
//! Room `i`'s exit is Door `i + 1`, so the last room (99) ends at Door 100.
//! Each room's south side is the previous room's north wall.

use bevy::math::Vec2;

use crate::data::EntityConfig;
use crate::rng::GameRng;

pub const FIRST_ROOM: u32 = 2;
pub const LAST_ROOM: u32 = 99;
/// The final door number of the floor.
pub const FINAL_DOOR: u32 = LAST_ROOM + 1;
/// North face of the corridor's far wall: room 2 starts here.
pub const START_Z: f32 = -14.2;
pub const WALL_T: f32 = 0.2;
const DOOR_MARGIN: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    GuestWing,
    Archive,
    ServiceTunnels,
    QuarryLift,
    Atrium,
}

impl Area {
    pub fn for_room(i: u32) -> Area {
        match i {
            0..=20 => Area::GuestWing,
            21..=40 => Area::Archive,
            41..=60 => Area::ServiceTunnels,
            61..=80 => Area::QuarryLift,
            _ => Area::Atrium,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Area::GuestWing => "The Guest Wing",
            Area::Archive => "The Archive",
            Area::ServiceTunnels => "The Service Tunnels",
            Area::QuarryLift => "The Quarry Lift",
            Area::Atrium => "The Atrium",
        }
    }

    pub fn ceiling(self) -> f32 {
        match self {
            Area::GuestWing => 3.0,
            Area::Archive => 4.2,
            Area::ServiceTunnels => 2.8,
            Area::QuarryLift => 3.3,
            Area::Atrium => 6.0,
        }
    }

    /// (width range, depth range) for ordinary rooms in this area.
    fn size_ranges(self) -> ((f32, f32), (f32, f32)) {
        match self {
            Area::GuestWing => ((5.0, 8.0), (5.0, 8.0)),
            Area::Archive => ((7.0, 10.0), (7.0, 10.0)),
            Area::ServiceTunnels => ((4.5, 6.5), (7.0, 12.0)),
            Area::QuarryLift => ((5.0, 8.0), (6.0, 10.0)),
            Area::Atrium => ((10.0, 13.0), (9.0, 13.0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoomKind {
    /// First room of an area: a breather with a title card.
    Transition,
    Loot,
    /// Long, narrow hall.
    Hall,
    /// Lamp is broken; navigate by the door lights.
    Dark,
    /// Exit door is locked; the key is in one of the drawers.
    KeyPuzzle,
    /// Room 99: the grand hall before Door 100.
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightState {
    Bright,
    Dim,
    Dark,
}

/// Which entity (if any) acts when the player enters a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encounter {
    /// Rushes through once: hide.
    Surge,
    /// Rushes back and forth several times: stay hidden.
    Undertow,
    /// Dark rooms: turn and look at it.
    Whisperer,
    /// Eyes on the walls: look away.
    Gallery,
    /// Pressure wave: stand still.
    Hush,
}

impl Encounter {
    /// Encounters whose counter is hiding.
    pub fn needs_hiding(self) -> bool {
        matches!(self, Encounter::Surge | Encounter::Undertow)
    }
}

/// A fake door with the wrong number on the north wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mimic {
    pub x: f32,
    pub shown_number: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoomPlan {
    pub index: u32,
    pub area: Area,
    pub kind: RoomKind,
    pub light: LightState,
    /// Interior bounds on the XZ plane (x, z). `max.y` is the south face.
    pub min: Vec2,
    pub max: Vec2,
    pub height: f32,
    /// Door into this room (on the south side), x position.
    pub entry_x: f32,
    /// Exit door (Door `index + 1`, on the north wall), x position.
    pub exit_x: f32,
    /// Number of lootable containers.
    pub containers: u8,
    /// At least one wardrobe/locker.
    pub hiding: bool,
    /// Entity encounter triggered on entering this room.
    pub encounter: Option<Encounter>,
    pub mimic: Option<Mimic>,
    /// Per-room seed for furniture, loot and decor.
    pub seed: u64,
}

impl RoomPlan {
    pub fn surge(&self) -> bool {
        matches!(self.encounter, Some(Encounter::Surge | Encounter::Undertow))
    }

    pub fn exit_door(&self) -> u32 {
        self.index + 1
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn depth(&self) -> f32 {
        self.max.y - self.min.y
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorPlan {
    pub seed: u64,
    pub rooms: Vec<RoomPlan>,
}

impl FloorPlan {
    pub fn room(&self, index: u32) -> Option<&RoomPlan> {
        index.checked_sub(FIRST_ROOM).and_then(|i| self.rooms.get(i as usize))
    }

    /// Which generated room contains world z (south faces are inclusive).
    pub fn room_at_z(&self, z: f32) -> Option<u32> {
        if z > START_Z {
            return None;
        }
        // Rooms are sorted by decreasing z; binary search on the south face.
        let idx = self.rooms.partition_point(|r| r.max.y + 0.001 >= z);
        let r = &self.rooms[idx.saturating_sub(1)];
        if z <= r.max.y + 0.001 && z >= r.min.y - WALL_T {
            Some(r.index)
        } else {
            None
        }
    }
}

fn wrong_number(rng: &mut GameRng, real: u32) -> u32 {
    let swapped = {
        let s = format!("{real:03}");
        let b = s.as_bytes();
        let t = format!("{}{}{}", b[0] as char, b[2] as char, b[1] as char);
        t.parse::<u32>().unwrap_or(real + 1)
    };
    let options = [real + 1, real.saturating_sub(2).max(1), real + 10, swapped];
    for _ in 0..8 {
        let n = options[rng.below(options.len() as u32) as usize];
        if n != real {
            return n;
        }
    }
    real + 1
}

pub fn generate(seed: u64) -> FloorPlan {
    generate_with(seed, &EntityConfig::default())
}

pub fn generate_with(seed: u64, cfg: &EntityConfig) -> FloorPlan {
    let mut rooms: Vec<RoomPlan> = Vec::new();
    let mut entry_x = 0.0f32;
    let mut south = START_Z;
    let mut history: Vec<RoomKind> = Vec::new();
    let mut last_seen: std::collections::HashMap<Encounter, i64> = std::collections::HashMap::new();
    let mut last_any: i64 = -100;
    let mut last_key: i64 = -100;

    for index in FIRST_ROOM..=LAST_ROOM {
        let mut rng = GameRng::fork(seed, u64::from(index));
        let area = Area::for_room(index);
        let first_of_area = index == FIRST_ROOM || Area::for_room(index - 1) != area;

        // ---- Kind (weighted, with cooldowns) ----
        let kind = if index == LAST_ROOM {
            RoomKind::Final
        } else if first_of_area {
            RoomKind::Transition
        } else {
            let prev = history.last().copied();
            let prev2 = history.iter().rev().nth(1).copied();
            let dark_w = if area == Area::Archive { 3.5 } else { 1.6 };
            let candidates = [
                (RoomKind::Loot, 5.0),
                (RoomKind::Hall, 2.2),
                (RoomKind::Dark, if index >= 5 { dark_w } else { 0.0 }),
                (RoomKind::KeyPuzzle, if index >= 8 && i64::from(index) - last_key >= 6 { 1.3 } else { 0.0 }),
            ];
            let weights: Vec<f32> = candidates
                .iter()
                .map(|(k, w)| {
                    let mut w = *w;
                    if Some(*k) == prev && *k == RoomKind::Hall {
                        w = 0.0; // never two halls in a row
                    }
                    if Some(*k) == prev && Some(*k) == prev2 {
                        w = 0.0; // never three of a kind in a row
                    }
                    w
                })
                .collect();
            let i = rng.weighted_index(&weights).unwrap_or(0);
            candidates[i].0
        };
        if kind == RoomKind::KeyPuzzle {
            last_key = i64::from(index);
        }

        // ---- Size ----
        let ((w0, w1), (d0, d1)) = area.size_ranges();
        let (width, depth) = match kind {
            RoomKind::Hall => (3.0, rng.range_f32(10.0, 16.0)),
            RoomKind::Final => (13.0, 15.0),
            RoomKind::Transition => (w1, d0.max(6.0)),
            _ => (rng.range_f32(w0, w1), rng.range_f32(d0, d1)),
        };
        let width = (width * 10.0).round() / 10.0;
        let depth = (depth * 10.0).round() / 10.0;

        // Centre so the entry door fits with a margin; drift gently, pull back toward x = 0.
        let lo = entry_x + DOOR_MARGIN - width / 2.0;
        let hi = entry_x - DOOR_MARGIN + width / 2.0;
        let mut cx = rng.range_f32(lo, hi);
        cx = cx.clamp(lo.max(-6.0).min(hi), hi.min(6.0).max(lo));
        let min = Vec2::new(cx - width / 2.0, south - depth);
        let max = Vec2::new(cx + width / 2.0, south);

        let exit_x = if kind == RoomKind::Final {
            cx
        } else {
            ((rng.range_f32(min.x + DOOR_MARGIN, max.x - DOOR_MARGIN)) * 10.0).round() / 10.0
        };

        let light = match kind {
            RoomKind::Dark => LightState::Dark,
            RoomKind::Transition | RoomKind::Final => LightState::Bright,
            _ => {
                let dim_chance = if area == Area::ServiceTunnels { 0.5 } else { 0.3 };
                if rng.next_f32() < dim_chance { LightState::Dim } else { LightState::Bright }
            }
        };

        // ---- Encounter: at most one per room, never back-to-back rooms ----
        let encounter = if matches!(kind, RoomKind::Final | RoomKind::Transition) || i64::from(index) - last_any < 2 {
            None
        } else {
            let mut chosen = None;
            let order = [
                (Encounter::Whisperer, &cfg.whisperer, light == LightState::Dark),
                (Encounter::Undertow, &cfg.undertow, true),
                (Encounter::Surge, &cfg.surge, true),
                (Encounter::Hush, &cfg.hush, light != LightState::Dark),
                (Encounter::Gallery, &cfg.gallery, light != LightState::Dark && kind != RoomKind::Hall),
            ];
            for (enc, t, allowed) in order {
                let since = i64::from(index) - last_seen.get(&enc).copied().unwrap_or(-1000);
                // Always draw the roll so the RNG stream doesn't depend on earlier outcomes.
                let roll = rng.next_f32();
                if chosen.is_none() && allowed && index >= t.min_room && since >= i64::from(t.cooldown) && roll < t.chance {
                    chosen = Some(enc);
                }
            }
            chosen
        };
        if let Some(e) = encounter {
            last_seen.insert(e, i64::from(index));
            last_any = i64::from(index);
        }
        let surge = encounter.is_some_and(Encounter::needs_hiding);

        let containers = match kind {
            RoomKind::Hall => rng.below(2) as u8,
            RoomKind::Loot => 2 + rng.below(2) as u8,
            RoomKind::KeyPuzzle => 3,
            RoomKind::Final => 0,
            _ => 1 + rng.below(2) as u8,
        };
        let hiding = surge || matches!(kind, RoomKind::Transition) || (kind != RoomKind::Final && rng.next_f32() < 0.45);

        // Mimic door: needs room on the north wall away from the real exit.
        let mimic = if index >= 10 && kind != RoomKind::Hall && kind != RoomKind::Final && rng.next_f32() < 0.16 {
            let left = (min.x + DOOR_MARGIN, exit_x - 1.8);
            let right = (exit_x + 1.8, max.x - DOOR_MARGIN);
            let side = if rng.next_f32() < 0.5 { left } else { right };
            let alt = if side == left { right } else { left };
            let span = if side.1 > side.0 { Some(side) } else if alt.1 > alt.0 { Some(alt) } else { None };
            span.map(|(a, b)| Mimic { x: ((rng.range_f32(a, b)) * 10.0).round() / 10.0, shown_number: wrong_number(&mut rng, index + 1) })
        } else {
            None
        };

        rooms.push(RoomPlan {
            index,
            area,
            kind,
            light,
            min,
            max,
            height: area.ceiling(),
            entry_x,
            exit_x,
            containers,
            hiding,
            encounter,
            mimic,
            seed: rng.next_u64(),
        });
        history.push(kind);
        entry_x = exit_x;
        south = min.y - WALL_T;
    }
    FloorPlan { seed, rooms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_floor() {
        assert_eq!(generate(42), generate(42));
        assert_ne!(generate(42), generate(43));
    }

    #[test]
    fn hundred_doors_in_order() {
        let plan = generate(7);
        assert_eq!(plan.rooms.len() as u32, LAST_ROOM - FIRST_ROOM + 1);
        for (k, r) in plan.rooms.iter().enumerate() {
            assert_eq!(r.index, FIRST_ROOM + k as u32);
        }
        assert_eq!(plan.rooms.last().unwrap().exit_door(), 100);
        assert_eq!(plan.rooms.last().unwrap().kind, RoomKind::Final);
    }

    #[test]
    fn doors_fit_both_rooms_and_rooms_chain() {
        for seed in 0..50 {
            let plan = generate(seed);
            assert_eq!(plan.rooms[0].max.y, START_Z);
            for w in plan.rooms.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                assert!((b.max.y - (a.min.y - WALL_T)).abs() < 1e-4, "rooms must chain");
                assert_eq!(b.entry_x, a.exit_x);
                for r in [a, b] {
                    let x = if r.index == a.index { a.exit_x } else { b.entry_x };
                    assert!(x - DOOR_W_HALF >= r.min.x - 1e-3 && x + DOOR_W_HALF <= r.max.x + 1e-3, "seed {seed} room {} door outside", r.index);
                }
            }
        }
    }
    const DOOR_W_HALF: f32 = 0.5;

    #[test]
    fn areas_follow_the_floor() {
        let plan = generate(1);
        assert_eq!(plan.room(2).unwrap().area, Area::GuestWing);
        assert_eq!(plan.room(21).unwrap().kind, RoomKind::Transition);
        assert_eq!(plan.room(45).unwrap().area, Area::ServiceTunnels);
        assert_eq!(plan.room(70).unwrap().area, Area::QuarryLift);
        assert_eq!(plan.room(99).unwrap().area, Area::Atrium);
    }

    #[test]
    fn no_three_in_a_row_and_no_double_halls() {
        for seed in 0..100 {
            let plan = generate(seed);
            for w in plan.rooms.windows(3) {
                assert!(!(w[0].kind == w[1].kind && w[1].kind == w[2].kind), "seed {seed}");
            }
            for w in plan.rooms.windows(2) {
                assert!(!(w[0].kind == RoomKind::Hall && w[1].kind == RoomKind::Hall), "seed {seed}");
            }
        }
    }

    #[test]
    fn surge_rooms_have_hiding_and_cooldown() {
        let mut total = 0;
        for seed in 0..100 {
            let plan = generate(seed);
            let surges: Vec<u32> = plan.rooms.iter().filter(|r| r.encounter == Some(Encounter::Surge)).map(|r| r.index).collect();
            total += surges.len();
            for r in plan.rooms.iter().filter(|r| r.surge()) {
                assert!(r.hiding, "surge room {} must have a hiding spot", r.index);
                assert!(r.index >= 6);
            }
            for w in surges.windows(2) {
                assert!(w[1] - w[0] >= 6, "seed {seed}: surges too close");
            }
        }
        assert!(total > 100, "surge should appear several times per floor on average");
    }

    #[test]
    fn encounters_respect_their_rules() {
        let cfg = EntityConfig::default();
        let mut seen = std::collections::HashMap::new();
        for seed in 0..200 {
            let plan = generate(seed);
            let mut prev: Option<u32> = None;
            for r in &plan.rooms {
                let Some(e) = r.encounter else { continue };
                *seen.entry(e).or_insert(0) += 1;
                if let Some(p) = prev {
                    assert!(r.index - p >= 2, "encounters in adjacent rooms");
                }
                prev = Some(r.index);
                match e {
                    Encounter::Whisperer => assert_eq!(r.light, LightState::Dark),
                    Encounter::Gallery | Encounter::Hush => assert_ne!(r.light, LightState::Dark),
                    Encounter::Undertow => assert!(r.index >= cfg.undertow.min_room && r.hiding),
                    Encounter::Surge => assert!(r.hiding),
                }
            }
        }
        for e in [Encounter::Surge, Encounter::Undertow, Encounter::Whisperer, Encounter::Gallery, Encounter::Hush] {
            assert!(seen.get(&e).copied().unwrap_or(0) > 20, "{e:?} too rare: {seen:?}");
        }
    }

    #[test]
    fn mimics_never_show_the_real_number() {
        for seed in 0..100 {
            for r in generate(seed).rooms {
                if let Some(m) = r.mimic {
                    assert_ne!(m.shown_number, r.exit_door());
                    assert!((m.x - r.exit_x).abs() >= 1.7, "mimic overlaps real door");
                    assert!(m.x - 0.5 >= r.min.x && m.x + 0.5 <= r.max.x);
                }
            }
        }
    }

    #[test]
    fn room_lookup_by_z() {
        let plan = generate(3);
        for r in &plan.rooms {
            let mid = (r.min.y + r.max.y) / 2.0;
            assert_eq!(plan.room_at_z(mid), Some(r.index));
        }
        assert_eq!(plan.room_at_z(0.0), None);
    }
}
