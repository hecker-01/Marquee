//! Background work: source, then dedupe, then art, then the UI thread.

use crate::UserEvent;
use crate::art::{self, Art};
use crate::dedupe::{Decision, Dedupe};
use crate::model::{CardContent, Track};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

/// Show the card without a cover when the cover takes longer than this.
const COVER_WAIT: Duration = Duration::from_millis(800);

pub fn spawn(proxy: EventLoopProxy<UserEvent>) {
    std::thread::Builder::new()
        .name("pipeline".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(run(proxy));
        })
        .expect("pipeline thread");
}

async fn run(proxy: EventLoopProxy<UserEvent>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Track>(32);
    std::thread::Builder::new()
        .name("source".into())
        .spawn(move || {
            if let Err(e) = crate::source::platform_source().run(tx) {
                tracing::error!("now-playing source stopped: {e:#}");
            }
        })
        .expect("source thread");

    let art = Arc::new(Art::new());
    let mut dedupe = Dedupe::new();
    let mut generation = 0u64;
    loop {
        let deadline = dedupe.deadline();
        tokio::select! {
            track = rx.recv() => match track {
                Some(track) => {
                    tracing::debug!("event: {track:?}");
                    dedupe.push(track, Instant::now());
                }
                None => break,
            },
            _ = sleep_until(deadline), if deadline.is_some() => {
                match dedupe.poll(Instant::now()) {
                    Decision::Show(track) => {
                        generation += 1;
                        tracing::info!("new track: {} - {}", track.artist, track.title);
                        tokio::spawn(deliver(art.clone(), proxy.clone(), track, generation));
                    }
                    Decision::CoverUpdate(track) => {
                        let (art, proxy) = (art.clone(), proxy.clone());
                        tokio::spawn(async move {
                            if let Some(cover) = art.resolve(&track).await {
                                let _ = proxy.send_event(UserEvent::Cover { png: cover.png, generation });
                            }
                        });
                    }
                    Decision::Ignore => {}
                }
            }
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    if let Some(at) = deadline {
        tokio::time::sleep_until(at.into()).await;
    }
}

/// Resolve the cover and send the card. Without a cover after 800 ms, send
/// the card with the placeholder and the cover later.
async fn deliver(art: Arc<Art>, proxy: EventLoopProxy<UserEvent>, track: Track, generation: u64) {
    let mut content = CardContent {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        cover: None,
        accent: None,
    };
    let resolve = art.resolve(&track);
    tokio::pin!(resolve);
    match tokio::time::timeout(COVER_WAIT, &mut resolve).await {
        Ok(cover) => {
            if let Some(c) = cover {
                content.cover = Some(c.png);
                content.accent = Some(c.accent);
            }
            let _ = proxy.send_event(UserEvent::Show { content, generation });
        }
        Err(_) => {
            let _ = proxy.send_event(UserEvent::Show { content, generation });
            if let Some(c) = resolve.await {
                let _ = proxy.send_event(UserEvent::Cover { png: c.png, generation });
            }
        }
    }
}

/// Content for the Test card menu item. Each click shows the next of five
/// generated covers with very different colors, then the placeholder.
pub fn test_content(index: u64) -> CardContent {
    if index % 6 == 5 {
        return CardContent::sample();
    }
    const COVERS: [([u8; 3], [u8; 3], &str); 5] = [
        ([230, 57, 70], [255, 183, 3], "Red Sunset"),
        ([29, 53, 87], [69, 123, 157], "Deep Blue"),
        ([46, 196, 182], [203, 243, 240], "Mint Morning"),
        ([255, 221, 0], [255, 255, 255], "Lemon Light"),
        ([40, 40, 40], [120, 120, 120], "Graphite"),
    ];
    let (a, b, name) = COVERS[(index % 6) as usize];
    let img = image::RgbImage::from_fn(art::COVER_PX, art::COVER_PX, |x, y| {
        let t = (x + y) as f32 / (2 * art::COVER_PX) as f32;
        let band = if (x / 24 + y / 24) % 2 == 0 { 1.0 } else { 0.9 };
        image::Rgb(std::array::from_fn(|i| {
            ((a[i] as f32 * (1.0 - t) + b[i] as f32 * t) * band) as u8
        }))
    });
    let mut png = Vec::new();
    let _ = image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
    let cover = art::process(&png).ok();
    CardContent {
        title: format!("{name}, a test card with a rather long song title"),
        artist: "The Placeholders".into(),
        album: "Marquee Sessions".into(),
        accent: cover.as_ref().map(|c| c.accent),
        cover: cover.map(|c| c.png),
    }
}
