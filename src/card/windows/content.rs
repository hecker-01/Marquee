//! Drawing of cover and text, the part of a frame that crossfades.

use super::cover::Shown;
use super::draw::{Renderer, Scene, color, rect, rounded};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Result;
use windows_numerics::Matrix3x2;

impl Renderer {
    /// Draw cover and text at the given opacity.
    pub(super) unsafe fn content(
        &mut self,
        t: &ID2D1HwndRenderTarget,
        s: &Scene,
        shown: &Shown,
        alpha: f32,
        family: &'static str,
    ) -> Result<()> {
        if alpha <= 0.0 {
            return Ok(());
        }
        let l = s.layout;
        let (primary, secondary) = match s.palette {
            Some(p) => (p.title, p.artist),
            None => {
                let fg = s.system.foreground;
                (fg, fg.with_alpha(if s.system.dark { 0.786 } else { 0.62 }))
            }
        };
        let album_color = s.palette.map(|p| p.album).unwrap_or(secondary);
        let cover = rect(l.padding, l.padding, l.cover, l.cover);
        // SAFETY: called from `draw` between BeginDraw and EndDraw.
        unsafe {
            match &shown.pixels {
                Some((w, h, px)) => {
                    let props = D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    };
                    let bmp = t.CreateBitmap(
                        D2D_SIZE_U { width: *w, height: *h },
                        Some(px.as_ptr() as _),
                        w * 4,
                        &props,
                    )?;
                    let brush = t.CreateBitmapBrush(&bmp, None, None)?;
                    let k = l.cover / *w as f32;
                    brush.SetTransform(&Matrix3x2 {
                        M11: k,
                        M12: 0.0,
                        M21: 0.0,
                        M22: k,
                        M31: l.padding,
                        M32: l.padding,
                    });
                    brush.SetOpacity(alpha);
                    t.FillRoundedRectangle(&rounded(cover, l.cover_radius), &brush);
                }
                None => {
                    let tile = t.CreateSolidColorBrush(&color(primary.with_alpha(0.1), alpha), None)?;
                    t.FillRoundedRectangle(&rounded(cover, l.cover_radius), &tile);
                    let note = if s.palette.is_none() {
                        s.system.accent
                    } else {
                        secondary
                    };
                    let brush = t.CreateSolidColorBrush(&color(note, alpha), None)?;
                    let icon = self.formats(&l, family)?.icon.clone();
                    t.DrawText(
                        &[0xE8D6],
                        &icon,
                        &cover,
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
            let f = self.formats(&l, family)?;
            let x = l.padding + l.cover + l.gap;
            let w = l.width - x - l.padding;
            let lines = [
                (&shown.content.title, &f.title, l.title_size, primary),
                (&shown.content.artist, &f.artist, l.artist_size, secondary),
                (&shown.content.album, &f.album, l.album_size, album_color),
            ];
            let heights: Vec<f32> = lines.iter().map(|(_, _, size, _)| (size * 1.36).round()).collect();
            let spacing = 1.0;
            let total: f32 = heights.iter().sum::<f32>() + spacing * 2.0;
            let mut y = ((l.height - total) / 2.0).round();
            for ((text, format, _, c), h) in lines.iter().zip(heights) {
                if !text.is_empty() {
                    let brush = t.CreateSolidColorBrush(&color(*c, alpha), None)?;
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    t.DrawText(
                        &wide,
                        *format,
                        &rect(x, y, w, h),
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
                y += h + spacing;
            }
        }
        Ok(())
    }
}
