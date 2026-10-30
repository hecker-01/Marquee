//! Direct2D and DirectWrite drawing of the Windows card.

use super::cover::Shown;
use super::system::{SystemColors, first_font};
use crate::theme::{Fill, Layout, Palette, Rgba};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{HSTRING, Result};
use windows_numerics::{Matrix3x2, Vector2};

/// Everything one frame needs.
pub struct Scene<'a> {
    pub layout: Layout,
    pub palette: Option<Palette>,
    pub system: SystemColors,
    /// The DWM backdrop shows through; clear to transparent.
    pub backdrop: bool,
    /// Windows 10: no DWM border, so the card draws its own.
    pub draw_border: bool,
    pub current: &'a Shown,
    /// The previous content and its remaining opacity during a crossfade.
    pub previous: Option<(&'a Shown, f32)>,
    pub progress: Option<f32>,
}

pub(super) struct Formats {
    key: (u32, u32, bool),
    pub title: IDWriteTextFormat,
    pub artist: IDWriteTextFormat,
    pub album: IDWriteTextFormat,
    pub icon: IDWriteTextFormat,
}

pub struct Renderer {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: Option<(ID2D1HwndRenderTarget, (u32, u32, u32))>,
    formats: Option<Formats>,
    pub ui_font: &'static str,
    pub plain_font: &'static str,
    icon_font: &'static str,
}

/// Tint over the acrylic backdrop: the Fluent layer colors at an opacity
/// matched by pixel against a Windows 11 notification (both about RGB 33 on a
/// dark desktop).
const ACRYLIC_TINT: f32 = 0.92;
const TINT_DARK: u32 = 0x202020;
const TINT_LIGHT: u32 = 0xF3F3F3;

pub(super) fn color(c: Rgba, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a * alpha,
    }
}

pub(super) fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

pub(super) fn rounded(r: D2D_RECT_F, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: r,
        radiusX: radius,
        radiusY: radius,
    }
}

impl Renderer {
    pub fn new() -> Result<Self> {
        // SAFETY: factory creation has no preconditions.
        let (d2d, dwrite) = unsafe {
            (
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)?,
            )
        };
        let ui_font = first_font(&dwrite, &["Segoe UI Variable Text", "Segoe UI"]);
        let icon_font = first_font(&dwrite, &["Segoe Fluent Icons", "Segoe MDL2 Assets"]);
        Ok(Renderer {
            d2d,
            dwrite,
            target: None,
            formats: None,
            ui_font,
            plain_font: "Segoe UI",
            icon_font,
        })
    }

    fn format(&self, family: &str, weight: DWRITE_FONT_WEIGHT, size: f32) -> Result<IDWriteTextFormat> {
        // SAFETY: COM calls on live factory objects with valid arguments.
        unsafe {
            let f = self.dwrite.CreateTextFormat(
                &HSTRING::from(family),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &HSTRING::from("en-us"),
            )?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let sign = self.dwrite.CreateEllipsisTrimmingSign(&f)?;
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            f.SetTrimming(&trimming, &sign)?;
            Ok(f)
        }
    }

    pub(super) fn formats(&mut self, l: &Layout, family: &'static str) -> Result<&Formats> {
        let key = (
            l.title_size.to_bits(),
            l.artist_size.to_bits(),
            family == self.plain_font,
        );
        if self.formats.as_ref().map(|f| f.key) != Some(key) {
            let weight = if l.title_bold {
                DWRITE_FONT_WEIGHT_BOLD
            } else {
                DWRITE_FONT_WEIGHT_SEMI_BOLD
            };
            let icon = self.format(self.icon_font, DWRITE_FONT_WEIGHT_NORMAL, l.cover * 0.4)?;
            // SAFETY: setting alignment on a fresh text format.
            unsafe {
                icon.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                icon.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            }
            self.formats = Some(Formats {
                key,
                title: self.format(family, weight, l.title_size)?,
                artist: self.format(family, DWRITE_FONT_WEIGHT_NORMAL, l.artist_size)?,
                album: self.format(family, DWRITE_FONT_WEIGHT_NORMAL, l.album_size)?,
                icon,
            });
        }
        Ok(self.formats.as_ref().expect("set above"))
    }

    fn target(&mut self, hwnd: HWND, w: u32, h: u32, dpi: u32) -> Result<ID2D1HwndRenderTarget> {
        if let Some((t, key)) = &self.target
            && *key == (w, h, dpi)
        {
            return Ok(t.clone());
        }
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U { width: w, height: h },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: the window handle is alive; property structs live for the call.
        let t = unsafe { self.d2d.CreateHwndRenderTarget(&props, &hwnd_props)? };
        self.target = Some((t.clone(), (w, h, dpi)));
        Ok(t)
    }

    /// Draw one frame. `size` is in physical pixels.
    pub fn draw(&mut self, hwnd: HWND, size: (u32, u32), scale: f32, scene: &Scene) -> Result<()> {
        let family = if scene.palette.is_some() && scene.layout.title_bold {
            self.plain_font
        } else {
            self.ui_font
        };
        let t = self.target(hwnd, size.0, size.1, (96.0 * scale).round() as u32)?;
        let l = scene.layout;
        // SAFETY: all drawing happens between BeginDraw and EndDraw on the UI
        // thread that owns the render target.
        unsafe {
            t.BeginDraw();
            t.SetTransform(&Matrix3x2::identity());
            let full = rect(0.0, 0.0, l.width, l.height);
            match scene.palette.map(|p| p.fill) {
                Some(Fill::Solid(c)) => t.Clear(Some(&color(c, 1.0))),
                Some(Fill::Gradient(a, b)) => {
                    t.Clear(Some(&color(a, 1.0)));
                    let stops = [
                        D2D1_GRADIENT_STOP {
                            position: 0.0,
                            color: color(a, 1.0),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.0,
                            color: color(b, 1.0),
                        },
                    ];
                    let coll = t.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
                    let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: Vector2 { X: 0.0, Y: 0.0 },
                        endPoint: Vector2 {
                            X: l.width,
                            Y: l.height,
                        },
                    };
                    let brush = t.CreateLinearGradientBrush(&props, None, &coll)?;
                    t.FillRectangle(&full, &brush);
                }
                None if scene.backdrop => {
                    // Clear to transparent so the acrylic shows, then lay the
                    // Fluent tint on top, as system notifications do.
                    t.Clear(Some(&D2D1_COLOR_F::default()));
                    let c = Rgba::hex(if scene.system.dark { TINT_DARK } else { TINT_LIGHT });
                    let tint = t.CreateSolidColorBrush(&color(c, ACRYLIC_TINT), None)?;
                    t.FillRectangle(&full, &tint);
                }
                None => t.Clear(Some(&color(scene.system.background, 1.0))),
            }
            if scene.draw_border {
                let c = scene
                    .palette
                    .and_then(|p| p.border)
                    .unwrap_or(scene.system.foreground.with_alpha(0.12));
                let brush = t.CreateSolidColorBrush(&color(c, 1.0), None)?;
                let r = rounded(rect(0.5, 0.5, l.width - 1.0, l.height - 1.0), l.corner_radius);
                t.DrawRoundedRectangle(&r, &brush, 1.0, None);
            }
            if let (Some(p), Some(color_p)) = (scene.progress, scene.palette.and_then(|p| p.progress)) {
                let brush = t.CreateSolidColorBrush(&color(color_p, 1.0), None)?;
                t.FillRectangle(&rect(0.0, l.height - 2.0, l.width * p.clamp(0.0, 1.0), 2.0), &brush);
            }
            if let Some((prev, alpha)) = scene.previous {
                self.content(&t, scene, prev, alpha, family)?;
            }
            let fade_in = scene.previous.map_or(1.0, |(_, a)| 1.0 - a);
            self.content(&t, scene, scene.current, fade_in, family)?;
            if let Err(e) = t.EndDraw(None, None) {
                // The device was lost; build a new target next frame.
                self.target = None;
                return Err(e);
            }
        }
        Ok(())
    }
}
