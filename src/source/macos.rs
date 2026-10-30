//! macOS source: the distributed notification the Spotify desktop app posts.
//!
//! Not yet verified on a Mac. The key names below follow the plan; run
//! `cargo run --example probe_macos` and correct them if the output differs.

use super::NowPlayingSource;
use crate::model::{ArtSource, Track};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2_foundation::{NSDictionary, NSDistributedNotificationCenter, NSNotification, NSRunLoop, NSString};
use std::ptr::NonNull;

const NOTIFICATION: &str = "com.spotify.client.PlaybackStateChanged";

pub struct MacSource;

impl MacSource {
    pub fn new() -> Self {
        MacSource
    }
}

impl NowPlayingSource for MacSource {
    fn run(self: Box<Self>, tx: tokio::sync::mpsc::Sender<Track>) -> anyhow::Result<()> {
        let center = NSDistributedNotificationCenter::defaultCenter();
        let name = NSString::from_str(NOTIFICATION);
        let block = RcBlock::new(move |note: NonNull<NSNotification>| {
            // SAFETY: the notification center passes a valid notification for
            // the duration of the callback.
            let note = unsafe { note.as_ref() };
            let Some(info) = note.userInfo() else { return };
            tracing::debug!("Spotify notification: {info:?}");
            if let Some(track) = parse(&info) {
                let _ = tx.blocking_send(track);
            }
        });
        // SAFETY: the block is kept alive by this frame, which never returns
        // because the run loop below runs forever.
        let _token = unsafe { center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block) };
        tracing::info!("listening for {NOTIFICATION}");
        // Notifications arrive through this thread's run loop. While Spotify is
        // not running, the run loop sleeps and the app stays idle.
        NSRunLoop::currentRunLoop().run();
        Ok(())
    }
}

fn string(info: &NSDictionary, key: &str) -> String {
    info.objectForKey(&NSString::from_str(key))
        .and_then(|v| v.downcast::<NSString>().ok())
        .map(|s: Retained<NSString>| s.to_string())
        .unwrap_or_default()
}

fn parse(info: &NSDictionary) -> Option<Track> {
    let uri = string(info, "Track ID");
    let state = string(info, "Player State");
    if state == "Stopped" {
        return None;
    }
    // Ads use a `spotify:ad:` URI; they get an empty id so dedupe skips them.
    let id = if uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:") {
        uri.clone()
    } else {
        String::new()
    };
    let art = (!id.is_empty()).then(|| ArtSource::Url(format!("https://open.spotify.com/oembed?url={uri}")));
    Some(Track {
        id,
        title: string(info, "Name"),
        artist: string(info, "Artist"),
        album: string(info, "Album"),
        art,
        is_playing: state == "Playing",
    })
}
