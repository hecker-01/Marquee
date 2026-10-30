//! System facts for the Windows card: colors, monitors, motion and fonts.

use crate::placement::Rect;
use crate::settings::MonitorMode;
use crate::theme::Rgba;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::UI::ViewManagement::{UIColorType, UISettings};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::DirectWrite::IDWriteFactory;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, PostMessageW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    WM_APP,
};
use windows::core::{BOOL, HSTRING};

/// Posted to the card window when the system colors change.
pub const WM_COLORS_CHANGED: u32 = WM_APP + 1;

/// Colors of the System theme, read from UISettings.
#[derive(Debug, Clone, Copy)]
pub struct SystemColors {
    pub dark: bool,
    pub foreground: Rgba,
    pub background: Rgba,
    pub accent: Rgba,
}

fn to_rgba(c: windows::UI::Color) -> Rgba {
    Rgba {
        r: c.R as f32 / 255.0,
        g: c.G as f32 / 255.0,
        b: c.B as f32 / 255.0,
        a: c.A as f32 / 255.0,
    }
}

pub struct Colors {
    ui: UISettings,
    token: i64,
}

impl Colors {
    /// Watch the system colors and post `WM_COLORS_CHANGED` on change to the
    /// window in `target`, if one exists at that moment.
    pub fn watch(target: Arc<AtomicIsize>) -> anyhow::Result<Self> {
        let ui = UISettings::new()?;
        let token = ui.ColorValuesChanged(&windows::Foundation::TypedEventHandler::new(move |_, _| {
            // SAFETY: posting a message to a window handle is safe even if the
            // window is gone; the call then fails without side effects.
            let hwnd = target.load(Ordering::Acquire);
            if hwnd != 0 {
                unsafe {
                    let _ = PostMessageW(Some(HWND(hwnd as _)), WM_COLORS_CHANGED, WPARAM(0), LPARAM(0));
                }
            }
            Ok(())
        }))?;
        Ok(Colors { ui, token })
    }

    pub fn read(&self) -> SystemColors {
        let get = |t| self.ui.GetColorValue(t).map(to_rgba).unwrap_or(Rgba::BLACK);
        let foreground = get(UIColorType::Foreground);
        SystemColors {
            // Light text means a dark theme.
            dark: foreground.r > 0.5,
            foreground,
            background: get(UIColorType::Background),
            accent: get(UIColorType::Accent),
        }
    }
}

impl Drop for Colors {
    fn drop(&mut self) {
        let _ = self.ui.RemoveColorValuesChanged(self.token);
    }
}

/// Work area of the chosen monitor in physical pixels, and its scale factor.
pub fn work_area(mode: MonitorMode) -> (Rect, f32) {
    // SAFETY: plain Win32 queries with valid out pointers on this stack frame.
    unsafe {
        let monitor = match mode {
            MonitorMode::Primary => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
            MonitorMode::Cursor => {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
            }
        };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let r = info.rcWork;
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let rect = Rect {
            x: r.left as f64,
            y: r.top as f64,
            w: (r.right - r.left) as f64,
            h: (r.bottom - r.top) as f64,
        };
        (rect, dx as f32 / 96.0)
    }
}

/// True when the user turned off animations in Windows settings.
pub fn reduced_motion() -> bool {
    let mut enabled = BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && !enabled.as_bool()
}

/// The first installed font family from the list.
pub fn first_font(dwrite: &IDWriteFactory, names: &[&'static str]) -> &'static str {
    let mut collection = None;
    // SAFETY: GetSystemFontCollection fills the out parameter on success.
    let _ = unsafe { dwrite.GetSystemFontCollection(&mut collection, false) };
    let Some(collection) = collection else {
        return names[names.len() - 1];
    };
    for name in names {
        let (mut index, mut exists) = (0u32, BOOL(0));
        // SAFETY: valid out pointers; the name lives for the call.
        let found = unsafe { collection.FindFamilyName(&HSTRING::from(*name), &mut index, &mut exists) };
        if found.is_ok() && exists.as_bool() {
            return name;
        }
    }
    names[names.len() - 1]
}
