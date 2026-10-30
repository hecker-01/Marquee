//! Core data types shared by sources, the pipeline and the cards.

/// One track as reported by a now-playing source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: Option<ArtSource>,
    pub is_playing: bool,
}

/// Where the cover of a track comes from.
#[derive(Clone, PartialEq, Eq)]
#[allow(dead_code)] // each system builds only one variant
pub enum ArtSource {
    /// A URL to fetch. Either a direct image URL or a Spotify oEmbed URL.
    Url(String),
    /// Encoded image bytes handed over by the system.
    Bytes(Vec<u8>),
}

impl std::fmt::Debug for ArtSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtSource::Url(url) => write!(f, "Url({url})"),
            ArtSource::Bytes(b) => write!(f, "Bytes({} bytes)", b.len()),
        }
    }
}

/// Everything a card needs to draw one track.
#[derive(Debug, Clone, Default)]
pub struct CardContent {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// PNG bytes of the cover, already resized.
    pub cover: Option<Vec<u8>>,
    /// Dominant color of the cover, used by the Cover theme.
    pub accent: Option<[u8; 3]>,
}

impl CardContent {
    /// Content used by the Test card menu item.
    pub fn sample() -> Self {
        CardContent {
            title: "Marquee test card with a rather long song title".into(),
            artist: "The Placeholders".into(),
            album: "Now Playing".into(),
            cover: None,
            accent: None,
        }
    }
}
