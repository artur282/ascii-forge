# ASCII Forge — Phase 1: Core static output

Objective: deliver the first usable vertical slice: a versioned scene format, text/FIGlet layers, PNG/ANSI/SVG output, a CLI skeleton with `doctor`, and CI.

Branch: `feat/phase-1-core`  
Delivery: single feature branch, no push; the user reviews and commits.  
Test runner: `cargo test` (unit + integration), `cargo fmt`, `cargo clippy -D warnings`.

## Specs

### S1 — Scene IR and validation (issue #1)

- Versioned `Scene` with `Canvas`, `Layer`, `Timeline`, `Effect` and `OutputSpec`.
- JSON round-trip with `serde`; JSON Schema generated with `schemars`.
- Validation rejects unknown versions, out-of-range cols/rows/fps/frames, invalid colors, duplicate layer IDs, missing effect targets and malformed keyframes.
- `Scene::validate()` returns actionable errors.

### S2 — Text and FIGlet layers (issue #2)

- `TextLayer` and `FigletLayer` render into a deterministic `Grid`.
- `figlet-rs` integration with standard, small, big and slant fonts, plus custom `.flf` files.
- Text normalization: strip accents; reject or replace unsupported glyphs with a warning.
- Fit behavior: try a smaller font, then scale down, then return a clear error.
- Font catalog available to the CLI.

### S3 — PNG and ANSI rendering (issue #3)

- Rasterize a `Frame` to RGBA using a monospace TTF and cell metrics.
- PNG and JPEG export with scale, background and transparency options.
- Plain text and truecolor ANSI output; respect `NO_COLOR` and `--no-color`.
- Deterministic output for a fixed scene and seed.

### S4 — SVG export (issue #4)

- Export a `Frame` as SVG `<text>` elements with `xml:space="preserve"`.
- Preserve per-cell color with `<tspan>` or grouped `<text>` runs.
- Escape XML safely; document the font dependency.
- Transparent background support.

### S5 — CLI skeleton, config and doctor (issue #5)

- `clap` commands: `generate`, `render`, `preview`, `import`, `fonts`, `doctor`, `--version`.
- `--json`, `--verbose`, `--no-color`.
- Config from the platform config dir; environment variables override.
- `doctor` checks Rust, FFmpeg/ffprobe, encoders, fonts and write permissions.
- Cache and artifacts under `.ascii-forge/`.

### S6 — CI workflow (issue #18)

- GitHub Actions on push/PR to `main`.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`.
- Install FFmpeg; media tests skip cleanly when unavailable.
- `contents: read` permissions; caching; reproducible locally.

### S7 — Documentation update (issue #19)

- README documents implemented commands, scene format and limitations.
- No feature is documented before it exists.
- Links to research, roadmap and security remain valid.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1-S6 | inline (parent) | done | Cargo skeleton compiles; `cargo test --test smoke` passes. |
| T2 | S1 | inline (parent) | done | `src/scene/mod.rs` types compile; smoke round-trip passes. |
| T3 | S1 | delegated direct (worker) | done | Full validation + 27 tests pass; effect ranges documented in code. |
| T4 | S2 | delegated direct (worker) | done | FIGlet/text implemented; 21 tests pass. |
| T5 | S3 | delegated direct (worker) | done | PNG/JPEG/ANSI implemented; 7 tests pass. CI now installs `fonts-dejavu-core`. |
| T6 | S4 | delegated direct (worker) | done | SVG exporter + 11 tests pass; XML escaping and color runs covered. |
| T7 | S5 | delegated direct (worker) | done | CLI/config/doctor implemented; 9 tests pass. `.ascii-forge/` cache layout completed in Phase 4 (`src/cache.rs`). |
| T8 | S6 | delegated direct (worker) | done | `.github/workflows/ci.yml` + `deny.toml`; YAML/TOML structure validated. |
| T9 | S7 | inline (parent) | done | README/ROADMAP updated; final verification recorded in L5. |
| T10 | S3, S5 | inline (parent) | done | Static `render` command composes text/FIGlet/grid layers; 7 integration tests; exact output dimensions. |

## Log

### L1 — User authorization (verbatim)

> si procede con el desarrollo (si puedes desplegar varios agentes para agilizar el desarrollo mucho mejor)

### L2 — Planning notes

- The repository was initialized from `guia-repositorio-productivo.md`; issues #1-#19 and phase milestones exist.
- Phase 1 is the first vertical slice. The parent creates the shared contract first so parallel workers have a stable interface.
- Workers edit disjoint surfaces; the parent owns `Cargo.toml`, `src/lib.rs`, `src/scene/mod.rs` and integration.
- No commits or pushes are made without explicit user authorization.

### L3 — Bootstrap and delegation (2026-11-16)

- Created branch `feat/phase-1-core`.
- Added `Cargo.toml`, `src/lib.rs`, `src/main.rs`, the scene contract, renderer/CLI stubs and a smoke test.
- `cargo check` passes; `cargo test --test smoke` passes.
- Launched six delegated workers with disjoint edit surfaces: scene validation (#1), FIGlet/text (#2), PNG/ANSI (#3), SVG (#4), CLI/config/doctor (#5), CI (#18).

### L4 — Integration (2026-11-16)

- All six workers completed. Parent fixed four clippy findings: `writeln!` in SVG, `is_multiple_of` in validation, and renamed `Rgba::rgba` to `Rgba::new`.
- Added `tests/scene_schema.rs` to verify the generated JSON Schema.
- Updated README and ROADMAP for the implemented Phase 1 slice.
- `cargo fmt --all` run; `cargo clippy --all-targets -- -D warnings` clean; full test suite passes (76 tests before the schema test; 77 after).

### L5 — Final verification (2026-11-16)

- Added the static `render` command: scene JSON → validation → first-frame composition → PNG/JPEG/SVG/ANSI/text, with exact output dimensions and 7 integration tests.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 84 passed (9 CLI, 21 FIGlet, 7 render command, 7 PNG/ANSI, 11 SVG, 1 schema, 27 validation, 1 smoke).
- `cargo build --release`: success.
- Manual smoke: `--version`, `--help`, `fonts`, `fonts --json`, `doctor`, `doctor --json`, static `render` to PNG at exactly 1280×720, SVG output, and pending-command errors.
- No commits or pushes. Changes remain on `feat/phase-1-core` for user review.
- CI is green on `main` ([run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793)). `.ascii-forge/` cache layout closed in Phase 4; GIF/video/effects/AI commands shipped in Phases 2-4.
