//! Marquee as a library: the binary in `main.rs` wires these modules together,
//! and the examples use them to probe sources and check the card.

pub mod anim;
pub mod art;
pub mod card;
pub mod dedupe;
pub mod model;
pub mod pipeline;
pub mod placement;
pub mod settings;
pub mod source;
pub mod theme;
pub mod tray;

use model::CardContent;
use tray_icon::menu::MenuEvent;

/// Messages from other threads to the UI thread.
#[derive(Debug)]
pub enum UserEvent {
    Show { content: CardContent, generation: u64 },
    Cover { png: Vec<u8>, generation: u64 },
    Menu(MenuEvent),
}
