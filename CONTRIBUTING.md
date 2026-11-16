# Contributing

## Before you start

- Read [README.md](README.md), [ROADMAP.md](ROADMAP.md) and [docs/research.md](docs/research.md).
- Every change should map to an open issue. If there is no issue, open one first.
- Do not commit secrets, generated media or `target/`.

## Requirements

- Rust 1.99+ (edition 2024).
- FFmpeg 6+ with `libx264` and `libvpx-vp9` for video-related work.
- A monospace TTF for rendering tests (DejaVu Sans Mono is used by default).

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Media tests may require FFmpeg. If a required tool is unavailable, the test must
skip with a clear message, not silently pass.

## Issues

Use the issue templates. A feature issue must include:

- Context: what exists today and what hurts.
- Proposed behavior: observable outcomes.
- Acceptance criteria: concrete, testable items.
- Suggested implementation: optional.
- Risks and limits.
- Out of scope.

Titles use Conventional Commit prefixes: `feat:`, `fix:`, `perf:`, `refactor:`,
`docs:`, `build:`, `ci:`, `chore:`.

## Branches and commits

- Create a branch per issue: `feat/short-name`, `fix/short-name`.
- One commit is one reviewable unit. Keep tests and docs with the behavior.
- Follow Conventional Commits:

```text
feat(render): add PNG export
fix(ffmpeg): pad odd dimensions for H.264
```

- Do not rewrite shared history.
- Do not mix large refactors with features.

## Pull requests

- Link the issue with `Closes #N`.
- Include verification commands and their observed results.
- Keep the diff focused.
- Update README/docs in the same PR when behavior changes.

## Agent workflow

If an agent works on an issue:

- One issue per agent and per worktree.
- The agent must not commit or push.
- The agent delivers a patch and test evidence.
- The parent reviews the patch, resolves conflicts and verifies.
- The agent must not touch files outside the issue.

## License

By contributing, you agree that your contribution is dual-licensed under
MIT OR Apache-2.0.
