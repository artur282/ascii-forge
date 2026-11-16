# ASCII Forge

AI-driven ASCII art, images, GIFs and videos from the command line.

> **Status:** Phase 0 — research and planning. No implementation yet.  
> See [docs/research.md](docs/research.md) for the technical investigation and [ROADMAP.md](ROADMAP.md) for the delivery plan.

## Why ASCII Forge

Most ASCII tools are either image converters or web editors. ASCII Forge is a **programmable pipeline**:

- Describe a logo or a promo in natural language.
- The model returns a small, validated **scene plan** — not raw ASCII, not keyframes.
- A deterministic Rust compiler turns that plan into an editable scene.
- The same scene renders to text, PNG, SVG, GIF, MP4 or WebM.

The goal is repeatable output: the same scene and seed must produce the same frames. The model decides *what*; Rust decides *how*.

## Features (planned)

### Authoring

- Natural-language generation of ASCII logos and promotional animations.
- Declarative, versioned scene format (JSON/YAML).
- Text and FIGlet/Toilet layers with font fitting and fallback.
- AI-generated scene plans with schema validation, repair and caching.
- Offline templates that work without an API key.

### Rendering

- Truecolor ANSI and plain-text output.
- PNG and JPEG rasterization with a monospace font.
- SVG export for scalable logos.
- Deterministic effects: typewriter, fade, slide, scanline, glitch, wave, dissolve and blink.

### Media

- Animated GIF with palette quantization and size warnings.
- MP4 (H.264) and WebM (VP9) through an external FFmpeg process.
- Optional audio mux and voice-over via `espeak-ng`.
- Image and video import to ASCII, including color, braille and dithering modes.

### Developer experience

- Single Rust binary.
- Terminal preview with playback and frame stepping.
- `doctor` command for FFmpeg, fonts and codecs.
- Optional MCP server for AI assistants.

## Requirements

- Rust 1.99 or newer (edition 2024).
- FFmpeg 6 or newer with `libx264` and `libvpx-vp9` for video export.
- A monospace TTF font. DejaVu Sans Mono is the default.

FFmpeg is an **external runtime dependency**. ASCII Forge does not bundle a GPL FFmpeg build.

## Install (planned)

```bash
cargo install --path .
```

A release tarball and install script will be added in the packaging phase.

## Usage (planned)

```bash
# Generate a logo scene from a prompt
ascii-forge generate "retro terminal logo for NEBULA, green on black" \
  --template logo --out nebula.json

# Render a static PNG
ascii-forge render nebula.json --format png --out nebula.png

# Render a promo video
ascii-forge render nebula.json --format mp4 --out nebula.mp4

# Preview in the terminal
ascii-forge preview nebula.json --play

# Import an image or video
ascii-forge import input.png --cols 100 --color --out scene.json

# Check dependencies and fonts
ascii-forge doctor
```

Commands are illustrative until the corresponding issue is implemented.

## Configuration (planned)

| Path | Purpose |
| --- | --- |
| `~/.config/ascii-forge/config.toml` | Provider, model, defaults, FFmpeg path |
| `.ascii-forge/cache/` | Cached AI responses and rendered previews |
| `.ascii-forge/artifacts/` | Generated projects and exports |

Secrets are read from environment variables only:

- `ASCII_FORGE_API_KEY`
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `OLLAMA_HOST`

## Architecture

```text
Prompt
  │
  ▼
ScenePlan (LLM, JSON, validated)
  │
  ▼
Compiler (Rust, deterministic)
  │
  ▼
Scene (JSON/YAML, editable)
  │
  ▼
Renderer (grid of cells → frames)
  │
  ├─ TXT / ANSI
  ├─ PNG / JPEG / SVG
  ├─ GIF
  └─ raw RGBA → FFmpeg → MP4 / WebM
```

The LLM never draws ASCII character by character and never emits keyframes. It chooses text, font, palette, layout hints, beats and effects. The compiler resolves the grid, timing and keyframes.

## Security and privacy

- No telemetry.
- Prompts and generated scenes are sent only to the provider you configure.
- API keys come from environment variables, never from the repository.
- The LLM cannot choose file paths, run commands or inject code; it only returns bounded enums, numbers, text and colors.
- FFmpeg is invoked with an argument array, never through a shell.
- Security reports go through GitHub private vulnerability reporting; see [SECURITY.md](SECURITY.md).

## Limitations

- No GUI editor.
- GIF is limited to 256 colors and is not ideal for long, high-resolution animations.
- Video export requires FFmpeg; a self-contained encoder is not part of v1.
- CJK and emoji support is limited by the chosen monospace font and grid width.
- AI output is non-deterministic; the compiled scene is deterministic once written.
- The project is in planning; none of the commands above are implemented yet.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

## Roadmap

See [ROADMAP.md](ROADMAP.md). Work is tracked as one issue per feature under the phase milestones.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
