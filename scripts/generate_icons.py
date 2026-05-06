"""Generate icon.png and icon.ico for the NEXUS Tauri app from a source image.

Usage: python scripts/generate_icons.py <source_image>
"""

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS_DIR = ROOT / "nexus-tauri" / "src-tauri" / "icons"
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def square_crop(img: Image.Image) -> Image.Image:
    w, h = img.size
    if w == h:
        return img
    side = min(w, h)
    left = (w - side) // 2
    top = (h - side) // 2
    return img.crop((left, top, left + side, top + side))


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: generate_icons.py <source_image>", file=sys.stderr)
        return 2

    src = Image.open(sys.argv[1]).convert("RGBA")
    src = square_crop(src)

    ICONS_DIR.mkdir(parents=True, exist_ok=True)

    png_master = src.resize((1024, 1024), Image.LANCZOS)
    png_master.save(ICONS_DIR / "icon.png", format="PNG", optimize=True)

    ico_master = src.resize((256, 256), Image.LANCZOS)
    ico_master.save(
        ICONS_DIR / "icon.ico",
        format="ICO",
        sizes=[(s, s) for s in ICO_SIZES],
    )

    print(f"wrote {ICONS_DIR / 'icon.png'} ({png_master.size})")
    print(f"wrote {ICONS_DIR / 'icon.ico'} (sizes: {ICO_SIZES})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
