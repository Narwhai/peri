//! Themes: named colour palettes with a light and a dark mode, and the appearance that picks one.

use rune::Color;
use rune::color::srgb_decode;
use serde::{Deserialize, Serialize};

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
    /// The app's accent: buttons, toggles, playing icons, focus.
    pub accent: Color,
    /// Text and icons drawn on an `accent` fill.
    pub on_accent: Color,
    /// The selected option of a segmented control.
    pub selected: Color,
    /// The Home, Library and Graph tab icons, when active.
    pub tabs: [Color; 3],
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

    /// The Bolis theme's palette in one mode: the Tone palette from `gray-color-scheme/color-palette.json`.
    fn bolis(dark: bool) -> Palette {
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
                accent: rgb(0x3d85f3),
                on_accent: rgb(0x0b0b0c),
                selected: rgb(0x599b94),
                tabs: [rgb(0xc7693c), rgb(0x3d85f3), rgb(0x9560fd)],
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
                accent: rgb(0x155fca),
                on_accent: Color::WHITE,
                selected: rgb(0x2a6e67),
                tabs: [rgb(0xa34918), rgb(0x155fca), rgb(0x8000fa)],
            }
        }
    }

    /// The Peridot theme's palette in one mode: the Ladder palette from `color-schemes/peridot-palette.css`
    /// (seeded from #93a043). Neutrals `n-3…n2` give the surfaces, strokes and text, the `v-3…v2` steps the accent.
    /// Ladder has no red or extra hues, so `red` is a hand-picked one that reads on both backgrounds.
    fn peridot(dark: bool) -> Palette {
        if dark {
            Palette {
                dark,
                bg: rgb(0x101107),        // n-3
                bg2: rgb(0x1c1e13),       // n-2
                ui: rgb(0x414337),        // n-1
                ui2: rgb(0x616457),       // n
                ui3: rgb(0x4a5204),       // v-2
                tx: rgb(0xa8ab9d),        // n2
                tx2: rgb(0x85877a),       // n1
                tx3: rgb(0x616457),       // n
                red: rgb(0xf0695d),
                accent: rgb(0x889601),    // v
                on_accent: rgb(0x101107), // n-3
                selected: rgb(0x98a71e),  // v1
                tabs: [rgb(0x889601); 3], // v
            }
        } else {
            Palette {
                dark,
                bg: rgb(0xf2f5e7),        // n-3
                bg2: rgb(0xe5e8d9),       // n-2
                ui: rgb(0xb3b6a7),        // n-1
                ui2: rgb(0x898b7e),       // n
                ui3: rgb(0xa2b147),       // v-2
                tx: rgb(0x484b3e),        // n2
                tx2: rgb(0x65675a),       // n1
                tx3: rgb(0x898b7e),       // n
                red: rgb(0xb3261e),
                accent: rgb(0x5f6909),    // v
                on_accent: rgb(0xf2f5e7), // n-3
                selected: rgb(0x515a06),  // v1
                tabs: [rgb(0x5f6909); 3], // v
            }
        }
    }
}

/// A named colour theme, with a light and a dark palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Bolis,
    Peridot,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Bolis, Theme::Peridot];

    /// The value saved in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Bolis => "bolis",
            Theme::Peridot => "peridot",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Bolis => "Bolis",
            Theme::Peridot => "Peridot",
        }
    }

    pub fn palette(self, dark: bool) -> Palette {
        match self {
            Theme::Bolis => Palette::bolis(dark),
            Theme::Peridot => Palette::peridot(dark),
        }
    }
}

/// Whether the app is light, dark, or follows the system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Light, Appearance::Dark, Appearance::System];

    /// The value saved in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
            Appearance::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
            Appearance::System => "System",
        }
    }

    /// Whether to draw dark, given the system's setting.
    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            Appearance::Light => false,
            Appearance::Dark => true,
            Appearance::System => system_dark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.x relative luminance of a colour (Rune's colour is linear sRGB already).
    fn luminance(c: Color) -> f32 {
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    }

    /// WCAG contrast ratio between two colours, from 1 to 21.
    fn contrast(a: Color, b: Color) -> f32 {
        let (hi, lo) = if luminance(a) >= luminance(b) { (luminance(a), luminance(b)) } else { (luminance(b), luminance(a)) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn hex_round_trips_through_linear_colour() {
        assert_eq!(rgb(0x3d85f3).hex(), "#3d85f3ff");
        assert_eq!(Theme::Bolis.palette(false).bg.hex(), "#edeeefff");
        assert_eq!(Theme::Bolis.palette(true).bg.hex(), "#0b0b0cff");
        assert_eq!(Theme::Peridot.palette(false).bg.hex(), "#f2f5e7ff");
        assert_eq!(Theme::Peridot.palette(true).bg.hex(), "#101107ff");
    }

    #[test]
    fn every_theme_keeps_its_text_and_accent_readable() {
        for theme in Theme::ALL {
            for dark in [false, true] {
                let p = theme.palette(dark);
                let label = format!("{} {}", theme.name(), if dark { "dark" } else { "light" });
                assert!(contrast(p.tx, p.bg) >= 7.0, "{label}: tx on bg");
                assert!(contrast(p.tx2, p.bg) >= 4.5, "{label}: tx2 on bg");
                assert!(contrast(p.tx3, p.bg) >= 3.0, "{label}: tx3 on bg");
                assert!(contrast(p.on_accent, p.accent) >= 4.5, "{label}: on_accent on accent");
                assert!(contrast(p.red, p.bg) >= 4.5, "{label}: red on bg");
                // Header and now-playing bar text sits on bg2; segmented labels and accent icons on bg.
                assert!(contrast(p.tx, p.bg2) >= 7.0, "{label}: tx on bg2");
                assert!(contrast(p.tx2, p.bg2) >= 4.5, "{label}: tx2 on bg2");
                assert!(contrast(p.selected, p.bg) >= 4.5, "{label}: selected on bg");
                assert!(contrast(p.accent, p.bg) >= 3.0, "{label}: accent on bg");
                for (i, tab) in p.tabs.iter().enumerate() {
                    assert!(contrast(*tab, p.bg) >= 3.0, "{label}: tab {i} on bg");
                }
            }
        }
    }

    #[test]
    fn peridot_uses_the_ladder_palette() {
        // Ladder's n-3 (bg), n2 (tx) and v (accent) steps, light then dark.
        let light = Theme::Peridot.palette(false);
        assert_eq!([light.bg.hex(), light.tx.hex(), light.accent.hex()], ["#f2f5e7ff", "#484b3eff", "#5f6909ff"]);
        let dark = Theme::Peridot.palette(true);
        assert_eq!([dark.bg.hex(), dark.tx.hex(), dark.accent.hex()], ["#101107ff", "#a8ab9dff", "#889601ff"]);
    }

    #[test]
    fn appearance_picks_the_mode() {
        assert!(!Appearance::Light.dark(true));
        assert!(Appearance::Dark.dark(false));
        assert!(Appearance::System.dark(true));
        assert!(!Appearance::System.dark(false));
        assert_eq!(Appearance::default(), Appearance::System);
    }

    #[test]
    fn saved_values_round_trip() {
        for (value, json) in [(Appearance::Light, "light"), (Appearance::Dark, "dark"), (Appearance::System, "system")] {
            assert_eq!(serde_json::to_string(&value).unwrap(), format!("\"{json}\""));
            assert_eq!(serde_json::from_str::<Appearance>(&format!("\"{json}\"")).unwrap(), value);
        }
        assert_eq!(serde_json::to_string(&Theme::Bolis).unwrap(), "\"bolis\"");
        assert_eq!(serde_json::from_str::<Theme>("\"bolis\"").unwrap(), Theme::Bolis);
        assert_eq!(serde_json::to_string(&Theme::Peridot).unwrap(), "\"peridot\"");
        assert_eq!(serde_json::from_str::<Theme>("\"peridot\"").unwrap(), Theme::Peridot);
        assert!(serde_json::from_str::<Appearance>("\"sepia\"").is_err());
    }
}
