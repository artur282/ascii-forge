# Packaging

How ASCII Forge is installed and released. The installer is
[`install.sh`](../install.sh) at the repository root; the release workflow is
[`.github/workflows/release.yml`](../.github/workflows/release.yml).

## Install from a release archive

Linux and macOS archives are `.tar.gz`; the Windows archive is `.zip`. Every
archive contains:

| File | Purpose |
| --- | --- |
| `ascii-forge` (`ascii-forge.exe` on Windows) | Release binary |
| `README.md` | Usage and requirements |
| `LICENSE-MIT`, `LICENSE-APACHE` | Licenses |
| `install.sh` | Installer for Unix-like systems |

```bash
mkdir ascii-forge-v0.1.0
tar -xzf ascii-forge-v0.1.0-x86_64-unknown-linux-gnu.tar.gz -C ascii-forge-v0.1.0
cd ascii-forge-v0.1.0
sh install.sh
```

The archived `install.sh` finds the binary next to itself and installs it
without Rust or Cargo. On Windows, unzip the archive and copy
`ascii-forge.exe` to a directory on `PATH`; `install.sh` is Unix-only.

## Install script

`install.sh` is POSIX `sh`, works from any checkout or release archive, never
uses `sudo`, and tolerates spaces in paths.

```bash
sh install.sh                       # install an existing binary, build only if needed
sh install.sh --build               # force cargo build --release first
sh install.sh --no-build            # require an existing binary; never build
sh install.sh --prefix "$HOME/bin"  # custom directory
sh install.sh --uninstall           # remove the binary
```

| Setting / flag | Effect |
| --- | --- |
| `PREFIX` | Install directory. Default: `$HOME/.local/bin`. |
| `--prefix DIR` | Same as `PREFIX`, but takes precedence when both are set. |
| `--build` | Run `cargo build --release` in the repository root, then install. Requires Rust 1.99+. |
| `--no-build` | Fail with an actionable message unless a binary already exists. |
| `ASCII_FORGE_BINARY` | Binary to install. Defaults to `target/release/ascii-forge` when present, otherwise an `ascii-forge` file next to `install.sh` (release-archive layout). |
| `--uninstall` | Remove `PREFIX/ascii-forge` only. Other files and the directory itself are left alone, and a missing binary still exits 0. |
| `-h`, `--help` | Print usage. |

The binary is installed as `PREFIX/ascii-forge`. Installs are idempotent: rerunning
the script replaces the binary and leaves everything else untouched. The script
refuses to replace a directory, prints the installed path, and reminds you that
FFmpeg is a runtime dependency.

Make sure the install directory is on `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
ascii-forge doctor
```

`PREFIX` must be writable by the current user. Do not run the script with
`sudo`; pick a user-writable `PREFIX` instead.

## Verifying downloads

Releases publish `SHA256SUMS` covering every archive.

```bash
sha256sum -c SHA256SUMS                  # Linux
shasum -a 256 -c SHA256SUMS              # macOS
```

```powershell
Get-FileHash .\ascii-forge-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Compare the output with the matching line in `SHA256SUMS`.

## Supported targets

| OS | Runner | Rust target | Archive |
| --- | --- | --- | --- |
| Linux x86_64 | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| macOS x86_64 | `macos-13` | `x86_64-apple-darwin` | `.tar.gz` |
| macOS arm64 | `macos-14` | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86_64 | `windows-2022` | `x86_64-pc-windows-msvc` | `.zip` |

Other Unix-like systems can build from source with
`cargo build --release` or `sh install.sh --build`; `install.sh` accepts Linux,
macOS, the BSDs, DragonFly and SunOS.

## Release process

Releases are cut from tags; the workflow has no manual or branch trigger.

1. Make sure `main` is green in CI and `Cargo.toml` has the release version.
2. Create an annotated tag and push it:

   ```bash
   git tag -a v0.1.0 -m "ASCII Forge 0.1.0"
   git push origin v0.1.0
   ```

3. The `Release` workflow builds the four targets, packages one archive per
   target, generates `SHA256SUMS`, and publishes a GitHub Release on the tag
   with generated release notes.

Only tags matching `v*` start the workflow. Ordinary branch pushes and pull
requests never publish anything, and the release workflow does not run tests;
CI (`ci.yml`) owns formatting, clippy and the test suite. The `GITHUB_TOKEN`
from the workflow is the only credential, and `contents: write` is granted only
to the publishing job.

Releases are immutable once published; bump the version for the next tag
rather than moving an existing tag.

## Runtime dependencies

None of these are bundled in the archives.

| Dependency | Needed for | Notes |
| --- | --- | --- |
| FFmpeg 6+ with `libx264` and `libvpx-vp9` | MP4/WebM export and video/GIF import | Required at runtime; ASCII Forge invokes it directly, never through a shell. |
| `ffprobe` | Media probing, import checks, `doctor` | Ships with FFmpeg. |
| Monospace TTF font | PNG, JPEG and terminal raster output | DejaVu Sans Mono is the default; configure with `[render].font_path` or `ASCII_FORGE_FONT`. |
| `espeak-ng` | `--voice` | Optional; only used when the flag is passed. |
| API key | AI prompt mode and `--vision` | Optional; offline templates need no key and no network. |

Run `ascii-forge doctor` after installing to check FFmpeg, `libx264`,
`libvpx-vp9`, the font and the config-directory permissions. See the
[README requirements](../README.md#requirements) for details.

## Out of scope

This iteration ships archives and `install.sh` only. There is no AppImage,
Flatpak, Snap, Homebrew formula, Windows installer or package-manager
distribution, and no code signing or notarization. Those can be added later
without changing the archive layout or the tag flow.
