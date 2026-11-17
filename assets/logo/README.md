# ASCII Forge — animated logo

Source artwork: **Vincent van Gogh, _Head of a Skeleton with a Burning Cigarette_, 1886** (public domain).
The painting is the skeleton study of the Antwerp period. This directory contains only the
rendered outputs; the source image is not redistributed.

## Deliverables

| File | Size | Purpose |
| --- | --- | --- |
| `ascii-forge-logo-thumb.webp` | 640 × 640, WebP q82 | README preview thumbnail |
| `ascii-forge-logo.png` | 1400 × 1400 | still at frame 45 (ember near peak, plume mature) |
| `ascii-forge-logo.gif` | 800 × 800, 90 frames, 128-color palette | looping animation |
| `ascii-forge-logo.mp4` | 1080 × 1080, H.264 High, CRF 20, 30 fps, 3.0 s | video master |

The scene uses a 140 × 76 grid and a `#080706` background. Layers, in order:
skeleton grid → smoke frames → title.

## Palette

| Role | Color |
| --- | --- |
| Background | `#080706` warm black |
| Bone | `#4E3B2A` → `#F6ECD8`, graded from the import with a 1.40 background-relative gain |
| Smoke | `#90887E` → `#FAF6EE`, pale warm gray to white |
| Title / ember | `#F2C14E` gold, ember tip `#FFB347` |

## Motion

- Timeline: 90 frames at 30 fps, looping.
- Title (`ASCII FORGE`, FIGlet `small`): typewriter over frames 0–26 at 9 glyphs/frame
  (all 133 glyph cells appear by frame 14), opacity 0 → 1 and y 71 → 69 over frames 0–20
  with `ease_out`, then held to frame 89.
- Canvas scanline: frames 0–90, intensity 0.10, speed 0.844444 rows/frame (one pass per loop).
- Smoke glitch: frames 82–86, intensity 0.14, seed 1886 — four frames (0.13 s), applied to the
  smoke layer only so the skull and title stay legible.
- Ember: two pulses per loop (0.67 Hz) at the cigarette tip, cells (110, 53) / (111, 53) /
  (109, 53) / (110, 54) in the smoke window.

## Smoke construction

The plume is procedural and deterministic (no image generation): 7 core strands at lanes
−4.6…+4.6 cells plus 2 outer wisps, 30 and 6 particles per strand respectively, 222 particles in
total, seeded with 1886. Each particle rises 44 rows while drifting +11.5 columns from the tip
(110.5, 53) toward (122, 8), swings on a shared 3.0+1.2-cell meander plus its strand curl, widens
from σ 0.7 to ~2.5 cells, and fades in/out with `sin(π p)^0.45` (harder fade after p = 0.70).
Four trail samples per particle are splatted into a 36 × 55-cell window, thresholded with an
ordered 4 × 4 Bayer matrix and mapped to an 11-level pale smoke ramp.

## Timing note

GIF delay is stored in 10 ms units, so 90 frames at 30 fps is encoded with a 3 cs delay and plays
in 2.7 s at 33.3 fps; the MP4 is the exact 3.0 s / 30 fps master.

The procedural scene builder and the source crop are kept outside the repository. The outputs are
self-contained and can be re-rendered from the retained scene in a development workspace.
