# Changelog

All notable user-visible changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
Semantic Versioning once it reaches 1.0.

## [0.1.0] - 2026-11-17

### Added

- Image/video/GIF import with charsets, color, braille, Floyd-Steinberg
  dithering, Sobel edges and the `Frames` scene layer.
- AI scene planning through an OpenAI-compatible provider, offline templates,
  bounded repair retries, project cache and an opt-in vision review loop.
- MCP stdio server tools: `create_scene`, `render_preview`, `export`,
  `list_fonts` and `doctor`.
- `install.sh`, a tag-only release workflow, `SHA256SUMS` and packaging docs.
- Gallery examples and the separate animated logo in `assets/`, with fast WebP
  thumbnails for README previews.
- `cache clear` for the project-local response cache.
- Windows `install.ps1`, release preflight/tag-version checks and
  `cargo-deny` dependency policy CI.
- MCP overwrite protection and provider HTTPS/loopback validation.

### Changed

- Imported videos are embedded in the scene as bounded `Frames`; scenes cap at
  2,000,000 cells.
- The scene cache lives under `.ascii-forge/cache/`; generated artifacts live
  under `.ascii-forge/artifacts/`.
- `imageproc` no longer enables its default image formats; AVIF/ravif is absent
  from the dependency graph.
- MCP is a default Cargo feature; `--no-default-features` omits `rmcp`/Tokio.

### Fixed

- Removed the vulnerable transitive `zip` dependency by vendoring the FIGlet
  implementation without Toilet/zip support.

Initial development release. The public API, scene schema and CLI flags may
change before 1.0.
