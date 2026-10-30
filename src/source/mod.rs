//! Now-playing sources. Each system has one; the factory picks it at compile time.

use crate::model::Track;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
pub mod fake;

/// A source of track events. `run` blocks and sends every observed state.
pub trait NowPlayingSource: Send {
    fn run(self: Box<Self>, tx: tokio::sync::mpsc::Sender<Track>) -> anyhow::Result<()>;
}

/// The source for the system this binary was built for.
pub fn platform_source() -> Box<dyn NowPlayingSource> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsSource::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacSource::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        compile_error!("Marquee supports Windows and macOS only");
    }
}
