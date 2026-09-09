# Agent Instructions — WhenAmp

## Test coverage

Every new feature or behavior change must ship with unit tests that cover it.
Before considering a change done:

- Add or update tests in the same module (`#[cfg(test)] mod tests` at the
  bottom of the file, following the existing pattern in `src/player.rs` and
  `src/main.rs`).
- Prefer testing pure logic directly (e.g. `format_duration`). For anything
  touching `Player`, which opens a real audio output device, follow the
  `test_player()` pattern: skip (don't fail) the test when no device is
  available, so the suite stays green on headless CI.
- Run `cargo test` locally and confirm it passes before committing. The
  pre-commit hook (`.githooks/pre-commit`) also runs it automatically.

Do not merge or commit a feature with no corresponding test unless it's
purely a UI layout tweak with no new logic to verify.

## Setup note

This repo uses a tracked git hooks directory. After cloning, run:

```sh
git config core.hooksPath .githooks
```
