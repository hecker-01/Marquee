//! Probe: prints every Spotify playback notification on macOS.
//!
//! Run with `cargo run --example probe_macos`, then play, pause and skip songs
//! in the Spotify desktop app. Paste the output back to the builder.

#[cfg(target_os = "macos")]
fn main() {
    use block2::RcBlock;
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSRunLoop, NSString};
    use std::ptr::NonNull;

    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.spotify.client.PlaybackStateChanged");
    let block = RcBlock::new(|note: NonNull<NSNotification>| {
        // SAFETY: AppKit passes a valid notification for the duration of the call.
        let note = unsafe { note.as_ref() };
        match note.userInfo() {
            Some(info) => {
                println!("raw userInfo: {info:?}");
                let keys = ["Name", "Artist", "Album", "Track ID", "Player State"];
                for key in keys {
                    let value = info
                        .objectForKey(&NSString::from_str(key))
                        .map(|v| format!("{v:?}"))
                        .unwrap_or_else(|| "<missing>".into());
                    println!("  {key}: {value}");
                }
            }
            None => println!("notification without userInfo"),
        }
        let track_id = note
            .userInfo()
            .and_then(|i| i.objectForKey(&NSString::from_str("Track ID")))
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        println!("  cover: via https://open.spotify.com/oembed?url={track_id}");
    });
    // SAFETY: the block lives until the end of main, which never returns.
    let _token = unsafe { center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block) };
    println!("listening for {name} ...");
    NSRunLoop::currentRunLoop().run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("probe_macos only runs on macOS");
}
