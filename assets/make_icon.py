"""Draws the Sundial tray icon.

Usage: python assets/make_icon.py assets/icon.png
Also writes preview.png (the icon at tray sizes on dark and light taskbars)
to the current directory.
"""
import math, sys
from PIL import Image, ImageDraw

S = 1024                      # supersampled canvas
BG = (26, 28, 31, 255)
RIM = (154, 160, 166, 255)
TICK = (232, 234, 237, 255)
GREEN = (74, 222, 128, 255)
c = S / 2

def pt(angle_deg, r):
    # 0 deg = 12 o'clock, clockwise
    a = math.radians(angle_deg)
    return (c + r * math.sin(a), c - r * math.cos(a))

im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)

R = S * 0.48
d.ellipse([c - R, c - R, c + R, c + R], fill=BG)
rim_w = S * 0.05
d.ellipse([c - R, c - R, c + R, c + R], outline=RIM, width=int(rim_w))

# Hour ticks at 12, 3, 6, 9.
for a in (0, 90, 180, 270):
    w = S * 0.06
    p1, p2 = pt(a, R * 0.80), pt(a, R * 0.62)
    d.line([p1, p2], fill=TICK, width=int(w))

# Gnomon shadow: a broad slice of the dial - how a sundial's shadow falls,
# and bold enough to survive a 16 px tray.
SR = R * 0.80
d.pieslice([c - SR, c - SR, c + SR, c + SR], start=-90 + 25, end=-90 + 78, fill=GREEN)
dot = S * 0.06
d.ellipse([c - dot, c - dot, c + dot, c + dot], fill=TICK)

out = sys.argv[1]
im.resize((64, 64), Image.LANCZOS).save(out)

# Preview sheet at real tray sizes on light and dark taskbars.
sizes = [16, 20, 24, 32, 64]
sheet = Image.new("RGBA", (sum(sizes) + 12 * len(sizes) + 12, 2 * 76 + 12), (0, 0, 0, 255))
x = 12
for s in sizes:
    small = im.resize((s, s), Image.LANCZOS)
    for row, bg in enumerate([(32, 32, 32, 255), (243, 243, 243, 255)]):
        tile = Image.new("RGBA", (s + 8, s + 8), bg)
        tile.alpha_composite(small, (4, 4))
        sheet.paste(tile, (x - 4, 8 + row * 76))
    x += s + 12
sheet.resize((sheet.width * 3, sheet.height * 3), Image.NEAREST).save("preview.png")
