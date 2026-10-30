//! Cover art: fetch, disk cache, resize and dominant color.

use crate::model::{ArtSource, Track};
use crate::theme::saturation;
use anyhow::Context;
use image::{DynamicImage, GenericImageView, RgbImage, imageops::FilterType};
use std::path::PathBuf;
use std::time::Duration;

/// Covers are stored at this size: a 72 px cover at 200 percent scale.
pub const COVER_PX: u32 = 144;
const CACHE_LIMIT: usize = 50;
const TIMEOUT: Duration = Duration::from_secs(3);

/// A cover ready for a card.
#[derive(Debug, Clone)]
pub struct Cover {
    pub png: Vec<u8>,
    pub accent: [u8; 3],
}

pub struct Art {
    client: reqwest::Client,
    dir: PathBuf,
}

impl Default for Art {
    fn default() -> Self {
        Self::new()
    }
}

impl Art {
    pub fn new() -> Self {
        let dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Marquee")
            .join("covers");
        let _ = std::fs::create_dir_all(&dir);
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("Marquee/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Art { client, dir }
    }

    /// Resolve the cover of a track: from the disk cache, from bytes or from the web.
    pub async fn resolve(&self, track: &Track) -> Option<Cover> {
        let source = track.art.as_ref()?;
        let path = self.dir.join(format!("{:016x}.png", fnv1a(track.id.as_bytes())));
        if let Ok(png) = tokio::fs::read(&path).await
            && let Ok(img) = image::load_from_memory(&png)
        {
            return Some(Cover {
                accent: dominant_color(&img),
                png,
            });
        }
        let raw = match source {
            ArtSource::Bytes(b) => b.clone(),
            ArtSource::Url(url) => match self.download(url).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("cover download failed: {e:#}");
                    return None;
                }
            },
        };
        let cover = match tokio::task::spawn_blocking(move || process(&raw)).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                tracing::warn!("cover could not be decoded: {e:#}");
                return None;
            }
            Err(_) => return None,
        };
        if tokio::fs::write(&path, &cover.png).await.is_ok() {
            let dir = self.dir.clone();
            let _ = tokio::task::spawn_blocking(move || prune(&dir, CACHE_LIMIT)).await;
        }
        Some(cover)
    }

    async fn download(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let image_url = if url.contains("open.spotify.com/oembed") {
            let reply: serde_json::Value = self.client.get(url).send().await?.error_for_status()?.json().await?;
            reply["thumbnail_url"]
                .as_str()
                .context("oEmbed reply has no thumbnail_url")?
                .to_string()
        } else {
            url.to_string()
        };
        let bytes = self
            .client
            .get(image_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(bytes.to_vec())
    }
}

/// Decode, crop to a square, resize and encode as PNG.
pub fn process(raw: &[u8]) -> anyhow::Result<Cover> {
    let img = image::load_from_memory(raw)?;
    let (w, h) = img.dimensions();
    let side = w.min(h);
    let square = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    let small = square.resize_exact(COVER_PX, COVER_PX, FilterType::Lanczos3);
    let mut png = Vec::new();
    small.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok(Cover {
        accent: dominant_color(&small),
        png,
    })
}

/// Keep only the newest `limit` files in the cache folder.
fn prune(dir: &std::path::Path, limit: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= limit {
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in files.into_iter().skip(limit) {
        let _ = std::fs::remove_file(path);
    }
}

/// Stable 64-bit FNV-1a hash for cache file names.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf29ce484222325, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

/// The dominant color: shrink to 32 by 32, run k-means, take the most
/// saturated cluster that covers a fair share of the cover, then darken it.
pub fn dominant_color(img: &DynamicImage) -> [u8; 3] {
    let small: RgbImage = img.resize_exact(32, 32, FilterType::Triangle).to_rgb8();
    let pixels: Vec<[f32; 3]> = small.pixels().map(|p| p.0.map(|v| v as f32)).collect();
    let clusters = kmeans(&pixels, 5, 12);
    let total = pixels.len() as f32;
    let best = clusters
        .iter()
        .filter(|(_, n)| *n as f32 / total >= 0.08)
        .max_by(|a, b| sat(a.0).total_cmp(&sat(b.0)))
        .or_else(|| clusters.iter().max_by_key(|(_, n)| *n))
        .map(|(c, _)| *c)
        .unwrap_or([60.0, 60.0, 72.0]);
    best.map(|v| (v * 0.8).round().clamp(0.0, 255.0) as u8)
}

fn sat(c: [f32; 3]) -> f32 {
    saturation(c.map(|v| v as u8))
}

/// Plain k-means with a deterministic start: centers spread over brightness.
fn kmeans(pixels: &[[f32; 3]], k: usize, rounds: usize) -> Vec<([f32; 3], usize)> {
    if pixels.is_empty() {
        return Vec::new();
    }
    let mut sorted = pixels.to_vec();
    sorted.sort_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    let mut centers: Vec<[f32; 3]> = (0..k).map(|i| sorted[(2 * i + 1) * sorted.len() / (2 * k)]).collect();
    let mut counts = vec![0usize; k];
    for _ in 0..rounds {
        let mut sums = vec![[0f32; 3]; k];
        counts.iter_mut().for_each(|c| *c = 0);
        for p in pixels {
            let i = nearest(&centers, p);
            counts[i] += 1;
            (0..3).for_each(|j| sums[i][j] += p[j]);
        }
        for i in 0..k {
            if counts[i] > 0 {
                centers[i] = sums[i].map(|s| s / counts[i] as f32);
            }
        }
    }
    centers.into_iter().zip(counts).filter(|(_, n)| *n > 0).collect()
}

fn nearest(centers: &[[f32; 3]], p: &[f32; 3]) -> usize {
    let dist = |c: &[f32; 3]| (0..3).map(|j| (c[j] - p[j]).powi(2)).sum::<f32>();
    (0..centers.len())
        .min_by(|a, b| dist(&centers[*a]).total_cmp(&dist(&centers[*b])))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn two_tone(main: [u8; 3], share: f32, other: [u8; 3]) -> DynamicImage {
        let img = RgbImage::from_fn(64, 64, |x, _| {
            if (x as f32) < 64.0 * share {
                Rgb(main)
            } else {
                Rgb(other)
            }
        });
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn picks_saturated_color_over_gray() {
        let c = dominant_color(&two_tone([220, 30, 30], 0.3, [128, 128, 128]));
        assert!(c[0] > 120 && c[1] < 60 && c[2] < 60, "{c:?}");
    }

    #[test]
    fn result_is_darkened() {
        let c = dominant_color(&two_tone([40, 80, 250], 1.0, [0, 0, 0]));
        assert!(c[2] < 250 && c[2] > 150, "{c:?}");
    }

    #[test]
    fn gray_cover_stays_neutral() {
        let c = dominant_color(&two_tone([200, 200, 200], 0.5, [30, 30, 30]));
        assert!(saturation(c) < 0.05, "{c:?}");
    }

    #[test]
    fn process_makes_square_png() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 200, Rgb([10, 200, 90])));
        let mut raw = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png)
            .unwrap();
        let cover = process(&raw).unwrap();
        let out = image::load_from_memory(&cover.png).unwrap();
        assert_eq!(out.dimensions(), (COVER_PX, COVER_PX));
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = std::env::temp_dir().join(format!("marquee-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("{i}.png")), [i]).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        prune(&dir, 2);
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        left.sort();
        assert_eq!(left, ["3.png", "4.png"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }
}
