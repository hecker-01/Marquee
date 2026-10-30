//! The card: a native window that shows one track. One implementation per system.

use crate::model::CardContent;
use crate::settings::Settings;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// A card lives on the UI thread. It animates and hides itself.
pub trait Card {
    fn show(&mut self, content: CardContent, settings: &Settings);
    fn update_cover(&mut self, cover: Vec<u8>);
    fn hide(&mut self);
}

/// The card for the system this binary was built for. Call on the UI thread.
pub fn platform_card() -> anyhow::Result<Box<dyn Card>> {
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WinCard::new()?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacCard::new()?))
    }
}
