//! Easing curves and the card timeline. Pure functions.
//!
//! The Windows card steps through these frames itself. The macOS card takes
//! only the durations and lets Core Animation run the curves.

#![cfg_attr(target_os = "macos", allow(dead_code))]

use std::time::Duration;

pub const SLIDE: Duration = Duration::from_millis(300);
pub const REDUCED_FADE: Duration = Duration::from_millis(150);
pub const CROSSFADE: Duration = Duration::from_millis(250);

/// Decelerating curve: fast start, soft landing.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Accelerating curve for leaving the screen.
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// The phases of one card appearance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    pub enter: Duration,
    pub hold: Duration,
    pub exit: Duration,
    /// When true the card only fades and does not slide.
    pub reduced_motion: bool,
}

/// Where the card is at one moment of the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// 0 is fully off-screen at the slide start, 1 is at rest.
    pub position: f32,
    pub opacity: f32,
    /// Share of the hold time that has passed, for the progress line.
    pub progress: f32,
    pub done: bool,
}

impl Timeline {
    pub fn new(hold: Duration, reduced_motion: bool) -> Self {
        let edge = if reduced_motion { REDUCED_FADE } else { SLIDE };
        Timeline {
            enter: edge,
            hold,
            exit: edge,
            reduced_motion,
        }
    }

    pub fn total(&self) -> Duration {
        self.enter + self.hold + self.exit
    }

    /// Whether `elapsed` falls inside the hold phase, where nothing moves.
    pub fn is_holding(&self, elapsed: Duration) -> bool {
        elapsed >= self.enter && elapsed < self.enter + self.hold
    }

    pub fn frame(&self, elapsed: Duration) -> Frame {
        let secs = |d: Duration| d.as_secs_f32().max(f32::EPSILON);
        let (mut position, opacity, progress);
        if elapsed < self.enter {
            let t = elapsed.as_secs_f32() / secs(self.enter);
            position = ease_out_cubic(t);
            opacity = position;
            progress = 0.0;
        } else if elapsed < self.enter + self.hold {
            position = 1.0;
            opacity = 1.0;
            progress = (elapsed - self.enter).as_secs_f32() / secs(self.hold);
        } else {
            let t = (elapsed - self.enter - self.hold).as_secs_f32() / secs(self.exit);
            position = 1.0 - ease_in_cubic(t);
            opacity = position;
            progress = 1.0;
        }
        if self.reduced_motion {
            position = 1.0;
        }
        Frame {
            position,
            opacity,
            progress,
            done: elapsed >= self.total(),
        }
    }

    /// The elapsed time to jump to when a new song arrives while the card
    /// shows: stay at rest and start the hold again. While entering, keep going.
    pub fn restart_hold(&self, elapsed: Duration) -> Duration {
        if elapsed < self.enter {
            return elapsed;
        }
        if elapsed < self.enter + self.hold {
            return self.enter;
        }
        // Leaving: walk back to the matching point of the enter phase.
        let out = elapsed - self.enter - self.hold;
        let t = 1.0 - ease_in_cubic(out.as_secs_f32() / self.exit.as_secs_f32().max(f32::EPSILON));
        let back = inverse_ease_out(t);
        self.enter.mul_f32(back)
    }
}

/// Inverse of `ease_out_cubic`.
fn inverse_ease_out(y: f32) -> f32 {
    1.0 - (1.0 - y.clamp(0.0, 1.0)).cbrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn easing_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5, "decelerating curve runs ahead");
        assert_eq!(ease_in_cubic(1.0), 1.0);
    }

    #[test]
    fn phases() {
        let tl = Timeline::new(ms(5000), false);
        assert_eq!(tl.total(), ms(5600));
        let f = tl.frame(ms(0));
        assert_eq!((f.position, f.opacity), (0.0, 0.0));
        let f = tl.frame(ms(2800));
        assert_eq!((f.position, f.opacity), (1.0, 1.0));
        assert!((f.progress - 0.5).abs() < 0.01);
        assert!(tl.frame(ms(5600)).done);
        assert!(!tl.frame(ms(5599)).done);
        assert!(tl.is_holding(ms(300)) && !tl.is_holding(ms(5300)));
    }

    #[test]
    fn reduced_motion_fades_without_sliding() {
        let tl = Timeline::new(ms(5000), true);
        assert_eq!(tl.enter, ms(150));
        let f = tl.frame(ms(75));
        assert_eq!(f.position, 1.0);
        assert!(f.opacity > 0.0 && f.opacity < 1.0);
    }

    #[test]
    fn restart_hold_resets_timer() {
        let tl = Timeline::new(ms(5000), false);
        assert_eq!(tl.restart_hold(ms(100)), ms(100));
        assert_eq!(tl.restart_hold(ms(4000)), ms(300));
        // Halfway out: the card comes back from where it is, without a jump.
        let out = ms(5450);
        let back = tl.restart_hold(out);
        let before = tl.frame(out).position;
        let after = tl.frame(back).position;
        assert!((before - after).abs() < 0.01, "{before} vs {after}");
    }
}
