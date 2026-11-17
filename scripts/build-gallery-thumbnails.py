#!/usr/bin/env python3
"""Regenerate the README gallery thumbnails from the full-size still PNGs.

Each thumbnail is a 640 px wide WebP preview that lives next to its source
asset. The PNG, GIF and MP4 masters are only read, never modified. The script
is offline and deterministic: the same inputs always produce the same bytes.

Usage:
    scripts/build-gallery-thumbnails.py
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

try:
    from PIL import Image, features
except ImportError:
    sys.exit(
        "error: Pillow is required to build thumbnails "
        "(install it with: python3 -m pip install Pillow)"
    )

THUMBNAIL_WIDTH = 640
WEBP_QUALITY = 82
WEBP_METHOD = 6

REPO_ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class Thumbnail:
    source: Path
    target: Path


ASSET_STEMS = (
    "assets/logo/ascii-forge-logo",
    "assets/examples/great-wave/great-wave",
    "assets/examples/red-fuji/red-fuji",
    "assets/examples/starry-night/starry-night",
    "assets/examples/sunflowers/sunflowers",
    "assets/examples/vitruvian/vitruvian",
)


def gallery_thumbnails() -> list[Thumbnail]:
    """Return the logo and the five examples in gallery order."""
    return [
        Thumbnail(
            source=REPO_ROOT / f"{stem}.png",
            target=REPO_ROOT / f"{stem}-thumb.webp",
        )
        for stem in ASSET_STEMS
    ]


def check_sources(thumbnails: list[Thumbnail]) -> list[Path]:
    """Return the sources that are missing on disk."""
    return [
        thumbnail.source for thumbnail in thumbnails if not thumbnail.source.is_file()
    ]


def build_thumbnail(thumbnail: Thumbnail) -> tuple[int, int, int]:
    """Write one WebP thumbnail and return its width, height and byte size."""
    with Image.open(thumbnail.source) as image:
        width = THUMBNAIL_WIDTH
        height = max(1, round(image.height * width / image.width))
        preview = image.resize((width, height), Image.Resampling.LANCZOS)
        thumbnail.target.parent.mkdir(parents=True, exist_ok=True)
        preview.save(thumbnail.target, "WEBP", quality=WEBP_QUALITY, method=WEBP_METHOD)
    return width, height, thumbnail.target.stat().st_size


def relative(path: Path) -> str:
    return str(path.relative_to(REPO_ROOT))


def main() -> int:
    thumbnails = gallery_thumbnails()

    missing = check_sources(thumbnails)
    if missing:
        print("error: missing full-size source asset(s):", file=sys.stderr)
        for path in missing:
            print(f"  {relative(path)}", file=sys.stderr)
        return 1

    if not features.check("webp"):
        print(
            "error: this Pillow build has no WebP support; "
            "reinstall Pillow from a wheel that includes libwebp",
            file=sys.stderr,
        )
        return 1

    failures = 0
    total_bytes = 0
    for thumbnail in thumbnails:
        try:
            width, height, size = build_thumbnail(thumbnail)
        except OSError as error:
            print(f"error: {relative(thumbnail.source)}: {error}", file=sys.stderr)
            failures += 1
            continue
        total_bytes += size
        print(f"{relative(thumbnail.target)}  {width}x{height}  {size} bytes")

    print(f"total: {len(thumbnails) - failures} thumbnail(s), {total_bytes} bytes")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
