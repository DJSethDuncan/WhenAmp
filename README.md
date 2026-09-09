# WhenAmp

A native music player, built in Rust.

Phase 1 targets macOS with a minimal feature set: load a song, play, pause, stop.
The audio stack (cpal + symphonia + rodio) and UI stack (egui/eframe) are cross-platform,
so Windows/Linux support is expected to follow without a rewrite.

## Build & run

```sh
cargo run
```

## Status

Phase 1: load / play / pause / stop, no library or playlist support yet.
