#!/usr/bin/env python3
"""Render the README state gallery as Discord-style activity cards.

The strings below mirror build_activity() in src/activity.rs (see its tests);
keep them in sync when labels change.  These are mockups of Discord's profile
activity section, not screenshots: the real client uses its own font, spacing
and app artwork.

Run:  python3 assets/render_states.py [out.png]
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ICON = os.path.join(HERE, "icon.png")

S = 2  # supersample factor
W, H = 1280, 380
COLS, ROWS = 3, 2
MARGIN, GAP_X, GAP_Y = 36, 22, 20

CARD_W = (W - 2 * MARGIN - (COLS - 1) * GAP_X) // COLS
CARD_H = 100
HEADER_H = 18
SECTION_H = HEADER_H + CARD_H

# Discord dark profile palette.
BG = (35, 36, 40)          # #232428
CARD = (43, 45, 49)        # #2b2d31
CARD_EDGE = (30, 31, 34)   # #1e1f22
HEADER = (181, 186, 193)   # #b5bac1
TITLE = (242, 243, 245)    # #f2f3f5
DETAILS = (219, 222, 225)  # #dbdee1
STATE = (181, 186, 193)    # #b5bac1

# (state label for our own table, details, state) — the first item is not drawn
# on the card; Discord only shows the details/state lines below the app name.
CARDS = [
    ("In match", "Ranked \u2022 FoC \u2022 Banning Pick",
     "Pyromancer \u2022 Lv 12 \u2022 4/2/7"),
    ("In queue", "Searching for a match", "In queue"),
    ("Hero select", "Ranked \u2022 FoC \u2022 All Pick", "Hero Select"),
    ("Spectating", "Spectating \u2022 FoC", "Watching Master Of Arms"),
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


def rounded_icon(size: int, radius: int) -> Image.Image:
    icon = Image.open(ICON).convert("RGBA").resize((size, size), Image.LANCZOS)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, size - 1, size - 1], radius=radius, fill=255
    )
    icon.putalpha(mask)
    return icon


def tracked_text(draw: ImageDraw.ImageDraw, xy: tuple[int, int], text: str,
                 f: ImageFont.FreeTypeFont, fill, tracking: int) -> None:
    x, y = xy
    for ch in text:
        draw.text((x, y), ch, font=f, fill=fill)
        x += int(draw.textlength(ch, font=f)) + tracking


def draw_section(canvas: Image.Image, x: int, y: int, details: str,
                 state: str) -> None:
    d = ImageDraw.Draw(canvas)

    # Discord labels the activity section above the rounded card.
    tracked_text(d, (x, y), "PLAYING A GAME", font("semibold", 10),
                 HEADER, 1 * S)

    card_y = y + HEADER_H * S
    d.rounded_rectangle(
        [x, card_y, x + CARD_W * S - 1, card_y + CARD_H * S - 1],
        radius=8 * S, fill=CARD, outline=CARD_EDGE, width=S,
    )

    icon_size = 64 * S
    canvas.alpha_composite(
        rounded_icon(icon_size, 5 * S),
        (x + 14 * S, card_y + (CARD_H * S - icon_size) // 2),
    )

    tx = x + 92 * S
    ty = card_y + 14 * S
    d.text((tx, ty), "Heroes of Newerth", font=font("bold", 15),
           fill=TITLE, anchor="lt")
    d.text((tx, ty + 23 * S), details, font=font("semibold", 13),
           fill=DETAILS, anchor="lt")
    d.text((tx, ty + 42 * S), state, font=font("regular", 12),
           fill=STATE, anchor="lt")


def main() -> None:
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "states-discord.png")
    canvas = Image.new("RGBA", (W * S, H * S), BG + (255,))
    top = (H * S - (ROWS * SECTION_H + (ROWS - 1) * GAP_Y) * S) // 2
    for i, (_, details, state) in enumerate(CARDS):
        col, row = i % COLS, i // COLS
        x = (MARGIN + col * (CARD_W + GAP_X)) * S
        y = top + row * (SECTION_H + GAP_Y) * S
        draw_section(canvas, x, y, details, state)
    canvas.convert("RGB").resize((W, H), Image.LANCZOS).save(out, optimize=True)
    print(f"wrote {out} ({W}x{H})")


if __name__ == "__main__":
    main()
