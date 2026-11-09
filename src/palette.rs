//! The Tone palette from `gray-color-scheme/color-palette.json`, as sRGB hex per mode.

use rune::Color;
use rune::color::srgb_decode;

use crate::credits::Role;

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

/// A role's pill: (fill, text), in Tailwind's 500 tint behind its 700 (light) or 300 (dark).
fn role_hues(r: Role) -> (u32, u32, u32) {
    match r {
        Role::Composer => (0x8b5cf6, 0x6d28d9, 0xc4b5fd),
        Role::Lyricist => (0xd946ef, 0xa21caf, 0xf0abfc),
        Role::Writer => (0xec4899, 0xbe185d, 0xf9a8d4),
        Role::Producer => (0xf97316, 0xc2410c, 0xfdba74),
        Role::Arranger => (0xf59e0b, 0xb45309, 0xfcd34d),
        Role::Conductor => (0x84cc16, 0x4d7c0f, 0xbef264),
        Role::Performer => (0x10b981, 0x047857, 0x6ee7b7),
        Role::Engineer => (0x06b6d4, 0x0e7490, 0x67e8f9),
        Role::Mixer => (0x3b82f6, 0x1d4ed8, 0x93c5fd),
        Role::Remixer => (0xef4444, 0xb91c1c, 0xfca5a5),
        Role::DjMixer => (0xeab308, 0xa16207, 0xfde047),
    }
}

impl Palette {
    /// A credit role's pill colours: (fill, text).
    pub fn role(&self, r: Role) -> (Color, Color) {
        let (base, light, dark) = role_hues(r);
        (rgb(base).alpha(if self.dark { 0.22 } else { 0.18 }), rgb(if self.dark { dark } else { light }))
    }

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
