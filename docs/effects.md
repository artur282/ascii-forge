# Effects

An effect is a deterministic transformation applied to a cell grid for a
range of frames. Effects never change the scene: composing a frame builds a
fresh grid and every effect is a pure function of the frame index, the scene
seed, its own parameters and that grid.

## Activation window

An effect is active only while `start <= frame < end`. Outside that window it
is the identity transform. Progress is

```text
progress = clamp((frame - start) / (end - start), 0, 1)
```

Because the window is half-open, the last active frame never reaches progress
`1.0`; use `end - start` long enough for the effect to settle. `start` must be
less than `end`, and `end` must not exceed `timeline.frames`.

## Layering

Every effect has an optional `target`:

| `target` | Applied by | When |
|---|---|---|
| `"layer-id"` | `apply_layer` | To that layer's own grid, before it is overlaid on the canvas. |
| omitted / `null` | `apply_canvas` | To the composited canvas, after every layer. |

Effects run in document order, so a later effect sees the output of an earlier
one. Layer effects leave vacated cells fully transparent
(`rgba(0,0,0,0)`), so the overlay keeps whatever is underneath. Canvas effects
leave vacated cells in the scene background color.

`apply_layer` ignores effects that target another layer, and `apply_canvas`
ignores effects that target a layer. An unknown `target` is a validation
error, not an effect error.

## Determinism

Randomness uses an internal SplitMix64 generator. No wall clock, thread RNG,
process entropy or hash-map order is involved, so the same scene, seed and
frame always produce the same grid.

- The effect seed combines `scene.seed`, the effect's own `seed` when the
  variant has one, and a stable FNV-1a hash of `effect.id`.
- Per-cell effects hash the combined seed with the cell's row-major index.
- `dissolve` never reseeds per frame: a cell that is visible stays visible as
  progress grows.
- `glitch` re-rolls its bands every frame from the seed mixed with the frame
  index, so the tear pattern changes over time but remains reproducible.

## Catalog

The JSON `type` tag uses `snake_case`. All effects also accept `id`, `target`,
`start` and `end`.

| Type | Parameters (defaults) | Semantics |
|---|---|---|
| `typewriter` | `chars_per_frame` (`1.0`) | Reveals `ceil((frame - start + 1) * chars_per_frame)` non-space cells in row-major order. Unrevealed cells become spaces while keeping their background; the first active frame reveals at least one character. |
| `fade` | `from` (`0.0`), `to` (`1.0`) | Opacity is `clamp(from + (to - from) * progress, 0, 1)`. Foreground and opaque background blend toward the scene background by `1 - opacity`; fully transparent cells stay transparent. |
| `slide` | `dx`, `dy` (required) | Shifts content by `round(progress * dx)` and `round(progress * dy)`; vacated cells use transparent (layer) or background (canvas). |
| `scanline` | `intensity` (`0.5`), `speed` (`1.0`) | Darkens a moving horizontal band. The band center is `((frame - start) * speed) mod rows`; rows within two cells fall off linearly, blending foreground and opaque background toward the scene background by `intensity * falloff`. |
| `glitch` | `intensity` (`0.5`), `seed` (`0`) | Splits rows into `clamp(ceil(intensity * rows / 4), 1, rows)` slices and shifts each by a seeded horizontal amount of at most `round(intensity * cols)`, then shifts one color channel of one band by `±round(intensity * 64)`. `intensity: 0` is the identity. |
| `wave` | `amplitude` (`1.0`), `frequency` (`0.1`), `speed` (`1.0`) | Shifts each column by `round(amplitude * sin(frequency * x + speed * frame / 10))` rows; vacated cells use transparent (layer) or background (canvas). |
| `dissolve` | `seed` (`0`) | Deterministically hides a cell while `hash(seed, cell_index) / u64::MAX > progress`. Progress `0` hides everything, progress `1` shows everything; the pattern is stable across frames. |
| `blink` | `period` (`30`) | Visible while `(frame / period) % 2 == 0`, hidden otherwise. `period: 0` is treated as always visible. |

## Validation bounds

`Scene::validate` rejects out-of-range effect parameters and unknown targets
with `ValidationError`. The effect engine also rejects non-finite values (and a
non-positive `chars_per_frame`) with `EffectError::InvalidParameters` when it
is called directly with an unvalidated effect.

| Parameter | Range |
|---|---|
| `chars_per_frame` | `0.01..=100` |
| `from`, `to`, `intensity` | `0..=1` |
| `dx`, `dy` | `-10000..=10000` |
| `speed` | `0..=20` |
| `amplitude` | `0..=100` |
| `frequency` | `0..=50` |
| `period` | `1..=18000` |

## Example

```json
{
  "version": 1,
  "seed": 7,
  "canvas": {
    "cols": 40,
    "rows": 12,
    "background": { "r": 10, "g": 12, "b": 16, "a": 255 },
    "foreground": { "r": 0, "g": 255, "b": 136, "a": 255 }
  },
  "layers": [
    {
      "id": "title",
      "kind": { "type": "text", "text": "ASCII FORGE", "align": "center" }
    }
  ],
  "timeline": { "fps": 30, "frames": 90, "loop": true },
  "effects": [
    {
      "id": "type",
      "kind": { "type": "typewriter", "chars_per_frame": 2.0 },
      "target": "title",
      "start": 0,
      "end": 30
    },
    {
      "id": "fade-in",
      "kind": { "type": "fade", "from": 0.0, "to": 1.0 },
      "target": "title",
      "start": 0,
      "end": 20
    },
    {
      "id": "scan",
      "kind": { "type": "scanline", "intensity": 0.35, "speed": 0.75 },
      "start": 20,
      "end": 90
    },
    {
      "id": "tear",
      "kind": { "type": "glitch", "intensity": 0.6, "seed": 42 },
      "start": 60,
      "end": 90
    }
  ]
}
```
