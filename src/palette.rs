//! The Tone palette from `gray-color-scheme/color-palette.json`, as sRGB hex per mode.

use rune::Color;
use rune::color::srgb_decode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Window background.
    pub bg: Color,
    /// Raised surfaces: header, now-playing bar.
    pub bg2: Color,
    /// Borders, tracks, hover and selection fills.
    pub ui: Color,
    /// Strong UI strokes.
    pub ui2: Color,
    /// Tinted UI accent.
    pub ui3: Color,
    pub tx: Color,
    pub tx2: Color,
    pub tx3: Color,
    pub red: Color,
    pub blue: Color,
}

/// `0xRRGGBB` in sRGB to Rune's linear colour.
fn rgb(hex: u32) -> Color {
    let channel = |shift: u32| srgb_decode(((hex >> shift) & 0xff) as f32 / 255.0);
    Color { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }
}

impl Palette {
    pub fn new(dark: bool) -> Palette {
        if dark {
            Palette {
                dark,
                bg: rgb(0x0b0b0c),
                bg2: rgb(0x18191a),
                ui: rgb(0x2e3134),
                ui2: rgb(0x777d83),
                ui3: rgb(0x425d7e),
                tx: rgb(0xedeeef),
                tx2: rgb(0x98a0a5),
                tx3: rgb(0x6b6f74),
                red: rgb(0xe9434b),
                blue: rgb(0x3d85f3),
            }
        } else {
            Palette {
                dark,
                bg: rgb(0xedeeef),
                bg2: rgb(0xdee0e2),
                ui: rgb(0xc0c4c9),
                ui2: rgb(0x6d7279),
                ui3: rgb(0x7696be),
                tx: rgb(0x0b0b0c),
                tx2: rgb(0x4b5257),
                tx3: rgb(0x7c8085),
                red: rgb(0xc4182d),
                blue: rgb(0x155fca),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_linear_colour() {
        assert_eq!(rgb(0x3d85f3).hex(), "#3d85f3ff");
        assert_eq!(Palette::new(false).bg.hex(), "#edeeefff");
        assert_eq!(Palette::new(true).bg.hex(), "#0b0b0cff");
    }
}
