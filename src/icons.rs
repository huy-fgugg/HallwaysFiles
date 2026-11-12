//! Pixel-art item icons generated in code (32x32, original designs), shown in
//! the inventory slots. No image files needed; swap for painted icons later.

use bevy::{
    asset::RenderAssetUsages,
    image::{Image, ImageSampler},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::collections::HashMap;

use crate::items::ItemKind;

pub const ICON: usize = 32;

/// Tiny RGBA canvas with a few drawing helpers.
pub struct Canvas {
    pub px: Vec<[u8; 4]>,
}

impl Canvas {
    pub fn new() -> Self {
        Self { px: vec![[0, 0, 0, 0]; ICON * ICON] }
    }

    fn set(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if (0..ICON as i32).contains(&x) && (0..ICON as i32).contains(&y) {
            self.px[y as usize * ICON + x as usize] = c;
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 4]) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, c);
            }
        }
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: [u8; 4]) {
        for y in 0..ICON as i32 {
            for x in 0..ICON as i32 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn ring(&mut self, cx: f32, cy: f32, r0: f32, r1: f32, c: [u8; 4]) {
        for y in 0..ICON as i32 {
            for x in 0..ICON as i32 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d >= r0 && d <= r1 {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// Thick line (for diagonal shapes).
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: [u8; 4]) {
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()) * 3.0) as i32 + 1;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.disc(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, w / 2.0, c);
        }
    }

    /// 1-px dark outline around every opaque pixel, for readability on any background.
    fn outline(&mut self) {
        let src = self.px.clone();
        for y in 0..ICON as i32 {
            for x in 0..ICON as i32 {
                if src[y as usize * ICON + x as usize][3] != 0 {
                    continue;
                }
                let near = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    (0..ICON as i32).contains(&nx) && (0..ICON as i32).contains(&ny) && src[ny as usize * ICON + nx as usize][3] > 0 && src[ny as usize * ICON + nx as usize] != [0, 0, 0, 0]
                });
                if near {
                    self.px[y as usize * ICON + x as usize] = [12, 10, 14, 255];
                }
            }
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.px.iter().flatten().copied().collect()
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

const STEEL: [u8; 4] = [150, 160, 172, 255];
const STEEL_HI: [u8; 4] = [205, 212, 222, 255];
const STEEL_LO: [u8; 4] = [90, 98, 110, 255];
const BRASS: [u8; 4] = [196, 152, 72, 255];
const BRASS_HI: [u8; 4] = [238, 200, 120, 255];
const BRASS_LO: [u8; 4] = [130, 94, 40, 255];
const WHITE: [u8; 4] = [236, 232, 222, 255];
const CREAM: [u8; 4] = [214, 204, 182, 255];
const RED: [u8; 4] = [200, 52, 48, 255];
const GREEN: [u8; 4] = [86, 176, 84, 255];
const GREEN_LO: [u8; 4] = [48, 110, 52, 255];
const GLOW: [u8; 4] = [255, 236, 150, 255];
const GLOW_SOFT: [u8; 4] = [255, 230, 150, 90];
const DARK: [u8; 4] = [40, 40, 46, 255];

pub fn draw(kind: ItemKind) -> Canvas {
    let mut c = Canvas::new();
    match kind {
        ItemKind::Flashlight => {
            // Beam, body, grip rings, head and lit lens (pointing up-right).
            c.line(18.0, 14.0, 30.0, 2.0, 9.0, GLOW_SOFT);
            c.line(6.0, 26.0, 16.0, 16.0, 6.0, STEEL_LO);
            c.line(6.0, 25.0, 16.0, 15.0, 3.0, STEEL);
            for k in 0..3 {
                let t = 8.0 + k as f32 * 2.5;
                c.line(t, 26.0 - (t - 6.0), t + 0.6, 25.4 - (t - 6.0), 6.5, DARK);
            }
            c.line(16.0, 16.0, 20.0, 12.0, 9.0, STEEL_LO);
            c.line(16.0, 15.0, 20.0, 11.0, 6.0, STEEL_HI);
            c.disc(20.5, 11.5, 3.2, GLOW);
            c.rect(4, 26, 6, 28, DARK);
        }
        ItemKind::Lockpick => {
            // Wooden-wrapped handle, long steel shaft, hooked tip, plus a tension wrench.
            c.line(5.0, 27.0, 11.0, 21.0, 5.0, [120, 78, 44, 255]);
            c.line(5.0, 26.0, 10.0, 21.0, 2.0, [168, 116, 66, 255]);
            c.line(11.0, 21.0, 25.0, 7.0, 2.2, STEEL);
            c.line(11.0, 20.0, 24.0, 7.0, 1.0, STEEL_HI);
            c.line(25.0, 7.0, 27.0, 4.0, 2.0, STEEL);
            c.line(27.0, 4.0, 29.0, 5.5, 2.0, STEEL);
            c.line(8.0, 14.0, 20.0, 26.0, 2.0, STEEL_LO);
            c.line(20.0, 26.0, 24.0, 24.0, 2.0, STEEL_LO);
        }
        ItemKind::Vitamins => {
            // Amber pill bottle with a white cap and label, two loose pills.
            c.rect(9, 9, 22, 27, [176, 96, 30, 255]);
            c.rect(10, 10, 12, 26, [214, 134, 56, 255]);
            c.rect(8, 4, 23, 9, WHITE);
            c.rect(8, 8, 23, 9, CREAM);
            for x in (9..23).step_by(2) {
                c.set(x, 5, CREAM);
            }
            c.rect(10, 15, 21, 22, WHITE);
            c.rect(14, 16, 17, 21, GREEN);
            c.rect(12, 18, 19, 19, GREEN);
            c.line(24.0, 26.0, 28.0, 26.0, 3.0, GREEN);
            c.line(26.0, 26.0, 28.0, 26.0, 3.0, WHITE);
            c.line(3.0, 28.0, 6.0, 25.0, 3.0, GREEN_LO);
        }
        ItemKind::Bandage => {
            // Rolled bandage (spiral end) with a strip unrolling and a red cross.
            c.disc(13.0, 14.0, 9.0, WHITE);
            c.ring(13.0, 14.0, 2.0, 3.0, CREAM);
            c.ring(13.0, 14.0, 5.0, 6.0, CREAM);
            c.disc(13.0, 14.0, 1.4, [170, 160, 140, 255]);
            c.line(20.0, 20.0, 29.0, 27.0, 5.0, WHITE);
            c.line(21.0, 21.0, 29.0, 28.0, 1.0, CREAM);
            c.rect(23, 4, 25, 12, RED);
            c.rect(20, 7, 28, 9, RED);
        }
        ItemKind::SkeletonKey => {
            // Ornate brass key: clover bow, long shank, toothed bit.
            c.ring(9.0, 9.0, 3.0, 6.5, BRASS);
            c.ring(9.0, 9.0, 3.0, 4.0, BRASS_LO);
            for (x, y) in [(9.0, 2.5), (2.5, 9.0), (15.0, 15.0)] {
                c.disc(x, y, 2.2, BRASS);
            }
            c.line(13.0, 13.0, 27.0, 27.0, 3.2, BRASS);
            c.line(13.0, 12.5, 27.0, 26.5, 1.2, BRASS_HI);
            c.rect(20, 24, 23, 28, BRASS);
            c.rect(24, 21, 28, 24, BRASS);
            c.set(25, 23, BRASS_LO);
            c.set(21, 26, BRASS_LO);
        }
    }
    c.outline();
    c
}

#[derive(Resource, Default)]
pub struct ItemIcons(pub HashMap<ItemKind, Handle<Image>>);

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemIcons>().add_systems(Startup, build_icons);
    }
}

fn build_icons(mut icons: ResMut<ItemIcons>, mut images: ResMut<Assets<Image>>) {
    for kind in ItemKind::ALL {
        let mut img = Image::new(
            Extent3d { width: ICON as u32, height: ICON as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            draw(kind).bytes(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        // Crisp pixels when scaled up.
        img.sampler = ImageSampler::nearest();
        icons.0.insert(kind, images.add(img));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_content_and_differs() {
        let mut seen = vec![];
        for kind in ItemKind::ALL {
            let bytes = draw(kind).bytes();
            assert_eq!(bytes.len(), ICON * ICON * 4);
            let opaque = bytes.chunks(4).filter(|p| p[3] > 0).count();
            assert!(opaque > 80, "{kind:?} icon is nearly empty");
            assert!(!seen.contains(&bytes), "{kind:?} icon duplicates another");
            seen.push(bytes);
        }
    }
}
