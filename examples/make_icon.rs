//! Writes the app icon (the tray sign on a dark rounded square): `cargo run --example make_icon -- out.png`.
use image::{Rgba, RgbaImage};

fn main() {
    let n = 1024u32;
    let out = std::env::args().nth(1).expect("usage: make_icon <out.png>");
    let sign = marquee::tray::icon_rgba(n, [0xF5, 0xB8, 0x3D]);
    let mut img = RgbaImage::new(n, n);
    let (m, r) = (n as f32 * 0.1, n as f32 * 0.18); // macOS icon grid margin and corner radius
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(m + r, n as f32 - m - r);
            let cy = fy.clamp(m + r, n as f32 - m - r);
            let a = (r - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
            // Sign scaled to 60% and centered on the square.
            let (sx, sy) = (
                ((fx - n as f32 / 2.0) / 0.6 + n as f32 / 2.0) as i32,
                ((fy - n as f32 / 2.0) / 0.6 + n as f32 / 2.0) as i32,
            );
            let s = if (0..n as i32).contains(&sx) && (0..n as i32).contains(&sy) {
                let i = ((sy as u32 * n + sx as u32) * 4) as usize;
                [sign[i], sign[i + 1], sign[i + 2], sign[i + 3]]
            } else {
                [0; 4]
            };
            let sa = s[3] as f32 / 255.0;
            let mix = |bg: f32, fg: u8| (bg * (1.0 - sa) + fg as f32 * sa) as u8;
            img.put_pixel(
                x,
                y,
                Rgba([
                    mix(0x1C as f32, s[0]),
                    mix(0x1C as f32, s[1]),
                    mix(0x20 as f32, s[2]),
                    (a * 255.0) as u8,
                ]),
            );
        }
    }
    img.save(out).unwrap();
}
