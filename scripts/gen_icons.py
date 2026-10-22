"""Draws the Mokora mark — a mahjong tile with an "M" on its face — and writes
every icon the app and the docs use, from one set of numbers.

    python scripts/gen_icons.py

Needs Pillow and fontTools (the wordmark's letters are read from a system
font and written out as paths, so the SVG does not depend on fonts at view
time). Output:

    assets/logo/mokora-icon.svg / .png        the tile alone
    assets/logo/mokora-logo-{light,dark}.svg / .png   tile + "Mokora"
    frontend/src/assets/logo/                 copies of the SVGs the UI imports
    frontend/public/favicon.svg
    icons/                                    the sizes tauri.conf.json lists
"""

import shutil
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent

# Tile, portrait. Units are SVG user units; the viewBox is W x H.
W, H = 280, 340
GREEN = "#1F8A5B"  # the tile's back, seen as a rim on the right and bottom
GREEN_DARK = "#17684A"
FACE = "#F6F2E7"
EDGE = "#23332C"
RED = "#D1343F"

# The "M": one closed polygon, straight strokes, flat top.
M_POLY = [
    (74, 258), (74, 80), (106, 80), (140, 168), (174, 80), (206, 80),
    (206, 258), (178, 258), (178, 140), (150, 214), (130, 214), (102, 140),
    (102, 258),
]
FACE_RECT = (22, 22, 244, 296, 38)  # x, y, w, h, corner radius
BACK_OFFSET = 14

FONT = Path("C:/Windows/Fonts/segoeuib.ttf")
WORD = "Mokora"


def tile_svg_body() -> str:
    x, y, w, h, r = FACE_RECT
    m = " ".join(f"{px},{py}" for px, py in M_POLY)
    return (
        f'<rect x="{x + BACK_OFFSET}" y="{y + BACK_OFFSET}" width="{w}" height="{h}" rx="{r}" fill="{GREEN}"/>'
        f'<rect x="{x + BACK_OFFSET}" y="{y + BACK_OFFSET}" width="{w}" height="{h}" rx="{r}" fill="none" stroke="{GREEN_DARK}" stroke-width="3"/>'
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{FACE}" stroke="{EDGE}" stroke-width="5"/>'
        f'<polygon points="{m}" fill="{RED}"/>'
    )


def svg(vb_w, vb_h, body, title):
    return (
        f'<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vb_w} {vb_h}" width="{vb_w}" height="{vb_h}" '
        f'shape-rendering="geometricPrecision"><title>{title}</title>{body}</svg>\n'
    )


# The rim pokes out BACK_OFFSET past the face, so the drawn area is a bit
# larger than the face rect.
DRAW_W = FACE_RECT[0] + FACE_RECT[2] + BACK_OFFSET + 4
DRAW_H = FACE_RECT[1] + FACE_RECT[3] + BACK_OFFSET + 4


def wordmark_path(fill: str, height: float):
    """The letters of WORD as one SVG path, baseline at y=0, cap height ~ `height`."""
    font = TTFont(FONT)
    gs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = height / font["OS/2"].sCapHeight
    x = 0.0
    parts = []
    for ch in WORD:
        name = cmap[ord(ch)]
        pen = SVGPathPen(gs)
        tp = TransformPen(pen, (scale, 0, 0, -scale, x, 0))
        gs[name].draw(tp)
        parts.append(pen.getCommands())
        x += gs[name].width * scale
    return f'<path d="{" ".join(parts)}" fill="{fill}"/>', x


def render_tile(size: int) -> Image.Image:
    """Tile on a transparent square, drawn at 4x and shrunk."""
    ss = 4
    px = size * ss
    scale = px * 0.9 / max(DRAW_W, DRAW_H)
    ox = (px - DRAW_W * scale) / 2
    oy = (px - DRAW_H * scale) / 2
    img = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def box(x, y, w, h):
        return [ox + x * scale, oy + y * scale, ox + (x + w) * scale, oy + (y + h) * scale]

    x, y, w, h, r = FACE_RECT
    rr = r * scale
    d.rounded_rectangle(box(x + BACK_OFFSET, y + BACK_OFFSET, w, h), rr, fill=GREEN, outline=GREEN_DARK, width=max(1, int(3 * scale)))
    d.rounded_rectangle(box(x, y, w, h), rr, fill=FACE, outline=EDGE, width=max(1, int(5 * scale)))
    d.polygon([(ox + px_ * scale, oy + py * scale) for px_, py in M_POLY], fill=RED)
    return img.resize((size, size), Image.LANCZOS)


def main():
    logo = ROOT / "assets" / "logo"
    ui = ROOT / "frontend" / "src" / "assets" / "logo"
    for d in (logo, ui):
        d.mkdir(parents=True, exist_ok=True)

    # --- tile alone
    icon_svg = svg(DRAW_W, DRAW_H, tile_svg_body(), "Mokora icon")
    (logo / "mokora-icon.svg").write_text(icon_svg, encoding="utf-8")

    # --- tile + wordmark, once per background
    for variant, ink in (("light", "#1A1A1A"), ("dark", "#FFFFFF")):
        word, word_w = wordmark_path(ink, 150)
        gap = 44
        total_w = DRAW_W + gap + word_w
        baseline = DRAW_H / 2 + 150 / 2
        body = (
            tile_svg_body()
            + f'<g transform="translate({DRAW_W + gap:.1f} {baseline:.1f})">{word}</g>'
        )
        text = svg(round(total_w), DRAW_H, body, f"Mokora logo {variant}")
        (logo / f"mokora-logo-{variant}.svg").write_text(text, encoding="utf-8")

    # --- PNGs for the READMEs
    render_tile(512).save(logo / "mokora-icon.png")

    # --- what the UI imports (one tile for both themes, two wordmark inkings)
    shutil.copy(logo / "mokora-icon.svg", ui / "mokora-icon.svg")
    shutil.copy(logo / "mokora-logo-light.svg", ui / "mokora-logo-light.svg")
    shutil.copy(logo / "mokora-logo-dark.svg", ui / "mokora-logo-dark.svg")
    shutil.copy(logo / "mokora-icon.svg", ROOT / "frontend" / "public" / "favicon.svg")

    # --- tauri icons
    icons = ROOT / "icons"
    big = render_tile(1024)
    big.resize((512, 512), Image.LANCZOS).save(icons / "icon.png")
    for name, size in {
        "32x32.png": 32, "64x64.png": 64, "128x128.png": 128, "128x128@2x.png": 256,
        "StoreLogo.png": 50, "Square30x30Logo.png": 30, "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71, "Square89x89Logo.png": 89, "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142, "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284, "Square310x310Logo.png": 310,
    }.items():
        big.resize((size, size), Image.LANCZOS).save(icons / name)
    big.save(icons / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    big.save(icons / "icon.icns")
    print("icons written")


if __name__ == "__main__":
    main()
