//! Color palettes, text color choice and card metrics. Pure functions.

use crate::settings::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const WHITE: Rgba = Rgba::hex(0xFFFFFF);
    pub const BLACK: Rgba = Rgba::hex(0x000000);

    pub const fn hex(rgb: u32) -> Rgba {
        Rgba {
            r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
            g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
            b: (rgb & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn from_u8(c: [u8; 3]) -> Rgba {
        Rgba::hex(((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32)
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba { a, ..self }
    }

    pub fn scale(self, k: f32) -> Rgba {
        Rgba {
            r: self.r * k,
            g: self.g * k,
            b: self.b * k,
            a: self.a,
        }
    }
}

/// WCAG relative luminance of an opaque color.
pub fn luminance(c: Rgba) -> f32 {
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio between two opaque colors, from 1 to 21.
pub fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// HSV saturation of a color, from 0 to 1.
pub fn saturation(c: [u8; 3]) -> f32 {
    let max = c.iter().copied().max().unwrap_or(0) as f32;
    let min = c.iter().copied().min().unwrap_or(0) as f32;
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    Solid(Rgba),
    /// Top-left to bottom-right linear gradient.
    Gradient(Rgba, Rgba),
}

/// Explicit colors for the Spotify and Cover themes. The System theme has no
/// palette: each platform uses its own backdrop and dynamic system colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub fill: Fill,
    pub border: Option<Rgba>,
    pub title: Rgba,
    pub artist: Rgba,
    pub album: Rgba,
    pub progress: Option<Rgba>,
}

pub const SPOTIFY_SURFACE: u32 = 0x121212;
pub const SPOTIFY_BORDER: u32 = 0x2A2A2A;
pub const SPOTIFY_SECONDARY: u32 = 0xB3B3B3;
pub const SPOTIFY_GREEN: u32 = 0x1DB954;
const NEUTRAL_ACCENT: [u8; 3] = [0x3A, 0x3A, 0x48];

pub fn palette(theme: Theme, accent: Option<[u8; 3]>) -> Option<Palette> {
    match theme {
        Theme::System => None,
        Theme::Spotify => Some(Palette {
            fill: Fill::Solid(Rgba::hex(SPOTIFY_SURFACE)),
            border: Some(Rgba::hex(SPOTIFY_BORDER)),
            title: Rgba::WHITE,
            artist: Rgba::hex(SPOTIFY_SECONDARY),
            album: Rgba::hex(SPOTIFY_SECONDARY),
            progress: Some(Rgba::hex(SPOTIFY_GREEN)),
        }),
        Theme::Cover => Some(cover_palette(accent.unwrap_or(NEUTRAL_ACCENT))),
    }
}

/// Gradient from the dominant color to a darker shade, with white or black
/// text that reaches a contrast of at least 4.5 to 1 on both ends.
pub fn cover_palette(accent: [u8; 3]) -> Palette {
    const MIN: f32 = 4.5;
    let start = Rgba::from_u8(accent);
    let darkest = start.scale(0.55);
    let (text, end) = if contrast(Rgba::WHITE, start) >= MIN {
        (Rgba::WHITE, darkest)
    } else {
        // Black text: darken the end only as far as black stays readable.
        let mut k = 0.55;
        while k < 1.0 && contrast(Rgba::BLACK, start.scale(k)) < MIN {
            k += 0.01;
        }
        (Rgba::BLACK, start.scale(k.min(1.0)))
    };
    Palette {
        fill: Fill::Gradient(start, end),
        border: None,
        title: text,
        artist: text.with_alpha(0.85),
        album: text.with_alpha(0.7),
        progress: None,
    }
}

/// Card metrics in logical pixels (DIPs on Windows, points on macOS).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub padding: f32,
    pub cover: f32,
    pub cover_radius: f32,
    pub corner_radius: f32,
    /// Space between the cover and the text column.
    pub gap: f32,
    pub title_size: f32,
    pub title_bold: bool,
    pub artist_size: f32,
    pub album_size: f32,
}

pub fn layout(theme: Theme) -> Layout {
    let base = if cfg!(target_os = "macos") {
        Layout {
            width: 344.0,
            height: 76.0,
            padding: 10.0,
            cover: 56.0,
            cover_radius: 6.0,
            corner_radius: 16.0,
            gap: 10.0,
            title_size: 13.0,
            title_bold: false,
            artist_size: 12.0,
            album_size: 12.0,
        }
    } else {
        Layout {
            width: 364.0,
            height: 92.0,
            padding: 14.0,
            cover: 64.0,
            cover_radius: 4.0,
            corner_radius: 8.0,
            gap: 12.0,
            title_size: 14.0,
            title_bold: false,
            artist_size: 12.0,
            album_size: 12.0,
        }
    };
    match theme {
        Theme::Spotify => {
            let padding = 10.0;
            Layout {
                padding,
                cover: 72.0,
                cover_radius: 4.0,
                corner_radius: 8.0,
                height: 72.0 + 2.0 * padding,
                title_size: 15.0,
                title_bold: true,
                artist_size: 13.0,
                ..base
            }
        }
        Theme::System | Theme::Cover => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_extremes() {
        assert!((contrast(Rgba::WHITE, Rgba::BLACK) - 21.0).abs() < 0.01);
        assert!((contrast(Rgba::WHITE, Rgba::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn cover_text_contrast_is_at_least_4_5_for_all_colors() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let p = cover_palette([r as u8, g as u8, b as u8]);
                    let Fill::Gradient(start, end) = p.fill else {
                        panic!("cover theme must use a gradient");
                    };
                    assert!(contrast(p.title, start) >= 4.5, "start {r},{g},{b}");
                    assert!(contrast(p.title, end) >= 4.5, "end {r},{g},{b}");
                }
            }
        }
    }

    #[test]
    fn cover_end_is_darker() {
        let p = cover_palette([200, 40, 60]);
        let Fill::Gradient(start, end) = p.fill else { panic!() };
        assert!(luminance(end) < luminance(start));
    }

    #[test]
    fn light_cover_gets_black_text() {
        assert_eq!(cover_palette([250, 240, 200]).title, Rgba::BLACK);
        assert_eq!(cover_palette([20, 30, 90]).title, Rgba::WHITE);
    }

    #[test]
    fn spotify_palette_uses_brand_colors() {
        let p = palette(Theme::Spotify, None).unwrap();
        assert_eq!(p.fill, Fill::Solid(Rgba::hex(0x121212)));
        assert_eq!(p.progress, Some(Rgba::hex(0x1DB954)));
        assert!(palette(Theme::System, None).is_none());
    }

    #[test]
    fn layouts_fit_cover_and_padding() {
        for theme in [Theme::System, Theme::Spotify, Theme::Cover] {
            let l = layout(theme);
            assert!((l.cover + 2.0 * l.padding - l.height).abs() < 0.01, "{theme:?}");
        }
    }

    #[test]
    fn saturation_of_gray_is_zero() {
        assert_eq!(saturation([128, 128, 128]), 0.0);
        assert_eq!(saturation([255, 0, 0]), 1.0);
    }
}
