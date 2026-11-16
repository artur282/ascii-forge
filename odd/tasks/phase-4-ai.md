# ASCII Forge — Phase 4: AI generation and templates

Objective: generate scenes from natural-language prompts through an OpenAI-compatible provider, keep the model at the high-level `ScenePlan` layer, compile plans deterministically to valid scenes, and provide five offline templates that use the same compiler.

Branch: `feat/phase-1-core` (Phase 4 sits on top of the uncommitted Phase 1-3 work).  
Delivery: single feature branch, no push; the user reviews and commits.  
Test runner: `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.

## Specs

### S0 — Project cache layout (closes a Phase 1 pending item)

- `.ascii-forge/cache/<namespace>/<sha256>.json` under the current working directory.
- `ProjectCache::discover`, `get`, `put`, `artifact_path`; `--no-cache` bypasses reads and writes.
- Cache keys are SHA-256 over stable request parts, never process-random hashes.
- `.ascii-forge/` is ignored by git.
- Cache tests cover round-trip, missing entries, stable keys and artifact paths.

### S1 — `ScenePlan` schema and deterministic compiler (issue #13)

- `ScenePlan` fields: `title`, optional `tagline`, `template`, `aspect`, `palette`, `font`, `layout`, `beats`, `seed`.
- `ScenePlan::validate` enforces bounded text, palette size, known font, 1..=8 beats, beat frame budget and seed.
- `compile_plan(&ScenePlan) -> Result<Scene, CompileError>` produces a valid scene; the model never emits raw keyframes, paths, commands or pixels.
- Layouts: centered, stacked, lower-third, full-bleed. Aspects: landscape 1920×1080, square 1080×1080, portrait 1080×1920.
- Beats map to bounded effects (typewriter, fade, slide, glitch, scanline, wave, dissolve, blink, none), targets (title, tagline, canvas) and frame counts.
- Compilation is deterministic: same plan and seed produce identical JSON.
- Tests cover validation failures, every layout/aspect, effect mapping, determinism and a compiled scene that passes `Scene::validate`.

### S2 — Provider, retries and caching (issue #13)

- `src/ai/provider.rs` defines a `Provider` trait, `PlanRequest`, `PlanResponse`, `TokenUsage` and a mock provider for offline tests.
- `OpenAiProvider` uses `ureq` against an OpenAI-compatible `/chat/completions` endpoint, sends the generated JSON Schema and never lets the model choose paths or commands.
- API key precedence: `ASCII_FORGE_API_KEY`, then `OPENAI_API_KEY`; missing key is a clear error.
- Request `response_format` uses `json_schema`; on a provider rejection, fall back to `json_object` once. Parse fenced JSON defensively.
- Generation retries at most twice with the validation/parse errors in the repair prompt; after that it fails with the collected errors.
- Final successful plans and token usage are cached under `.ascii-forge/cache/ai/<sha256>.json`; `--no-cache` bypasses.
- Token usage is logged to stderr when present and included in `--json` output.
- Mock-provider tests cover success, invalid JSON repair, two-retry exhaustion, fallback parsing and cache hit/miss.

### S3 — Offline templates (issue #14)

- `src/templates/` exposes `logo`, `promo`, `glitch`, `matrix` and `lower_third`.
- `generate --template <name> --title <text> [--tagline <text>] [--seed N]` compiles directly to a scene without network access or an API key.
- Template parameters are validated; title is required, tagline optional, seed defaults to 0.
- Snapshot tests cover each template; README documents parameters and output.
- Templates use the same compiler as AI plans.

### S4 — Vision preview loop (issue #15)

- `--vision` is opt-in. It requires a configured vision model (`--vision-model` or `[ai].vision_model`); it never sends images otherwise.
- Render first, middle and last frames to low-resolution PNG, send one bounded critique request, allow at most one revised `ScenePlan`.
- A render, network, parse or validation failure keeps the original scene and reports a warning; it never blocks generation.
- A mock vision provider tests the revision path, the single-revision bound and the fallback to the original.
- Cached critiques use the same project cache; `--no-cache` bypasses.
- README documents cost, privacy and opt-in behavior.

### S5 — CLI and documentation

- `generate` supports prompt mode, template mode, `--title`, `--tagline`, `--seed`, `--provider`, `--model`, `--base-url`, `--vision`, `--vision-model`, `--no-cache`, `--out` and `--json`.
- `generate` without a template requires a prompt; template mode does not call the network.
- JSON output reports the scene path, plan source (AI/template), template, frames, seed and token usage when available.
- README documents the provider contract, environment variables, cache layout, templates, vision opt-in and offline mode.
- CI is green on `main`: [run 43921561793](https://github.com/artur282/ascii-forge/actions/runs/43921561793).

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S0 | inline (parent) | done | `src/cache.rs` and `tests/cache.rs`; 6 tests pass; `.ascii-forge/` already gitignored. |
| T2 | S1, S3 | delegated direct (worker) | done | 24 plan/compiler tests + 14 template tests + 5 snapshots; all pass. |
| T3 | S2 | delegated direct (worker) | done | Provider/cache implemented; 19 offline tests pass. |
| T4 | S4 | delegated direct (worker) | done | Vision loop implemented; 9 offline tests pass. |
| T5 | S1-S5 | inline (parent) | done | `src/ai/generate.rs`, CLI `generate`, provider and vision wiring; 10 generation tests + 13 CLI tests pass. |
| T6 | S5 | inline (parent) | done | README, ROADMAP and this feature document updated. |
| T7 | all | inline (parent) | done | fmt/clippy clean; 269 tests pass; release build; template PNG, prompt/vision mock e2e and cache hit verified. |

## Log

### L1 — User authorization (verbatim)

> ok termina los pendientes y continua con la fase 4

Previous standing authorization:

> si procede con el desarrollo (si puedes desplegar varios agentes para agilizar el desarrollo mucho mejor)

### L2 — Planning notes (2026-11-16)

- At planning time, pending before Phase 4 were the `.ascii-forge/` cache layout and the first GitHub CI run. The user later authorized commits/push; cache and CI are now complete.
- Phase 4 maps to issues #13 (LLM ScenePlan), #14 (templates/offline) and #15 (vision loop).
- The parent owns `Cargo.toml`, `src/lib.rs`, `src/ai/mod.rs`, `src/templates/mod.rs`, `src/cli/mod.rs`, README/ROADMAP and final verification.
- Workers own disjoint surfaces: plan/compiler/templates, provider/cache, vision.
- New dependencies: `ureq` for the OpenAI-compatible HTTP client, `sha2` for stable cache keys and `base64` for vision image parts.
- The model never emits raw keyframes, commands, paths or pixels; `ScenePlan` is bounded and compiled by Rust.
- No commits or pushes without explicit user authorization.

### L3 — Verification (2026-11-16)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 269 passed, 0 failed, 0 ignored.
- `cargo build --release`: success.
- Manual template smoke: `generate --template promo --title NEBULA --tagline "navigate the noise" --seed 7` produced 90 frames, 3 effect ranges and a valid 1920×1080 PNG.
- Local OpenAI-compatible mock server: prompt mode returned a plan and usage (11/22/33 tokens); `--vision` replaced the tagline after exactly one critique; a second run with the same key reported `cached: true`, `attempts: 0` and did not call the provider.
- Cache path observed: `.ascii-forge/cache/ai/<sha256>.json`.
- Release binary smoke: `generate --template matrix --title WAKE --seed 42` produced a 60-frame scene and a 1080×1920 PNG; prompt mode also worked outside the repository.
- CI is green on `main`; latest run after the vendored-figlet security fix: [43923397363](https://github.com/artur282/ascii-forge/actions/runs/43923397363) (`f0d836b`).
