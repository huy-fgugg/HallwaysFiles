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
/// Door geometry (must match `world::Builder::door`).
pub const DOOR_W: f32 = 1.0;
pub const DOOR_H: f32 = 2.1;
/// Casing and top cap stick out this far past each side of the opening.
pub const DOOR_CASING: f32 = 0.215;
/// The leaf opens this far (radians) into the room beyond the wall.
pub const DOOR_SWING: f32 = 1.75;
/// Leaf length (hinge to tip).
pub const DOOR_LEAF: f32 = DOOR_W - 0.04;
/// Space kept between a door's casing and the side wall.
const DOOR_SIDE_CLEAR: f32 = 0.2;
/// Room-generation rule for every door at x = d in a wall spanning
/// [x_min, x_max]:
///
///   x_min + m <= d <= x_max - m,
///   m = W/2 + max(casing, leaf * max(0, -cos(swing))) + side clearance.
///
/// The leaf overshoots its hinge by leaf * -cos(swing) when fully open, so m
/// covers both the casing and the open leaf. See `door_margin()`; a test
/// checks this constant against the formula.
const DOOR_MARGIN: f32 = 0.95;

/// The margin from the formula above (in metres).
pub fn door_margin() -> f32 {
    let overshoot = DOOR_LEAF * (-DOOR_SWING.cos()).max(0.0);
    DOOR_W / 2.0 + DOOR_CASING.max(overshoot) + DOOR_SIDE_CLEAR
}

/// A floor rectangle (x, z) that must stay empty.
pub type Zone = (Vec2, Vec2);

/// True when two floor rectangles overlap.
pub fn zones_overlap(a: &Zone, b: &Zone) -> bool {
    a.0.x < b.1.x && a.1.x > b.0.x && a.0.y < b.1.y && a.1.y > b.0.y
}

/// Floor areas around this room's doors that furniture, columns, posts and
/// hazards must never touch:
///
/// * the entry door's swing: its leaf opens into this room, sweeping a quarter
///   disc of radius `leaf` from the hinge at x0 = entry_x - W/2, and its tip
///   ends up `leaf * -cos(swing)` west of the hinge;
/// * a 1.2 m standing area in front of the exit door and every fake door.
pub fn door_zones(r: &RoomPlan) -> Vec<Zone> {
    let hw = DOOR_W / 2.0;
    let overshoot = DOOR_LEAF * (-DOOR_SWING.cos()).max(0.0);
    let x0 = r.entry_x - hw;
    let mut z = vec![(Vec2::new(x0 - overshoot - 0.25, r.max.y - DOOR_LEAF - 0.3), Vec2::new(r.entry_x + hw + 0.25, r.max.y))];
    for x in std::iter::once(r.exit_x).chain(r.mimics.iter().map(|m| m.x)) {
        z.push((Vec2::new(x - hw - 0.3, r.min.y), Vec2::new(x + hw + 0.3, r.min.y + 1.2)));
    }
    z
}
/// Minimum centre-to-centre distance between doors on a choice wall.
pub const FAKE_SPACING: f32 = 1.8;
/// Rooms of the Door 40 chase: The Hollow pursues you from Room 041 to 046.
pub const PURSUIT_FIRST: u32 = 41;
pub const PURSUIT_LAST: u32 = 46;
/// Chase rooms whose exit wall has a fake door beside the real one.
pub const CHASE_CHOICE_ROOMS: [u32; 2] = [43, 45];
/// Room entered through Door 050: the Reading Hall, a big archive room whose
/// exit is locked until you find the key among its shelves.
pub const READING_HALL: u32 = 50;
/// Width of the opening in a divider (a wall, barricade or shelf row).
pub const DIVIDER_GAP: f32 = 1.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Area {
    GuestWing,
    Archive,
    ServiceTunnels,
    /// Outdoors: walled night gardens under an open sky.
    Grounds,
    QuarryLift,
    Atrium,
}

impl Area {
    pub const ALL: [Area; 6] = [Area::GuestWing, Area::Archive, Area::ServiceTunnels, Area::Grounds, Area::QuarryLift, Area::Atrium];

    /// No ceiling: open sky.
    pub fn outdoors(self) -> bool {
        self == Area::Grounds
    }

    pub fn for_room(i: u32) -> Area {
        match i {
            0..=20 => Area::GuestWing,
            21..=40 => Area::Archive,
            41..=50 => Area::ServiceTunnels,
            51..=60 => Area::Grounds,
            61..=80 => Area::QuarryLift,
            _ => Area::Atrium,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Area::GuestWing => "The Guest Wing",
            Area::Archive => "The Archive",
            Area::ServiceTunnels => "The Service Tunnels",
            Area::Grounds => "The Grounds",
            Area::QuarryLift => "The Quarry Lift",
            Area::Atrium => "The Atrium",
        }
    }

    pub fn ceiling(self) -> f32 {
        match self {
            Area::GuestWing => 3.0,
            Area::Archive => 4.2,
            Area::ServiceTunnels => 2.8,
            Area::Grounds => 3.6,
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
            Area::Grounds => ((11.0, 16.0), (10.0, 16.0)),
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
    /// Outdoors: a blind patroller that hears footsteps. Crouch.
    Listener,
}

impl Encounter {
    /// Encounters whose counter is hiding.
    pub fn needs_hiding(self) -> bool {
        matches!(self, Encounter::Surge | Encounter::Undertow)
    }
}

/// Unusual architecture that turns a room into a small climb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Feature {
    /// Stairs climb to a landing; the exit door sits `height` above the floor
    /// and the next room starts on a matching landing with stairs down.
    RaisedExit { height: f32 },
    /// The key waits in a chest on a loft up a staircase along a side wall.
    KeyLoft { east: bool },
    /// Walls across the room with an opening at alternating ends: the way
    /// through turns back and forth.
    Zigzag,
    /// The Reading Hall (see [`READING_HALL`]): rows of tall shelves.
    ReadingHall,
}

/// A line across the room (constant z) that is solid except for an opening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Divider {
    pub z: f32,
    /// Opening, x from..to.
    pub gap: (f32, f32),
}

pub const LOFT_HEIGHT: f32 = 1.6;
pub const STEP_RISE: f32 = 0.2;
pub const STEP_RUN: f32 = 0.3;
pub const LANDING_DEPTH: f32 = 1.3;

/// Number of steps for a climb of `height`.
pub fn steps_for(height: f32) -> usize {
    (height / STEP_RISE).ceil() as usize
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
    /// Fake doors on the north wall (wrong numbers; opening one hurts).
    pub mimics: Vec<Mimic>,
    pub feature: Option<Feature>,
    /// Part of the Door 40 chase (no locks, fake doors or other entities).
    pub pursuit: bool,
    /// Entry landing height (when the previous room's exit is raised).
    pub entry_height: f32,
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

    /// Lines across the room that force the way through to turn: walls in
    /// zigzag rooms, barricades in the chase, shelf rows in the Reading Hall.
    pub fn dividers(&self) -> Vec<Divider> {
        let max_n = match self.feature {
            Some(Feature::Zigzag) => 3,
            Some(Feature::ReadingHall) => 4,
            _ if self.pursuit => 4,
            _ => return vec![],
        };
        // Keep clear of the entry landing and its stairs down.
        let stairs = if self.entry_height > 0.0 { LANDING_DEPTH + self.entry_height / STEP_RISE * STEP_RUN + 0.6 } else { 0.0 };
        let (z_s, z_n) = (self.max.y - 2.6 - stairs, self.min.y + 2.6);
        if z_s <= z_n || self.width() < DIVIDER_GAP + 2.0 {
            return vec![];
        }
        let n = (((z_s - z_n) / 3.2).floor() as usize + 1).clamp(1, max_n);
        let mut east = self.seed & 1 == 1;
        (0..n)
            .map(|i| {
                let z = if n == 1 { (z_s + z_n) / 2.0 } else { z_s - i as f32 * (z_s - z_n) / (n - 1) as f32 };
                let gap = if east { (self.max.x - DIVIDER_GAP, self.max.x) } else { (self.min.x, self.min.x + DIVIDER_GAP) };
                east = !east;
                Divider { z, gap }
            })
            .collect()
    }

    /// Walking route through the room at floor level, from just inside the
    /// entry door, through every divider opening, to just before the exit.
    pub fn route(&self) -> Vec<Vec2> {
        let mut pts = vec![Vec2::new(self.entry_x, self.max.y - 0.6)];
        for d in self.dividers() {
            let gx = (d.gap.0 + d.gap.1) / 2.0;
            pts.push(Vec2::new(gx, d.z + 0.9));
            pts.push(Vec2::new(gx, d.z - 0.9));
        }
        // A raised exit is reached up its stairs: line up at their foot first.
        if let Some(Feature::RaisedExit { height }) = self.feature {
            pts.push(Vec2::new(self.exit_x, self.min.y + LANDING_DEPTH + height / STEP_RISE * STEP_RUN + 0.6));
        }
        pts.push(Vec2::new(self.exit_x, self.min.y + 0.6));
        pts
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorPlan {
    pub seed: u64,
    pub rooms: Vec<RoomPlan>,
    /// Last generated room (the final door is one higher).
    pub last_room: u32,
}

impl FloorPlan {
    pub fn final_door(&self) -> u32 {
        self.last_room + 1
    }

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

/// Knobs for custom floors (presets). `Default` is the classic 100-door floor.
#[derive(Debug, Clone, PartialEq)]
pub struct GenOptions {
    /// Last generated room; the final door is `last_room + 1`.
    pub last_room: u32,
    /// Areas in order, sharing the floor evenly. `None` = classic layout.
    pub areas: Option<Vec<Area>>,
    /// Room size multiplier.
    pub size: f32,
    /// Multiplier on the chance of dim and dark rooms.
    pub darkness: f32,
    pub fake_doors: f32,
    pub stairs: f32,
    pub loot: f32,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self { last_room: LAST_ROOM, areas: None, size: 1.0, darkness: 1.0, fake_doors: 1.0, stairs: 1.0, loot: 1.0 }
    }
}

impl GenOptions {
    pub fn area_for(&self, index: u32) -> Area {
        match &self.areas {
            Some(list) if !list.is_empty() => {
                let count = (self.last_room + 1 - FIRST_ROOM).max(1);
                let k = ((index.saturating_sub(FIRST_ROOM)) as usize * list.len() / count as usize).min(list.len() - 1);
                list[k]
            }
            _ => Area::for_room(index),
        }
    }
}

pub fn generate(seed: u64) -> FloorPlan {
    generate_with(seed, &EntityConfig::default())
}

pub fn generate_with(seed: u64, cfg: &EntityConfig) -> FloorPlan {
    generate_custom(seed, cfg, &GenOptions::default())
}

pub fn generate_custom(seed: u64, cfg: &EntityConfig, opt: &GenOptions) -> FloorPlan {
    let last = opt.last_room.clamp(FIRST_ROOM + 4, LAST_ROOM);
    let mut rooms: Vec<RoomPlan> = Vec::new();
    let mut entry_x = 0.0f32;
    let mut south = START_Z;
    let mut history: Vec<RoomKind> = Vec::new();
    let mut last_seen: std::collections::HashMap<Encounter, i64> = std::collections::HashMap::new();
    let mut last_any: i64 = -100;
    let mut last_key: i64 = -100;
    let mut pending_entry = 0.0f32;

    for index in FIRST_ROOM..=last {
        let mut rng = GameRng::fork(seed, u64::from(index));
        let area = opt.area_for(index);
        let first_of_area = index == FIRST_ROOM || opt.area_for(index - 1) != area;

        // ---- Kind (weighted, with cooldowns) ----
        let kind = if index == last {
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
                (RoomKind::Dark, if index >= 5 { dark_w * opt.darkness.clamp(0.0, 3.0) } else { 0.0 }),
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
        // The chase rooms must never be locked.
        let pursuit = (PURSUIT_FIRST..=PURSUIT_LAST).contains(&index) && last >= PURSUIT_LAST + 2;
        let kind = if pursuit && kind == RoomKind::KeyPuzzle {
            // Swap the lock for something that keeps the no-three-in-a-row rule.
            let prev = history.last().copied();
            let prev2 = history.iter().rev().nth(1).copied();
            [RoomKind::Loot, RoomKind::Dark, RoomKind::Hall]
                .into_iter()
                .find(|k| !(Some(*k) == prev && Some(*k) == prev2) && !(*k == RoomKind::Hall && prev == Some(RoomKind::Hall)))
                .unwrap_or(RoomKind::Loot)
        } else {
            kind
        };
        let reading_hall = index == READING_HALL && last >= READING_HALL + 2 && !first_of_area && !pursuit;
        let kind = if reading_hall { RoomKind::KeyPuzzle } else { kind };
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
        // Outdoor gardens: wide, and some stretch out much longer.
        let (width, depth) = if area == Area::Grounds && kind == RoomKind::Hall {
            (6.0, depth)
        } else if area == Area::Grounds && !matches!(kind, RoomKind::Final | RoomKind::Transition) && rng.next_f32() < 0.35 {
            (width, (depth * 1.5).min(24.0))
        } else {
            (width, depth)
        };
        // The chase runs through big rooms; the Reading Hall is the biggest.
        let (width, depth) = if reading_hall {
            (13.0, 19.0)
        } else if pursuit {
            (rng.range_f32(9.0, 12.0), rng.range_f32(14.0, 19.0))
        } else {
            (width, depth)
        };
        // Custom size, never so small that doors or stairs stop fitting.
        let k = opt.size.clamp(0.5, 1.8);
        let (width, depth) = if kind == RoomKind::Hall { (width, depth * k) } else { ((width * k).max(4.0), (depth * k).max(5.0)) };
        let width = (width * 10.0).round() / 10.0;
        let depth = (depth * 10.0).round() / 10.0;
        // Rooms entered from a raised door need space for the stairs down.
        let entry_height = std::mem::take(&mut pending_entry);
        let depth = if entry_height > 0.0 { depth.max(7.0) } else { depth };

        // Centre so the entry door fits with a margin; drift gently, pull back toward x = 0.
        let lo = entry_x + DOOR_MARGIN - width / 2.0;
        let hi = entry_x - DOOR_MARGIN + width / 2.0;
        let mut cx = rng.range_f32(lo, hi);
        cx = cx.clamp(lo.max(-6.0).min(hi), hi.min(6.0).max(lo));
        let min = Vec2::new(cx - width / 2.0, south - depth);
        let max = Vec2::new(cx + width / 2.0, south);

        // Doors snap to 10 cm, always inward so they stay inside the margin.
        let snap = |x: f32| {
            let (lo, hi) = ((min.x + DOOR_MARGIN) * 10.0, (max.x - DOOR_MARGIN) * 10.0);
            if lo.ceil() <= hi.floor() { (x * 10.0).round().clamp(lo.ceil(), hi.floor()) / 10.0 } else { (min.x + max.x) / 2.0 }
        };
        let exit_x = if kind == RoomKind::Final { cx } else { snap(rng.range_f32(min.x + DOOR_MARGIN, max.x - DOOR_MARGIN)) };

        let light = match kind {
            _ if reading_hall => LightState::Dim,
            RoomKind::Dark => LightState::Dark,
            RoomKind::Transition | RoomKind::Final => LightState::Bright,
            _ => {
                let dim_chance = (if area == Area::ServiceTunnels { 0.5 } else { 0.3 }) * opt.darkness.clamp(0.0, 3.0);
                if rng.next_f32() < dim_chance { LightState::Dim } else { LightState::Bright }
            }
        };

        // ---- Encounter: at most one per room, never back-to-back rooms ----
        let encounter = if pursuit || reading_hall || matches!(kind, RoomKind::Final | RoomKind::Transition) || i64::from(index) - last_any < 2 {
            None
        } else {
            let mut chosen = None;
            let order = [
                (Encounter::Listener, &cfg.listener, matches!(area, Area::Grounds | Area::Archive) && light != LightState::Dark),
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
                if chosen.is_none() && allowed && !(entry_height > 0.0 && enc.needs_hiding()) && index >= t.min_room && since >= i64::from(t.cooldown) && roll < t.chance {
                    chosen = Some(enc);
                }
            }
            chosen
        };
        if let Some(e) = encounter {
            last_seen.insert(e, i64::from(index));
            last_any = i64::from(index);
        }
        // A Surge/Undertow keeps making passes into the next rooms: keep it out
        // of the two rooms before the chase.
        let near_chase = last >= PURSUIT_LAST + 2 && (PURSUIT_FIRST - 2..PURSUIT_FIRST).contains(&index);
        let encounter = if near_chase && encounter.is_some_and(Encounter::needs_hiding) { None } else { encounter };
        let surge = encounter.is_some_and(Encounter::needs_hiding);

        let containers = match kind {
            RoomKind::Hall => rng.below(2) as u8,
            RoomKind::Loot => 2 + rng.below(2) as u8,
            RoomKind::KeyPuzzle if reading_hall => 7,
            RoomKind::KeyPuzzle => 3,
            RoomKind::Final => 0,
            _ => 1 + rng.below(2) as u8,
        };
        let containers = if kind == RoomKind::KeyPuzzle { containers } else { ((containers as f32 * opt.loot.clamp(0.0, 3.0)).round() as u8).min(5) };
        let hiding = surge || matches!(kind, RoomKind::Transition) || (kind != RoomKind::Final && rng.next_f32() < 0.45);

        // ---- Weird architecture ----
        let feature_roll = rng.next_f32();
        let zig_roll = rng.next_f32();
        let feature = if pursuit {
            None
        } else if reading_hall {
            Some(Feature::ReadingHall)
        } else if kind == RoomKind::KeyPuzzle && width >= 5.5 && depth >= 7.6 && feature_roll < 0.6 * opt.stairs.clamp(0.0, 1.6) {
            Some(Feature::KeyLoft { east: rng.next_f32() < 0.5 })
        } else if index >= 8
            && index + 2 < last
            && entry_height == 0.0
            && !surge
            && matches!(kind, RoomKind::Loot | RoomKind::Dark)
            && depth >= 7.5
            && feature_roll < 0.14 * opt.stairs.clamp(0.0, 6.0)
        {
            let height = if rng.next_f32() < 0.5 { 1.2 } else { 1.6 };
            Some(Feature::RaisedExit { height })
        } else if index >= 6
            && entry_height == 0.0
            && !matches!(area, Area::Grounds | Area::Atrium)
            && encounter.is_none_or(|e| e != Encounter::Listener)
            && matches!(kind, RoomKind::Loot | RoomKind::Dark)
            && width >= 5.0
            && depth >= 8.0
            && zig_roll < 0.16 * opt.stairs.clamp(0.0, 4.0)
        {
            Some(Feature::Zigzag)
        } else {
            None
        };
        if let Some(Feature::RaisedExit { height }) = feature {
            pending_entry = height;
        }

        // Fake doors: a "choice wall" of look-alike doors around the real exit.
        // Only the real door shows the next number; every fake shows a wrong one.
        // Two chase rooms end in a choice of doors; the guiding light marks the real one.
        let chase_choice = pursuit && CHASE_CHOICE_ROOMS.contains(&index) && opt.fake_doors > 0.0;
        let mimics = if chase_choice || (index >= 5 && !pursuit && feature.is_none() && !matches!(kind, RoomKind::Hall | RoomKind::Final | RoomKind::Transition)) {
            let chance = if chase_choice { 1.0 } else { (if index < 20 { 0.06 } else { 0.11 }) * opt.fake_doors.clamp(0.0, 8.0) };
            if rng.next_f32() < chance {
                // Candidate slots along the north wall, clear of the real exit.
                let mut slots = Vec::new();
                let mut x = min.x + DOOR_MARGIN;
                while x <= max.x - DOOR_MARGIN + 1e-3 {
                    let sx = snap(x);
                    if (sx - exit_x).abs() >= FAKE_SPACING {
                        slots.push(sx);
                    }
                    x += FAKE_SPACING;
                }
                // Shuffle deterministically, then keep fakes that don't overlap each other.
                for i in (1..slots.len()).rev() {
                    let j = rng.below(i as u32 + 1) as usize;
                    slots.swap(i, j);
                }
                let want = if chase_choice { 1 } else { 1 + rng.below(3) as usize };
                let mut picked: Vec<Mimic> = Vec::new();
                for sx in slots {
                    if picked.len() >= want {
                        break;
                    }
                    if picked.iter().all(|m| (m.x - sx).abs() >= FAKE_SPACING) {
                        let mut n = wrong_number(&mut rng, index + 1);
                        for _ in 0..6 {
                            if picked.iter().all(|m| m.shown_number != n) {
                                break;
                            }
                            n = wrong_number(&mut rng, index + 1);
                        }
                        picked.push(Mimic { x: sx, shown_number: n });
                    }
                }
                picked.sort_by(|a, b| a.x.total_cmp(&b.x));
                picked
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        rooms.push(RoomPlan {
            index,
            area: if reading_hall { Area::Archive } else { area },
            kind,
            light,
            min,
            max,
            height: {
                let climb = match feature {
                    Some(Feature::RaisedExit { height }) => height,
                    _ => 0.0,
                }
                .max(entry_height);
                if climb > 0.0 { area.ceiling().max(climb + 2.9) } else { area.ceiling() }
            },
            entry_x,
            exit_x,
            containers,
            hiding,
            encounter,
            mimics,
            feature,
            pursuit,
            entry_height,
            seed: rng.next_u64(),
        });
        history.push(kind);
        entry_x = exit_x;
        south = min.y - WALL_T;
    }
    ensure_every_encounter(&mut rooms, cfg, seed);
    FloorPlan { seed, rooms, last_room: last }
}

/// Every entity that can appear (chance > 0) appears at least once per floor:
/// any that the dice skipped is placed in a suitable free room, keeping the
/// usual rules (never in adjacent rooms, not in the chase or transition rooms).
fn ensure_every_encounter(rooms: &mut [RoomPlan], cfg: &EntityConfig, seed: u64) {
    let all = [
        (Encounter::Surge, &cfg.surge),
        (Encounter::Undertow, &cfg.undertow),
        (Encounter::Whisperer, &cfg.whisperer),
        (Encounter::Gallery, &cfg.gallery),
        (Encounter::Hush, &cfg.hush),
        (Encounter::Listener, &cfg.listener),
    ];
    let mut rng = GameRng::fork(seed, 0xE11C_0000);
    for (enc, t) in all {
        if t.chance <= 0.0 || rooms.iter().any(|r| r.encounter == Some(enc)) {
            continue;
        }
        // strict: a free room; otherwise a room holding an entity that already
        // appears more than once (its neighbours are already clear).
        let ok = |i: usize, rooms: &[RoomPlan], strict: bool| {
            let r = &rooms[i];
            let free_near = (i == 0 || rooms[i - 1].encounter.is_none()) && rooms.get(i + 1).is_none_or(|n| n.encounter.is_none());
            let slot_ok = if strict {
                r.encounter.is_none() && free_near
            } else {
                r.encounter.is_some_and(|e| rooms.iter().filter(|x| x.encounter == Some(e)).count() >= 2)
            };
            let fits = match enc {
                Encounter::Whisperer => r.light == LightState::Dark,
                Encounter::Gallery => r.light != LightState::Dark && r.kind != RoomKind::Hall,
                Encounter::Hush => r.light != LightState::Dark,
                Encounter::Listener => matches!(r.area, Area::Grounds | Area::Archive) && r.light != LightState::Dark,
                Encounter::Surge | Encounter::Undertow => r.entry_height == 0.0 && !(PURSUIT_FIRST - 2..PURSUIT_FIRST).contains(&r.index),
            };
            slot_ok
                && !r.pursuit
                && r.feature != Some(Feature::ReadingHall)
                && fits
                && r.index >= t.min_room
                && !matches!(r.kind, RoomKind::Final | RoomKind::Transition)
        };
        let mut candidates: Vec<usize> = (0..rooms.len()).filter(|i| ok(*i, rooms, true)).collect();
        if candidates.is_empty() {
            candidates = (0..rooms.len()).filter(|i| ok(*i, rooms, false)).collect();
        }
        let mut cleared_neighbours = false;
        if candidates.is_empty() {
            // Last resort: a free room whose only neighbouring entities are
            // repeats; those neighbours step aside.
            let dup = |e: Encounter, rooms: &[RoomPlan]| rooms.iter().filter(|x| x.encounter == Some(e)).count() >= 2;
            candidates = (0..rooms.len())
                .filter(|&i| {
                    let mut probe = rooms.to_vec();
                    let near = [i.checked_sub(1), Some(i + 1)];
                    let ok_near = near.iter().flatten().all(|&n| probe.get(n).is_none_or(|x| x.encounter.is_none_or(|e| dup(e, rooms))));
                    for &n in near.iter().flatten() {
                        if let Some(x) = probe.get_mut(n) {
                            x.encounter = None;
                        }
                    }
                    ok_near && ok(i, &probe, true)
                })
                .collect();
            cleared_neighbours = true;
        }
        if candidates.is_empty() {
            continue;
        }
        let i = candidates[rng.below(candidates.len() as u32) as usize];
        if cleared_neighbours {
            for n in [i.checked_sub(1), Some(i + 1)].into_iter().flatten() {
                if let Some(x) = rooms.get_mut(n) {
                    x.encounter = None;
                }
            }
        }
        rooms[i].encounter = Some(enc);
        // The Listener patrols the whole room: no zigzag walls in its way.
        if enc == Encounter::Listener && rooms[i].feature == Some(Feature::Zigzag) {
            rooms[i].feature = None;
        }
        if enc.needs_hiding() {
            rooms[i].hiding = true;
        }
    }
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
    fn door_margin_matches_the_formula() {
        assert!(DOOR_MARGIN + 1e-4 >= door_margin(), "margin {DOOR_MARGIN} < formula {}", door_margin());
        // The open leaf's overshoot stays inside the casing allowance.
        assert!(DOOR_LEAF * (-DOOR_SWING.cos()) < DOOR_CASING + DOOR_SIDE_CLEAR);
    }

    fn option_sets() -> Vec<GenOptions> {
        vec![
            GenOptions::default(),
            GenOptions { size: 0.5, ..default_opts() },
            GenOptions { size: 1.8, ..default_opts() },
            GenOptions { fake_doors: 8.0, stairs: 6.0, ..default_opts() },
            GenOptions { last_room: 30, areas: Some(vec![Area::Grounds, Area::ServiceTunnels, Area::Atrium]), ..default_opts() },
        ]
    }
    /// Every door on every floor obeys the room-generation rule: inside its
    /// wall by the door margin, tall enough rooms on both sides, fake doors
    /// never overlapping, and door swings / door fronts never cut by a
    /// divider or a loft.
    #[test]
    fn every_door_obeys_the_generation_rule() {
        let m = door_margin();
        for opt in option_sets() {
            for seed in 0..300u64 {
                let plan = generate_custom(seed, &EntityConfig::default(), &opt);
                for (k, r) in plan.rooms.iter().enumerate() {
                    let tag = format!("seed {seed} room {} opt {opt:?}", r.index);
                    for x in [r.entry_x, r.exit_x].into_iter().chain(r.mimics.iter().map(|mm| mm.x)) {
                        assert!(x >= r.min.x + m - 1e-3 && x <= r.max.x - m + 1e-3, "{tag}: door at {x} outside [{}, {}]", r.min.x, r.max.x);
                    }
                    let mut xs: Vec<f32> = r.mimics.iter().map(|mm| mm.x).chain([r.exit_x]).collect();
                    xs.sort_by(f32::total_cmp);
                    for w in xs.windows(2) {
                        assert!(w[1] - w[0] >= DOOR_W + 2.0 * DOOR_CASING - 1e-3, "{tag}: doors overlap");
                    }
                    // A raised exit's opening and sign fit under both ceilings.
                    if let Some(Feature::RaisedExit { height }) = r.feature {
                        let top = height + DOOR_H + 0.46;
                        assert!(top <= r.height + 1e-3, "{tag}: raised door taller than the room");
                        let next = &plan.rooms[k + 1];
                        assert!((next.entry_height - height).abs() < 1e-4 && top <= next.height + 1e-3, "{tag}: next room can't take the raised door");
                    }
                    let zones = door_zones(r);
                    for zn in &zones {
                        assert!(zn.0.y >= r.min.y - 1e-3 && zn.1.y <= r.max.y + 1e-3, "{tag}: door zone leaves the room");
                        for d in r.dividers() {
                            let line = (Vec2::new(r.min.x, d.z - 0.15), Vec2::new(r.max.x, d.z + 0.15));
                            assert!(!zones_overlap(zn, &line), "{tag}: divider cuts a door zone");
                        }
                        if let Some(Feature::KeyLoft { east }) = r.feature {
                            let (x0, x1) = if east { (r.max.x - 1.7, r.max.x) } else { (r.min.x, r.min.x + 1.7) };
                            let loft = (Vec2::new(x0, r.min.y + 1.6), Vec2::new(x1, r.min.y + 3.8 + LOFT_HEIGHT / STEP_RISE * STEP_RUN));
                            assert!(!zones_overlap(zn, &loft), "{tag}: loft blocks a door");
                        }
                    }
                }
            }
        }
    }

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
                    Encounter::Listener => assert_eq!(r.area, Area::Grounds),
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
                for (i, m) in r.mimics.iter().enumerate() {
                    assert_ne!(m.shown_number, r.exit_door());
                    assert!((m.x - r.exit_x).abs() >= 1.7, "mimic overlaps real door");
                    assert!(m.x - 0.5 >= r.min.x && m.x + 0.5 <= r.max.x);
                    for o in &r.mimics[i + 1..] {
                        assert!((o.x - m.x).abs() >= 1.7, "fake doors overlap");
                    }
                }
            }
        }
    }

    #[test]
    fn choice_walls_are_common_later() {
        let mut rooms_with_fakes = 0;
        let mut multi = 0;
        for seed in 0..20 {
            for r in generate(seed).rooms {
                if !r.mimics.is_empty() {
                    rooms_with_fakes += 1;
                    assert!(r.index >= 5);
                }
                if r.mimics.len() >= 2 {
                    multi += 1;
                }
            }
        }
        assert!(rooms_with_fakes > 60 && rooms_with_fakes < 400, "fake doors should be rare but present: {rooms_with_fakes}");
        assert!(multi > 15, "multi-door choice walls too rare: {multi}");
    }

    #[test]
    fn raised_exits_lead_to_landings() {
        let mut raised = 0;
        let mut lofts = 0;
        for seed in 0..30 {
            let plan = generate(seed);
            for (i, r) in plan.rooms.iter().enumerate() {
                match r.feature {
                    Some(Feature::RaisedExit { height }) => {
                        raised += 1;
                        let next = &plan.rooms[i + 1];
                        assert_eq!(next.entry_height, height);
                        assert!(next.depth() >= 6.95);
                        assert!(r.depth() >= 7.45);
                        assert!(r.mimics.is_empty());
                        assert!(!r.surge() && !next.surge(), "stair rooms need hiding space");
                        assert!(next.feature.is_none() || !matches!(next.feature, Some(Feature::RaisedExit { .. })));
                    }
                    Some(Feature::KeyLoft { .. }) => {
                        lofts += 1;
                        assert_eq!(r.kind, RoomKind::KeyPuzzle);
                    }
                    Some(Feature::Zigzag | Feature::ReadingHall) | None => {}
                }
                if i == 0 {
                    assert_eq!(r.entry_height, 0.0);
                }
            }
        }
        assert!(raised > 20, "raised exits too rare: {raised}");
        assert!(lofts > 10, "key lofts too rare: {lofts}");
    }

    #[test]
    fn custom_floors() {
        let opt = GenOptions { last_room: 24, areas: Some(vec![Area::Grounds, Area::Atrium]), size: 1.3, ..default_opts() };
        let plan = generate_custom(5, &EntityConfig::default(), &opt);
        assert_eq!(plan.last_room, 24);
        assert_eq!(plan.final_door(), 25);
        assert_eq!(plan.rooms.len(), 23);
        assert_eq!(plan.rooms.last().unwrap().kind, RoomKind::Final);
        assert!(plan.rooms.iter().take(11).all(|r| r.area == Area::Grounds));
        assert!(plan.rooms.iter().skip(12).all(|r| r.area == Area::Atrium));
        // Deterministic, and different options make a different floor.
        assert_eq!(plan, generate_custom(5, &EntityConfig::default(), &opt));
        assert_ne!(plan, generate(5));
        // No fake doors when they're turned off; lots when turned up.
        let none = generate_custom(5, &EntityConfig::default(), &GenOptions { fake_doors: 0.0, ..default_opts() });
        assert!(none.rooms.iter().all(|r| r.mimics.is_empty()));
    }

    #[test]
    fn gardens_are_wide_and_some_are_long() {
        let mut long = 0;
        for seed in 0..10 {
            for r in generate(seed).rooms.iter().filter(|r| r.area == Area::Grounds && r.kind != RoomKind::Final) {
                assert!(r.width() >= 5.9, "garden too narrow: {}", r.width());
                if r.depth() > 16.5 {
                    long += 1;
                }
            }
        }
        assert!(long >= 5, "some gardens should be long: {long}");
    }

    #[test]
    fn classic_floor_has_outdoor_grounds() {
        let plan = generate(1);
        assert_eq!(plan.room(55).unwrap().area, Area::Grounds);
        assert_eq!(plan.room(45).unwrap().area, Area::ServiceTunnels);
        assert_eq!(plan.final_door(), FINAL_DOOR);
    }

    fn default_opts() -> GenOptions {
        GenOptions::default()
    }

    #[test]
    fn every_monster_appears_every_run() {
        let all = [Encounter::Surge, Encounter::Undertow, Encounter::Whisperer, Encounter::Gallery, Encounter::Hush, Encounter::Listener];
        for seed in 0..200 {
            let plan = generate(seed);
            for e in all {
                assert!(plan.rooms.iter().any(|r| r.encounter == Some(e)), "seed {seed} has no {e:?}");
            }
        }
    }

    #[test]
    fn chase_rooms_are_clear() {
        for seed in 0..30 {
            let plan = generate(seed);
            for i in PURSUIT_FIRST..=PURSUIT_LAST {
                let r = plan.room(i).unwrap();
                assert!(r.pursuit);
                assert!(r.encounter.is_none() && r.feature.is_none());
                assert_eq!(r.mimics.len(), usize::from(CHASE_CHOICE_ROOMS.contains(&i)), "room {i}");
                assert!(r.dividers().len() >= 2, "chase rooms are big with obstacles");
                assert_ne!(r.kind, RoomKind::KeyPuzzle);
            }
            assert!(!plan.room(PURSUIT_LAST + 1).unwrap().pursuit);
        }
        // Short custom floors have no chase.
        let short = generate_custom(1, &EntityConfig::default(), &GenOptions { last_room: 30, ..GenOptions::default() });
        assert!(short.rooms.iter().all(|r| !r.pursuit));
    }

    #[test]
    fn no_rushing_entity_right_before_the_chase() {
        for seed in 0..200 {
            let plan = generate(seed);
            for i in PURSUIT_FIRST - 2..PURSUIT_FIRST {
                let r = plan.room(i).unwrap();
                assert!(!r.encounter.is_some_and(Encounter::needs_hiding), "seed {seed} room {i}: {:?}", r.encounter);
            }
        }
    }

    #[test]
    fn reading_hall_at_door_50() {
        for seed in 0..20 {
            let plan = generate(seed);
            let r = plan.room(READING_HALL).unwrap();
            assert_eq!(r.feature, Some(Feature::ReadingHall));
            assert_eq!(r.kind, RoomKind::KeyPuzzle);
            assert_eq!(r.area, Area::Archive);
            assert!(r.encounter.is_none() && r.dividers().len() >= 3);
        }
    }

    #[test]
    fn dividers_leave_a_way_through() {
        let mut zig = 0;
        for seed in 0..30 {
            for r in generate(seed).rooms {
                let ds = r.dividers();
                if r.feature == Some(Feature::Zigzag) {
                    zig += 1;
                }
                for (i, d) in ds.iter().enumerate() {
                    assert!(d.z < r.max.y - 2.0 && d.z > r.min.y + 2.0);
                    if r.entry_height > 0.0 {
                        let foot = r.max.y - LANDING_DEPTH - r.entry_height / STEP_RISE * STEP_RUN;
                        assert!(d.z < foot - 0.5, "divider cuts the entry stairs in room {}", r.index);
                    }
                    assert!(d.gap.0 >= r.min.x - 1e-3 && d.gap.1 <= r.max.x + 1e-3);
                    assert!((d.gap.1 - d.gap.0 - DIVIDER_GAP).abs() < 1e-3);
                    if i > 0 {
                        assert!(ds[i - 1].z - d.z >= 2.4, "dividers too close");
                        assert_ne!(ds[i - 1].gap.0, d.gap.0, "openings alternate sides");
                    }
                }
                let route = r.route();
                let raised = usize::from(matches!(r.feature, Some(Feature::RaisedExit { .. })));
                assert_eq!(route.len(), 2 + 2 * ds.len() + raised);
            }
        }
        assert!(zig > 10, "zigzag rooms should turn up: {zig}");
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

