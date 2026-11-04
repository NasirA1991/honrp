#!/usr/bin/env python3
"""Render the README state gallery from the presence payloads honrp sends.

The strings below mirror build_activity() in src/activity.rs (see its tests);
keep them in sync when labels change.  Output is illustrative artwork, not a
screenshot of Discord itself: the real client shows the same details/state
lines next to the Heroes of Newerth app art.

Run:  python3 assets/render_states.py [out.png]
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ICON = os.path.join(HERE, "icon.png")

S = 2  # supersample factor
W, H = 1280, 580
COLS, ROWS = 2, 3
MARGIN, GAP_X, GAP_Y = 36, 28, 26

CARD_W = (W - 2 * MARGIN - GAP_X) // COLS
CARD_H = (H - 2 * MARGIN - (ROWS - 1) * GAP_Y) // ROWS

BG_TL = (27, 48, 75)
BG_BR = (20, 42, 51)
CARD = (17, 24, 36)
EDGE = (40, 54, 70)
GREEN = (61, 220, 151)
TITLE = (242, 244, 246)
DETAILS = (226, 229, 233)
STATE = (170, 175, 182)

# (pill, details, state) - in the order build_activity() produces them.
CARDS = [
    ("In match", "Ranked \u2022 FoW \u2022 Banning Pick",
     "Pyromancer \u2022 Lv 12 \u2022 4/2/7"),
    ("In queue", "Searching for a match", "In queue"),
    ("Hero select", "Ranked \u2022 FoW \u2022 All Pick", "Hero Select"),
    ("Spectating", "Spectating \u2022 FoW", "Watching Master Of Arms"),
    ("Main menu", "Main Menu", "evilleader"),
    ("Plinko", "Playing Plinko", "evilleader"),
]

FONT_CANDIDATES = {
    "bold": [
        "/usr/share/fonts/TTF/OpenSans-Bold.ttf",
        "/usr/share/fonts/truetype/open-sans/OpenSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ],
    "semibold": [
        "/usr/share/fonts/TTF/OpenSans-SemiBold.ttf",
        "/usr/share/fonts/truetype/open-sans/OpenSans-SemiBold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ],
    "regular": [
        "/usr/share/fonts/TTF/OpenSans-Regular.ttf",
        "/usr/share/fonts/truetype/open-sans/OpenSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ],
}


def font(weight: str, size: int) -> ImageFont.FreeTypeFont:
    for path in FONT_CANDIDATES[weight]:
        if os.path.exists(path):
            return ImageFont.truetype(path, size * S)
    return ImageFont.load_default(size * S)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def background() -> Image.Image:
    im = Image.new("RGB", (W * S, H * S))
    px = im.load()
    for y in range(H * S):
        ty = y / (H * S - 1)
        for x in range(W * S):
            t = (ty + x / (W * S - 1)) / 2
            px[x, y] = lerp(BG_TL, BG_BR, t)
    grid = Image.new("RGBA", im.size, (0, 0, 0, 0))
    g = ImageDraw.Draw(grid)
    step = 64 * S
    for x in range(0, W * S, step):
        g.line([(x, 0), (x, H * S)], fill=(150, 180, 210, 16), width=S)
    for y in range(0, H * S, step):
        g.line([(0, y), (W * S, y)], fill=(150, 180, 210, 16), width=S)
    return Image.alpha_composite(im.convert("RGBA"), grid)


def rounded_icon(size: int, radius: int) -> Image.Image:
    icon = Image.open(ICON).convert("RGBA").resize((size, size), Image.LANCZOS)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1],
                                           radius=radius, fill=255)
    icon.putalpha(mask)
    return icon


def draw_card(canvas: Image.Image, box: tuple[int, int], pill: str,
              details: str, state: str) -> None:
    x0, y0 = box
    w, h = CARD_W * S, CARD_H * S
    d = ImageDraw.Draw(canvas)
    d.rounded_rectangle([x0, y0, x0 + w - 1, y0 + h - 1],
                        radius=18 * S, fill=CARD, outline=EDGE, width=S)

    icon_size = 80 * S
    canvas.alpha_composite(rounded_icon(icon_size, 17 * S),
                           (x0 + 26 * S, y0 + (h - icon_size) // 2))

    tx = x0 + 124 * S
    ty = (h - 83 * S) // 2
    d.text((tx, y0 + ty), "Heroes of Newerth", font=font("bold", 20),
           fill=TITLE)
    d.text((tx, y0 + ty + 30 * S), details, font=font("semibold", 17),
           fill=DETAILS)
    d.text((tx, y0 + ty + 56 * S), state, font=font("regular", 15), fill=STATE)

    # State pill, top-right; translucent parts go through an overlay because
    # ImageDraw writes alpha verbatim instead of compositing it.
    f = font("semibold", 13)
    tw = d.textlength(pill, font=f)
    pw, ph = int(tw + 26 * S), 25 * S
    px, py = x0 + w - 24 * S - pw, y0 + 22 * S
    pill_layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(pill_layer).rounded_rectangle(
        [px, py, px + pw, py + ph], radius=ph // 2,
        fill=GREEN + (26,), outline=GREEN + (150,), width=S)
    canvas.alpha_composite(pill_layer)
    d.text((px + 13 * S, py + ph / 2), pill, font=f, fill=GREEN, anchor="lm")


def main() -> None:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "states.png")
    canvas = background()
    for i, (pill, details, state) in enumerate(CARDS):
        col, row = i % COLS, i // COLS
        x = (MARGIN + col * (CARD_W + GAP_X)) * S
        y = (MARGIN + row * (CARD_H + GAP_Y)) * S
        draw_card(canvas, (x, y), pill, details, state)
    canvas.convert("RGB").resize((W, H), Image.LANCZOS).save(out, optimize=True)
    print(f"wrote {out} ({W}x{H})")


if __name__ == "__main__":
    main()
