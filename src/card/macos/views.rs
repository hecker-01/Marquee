//! AppKit views of the macOS card and their styling per theme.

use crate::theme::{self, Fill, Layout, Palette, Rgba};
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSFontWeightRegular, NSFontWeightSemibold, NSImage, NSImageScaling,
    NSImageSymbolConfiguration, NSImageView, NSLineBreakMode, NSPanel, NSTextField, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CGFloat;
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSArray, NSData, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{CAGradientLayer, CALayer};

/// NSFloatingWindowLevel.
const FLOATING_LEVEL: isize = 3;

pub struct Views {
    pub panel: Retained<NSPanel>,
    effect: Retained<NSVisualEffectView>,
    solid: Retained<NSView>,
    gradient: Retained<CAGradientLayer>,
    pub content: Retained<NSView>,
    pub image: Retained<NSImageView>,
    pub title: Retained<NSTextField>,
    pub artist: Retained<NSTextField>,
    pub album: Retained<NSTextField>,
    pub progress: Retained<NSView>,
}

fn cg(c: Rgba) -> objc2_core_foundation::CFRetained<CGColor> {
    CGColor::new_srgb(c.r as CGFloat, c.g as CGFloat, c.b as CGFloat, c.a as CGFloat)
}

fn ns(c: Rgba) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(c.r as CGFloat, c.g as CGFloat, c.b as CGFloat, c.a as CGFloat)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn label(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    field.setMaximumNumberOfLines(1);
    field
}

fn layer(view: &NSView) -> Option<Retained<CALayer>> {
    view.setWantsLayer(true);
    view.layer()
}

impl Views {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let frame = rect(0.0, 0.0, 344.0, 76.0);
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: we keep our own reference, so AppKit must not release it on close.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setFloatingPanel(true);
        panel.setLevel(FLOATING_LEVEL);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setIgnoresMouseEvents(true);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setOpaque(false);
        panel.setHasShadow(true);
        panel.setHidesOnDeactivate(false);
        panel.setBecomesKeyOnlyIfNeeded(true);

        let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
        effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        let solid = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let gradient = CAGradientLayer::new();
        if let Some(l) = layer(&solid) {
            l.addSublayer(&gradient);
        }
        let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let _ = layer(&content);
        let image = NSImageView::initWithFrame(NSImageView::alloc(mtm), frame);
        image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        let progress = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 0.0, 2.0));
        let (title, artist, album) = (label(mtm), label(mtm), label(mtm));
        for v in [&*image as &NSView, &title, &artist, &album] {
            content.addSubview(v);
        }
        root.addSubview(&effect);
        root.addSubview(&solid);
        root.addSubview(&content);
        root.addSubview(&progress);
        panel.setContentView(Some(&root));
        Views {
            panel,
            effect,
            solid,
            gradient,
            content,
            image,
            title,
            artist,
            album,
            progress,
        }
    }

    /// Lay out and color every view for a theme. Sizes are in points.
    pub fn style(&self, l: &Layout, palette: Option<Palette>) {
        let (w, h) = (l.width as f64, l.height as f64);
        let full = rect(0.0, 0.0, w, h);
        for v in [&*self.effect as &NSView, &self.solid, &self.content] {
            v.setFrame(full);
            if let Some(layer) = layer(v) {
                layer.setCornerRadius(l.corner_radius as CGFloat);
                layer.setMasksToBounds(true);
            }
        }
        self.gradient.setFrame(full);
        self.effect.setHidden(palette.is_some());
        self.solid.setHidden(palette.is_none());

        let pad = l.padding as f64;
        let cover = l.cover as f64;
        self.image.setFrame(rect(pad, pad, cover, cover));
        if let Some(layer) = layer(&self.image) {
            layer.setCornerRadius(l.cover_radius as CGFloat);
            layer.setMasksToBounds(true);
        }
        let x = pad + cover + l.gap as f64;
        let text_w = w - x - pad;
        let lines = [
            (&self.title, l.title_size),
            (&self.artist, l.artist_size),
            (&self.album, l.album_size),
        ];
        let heights: Vec<f64> = lines.iter().map(|(_, s)| (*s as f64 * 1.3).round()).collect();
        let total: f64 = heights.iter().sum::<f64>() + 2.0;
        // AppKit counts y from the bottom, so the first line sits highest.
        let mut top = ((h + total) / 2.0).round();
        for ((field, _), lh) in lines.iter().zip(&heights) {
            top -= lh;
            field.setFrame(rect(x, top, text_w, *lh));
            top -= 1.0;
        }
        // SAFETY: the font weight constants are immutable AppKit globals.
        let (semibold, regular) = unsafe { (NSFontWeightSemibold, NSFontWeightRegular) };
        let title_font = if l.title_bold {
            NSFont::boldSystemFontOfSize(l.title_size as CGFloat)
        } else {
            NSFont::systemFontOfSize_weight(l.title_size as CGFloat, semibold)
        };
        self.title.setFont(Some(&title_font));
        self.artist.setFont(Some(&NSFont::systemFontOfSize_weight(
            l.artist_size as CGFloat,
            regular,
        )));
        self.album
            .setFont(Some(&NSFont::systemFontOfSize_weight(l.album_size as CGFloat, regular)));

        match palette {
            None => {
                self.title.setTextColor(Some(&NSColor::labelColor()));
                self.artist.setTextColor(Some(&NSColor::secondaryLabelColor()));
                self.album.setTextColor(Some(&NSColor::tertiaryLabelColor()));
            }
            Some(p) => {
                self.title.setTextColor(Some(&ns(p.title)));
                self.artist.setTextColor(Some(&ns(p.artist)));
                self.album.setTextColor(Some(&ns(p.album)));
                let (a, b) = match p.fill {
                    Fill::Solid(c) => (c, c),
                    Fill::Gradient(a, b) => (a, b),
                };
                let colors = NSArray::from_retained_slice(&[cg(a), cg(b)].map(|c| {
                    // SAFETY: CGColor is toll-free bridged to an Objective-C object.
                    unsafe {
                        Retained::retain(
                            objc2_core_foundation::CFRetained::as_ptr(&c)
                                .as_ptr()
                                .cast::<objc2::runtime::AnyObject>(),
                        )
                        .expect("color")
                    }
                }));
                // SAFETY: CAGradientLayer.colors takes an array of CGColor objects.
                unsafe { self.gradient.setColors(Some(&colors)) };
                self.gradient
                    .setStartPoint(objc2_core_foundation::CGPoint::new(0.0, 1.0));
                self.gradient.setEndPoint(objc2_core_foundation::CGPoint::new(1.0, 0.0));
                if let Some(layer) = layer(&self.solid) {
                    let border = p.border.unwrap_or(Rgba::BLACK.with_alpha(0.0));
                    layer.setBorderColor(Some(&cg(border)));
                    layer.setBorderWidth(if p.border.is_some() { 1.0 } else { 0.0 });
                }
            }
        }
        if let Some(layer) = layer(&self.progress) {
            let green = palette.and_then(|p| p.progress).unwrap_or(Rgba::BLACK.with_alpha(0.0));
            layer.setBackgroundColor(Some(&cg(green)));
        }
        self.progress.setFrame(rect(0.0, 0.0, 0.0, 2.0));
    }

    /// Fill the labels and the cover. Without a cover, show the music note tile.
    pub fn fill(&self, content: &crate::model::CardContent, cover_pt: f64) {
        self.title.setStringValue(&NSString::from_str(&content.title));
        self.artist.setStringValue(&NSString::from_str(&content.artist));
        self.album.setStringValue(&NSString::from_str(&content.album));
        self.set_cover(content.cover.as_deref(), cover_pt);
    }

    pub fn set_cover(&self, png: Option<&[u8]>, cover_pt: f64) {
        let image = png.and_then(|bytes| NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(bytes)));
        let layer = layer(&self.image);
        match image {
            Some(img) => {
                img.setSize(NSSize::new(cover_pt, cover_pt));
                self.image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                self.image.setImage(Some(&img));
                self.image.setSymbolConfiguration(None);
                if let Some(l) = layer {
                    l.setBackgroundColor(None);
                }
            }
            None => {
                let name = NSString::from_str("music.note");
                let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None);
                let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(cover_pt * 0.4, 0.0);
                self.image.setImageScaling(NSImageScaling::ScaleNone);
                self.image.setSymbolConfiguration(Some(&config));
                self.image.setImage(symbol.as_deref());
                self.image.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
                if let Some(l) = layer {
                    l.setBackgroundColor(Some(&cg(theme::Rgba::hex(0x808080).with_alpha(0.18))));
                }
            }
        }
    }
}
