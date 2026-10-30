//! The cover and text currently on the Windows card.

use crate::model::CardContent;

/// Content on screen, with the cover as premultiplied BGRA pixels.
#[derive(Clone, Default)]
pub struct Shown {
    pub content: CardContent,
    pub pixels: Option<(u32, u32, Vec<u8>)>,
}

impl Shown {
    pub fn new(content: CardContent) -> Self {
        let pixels = content.cover.as_deref().and_then(to_bgra);
        Shown { content, pixels }
    }

    pub fn set_cover(&mut self, png: Vec<u8>) {
        self.pixels = to_bgra(&png);
        self.content.accent = cover_accent(&png).or(self.content.accent);
        self.content.cover = Some(png);
    }
}

/// Dominant color of PNG cover bytes, for covers that arrive late.
fn cover_accent(png: &[u8]) -> Option<[u8; 3]> {
    image::load_from_memory(png)
        .ok()
        .map(|img| crate::art::dominant_color(&img))
}

/// Decode PNG bytes to premultiplied BGRA pixels for Direct2D.
fn to_bgra(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut px = img.into_raw();
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        let (r, g, b) = (p[0] as u32 * a / 255, p[1] as u32 * a / 255, p[2] as u32 * a / 255);
        p[0] = b as u8;
        p[1] = g as u8;
        p[2] = r as u8;
    }
    Some((w, h, px))
}
