//! Tray (Windows) and menu bar (macOS) icon with the native settings menu.

use crate::settings::{Corner, MonitorMode, Settings, Theme};
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// What a menu click asks the app to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Settings changed; they are already updated.
    Changed,
    TestCard,
    Quit,
}

const THEMES: [(Theme, &str); 3] = [
    (Theme::System, "System"),
    (Theme::Spotify, "Spotify"),
    (Theme::Cover, "Cover"),
];
const CORNERS: [(Corner, &str); 4] = [
    (Corner::TopLeft, "Top left"),
    (Corner::TopRight, "Top right"),
    (Corner::BottomLeft, "Bottom left"),
    (Corner::BottomRight, "Bottom right"),
];
const DURATIONS: [(u64, &str); 3] = [(3, "3 seconds"), (5, "5 seconds"), (8, "8 seconds")];
const MONITORS: [(MonitorMode, &str); 2] = [
    (MonitorMode::Primary, "Primary monitor"),
    (MonitorMode::Cursor, "Monitor under cursor"),
];

pub struct Tray {
    _icon: TrayIcon,
    enabled: CheckMenuItem,
    test: MenuItem,
    themes: Vec<(Theme, CheckMenuItem)>,
    corners: Vec<(Corner, CheckMenuItem)>,
    durations: Vec<(u64, CheckMenuItem)>,
    monitors: Vec<(MonitorMode, CheckMenuItem)>,
    login: CheckMenuItem,
    quit: MenuItem,
}

fn group<T: Copy>(title: &str, options: &[(T, &str)]) -> anyhow::Result<(Submenu, Vec<(T, CheckMenuItem)>)> {
    let sub = Submenu::new(title, true);
    let mut items = Vec::new();
    for (value, label) in options {
        let item = CheckMenuItem::new(*label, true, false, None);
        sub.append(&item)?;
        items.push((*value, item));
    }
    Ok((sub, items))
}

impl Tray {
    /// Build the icon and menu. On macOS call this after the event loop started.
    pub fn new(settings: &Settings) -> anyhow::Result<Self> {
        let menu = Menu::new();
        let enabled = CheckMenuItem::new("Enabled", true, settings.enabled, None);
        let test = MenuItem::new("Test card", true, None);
        let (theme_menu, themes) = group("Theme", &THEMES)?;
        let (corner_menu, corners) = group("Position", &CORNERS)?;
        let (duration_menu, durations) = group("Duration", &DURATIONS)?;
        let (monitor_menu, monitors) = group("Monitor", &MONITORS)?;
        let login = CheckMenuItem::new("Start at login", true, settings.start_at_login, None);
        let quit = MenuItem::new("Quit Marquee", true, None);
        menu.append_items(&[
            &enabled,
            &test,
            &PredefinedMenuItem::separator(),
            &theme_menu,
            &corner_menu,
            &duration_menu,
            &monitor_menu,
            &PredefinedMenuItem::separator(),
            &login,
            &quit,
        ])?;

        let template = cfg!(target_os = "macos");
        let color = if template { [0, 0, 0] } else { [0xF5, 0xB8, 0x3D] };
        let size = 32;
        let icon = Icon::from_rgba(icon_rgba(size, color), size, size)?;
        let builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Marquee");
        // A template image follows the light and dark menu bar on macOS.
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icon);
        let tray_icon = builder.build()?;

        let tray = Tray {
            _icon: tray_icon,
            enabled,
            test,
            themes,
            corners,
            durations,
            monitors,
            login,
            quit,
        };
        tray.sync(settings);
        Ok(tray)
    }

    /// Show the current settings as check marks.
    pub fn sync(&self, s: &Settings) {
        self.enabled.set_checked(s.enabled);
        self.login.set_checked(s.start_at_login);
        self.themes.iter().for_each(|(v, i)| i.set_checked(*v == s.theme));
        self.corners.iter().for_each(|(v, i)| i.set_checked(*v == s.position));
        self.durations
            .iter()
            .for_each(|(v, i)| i.set_checked(*v == s.duration_secs));
        self.monitors.iter().for_each(|(v, i)| i.set_checked(*v == s.monitor));
    }

    /// Apply a menu click to the settings and say what else to do.
    pub fn handle(&self, id: &MenuId, s: &mut Settings) -> Option<Action> {
        if id == self.test.id() {
            return Some(Action::TestCard);
        }
        if id == self.quit.id() {
            return Some(Action::Quit);
        }
        if id == self.enabled.id() {
            s.enabled = !s.enabled;
        } else if id == self.login.id() {
            s.start_at_login = !s.start_at_login;
        } else if let Some(v) = pick(&self.themes, id) {
            s.theme = v;
        } else if let Some(v) = pick(&self.corners, id) {
            s.position = v;
        } else if let Some(v) = pick(&self.durations, id) {
            s.duration_secs = v;
        } else {
            s.monitor = pick(&self.monitors, id)?;
        }
        // Check items toggle themselves; reset them to the real state.
        self.sync(s);
        Some(Action::Changed)
    }
}

fn pick<T: Copy>(items: &[(T, CheckMenuItem)], id: &MenuId) -> Option<T> {
    items.iter().find(|(_, i)| i.id() == id).map(|(v, _)| *v)
}

/// Draw the icon: a marquee sign, a rounded board framed by a row of bulbs.
fn icon_rgba(size: u32, color: [u8; 3]) -> Vec<u8> {
    let s = size as f32;
    let mut out = vec![0u8; (size * size * 4) as usize];
    let board = |x: f32, y: f32| {
        let (x0, y0, x1, y1, r) = (s * 0.06, s * 0.2, s * 0.94, s * 0.8, s * 0.12);
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        (r - d + 0.5).clamp(0.0, 1.0)
    };
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let mut a = board(x, y);
            // Five bulbs cut out of the top and bottom edges.
            for i in 0..5 {
                let bx = s * (0.2 + 0.15 * i as f32);
                for by in [s * 0.3, s * 0.7] {
                    let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt();
                    a *= (d - s * 0.05).clamp(0.0, 1.0);
                }
            }
            let i = ((py * size + px) * 4) as usize;
            out[i..i + 3].copy_from_slice(&color);
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_has_opaque_and_clear_pixels() {
        let px = super::icon_rgba(32, [0, 0, 0]);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert!(px.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
        assert!(px.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
    }
}
