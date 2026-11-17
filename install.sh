#!/bin/sh
#
# ASCII Forge installer.
#
# Installs the ascii-forge binary into PREFIX (default: $HOME/.local/bin).
# Unix-like systems only. No root and no sudo required.

set -eu

PROGRAM_NAME="ascii-forge"

die() {
  printf 'install.sh: %s\n' "$1" >&2
  exit 1
}

usage() {
  cat <<'EOF'
ASCII Forge installer

Usage: sh install.sh [options]

Installs the ascii-forge binary into PREFIX (default: $HOME/.local/bin) as
PREFIX/ascii-forge. The script is for Unix-like systems only; it never uses
sudo and never removes files other than the installed binary.

Options:
  --prefix DIR   Install into DIR instead of $HOME/.local/bin
  --build        Run "cargo build --release" before installing
  --no-build     Install an existing binary and never invoke cargo
  --uninstall    Remove ascii-forge from PREFIX and exit
  -h, --help     Show this help and exit

Environment:
  PREFIX               Install directory (default: $HOME/.local/bin).
                       A --prefix option takes precedence.
  ASCII_FORGE_BINARY   Binary to install. Defaults to
                       target/release/ascii-forge when present, otherwise to an
                       ascii-forge file next to this script (release archives).

Without --build or --no-build, an existing binary is installed as-is and cargo
is only invoked when no binary can be found.

FFmpeg and ffprobe are runtime dependencies for video export and video/GIF
import; a monospace font is needed for raster output; espeak-ng is optional and
only used by --voice. Run "ascii-forge doctor" to check your environment.

Examples:
  sh install.sh
  sh install.sh --prefix "$HOME/.cargo/bin"
  sh install.sh --no-build
  ASCII_FORGE_BINARY=./ascii-forge sh install.sh --no-build
  sh install.sh --uninstall
EOF
}

script_path=$0
case "$script_path" in
  */*) ;;
  *)
    if resolved_path=$(command -v "$script_path" 2>/dev/null); then
      script_path=$resolved_path
    fi
    ;;
esac
script_dir=$(CDPATH= cd "$(dirname "$script_path")" && pwd) ||
  die "cannot resolve the directory that contains install.sh"

prefix=${PREFIX:-}
mode=auto
action=install

while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix)
      if [ "$#" -lt 2 ]; then
        die "option --prefix requires a directory"
      fi
      prefix=$2
      if [ -z "$prefix" ]; then
        die "option --prefix requires a directory"
      fi
      shift
      ;;
    --prefix=*)
      prefix=${1#--prefix=}
      if [ -z "$prefix" ]; then
        die "option --prefix requires a directory"
      fi
      ;;
    --build)
      if [ "$mode" = no-build ]; then
        die "options --build and --no-build are mutually exclusive"
      fi
      mode=build
      ;;
    --no-build)
      if [ "$mode" = build ]; then
        die "options --build and --no-build are mutually exclusive"
      fi
      mode=no-build
      ;;
    --uninstall)
      action=uninstall
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    --)
      shift
      if [ "$#" -gt 0 ]; then
        die "unexpected argument '$1'; run with --help for usage"
      fi
      ;;
    -*)
      die "unknown option '$1'; run with --help for usage"
      ;;
    *)
      die "unexpected argument '$1'; run with --help for usage"
      ;;
  esac
  shift
done

if [ -z "$prefix" ]; then
  if [ -n "${HOME:-}" ]; then
    prefix="$HOME/.local/bin"
  else
    die "HOME is not set; pass --prefix DIR or set PREFIX"
  fi
fi

operating_system=$(uname -s 2>/dev/null || printf 'unknown')
case "$operating_system" in
  Linux | Darwin | *BSD | DragonFly | SunOS) ;;
  *)
    die "unsupported operating system '$operating_system'; install.sh supports Unix-like systems only (on Windows use the .zip release archive)"
    ;;
esac

find_source_binary() {
  for candidate in "$script_dir/target/release/$PROGRAM_NAME" "$script_dir/$PROGRAM_NAME"; do
    if [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

has_binary() {
  [ -n "$1" ] && [ -f "$1" ]
}

build_release() {
  if [ ! -f "$script_dir/Cargo.toml" ]; then
    die "cannot build from source: no Cargo.toml next to install.sh in '$script_dir'; extract a release archive or set ASCII_FORGE_BINARY to a prebuilt binary"
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    die "cargo was not found on PATH; install Rust from https://rustup.rs, or set ASCII_FORGE_BINARY to a prebuilt binary and rerun with --no-build"
  fi
  printf 'Building %s with cargo build --release...\n' "$PROGRAM_NAME" >&2
  (cd "$script_dir" && cargo build --release) ||
    die "cargo build --release failed; fix the build errors and rerun install.sh"
}

absolute_path() {
  absolute_parent=$(dirname "$1")
  absolute_name=$(basename "$1")
  absolute_parent=$(CDPATH= cd "$absolute_parent" && pwd) || return 1
  printf '%s/%s\n' "$absolute_parent" "$absolute_name"
}

install_binary() {
  install_source=$1
  install_prefix=$2

  if [ -e "$install_prefix" ] && [ ! -d "$install_prefix" ]; then
    die "install prefix '$install_prefix' exists and is not a directory"
  fi
  mkdir -p "$install_prefix" ||
    die "cannot create install directory '$install_prefix'"

  if [ ! -w "$install_prefix" ]; then
    die "install directory '$install_prefix' is not writable; choose a directory you own instead of using sudo"
  fi

  install_destination="$install_prefix/$PROGRAM_NAME"
  if [ -d "$install_destination" ]; then
    die "refusing to replace directory '$install_destination'; move it aside and rerun install.sh"
  fi

  install_source_absolute=$(absolute_path "$install_source") ||
    die "cannot resolve binary path '$install_source'"
  install_destination_absolute=$(absolute_path "$install_destination") ||
    die "cannot resolve install path '$install_destination'"

  if [ "$install_source_absolute" = "$install_destination_absolute" ]; then
    printf 'ASCII Forge is already installed at %s\n' "$install_destination"
  else
    install_temporary="$install_prefix/.$PROGRAM_NAME.install.$$"
    trap 'rm -f "$install_temporary"' EXIT HUP INT TERM
    cp "$install_source" "$install_temporary" ||
      die "failed to copy '$install_source' to '$install_temporary'"
    chmod 755 "$install_temporary" ||
      die "failed to mark '$install_temporary' executable"
    mv -f "$install_temporary" "$install_destination" ||
      die "failed to install '$install_destination'"
    trap - EXIT HUP INT TERM
    printf 'Installed ASCII Forge to %s\n' "$install_destination"
  fi

  printf 'FFmpeg and ffprobe are runtime dependencies for video export and video/GIF import; a monospace font is needed for raster output and espeak-ng is optional.\n'
  printf 'Run "%s doctor" to check your environment.\n' "$PROGRAM_NAME"
}

uninstall_binary() {
  uninstall_prefix=$1
  uninstall_destination="$uninstall_prefix/$PROGRAM_NAME"

  if [ -d "$uninstall_destination" ]; then
    die "refusing to remove '$uninstall_destination': it is a directory, not the ascii-forge binary"
  fi

  if [ -e "$uninstall_destination" ]; then
    rm -f "$uninstall_destination" ||
      die "failed to remove '$uninstall_destination'"
    printf 'Removed ASCII Forge from %s\n' "$uninstall_destination"
  else
    printf 'ASCII Forge is not installed at %s; nothing to remove\n' "$uninstall_destination"
  fi

  printf 'Left the install directory %s in place.\n' "$uninstall_prefix"
}

if [ "$action" = uninstall ]; then
  uninstall_binary "$prefix"
  exit 0
fi

explicit_binary=${ASCII_FORGE_BINARY:-}
source_binary=
if [ -n "$explicit_binary" ]; then
  source_binary=$explicit_binary
elif ! source_binary=$(find_source_binary); then
  source_binary=
fi

case "$mode" in
  build) build_release ;;
  no-build) ;;
  auto)
    if ! has_binary "$source_binary"; then
      build_release
    fi
    ;;
esac

if [ -z "$source_binary" ]; then
  if [ -n "$explicit_binary" ]; then
    source_binary=$explicit_binary
  elif ! source_binary=$(find_source_binary); then
    source_binary="$script_dir/target/release/$PROGRAM_NAME"
  fi
fi

if ! has_binary "$source_binary"; then
  if [ "$mode" = no-build ]; then
    die "--no-build was given, but no binary was found at '$source_binary'; run with --build to compile from source, or set ASCII_FORGE_BINARY to a prebuilt executable"
  fi
  die "no binary was found at '$source_binary'; run with --build to compile from source, or set ASCII_FORGE_BINARY to a prebuilt executable"
fi

install_binary "$source_binary" "$prefix"
