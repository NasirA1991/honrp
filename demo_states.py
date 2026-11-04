#!/usr/bin/env python3
"""Render the states honrp reports, using the same build_activity() as the daemon.

Run:  python3 demo_states.py [out.png]

Outputs a Markdown table on stdout and a gallery PNG (default demo_states.png)
with one Discord-style activity card per scenario.  Inputs are synthetic so
every state can be shown without actually queueing / playing / spectating.
"""

import os
import sys
import time
from typing import Optional

from PIL import Image, ImageDraw, ImageFont

import honrp

HERE = os.path.dirname(os.path.abspath(__file__))
ASSET = os.path.join(HERE, "assets", "hon.png")

NOW = int(time.time())


def st(**kw) -> honrp.State:
    s = honrp.State(running=True, pid=4242)
    for k, v in kw.items():
        setattr(s, k, v)
    return s


SCENARIOS = [
    ("Main menu (home)", dict(account="evilleader", session_start=NOW - 5400)),
    ("In queue", dict(account="evilleader", in_queue=True,
                      session_start=NOW - 980)),
    ("Match found - game lobby", dict(account="evilleader", panel="game_lobby",
                                      lobby_phase=1, session_start=NOW - 1200)),
    ("Hero select (ranked, all pick)", dict(
        in_game=False, game_phase=2, local_client=0,
        account="evilleader", arranged_match=2, world="caldavar",
        lobby_mode="allpick")),
    ("In match - ranked Caldavar", dict(
        in_game=True, account="evilleader", hero="Hero_Pyromancer",
        level=12, kills=4, deaths=2, assists=7,
        arranged_match=2, map_id="caldavar", lobby_mode="banningpick",
        match_started=NOW - 754)),
    ("In match - unranked Mid Wars", dict(
        in_game=True, account="evilleader", hero="Hero_DrRepulsor",
        level=9, kills=1, deaths=3, assists=4,
        arranged_match=5, map_id="midwars", lobby_mode="allpick",
        match_started=NOW - 187)),
    ("Spectating a match", dict(
        in_game=True, account="evilleader", local_client=-1, view_client=3,
        view_hero="Hero_MasterOfArms", arranged_match=2,
        map_id="caldavar", lobby_mode="banningpick",
        match_started=NOW - 1320)),
    ("Plinko", dict(account="evilleader", panel="plinko",
                    session_start=NOW - 3600)),
    ("Store", dict(account="evilleader", panel="store",
                   session_start=NOW - 3600)),
    ("Match stats", dict(account="evilleader", panel="matchstats",
                         session_start=NOW - 3600)),
]


def activity_for(kw: dict) -> Optional[dict]:
    return honrp.build_activity(st(**kw), {"large_image": "hon",
                                           "large_text": "Heroes of Newerth"})


def elapsed(start_ms: Optional[int]) -> str:
    if not start_ms:
        return ""
    secs = max(0, NOW - start_ms // 1000)
    h, rem = divmod(secs, 3600)
    m, s = divmod(rem, 60)
    return f"{h}:{m:02d}:{s:02d} elapsed" if h else f"{m}:{s:02d} elapsed"


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    for base in ("/usr/share/fonts/TTF", "/usr/share/fonts/dejavu",
                 "/usr/share/fonts/truetype/dejavu"):
        path = os.path.join(base, name)
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def rounded(img: Image.Image, radius: int) -> Image.Image:
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, img.size[0] - 1, img.size[1] - 1], radius=radius, fill=255)
    out = Image.new("RGBA", img.size)
    out.paste(img, (0, 0), mask)
    return out


def render_gallery(out_path: str):
    card_w, card_h = 600, 150
    cell_w, cell_h = 640, 200
    cols, rows = 2, (len(SCENARIOS) + 1) // 2
    pad = 30
    W = cols * cell_w + pad * 2
    H = rows * cell_h + pad * 2 + 70

    page = Image.new("RGB", (W, H), "#111214")
    d = ImageDraw.Draw(page)
    d.text((pad, 26), "Heroes of Newerth  -  Discord Rich Presence states",
           font=font(24, True), fill="#f2f3f5")
    d.text((pad, 58), "rendered by honrp with the daemon's own build_activity()",
           font=font(13), fill="#8a8f98")

    icon = Image.open(ASSET).convert("RGBA").resize((86, 86), Image.LANCZOS)
    icon = rounded(icon, 10)

    for i, (caption, kw) in enumerate(SCENARIOS):
        act = activity_for(kw) or {}
        col, row = i % cols, i // cols
        x = pad + col * cell_w
        y = pad + 70 + row * cell_h

        d.text((x, y), caption, font=font(14, True), fill="#f0a35e")

        card = Image.new("RGBA", (card_w, card_h), (0, 0, 0, 0))
        cd = ImageDraw.Draw(card)
        cd.rounded_rectangle([0, 0, card_w - 1, card_h - 1], radius=14,
                             fill="#2b2d31", outline="#3a3c42")
        card.alpha_composite(icon, (16, 32))

        tx = 118
        cd.text((tx, 24), "Heroes of Newerth", font=font(17, True),
                fill="#f2f3f5")
        cd.text((tx, 52), act.get("details", ""), font=font(15),
                fill="#dbdee1")
        cd.text((tx, 76), act.get("state", ""), font=font(14),
                fill="#b5bac1")
        ts = (act.get("timestamps") or {}).get("start")
        if ts:
            cd.text((tx, 100), elapsed(ts), font=font(12), fill="#949ba4")
        page.paste(card, (x, y + 26), card)

    page.save(out_path)
    print(f"wrote {out_path} ({W}x{H})")


def print_table():
    rows = []
    for caption, kw in SCENARIOS:
        act = activity_for(kw) or {}
        ts = (act.get("timestamps") or {}).get("start")
        rows.append((caption, act.get("details", ""), act.get("state", ""),
                     elapsed(ts)))
    w1 = max(len(r[0]) for r in rows)
    w2 = max(len(r[1]) for r in rows)
    w3 = max(len(r[2]) for r in rows)
    print(f"{'scenario'.ljust(w1)} | {'details'.ljust(w2)} | "
          f"{'state'.ljust(w3)} | timer")
    print("-+-".join("-" * w for w in (w1, w2, w3, 15)))
    for r in rows:
        print(f"{r[0].ljust(w1)} | {r[1].ljust(w2)} | {r[2].ljust(w3)} | {r[3]}")


if __name__ == "__main__":
    print_table()
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "demo_states.png")
    render_gallery(out)
