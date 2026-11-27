#!/usr/bin/env python3
"""The download website's pictures and videos (site/media), from the island's
own frames and the promo reels (see docs/promo.md for making those first).

  python3 scripts/site/make-media.py --reels promo-videos

It writes small files: WebP pictures with see-through corners, a short
silent loop for the top of the page, and the reels at 540x960 with a
poster frame each (silent: the page never plays sound by itself).
"""

import argparse
import math
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
LAPTOP = os.path.join(ROOT, "build")
PHONE = os.path.join(ROOT, "android", "app", "build", "promo")
BG = (11, 11, 18)  # the page's background, behind the looping video

# (output, frames folder, frame number)
PICTURES = [
    ("music", os.path.join(LAPTOP, "promo-laptop-music"), 285),
    ("control-centre", os.path.join(LAPTOP, "promo-laptop-styles"), 330),
    ("together", os.path.join(LAPTOP, "promo-laptop-together"), 235),
    ("languages", os.path.join(LAPTOP, "promo-laptop-languages"), 375),
    ("phone", os.path.join(PHONE, "new-phone"), 400),
    ("phone-sliders", os.path.join(PHONE, "new-phone"), 290),
    # The island small: a song's lyrics, the phone connecting, a message.
    ("pill-music", os.path.join(LAPTOP, "promo-laptop-music"), 180),
    ("pill-phone", os.path.join(LAPTOP, "promo-laptop-together"), 45),
    ("pill-message", os.path.join(LAPTOP, "promo-laptop-together"), 140),
    ("pill-copied", os.path.join(LAPTOP, "promo-laptop-together"), 395),
]
# Each reel, and the moment its poster shows (a full caption over the island).
REELS = {"styles": 12.5, "together": 10.5, "new-phone": 10.5, "languages": 7.5}


def frame(folder, i):
    return Image.open(os.path.join(folder, f"f{i:04d}.png")).convert("RGBA")


def picture(name, folder, i, out):
    im = frame(folder, i)
    box = im.getbbox()
    im = im.crop((max(0, box[0] - 8), max(0, box[1] - 8), min(im.width, box[2] + 8), min(im.height, box[3] + 8)))
    # About twice the size they show at (220-280px wide), for sharp screens.
    im.thumbnail((480 if name.startswith(("music", "control", "together", "languages", "phone")) else 760, 520), Image.LANCZOS)
    im.save(os.path.join(out, f"{name}.webp"), "WEBP", quality=88, method=6)


def small_island(out):
    """The island small, three times, on a warm lit ground: a song, the phone, a message."""
    names = ["pill-music", "pill-phone", "pill-message"]
    pills = [Image.open(os.path.join(out, f"{n}.webp")).convert("RGBA") for n in names]
    w, h = 760, 640
    fade = Image.linear_gradient("L").resize((w, h))
    img = Image.composite(Image.new("RGBA", (w, h), (246, 195, 214, 255)), Image.new("RGBA", (w, h), (251, 231, 207, 255)), fade)
    y = 92
    for p in pills:
        p.thumbnail((560, 80), Image.LANCZOS)
        img.alpha_composite(p, ((w - p.width) // 2, y))
        y += p.height + 110
    img.save(os.path.join(out, "small-island.webp"), "WEBP", quality=88, method=6)


def icon(size):
    """Islet's icon (assets/icon.svg) drawn at `size`: an island over a horizon on a blue-to-teal tile."""
    k = size / 1024
    # Blue at the top left to teal at the bottom right.
    big = Image.linear_gradient("L").resize((size * 2, size * 2)).rotate(45, resample=Image.BICUBIC)
    fade = big.crop((size // 2, size // 2, size // 2 + size, size // 2 + size))
    tile = Image.composite(Image.new("RGBA", (size, size), (18, 181, 166, 255)), Image.new("RGBA", (size, size), (46, 58, 242, 255)), fade)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([64 * k, 64 * k, 960 * k, 960 * k], 220 * k, fill=255)
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    img.paste(tile, (0, 0), mask)
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([262 * k, 300 * k, 762 * k, 450 * k], 75 * k, fill=(0, 0, 0, 255))
    d.ellipse([653 * k, 341 * k, 721 * k, 409 * k], fill=(18, 181, 166, 255))
    d.rounded_rectangle([312 * k, 356 * k, 512 * k, 394 * k], 19 * k, fill=(255, 255, 255, 235))
    d.arc([192 * k, 620 * k, 832 * k, 900 * k], 200, 340, fill=(255, 255, 255, 217), width=max(1, int(40 * k)))
    return img


def get_picture(out):
    """The download section's picture: the icon over the island saying a copy arrived."""
    w, h = 760, 720
    fade = Image.linear_gradient("L").resize((w, h))
    img = Image.composite(Image.new("RGBA", (w, h), (194, 216, 241, 255)), Image.new("RGBA", (w, h), (219, 234, 251, 255)), fade)
    ic = icon(240)
    img.alpha_composite(ic, ((w - 240) // 2, 150))
    pill = Image.open(os.path.join(out, "pill-copied.webp")).convert("RGBA")
    pill.thumbnail((600, 80), Image.LANCZOS)
    img.alpha_composite(pill, ((w - pill.width) // 2, 470))
    img.save(os.path.join(out, "get.webp"), "WEBP", quality=88, method=6)


def wallpaper(w, h):
    """A made-up desktop wallpaper: a dusk sky over dark hills."""
    top, mid, low = (40, 60, 140), (230, 120, 90), (250, 200, 120)
    img = Image.new("RGBA", (w, h))
    px = img.load()
    for y in range(h):
        f = y / max(1, h - 1)
        a, b, t = (top, mid, f / 0.6) if f < 0.6 else (mid, low, (f - 0.6) / 0.4)
        c = tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))
        for x in range(w):
            px[x, y] = (*c, 255)
    d = ImageDraw.Draw(img)
    hill = [(0, h), (0, h * 0.78)]
    for k in range(0, 101):
        x = w * k / 100
        hill.append((x, h * (0.78 - 0.06 * math.sin(k / 100 * math.pi * 1.6 + 0.4))))
    hill += [(w, h)]
    d.polygon(hill, fill=(30, 25, 50, 255))
    return img


def loop(out):
    """The top of the page: Control Centre opening in each layout, at the top of a desktop."""
    folder = os.path.join(LAPTOP, "promo-laptop-styles")
    files = sorted(f for f in os.listdir(folder) if f.endswith(".png"))
    # One crop for every frame: the widest the island gets, on a screen about as tall as it is wide.
    box = [10**6, 10**6, 0, 0]
    for f in files[::10]:
        b = Image.open(os.path.join(folder, f)).getbbox()
        if b:
            box = [min(box[0], b[0]), min(box[1], b[1]), max(box[2], b[2]), max(box[3], b[3])]
    w = (box[2] - box[0] + 120) // 2 * 2
    h = int(w * 1.3) // 2 * 2
    x0 = box[0] - 60
    paper = wallpaper(w, h)
    with tempfile.TemporaryDirectory() as tmp:
        for k, f in enumerate(files):
            frame = Image.open(os.path.join(folder, f)).convert("RGBA").crop((x0, 0, x0 + w, min(h, 650)))
            bg = paper.copy()
            bg.alpha_composite(frame, (0, 0))
            bg.convert("RGB").save(os.path.join(tmp, f"{k:04d}.png"))
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", "30", "-i", os.path.join(tmp, "%04d.png"),
                        "-vf", "scale=720:-2", "-c:v", "libx264", "-crf", "27", "-preset", "slow", "-pix_fmt", "yuv420p",
                        "-movflags", "+faststart", "-an", os.path.join(out, "hero.mp4")], check=True)
        Image.open(os.path.join(tmp, f"{len(files) - 60:04d}.png")).convert("RGB").resize(
            (720, round(720 * h / w))).save(os.path.join(out, "hero.jpg"), quality=84)


def reel(name, at, src, out):
    path = os.path.join(src, f"islet-{name}-silent.mp4")
    if not os.path.exists(path):
        path = os.path.join(src, f"islet-{name}.mp4")
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", path, "-vf", "scale=540:960", "-c:v", "libx264",
                    "-crf", "29", "-preset", "slow", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an",
                    os.path.join(out, f"reel-{name}.mp4")], check=True)
    # The poster: a moment from the story, with its caption.
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", str(at), "-i", path, "-frames:v", "1",
                    "-vf", "scale=540:960", "-q:v", "5", os.path.join(out, f"reel-{name}.jpg")], check=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--reels", required=True, help="the folder make-reels.py --new wrote to")
    ap.add_argument("--out", default=os.path.join(ROOT, "site", "media"))
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    for name, folder, i in PICTURES:
        picture(name, folder, i, a.out)
    small_island(a.out)
    get_picture(a.out)
    loop(a.out)
    for name, at in REELS.items():
        reel(name, at, a.reels, a.out)
    for f in sorted(os.listdir(a.out)):
        print(f"{f:28} {os.path.getsize(os.path.join(a.out, f)) / 1e6:5.2f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
