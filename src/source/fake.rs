//! A scripted source for tests.

use super::NowPlayingSource;
use crate::model::Track;

pub struct FakeSource {
    pub tracks: Vec<Track>,
}

impl NowPlayingSource for FakeSource {
    fn run(self: Box<Self>, tx: tokio::sync::mpsc::Sender<Track>) -> anyhow::Result<()> {
        for track in self.tracks {
            tx.blocking_send(track)?;
        }
        Ok(())
    }
}

pub fn track(id: &str, title: &str, playing: bool) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artist: "Artist".into(),
        album: "Album".into(),
        art: None,
        is_playing: playing,
    }
}
