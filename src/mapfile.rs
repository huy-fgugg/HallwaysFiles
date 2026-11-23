//! Hand-built maps (the Map Builder). A map is a Custom Floor preset that also
//! lists its rooms one by one; it's saved as a `.hallwaysmap` file you can
//! copy to another computer and open (or edit) there. See
//! [`crate::floor::plan::from_map`] for how the rooms are laid out safely.

use serde::{Deserialize, Serialize};

use crate::floor::plan::Area;

pub const MAP_EXTENSION: &str = "hallwaysmap";
/// Rooms a map can hold (Door 100 at most, counting the final hall).
pub const MAX_ROOMS: usize = 96;

/// What a room is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapKind {
    /// Drawers to search.
    Loot,
    /// A long, narrow hall.
    Hall,
    /// The exit is locked; the key is in a drawer.
    Locked,
    /// Nothing to search.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapLight {
    Bright,
    Dim,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapMonster {
    None,
    Surge,
    Undertow,
    Whisperer,
    Gallery,
    Hush,
    Listener,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapFeature {
    None,
    /// A forest of pillars.
    Pillars,
    /// Walls with gaps at alternating ends.
    Zigzag,
    /// The exit is up a staircase.
    Stairs,
    /// A stream with a footbridge (gardens only).
    Stream,
    /// The key waits on a loft (locked rooms).
    Loft,
}

/// One room of a map.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapRoom {
    pub area: Area,
    /// Metres.
    pub width: f32,
    pub depth: f32,
    pub kind: MapKind,
    pub light: MapLight,
    pub monster: MapMonster,
    pub feature: MapFeature,
}

impl Default for MapRoom {
    fn default() -> Self {
        Self { area: Area::GuestWing, width: 8.0, depth: 10.0, kind: MapKind::Loot, light: MapLight::Bright, monster: MapMonster::None, feature: MapFeature::None }
    }
}

pub const AREAS: [Area; 6] = Area::ALL;
pub const KINDS: [MapKind; 4] = [MapKind::Loot, MapKind::Hall, MapKind::Locked, MapKind::Empty];
pub const LIGHTS: [MapLight; 3] = [MapLight::Bright, MapLight::Dim, MapLight::Dark];
pub const MONSTERS: [MapMonster; 7] = [MapMonster::None, MapMonster::Surge, MapMonster::Undertow, MapMonster::Whisperer, MapMonster::Gallery, MapMonster::Hush, MapMonster::Listener];
pub const FEATURES: [MapFeature; 6] = [MapFeature::None, MapFeature::Pillars, MapFeature::Zigzag, MapFeature::Stairs, MapFeature::Stream, MapFeature::Loft];

/// The settings of a room the builder can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoomField {
    Area,
    Width,
    Depth,
    Kind,
    Light,
    Monster,
    Feature,
}

impl RoomField {
    pub const ALL: [RoomField; 7] = [RoomField::Area, RoomField::Width, RoomField::Depth, RoomField::Kind, RoomField::Light, RoomField::Monster, RoomField::Feature];

    pub fn label(self) -> &'static str {
        match self {
            RoomField::Area => "Area",
            RoomField::Width => "Width",
            RoomField::Depth => "Length",
            RoomField::Kind => "Type",
            RoomField::Light => "Lights",
            RoomField::Monster => "Monster",
            RoomField::Feature => "Structure",
        }
    }
}

fn step<T: Copy + PartialEq>(all: &[T], v: T, d: i32) -> T {
    let i = all.iter().position(|x| *x == v).unwrap_or(0) as i32;
    all[(i + d).rem_euclid(all.len() as i32) as usize]
}

impl MapRoom {
    pub fn adjust(&mut self, f: RoomField, d: i32) {
        match f {
            RoomField::Area => self.area = step(&AREAS, self.area, d),
            RoomField::Width => self.width = (self.width + d as f32).clamp(4.0, 16.0),
            RoomField::Depth => self.depth = (self.depth + d as f32 * 2.0).clamp(6.0, 40.0),
            RoomField::Kind => self.kind = step(&KINDS, self.kind, d),
            RoomField::Light => self.light = step(&LIGHTS, self.light, d),
            RoomField::Monster => self.monster = step(&MONSTERS, self.monster, d),
            RoomField::Feature => self.feature = step(&FEATURES, self.feature, d),
        }
    }

    pub fn value(&self, f: RoomField) -> String {
        match f {
            RoomField::Area => self.area.name().to_string(),
            RoomField::Width => format!("{:.0} m", self.width),
            RoomField::Depth => format!("{:.0} m", self.depth),
            RoomField::Kind => format!("{:?}", self.kind),
            RoomField::Light => format!("{:?}", self.light),
            RoomField::Monster => match self.monster {
                MapMonster::None => "None".into(),
                MapMonster::Surge => "Rusher".into(),
                MapMonster::Undertow => "Undertow".into(),
                MapMonster::Whisperer => "Whisperer".into(),
                MapMonster::Gallery => "Gallery".into(),
                MapMonster::Hush => "Hush".into(),
                MapMonster::Listener => "Groundskeeper".into(),
            },
            RoomField::Feature => match self.feature {
                MapFeature::None => "None".into(),
                MapFeature::Pillars => "Pillars".into(),
                MapFeature::Zigzag => "Zigzag walls".into(),
                MapFeature::Stairs => "Stairs up".into(),
                MapFeature::Stream => "Stream".into(),
                MapFeature::Loft => "Key loft".into(),
            },
        }
    }

    /// A short line for the room list.
    pub fn summary(&self) -> String {
        let area = self.area.name().trim_start_matches("The ");
        let mut s = format!("{area} - {}", self.value(RoomField::Kind));
        if self.monster != MapMonster::None {
            s += &format!(" - {}", self.value(RoomField::Monster));
        }
        s
    }

    /// Clamps sizes into range (hand-edited files).
    pub fn sanitized(mut self) -> Self {
        let fix = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.width = fix(self.width, 4.0, 16.0, 8.0);
        self.depth = fix(self.depth, 6.0, 40.0, 10.0);
        self
    }
}

/// A fresh map to start from: a few rooms through two areas.
pub fn starter_rooms() -> Vec<MapRoom> {
    vec![
        MapRoom::default(),
        MapRoom { kind: MapKind::Hall, depth: 18.0, ..MapRoom::default() },
        MapRoom { monster: MapMonster::Surge, ..MapRoom::default() },
        MapRoom { kind: MapKind::Locked, feature: MapFeature::Loft, ..MapRoom::default() },
        MapRoom { area: Area::Grounds, width: 14.0, depth: 20.0, feature: MapFeature::Stream, ..MapRoom::default() },
    ]
}
