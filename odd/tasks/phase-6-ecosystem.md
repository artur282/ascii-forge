# ASCII Forge — Phase 6: Ecosystem

Objective: expose ASCII Forge to MCP clients and ship installable release artifacts.

Branch: `main` (Phase 6 follows the shipped Phases 1-5).  
Delivery: feature branch/commits, tests, docs; push after review.  
Test runner: `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.

## Specs

### S1 — MCP stdio server (issue #16)

- `ascii-forge mcp` starts an MCP server over stdio using `rmcp`; normal CLI commands do not require MCP.
- Tools: `create_scene`, `render_preview`, `export`, `list_fonts`, `doctor`.
- `create_scene` accepts an offline `template` (logo, promo, glitch, matrix, lower_third) or a `prompt` for the AI provider, plus title/tagline/seed and an optional relative output path.
- Scenes are written under the project's `.ascii-forge/artifacts/` by default; paths reject absolute paths, parent traversal and symlink escapes.
- `create_scene` returns the scene path, origin (template/AI), frames, seed, attempts, cached and token usage when available.
- `render_preview` validates the scene and returns a bounded text or ANSI preview; it never returns an unbounded document.
- `export` validates the scene and writes PNG/JPEG/SVG/GIF/MP4/WebM/ANSI/text artifacts, returning the path and MIME type.
- `list_fonts` returns the FIGlet catalog and resolved render font; `doctor` returns the structured diagnostics.
- Invalid inputs return structured tool errors (`isError` with an error code/message), not protocol crashes.
- The server writes protocol messages only to stdout; diagnostics go to stderr.
- Tests cover tool listing, template scene creation, preview, export, invalid input and a clean stdio handshake.

### S2 — Packaging and install (issue #17)

- `install.sh` installs a release binary into `PREFIX` (default `$HOME/.local/bin`).
- Support `--build` (force `cargo build --release`), `--no-build` (use an existing binary or `ASCII_FORGE_BINARY`), and `--uninstall`.
- The script is safe for spaces, fails clearly when the binary is missing, and does not require root.
- A release workflow runs only on `v*` tags and builds Linux x86_64, macOS x86_64/aarch64 and Windows x86_64 tarballs.
- Each archive contains the binary, README and both licenses; `SHA256SUMS` is published with the release.
- Packaging docs document the FFmpeg runtime dependency, the install/uninstall flow, tag release process and supported platforms.

### S3 — Verification and docs

- MCP configuration examples for Claude Desktop, Cursor and OpenCode.
- Release/install docs in README and `packaging/`.
- `cargo fmt`, clippy `-D warnings`, full tests and release build pass.
- CI green after the Phase 6 push; issue #16 and #17 closed when green.

## Tasks

| ID | Linked specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 | S1 | delegated direct (worker) | done | `src/mcp/**`, `tests/mcp.rs`; 6 MCP tests pass. |
| T2 | S2 | delegated direct (worker) | done | `install.sh`, release workflow, packaging tests; 10 install tests pass. |
| T3 | S1 | inline (parent) | done | CLI `mcp`, `rmcp`/Tokio deps, public diagnostics helpers. |
| T4 | S3 | inline (parent) | done | README, packaging docs, MCP config examples. |
| T5 | all | inline (parent) | done | 313 tests, fmt/clippy, release build, manual MCP and release-package smoke. |

## Log

### L1 — User authorization (verbatim)

> continua con la fase 6

### L2 — Planning notes (2026-11-16)

- Phase 6 maps to #16 (MCP) and #17 (packaging); these are the last open roadmap issues.
- `rmcp = 3.10` with `transport-io`; `tokio` with `rt-multi-thread`, `macros` and `io-std`.
- The parent owns Cargo dependencies, `src/lib.rs`, the CLI `mcp` command and documentation.
- Workers own disjoint surfaces: `src/mcp/**` and `install.sh`/release workflow.
- MCP artifact paths are constrained to the project directory and `.ascii-forge/artifacts`; no remote hosting or authentication.
- FFmpeg remains an external runtime dependency; install docs must state it explicitly.

### L3 — Verification (2026-11-16)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 313 passed, 0 failed.
- `cargo build --release`: success.
- MCP: 6 protocol tests pass; manual release-binary handshake listed five tools, created a template scene, returned a bounded preview and exported a PNG.
- Installer: 10 tests pass; manual archive extraction + `install.sh --no-build` + `--version` succeeded; `SHA256SUMS` verified.
- Release workflow: PyYAML parse succeeds and the trigger is only `push.tags: v*`.
- CI remains pending because Phase 6 has not been pushed yet.
