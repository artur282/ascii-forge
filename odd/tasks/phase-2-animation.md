# ASCII Forge — Phase 2: Animation, effects, preview and GIF

Objective: turn the static scene into a deterministic animation pipeline: timeline interpolation, eight reusable effects, an interactive terminal preview and a streaming GIF exporter.

Branch: `feat/phase-1-core` (Phase 2 sits on top of the uncommitted Phase 1 work).  
Delivery: single feature branch, no push; the user reviews and commits.  
Test runner: `cargo test` (unit + integration), `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.

## Specs

### S1 — Timeline, keyframes and easing (issue #6)

- `Scene` gains a deterministic `seed` (`u64`, JSON default `0`). The same scene and seed produce identical frames.
- `src/timeline/` exposes `ease(Easing, f32) -> f32`, `interpolate(Transform, Transform, f32) -> Transform`, and `transform_at(&Layer, u32) -> Transform`.
- `transform_at` rules: no keyframes → `layer.transform`; before the first keyframe → first keyframe transform; after the last → last keyframe transform; between keys → interpolate with the source keyframe's easing and round `x`/`y`.
- Easing: linear, ease-in, ease-out, ease-in-out and cubic-bezier (`x1,y1,x2,y2`). Cubic-bezier x control points must be `0..=1` and all values finite; validation rejects bad values with a clear error.
- Keyframes must be strictly increasing and `< timeline.frames`; the existing validation is preserved and extended.
- No rotation in v1: a character grid cannot rotate without resampling.

### S2 — Effects engine (issue #7)

- `src/effect/` implements typewriter, fade, slide, scanline, glitch, wave, dissolve and blink.
- Every effect is a pure function of `(frame_index, scene seed, effect params, grid)`. It never mutates the `Scene`.
- Effects target one layer (`target: Some(id)`) or the whole canvas (`target: None`), and only apply while `start <= frame < end`.
- Layer effects run on the layer grid before it is overlaid; canvas effects run after all layers are composited.
- Deterministic randomness uses an internal splitmix64 generator seeded from the scene seed, effect seed and effect id. No wall clock, thread RNG or process entropy.
- Unknown targets fail validation; out-of-range parameters keep the existing validation errors.
- The effect catalog is documented in `docs/effects.md`.

### S3 — Terminal preview (issue #8)

- `ascii-forge preview scene.json` renders the animation in the terminal with `ratatui` + `crossterm`.
- Controls: `Space` play/pause, `Left`/`Right` step one frame, `Home`/`End` jump, `q`/`Esc` quit.
- Truecolor ANSI with 256-color and 16-color fallback, chosen from terminal capabilities; `--no-color` forces plain text.
- The status bar shows frame number, fps and duration.
- Non-TTY output prints the current frame and returns; it never hangs.
- `src/tui/` exposes a pure `frame_to_buffer(&Frame, Rect, ColorMode) -> ratatui::buffer::Buffer` covered by tests.

### S4 — GIF export (issue #9)

- `src/encode/gif.rs` streams frames to disk with `gif` + `quantette`; it never collects the whole animation in memory.
- Options: fps, loop, palette size (2..=256) and transparency.
- Per-frame delay is in 1/100 s and derives from fps; loop count comes from `scene.timeline.loop`.
- A 3-frame scene produces a valid GIF with the expected delays; a decoder test validates frame count, dimensions and delay.
- The palette is deterministic for a fixed scene/seed.
- The CLI warns on stderr when the animation is long or the estimated output is excessive, and documents the 256-color and timing limits.

### S5 — CLI integration

- `render --format gif` composes every frame through `compose_frame`, rasterizes it and streams it to the GIF encoder.
- `render --frame N` selects a static frame for PNG/JPEG/SVG/ANSI/text; out-of-range frames fail clearly.
- `preview` loads and validates the scene and calls the TUI module; `--json` is rejected with a clear message.
- At Phase 2 time `generate` and `import` remained pending; `generate` shipped in Phase 4 and `import` remains Phase 5.

### S6 — Documentation and verification

- README documents `seed`, keyframes, effects, preview controls, GIF limits and the new commands.
- ROADMAP marks Phase 2 items consistently with what actually shipped.
- Full verification: fmt, clippy `-D warnings`, test suite, release build and a manual GIF/preview smoke test.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1, S2, S5 | inline (parent) | done | `Scene.seed`, `Rgba::blend`, module contracts, compose multi-frame pipeline. |
| T2 | S1 | delegated direct (worker) | done | `src/timeline/`, `tests/timeline.rs`; 12 tests pass. |
| T3 | S2 | delegated direct (worker) | done | `src/effect/`, `tests/effects.rs`, `docs/effects.md`; 20 tests pass. |
| T4 | S4 | delegated direct (worker) | done | `src/encode/`, `tests/gif_export.rs`; 14 tests pass. |
| T5 | S3 | delegated direct (worker) | done | `src/tui/`, `tests/tui.rs`; 8 tests pass. |
| T6 | S1, S2 | inline (parent) | done | Keyframes, scale, opacity and effects compose; 8 tests pass. |
| T7 | S4, S5 | inline (parent) | done | `render --format gif`, `--gif-colors`, `--gif-transparent`, `--frame`; streaming and size warning. |
| T8 | S3, S5 | inline (parent) | done | `preview` wiring, `--json` rejection, non-TTY fallback. |
| T9 | S6 | inline (parent) | done | README, ROADMAP, `docs/effects.md`, Phase 2 feature doc. |
| T10 | all | inline (parent) | done | `fmt`, clippy, 153 tests, release build and manual GIF/preview smoke; no high-risk item required an independent verifier. |

## Log

### L1 — User authorization (verbatim)

> continua con la seguiente fase

Previous standing authorization:

> si procede con el desarrollo (si puedes desplegar varios agentes para agilizar el desarrollo mucho mejor)

### L2 — Planning notes (2026-11-16)

- Phase 2 maps to issues #6 (timeline), #7 (effects), #8 (preview) and #9 (GIF).
- The parent owns the shared contract (`Cargo.toml`, `src/lib.rs`, `src/scene/mod.rs`, `src/scene/validate.rs`, `src/scene/compose.rs`, `src/cli/mod.rs`) before parallel workers start.
- Workers own disjoint surfaces: timeline, effects, GIF encoder and TUI. The parent integrates and writes the CLI.
- No external RNG dependency: effects use an internal splitmix64 generator so determinism does not depend on a crate version.
- `ratatui 0.30` and `crossterm 0.29` are the only new dependencies.
- No commits or pushes without explicit user authorization.

### L3 — Shared contract and delegation (2026-11-16)

- Added `Scene.seed` with JSON default `0`, `Rgba::TRANSPARENT`, `Rgba::blend` and cubic-bezier validation.
- Replaced the static composer with a deterministic pipeline: timeline transform, nearest-neighbor scale, layer effects, opacity-aware overlay and canvas effects.
- Added `ratatui`/`crossterm`; kept deterministic effects dependency-free with an internal splitmix64 generator.
- Launched four workers with disjoint edit surfaces: timeline (`src/timeline/`, `tests/timeline.rs`), effects (`src/effect/`, `tests/effects.rs`, `docs/effects.md`), GIF (`src/encode/`, `tests/gif_export.rs`) and TUI (`src/tui/`, `tests/tui.rs`).
- Parent added `tests/compose_animation.rs` and CLI integration while workers ran.

### L4 — Integration (2026-11-16)

- All four workers completed. Public contracts matched the parent stubs; no API rework was needed.
- CLI now streams GIFs with `--format gif`, `--gif-colors` and `--gif-transparent`, selects static frames with `--frame`, rejects out-of-range frames and rejects `preview --json`.
- `preview` uses the TUI module; non-TTY output prints one frame without entering raw mode.
- Updated README, ROADMAP and `docs/effects.md`; fixed the effect JSON example to use the real `kind` nesting.
- `cargo fmt --all` clean.

### L5 — Final verification (2026-11-16)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 153 passed: 9 CLI, 8 compose animation, 20 effects, 21 FIGlet, 14 GIF, 12 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 29 validation, 1 smoke, 12 timeline, 8 TUI.
- `cargo build --release`: success.
- Manual smoke: rendered a 30-frame 320×180 GIF (28 KB, decoded by ffprobe with 30 frames), rendered a transparent 16-color GIF, rendered PNG frame 10 at 320×180, ran `preview --no-color` through a pseudo-TTY (entered/left the alternate screen and exited with `q`), and exercised `preview --json` and out-of-range frame errors.
- Shipped in the Phase 1-4 delivery on `main`; CI is green ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)).
