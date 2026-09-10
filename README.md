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

## Equalizer

Click EQ to toggle a 10-band graphic EQ, fused into the window the same way
the playlist is — always sits between the player and the playlist (if both
are open), no separate window. It's a real audio EQ, not decorative: each
band is an RBJ peaking biquad filter (±12dB, Q 1.2) applied live to the
decoded stream, at the classic Winamp band frequencies (60, 170, 310, 600,
1K, 3K, 6K, 12K, 14K, 16K Hz). The panel has its own ON/OFF switch,
separate from panel visibility — opening the panel doesn't engage the
filtering, so you can dial in a curve before it affects playback. Drag a
band up/down to boost/cut, double-click to reset it to 0dB, or use RESET
to zero every band at once. The small graph above the sliders traces a
line through the 10 band values — the same convention Winamp's own EQ
uses, not a computed frequency-response curve.

## Playlist

Click PL to toggle the playlist on or off. It's fused directly into the
player — literally the same OS window, rendered right below the chassis —
with no separate window to manage and no close button of its own (PL is
the only toggle). Opening it starts at a comfortably tall default size;
grab the window's bottom edge or a bottom corner to resize it taller or
shorter, and the track list fills whatever space is available. (An earlier
version of this let the playlist tear off into its own floating window;
that didn't work reliably, so it's parked for now — everything here is one
window.) The track list sits in its own recessed black panel, matching the
LCD display's chrome, inset a few pixels from the window edges.

- Drop audio files on the player window to queue them right after the
  current track and start playing the first one immediately (this also
  works while the playlist is closed).
- ADD opens a file picker (multi-select); SAVE/LOAD read and write M3U8
  playlists (`#EXTM3U`/`#EXTINF` with one path per line — the most broadly
  compatible playlist format); CLEAR empties the queue. Each row shows its
  position number; single-click only selects/highlights a track,
  double-click plays it; × removes it.
- Prev/Next walk the queue; with no playlist loaded they fall back to
  restarting the current track. When a track finishes, WhenAmp repeats it
  (if Repeat is on) or auto-advances to the next queued track, matching
  ordinary media-player behavior.

## Design

The UI chrome ("SONIC DECK" graphite skin) is ported from a Claude Design
mockup. Volume, Balance (double-click to re-center), Repeat, Prev/Next/PL,
and now EQ are wired to real playback and the playlist; Shuffle remains a
visual toggle with no backing behavior (the queue is navigated in order).
The visualizer is a
real-time FFT spectrum analyzer (bass on the left, treble on the right),
fed by a mono downmix tap on the decoded audio stream, with gain tuned so
bars use more of the available height and a treble-side boost compensating
for music's natural high-frequency rolloff so the spectrum reads evenly
across bass to treble.

Track titles (in the LCD marquee and the playlist) show as "Artist -
Title" when the file has an artist tag, falling back to just the title,
then the filename.

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
a real-time FFT visualizer, a real 10-band EQ, and a playlist (drag & drop,
M3U8 save/import).
