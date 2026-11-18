---
name: ascii-forge
description: "Trigger: generate an image, logo, poster, banner, GIF or video with the ascii-forge CLI, especially from a text-only model that cannot generate images. Covers one-shot make, capabilities discovery and rendering."
license: MIT
metadata:
  author: arthur
  version: "1.1.0"
---

## Activation Contract

Use when asked to produce a visual artifact (image, logo, banner, poster, GIF,
MP4, ASCII art) and the available model cannot generate images directly. The
`ascii-forge` CLI renders deterministic ASCII-style visuals from a title, a
tagline or an imported source. No image model is involved.

## First Step

Discover the installed contract before composing commands:

```bash
ascii-forge capabilities --json
```

Read `version`, `commands`, `formats`, `templates` and `examples` from that
object. Never invent flags or template names; if a flag is not listed by
`ascii-forge <command> --help`, it does not exist.

## One-Shot Still Image

```bash
ascii-forge make "<TITLE>" --template <logo|promo|glitch|matrix|lower-third> \
  --tagline "<TAGLINE>" --seed <N> --out <FILE> --format <png|jpeg|svg> --json
```

- `make` is offline: no API key, no network, one command.
- `--template` defaults to `logo`; `--out` defaults to a slug of the title.
- `--frame` defaults to the settled last frame, so text and fades are complete.
- Parse the single JSON object on stdout: `output`, `format`, `width`,
  `height`, `template`, `title`, `tagline`, `seed`, `frame`, `frames`.
- Report `output` and `width`x`height`; confirm the file exists.

## Choosing A Template

| Intent | Template | Why |
| --- | --- | --- |
| Quick title card | `logo` | centered title with typewriter reveal (preset, not a brand mark) |
| Announcement or launch card | `promo` | stacked title and tagline |
| Intro, teaser, signal corruption | `glitch` | full-bleed glitch and scanline |
| Portrait loop or wallpaper | `matrix` | 1080x1920 green dissolve |
| Video caption or name plate | `lower-third` | lower bar with title |

## Original Artwork And Logos

`make` and the templates are for quick cards. For a real logo, icon or brand
scene, build a custom scene: do not typeset the name as the logo and do not
downsample a flat vector drawing.

- A logo is a mark or an illustrated scene; the wordmark is a signature at most.
- Author the character grid directly: one character + color per region.
  Container bodies `H/M/#/X`, ribs `|`, edges `+`/`=`, water `~`, smoke `.:+o`,
  moon `@` with `o` craters.
- Square grid: `rows = round(cols * 0.542)` (e.g. 175×95); round shapes use
  `ry = round(rx * 0.542)`.
- Put the horizon at the waterline (around `deck` or `hull_bottom - 2`).
- Split layers (`sea`, `art`, `beacon`, `title`, `tagline`) so motion touches
  one area only.
- Effects run only inside `[start, end)`; use keyframes for a state that must
  persist, and apply the typewriter to the dim glow copy too.
- Budget: 800×800 / 72 frames / 64–128 colors is about 5 MB. Full-canvas wave
  or scanline inflates the GIF; animate small areas.
- Verify the PNG IHDR dimensions and the GIF frame count and loop flag.

A worked example of the pipeline, palette and motion lives in
[`assets/logo/README.md`](../../assets/logo/README.md).

## Animation And Video

`make` only writes static images. For motion, keep the two-step path:

```bash
ascii-forge generate --template promo --title "<TITLE>" \
  --tagline "<TAGLINE>" --out scene.json --json
ascii-forge render scene.json --format gif --out animation.gif --json
ascii-forge render scene.json --format mp4 --out clip.mp4 --json
```

GIF and video need FFmpeg. Check with `ascii-forge doctor --json` when a video
or GIF render fails.

## Source Fidelity

To reproduce an existing image or clip instead of drawing a title:

```bash
ascii-forge import <SOURCE> --cols <N> --color --out scene.json --json
ascii-forge render scene.json --format png --out <FILE>
```

Use `--edges` for line art, `--braille` for fine detail, `--dither` for
monochrome shading, and `--max-frames`/`--fps` to bound video imports.

## Hard Rules

- Always pass `--json` in automation and parse stdout; diagnostics go to
  stderr and failures return a non-zero exit code.
- Never call `generate` with a prompt unless `ASCII_FORGE_API_KEY` or
  `OPENAI_API_KEY` is configured; prefer `make` and templates otherwise.
- Do not overwrite an existing artifact silently: check the target path first
  or ask for confirmation.
- Keep one focal subject per artifact and at most one accent family; the
  templates already enforce this.
- Treat `frame`/`frames` in the JSON as evidence; do not claim an animation
  when only a still frame was rendered.

## Output Contract

Return the artifact path, format, pixel dimensions, template or source mode,
and the frame rendered. State every skipped or failed check, including missing
FFmpeg or a missing API key.
