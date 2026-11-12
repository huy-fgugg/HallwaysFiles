//! Generates engraved brass door-number plaque textures at runtime
//! (bitmap digits, no font files needed).

use bevy::{
    asset::RenderAssetUsages,
    image::Image,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// 5x7 bitmap digits, one row per byte (low 5 bits, MSB = leftmost pixel).
const DIGITS: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
    [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
];

pub const PLAQUE_W: usize = 192;
pub const PLAQUE_H: usize = 80;

/// Returns RGBA8 pixels for a plaque showing `number` zero-padded to 3 digits.
pub fn plaque_pixels(number: u32) -> Vec<u8> {
    let text = format!("{:03}", number % 1000);
    let mut px = vec![0u8; PLAQUE_W * PLAQUE_H * 4];
    let scale = 8;
    let glyph_w = 5 * scale;
    let spacing = 12;
    let total_w = text.len() * glyph_w + (text.len() - 1) * spacing;
    let x0 = (PLAQUE_W - total_w) / 2;
    let y0 = (PLAQUE_H - 7 * scale) / 2;
    for y in 0..PLAQUE_H {
        for x in 0..PLAQUE_W {
            // Brushed brass with a darker bevelled border.
            let edge = x.min(PLAQUE_W - 1 - x).min(y).min(PLAQUE_H - 1 - y);
            let brushed = ((x * 7 + y * 131) % 17) as f32 / 17.0 * 0.06;
            let bevel = if edge < 5 { 0.7 } else { 1.0 };
            let c = [0.78 + brushed, 0.6 + brushed, 0.3 + brushed * 0.5].map(|v| v * bevel);
            let i = (y * PLAQUE_W + x) * 4;
            px[i] = (c[0] * 255.0) as u8;
            px[i + 1] = (c[1] * 255.0) as u8;
            px[i + 2] = (c[2] * 255.0) as u8;
            px[i + 3] = 255;
        }
    }
    for (ci, ch) in text.chars().enumerate() {
        let d = ch.to_digit(10).unwrap() as usize;
        let gx = x0 + ci * (glyph_w + spacing);
        for (row, bits) in DIGITS[d].iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let x = gx + col * scale + sx;
                        let y = y0 + row * scale + sy;
                        let i = (y * PLAQUE_W + x) * 4;
                        // Engraved: near-black with a slight warm tint.
                        px[i] = 28;
                        px[i + 1] = 20;
                        px[i + 2] = 12;
                    }
                }
            }
        }
    }
    px
}

pub fn plaque_image(number: u32) -> Image {
    Image::new(
        Extent3d { width: PLAQUE_W as u32, height: PLAQUE_H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        plaque_pixels(number),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaque_has_engraving() {
        let px = plaque_pixels(7);
        assert_eq!(px.len(), PLAQUE_W * PLAQUE_H * 4);
        let dark = px.chunks(4).filter(|p| p[0] < 40).count();
        assert!(dark > 100, "digits should be engraved, got {dark} dark pixels");
    }

    #[test]
    fn different_numbers_differ() {
        assert_ne!(plaque_pixels(1), plaque_pixels(2));
    }
}
