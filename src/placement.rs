//! Card position math. Pure functions.
//!
//! Rectangles use a top-left origin with y growing down. The macOS card flips
//! the result into AppKit coordinates itself.

use crate::settings::Corner;

pub const GAP: f64 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Where the card rests inside a work area.
pub fn rest_position(work: Rect, size: (f64, f64), corner: Corner, gap: f64) -> (f64, f64) {
    let x = if corner.is_left() {
        work.x + gap
    } else {
        work.x + work.w - gap - size.0
    };
    let y = if corner.is_top() {
        work.y + gap
    } else {
        work.y + work.h - gap - size.1
    };
    (x, y)
}

/// Where the slide starts: just past the nearest side edge of the work area.
pub fn start_position(work: Rect, size: (f64, f64), corner: Corner, gap: f64) -> (f64, f64) {
    let (x, y) = rest_position(work, size, corner, gap);
    let distance = size.0 + gap;
    if corner.is_left() {
        (x - distance, y)
    } else {
        (x + distance, y)
    }
}

/// Position at `t` of the slide, where 0 is the start and 1 is at rest.
/// The Windows card steps the slide itself; macOS hands both ends to Core Animation.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn slide_position(work: Rect, size: (f64, f64), corner: Corner, gap: f64, t: f32) -> (f64, f64) {
    let (sx, sy) = start_position(work, size, corner, gap);
    let (rx, ry) = rest_position(work, size, corner, gap);
    let t = t as f64;
    (sx + (rx - sx) * t, sy + (ry - sy) * t)
}

/// Pick the screen that contains the point, falling back to the first one.
/// Windows asks the system for the monitor under the cursor instead.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn screen_at(screens: &[Rect], point: (f64, f64)) -> Option<Rect> {
    screens
        .iter()
        .copied()
        .find(|s| s.contains(point.0, point.1))
        .or_else(|| screens.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0, // 1080 minus a 48 px taskbar
    };
    const SIZE: (f64, f64) = (364.0, 92.0);

    #[test]
    fn corners_keep_the_gap() {
        assert_eq!(rest_position(WORK, SIZE, Corner::TopLeft, GAP), (16.0, 16.0));
        assert_eq!(rest_position(WORK, SIZE, Corner::TopRight, GAP), (1540.0, 16.0));
        assert_eq!(rest_position(WORK, SIZE, Corner::BottomLeft, GAP), (16.0, 924.0));
        assert_eq!(rest_position(WORK, SIZE, Corner::BottomRight, GAP), (1540.0, 924.0));
    }

    #[test]
    fn bottom_corner_stays_above_taskbar() {
        let (_, y) = rest_position(WORK, SIZE, Corner::BottomRight, GAP);
        assert!(y + SIZE.1 <= WORK.h - GAP);
    }

    #[test]
    fn offset_work_area_on_second_monitor() {
        let work = Rect {
            x: -1280.0,
            y: 200.0,
            w: 1280.0,
            h: 984.0,
        };
        assert_eq!(rest_position(work, SIZE, Corner::TopLeft, GAP), (-1264.0, 216.0));
    }

    #[test]
    fn slide_starts_off_the_near_edge() {
        let (x, _) = start_position(WORK, SIZE, Corner::BottomRight, GAP);
        assert!(x >= WORK.w);
        let (x, _) = start_position(WORK, SIZE, Corner::TopLeft, GAP);
        assert!(x + SIZE.0 <= 0.0);
        let end = slide_position(WORK, SIZE, Corner::TopLeft, GAP, 1.0);
        assert_eq!(end, rest_position(WORK, SIZE, Corner::TopLeft, GAP));
    }

    #[test]
    fn screen_under_point() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let b = Rect {
            x: 100.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        assert_eq!(screen_at(&[a, b], (150.0, 10.0)), Some(b));
        assert_eq!(screen_at(&[a, b], (500.0, 10.0)), Some(a));
        assert_eq!(screen_at(&[], (0.0, 0.0)), None);
    }
}
