//! The macOS card: a non-activating NSPanel with a visual effect backdrop,
//! animated with NSAnimationContext. Not yet run on a Mac; see the README.

mod views;

use super::Card;
use crate::anim::{CROSSFADE, SLIDE, Timeline};
use crate::model::CardContent;
use crate::placement::{self, GAP, Rect};
use crate::settings::{MonitorMode, Settings};
use crate::theme;
use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSAnimatablePropertyContainer, NSAnimationContext, NSEvent, NSScreen, NSWorkspace};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};
use objc2_quartz_core::{
    CAMediaTiming, CAMediaTimingFunction, CATransition, kCAMediaTimingFunctionEaseIn, kCAMediaTimingFunctionEaseOut,
    kCAMediaTimingFunctionLinear, kCATransitionFade,
};
use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::Rc;
use views::Views;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Hidden,
    Entering,
    Holding,
    Leaving,
}

struct Inner {
    views: Views,
    phase: Cell<Phase>,
    /// Bumped on every show and hide so stale timers and completions do nothing.
    generation: Cell<u64>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    rest: Cell<NSRect>,
    start: Cell<NSRect>,
    edge: Cell<f64>,
    hold: Cell<f64>,
    layout: Cell<theme::Layout>,
}

pub struct MacCard {
    inner: Rc<Inner>,
}

impl MacCard {
    pub fn new() -> anyhow::Result<Self> {
        let mtm =
            MainThreadMarker::new().ok_or_else(|| anyhow::anyhow!("the card must be created on the main thread"))?;
        let empty = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        let inner = Rc::new(Inner {
            views: Views::new(mtm),
            phase: Cell::new(Phase::Hidden),
            generation: Cell::new(0),
            timer: RefCell::new(None),
            rest: Cell::new(empty),
            start: Cell::new(empty),
            edge: Cell::new(SLIDE.as_secs_f64()),
            hold: Cell::new(5.0),
            layout: Cell::new(theme::layout(crate::settings::Theme::System)),
        });
        Ok(MacCard { inner })
    }
}

/// The visible frame of the chosen screen, flipped to a top-left origin for
/// the placement math, plus the height of the main screen to flip back.
fn work_area(mode: MonitorMode, mtm: MainThreadMarker) -> Option<(Rect, f64)> {
    let screens = NSScreen::screens(mtm);
    let primary_h = screens.firstObject()?.frame().size.height;
    let flip = |r: NSRect| Rect {
        x: r.origin.x,
        y: primary_h - r.origin.y - r.size.height,
        w: r.size.width,
        h: r.size.height,
    };
    let rects: Vec<Rect> = screens.iter().map(|s| flip(s.visibleFrame())).collect();
    let frames: Vec<Rect> = screens.iter().map(|s| flip(s.frame())).collect();
    let index = match mode {
        MonitorMode::Primary => 0,
        MonitorMode::Cursor => {
            let p = NSEvent::mouseLocation();
            let hit = placement::screen_at(&frames, (p.x, primary_h - p.y))?;
            frames.iter().position(|f| *f == hit).unwrap_or(0)
        }
    };
    Some((rects[index], primary_h))
}

fn to_appkit(x: f64, y: f64, w: f64, h: f64, primary_h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, primary_h - y - h), NSSize::new(w, h))
}

fn timing(name: &'static NSString) -> Retained<CAMediaTimingFunction> {
    CAMediaTimingFunction::functionWithName(name)
}

impl Inner {
    /// Animate the panel to a frame and opacity, then run `done` if nothing
    /// newer happened in between.
    fn animate(
        self: &Rc<Self>,
        frame: NSRect,
        alpha: f64,
        secs: f64,
        curve: &'static NSString,
        done: impl Fn(&Rc<Inner>) + 'static,
    ) {
        let generation = self.generation.get();
        let me = self.clone();
        let changes = RcBlock::new(move |ctx: NonNull<NSAnimationContext>| {
            // SAFETY: AppKit passes a valid context for the duration of the block.
            let ctx = unsafe { ctx.as_ref() };
            ctx.setDuration(secs);
            ctx.setTimingFunction(Some(&timing(curve)));
            let animator = me.views.panel.animator();
            animator.setFrame_display(frame, true);
            animator.setAlphaValue(alpha);
        });
        let me = self.clone();
        let completion = RcBlock::new(move || {
            if me.generation.get() == generation {
                done(&me);
            }
        });
        NSAnimationContext::runAnimationGroup_completionHandler(&changes, Some(&completion));
    }

    fn enter(self: &Rc<Self>) {
        self.phase.set(Phase::Entering);
        self.animate(
            self.rest.get(),
            1.0,
            self.edge.get(),
            unsafe { kCAMediaTimingFunctionEaseOut },
            |me| me.start_hold(),
        );
    }

    fn start_hold(self: &Rc<Self>) {
        self.phase.set(Phase::Holding);
        if let Some(t) = self.timer.borrow_mut().take() {
            t.invalidate();
        }
        let l = self.layout.get();
        let progress = &self.views.progress;
        progress.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 2.0)));
        let hold = self.hold.get();
        let bar = progress.clone();
        let grow = RcBlock::new(move |ctx: NonNull<NSAnimationContext>| {
            // SAFETY: AppKit passes a valid context for the duration of the block.
            let ctx = unsafe { ctx.as_ref() };
            ctx.setDuration(hold);
            ctx.setTimingFunction(Some(&timing(unsafe { kCAMediaTimingFunctionLinear })));
            bar.animator()
                .setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(l.width as f64, 2.0)));
        });
        NSAnimationContext::runAnimationGroup(&grow);
        let generation = self.generation.get();
        let me = self.clone();
        let fire = RcBlock::new(move |_: NonNull<NSTimer>| {
            if me.generation.get() == generation && me.phase.get() == Phase::Holding {
                me.leave();
            }
        });
        // SAFETY: the timer retains the block; it runs once on the main run loop.
        let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(hold, false, &fire) };
        *self.timer.borrow_mut() = Some(timer);
    }

    fn leave(self: &Rc<Self>) {
        self.phase.set(Phase::Leaving);
        self.animate(
            self.start.get(),
            0.0,
            self.edge.get(),
            unsafe { kCAMediaTimingFunctionEaseIn },
            |me| me.hide(),
        );
    }

    fn hide(&self) {
        self.generation.set(self.generation.get() + 1);
        self.phase.set(Phase::Hidden);
        if let Some(t) = self.timer.borrow_mut().take() {
            t.invalidate();
        }
        self.views.panel.orderOut(None);
    }

    /// Fade the content view to its new state.
    fn crossfade(&self) {
        let transition = CATransition::animation();
        // SAFETY: kCATransitionFade is an immutable QuartzCore global.
        transition.setType(unsafe { kCATransitionFade });
        transition.setDuration(CROSSFADE.as_secs_f64());
        if let Some(layer) = self.views.content.layer() {
            layer.addAnimation_forKey(&transition, Some(&NSString::from_str("crossfade")));
        }
    }
}

impl Card for MacCard {
    fn show(&mut self, content: CardContent, settings: &Settings) {
        let me = &self.inner;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some((work, primary_h)) = work_area(settings.monitor, mtm) else {
            return;
        };
        let l = theme::layout(settings.theme);
        let size = (l.width as f64, l.height as f64);
        let (rx, ry) = placement::rest_position(work, size, settings.position, GAP);
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        let (sx, sy) = if reduced {
            (rx, ry)
        } else {
            placement::start_position(work, size, settings.position, GAP)
        };
        me.rest.set(to_appkit(rx, ry, size.0, size.1, primary_h));
        me.start.set(to_appkit(sx, sy, size.0, size.1, primary_h));
        let timeline = Timeline::new(std::time::Duration::from_secs(settings.duration_secs), reduced);
        me.edge.set(timeline.enter.as_secs_f64());
        me.hold.set(timeline.hold.as_secs_f64());
        me.layout.set(l);

        let palette = theme::palette(settings.theme, content.accent);
        match me.phase.get() {
            Phase::Hidden => {
                me.views.style(&l, palette);
                me.views.fill(&content, l.cover as f64);
                me.generation.set(me.generation.get() + 1);
                me.views.panel.setAlphaValue(0.0);
                me.views.panel.setFrame_display(me.start.get(), false);
                me.views.panel.orderFrontRegardless();
                me.enter();
            }
            phase => {
                // A new song while the card shows: crossfade and restart the hold.
                me.crossfade();
                me.views.style(&l, palette);
                me.views.fill(&content, l.cover as f64);
                me.generation.set(me.generation.get() + 1);
                if phase == Phase::Leaving {
                    me.enter();
                } else if phase == Phase::Holding {
                    me.start_hold();
                } else {
                    me.enter();
                }
            }
        }
    }

    fn update_cover(&mut self, cover: Vec<u8>) {
        let l = self.inner.layout.get();
        if self.inner.phase.get() != Phase::Hidden {
            self.inner.crossfade();
        }
        self.inner.views.set_cover(Some(&cover), l.cover as f64);
    }

    fn hide(&mut self) {
        self.inner.hide();
    }
}
