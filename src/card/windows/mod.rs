//! The Windows card: a layered, click-through popup with a DWM backdrop,
//! drawn with Direct2D and DirectWrite and animated by a 60 Hz timer.
//!
//! The window exists only while a card shows. As long as a process owns a
//! top-level window, some GPU drivers and overlays load into it, which costs
//! about 50 MB; destroying the window on hide keeps the idle app small.

mod content;
mod cover;
mod draw;
mod system;
mod window;

use super::Card;
use crate::anim::{CROSSFADE, Timeline};
use crate::model::CardContent;
use crate::placement::{GAP, Rect, slide_position};
use crate::settings::{Corner, Settings, Theme};
use crate::theme::{self, Layout};
use cover::Shown;
use draw::{Renderer, Scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use system::{Colors, SystemColors};
use window::wndproc;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const FRAME_TIMER: usize = 1;
const HOLD_TIMER: usize = 2;
const FRAME_MS: u32 = 16;

struct State {
    /// Null while no card shows.
    hwnd: HWND,
    /// The window that system color changes are posted to, 0 when none.
    color_target: Arc<AtomicIsize>,
    /// Created for each appearance and dropped on hide, like the window.
    renderer: Option<Renderer>,
    colors: Option<Colors>,
    system: SystemColors,
    theme: Theme,
    corner: Corner,
    layout: Layout,
    work: Rect,
    scale: f32,
    timeline: Timeline,
    start: Instant,
    current: Shown,
    previous: Option<(Shown, Instant)>,
    visible: bool,
    backdrop: bool,
    dwm_corners: bool,
}

pub struct WinCard {
    state: Rc<RefCell<State>>,
}

impl WinCard {
    pub fn new() -> anyhow::Result<Self> {
        // SAFETY: registering a window class with a static name and procedure.
        unsafe {
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: GetModuleHandleW(None)?.into(),
                lpszClassName: w!("MarqueeCard"),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            RegisterClassExW(&class);
        }
        let color_target = Arc::new(AtomicIsize::new(0));
        let colors = Colors::watch(color_target.clone())
            .map_err(|e| tracing::warn!("no UISettings: {e:#}"))
            .ok();
        let state = Rc::new(RefCell::new(State {
            hwnd: HWND::default(),
            color_target,
            renderer: None,
            colors,
            system: SystemColors {
                dark: true,
                foreground: theme::Rgba::WHITE,
                background: theme::Rgba::hex(0x202020),
                accent: theme::Rgba::hex(0x4CC2FF),
            },
            theme: Theme::System,
            corner: Corner::BottomRight,
            layout: theme::layout(Theme::System),
            work: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            scale: 1.0,
            timeline: Timeline::new(Duration::from_secs(5), false),
            start: Instant::now(),
            current: Shown::default(),
            previous: None,
            visible: false,
            backdrop: false,
            dwm_corners: false,
        }));
        Ok(WinCard { state })
    }

    /// Create the card window if it does not exist yet.
    fn ensure_window(&self) -> anyhow::Result<()> {
        let mut s = self.state.borrow_mut();
        if !s.hwnd.is_invalid() {
            return Ok(());
        }
        // SAFETY: window creation on the UI thread. The state pointer stored in
        // GWLP_USERDATA stays valid because the window is destroyed (and the
        // pointer cleared) before the Rc goes away.
        unsafe {
            let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
            let instance = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                ex,
                w!("MarqueeCard"),
                w!("Marquee"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA)?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&self.state) as isize);
            s.hwnd = hwnd;
            s.color_target.store(hwnd.0 as isize, Ordering::Release);
        }
        Ok(())
    }
}

impl Card for WinCard {
    fn show(&mut self, content: CardContent, settings: &Settings) {
        if let Err(e) = self.ensure_window() {
            tracing::error!("could not create the card window: {e:#}");
            return;
        }
        let mut s = self.state.borrow_mut();
        if !s.visible
            && let Some(c) = &s.colors
        {
            s.system = c.read();
        }
        let now = Instant::now();
        let (work, scale) = system::work_area(settings.monitor);
        let restyle = !s.visible || s.theme != settings.theme || s.work != work || s.scale != scale;
        s.theme = settings.theme;
        s.corner = settings.position;
        s.work = work;
        s.scale = scale;
        s.layout = theme::layout(settings.theme);
        let hold = Duration::from_secs(settings.duration_secs);
        if s.visible {
            // A new song while the card shows: crossfade and restart the hold.
            let elapsed = now - s.start;
            let back = s.timeline.restart_hold(elapsed);
            s.timeline = Timeline { hold, ..s.timeline };
            s.start = now - back;
            let old = std::mem::replace(&mut s.current, Shown::new(content));
            s.previous = Some((old, now));
        } else {
            s.timeline = Timeline::new(hold, system::reduced_motion());
            s.start = now;
            s.current = Shown::new(content);
            s.previous = None;
        }
        if restyle {
            s.apply_style();
        }
        s.visible = true;
        s.tick();
        // SAFETY: the window belongs to this thread.
        unsafe {
            let _ = ShowWindow(s.hwnd, SW_SHOWNOACTIVATE);
            SetTimer(Some(s.hwnd), FRAME_TIMER, FRAME_MS, None);
        }
    }

    fn update_cover(&mut self, cover: Vec<u8>) {
        let mut s = self.state.borrow_mut();
        let old = s.current.clone();
        s.current.set_cover(cover);
        if s.visible {
            s.previous = Some((old, Instant::now()));
            // SAFETY: the window belongs to this thread.
            unsafe { SetTimer(Some(s.hwnd), FRAME_TIMER, FRAME_MS, None) };
        }
    }

    fn hide(&mut self) {
        self.state.borrow_mut().hide();
    }
}

impl Drop for WinCard {
    fn drop(&mut self) {
        self.state.borrow_mut().hide();
    }
}

impl State {
    fn size_px(&self) -> (u32, u32) {
        let w = (self.layout.width * self.scale).round() as u32;
        let h = (self.layout.height * self.scale).round() as u32;
        (w, h)
    }

    fn hide(&mut self) {
        self.visible = false;
        self.previous = None;
        self.renderer = None;
        self.color_target.store(0, Ordering::Release);
        if self.hwnd.is_invalid() {
            return;
        }
        // SAFETY: the window belongs to this thread. The state pointer is
        // cleared first, so messages sent while destroying go to the default
        // procedure. Destroying also removes its timers.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
        self.hwnd = HWND::default();
    }

    /// Advance the animation one step: move, fade and redraw.
    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.start;
        let frame = self.timeline.frame(elapsed);
        if frame.done {
            self.hide();
            return;
        }
        let crossfade = self
            .previous
            .as_ref()
            .map(|(_, at)| (now - *at).as_secs_f32() / CROSSFADE.as_secs_f32());
        if crossfade.is_some_and(|t| t >= 1.0) {
            self.previous = None;
        }
        let (w, h) = self.size_px();
        let gap = GAP * self.scale as f64;
        let (x, y) = slide_position(self.work, (w as f64, h as f64), self.corner, gap, frame.position);
        // SAFETY: the window belongs to this thread.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x.round() as i32,
                y.round() as i32,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ =
                SetLayeredWindowAttributes(self.hwnd, COLORREF(0), (frame.opacity * 255.0).round() as u8, LWA_ALPHA);
        }
        self.paint(frame.progress);

        // While holding with nothing moving, sleep until the hold ends.
        let palette = theme::palette(self.theme, self.current.content.accent);
        let shows_progress = palette.and_then(|p| p.progress).is_some();
        if self.timeline.is_holding(elapsed) && self.previous.is_none() && !shows_progress {
            let left = self.timeline.enter + self.timeline.hold - elapsed;
            // SAFETY: the window belongs to this thread.
            unsafe {
                let _ = KillTimer(Some(self.hwnd), FRAME_TIMER);
                SetTimer(Some(self.hwnd), HOLD_TIMER, left.as_millis().max(1) as u32, None);
            }
        }
    }

    fn paint(&mut self, progress: f32) {
        let now = Instant::now();
        let palette = theme::palette(self.theme, self.current.content.accent);
        let previous = self.previous.as_ref().map(|(shown, at)| {
            let t = (now - *at).as_secs_f32() / CROSSFADE.as_secs_f32();
            (shown, 1.0 - t.clamp(0.0, 1.0))
        });
        let scene = Scene {
            layout: self.layout,
            palette,
            system: self.system,
            backdrop: self.backdrop,
            draw_border: !self.dwm_corners,
            current: &self.current,
            previous,
            progress: Some(progress),
        };
        let size = self.size_px();
        if self.renderer.is_none() {
            self.renderer = Renderer::new().map_err(|e| tracing::warn!("no renderer: {e}")).ok();
        }
        let Some(renderer) = self.renderer.as_mut() else { return };
        if let Err(e) = renderer.draw(self.hwnd, size, self.scale, &scene) {
            tracing::debug!("draw failed: {e}");
        }
    }
}
