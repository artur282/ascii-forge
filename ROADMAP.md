# Roadmap

Status notation: **Now**, **Next**, **Later**, **Done**.

## Phase 0 — Research and planning — Done

- Rust stack, crate licenses, FFmpeg strategy and AI design.
- Feasibility spike for rendering and encoding.
- See [docs/research.md](docs/research.md).

## Phase 1 — Core static output — Done

- [x] Scene IR, validation and versioning.
- [x] Text and FIGlet layers.
- [x] PNG and ANSI renderers.
- [x] SVG exporter.
- [x] CLI `render` for static PNG/JPEG/SVG/ANSI/text, plus `fonts` and `doctor`, config loading and environment overrides.
- [x] CI workflow added and green on `main`.
- [x] README and usage documentation updated.

Completed in Phase 1:

- [x] Cache and artifact layout under `.ascii-forge/`.
- [x] First CI run on GitHub: [run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793).

### Phase 1 verification (2026-11-16)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — **84 tests passed** (9 CLI, 21 FIGlet, 7 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 27 validation, 1 smoke).
- `cargo build --release` — success.
- Manual CLI smoke: `--version`, `--help`, `fonts`, `fonts --json`, `doctor`, `doctor --json`, `render` PNG at exactly 1280×720, SVG output, and the pending-command error messages.

## Phase 2 — Animation, GIF and preview — Done

- [x] Timeline, keyframes and easing with a deterministic scene seed.
- [x] Deterministic effects engine (typewriter, fade, slide, scanline, glitch, wave, dissolve, blink).
- [x] Terminal preview with playback, frame stepping, scrubbing and color fallback.
- [x] Streaming GIF export with palette quantization, loop control and transparency.
- [x] Local verification: fmt, clippy, 153 tests, release build and manual GIF/preview smoke.
- [x] CI green on `main` ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)).

### Phase 2 verification (2026-11-16)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — **153 tests passed** (9 CLI, 8 compose animation, 20 effects, 21 FIGlet, 14 GIF, 12 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 29 validation, 1 smoke, 12 timeline, 8 TUI).
- `cargo build --release` — success.
- Manual smoke: 30-frame 320×180 GIF (28 KB) decoded by `ffprobe`; PNG frame selection; pseudo-TTY preview with `q`; `--json` and out-of-range frame errors.

## Phase 3 — Video and audio — Done

- [x] Streaming FFmpeg video export (MP4/H.264 and WebM/VP9) from raw RGBA.
- [x] CRF, preset, fps and dimension overrides; odd-dimension padding.
- [x] Video progress and actionable FFmpeg errors.
- [x] Audio mux into MP4 and optional `espeak-ng` voice-over.
- [x] Local verification: fmt, clippy, 183 tests, release build and manual MP4/WebM/audio/voice smoke.
- [x] CI green on `main` ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)).

### Phase 3 verification (2026-11-16)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — **183 tests passed** (12 audio, 9 CLI, 8 compose animation, 20 effects, 21 FIGlet, 14 GIF, 19 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 29 validation, 1 smoke, 12 timeline, 8 TUI, 11 video).
- `cargo build --release` — success.
- Manual smoke: odd 63×63 MP4 padded to 64×64, VP9 WebM, AAC audio mux, synthesized voice-over, missing-FFmpeg error and WebM-audio rejection.

## Phase 4 — AI generation — Done

- [x] Project cache layout under `.ascii-forge/` and `--no-cache`.
- [x] `ScenePlan` schema and deterministic compiler.
- [x] OpenAI-compatible provider, validation, repair and caching.
- [x] Offline templates: logo, promo, glitch, matrix and lower-third.
- [x] Optional vision preview loop.
- [x] Unit/integration tests and manual prompt/template/vision smoke tests.
- [x] CI green on `main` ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)).

### Phase 4 verification (2026-11-16)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — **269 tests passed** (12 audio, 13 CLI, 24 AI plan/compiler, 6 cache, 8 compose animation, 20 effects, 21 FIGlet, 10 generation, 14 GIF, 19 provider, 19 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 29 validation, 1 smoke, 14 templates, 12 timeline, 8 TUI, 11 video, 9 vision).
- `cargo build --release` — success.
- Manual smoke: offline `promo` scene (90 frames, three effect ranges) rendered to a 1920×1080 PNG; OpenAI-compatible mock server exercised prompt mode, token usage, one vision revision and a cache hit with `cached: true` / `attempts: 0`.

## Phase 5 — Media import — Implemented locally

- [x] Image to ASCII: charsets, color, background, invert, dithering and Sobel edges.
- [x] Braille mode using 2×4 pixels per cell and U+2800..U+28FF.
- [x] Video and GIF to ASCII frames through FFmpeg.
- [x] `Frames` scene layer with modulo playback and bounded validation.
- [x] CLI `import`, JSON reports and documentation.
- [x] Tests: image conversion, braille, color, dithering, edges, Frames layer and conditional FFmpeg video/GIF fixtures.
- [ ] CI green after the Phase 5 push (pending user authorization).

### Phase 5 verification (2026-11-16)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo test` — **293 tests passed**.
- `cargo build --release` — success.
- Manual image import: 40×11 color scene rendered to 640×325 PNG.
- Manual video/GIF import: 2 braille frames rendered to a 320×236 GIF; `--edges` produced directional glyphs; release binary verified.

## Phase 6 — Ecosystem — Later

- MCP server mode.
- Release packaging, install script and checksums.
- Optional self-contained H.264/AV1 encoder path.

## Out of scope for v1

- GUI editor.
- Real-time terminal video player.
- Cloud rendering or collaboration.
- Bundling a GPL FFmpeg build.
- Training or hosting models.
