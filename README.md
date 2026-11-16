# ASCII Forge

AI-driven ASCII art, images, GIFs and videos from the command line.

> **Status:** Phases 1-4 are implemented on `main`; CI is green.
> The scene format, text/FIGlet layers, static PNG/JPEG/ANSI/SVG rendering,
> timeline/keyframe animation, deterministic effects, terminal preview, GIF
> export, MP4/WebM video, audio muxing, voice-over, AI scene planning, offline
> templates, opt-in vision review, `generate`, `render`, `fonts` and `doctor`
> are implemented. `import` remains a CLI skeleton.
>
> See [docs/research.md](docs/research.md) for the investigation,
> [docs/effects.md](docs/effects.md) for the effect catalog, and
> [ROADMAP.md](ROADMAP.md) for the delivery plan.

## Why ASCII Forge

Most ASCII tools are either image converters or web editors. ASCII Forge is a
**programmable pipeline**:

- Describe a logo or a promo in natural language.
- The model returns a small, validated **scene plan** — not raw ASCII, not keyframes.
- A deterministic Rust compiler turns that plan into an editable scene.
- The same scene renders to text, PNG, SVG, GIF, MP4 or WebM.

The goal is repeatable output: the same scene and seed must produce the same
frames. The model decides *what*; Rust decides *how*.

## Implemented

### Scene

- Versioned `Scene` / `Canvas` / `Layer` / `Timeline` / `Effect` / `OutputSpec` types.
- JSON serialization and a generated JSON Schema.
- Validation with collected, actionable errors: canvas, timeline, layer IDs,
  text/font limits, transforms, keyframes, effect targets and parameters, output
  dimensions, and even dimensions for MP4/WebM.

### Text and FIGlet

- FIGlet rendering with `standard`, `small`, `big`, `slant` and custom `.flf` files.
- Accent stripping, typographic-quote normalization and printable-ASCII mapping.
- Font fitting with fallback order and nearest-neighbor scaling.
- Text wrapping, alignment and transparent backgrounds.

### Animation

- Per-layer keyframes for `x`, `y`, `scale` and `opacity`.
- Linear, ease-in, ease-out, ease-in-out and cubic-bezier easing.
- Deterministic `Scene.seed`; the same scene and seed produce identical frames.
- Eight effects: typewriter, fade, slide, scanline, glitch, wave, dissolve and
  blink. Effects target one layer or the whole canvas and run only inside their
  `start`/`end` window.
- Effects never mutate the scene; randomness comes from an internal seeded
  generator, not the wall clock or thread RNG.

### Rendering

- PNG and JPEG rasterization with a monospace TTF and deterministic cell metrics.
- Truecolor ANSI and plain-text output.
- SVG export with color runs, `xml:space="preserve"` and XML escaping.
- Streaming GIF export with `gif` + `quantette`, per-frame delays, loop control,
  deterministic palettes and optional transparency.

### Video and audio

- MP4/H.264 and WebM/VP9 export through an external FFmpeg process.
- Raw RGBA frames stream to FFmpeg stdin; no PNG intermediates and no full
  animation in memory.
- CRF, x264 preset, fps, width and height overrides.
- Odd dimensions are padded to even sizes for `yuv420p`.
- Existing audio can be muxed into MP4 without re-encoding the video.
- `--voice` synthesizes a voice-over with `espeak-ng` and muxes it into MP4.

### AI generation and templates

- A small, bounded `ScenePlan` schema: title, tagline, template, aspect, palette,
  font, layout, beats and seed.
- A deterministic Rust compiler turns a plan into a valid editable scene. The
  model never chooses paths, commands, pixels or raw keyframes.
- `generate` uses an OpenAI-compatible `/chat/completions` endpoint through
  `ureq`, requests a JSON Schema response and falls back to JSON mode when a
  provider rejects structured output. `--provider` is a label for the cache and
  output; `--base-url` must point at an OpenAI-compatible endpoint.
- Invalid JSON and invalid plans trigger at most two repair attempts with the
  validation errors. A third failure exits with the collected errors.
- Five offline templates (`logo`, `promo`, `glitch`, `matrix`, `lower_third`)
  use the same compiler and need no network or API key.
- Successful plans and token usage are cached under `.ascii-forge/cache/ai/`;
  `--no-cache` bypasses the cache. Token usage is reported on stderr and in
  `--json` output.
- `--vision` renders the first, middle and last frames to low-resolution PNGs,
  asks a vision-capable model for one critique and allows at most one revised
  plan. The loop is opt-in, critiques are cached under
  `.ascii-forge/cache/vision/`, and a network or render failure keeps the
  original scene.

### CLI

- `ascii-forge generate "retro terminal logo for NEBULA" --out scene.json`
  generates a scene through an OpenAI-compatible provider.
- `ascii-forge generate --template promo --title NEBULA --tagline "navigate the noise"`
  builds a scene offline.
- `ascii-forge render scene.json --format png|jpeg|svg|ansi|text --frame N --out ...`
  composes one frame from text/FIGlet/grid layers and writes it.
- `ascii-forge render scene.json --format gif --out animation.gif` streams every
  timeline frame to a GIF.
- `ascii-forge render scene.json --format mp4|webm --out video.mp4` streams every
  timeline frame to FFmpeg. `--crf`, `--preset`, `--fps`, `--width`, `--height`,
  `--ffmpeg-path`, `--audio` and `--voice` tune the export.
- `ascii-forge preview scene.json` plays the animation in a truecolor terminal
  with play/pause, frame stepping and scrubbing. Non-TTY output prints one frame
  and exits.
- `ascii-forge fonts` lists the FIGlet catalog and the resolved render font.
- `ascii-forge doctor` checks FFmpeg/ffprobe, `libx264`, `libvpx-vp9`, a
  monospace font and config-directory writability. It supports `--json`.
- Global `--verbose`, `--json`, `--no-color`, `--help` and `--version`.

## Planned

- `import` for image/video to ASCII, including braille and dithering.
- Advanced audio mixing (the current pass muxes one audio track).
- MCP server and release packaging.

## Requirements

- Rust 1.99 or newer (edition 2024).
- FFmpeg 6 or newer with `libx264` and `libvpx-vp9` for video export, plus
  `ffprobe` for validation tests.
- A monospace TTF font. DejaVu Sans Mono is the default.
- `espeak-ng` is optional and only needed for `--voice`.
- An API key only for AI prompt mode or `--vision`. Offline templates need no
  key and no network.

FFmpeg is an **external runtime dependency**. ASCII Forge does not bundle a GPL
FFmpeg build or any audio assets.

## Build from source

```bash
cargo build --release
./target/release/ascii-forge --help
```

A published binary and install script will be added in the packaging phase.

## Usage

Implemented today:

```bash
# AI prompt mode: requires ASCII_FORGE_API_KEY or OPENAI_API_KEY
export ASCII_FORGE_API_KEY="..."
ascii-forge generate "retro terminal logo for NEBULA" --out scene.json
ascii-forge generate "a glitchy promo for ORBIT" --provider openai \
  --model gpt-4o-mini --out orbit.json
ascii-forge generate "a neon logo" --vision --vision-model gpt-4o-mini \
  --out reviewed.json

# Offline templates: no network, no API key
ascii-forge generate --template logo --title NEBULA \
  --tagline "navigate the noise" --seed 7 --out logo.json
ascii-forge generate --template promo --title NEBULA --out promo.json
ascii-forge generate --template glitch --title ORBIT --out glitch.json
ascii-forge generate --template matrix --title WAKE --out matrix.json
ascii-forge generate --template lower-third --title LIVE --out live.json

# List FIGlet fonts and the resolved render font
ascii-forge fonts

# Human-readable and machine-readable diagnostics
ascii-forge doctor
ascii-forge doctor --json

# Render one frame of a scene (frame 0 by default)
ascii-forge render scene.json --format png --out logo.png
ascii-forge render scene.json --format png --frame 12 --out frame-12.png
ascii-forge render scene.json --format svg --out logo.svg
ascii-forge render scene.json --format text
ascii-forge render scene.json --format ansi

# Stream every timeline frame to a GIF
ascii-forge render scene.json --format gif --out animation.gif
ascii-forge render scene.json --format gif --gif-colors 64 --gif-transparent --out animation.gif

# Play the animation in the terminal
ascii-forge preview scene.json

# MP4/H.264 and WebM/VP9
ascii-forge render scene.json --format mp4 --out promo.mp4
ascii-forge render scene.json --format mp4 --crf 20 --preset slow --out promo.mp4
ascii-forge render scene.json --format webm --out promo.webm

# Mux an existing track or synthesize a voice-over (MP4 only)
ascii-forge render scene.json --format mp4 --audio track.wav --out promo.mp4
ascii-forge render scene.json --format mp4 --voice "Welcome to NEBULA" --out promo.mp4
```

Offline template shapes:

| Template | Aspect | Layout | Default palette | Typical use |
| --- | --- | --- | --- | --- |
| `logo` | 1920×1080 | centered | dark, cyan, white | product or channel mark |
| `promo` | 1920×1080 | stacked | dark, magenta, white | short announcement |
| `glitch` | 1920×1080 | full-bleed | black, green | signal-corruption intro |
| `matrix` | 1080×1920 | full-bleed | black, green | portrait loop |
| `lower_third` | 1920×1080 | lower-third | dark, white, cyan | name/title caption |

`generate` flags: `--template`, `--title`, `--tagline`, `--seed`, `--provider`,
`--model`, `--base-url`, `--vision`, `--vision-model`, `--no-cache`, `--out` and
the global `--json`. Template mode uses `--title` as its required title and
never calls the network.

`--output` is accepted as an alias of `--out`. `preview` controls:

| Key | Action |
| --- | --- |
| `Space` | Play or pause |
| `Left` / `Right` | Step one frame |
| `Home` / `End` | Jump to the first or last frame |
| `q` / `Esc` | Quit |

The import command is still a skeleton:

```bash
ascii-forge import input.png --cols 100 --color
```

It exits with `import is not implemented yet`.

## Scene format

A minimal scene:

```json
{
  "version": 1,
  "seed": 7,
  "canvas": {
    "cols": 40,
    "rows": 12,
    "cell_aspect": 0.542,
    "background": { "r": 10, "g": 12, "b": 16, "a": 255 },
    "foreground": { "r": 0, "g": 255, "b": 136, "a": 255 }
  },
  "layers": [
    {
      "id": "logo",
      "kind": { "type": "figlet", "text": "NEBULA", "font": "standard" },
      "transform": { "x": -40, "y": 0, "scale": 1.0, "opacity": 0.0 },
      "keyframes": [
        {
          "frame": 0,
          "transform": { "x": -40, "y": 0, "scale": 1.0, "opacity": 0.0 },
          "easing": "ease_out"
        },
        {
          "frame": 20,
          "transform": { "x": 0, "y": 0, "scale": 1.0, "opacity": 1.0 },
          "easing": "ease_in_out"
        }
      ]
    }
  ],
  "timeline": { "fps": 30, "frames": 60, "loop": true },
  "effects": [
    {
      "id": "fade-in",
      "kind": { "type": "fade", "from": 0.0, "to": 1.0 },
      "target": "logo",
      "start": 0,
      "end": 20
    }
  ],
  "output": { "width": 1920, "height": 1080, "format": "gif" }
}
```

Validation limits are defined in `src/scene/validate.rs` and pinned by
integration tests. The JSON Schema is generated from the Rust types and checked
by `tests/scene_schema.rs`. `output.width` and `output.height` are exact for
PNG/JPEG/GIF; SVG uses them for the document size while keeping a logical
viewBox. Keyframes must be strictly increasing, stay before `timeline.frames`
and use easing with cubic-bezier x control points in `0..=1`.

GIF is limited to 256 colors and 1/100-second timing. `--gif-colors` lowers the
palette size and `--gif-transparent` keeps the canvas transparent instead of
painting the scene background. Very long or high-resolution animations should
use MP4 instead; ASCII Forge warns on stderr when a GIF is likely to be
excessive.

## Configuration

The config file is loaded from the platform config directory:

| Platform | Path |
| --- | --- |
| Linux | `~/.config/ascii-forge/config.toml` |
| macOS | `~/Library/Application Support/ascii-forge/config.toml` |
| Windows | `%APPDATA%\ascii-forge\config.toml` |

Supported keys:

```toml
[ffmpeg]
path = "/usr/bin/ffmpeg"

[render]
font_path = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
font_size = 24.0

[ai]
provider = "openai"
model = "gpt-5"
base_url = "https://api.openai.com/v1"
vision_model = "gpt-4o-mini"
```

Environment overrides:

- `ASCII_FORGE_FFMPEG`
- `ASCII_FORGE_FONT`
- `ASCII_FORGE_FONT_SIZE`

API keys are read from `ASCII_FORGE_API_KEY`, then `OPENAI_API_KEY`; they are
never read from the config file. Unknown config keys such as `api_key` are
rejected.

Caches and generated artifacts live under `.ascii-forge/` in the project
directory. Pass `--no-cache` to bypass the response cache.

## Architecture

```text
Prompt
  │
  ▼
ScenePlan (LLM or template, validated)   ← implemented
  │
  ▼
Compiler (Rust, deterministic)           ← implemented
  │
  ▼
Scene (JSON, editable)                   ← implemented
  │
  ▼
Timeline + effects → frames              ← implemented
  │
  ▼
Renderer (grid of cells → frames)
  │
  ├─ TXT / ANSI                          ← implemented
  ├─ PNG / JPEG / SVG                    ← implemented
  ├─ GIF (streaming)                     ← implemented
  └─ raw RGBA → FFmpeg → MP4 / WebM      ← implemented
       └─ optional audio → AAC / Opus    ← MP4 AAC implemented
```

The LLM never draws ASCII character by character and never emits keyframes. It
chooses text, font, palette, layout hints, beats and effects. The compiler
resolves the grid, timing and keyframes.

## Security and privacy

- No telemetry.
- Prompts are sent only to the provider you configure. Offline templates never
  send a request.
- `--vision` sends three low-resolution PNG frames and the validated scene plan
  to the configured vision model. It is off by default and requires an explicit
  `--vision-model` or `[ai].vision_model`.
- API keys come from `ASCII_FORGE_API_KEY` or `OPENAI_API_KEY`; they are never
  written to the config file, scene, cache or logs.
- The LLM cannot choose file paths, run commands or inject code; it only returns
  bounded enums, numbers, text and colors.
- FFmpeg is invoked with an argument array, never through a shell.
- Security reports go through GitHub private vulnerability reporting; see
  [SECURITY.md](SECURITY.md).

## Limitations

- `import` is a skeleton; image and video import are not implemented.
- AI prompt mode needs an OpenAI-compatible provider and an API key. Output is
  model-dependent; the compiled scene is deterministic once written.
- `--vision` adds latency and provider cost and is limited to one revision.
- Image layers are not supported in rendering yet.
- Audio muxing is MP4/AAC only; WebM audio is not implemented.
- The audio pass muxes one track; advanced mixing is not implemented.
- GIF is limited to 256 colors and 1/100-second timing; long or high-resolution
  animations are slow and large.
- Terminal preview degrades to 256 or 16 colors when truecolor is unavailable.
- Wide CJK glyphs and emoji are not supported in the cell grid yet; text input is
  normalized to printable ASCII.
- Raster rendering depends on a system monospace font; CI installs
  `fonts-dejavu-core`.
- FFmpeg is an external dependency; a self-contained encoder is not part of v1.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

The CI workflow is in `.github/workflows/ci.yml`. `deny.toml` is prepared for a
future `cargo-deny` step but is not wired into CI yet.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

## Roadmap

See [ROADMAP.md](ROADMAP.md). Work is tracked as one issue per feature under the
phase milestones.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
