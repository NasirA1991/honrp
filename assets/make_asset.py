#!/usr/bin/env python3
"""Build the Discord Rich Presence art asset from the official client logo.

Source: heroes of newerth/resources0.jz :: ui/fe3/elements/reborn_logo.tga
        (1024x1024 RGBA, the "Heroes of Newerth: Reborn" wordmark + phoenix)
Output: hon.png (512x512, the key `hon`) and hon@1024.png
"""
import os
import sys
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "reborn_logo.tga")
S = 1024

logo = Image.open(SRC).convert("RGBA")
if logo.size != (S, S):
    logo = logo.resize((S, S), Image.LANCZOS)

# warm dark backdrop with a soft orange glow behind the phoenix
yy, xx = np.mgrid[0:S, 0:S].astype(np.float32)
cx, cy = S * 0.5, S * 0.47
r = np.sqrt((xx - cx) ** 2 + (yy - cy) ** 2) / (S * 0.72)
r = np.clip(r, 0.0, 1.0)

inner = np.array([44, 38, 35], dtype=np.float32)   # warm charcoal
outer = np.array([8, 8, 10], dtype=np.float32)     # near black
t = (r ** 1.5)[..., None]
bg = inner * (1.0 - t) + outer * t
glow = np.exp(-((r * 2.6) ** 2))[..., None] * np.array([255, 118, 26], dtype=np.float32) * 0.30
bg = np.clip(bg + glow, 0, 255)
canvas = Image.fromarray(bg.astype(np.uint8), "RGB").convert("RGBA")
canvas.alpha_composite(logo, (0, 0))

canvas.save(os.path.join(HERE, "hon@1024.png"), optimize=True)
canvas.resize((512, 512), Image.LANCZOS).save(os.path.join(HERE, "hon.png"), optimize=True)
# transparent variant, handy if you prefer Discord's own background
logo.resize((512, 512), Image.LANCZOS).save(os.path.join(HERE, "hon-transparent.png"), optimize=True)
print("wrote hon.png (512), hon@1024.png, hon-transparent.png")
