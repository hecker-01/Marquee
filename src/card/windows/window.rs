//! Window styling through DWM and the window procedure of the card.

use super::system::WM_COLORS_CHANGED;
use super::{FRAME_MS, FRAME_TIMER, HOLD_TIMER, State};
use crate::settings::Theme;
use std::cell::RefCell;
use std::time::Instant;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn, ValidateRect};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;

impl State {
    /// Backdrop, corners and border for the current theme.
    pub(super) fn apply_style(&mut self) {
        let hwnd = self.hwnd;
        let (w, h) = self.size_px();
        let system = self.theme == Theme::System;
        // SAFETY: DWM attribute calls with pointers to locals that outlive the call.
        unsafe {
            let set = |attr: DWMWINDOWATTRIBUTE, v: u32| {
                DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const _, 4).is_ok()
            };
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                w as i32,
                h as i32,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            set(DWMWA_USE_IMMERSIVE_DARK_MODE, self.system.dark as u32);
            let kind = if system { DWMSBT_TRANSIENTWINDOW } else { DWMSBT_NONE };
            self.backdrop = set(DWMWA_SYSTEMBACKDROP_TYPE, kind.0 as u32) && system;
            let margins = if self.backdrop { -1 } else { 0 };
            let m = MARGINS {
                cxLeftWidth: margins,
                cxRightWidth: margins,
                cyTopHeight: margins,
                cyBottomHeight: margins,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
            self.dwm_corners = set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0 as u32);
            if self.dwm_corners {
                let border = match self.theme {
                    Theme::System => DWMWA_COLOR_DEFAULT,
                    Theme::Spotify => 0x002A2A2A,
                    Theme::Cover => DWMWA_COLOR_NONE,
                };
                set(DWMWA_BORDER_COLOR, border);
                SetWindowRgn(hwnd, None, true);
            } else {
                // Windows 10: round the window with a region instead.
                let d = (2.0 * self.layout.corner_radius * self.scale).round() as i32;
                let rgn = CreateRoundRectRgn(0, 0, w as i32 + 1, h as i32 + 1, d, d);
                SetWindowRgn(hwnd, Some(rgn), true);
            }
        }
    }
}

/// Window procedure of the card.
pub(super) unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: GWLP_USERDATA holds a pointer to the RefCell<State> owned by the
    // WinCard, or 0 before setup and after drop.
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>;
        let state = ptr.as_ref().and_then(|cell| cell.try_borrow_mut().ok());
        match (msg, state) {
            (WM_TIMER, Some(mut s)) => {
                if wparam.0 == HOLD_TIMER {
                    let _ = KillTimer(Some(hwnd), HOLD_TIMER);
                    SetTimer(Some(hwnd), FRAME_TIMER, FRAME_MS, None);
                }
                if s.visible {
                    s.tick();
                }
                LRESULT(0)
            }
            (WM_PAINT, Some(mut s)) => {
                let progress = s.timeline.frame(Instant::now() - s.start).progress;
                s.paint(progress);
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            (WM_COLORS_CHANGED, Some(mut s)) => {
                if let Some(c) = &s.colors {
                    s.system = c.read();
                }
                s.apply_style();
                if s.visible {
                    let progress = s.timeline.frame(Instant::now() - s.start).progress;
                    s.paint(progress);
                }
                LRESULT(0)
            }
            (WM_MOUSEACTIVATE, _) => LRESULT(MA_NOACTIVATE as isize),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
