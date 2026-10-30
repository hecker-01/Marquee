//! Pure logic that decides when a track event becomes a card.
//!
//! Rules: show a card when the id differs from the last shown id and the
//! track is playing. Ignore pause and resume of the same track. Ignore ads.
//! Wait a short settle time and show only the newest track of a burst.

use crate::model::Track;
use std::time::{Duration, Instant};

pub const SETTLE: Duration = Duration::from_millis(400);

/// What the pipeline should do with a track after it settled.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// A new song: show a card.
    Show(Track),
    /// The song on the card again, now with a cover it lacked before.
    CoverUpdate(Track),
    /// Nothing to do.
    Ignore,
}

pub fn is_ad(track: &Track) -> bool {
    track.id.trim().is_empty() || track.title.trim().eq_ignore_ascii_case("advertisement")
}

#[derive(Default)]
pub struct Dedupe {
    last_shown: Option<String>,
    last_had_art: bool,
    pending: Option<(Track, Instant)>,
}

impl Dedupe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new event. The newest event replaces any pending one, but
    /// keeps the cover of a pending event for the same track.
    pub fn push(&mut self, mut track: Track, now: Instant) {
        if let Some((old, _)) = self.pending.take()
            && old.id == track.id
            && track.art.is_none()
        {
            track.art = old.art;
        }
        self.pending = Some((track, now + SETTLE));
    }

    /// When the pending event settles, if there is one.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|(_, at)| *at)
    }

    /// Decide on the pending event once its settle time has passed.
    pub fn poll(&mut self, now: Instant) -> Decision {
        match &self.pending {
            Some((_, at)) if *at <= now => {}
            _ => return Decision::Ignore,
        }
        let (track, _) = self.pending.take().expect("checked above");
        self.decide(track)
    }

    fn decide(&mut self, track: Track) -> Decision {
        if is_ad(&track) || !track.is_playing {
            return Decision::Ignore;
        }
        if self.last_shown.as_deref() == Some(track.id.as_str()) {
            if !self.last_had_art && track.art.is_some() {
                self.last_had_art = true;
                return Decision::CoverUpdate(track);
            }
            return Decision::Ignore;
        }
        self.last_shown = Some(track.id.clone());
        self.last_had_art = track.art.is_some();
        Decision::Show(track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtSource;
    use crate::source::NowPlayingSource;
    use crate::source::fake::{FakeSource, track};

    fn settle(d: &mut Dedupe, t: Track, now: Instant) -> Decision {
        d.push(t, now);
        d.poll(now + SETTLE)
    }

    fn shown(d: &Decision) -> Option<&str> {
        match d {
            Decision::Show(t) => Some(t.title.as_str()),
            _ => None,
        }
    }

    #[test]
    fn new_track_shows() {
        let mut d = Dedupe::new();
        let r = settle(&mut d, track("a", "A", true), Instant::now());
        assert_eq!(shown(&r), Some("A"));
    }

    #[test]
    fn pause_and_resume_do_not_show() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        assert!(shown(&settle(&mut d, track("a", "A", true), t0)).is_some());
        assert_eq!(settle(&mut d, track("a", "A", false), t0), Decision::Ignore);
        assert_eq!(settle(&mut d, track("a", "A", true), t0), Decision::Ignore);
    }

    #[test]
    fn ads_are_ignored() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        assert_eq!(settle(&mut d, track("", "Song", true), t0), Decision::Ignore);
        assert_eq!(settle(&mut d, track("x", "Advertisement", true), t0), Decision::Ignore);
    }

    #[test]
    fn paused_new_track_waits_until_played() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        assert_eq!(settle(&mut d, track("b", "B", false), t0), Decision::Ignore);
        assert_eq!(shown(&settle(&mut d, track("b", "B", true), t0)), Some("B"));
    }

    #[test]
    fn burst_shows_only_newest() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        d.push(track("a", "A", true), t0);
        assert_eq!(d.poll(t0 + Duration::from_millis(200)), Decision::Ignore);
        d.push(track("b", "B", true), t0 + Duration::from_millis(300));
        assert_eq!(d.poll(t0 + Duration::from_millis(500)), Decision::Ignore);
        let r = d.poll(t0 + Duration::from_millis(700));
        assert_eq!(shown(&r), Some("B"));
        assert_eq!(d.deadline(), None);
    }

    #[test]
    fn burst_keeps_cover_of_same_track() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        let mut with_art = track("a", "A", true);
        with_art.art = Some(ArtSource::Bytes(vec![1]));
        d.push(with_art, t0);
        d.push(track("a", "A", true), t0);
        match d.poll(t0 + SETTLE) {
            Decision::Show(t) => assert!(t.art.is_some()),
            other => panic!("expected a card, got {other:?}"),
        }
    }

    #[test]
    fn late_cover_updates_card_once() {
        let mut d = Dedupe::new();
        let t0 = Instant::now();
        settle(&mut d, track("a", "A", true), t0);
        let mut with_art = track("a", "A", true);
        with_art.art = Some(ArtSource::Bytes(vec![1]));
        assert!(matches!(settle(&mut d, with_art.clone(), t0), Decision::CoverUpdate(_)));
        assert_eq!(settle(&mut d, with_art, t0), Decision::Ignore);
    }

    #[test]
    fn fake_source_through_dedupe() {
        let script = vec![
            track("1", "One", true),
            track("1", "One", false),
            track("1", "One", true),
            track("", "Ad", true),
            track("2", "Two", true),
            track("3", "Three", true),
        ];
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        Box::new(FakeSource { tracks: script }).run(tx).unwrap();
        let mut d = Dedupe::new();
        let mut now = Instant::now();
        let mut cards = Vec::new();
        while let Ok(t) = rx.try_recv() {
            // Each event arrives a second apart, so each one settles.
            d.push(t, now);
            now += Duration::from_secs(1);
            if let Some(title) = shown(&d.poll(now)) {
                cards.push(title.to_string());
            }
        }
        assert_eq!(cards, ["One", "Two", "Three"]);
    }
}
