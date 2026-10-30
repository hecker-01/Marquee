//! Probe: prints every media event from the Spotify session on Windows.
//!
//! Run with `cargo run --example probe_windows`. Optional arguments `next`,
//! `toggle`, `play` or `pause` send that command to Spotify once at start, which helps testing.

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use std::sync::mpsc;
    use std::time::Duration;
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    use windows::Storage::Streams::DataReader;

    let manager = Manager::RequestAsync()?.join()?;
    let sessions = manager.GetSessions()?;
    for s in &sessions {
        println!("session: {}", s.SourceAppUserModelId()?);
    }
    let session = sessions
        .into_iter()
        .find(|s| {
            s.SourceAppUserModelId()
                .map(|id| id.to_string().to_lowercase().contains("spotify"))
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("no Spotify session found"))?;

    match std::env::args().nth(1).as_deref() {
        Some("next") => drop(session.TrySkipNextAsync()?.join()?),
        Some("toggle") => drop(session.TryTogglePlayPauseAsync()?.join()?),
        Some("play") => drop(session.TryPlayAsync()?.join()?),
        Some("pause") => drop(session.TryPauseAsync()?.join()?),
        _ => {}
    }

    let (tx, rx) = mpsc::channel::<&'static str>();
    let t1 = tx.clone();
    session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = t1.send("MediaPropertiesChanged");
        Ok(())
    }))?;
    let t2 = tx.clone();
    session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        let _ = t2.send("PlaybackInfoChanged");
        Ok(())
    }))?;
    let _ = tx.send("initial");

    loop {
        let Ok(kind) = rx.recv_timeout(Duration::from_secs(60)) else {
            continue;
        };
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
        let cover = match props.Thumbnail() {
            Ok(thumb) => {
                let stream = thumb.OpenReadAsync()?.join()?;
                let size = stream.Size()? as u32;
                let reader = DataReader::CreateDataReader(&stream)?;
                reader.LoadAsync(size)?.join()?;
                let mut buf = vec![0u8; size as usize];
                reader.ReadBytes(&mut buf)?;
                let dims = image::load_from_memory(&buf)
                    .map(|i| format!("{}x{}", i.width(), i.height()))
                    .unwrap_or_else(|e| format!("undecodable: {e}"));
                if let Some(path) = std::env::var_os("PROBE_SAVE_COVER") {
                    std::fs::write(path, &buf)?;
                }
                format!("{size} bytes, {dims}, type {}", stream.ContentType()?)
            }
            Err(_) => "none".into(),
        };
        println!(
            "[{kind}] status={status:?} title={:?} artist={:?} album={:?} album_artist={:?} cover={cover}",
            props.Title()?.to_string(),
            props.Artist()?.to_string(),
            props.AlbumTitle()?.to_string(),
            props.AlbumArtist()?.to_string(),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("probe_windows only runs on Windows");
}
