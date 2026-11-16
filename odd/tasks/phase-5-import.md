# ASCII Forge — Phase 5: Image and video import

Objective: turn existing images and videos/GIFs into editable ASCII scenes with aspect-correct dimensions, configurable charsets, color, braille and dithering, reusing the same scene/render pipeline.

Branch: `main` (Phase 5 follows the shipped Phases 1-4).  
Delivery: feature branch, tests, docs; no push until the user authorizes it.  
Test runner: `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.

## Specs

### S1 — Image to grid (issue #12)

- `import <image> --cols N [--rows M]` converts an image to a `Grid` with luminance-to-charset mapping.
- Rows are derived from source aspect and `cell_aspect` unless `--rows` is given: `rows = round(cols * cell_aspect * src_h / src_w)`.
- Default charset: ` .:-=+*#%@`; presets `ascii`, `blocks` (` ░▒▓█`) and `detailed`; arbitrary custom ramps are accepted.
- `--color` samples the average source color per cell; monochrome uses the configured foreground.
- `--invert` and `--background`/`--foreground` are configurable.
- Output is a valid static `Scene` with one `Grid` layer, timeline 1 frame and a PNG output spec whose pixel dimensions preserve the cell aspect.

### S2 — Braille, dithering and edges

- `--braille` uses 2×4 source pixels per character cell and Unicode braille U+2800..U+28FF.
- Braille dot order is the standard 8-dot layout; dark pixels set dots (inverted by `--invert`).
- `--dither` applies Floyd–Steinberg error diffusion to the luminance ramp.
- `--edges` applies a Sobel gradient and selects directional glyphs (`-`, `/`, `|`, `\`) above a threshold.
- `--braille` conflicts with `--edges` and `--dither` with a clear error.

### S3 — Video and GIF import (issue #12)

- `import <video-or-gif> --fps N --max-frames N` decodes through FFmpeg to raw RGBA frames.
- The first frame determines source aspect; rows follow the same aspect rule unless `--rows` is given.
- Each frame goes through the same image conversion.
- Output is a valid animated `Scene` with a `Frames` layer, timeline fps, finite frame count and GIF output spec.
- FFmpeg is an external runtime dependency; missing FFmpeg/ffprobe or a decode failure is a clear error.
- Defaults: 12 fps and 120 frames, bounded to avoid huge scene files; one
  imported video scene embeds at most 2,000,000 cells.

### S4 — Scene and CLI integration

- Add `LayerKind::Frames(FramesLayer)` with non-empty, dimension-checked grids; compose selects `frame_index % frames.len()`.
- `import` supports image and video inputs, `--cols`, `--rows`, `--charset`, `--color`, `--braille`, `--dither`, `--edges`, `--invert`, `--background`, `--foreground`, `--fps`, `--max-frames`, `--ffmpeg-path`, `--ffprobe-path`, `--out` and `--json`.
- Static images produce a PNG scene; video/GIF produce a GIF scene.
- `--json` reports output, kind (image/video), dimensions, frames, fps and conversion flags.
- README documents examples, presets, limits and the FFmpeg dependency.

### S5 — Verification

- Unit tests cover dimensions, charset mapping, color vs mono, braille range, dithering, edges and invalid options.
- Video test is conditional on FFmpeg availability and uses a tiny generated fixture.
- `cargo fmt`, clippy `-D warnings`, full tests and release build pass.
- CI is green after the delivery push.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1, S2 | inline (parent) | done | `src/import/mod.rs`; 12 import tests pass. |
| T2 | S4 | delegated direct (worker) | done | `Frames` layer, validation, compose; 9 tests pass. |
| T3 | S3 | inline (parent) | done | `src/import/video.rs`; 3 conditional FFmpeg video/GIF tests pass. |
| T4 | S4 | inline (parent) | done | CLI `import`, scene builders and JSON; 13 CLI tests pass. |
| T5 | S4, S5 | inline (parent) | done | README/ROADMAP/this document; full verification recorded in L3. |

## Log

### L1 — User authorization (verbatim)

> continua con la siguente fase

Standing authorization from the previous delivery:

> si haz los commits de las features y cierra los issues de lo que ya esta listo te autorizo a hacer push

### L2 — Planning notes (2026-11-16)

- Phase 5 maps to issue #12; #16/#17 remain Phase 6.
- Existing `ImageLayer` is a schema placeholder; import will emit `Grid`/`Frames` layers so rendering stays deterministic and self-contained.
- Video frames are embedded in the scene JSON, bounded by `--max-frames`; large videos should be exported in short slices.
- New scene variant `Frames` is needed because the current timeline animates transforms, not arbitrary per-frame grids.
- No AI/OCR; source quality drives output quality.
- No commits/pushes until the user authorizes this new phase.

### L3 — Verification (2026-11-16)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 293 passed, 0 failed, 0 ignored.
- `cargo build --release`: success.
- Manual image smoke: `import testsrc.png --cols 40 --color` produced a valid 40×11 scene and a 640×325 PNG.
- Manual video smoke: `import testsrc.mp4 --cols 20 --fps 5 --max-frames 3 --braille` produced 2 braille frames and a 320×236 GIF (9.9 KB).
- Manual edges smoke: `--edges` produced `- / | \\` directional glyphs.
- Release binary smoke: braille image import and PNG render succeeded.
- CI remains pending because Phase 5 has not been pushed yet.
