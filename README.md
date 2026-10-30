# Marquee

A marquee is the sign above a theater entrance that shows what is playing now.
Marquee does the same for your desktop. When a new song starts in the Spotify
desktop app, a small card slides in with the cover, title, artist and album,
stays a few seconds, then slides out. The app lives in the tray (Windows) or
the menu bar (macOS). It needs no Spotify login and uses no Spotify Web API.

The card is a native window built from system parts: a DWM acrylic backdrop
with Direct2D and DirectWrite on Windows, and an `NSPanel` with an
`NSVisualEffectView` on macOS. It is not a system notification and there is
no webview anywhere.

Marquee is not affiliated with Spotify. It uses no Spotify logo, wordmark or
other asset.

## Install

### Windows 10 and 11

1. Download `Marquee-<version>-x86_64.msi` from the CI artifacts or a release.
2. Run it. It installs per user into `%LOCALAPPDATA%\Programs\Marquee`, adds a
   Start menu entry and starts Marquee.
3. The build is not signed, so SmartScreen may warn. Choose
   **More info → Run anyway**.

Build it yourself:

```bash
cargo build --release
```

```bash
cargo install cargo-wix
```

```bash
cargo wix
```

`cargo wix` needs the WiX Toolset 3 (`candle.exe` and `light.exe`) on the PATH.

### macOS 13 and later

1. Download `Marquee-<version>.dmg`, open it and drag Marquee to Applications.
2. The build is only ad-hoc signed, so Gatekeeper blocks the first start.
   Right-click Marquee in Applications, choose **Open**, then **Open** again.

Build it yourself on a Mac (needs `rustup target add aarch64-apple-darwin
x86_64-apple-darwin` and `brew install create-dmg`):

```bash
bash packaging/macos/build-app.sh
```

This makes a universal `target/macos/Marquee.app` and a dmg.

## Settings

Click the tray or menu bar icon:

| Item | Choices | Default |
|---|---|---|
| Enabled | on, off | on |
| Test card | shows a card now; each click cycles through five test covers and the placeholder | |
| Theme | System, Spotify, Cover | System |
| Position | four corners | bottom right on Windows, top right on macOS |
| Duration | 3, 5 or 8 seconds | 5 |
| Monitor | primary, monitor under cursor | primary |
| Start at login | on, off | off |
| Quit Marquee | | |

Changes apply at once and survive a restart. They are stored as JSON:

- Windows: `%APPDATA%\Marquee\settings.json`
- macOS: `~/Library/Application Support/Marquee/settings.json`

The log file `marquee.log` sits next to it. Set `MARQUEE_LOG=debug` for more
detail. Covers are cached (last 50) in the OS cache folder under
`Marquee/covers`.

### Themes

- **System** uses the system backdrop and system text colors and follows light
  and dark mode live.
- **Spotify** uses the Spotify color palette: surface `#121212`, border
  `#2A2A2A`, white and `#B3B3B3` text, and a 2 px `#1DB954` line that fills
  over the hold time. Fonts are Segoe UI and the macOS system font.
- **Cover** fills the card with a gradient from the dominant color of the
  cover to a darker shade. Text is white or black, whichever reaches a
  contrast of at least 4.5 to 1 on both ends of the gradient.

## Card metrics

Sizes are logical pixels (DIPs on Windows, points on macOS) and scale with DPI
and Retina.

| | Windows (measured) | macOS (starting values, not yet checked) |
|---|---|---|
| Card | 364 × 92 | 344 × 76 |
| Corner radius | 8 (DWM round corners) | 16 |
| Backdrop | transient acrylic plus `#202020` / `#F3F3F3` tint at 92 percent | `hudWindow` material |
| Padding | 14 | 10 |
| Cover | 64, radius 4 | 56, radius 6 |
| Title | Segoe UI Variable Text 14 semibold | system 13 semibold |
| Artist, album | 12 regular, secondary color | 12 regular, secondary and tertiary color |
| Spotify theme | cover 72, padding 10, title Segoe UI 15 bold, artist 13 | cover 72, padding 10, title 15 bold, artist 13 |
| Gap to screen edge | 16 | 16 |
| Slide | 300 ms ease-out in, 300 ms ease-in out | same |
| Reduced motion | 150 ms fade, no slide | same |

### Compared with a Windows 11 notification

`docs/screens/dark-compare-notification.png` and
`docs/screens/dark-compare-card.png` show a real toast and the System card in
the same screen region (Windows 11, dark mode, 100 percent scale).

| | Notification | Card |
|---|---|---|
| Width | 364 | 364 |
| Gap to work area | 16 right, 16 bottom | 16 right, 16 bottom |
| Corner radius | 8 | 8 (same DWM corners) |
| Border | DWM default | DWM default |
| Background | about RGB 34 | about RGB 36 |
| Secondary text | light gray | 78.6 percent white |

The first version cleared to the bare transient backdrop, which came out at
about RGB 90, much lighter than the toast. The card now lays the Fluent layer
tint (`#202020` dark, `#F3F3F3` light) at 92 percent opacity over the backdrop.
Kept on purpose: the card is 92 px tall where the toast is 126 px, and its
artist and album lines are 12 px where toast body text is 14 px, as the plan
sets. Not yet compared in light mode.

## Resource use

Measured on Windows 11 over 10 minutes with Spotify playing (3 cards shown in
that window), release build:

| | Value |
|---|---|
| Private working set (Task Manager) | 6.6 MB |
| Private bytes | 12.2 MB |
| Working set incl. shared DLL pages | 40.6 MB |
| CPU | 0.594 s in 600 s = 0.099 % of one core |

## How it reads the track

- **Windows**: the Global System Media Transport Controls session whose app id
  contains `Spotify` (verified: `Spotify.exe`). Marquee listens to
  `MediaPropertiesChanged` and `PlaybackInfoChanged` and reads the 300 × 300
  PNG thumbnail. While Spotify is not running it checks again every 5 seconds.
- **macOS**: the distributed notification
  `com.spotify.client.PlaybackStateChanged`, with the keys `Name`, `Artist`,
  `Album`, `Track ID` and `Player State`. The cover comes from
  `https://open.spotify.com/oembed?url=<track uri>` (field `thumbnail_url`)
  with a 3 second timeout and the disk cache.

The probes print the raw data on each system, and the gallery drives the real
card on Windows and saves screenshots to `docs/screens`:

```bash
cargo run --example probe_windows
```

```bash
cargo run --example probe_macos
```

```bash
cargo run --example gallery
```

`gallery` also takes `corners`, `crossfade` and `compare <toast.ps1>`.

## Known quirks

- **Windows has no track id.** Marquee builds one from artist, title and
  album. The same song on two albums counts as two songs.
- **Ads on Windows** are detected by an empty artist or the title
  `Advertisement`. On macOS ads use a `spotify:ad:` URI. Neither was observed
  in testing (the test account is Premium).
- **Windows 10 cover logo**: Spotify on Windows 10 is reported to stamp its logo
  on the thumbnail it hands to Windows. On Windows 11 the thumbnail is clean
  (checked). Not checked on Windows 10.
- **Windows 10** has no system backdrop. The card uses a solid fill in the
  system background color, a window region with an 8 px radius and a 1 px
  border. This is the intended fallback.
- **Exclusive fullscreen games** on Windows draw over the card.
- **Spotify reacts slowly at times.** A skip sometimes takes several seconds to
  reach the media session; the card follows when it does.
- **GPU drivers and overlays** load into any process that owns a top-level
  window (on the test machine the NVIDIA driver, about 50 MB). Marquee creates
  the card window only while a card shows and destroys it afterwards. Once the
  first card has shown, the driver DLLs stay mapped, which counts toward the
  working set but not toward private memory.
- **macOS has not been run yet.** The macOS source and card compile for both
  Apple targets but were built on Windows. The osascript fallback from the plan
  is not built; it is only needed if the notification route fails.

## Signing

Unsigned builds trigger Gatekeeper on macOS and SmartScreen on Windows.
Signing needs an Apple Developer account (Developer ID certificate and
notarization) and a Windows code signing certificate. `build-app.sh` only signs
ad hoc, which is enough for local tests.

## Development

```bash
cargo fmt --check
```

```bash
cargo clippy --all-targets -- -D warnings
```

```bash
cargo test
```

The macOS code can be type-checked from Windows. TLS is turned off there
because its C crypto code cannot be cross-compiled:

```bash
cargo clippy --all-targets --target aarch64-apple-darwin --no-default-features -- -D warnings
```

Layout: `src/source` reads the player, `src/dedupe.rs` decides when to show a
card, `src/art.rs` fetches and colors covers, `src/card` holds the native
windows, and `src/theme.rs`, `src/anim.rs` and `src/placement.rs` hold the pure
math with unit tests.

Before a public release, search crates.io, GitHub and the Mac App Store for
name clashes. Fallback names: Cuecard, Spinbill, Nowbar.
