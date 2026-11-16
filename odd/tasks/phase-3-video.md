# ASCII Forge — Phase 3: Video and audio

Objective: export animated scenes to MP4 (H.264) and WebM (VP9) through an external FFmpeg process, then optionally mux an existing audio file or an `espeak-ng` voice-over into MP4.

Branch: `feat/phase-1-core` (Phase 3 sits on top of the uncommitted Phase 1 and Phase 2 work).  
Delivery: single feature branch, no push; the user reviews and commits.  
Test runner: `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.

## Specs

### S1 — Streaming video encoder (issue #10)

- `src/encode/video.rs` streams raw RGBA frames to an external FFmpeg process through stdin; no PNG intermediates and no complete animation in memory.
- MP4 uses H.264 (`libx264`, `yuv420p`, `+faststart`). WebM uses VP9 (`libvpx-vp9`, `yuv420p`).
- The public API is `VideoFormat`, `VideoOptions`, `VideoError`, `VideoEncoder::new/write_frame/finish`.
- `write_frame` validates frame dimensions; `finish` waits for FFmpeg and reports its exit status and captured stderr.
- Missing FFmpeg, failed spawn, broken pipes and non-zero exits produce actionable errors.

### S2 — Video options and odd dimensions (issue #10)

- `--crf`, `--preset`, `--fps`, `--width`, `--height` and `--ffmpeg-path` are exposed by `render`.
- CRF defaults: 23 for MP4/H.264 and 32 for WebM/VP9. CRF is bounded by codec (H.264 `0..=51`, VP9 `0..=63`).
- `--preset` accepts the x264 preset names and maps them to `-cpu-used` for VP9.
- Odd dimensions are padded with `pad=ceil(iw/2)*2:ceil(ih/2)*2`; they never fail.
- The CLI reports encoding progress to stderr every 10% (or every frame for short clips).

### S3 — Audio mux and voice-over (issue #11)

- `src/encode/audio.rs` muxes an existing audio file into an MP4 without re-encoding the video (`-c:v copy`, AAC audio).
- `--audio FILE` validates the file with `ffprobe` and fails clearly when the file or probe is invalid.
- `--voice "text"` synthesizes a temporary WAV with `espeak-ng`, then muxes it like `--audio`.
- Audio is only muxed into MP4; WebM with audio fails with a clear message.
- Audio is padded with silence and cut at the video duration, so the output duration follows the video.
- Missing `ffmpeg`, `ffprobe` or `espeak-ng` produces a clear error naming the tool.

### S4 — CLI integration

- `render --format mp4|webm` encodes every timeline frame through `compose_frame` and the streaming video encoder.
- Audio/voice exports encode a silent temporary MP4, then run the audio mux pass into the final path.
- `--audio` and `--voice` are mutually exclusive.
- `--frame` is ignored for video formats; the whole timeline is exported.
- JSON output reports the artifact, format, frame count, fps, dimensions and whether audio was muxed.

### S5 — Tests and documentation

- Unit tests cover FFmpeg argument construction, preset mapping, CRF bounds and error messages.
- Integration tests use `ffprobe` for codec, fps, frame count, dimensions and audio streams; they skip cleanly when FFmpeg/ffprobe/espeak-ng are absent.
- Missing-FFmpeg and odd-dimension tests are non-optional where the tool is available.
- README documents video commands, CRF/preset tradeoffs, odd dimensions, audio/voice behavior and external-tool requirements.
- CI installs FFmpeg and the fonts it already needs; audio tests skip when `espeak-ng` is absent unless CI installs it.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1-S4 | inline (parent) | done | Feature doc, `tempfile` dependency, encode module contracts, CLI argument surface. |
| T2 | S1, S2 | delegated direct (worker) | done | `src/encode/video.rs`, `tests/video_export.rs`; 11 tests pass. |
| T3 | S3 | delegated direct (worker) | done | `src/encode/audio.rs`, `tests/audio_export.rs`; 12 tests pass. |
| T4 | S2, S4 | inline (parent) | done | `run_video` with streaming, progress, CRF/preset/fps/dimension flags and temporary silent MP4. |
| T5 | S3, S4 | inline (parent) | done | `--audio`, `--voice`, tool resolution, MP4-only guard and error paths. |
| T6 | S5 | inline (parent) | done | README, ROADMAP, Phase 3 feature doc. |
| T7 | all | inline (parent) | done | fmt, clippy, 183 tests, release build and manual MP4/WebM/audio/voice smoke. |

## Log

### L1 — User authorization (verbatim)

> continua

Previous standing authorization:

> si procede con el desarrollo (si puedes desplegar varios agentes para agilizar el desarrollo mucho mejor)

### L2 — Planning notes (2026-11-16)

- Phase 3 maps to issues #10 (FFmpeg video) and #11 (audio/voice-over).
- FFmpeg remains an external runtime dependency; nothing is linked or bundled.
- The video encoder owns only silent video. Audio muxing is a second pass in `src/encode/audio.rs`.
- The parent owns `Cargo.toml`, `src/lib.rs`, `src/encode/mod.rs`, `src/cli/mod.rs`, README/ROADMAP and the final verification.
- Workers own disjoint surfaces: video and audio modules with their integration tests.
- No commits or pushes without explicit user authorization.

### L3 — Shared contract and delegation (2026-11-16)

- Added `tempfile` as a runtime dependency for the temporary silent MP4 and voice WAV; CI installs `espeak-ng`.
- Added `src/encode/video.rs` and `src/encode/audio.rs` contracts and the CLI flags `--crf`, `--preset`, `--fps`, `--width`, `--height`, `--ffmpeg-path`, `--audio` and `--voice`.
- Verified the FFmpeg command shapes manually: 63x63 raw RGBA padded to 64x64 MP4, VP9 WebM, and AAC mux with `apad` + `-shortest` producing a 1.000s output from a 2s tone.
- Launched two workers in parallel: video encoder and audio/voice mux.

### L4 — Integration (2026-11-16)

- Both workers completed with matching contracts.
- `run_video` streams every composed frame to `VideoEncoder`, reports progress on stderr every 10%, and writes directly when there is no audio.
- With `--audio` or `--voice`, it writes a silent temporary MP4, synthesizes the voice when needed, probes the audio with `ffprobe`, and muxes to the final MP4.
- WebM plus audio is rejected before encoding; missing FFmpeg reports the configured path.
- Removed the obsolete “video is not implemented” CLI test and added eight CLI integration tests for MP4, WebM, odd dimensions, audio, voice, missing FFmpeg, WebM audio rejection and JSON metadata.

### L5 — Final verification (2026-11-16)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 183 passed: 12 audio, 9 CLI, 8 compose animation, 20 effects, 21 FIGlet, 14 GIF, 19 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 29 validation, 1 smoke, 12 timeline, 8 TUI, 11 video.
- `cargo build --release`: success.
- Manual smoke: 63x63 MP4 (ffprobe 64x64, 30 frames), VP9 WebM (30 frames), MP4 with an existing WAV (AAC track, 1.000s), MP4 with `--voice` (AAC track), missing-FFmpeg error, and WebM-with-audio rejection.
- Shipped in the Phase 1-4 delivery on `main`; CI is green ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)).
