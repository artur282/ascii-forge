# Agent-first CLI

Objective: let a text-only model produce image files with one CLI call, and
discover the whole contract without reading the README.

Branch: `main` (working tree; the user owns commits).
Delivery strategy: `ask-on-risk`.
Test runner: `cargo test`.

## Specs

### S1 — `ascii-forge make` one-shot image

`ascii-forge make <TITLE> [--template K] [--tagline TEXT] [--seed N]
[--frame N] [--out FILE] [--format png|jpeg|svg] [--json]` builds an offline
template plan, compiles it, validates it and renders one static artifact in a
single invocation. `--template` defaults to `logo`; `--seed` defaults to `0`.
The default frame is the settled last frame (`frames - 1`), never frame 0.

`--json` prints exactly one JSON object:

```json
{"output":"hero.png","format":"png","width":1920,"height":1080,
 "template":"logo","title":"Nebula","tagline":null,"seed":0,
 "frame":59,"frames":60}
```

Human output: `wrote hero.png (1280x720, logo, frame 59/60)`.

### S2 — `make` defaults and errors

- Default `--out` is `slug(title)` plus the format extension (`png`, `jpg`,
  `svg`).
- `slug(title)` lowercases ASCII, transliterates common Latin accents
  (`á é í ó ú ü ñ ç` -> `a e i o u u n c`), turns every run of other characters
  into one `-`, trims leading/trailing `-`, caps at 48 chars, and falls back to
  `image` when empty.
- A blank title fails with `title must not be empty`.
- `--format` accepts only `png`, `jpeg`, `svg`; anything else fails with
  `unknown format` listing the accepted values.
- Parent directories of `--out` are created.

### S3 — `ascii-forge capabilities [--json]`

Prints a stable, machine-readable catalog for agents. `--json` returns:

```json
{"name":"ascii-forge","version":"0.1.0",
 "commands":[{"name":"make","summary":"..."}],
 "formats":{"static":["png","jpeg","svg","ansi","text"],
            "animation":["gif"],"video":["mp4","webm"]},
 "templates":[{"name":"logo","aspect":"landscape","layout":"centered",
               "required":["title"],"optional":["tagline","seed"],
               "description":"..."}],
 "charsets":["ascii","blocks","detailed"],
 "effects":["typewriter","fade","slide","scanline","glitch","wave","dissolve","blink"],
 "examples":["ascii-forge make \"Shipping v1\" --template promo --json"]}
```

`mcp` appears in `commands` only when the `mcp` feature is compiled in.
Human output is a short summary of commands, templates and formats.

### S4 — Agent kit ships with the repository

- `skills/ascii-forge/SKILL.md` teaches an agent the one-shot workflow,
  discovery, guardrails and the output contract.
- README documents `make`, `capabilities` and the agent workflow.
- CHANGELOG gains an `[Unreleased]` entry.
- The release workflow copies `skills/` into the Linux/macOS tarball and the
  Windows zip.

### S5 — Verification

- `make` PNG dimensions match the JSON `width`/`height` and the PNG IHDR.
- Existing commands keep their behavior (focused test plus full suite).
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `cargo build --release` pass.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1, S2 | inline (parent) | done | `cargo test --test cli make_`: RED (unknown subcommand) then 7 green. |
| T2 | S3 | inline (parent) | done | `capabilities` unit dispatch; 2 integration tests green. |
| T3 | S4 | inline (parent) | done | `skills/ascii-forge/SKILL.md`, README sections, CHANGELOG entry. |
| T4 | S4 | inline (parent) | done | release workflow copies `skills/`; local archive simulation lists it. |
| T5 | S5 | inline (parent) | done | fmt, clippy, 353 tests, release build, manual smoke. |

## Log

### L1 — User request (verbatim)

> ok ahora quiero que hagas un desarrollo
> este proyecto surgio por la necesidad de generar imagenes sin necesidad de un modelo de ia que genere imagenes como nanobana sino que el modelo de ia que estoy usando por ejemplo aqui deepseek v4.1 flash que no tiene capacidad para generar imagenes pueda usar el cli y generar imagenes

Scope decision: "CLI agent-first completo": `make` one-shot,
`capabilities --json`, and the skill inside the repository.

### L2 — Planning notes (2026-11-18)

- Existing `generate --template` plus `render` is two steps and requires scene
  paths; `make` collapses both into one deterministic, network-free call.
- Templates are offline, so an agent needs no API key.
- `capabilities` exposes the same list as `TemplateArg`, `EffectKind`,
  `OutputFormat` and the charset presets; no duplicated runtime source of truth
  beyond static string tables.
- Work stays in the user's `main` checkout. No commits are created.

### L3 — Verification (2026-11-18)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 347 passed, 0 failed.
- `cargo build --release`: success.
- `cargo check --no-default-features`: success.
- Manual smoke with the release binary: `make "NEBULA" --template logo` wrote a
  1920×1080 PNG (87 KB) with `frame=59/60`; frame 0 rendered smaller (61 KB),
  confirming the settled-frame default; `make "Canción Ñandú" --template matrix`
  wrote `cancion-nandu.png` at 1080×1920; `capabilities --json` parsed with 10
  commands, 5 templates and 8 effects.
- Differential check: `generate --template promo` + `render --format png`
  still write the same 1920×1080 PNG; existing render tests unchanged and green.
- Release YAML parses (PyYAML) and the local archive simulation contains
  `skills/ascii-forge/SKILL.md` next to the binary and licenses.

### L4 — Independent verification (2026-11-18)

- `gentle-ai-verify` re-ran the suite against baseline `0355bab` on a scratch
  copy: S1–S5 met, no blockers, workspace unchanged.
- Verified independently: `make` JSON equals PNG IHDR dimensions for all five
  templates; the default frame is byte-identical to `render --frame 59` and
  different from frame 0; full accent set, 48-char cap and `image` fallback;
  JPEG uses `.jpg`; rejected runs write nothing; the no-default build reports
  9 commands without `mcp`; baseline `generate`+`render` output is
  byte-identical; tar and zip simulations include the skill.
- Advisory applied: `--template lower_third` is now an accepted alias, so the
  `template` value reported by `make --json` round-trips back into the CLI. The
  canonical spelling stays `lower-third`.
- Advisories left as-is: the `.gitignore` change is user-requested; an explicit
  `--out` extension that disagrees with `--format` follows existing `render`
  semantics; the PowerShell zip step was simulated with `zip` because `pwsh` is
  unavailable on this host.
- Verifier probes were added to `tests/cli.rs` (one gated on the `mcp`
  feature).
- Final state after the alias fix and the probe tests: `cargo test` 353 passed,
  0 failed; `cargo test --no-default-features --test cli` 31 passed; fmt,
  clippy and the release build are clean.

