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

Do not merge or commit a feature with no corresponding test unless it's
purely a UI layout tweak with no new logic to verify.

## Don't run the test suite yourself

Write and reason about tests, but don't run `cargo test` as a general
validation step — that's the pre-commit hook's job (`.githooks/pre-commit`
runs it automatically on every commit, see below), and re-running the whole
suite after every edit burns tokens for no benefit over letting the hook
catch it at commit time. `cargo build` for a compile check is fine. The one
exception: actively debugging a specific failing test (e.g. `cargo test
some_test_name`) to diagnose root cause — that's targeted investigation,
not blanket validation.

## Setup note

This repo uses a tracked git hooks directory. After cloning, run:

```sh
git config core.hooksPath .githooks
```
