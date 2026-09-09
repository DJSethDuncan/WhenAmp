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

## Design

The UI chrome ("SONIC DECK" graphite skin) is ported from a Claude Design
mockup. Volume, Balance (double-click to re-center), and Repeat are wired
to real playback; EQ, PL, and Shuffle remain visual toggles carried over
from the mockup with no backing behavior (no playlist exists to shuffle or
equalize). Prev/Next restart the current track, matching the mockup's own
single-file fallback behavior. The visualizer is a real-time FFT spectrum
analyzer (bass on the left, treble on the right), fed by a mono downmix tap
on the decoded audio stream.

The window is undecorated (no native title bar) and sized to exactly fit
the chassis; the in-app title strip is the drag handle. Its three window
controls: the square button toggles a compact single-row mini-player
(time, transport, ticker, spectrum — capped at 50px tall, same width as
the full window), the dash minimizes, and the "x" closes.

## Fonts

- LCD time display: [DSEG7-Classic](https://github.com/keshikan/DSEG) by
  keshikan (SIL OFL 1.1, `assets/fonts/DSEG-LICENSE.txt`).
- Chassis labels/badges: [Silkscreen](https://github.com/googlefonts/silkscreen)
  (SIL OFL 1.1, `assets/fonts/OFL-silkscreen.txt`).
- Base UI typeface: [IBM Plex Mono](https://github.com/IBM/plex) (SIL OFL 1.1,
  `assets/fonts/OFL-ibmplexmono.txt`).

All embedded in the binary under `assets/fonts/`.

## Status

Phase 1: load / play / pause / stop / seek with a draggable progress bar
and a real-time FFT visualizer, no playlist support yet.
