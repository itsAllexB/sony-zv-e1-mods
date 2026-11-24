#!/usr/bin/env python3
"""Draws the app icon and writes assets/AppIcon.icns (macOS) and assets/icon-256.png
(window icon on Windows/Linux). Needs Pillow; the .icns step needs macOS `iconutil`.

The mark is the same as the one the app paints in its hero (`paint_mark` in
src/gui.rs): a 3:2 viewfinder whose frame opens vertically, in Lucide's 24-unit grid.
"""
import math
import os
import shutil
import subprocess
import tempfile

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
S, SS = 1024, 4  # final size, supersampling
N = S * SS
BODY = (100 * SS, 100 * SS, 924 * SS, 924 * SS)  # macOS icon grid: 824 px body
ORANGE = (255, 138, 56)


def body_mask(box):
    m = Image.new("L", (N, N), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=int((box[2] - box[0]) * 0.225), fill=255)
    return m


def gradient(top, bottom):
    g = Image.new("RGB", (1, N))
    for y in range(N):
        t = y / (N - 1)
        g.putpixel((0, y), tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return g.resize((N, N))


def mark(d, cx, cy, box, w, col):
    u = box / 24

    def at(x, y):
        return (cx + (x - 12) * u, cy + (y - 12) * u)

    def cap(p):
        d.ellipse((p[0] - w / 2, p[1] - w / 2, p[0] + w / 2, p[1] + w / 2), fill=col)

    def stroke(points):
        d.line(points, fill=col, width=int(w), joint="curve")
        cap(points[0])
        cap(points[-1])

    x0, x1, y0, y1, arm = 1.5, 22.5, 5.0, 19.0, 4.5
    for x, y, dx, dy in ((x0, y0, 1, 1), (x1, y0, -1, 1), (x0, y1, 1, -1), (x1, y1, -1, -1)):
        stroke([at(x, y + dy * arm), at(x, y), at(x + dx * arm, y)])
    stroke([at(12, 8), at(12, 16)])
    for tip, back in ((8, 10.4), (16, 13.6)):
        stroke([at(9.6, back), at(12, tip), at(14.4, back)])


def draw():
    mask = body_mask(BODY)
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    shadow.putalpha(mask.point(lambda v: int(v * 0.32)))
    shadow = shadow.transform((N, N), Image.AFFINE, (1, 0, 0, 0, 1, -12 * SS)).filter(ImageFilter.GaussianBlur(18 * SS))
    img = Image.alpha_composite(img, shadow)
    layer = gradient((62, 62, 67), (22, 22, 25)).convert("RGBA")
    layer.putalpha(mask)
    img = Image.alpha_composite(img, layer)
    rim = Image.new("RGBA", (N, N), (255, 255, 255, 0))
    lower = body_mask((BODY[0], BODY[1] + 3 * SS, BODY[2], BODY[3] + 3 * SS))
    rim.putalpha(ImageChops.subtract(mask, lower).point(lambda v: int(v * 0.35)))
    img = Image.alpha_composite(img, rim)
    mark(ImageDraw.Draw(img), N / 2, N / 2, 520 * SS, 34 * SS, ORANGE)
    return img.resize((S, S), Image.LANCZOS)


def main():
    icon = draw()
    assets = os.path.join(ROOT, "assets")
    icon.resize((256, 256), Image.LANCZOS).save(os.path.join(assets, "icon-256.png"))
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = os.path.join(tmp, "AppIcon.iconset")
            os.mkdir(iconset)
            for s in (16, 32, 128, 256, 512):
                icon.resize((s, s), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}.png"))
                icon.resize((s * 2, s * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}@2x.png"))
            subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(assets, "AppIcon.icns")], check=True)


if __name__ == "__main__":
    main()
