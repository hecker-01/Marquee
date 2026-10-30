//! Gallery: shows the real card for each test cover in each theme and saves a
//! screenshot of each to `docs/screens`. Windows only.
//!
//! - `cargo run --example gallery` captures five covers and the placeholder in
//!   all three themes, named after the current light or dark mode.
//! - `cargo run --example gallery -- corners` shows the card in all four
//!   corners for both monitor modes and prints its rectangle and work area.
//! - `cargo run --example gallery -- crossfade` captures a song change while
//!   the card shows.
//! - `cargo run --example gallery -- compare <toast.ps1>` captures a real
//!   system notification and the System card in the same screen region.

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    win::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("gallery only runs on Windows; on macOS use the Test card menu item");
}

#[cfg(target_os = "windows")]
mod win {
    use marquee::card::{Card, platform_card};
    use marquee::model::CardContent;
    use marquee::pipeline::test_content;
    use marquee::settings::{Corner, MonitorMode, Settings, Theme};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use tao::event::{Event, StartCause};
    use tao::event_loop::{ControlFlow, EventLoop};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};
    use windows::core::w;

    enum Step {
        Show(CardContent, Settings),
        /// Save the card region (with a margin) under this file name.
        Capture(String),
        /// Save a fixed screen region under this file name.
        CaptureRegion(String, RECT),
        Report(String),
        /// Tile the gallery shots of one mode into a single sheet.
        Sheet(String),
        Hide,
        Toast(String),
        Wait(u64),
    }

    fn settings(theme: Theme, position: Corner, monitor: MonitorMode) -> Settings {
        Settings {
            theme,
            position,
            monitor,
            duration_secs: 8,
            ..Settings::default()
        }
    }

    fn mode() -> &'static str {
        let ui = windows::UI::ViewManagement::UISettings::new().ok();
        let fg = ui.and_then(|u| {
            u.GetColorValue(windows::UI::ViewManagement::UIColorType::Foreground)
                .ok()
        });
        if fg.is_some_and(|c| c.R > 128) { "dark" } else { "light" }
    }

    fn card_rect() -> Option<RECT> {
        // SAFETY: plain Win32 queries with a valid out pointer.
        unsafe {
            let hwnd = FindWindowW(w!("MarqueeCard"), None).ok()?;
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            Some(r)
        }
    }

    fn work_area_text() -> String {
        // SAFETY: plain Win32 queries with valid out pointers.
        unsafe {
            let hwnd = FindWindowW(w!("MarqueeCard"), None).unwrap_or_default();
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(mon, &mut info);
            let (m, r) = (info.rcMonitor, info.rcWork);
            format!(
                "monitor {},{}-{},{} work {},{}-{},{}",
                m.left, m.top, m.right, m.bottom, r.left, r.top, r.right, r.bottom
            )
        }
    }

    /// Copy a screen region into a PNG file.
    fn capture(r: RECT, path: &PathBuf) -> anyhow::Result<()> {
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        // SAFETY: GDI objects are created, used and released in this block.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bmp.into());
            BitBlt(mem, 0, 0, w, h, Some(screen), r.left, r.top, SRCCOPY | CAPTUREBLT)?;
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut px = vec![0u8; (w * h * 4) as usize];
            GetDIBits(
                mem,
                bmp,
                0,
                h as u32,
                Some(px.as_mut_ptr() as _),
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            for p in px.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                p[3] = 255;
            }
            image::RgbaImage::from_raw(w as u32, h as u32, px)
                .ok_or_else(|| anyhow::anyhow!("bad capture size"))?
                .save(path)?;
        }
        Ok(())
    }

    fn gallery_steps() -> VecDeque<Step> {
        let mode = mode();
        let mut steps = VecDeque::new();
        for (theme, name) in [
            (Theme::System, "system"),
            (Theme::Spotify, "spotify"),
            (Theme::Cover, "cover"),
        ] {
            for i in 0..6 {
                let s = settings(theme, Corner::BottomRight, MonitorMode::Primary);
                steps.push_back(Step::Show(test_content(i), s));
                steps.push_back(Step::Wait(1200));
                steps.push_back(Step::Capture(format!("{mode}-{name}-{i}.png")));
                steps.push_back(Step::Hide);
                steps.push_back(Step::Wait(300));
            }
        }
        steps.push_back(Step::Sheet(mode.into()));
        steps
    }

    /// Columns are themes, rows are covers.
    fn sheet(dir: &std::path::Path, mode: &str) -> anyhow::Result<()> {
        let mut shots = Vec::new();
        for theme in ["system", "spotify", "cover"] {
            for i in 0..6 {
                shots.push(image::open(dir.join(format!("{mode}-{theme}-{i}.png")))?.to_rgba8());
            }
        }
        let w = shots.iter().map(|s| s.width()).max().unwrap_or(0);
        let h = shots.iter().map(|s| s.height()).max().unwrap_or(0);
        let mut out = image::RgbaImage::from_pixel(w * 3, h * 6, image::Rgba([40, 40, 40, 255]));
        for (n, shot) in shots.iter().enumerate() {
            let (col, row) = (n as u32 / 6, n as u32 % 6);
            image::imageops::overlay(&mut out, shot, (col * w) as i64, (row * h) as i64);
        }
        out.save(dir.join(format!("{mode}-sheet.png")))?;
        Ok(())
    }

    fn corner_steps() -> VecDeque<Step> {
        let mut steps = VecDeque::new();
        for monitor in [MonitorMode::Primary, MonitorMode::Cursor] {
            for corner in [
                Corner::TopLeft,
                Corner::TopRight,
                Corner::BottomLeft,
                Corner::BottomRight,
            ] {
                steps.push_back(Step::Show(test_content(0), settings(Theme::System, corner, monitor)));
                steps.push_back(Step::Wait(1000));
                steps.push_back(Step::Report(format!("{monitor:?} {corner:?}")));
                steps.push_back(Step::Hide);
                steps.push_back(Step::Wait(300));
            }
        }
        steps
    }

    /// A second song while the card shows: capture mid-crossfade and after.
    fn crossfade_steps() -> VecDeque<Step> {
        let s = settings(Theme::System, Corner::BottomRight, MonitorMode::Primary);
        VecDeque::from([
            Step::Show(test_content(0), s.clone()),
            Step::Wait(1500),
            Step::Show(test_content(1), s),
            Step::Wait(110),
            Step::Capture("crossfade-mid.png".into()),
            Step::Wait(700),
            Step::Capture("crossfade-after.png".into()),
            Step::Hide,
        ])
    }

    fn compare_steps(toast: String) -> VecDeque<Step> {
        // The bottom right of the primary monitor, where both appear.
        // SAFETY: plain Win32 query with a valid out pointer.
        let work = unsafe {
            let mon = MonitorFromPoint(Default::default(), MONITOR_DEFAULTTOPRIMARY);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(mon, &mut info);
            info.rcWork
        };
        let region = RECT {
            left: work.right - 640,
            top: work.bottom - 260,
            right: work.right,
            bottom: work.bottom,
        };
        let mode = mode();
        let mut content = test_content(1);
        content.title = "Comparison card".into();
        VecDeque::from([
            Step::Toast(toast),
            Step::Wait(2500),
            Step::CaptureRegion(format!("{mode}-compare-notification.png"), region),
            Step::Wait(7000),
            Step::Show(
                content,
                settings(Theme::System, Corner::BottomRight, MonitorMode::Primary),
            ),
            Step::Wait(1200),
            Step::CaptureRegion(format!("{mode}-compare-card.png"), region),
            Step::Hide,
        ])
    }

    pub fn run() -> anyhow::Result<()> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let out = PathBuf::from("docs/screens");
        std::fs::create_dir_all(&out)?;
        let mut steps = match args.first().map(String::as_str) {
            Some("corners") => corner_steps(),
            Some("crossfade") => crossfade_steps(),
            Some("compare") => compare_steps(args.get(1).cloned().unwrap_or_default()),
            _ => gallery_steps(),
        };
        let event_loop = EventLoop::new();
        let mut card: Option<Box<dyn Card>> = None;
        let mut next = Instant::now();
        event_loop.run(move |event, _, flow| {
            if let Event::NewEvents(StartCause::Init) = event {
                card = platform_card().ok();
            }
            if Instant::now() < next {
                *flow = ControlFlow::WaitUntil(next);
                return;
            }
            let Some(c) = card.as_mut() else { return };
            let Some(step) = steps.pop_front() else {
                *flow = ControlFlow::Exit;
                return;
            };
            match step {
                Step::Show(content, s) => c.show(content, &s),
                Step::Hide => c.hide(),
                Step::Sheet(mode) => match sheet(&out, &mode) {
                    Ok(()) => println!("saved {mode}-sheet.png"),
                    Err(e) => println!("sheet: {e}"),
                },
                Step::Wait(ms) => next = Instant::now() + Duration::from_millis(ms),
                Step::Toast(script) => {
                    let _ = std::process::Command::new("powershell")
                        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &script])
                        .status();
                }
                Step::Report(label) => match card_rect() {
                    Some(r) => println!(
                        "{label}: card {},{}-{},{} {}",
                        r.left,
                        r.top,
                        r.right,
                        r.bottom,
                        work_area_text()
                    ),
                    None => println!("{label}: no card window"),
                },
                Step::Capture(name) => match card_rect() {
                    Some(r) => {
                        let m = 24;
                        let r = RECT {
                            left: r.left - m,
                            top: r.top - m,
                            right: r.right + m,
                            bottom: r.bottom + m,
                        };
                        match capture(r, &out.join(&name)) {
                            Ok(()) => println!("saved {name}"),
                            Err(e) => println!("{name}: {e}"),
                        }
                    }
                    None => println!("{name}: no card window"),
                },
                Step::CaptureRegion(name, r) => match capture(r, &out.join(&name)) {
                    Ok(()) => println!("saved {name}"),
                    Err(e) => println!("{name}: {e}"),
                },
            }
            *flow = ControlFlow::WaitUntil(next.max(Instant::now() + Duration::from_millis(10)));
        });
    }
}
