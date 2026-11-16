# ASCII Forge — Research: AI-driven ASCII images, videos and GIFs in Rust

**Status:** research complete — implementation not started  
**Date:** 2026-11-16  
**Project path:** `/home/arthur/Documentos/Desarrollo/personal/ascii-forge`  
**Target:** a Rust CLI that turns natural-language instructions into ASCII art, then exports PNG, SVG, GIF, MP4 and WebM.

> This document replaces the initial Go-oriented investigation. No Go code was written; the only artifact from that phase was the project directory.

---

## 1. Goal and scope

### Primary use cases

1. **Logo generation**
   - Prompt: "create a retro terminal logo for my program called NEBULA".
   - Output: ASCII logo as text, PNG, SVG and/or an animated reveal.

2. **Promo animation**
   - Prompt: "create a 6-second promo for NEBULA, glitch style, vertical video".
   - Output: GIF, MP4 or WebM with a timeline, effects and optional audio.

### v1 scope

- Declarative scene format (JSON/YAML) as the source of truth.
- Deterministic renderer: the same scene and seed always produce the same frames.
- Exporters: TXT/ANSI, PNG, SVG, GIF, MP4, WebM.
- AI command that converts a prompt into a valid scene.
- Image/video → ASCII import as an input path.
- Terminal preview (static and animated).
- `doctor` command for dependencies, fonts and codecs.

### Explicit non-goals for v1

- Real-time terminal video player (tplay/mcat territory).
- GUI editor.
- Cloud rendering or collaboration.
- Bundling a GPL FFmpeg build.
- Training or hosting models.

---

## 2. Verified local environment

Checked on 2026-11-16:

| Item | Result |
|---|---|
| OS / arch | Linux x86_64 |
| Rust | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (58f94df478 2026-08-27)` |
| rustup | `1.29.1` |
| FFmpeg | `6.1.1-3ubuntu5` |
| FFmpeg encoders | `libx264`, `libx265`, `libvpx-vp9`, `gif`, `png`, `mjpeg` |
| FFmpeg filters | `drawtext`, `palettegen`, `paletteuse`, `scale` |
| FFmpeg build | GPL-enabled (`--enable-gpl --enable-libx264 --enable-libvpx`) |
| Monospace fonts | DejaVu Sans Mono, Ubuntu Mono, Liberation Mono, Noto Mono, FreeMono |
| TTS | `espeak-ng` installed (optional voice-over) |
| chafa / img2txt / jp2a | not installed (optional external converters) |

The system FFmpeg is GPL-enabled. The CLI should invoke the user's FFmpeg binary and **not redistribute it** with a permissively licensed binary.

---

## 3. Decision summary

| Area | Decision | Why |
|---|---|---|
| Language | Rust 2024, single binary | Performance, strong image/font ecosystem, installed toolchain is current. |
| CLI | `clap` v4 derive | De facto standard, shell completions, good help output. |
| Video I/O | FFmpeg as an **external process** (`std::process::Command` or `ffmpeg-sidecar`) | Avoids linking libav*, keeps builds simple, supports H.264/VP9 immediately. |
| GIF | `gif` crate + `quantette` | MIT/Apache, no AGPL/GPL, per-frame control, streaming possible. |
| Rendering | `image` + `fontdue` (glyph atlas) or `ab_glyph` + `imageproc` | CPU-friendly, deterministic, no GPU dependency. |
| FIGlet | `figlet-rs` | FIGlet + Toilet fonts, Apache-2.0, 787k downloads. |
| AI | OpenAI-compatible HTTP via `ureq` + `serde`/`schemars` | Provider-agnostic, no async runtime required for v1. |
| Scene format | Versioned JSON with a Rust `Scene` struct and `ScenePlan` AI layer | Validation, determinism, easy editing. |
| Preview | `ratatui` + `crossterm` | Truecolor terminal playback, mature ecosystem. |
| MCP (later) | `rmcp` | Official Rust MCP SDK, Apache-2.0. |
| License | MIT OR Apache-2.0 | Rust convention; compatible with the chosen crates. |

**Key architectural rule:** the LLM never draws ASCII character by character. It produces a high-level, validated plan (text, font, layout, beats, effects, palette). The Rust renderer turns that plan into a deterministic scene and frames. This is the only reliable way to get good logos and animations from an LLM.

---

## 4. Rust crate stack

Versions verified on crates.io on 2026-11-16.

### Core

| Crate | Version | License | Role | Notes |
|---|---:|---|---|---|
| `clap` | 4.6.7 | MIT/Apache-2.0 | CLI parsing | Derive API, completions. |
| `serde` | 1.0.229 | MIT/Apache-2.0 | Serialization | Scene/config. |
| `serde_json` | 1.0.154 | MIT/Apache-2.0 | JSON | Scene + AI responses. |
| `serde_yaml_ng` | 0.10.0 | MIT | YAML | Maintained YAML fork; optional. |
| `toml` | 1.2.1 | MIT/Apache-2.0 | Config | `Cargo.toml`-style config. |
| `anyhow` | 1.0.104 | MIT/Apache-2.0 | Application errors | Context-rich CLI errors. |
| `thiserror` | 2.0.21 | MIT/Apache-2.0 | Library errors | Typed core errors. |
| `tracing` + `tracing-subscriber` | 0.1.44 / 0.3.23 | MIT | Logging | `-v` verbosity. |
| `indicatif` | 0.18.6 | MIT | Progress bars | Frame/encode progress. |
| `rayon` | 1.12.0 | MIT/Apache-2.0 | Parallelism | Parallel frame rendering. |
| `unicode-width` | 0.2.3 | MIT/Apache-2.0 | Display width | Wide/combining runes. |
| `tempfile` | 3.27.0 | MIT/Apache-2.0 | Temp files | Tests and FFmpeg intermediates. |

### Image, text and ASCII

| Crate | Version | License | Role | Notes |
|---|---:|---|---|---|
| `image` | 0.25.10 | MIT/Apache-2.0 | Decode/encode PNG/JPEG/GIF | Standard image IO. |
| `imageproc` | 0.27.0 | MIT | Drawing, Sobel, effects | `draw_text_mut`, gradients. |
| `ab_glyph` | 0.2.32 | Apache-2.0 | Font parsing/rasterization | Simple metrics; used by imageproc. |
| `fontdue` | 0.9.5 | MIT/Apache-2.0/Zlib | Fast glyph rasterizer | Best for a custom glyph atlas. |
| `fast_image_resize` | 6.1.0 | MIT/Apache-2.0 | SIMD resizing | Image/video import. |
| `figlet-rs` | 1.0.0 | Apache-2.0 | FIGlet/Toilet | Built-in standard/small/big/slant; `from_file`. |
| `gif` | 0.14.2 | MIT/Apache-2.0 | GIF encode/decode | `Frame::from_rgba_speed`, per-frame palette. |
| `quantette` | 0.6.0 | MIT/Apache-2.0 | Palette quantization | Oklab k-means, Floyd–Steinberg. |
| `zengif` | 0.7.3 | MIT/Apache-2.0 | Streaming GIF | **Disable default features** (default `zenquant` is AGPL). |
| `apng` | 0.3.4 | MIT | APNG (optional) | Lossless web animation. |
| `quick-xml` | 0.42.0 | MIT | SVG output | Safe XML escaping. |
| `anstyle` | 1.0.14 | MIT/Apache-2.0 | ANSI styling | Truecolor SGR output. |

### Video

| Crate | Version | License | Role | Notes |
|---|---:|---|---|---|
| `ffmpeg-sidecar` | 2.7.0 | MIT | FFmpeg process wrapper | Iterator/progress; `download_ffmpeg` default feature must be disabled if bundling is unwanted. |
| `ffmpeg-next` | 9.1.0 | WTFPL | Safe libav* bindings | Requires system FFmpeg dev libs; avoid for v1. |
| `video-rs` | 0.12.0 | MIT/Apache-2.0 | High-level video | Built on ffmpeg-next; same lib dependency. |
| `openh264` | 0.9.10 | BSD-2-Clause | H.264 encoder | Optional self-contained path; C source build. |
| `rav1e` | 0.8.1 | BSD-2-Clause | AV1 encoder | Pure Rust, slow; optional. |
| `mp4` | 0.14.0 | MIT | MP4 muxer | Pair with openh264/rav1e if avoiding FFmpeg. |
| `rusty_h264` | 0.17.2 | check | Pure-Rust H.264 | Experimental; watch maturity. |

**Avoid for licensing reasons in a permissive CLI:**

- `gifski` — AGPL-3.0-or-later.
- `imagequant` — GPL-3.0-or-later.
- `x264` / `x264-sys` — GPL (libx264).
- `zengif` default features — pull `zenquant` (AGPL). Use `default-features = false` + `quantette`.
- Bundling a GPL FFmpeg build.

### AI

| Crate | Version | License | Role | Notes |
|---|---:|---|---|---|
| `ureq` | 3.5.2 | MIT/Apache-2.0 | Blocking HTTP | Lean OpenAI-compatible client; no tokio. |
| `reqwest` | 0.13.5 | MIT/Apache-2.0 | Async HTTP | Alternative if streaming/tokio is needed. |
| `async-openai` | 0.43.0 | MIT | OpenAI SDK | Convenient, but pulls tokio. |
| `schemars` | 1.2.3 | MIT | JSON Schema from Rust | Generate the schema sent to the LLM. |
| `jsonschema` | 0.61.2 | MIT | Schema validation | Optional; custom validation may be enough. |

### Terminal, testing, release

| Crate | Version | License | Role |
|---|---:|---|---|
| `ratatui` | 0.30.2 | MIT | TUI preview/scrubber |
| `crossterm` | 0.29.0 | MIT | Terminal backend/events |
| `insta` | 1.49.0 | Apache-2.0 | Snapshot tests |
| `proptest` | 1.12.0 | MIT/Apache-2.0 | Property tests |
| `criterion` | 0.8.2 | Apache-2.0/MIT | Benchmarks |
| `assert_cmd` + `predicates` | latest | MIT/Apache-2.0 | CLI integration tests |
| `rmcp` | 3.10.0 | Apache-2.0 | MCP server (phase 6) |

---

## 5. Architecture

### 5.1 Project layout

Start as one crate with modules; split into a workspace when the library/MCP surfaces stabilize.

```text
ascii-forge/
├── Cargo.toml
├── src/
│   ├── main.rs              # clap entry point
│   ├── cli/                 # commands, config, doctor
│   ├── scene/               # Scene, ScenePlan, validation, versioning
│   ├── grid/                # Cell, Grid, rune width, ANSI parsing
│   ├── figlet/              # figlet-rs wrapper + font catalog
│   ├── import/              # image/video -> grid / Frames
│   ├── timeline/            # keyframes, easing, beats
│   ├── effect/              # deterministic effects
│   ├── render/              # grid -> RGBA/PNG/SVG/ANSI
│   ├── encode/              # GIF, APNG, FFmpeg video
│   ├── ai/                  # providers, prompt, schema, preview loop
│   └── tui/                 # ratatui preview
├── assets/
│   ├── fonts/               # embedded permissive TTFs
│   └── figlet/              # embedded .flf fonts
├── examples/
├── docs/
└── tests/
```

### 5.2 Core types

```rust
pub struct Scene {
    pub version: u32,
    pub canvas: Canvas,
    pub layers: Vec<Layer>,
    pub timeline: Timeline,
    pub effects: Vec<Effect>,
    pub output: OutputSpec,
}

pub struct Canvas {
    pub cols: u16,
    pub rows: u16,
    pub cell_aspect: f32,      // cell_w / cell_h, derived from the font
    pub background: Rgba,
    pub foreground: Rgba,
}

pub struct Timeline {
    pub fps: u16,              // 1..=60
    pub frames: u32,
    pub loop_: bool,
}

pub struct Cell {
    pub ch: char,
    pub fg: Rgba,
    pub bg: Rgba,
    pub flags: CellFlags,      // bold/dim/underline/blink, reserved
}
```

Layers:

```rust
pub enum LayerKind {
    Text(TextLayer),           // literal text
    Figlet(FigletLayer),       // FIGlet/Toilet output
    Image(ImageLayer),         // pre-converted ASCII grid
    Grid(GridLayer),           // raw cells or imported ANSI
}
```

Layer transforms and keyframes:

```rust
pub struct Transform { pub x: i32, pub y: i32, pub scale: f32, pub opacity: f32 }
pub struct Keyframe { pub frame: u32, pub transform: Transform, pub easing: Easing }
```

### 5.3 Render pipeline

```text
Scene (JSON)
   │ validate
   ▼
Timeline::frame(i) -> Frame (grid of Cells)
   │ effects (typewriter, scanline, glitch, wave, dissolve, ...)
   ▼
Renderer::rasterize(&Frame, &Font) -> RgbaImage
   │
   ├─ PNG / JPEG
   ├─ SVG (text elements, not rasterized)
   ├─ ANSI (truecolor SGR)
   ├─ GIF (palette quantization)
   └─ raw RGBA frames -> FFmpeg -> MP4 / WebM
```

Determinism:

- All randomness uses a seeded PRNG (`rand_chacha` or `rand_pcg`) stored in the scene.
- Effects are pure functions of `(frame_index, seed, params)`.
- No wall-clock, locale or environment dependence in rendering.
- Golden tests hash the output of fixed scenes.

### 5.4 Performance pipeline

- `rayon` renders frames in parallel; results are collected in order.
- For video, stream frames to FFmpeg's stdin instead of writing PNG intermediates.
- Bound memory with a small ring buffer (e.g., 4–8 frames in flight).
- For long GIFs, stream frames to `zengif` (with a permissive quantizer) or use FFmpeg's two-pass palette; do not collect hundreds of full RGBA frames.
- Reuse a glyph atlas: rasterize each `(char, size, style)` once with `fontdue`, then blit coverage masks per cell. This avoids re-rasterizing glyphs thousands of times.

---

## 6. AI generation design

### 6.1 Two-stage generation

**Stage 1 — `ScenePlan` (LLM-friendly)**

The model returns a compact, high-level plan with enums and bounded numbers:

```json
{
  "title": "NEBULA",
  "tagline": "navigate the noise",
  "template": "promo",
  "palette": ["#00ff88", "#0a0c10", "#ff3b6b"],
  "font": "standard",
  "beats": [
    {"kind": "logo_reveal", "duration": 1.5, "effect": "typewriter"},
    {"kind": "tagline", "duration": 1.0, "effect": "fade"},
    {"kind": "glitch", "duration": 0.5, "effect": "glitch"},
    {"kind": "outro", "duration": 1.0, "effect": "scanline"}
  ],
  "aspect": "16:9"
}
```

**Stage 2 — `Scene` (deterministic compiler)**

Rust compiles the plan into the full scene: canvas dimensions, layer positions, keyframes, effect parameters, exact frame count. The LLM never emits raw keyframes, pixel coordinates or ASCII grids.

Why:

- Smaller, more reliable JSON.
- Enums let the model choose only supported fonts/effects.
- The compiler enforces layout, aspect ratio and timing.
- Easier to retry/repair a bad plan.

### 6.2 Provider abstraction

```rust
pub trait LlmProvider {
    fn complete_json(&self, system: &str, user: &str, schema: &serde_json::Value) -> Result<String>;
}
```

Implementations:

- `OpenAiCompatible` — OpenAI, DeepSeek, OpenRouter, Groq, LM Studio, vLLM, Ollama (`/v1`).
- `Anthropic` — native messages API (optional).
- `Gemini` — OpenAI-compatible endpoint or native API (optional).
- `Mock` — tests and offline demos.

Request shape (OpenAI-compatible):

```json
{
  "model": "gpt-5.x",
  "messages": [
    {"role": "system", "content": "You are an ASCII art director. Return only JSON matching the schema."},
    {"role": "user", "content": "<prompt + schema + catalog + constraints>"}
  ],
  "response_format": {
    "type": "json_schema",
    "json_schema": {"name": "scene_plan", "strict": true, "schema": { ... }}
  },
  "temperature": 0.7
}
```

Fallbacks:

- If the provider rejects `json_schema`, retry with `{"type": "json_object"}`.
- If that fails, extract the first JSON object from the text, strip code fences, and validate.
- Maximum two repair attempts; then fail with the validation errors.

### 6.3 Prompt contents

- The JSON schema generated by `schemars`.
- A catalog of supported FIGlet fonts, effects, templates and palettes.
- Hard constraints: max cols/rows, fps, duration, output formats.
- 2–3 few-shot examples (logo, promo, glitch).
- Explicit rule: "Do not draw ASCII character by character. Choose text, font, layout, beats and effects."

### 6.4 Preview-in-the-loop (optional, high value)

After generating a plan, render 1–3 key frames to PNG and send them to a vision-capable model:

> "Here is the rendered first frame, mid frame and final frame. Does the logo fit? Is the contrast sufficient? Is the text legible? Suggest at most 3 concrete changes."

Allow **one** revision, then stop. This mirrors the draw → preview → tweak loop that makes `ASCII_ART_MCP` effective. Cache the rendered previews and the model's critique.

### 6.5 Cost, caching and secrets

- Cache responses under `.ascii-forge/cache/<sha256>.json`; `--no-cache` to bypass.
- Log token usage and estimated cost per generation.
- `--dry-run` prints frames, pixels, estimated output size and token budget before calling the API.
- Secrets from environment variables only:
  - `ASCII_FORGE_API_KEY`
  - `OPENAI_API_KEY`
  - `ANTHROPIC_API_KEY`
  - `OLLAMA_HOST`
- Config file stores provider, model, base URL and defaults, never the key.
- Never pass the API key to FFmpeg or child processes.

### 6.6 Offline mode

Templates must work without an API key:

```bash
ascii-forge generate --template promo --title NEBULA --tagline "navigate the noise"
```

The same compiler is used; only the plan source differs (LLM vs. template).

---

## 7. Output formats

### 7.1 TXT / ANSI

- Plain text: one rune per cell.
- ANSI: truecolor `\x1b[38;2;R;G;Bm` / `\x1b[48;2;R;G;Bm`, reset per line.
- `--no-color` for pipes/CI.
- Respect `NO_COLOR` and `CLICOLOR`.

### 7.2 PNG / JPEG

- PNG default (sharp edges, lossless).
- JPEG optional for smaller files, but avoid for logos.
- Rasterize at `cols * cell_w` × `rows * cell_h`.
- `--scale 2` for high-DPI output.
- Transparent background optional.

### 7.3 SVG

- One `<text>` element per row or per color run; `xml:space="preserve"`.
- `font-family="DejaVu Sans Mono, monospace"`.
- Per-cell color via `<tspan fill="...">` or separate `<text>` runs.
- For portable logos, optionally convert glyphs to paths (future; requires a font outline library such as `ttf-parser` + `kurbo`).
- `quick-xml` for escaping.
- SVG is the best format for a static logo.

### 7.4 GIF

- `gif` crate, `Frame::from_rgba_speed` or `quantette` palette + indexed pixels.
- 10–20 fps recommended; GIF delays are in centiseconds.
- 256 colors per frame; use a global palette for consistent branding.
- Memory: full RGBA frames are large. For long animations:
  - Render at a lower resolution (e.g., 1280×720).
  - Stream frames to `zengif` with `default-features = false` + `quantette`.
  - Or use FFmpeg's `palettegen`/`paletteuse` two-pass approach.
- No audio in GIF.

### 7.5 MP4 / WebM

Verified raw-frame pipe (tested locally):

```bash
ffmpeg -y -f rawvideo -pix_fmt rgba -s 1560x1440 -r 30 -i - \
  -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" \
  -an -c:v libx264 -crf 18 -preset medium \
  -pix_fmt yuv420p -movflags +faststart out.mp4
```

WebM:

```bash
ffmpeg -y -f rawvideo -pix_fmt rgba -s 1560x1440 -r 30 -i - \
  -an -c:v libvpx-vp9 -crf 30 -b:v 0 -pix_fmt yuv420p out.webm
```

Important findings:

- **H.264 with `yuv420p` requires even width and height.** The verified spike produced 195×144 and FFmpeg refused; padding to 196×144 fixed it.
- `-movflags +faststart` is important for web playback.
- `-crf 18` is visually lossless for ASCII; `23` is smaller.
- Audio: encode video first, then mux:
  ```bash
  ffmpeg -i video.mp4 -i voice.wav -c:v copy -c:a aac -shortest final.mp4
  ```
- Voice-over: `espeak-ng` → WAV → mux. Optional; user-provided music is easier and avoids TTS quality issues.

### 7.6 APNG / HTML

- APNG via the `apng` crate: lossless, larger than GIF, good for modern web.
- HTML: `<pre>` with `<span style="color:#...">` runs; optional JS player that swaps frames. Great for a landing page.
- React/Ink component export: later (Ascii-Motion does this well).

### 7.7 Resolution and aspect ratio

Cell aspect from font metrics:

```text
cell_aspect = cell_width / cell_height       // ≈ 0.54 for DejaVu Sans Mono at 24px
cols / rows = target_aspect / cell_aspect
```

Examples:

| Target | Aspect | Suggested grid |
|---|---:|---|
| 16:9 (1920×1080) | 1.778 | 160×49 |
| 9:16 (1080×1920) | 0.5625 | 90×87 |
| 1:1 (1080×1080) | 1.0 | 100×54 |
| Terminal 80×24 | 3.33 | 80×24 (cell aspect forced) |

Expose `--cols`, `--rows`, `--aspect` and `--fit` (pad/crop/stretch).

---

## 8. Image and video → ASCII import

### 8.1 Image → grid

Pipeline:

1. Decode with `image`.
2. Resize to the target cell grid with `fast_image_resize` (Lanczos3).
3. Compute luminance in linear light:
   ```text
   Y = 0.2126 R + 0.7152 G + 0.0722 B
   ```
   Convert sRGB to linear before averaging, then map back.
4. Map luminance to a character ramp, e.g. `" .:-=+*#%@"` (dark → light).
5. Optional:
   - Edge detection (Sobel via `imageproc`) to pick directional glyphs `| / \ - _`.
   - Floyd–Steinberg dithering for 1-bit/monochrome looks.
   - Braille mode: 2×4 pixels per cell → Unicode braille (U+2800 + bits). Highest effective resolution.
   - Color mode: average cell color as `fg`, background as `bg`.
   - Alpha handling: transparent pixels → spaces.

References:

- `TheZoraiz/ascii-image-converter` (Go, Apache-2.0) — braille, dithering, charset mapping.
- `orhnk/RASCII` (Rust, MIT) — `RenderOptions` with charset, color, background, invert.
- `hpjansson/chafa` (C, LGPL-3.0) — best-in-class symbol selection and color; use as algorithm reference only.

### 8.2 Video → frames

Use FFmpeg as a decoder:

```bash
ffmpeg -v error -i input.mp4 -f rawvideo -pix_fmt rgba -s 160x90 -r 15 -
```

Read RGBA frames from stdout, convert each to a grid, then render/encode. `ffmpeg-sidecar` provides an iterator and metadata. This avoids linking libav* and works with any codec FFmpeg supports.

### 8.3 GIF → frames

`gif` crate decoder, or FFmpeg. For simple imports, the crate is enough; for codec coverage, FFmpeg.

### 8.4 Phase 5 implementation notes

- `src/import/` uses `image` resizing and sRGB luminance (the linear-light idea
  remains a possible quality improvement).
- Transparent pixels are composited onto `--background` rather than dropped.
- Sobel edges and Floyd–Steinberg dithering are implemented without extra
  dependencies.
- Video/GIF frames are decoded as raw RGBA, sampled at `--fps`, bounded by
  `--max-frames`, and embedded in a `Frames` scene layer.
- The scene JSON is self-contained; short clips are the intended first use.

---

## 9. Performance

### 9.1 Verified feasibility spike

A minimal Rust spike in `/tmp` (not part of the project) validated the core path:

- `figlet-rs` standard font "AF" → 15×6 grid.
- DejaVu Sans Mono at 24px → cell 13×24 (aspect 0.542).
- Logo PNG 195×144 rendered in ~1 ms.
- 12-frame GIF (195×144, 8 cs delay) → 17.6 KB.
- 12-frame H.264 MP4 → 4.8 KB, ~83 ms including FFmpeg startup.
- H.264 needed padding from 195×144 to 196×144.

Benchmark of the naive per-cell `imageproc::draw_text_mut` path:

| Grid | Cells | Image | Frames | Time |
|---|---:|---|---:|---:|
| 120×60 | 7,200 | 1560×1440 | 10 | 305 ms total, **30.5 ms/frame** |

This is single-threaded and includes image allocation. With `rayon` and a glyph atlas, expect a 3–10× improvement.

### 9.2 Estimates

For 160×90 cells (14,400 cells), 300 frames at 30 fps:

- Rasterization: ~60 ms/frame naive → 18 s single-threaded; ~2–6 s with 8 cores + glyph atlas.
- H.264 1080p encode: ~5–15 s depending on preset.
- GIF at 1280×720, 15 fps, 150 frames: likely 30–120 s and 20–100 MB; reduce resolution/fps or stream.
- Memory: one RGBA 2080×2160 frame = 18 MB. A 4-frame pipeline = 72 MB. A full GIF frame set is prohibitive.

### 9.3 Optimizations

- Glyph atlas (`fontdue`) instead of per-glyph `draw_text_mut`.
- `rayon` for independent frames; ordered collection.
- Reuse buffers; avoid allocating a new image per frame.
- Stream video frames to FFmpeg; never write PNG intermediates.
- Stream GIF frames with `zengif` or FFmpeg palettegen.
- Offer `--preview-scale` and `--draft` for fast iteration.

---

## 10. Testing and quality

- **Golden tests:** fixed scene + seed → hash of PNG/GIF frames; `insta` snapshots for SVG/JSON.
- **Property tests:** arbitrary scene values never panic; validation rejects out-of-bounds cols/rows/fps/frames.
- **CLI tests:** `assert_cmd` + `tempfile`; `doctor` with missing FFmpeg.
- **Media tests:** `ffprobe` verifies codec, resolution, fps, frame count and duration; skip if FFmpeg is absent.
- **Benchmarks:** `criterion` for render and encode.
- **Lints:** `cargo fmt --check`, `cargo clippy -- -D warnings`.
- **Licenses/advisories:** `cargo deny check licenses advisories`; explicitly deny GPL/AGPL.
- **Determinism:** run the same scene twice and compare bytes.

CI matrix: Linux, macOS, Windows. Install FFmpeg; run unit tests and a smoke export. Keep media tests Linux-only if codecs differ.

---

## 11. Packaging and distribution

- Single binary; embed fonts and FIGlet fonts with `include_bytes!`.
- `cargo-dist` (or `cargo-release` + `goreleaser`-style scripts) for GitHub Releases.
- Homebrew tap later.
- FFmpeg is an external runtime dependency:
  - `ascii-forge doctor` checks `ffmpeg`/`ffprobe`, version, encoders and fonts.
  - `--ffmpeg-path` for custom installs.
  - Do not bundle a GPL FFmpeg build by default.
- Optional `--self-contained` path using `openh264` + `mp4` (H.264) or `rav1e` + WebM (AV1) later.
- Config:
  - `~/.config/ascii-forge/config.toml` (Linux)
  - `%APPDATA%\ascii-forge\config.toml` (Windows)
  - `~/Library/Application Support/ascii-forge/config.toml` (macOS)
- Cache and project artifacts under `.ascii-forge/`.

---

## 12. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| GPL/AGPL dependency sneaks in | License contamination | `cargo deny`; avoid gifski, imagequant, x264, zengif default. |
| FFmpeg missing or old | Video export fails | `doctor`, clear error, `--ffmpeg-path`, MP4/GIF fallback. |
| H.264 odd dimensions | Encoder refuses | Pad/scale to even dimensions (verified). |
| GIF memory/size explosion | OOM or huge files | Lower resolution/fps, stream frames, palettegen, warn. |
| Font licensing | Distribution problem | Embed only permissive fonts (DejaVu, Go Mono, JetBrains Mono OFL); include notices. |
| Unicode width/emoji | Misaligned grid | `unicode-width`; document CJK/emoji limitations; treat wide runes as 2 cells. |
| LLM returns invalid JSON | Generation fails | Schema + validation + two repair attempts + template fallback. |
| LLM cost/latency | Poor UX | Cache, dry-run estimate, local Ollama option, templates. |
| Nondeterministic output | Hard to test | Seeded PRNG; effects pure; golden tests. |
| Rust compile time | Slow iteration | Small dependency set; `cargo check`; optional `sccache`. |
| FFmpeg GPL build | Redistribution issue | Invoke system binary; don't bundle; document. |
| Cross-platform paths/terminals | Bugs | `directories` crate, `crossterm`, CI matrix. |

---

## 13. Reference projects and what to borrow

### Rust

| Project | Stars | What to borrow |
|---|---:|---|
| [`maxcurzi/tplay`](https://github.com/maxcurzi/tplay) | 704 | Terminal media playback, GIF handling, frame pipeline. |
| [`spoorn/media-to-ascii`](https://github.com/spoorn/media-to-ascii) | 122 | Image/video → ASCII, `ab_glyph`/`imageproc` rendering, FFmpeg encode. |
| [`orhnk/RASCII`](https://github.com/orhnk/RASCII) | 252 | Simple image→ASCII API, charsets, color/background. |
| [`sassman/t-rec-rs`](https://github.com/sassman/t-rec-rs) | 1,258 | Terminal recording → GIF, encoding pipeline. |
| [`Skardyy/mcat`](https://github.com/Skardyy/mcat) | 1,470 | Terminal media viewer architecture. |
| [`atanunq/viu`](https://github.com/atanunq/viu) | 3,314 | Terminal image rendering. |
| [`yuanbohan/rs-figlet`](https://github.com/yuanbohan/rs-figlet) | 71 | FIGlet/Toilet rendering (`figlet-rs`). |
| [`olekawaii/thorn`](https://github.com/olekawaii/thorn) | 50 | ASCII animation language/DSL ideas. |
| [`cascii/cascii`](https://github.com/cascii/cascii) | 4 | Video → ASCII frames with FFmpeg + Rayon. |
| [`jwnhy/video2ascii`](https://github.com/jwnhy/video2ascii) | 54 | Video→ASCII animation. |

### Other languages / tools

| Project | Stars | What to borrow |
|---|---:|---|
| [`CameronFoxly/Ascii-Motion`](https://github.com/CameronFoxly/Ascii-Motion) | 939 | Timeline, layers, keyframes, export formats, MCP design. |
| [`CameronFoxly/ascii-motion-mcp`](https://github.com/CameronFoxly/ascii-motion-mcp) | 12 | Tool surface, live/headless modes, artifact handling. |
| [`SolenmeChiara/ASCII_ART_MCP`](https://github.com/SolenmeChiara/ASCII_ART_MCP) | 0 | Draw → preview → iterate, stop-motion frames, MP4/GIF + TTS. |
| [`narsixyz/glyphcast`](https://github.com/narsixyz/glyphcast) | 87 | Browser video→ASCII studio, export video/React/frames. |
| [`hpjansson/chafa`](https://github.com/hpjansson/chafa) | 5,405 | Symbol selection, color, braille algorithms (LGPL; reference only). |
| [`TheZoraiz/ascii-image-converter`](https://github.com/TheZoraiz/ascii-image-converter) | 3,573 | Go image→ASCII, braille, dithering (Apache-2.0). |
| [`yantavares/ascii.mp4`](https://github.com/yantavares/ascii.mp4) | 116 | C++ video→ASCII MP4 rendering. |
| [`YanKaFei/ascii-cinema-engine`](https://github.com/YanKaFei/ascii-cinema-engine) | 0 | Deterministic 240×120 cell engine, causality. |
| [`ChrisBuilds/terminaltexteffects`](https://github.com/ChrisBuilds/terminaltexteffects) | 4,305 | Effect catalog and timing ideas. |

**Gap:** there is no mature Rust CLI that combines AI-authored scenes + deterministic ASCII rendering + PNG/SVG/GIF/MP4/WebM export. That is the niche.

---

## 14. Roadmap

### Phase 0 — Research (done)

- Rust stack, crate licenses, FFmpeg strategy, AI design, feasibility spike.

### Phase 1 — Core static output

- `Scene`/`Canvas`/`Cell` types + validation.
- Text and FIGlet layers.
- PNG, SVG, ANSI export.
- `clap` CLI: `render`, `fonts`, `doctor`.
- Golden tests.

### Phase 2 — Animation and GIF

- Timeline, keyframes, easing.
- Effects: typewriter, fade, slide, scanline, glitch, wave, dissolve, blink.
- Terminal preview with `ratatui`.
- GIF export with `gif` + `quantette`.
- Determinism tests.

### Phase 3 — Video

- FFmpeg external process wrapper.
- MP4 (H.264) and WebM (VP9).
- Frame streaming, progress, `--crf`, `--preset`.
- Audio mux and optional `espeak-ng` voice-over.
- `ffprobe` integration tests.

### Phase 4 — AI generation

- `ScenePlan` + compiler.
- OpenAI-compatible provider, schema, validation, retries.
- Cache, dry-run, cost logging.
- Templates: logo, promo, glitch, matrix, lower-third.
- Optional vision preview loop.

### Phase 5 — Image/video import and TUI

- Image → ASCII (charset, color, braille, dithering).
- Video/GIF → frames via FFmpeg.
- Ratatui scrubber/player.
- `import` command.

### Phase 6 — Ecosystem

- `ascii-forge mcp` via `rmcp`.
- React/HTML/APNG exports.
- `cargo-dist` releases, Homebrew tap.
- Optional self-contained H.264/AV1 path.

---

## 15. Open decisions

1. **Project name:** `ascii-forge` is provisional.
2. **Primary output:** social video (vertical MP4), web (SVG/HTML), or terminal (ANSI)? Defaults depend on this.
3. **AI provider default:** OpenAI, DeepSeek, OpenRouter, or local Ollama?
4. **FFmpeg dependency:** acceptable as an external runtime dependency? Recommended yes.
5. **Audio:** music/voice-over in v1, or video-only?
6. **License:** MIT OR Apache-2.0?
7. **Platforms:** Linux only first, or Linux/macOS/Windows from day one?
8. **Fonts:** embed only DejaVu Sans Mono, or also JetBrains Mono / Go Mono?

---

## 16. Sources

### Rust crates and docs

- clap — https://crates.io/crates/clap
- image — https://crates.io/crates/image
- imageproc — https://crates.io/crates/imageproc
- ab_glyph — https://crates.io/crates/ab_glyph
- fontdue — https://crates.io/crates/fontdue
- figlet-rs — https://crates.io/crates/figlet-rs
- gif — https://crates.io/crates/gif
- quantette — https://crates.io/crates/quantette
- zengif — https://crates.io/crates/zengif
- ffmpeg-sidecar — https://crates.io/crates/ffmpeg-sidecar
- openh264 — https://crates.io/crates/openh264
- rav1e — https://crates.io/crates/rav1e
- mp4 — https://crates.io/crates/mp4
- schemars — https://crates.io/crates/schemars
- jsonschema — https://crates.io/crates/jsonschema
- ureq — https://crates.io/crates/ureq
- async-openai — https://crates.io/crates/async-openai
- ratatui — https://crates.io/crates/ratatui
- rmcp — https://crates.io/crates/rmcp

### Reference projects

- ASCII Motion — https://github.com/CameronFoxly/Ascii-Motion
- ASCII Motion MCP — https://github.com/CameronFoxly/ascii-motion-mcp
- ASCII & SVG Art Studio MCP — https://github.com/SolenmeChiara/ASCII_ART_MCP
- Glyphcast — https://github.com/narsixyz/glyphcast
- tplay — https://github.com/maxcurzi/tplay
- media-to-ascii — https://github.com/spoorn/media-to-ascii
- RASCII — https://github.com/orhnk/RASCII
- cascii — https://github.com/cascii/cascii
- rs-figlet — https://github.com/yuanbohan/rs-figlet
- chafa — https://github.com/hpjansson/chafa
- ascii-image-converter — https://github.com/TheZoraiz/ascii-image-converter
- ascii.mp4 — https://github.com/yantavares/ascii.mp4
- ascii-cinema-engine — https://github.com/YanKaFei/ascii-cinema-engine
- terminaltexteffects — https://github.com/ChrisBuilds/terminaltexteffects

### Standards and guides

- FIGlet fonts — http://www.figlet.org/
- Unicode braille patterns — https://www.unicode.org/charts/PDF/U2800.pdf
- FFmpeg rawvideo — https://ffmpeg.org/ffmpeg-formats.html#rawvideo
- FFmpeg H.264 encoding guide — https://trac.ffmpeg.org/wiki/Encode/H.264
- FFmpeg VP9 encoding guide — https://trac.ffmpeg.org/wiki/Encode/VP9
- GIF89a specification — https://www.w3.org/Graphics/GIF/spec-gif89a.txt
