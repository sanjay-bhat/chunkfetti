"""Draws docs/banner.png, the README banner: block confetti bursting out of a browser window at dusk.
Run with any Python that has Pillow:  python scripts/banner.py"""
import math
import random
from pathlib import Path
from PIL import Image, ImageDraw

random.seed(7)
S, W, H = 4, 300, 100
im = Image.new("RGB", (W, H))
d = ImageDraw.Draw(im)

# dusk sky in flat bands, a few stars
bands = [(20, 18, 43), (30, 26, 66), (44, 36, 92), (68, 48, 118), (104, 62, 130), (150, 82, 128), (198, 108, 118),
         (232, 146, 112), (246, 190, 120)]
bh = H // len(bands) + 1
for i, c in enumerate(bands):
    d.rectangle([0, i * bh, W, (i + 1) * bh], fill=c)
for _ in range(45):
    d.point((random.randrange(W), random.randrange(40)),
            fill=(240, 236, 220) if random.random() < .7 else (180, 170, 230))

# grass on dirt
d.rectangle([0, 90, W, 92], fill=(93, 156, 58))
d.rectangle([0, 93, W, H], fill=(134, 96, 67))
for _ in range(120):
    d.point((random.randrange(W), random.randrange(93, H)),
            fill=random.choice([(110, 78, 54), (150, 110, 78), (120, 120, 120)]))

# browser window with a tiny block city inside
bx0, by0, bx1, by1 = 200, 22, 292, 88
d.rectangle([bx0 + 2, by0 + 2, bx1 + 2, by1 + 2], fill=(15, 12, 30))
d.rectangle([bx0, by0, bx1, by1], fill=(236, 232, 222))
d.rectangle([bx0, by0, bx1, by0 + 7], fill=(60, 56, 90))
for k, c in enumerate([(232, 96, 80), (240, 190, 70), (110, 190, 90)]):
    d.rectangle([bx0 + 3 + k * 5, by0 + 3, bx0 + 5 + k * 5, by0 + 5], fill=c)
d.rectangle([bx0 + 20, by0 + 2, bx1 - 4, by0 + 5], fill=(90, 86, 122))
d.rectangle([bx0 + 1, by0 + 8, bx1 - 1, by1 - 1], fill=(36, 40, 78))
x = bx0 + 3
while x < bx1 - 6:
    w, h = random.randint(5, 10), random.randint(10, 46)
    top = by1 - 1 - h
    d.rectangle([x, top, x + w, by1 - 2], fill=random.choice([(70, 66, 96), (92, 80, 70), (58, 62, 88), (110, 96, 84)]))
    for yy in range(top + 2, by1 - 3, 3):
        for xx in range(x + 1, x + w, 2):
            if random.random() < .45:
                d.point((xx, yy), fill=(255, 214, 120))
    x += w + 1

# block confetti flying out of the window
MATS = [((93, 156, 58), (134, 96, 67)), ((127, 127, 127), (100, 100, 100)), ((63, 118, 228), (40, 84, 170)),
        ((224, 193, 106), (180, 150, 70)), ((217, 111, 60), (170, 80, 40)), ((183, 211, 230), (140, 170, 196)),
        ((142, 108, 196), (108, 80, 160)), ((200, 60, 60), (150, 40, 40))]


def cube(cx, cy, s, m):
    top, side = m
    d.rectangle([cx, cy, cx + s, cy + s], fill=side)
    d.rectangle([cx, cy, cx + s, cy + max(1, s // 3)], fill=top)
    d.point((cx + s, cy), fill=(255, 255, 255))


TEXT = [(8, 17, 196, 69)]   # keep the title and tagline clear
ox, oy = bx0 + 30, by0 - 1       # the burst comes out of the top of the window
for _ in range(320):
    ang = math.radians(random.uniform(195, 345))
    r = random.random() ** 0.8 * 70
    cx = int(ox + 15 + math.cos(ang) * r * 1.9)
    cy = int(oy + math.sin(ang) * r * 0.32)   # a fountain fanning up out of the window
    if 0 < cx < W - 4 and 1 < cy < 87 and not (bx0 - 2 < cx < bx1 + 2 and by0 - 2 < cy < by1 + 2) \
            and not any(a - 4 < cx < c and b - 4 < cy < e for a, b, c, e in TEXT):
        cube(cx, cy, random.choice([1, 1, 2, 2, 3, 4]), random.choice(MATS))

# 5x7 pixel font
F = {
    'A': ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
    'B': ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
    'C': ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
    'D': ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
    'E': ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
    'F': ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
    'G': ["01110", "10001", "10000", "10111", "10001", "10001", "01110"],
    'H': ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
    'I': ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
    'K': ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
    'L': ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
    'M': ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
    'N': ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
    'O': ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
    'P': ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
    'R': ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
    'S': ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
    'T': ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
    'U': ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
    'W': ["10001", "10001", "10001", "10101", "10101", "11011", "10001"],
    'Y': ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
    ' ': ["00000"] * 7,
    '-': ["00000", "00000", "00000", "01110", "00000", "00000", "00000"],
}


def text(s, x, y, sc, col, shadow=None):
    for ch in s:
        for r, row in enumerate(F[ch]):
            for c, b in enumerate(row):
                if b == "1":
                    if shadow:
                        d.rectangle([x + c * sc + 1, y + r * sc + 1, x + c * sc + sc, y + r * sc + sc], fill=shadow)
                    d.rectangle([x + c * sc, y + r * sc, x + c * sc + sc - 1, y + r * sc + sc - 1], fill=col)
        x += 6 * sc


text("CHUNKFETTI", 10, 20, 3, (250, 246, 236), shadow=(30, 20, 50))
text("YOUR WORLD - IN ANY BROWSER", 12, 50, 1, (255, 226, 140))
text("FROM ORBIT TO A SINGLE BLOCK", 12, 60, 1, (200, 220, 255))
out = Path(__file__).resolve().parents[1] / "docs" / "banner.png"
im.resize((W * S, H * S), Image.NEAREST).save(out)
print(out)
