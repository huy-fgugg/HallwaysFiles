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

pub const SIGN_W: usize = 256;
pub const SIGN_H: usize = 96;

/// Large back-lit number sign hung above each door: warm glowing digits on a
/// dark enamel panel with a thin bright border. Readable from across a room.
pub fn sign_pixels(number: u32) -> Vec<u8> {
    let text = format!("{:03}", number % 1000);
    let mut px = vec![0u8; SIGN_W * SIGN_H * 4];
    for y in 0..SIGN_H {
        for x in 0..SIGN_W {
            let edge = x.min(SIGN_W - 1 - x).min(y).min(SIGN_H - 1 - y);
            let c: [u8; 3] = if edge < 3 { [150, 128, 90] } else if edge < 6 { [14, 12, 10] } else { [22, 19, 16] };
            let i = (y * SIGN_W + x) * 4;
            px[i..i + 3].copy_from_slice(&c);
            px[i + 3] = 255;
        }
    }
    let scale = 10;
    let glyph_w = 5 * scale;
    let spacing = 14;
    let total_w = text.len() * glyph_w + (text.len() - 1) * spacing;
    let x0 = (SIGN_W - total_w) / 2;
    let y0 = (SIGN_H - 7 * scale) / 2;
    for (ci, ch) in text.chars().enumerate() {
        let d = ch.to_digit(10).unwrap() as usize;
        let gx = x0 + ci * (glyph_w + spacing);
        for (row, bits) in DIGITS[d].iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                for sy in 1..scale - 1 {
                    for sx in 1..scale - 1 {
                        let i = ((y0 + row * scale + sy) * SIGN_W + gx + col * scale + sx) * 4;
                        px[i..i + 3].copy_from_slice(&[255, 222, 160]);
                    }
                }
            }
        }
    }
    px
}

pub fn sign_image(number: u32) -> Image {
    Image::new(
        Extent3d { width: SIGN_W as u32, height: SIGN_H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        sign_pixels(number),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
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
    fn sign_digits_glow() {
        let px = sign_pixels(42);
        assert_eq!(px.len(), SIGN_W * SIGN_H * 4);
        let lit = px.chunks(4).filter(|p| p[0] == 255).count();
        assert!(lit > 500, "sign digits should be bright, got {lit}");
        assert_ne!(sign_pixels(41), px);
    }

    #[test]
    fn different_numbers_differ() {
        assert_ne!(plaque_pixels(1), plaque_pixels(2));
    }
}
