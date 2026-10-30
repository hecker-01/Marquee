//! Windows source: the Global System Media Transport Controls session of Spotify.
//!
//! Verified with the probe on Windows 11 (Spotify 1.2): the session id is
//! `Spotify.exe`, both events fire on every track change and the thumbnail is
//! a 300 by 300 PNG without a Spotify logo.

use super::NowPlayingSource;
use crate::model::{ArtSource, Track};
use std::sync::mpsc;
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

/// How often to look for Spotify while it is not running.
const IDLE_CHECK: Duration = Duration::from_secs(5);

pub struct WindowsSource;

impl WindowsSource {
    pub fn new() -> Self {
        WindowsSource
    }
}

impl NowPlayingSource for WindowsSource {
    fn run(self: Box<Self>, tx: tokio::sync::mpsc::Sender<Track>) -> anyhow::Result<()> {
        let manager = Manager::RequestAsync()?.join()?;
        loop {
            match find_spotify(&manager) {
                Some(session) => {
                    tracing::info!("Spotify session found");
                    if let Err(e) = watch(&manager, &session, &tx) {
                        tracing::warn!("Spotify session watch ended: {e:#}");
                    }
                    if tx.is_closed() {
                        return Ok(());
                    }
                }
                None => std::thread::sleep(IDLE_CHECK),
            }
        }
    }
}

fn is_spotify(session: &Session) -> bool {
    session
        .SourceAppUserModelId()
        .map(|id| id.to_string().to_lowercase().contains("spotify"))
        .unwrap_or(false)
}

fn find_spotify(manager: &Manager) -> Option<Session> {
    let sessions = manager.GetSessions().ok()?;
    sessions.into_iter().find(is_spotify)
}

/// Follow one session until it disappears.
fn watch(manager: &Manager, session: &Session, tx: &tokio::sync::mpsc::Sender<Track>) -> anyhow::Result<()> {
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let w1 = wake_tx.clone();
    let props_token = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        tracing::trace!("MediaPropertiesChanged");
        let _ = w1.send(());
        Ok(())
    }))?;
    let w2 = wake_tx.clone();
    let playback_token = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        tracing::trace!("PlaybackInfoChanged");
        let _ = w2.send(());
        Ok(())
    }))?;
    let _ = wake_tx.send(());

    let mut art_for: Option<String> = None;
    let result = loop {
        match wake_rx.recv_timeout(IDLE_CHECK) {
            Ok(()) => {
                // Drain bursts: several events usually arrive together.
                while wake_rx.try_recv().is_ok() {}
                match read_track(session, &mut art_for) {
                    Ok(track) => {
                        if tx.blocking_send(track).is_err() {
                            break Ok(());
                        }
                    }
                    Err(e) => tracing::debug!("could not read media properties: {e}"),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                tracing::trace!("session check");
                if find_spotify(manager).is_none() {
                    tracing::info!("Spotify session closed");
                    break Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(()),
        }
    };
    let _ = session.RemoveMediaPropertiesChanged(props_token);
    let _ = session.RemovePlaybackInfoChanged(playback_token);
    result
}

/// Read the current track. The cover is read once per track to keep idle work low.
fn read_track(session: &Session, art_for: &mut Option<String>) -> anyhow::Result<Track> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = props.Title()?.to_string();
    let artist = props.Artist()?.to_string();
    let album = props.AlbumTitle()?.to_string();
    let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
    // Spotify has no track id here. Ads carry no artist, so they get an empty id.
    let id = if title.is_empty() || artist.is_empty() {
        String::new()
    } else {
        format!("{artist}\u{1f}{title}\u{1f}{album}")
    };
    let art = if art_for.as_deref() != Some(id.as_str()) {
        let bytes = read_thumbnail(&props).ok().filter(|b| !b.is_empty());
        if bytes.is_some() {
            *art_for = Some(id.clone());
        }
        bytes.map(ArtSource::Bytes)
    } else {
        None
    };
    Ok(Track {
        id,
        title,
        artist,
        album,
        art,
        is_playing: status == Status::Playing,
    })
}

fn read_thumbnail(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> anyhow::Result<Vec<u8>> {
    let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
    let size = u32::try_from(stream.Size()?)?;
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.join()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    Ok(buf)
}
