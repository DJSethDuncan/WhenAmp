# WhenAmp

A native music player, built in Rust.

Phase 1 targets macOS with a minimal feature set: load a song, play, pause, stop.
The audio stack (cpal + symphonia + rodio) and UI stack (egui/eframe) are cross-platform,
so Windows/Linux support is expected to follow without a rewrite.

## Build & run

```sh
cargo run
```

## Testing

```sh
cargo test
```

This repo tracks its git hooks in `.githooks/`. After cloning, enable them once:

```sh
git config core.hooksPath .githooks
```

This wires up a pre-commit hook that runs `cargo test` before every commit.
See `AGENTS.md` for the test-coverage expectations for new features.

## Playlist

Click PL to open the playlist as a separate window (a real OS window via
egui's multi-viewport support, not embedded). It starts docked to the right
edge of the player and follows it around; dragging the playlist's own
titlebar undocks it, and the small circle button on its titlebar toggles
docked/undocked directly.

- Drop audio files on the **player** window to queue them right after the
  current track and start playing the first one immediately.
- Drop audio files on the **playlist** window to queue them the same way,
  without changing playback.
- ADD opens a file picker (multi-select); SAVE/LOAD read and write M3U8
  playlists (`#EXTM3U`/`#EXTINF` with one path per line — the most broadly
  compatible playlist format); CLEAR empties the queue. Click a track to
  play it now; × removes it.
- Prev/Next walk the queue; with no playlist loaded they fall back to
  restarting the current track. When a track finishes, WhenAmp repeats it
  (if Repeat is on) or auto-advances to the next queued track, matching
  ordinary media-player behavior.

## Design

The UI chrome ("SONIC DECK" graphite skin) is ported from a Claude Design
mockup. Volume, Balance (double-click to re-center), Repeat, and now
Prev/Next/PL are wired to real playback and the playlist; EQ and Shuffle
remain visual toggles carried over from the mockup with no backing behavior
(no equalizer, and the queue is navigated in order). The visualizer is a
real-time FFT spectrum analyzer (bass on the left, treble on the right),
fed by a mono downmix tap on the decoded audio stream.

The window is undecorated (no native title bar) and sized to exactly fit
the chassis; the in-app title strip is the drag handle. Its three window
controls: the square button toggles "micro mode" — a single row the same
height as the title strip (24px), same width as the full window, with its
own window controls plus the time counter, transport, title ticker, and
spectrum analyzer — the dash minimizes, and the "x" closes.

The window is transparent with a 5px-rounded chassis painted on top; this
is what makes the rounded corners actually visible (an opaque background
the same color as the chassis would hide the rounding entirely) — the area
outside the rounded shape shows the desktop through.

## Fonts

- LCD time display: [DSEG7-Classic](https://github.com/keshikan/DSEG) by
  keshikan (SIL OFL 1.1, `assets/fonts/DSEG-LICENSE.txt`).
- Chassis labels/badges: [Silkscreen](https://github.com/googlefonts/silkscreen)
  (SIL OFL 1.1, `assets/fonts/OFL-silkscreen.txt`).
- Base UI typeface: [IBM Plex Mono](https://github.com/IBM/plex) (SIL OFL 1.1,
  `assets/fonts/OFL-ibmplexmono.txt`).

All embedded in the binary under `assets/fonts/`.

## Status

Phase 1: load / play / pause / stop / seek with a draggable progress bar,
a real-time FFT visualizer, and a playlist (drag & drop, M3U8 save/import,
dockable window).
