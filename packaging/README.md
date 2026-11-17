# Packaging

How ASCII Forge is installed and released. The installers are
[`install.sh`](../install.sh) for Unix-like systems and
[`install.ps1`](../install.ps1) for Windows; the release workflow is
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
| `install.ps1` | Windows installer (Windows archive only) |

```bash
mkdir ascii-forge-v0.1.0
tar -xzf ascii-forge-v0.1.0-x86_64-unknown-linux-gnu.tar.gz -C ascii-forge-v0.1.0
cd ascii-forge-v0.1.0
sh install.sh
```

The archived `install.sh` finds the binary next to itself and installs it
without Rust or Cargo. On Windows, unzip the archive and run the bundled
installer in PowerShell:

```powershell
Expand-Archive .\ascii-forge-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath ascii-forge-v0.1.0
pwsh -File .\ascii-forge-v0.1.0\install.ps1
```

Windows PowerShell 5.1 works too (`powershell -File ...`). Both installers pick
up the binary shipped next to them, so neither needs Rust or Cargo.

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

## Windows installer

`install.ps1` runs on Windows PowerShell 5.1 and PowerShell 7+. It installs
`ascii-forge.exe` into a per-user directory, requires no administrator rights,
and tolerates spaces in paths.

```powershell
pwsh -File install.ps1                    # install an existing binary, build only if needed
pwsh -File install.ps1 -Build             # force cargo build --release first
pwsh -File install.ps1 -NoBuild           # require an existing binary; never build
pwsh -File install.ps1 -Prefix "$env:USERPROFILE\bin"
pwsh -File install.ps1 -Uninstall
```

| Setting / flag | Effect |
| --- | --- |
| default prefix | `%LOCALAPPDATA%\Programs\ascii-forge\bin` |
| `PREFIX` | Install directory, used when `-Prefix` is absent. |
| `-Prefix DIR` | Install into `DIR`; takes precedence over `PREFIX` and the default. |
| `-Build` | Run `cargo build --release` in the repository root, then install. Requires Rust 1.99+. |
| `-NoBuild` | Fail with an actionable message unless an executable already exists. |
| `-Binary PATH` | Executable to install. Same as `ASCII_FORGE_BINARY`; defaults to `target\release\ascii-forge.exe` when present, otherwise to `ascii-forge.exe` next to `install.ps1` (release-archive layout). |
| `-Uninstall` | Remove `ascii-forge.exe` only. Other files and the directory itself are left alone, and a missing executable still exits 0. |
| `-Help` | Print usage. |

The executable is installed as `<prefix>\ascii-forge.exe`. Installs are
idempotent: rerunning the script replaces the executable and leaves everything
else untouched. The script refuses to replace a directory, prints the installed
path, and reminds you that FFmpeg is a runtime dependency. It never requests
elevation.

Add the install directory to your user `PATH`, then open a new terminal:

```powershell
$env:Path = "$env:LOCALAPPDATA\Programs\ascii-forge\bin;$env:Path"  # current session only
ascii-forge doctor
```

For a permanent entry, use **System Properties > Environment Variables** and
append `%LOCALAPPDATA%\Programs\ascii-forge\bin` to the user `Path` variable.

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
| macOS x86_64 | `macos-15-intel` | `x86_64-apple-darwin` | `.tar.gz` |
| macOS arm64 | `macos-15` | `aarch64-apple-darwin` | `.tar.gz` |
| Windows x86_64 | `windows-2022` | `x86_64-pc-windows-msvc` | `.zip` |

Other Unix-like systems can build from source with
`cargo build --release` or `sh install.sh --build`; `install.sh` accepts Linux,
macOS, the BSDs, DragonFly and SunOS.

## Release process

Releases are cut from tags; the workflow has no manual or branch trigger.

1. Make sure `main` is green in CI, `Cargo.toml` has the release version, and
   `Cargo.lock` is committed and in sync.
2. Create an annotated tag and push it:

   ```bash
   git tag -a v0.1.0 -m "ASCII Forge 0.1.0"
   git push origin v0.1.0
   ```

   The tag must be `v` plus the exact `version` from `Cargo.toml`. The release
   preflight strips the `v` and fails with a `::error::` annotation on any
   mismatch, before anything is built or published.
3. The `Release` workflow runs a Linux preflight (`cargo test --locked`) and
   only then builds the four targets with `cargo build --release --locked`. Each
   target is packaged into one archive (the Windows `.zip` ships both
   `install.sh` and `install.ps1`), `SHA256SUMS` is generated, and a GitHub
   Release is published on the tag with generated release notes.

Builds and tests use `--locked`. Cargo therefore reuses the committed
`Cargo.lock` verbatim and fails instead of silently updating it, so dependency
changes must update the lock file in the same commit. Tag releases are
serialized: a second tag queued while a release is running waits for it instead
of publishing concurrently (and is never cancelled).

Only tags matching `v*` start the workflow. Ordinary branch pushes and pull
requests never publish anything. CI (`ci.yml`) owns formatting, clippy, the
test suite, the `cargo deny --locked check` dependency policy, the
cross-platform `cargo check --locked` matrix and the MSRV check. The
`GITHUB_TOKEN` from the workflow is the only credential, and `contents: write`
is granted only to the publishing job.

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

This iteration ships archives, `install.sh` and `install.ps1`. There is no
AppImage, Flatpak, Snap, Homebrew formula or package-manager distribution, and
no code signing, notarization or MSI installer. Those can be added later without
changing the archive layout or the tag flow.
