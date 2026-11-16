# Roadmap

Status notation: **Now**, **Next**, **Later**, **Done**.

## Phase 0 — Research and planning — Done

- Rust stack, crate licenses, FFmpeg strategy and AI design.
- Feasibility spike for rendering and encoding.
- See [docs/research.md](docs/research.md).

## Phase 1 — Core static output — Now

- Scene IR, validation and versioning.
- Text and FIGlet layers.
- PNG and ANSI renderers.
- SVG exporter.
- CLI skeleton, configuration and `doctor`.
- CI workflow.
- User documentation.

## Phase 2 — Animation, GIF and preview — Next

- Timeline, keyframes and easing.
- Deterministic effects engine.
- Terminal preview with playback and frame stepping.
- GIF export with palette quantization and streaming.

## Phase 3 — Video and audio — Next

- FFmpeg video export (MP4/H.264 and WebM/VP9).
- Frame streaming, progress and error handling.
- Audio mux and optional voice-over.

## Phase 4 — AI generation — Later

- `ScenePlan` schema and compiler.
- OpenAI-compatible provider, validation, repair and caching.
- Offline templates.
- Optional vision preview loop.

## Phase 5 — Media import — Later

- Image to ASCII: charsets, color, braille, dithering.
- Video and GIF to ASCII frames through FFmpeg.

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
