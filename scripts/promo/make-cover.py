#!/usr/bin/env python3
"""The Islet Pro store cover (16:9) and thumbnail (square), in the reels' look.

  python3 scripts/promo/make-cover.py FRAME.png OUT_DIR

FRAME is an island frame with a transparent background, as the laptop
promo test writes them (ISLET_PROMO_DIR); the open music card looks best.
Writes islet-pro-cover.png (2560 x 1440) and islet-pro-thumbnail.png (1200 x 1200).

  python3 scripts/promo/make-cover.py --profile OUT_DIR

Writes islet-profile.png (1024 x 1024), a profile picture safe for a round crop.

  python3 scripts/promo/make-cover.py --youtube FRAME.png OUT_DIR

Writes islet-youtube-thumbnail.png (1280 x 720) for the install tutorial.
"""

import importlib.util
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, file))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


reels = _load("make_reels", "make-reels.py")
mv = reels.mv

PINK = (226, 64, 112)
VIOLET = (110, 60, 230)
DEEP = (10, 6, 22)
GREY = (190, 186, 206)


def background(w, h, light_at):
    """A deep backdrop lit from one point, in the card's pink and violet, with fine grain."""
    sw, sh = w // 4, h // 4
    y, x = np.mgrid[0:sh, 0:sw].astype(np.float32)
    lx, ly = light_at[0] * sw, light_at[1] * sh
    r = np.sqrt((x - lx) ** 2 + ((y - ly) * 1.1) ** 2) / max(sw, sh)
    light = np.clip(1.1 - r * 1.7, 0, 1) ** 2
    side = np.clip(1 - np.sqrt(((x - sw * 0.1) / sw) ** 2 + ((y - sh * 1.0) / sh) ** 2) * 1.6, 0, 1) ** 2
    img = np.zeros((sh, sw, 3), np.float32)
    for i in range(3):
        img[..., i] = DEEP[i] + (PINK[i] - DEEP[i]) * light * 0.55 + (VIOLET[i] - DEEP[i]) * side * 0.35
    bg = Image.fromarray(np.clip(img, 0, 255).astype(np.uint8)).resize((w, h), Image.BICUBIC).convert("RGBA")
    noise = np.random.default_rng(3).normal(0, 5, (h, w, 1))
    arr = np.clip(np.asarray(bg).astype(np.float32)[..., :3] + noise, 0, 255).astype(np.uint8)
    return Image.fromarray(arr).convert("RGBA")


def card(frame_path, scale):
    """The island frame, cropped to the island and scaled, over a soft glow in its colour."""
    f = Image.open(frame_path).convert("RGBA")
    f = f.crop(f.getbbox())
    f = f.resize((int(f.width * scale), int(f.height * scale)), Image.LANCZOS)
    pad = int(160 * scale)
    out = Image.new("RGBA", (f.width + 2 * pad, f.height + 2 * pad), (0, 0, 0, 0))
    glow = Image.new("RGBA", out.size, (0, 0, 0, 0))
    ImageDraw.Draw(glow).rounded_rectangle([pad, pad + 30 * scale, pad + f.width, pad + f.height + 30 * scale], int(110 * scale), fill=(*PINK, 150))
    out.alpha_composite(glow.filter(ImageFilter.GaussianBlur(70 * scale)))
    shadow = Image.new("RGBA", out.size, (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 170), (pad, int(pad + 40 * scale)), f.getchannel("A"))
    out.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(40 * scale)))
    out.alpha_composite(f, (pad, pad))
    return out


def chip(s, size):
    f = mv.font(size, "SemiBold")
    w = int(ImageDraw.Draw(Image.new("RGBA", (8, 8))).textlength(s, font=f)) + int(size * 1.6)
    h = int(size * 2.1)
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, w - 1, h - 1], h // 2, fill=(255, 255, 255, 22), outline=(255, 255, 255, 60), width=max(2, size // 16))
    d.text((w / 2, h / 2), s, font=f, fill=(240, 236, 248), anchor="mm")
    return img


def chips(items, size, max_width, gap):
    """Chips laid out in rows, left-aligned, no wider than max_width."""
    imgs = [chip(s, size) for s in items]
    rows, row, x = [], [], 0
    for im in imgs:
        if row and x + im.width > max_width:
            rows.append(row)
            row, x = [], 0
        row.append(im)
        x += im.width + gap
    rows.append(row)
    h = imgs[0].height
    out = Image.new("RGBA", (max_width, len(rows) * h + (len(rows) - 1) * gap), (0, 0, 0, 0))
    for r, row in enumerate(rows):
        x = 0
        for im in row:
            out.alpha_composite(im, (x, r * (h + gap)))
            x += im.width + gap
    return out


FEATURES = ["Glass & tinted looks", "Album-colour glow", "Music visualizer", "Stopwatch & world clocks",
            "Quick notes & to-do", "Screenshot preview", "60 fps, no limits"]


def title(size):
    """'Islet' in white over 'Pro' in the card's pink, both extruded."""
    a = reels.extruded("Islet", size, (255, 255, 255), (200, 196, 220), depth=max(8, size // 14))
    b = reels.extruded("Pro", size, (255, 140, 170), PINK, depth=max(8, size // 14))
    gap = int(size * 0.18)
    out = Image.new("RGBA", (a.width + gap + b.width, max(a.height, b.height)), (0, 0, 0, 0))
    out.alpha_composite(a, (0, 0))
    out.alpha_composite(b, (a.width + gap - int(size * 0.12), 0))
    return out


def cover(frame, path):
    W, H = 2560, 1440
    img = background(W, H, (0.74, 0.42))
    # The card on the right half; the words on the left never reach it.
    c = card(frame, 1.08)
    img.alpha_composite(c, (W - c.width + 20, (H - c.height) // 2 + 20))
    x = 150
    t = title(210)
    img.alpha_composite(t, (x - 30, 250))
    d = ImageDraw.Draw(img)
    d.multiline_text((x, 540), "The island for your laptop,\nfully unlocked.", font=mv.font(64, "Medium"), fill=GREY, spacing=18)
    img.alpha_composite(chips(FEATURES, 40, 1150, 22), (x, 760))
    d.text((x, 1200), "One payment  ·  up to 5 computers  ·  Windows, Mac & Linux", font=mv.font(44, "SemiBold"), fill=(255, 255, 255))
    d.text((x, 1270), "The free app: github.com/Jimchi-cmd/Dynamic-island-for-everyone", font=mv.font(36, "Medium"), fill=(160, 156, 180))
    img.convert("RGB").save(path, optimize=True)


def thumbnail(frame, path):
    S = 1200
    img = background(S, S, (0.5, 0.62))
    t = title(190)
    img.alpha_composite(t, ((S - t.width) // 2, 110))
    c = card(frame, 0.92)
    img.alpha_composite(c, ((S - c.width) // 2, 390))
    img.convert("RGB").save(path, optimize=True)


def profile(path):
    """A square profile picture: a black island with a pink glow over 'Islet'.

    Everything sits inside the middle circle, so a round crop cuts nothing off.
    """
    S = 1024
    img = background(S, S, (0.5, 0.45))
    pw, ph = 560, 168
    x0, y0 = (S - pw) // 2, 300
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).rounded_rectangle([x0, y0 + 24, x0 + pw, y0 + ph + 24], ph // 2, fill=(*PINK, 170))
    img.alpha_composite(glow.filter(ImageFilter.GaussianBlur(60)))
    pill = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(pill)
    d.rounded_rectangle([x0, y0, x0 + pw, y0 + ph], ph // 2, fill=(0, 0, 0, 255), outline=(255, 255, 255, 40), width=3)
    # A tiny album square, moving bars and the camera dot: the island playing music.
    a = 92
    ax, ay = x0 + 38, y0 + (ph - a) // 2
    d.rounded_rectangle([ax, ay, ax + a, ay + a], 22, fill=PINK)
    d.rounded_rectangle([ax + 20, ay + 20, ax + a - 20, ay + a - 20], 14, fill=(255, 140, 170))
    bx, cy = x0 + pw - 190, y0 + ph // 2
    for i, h in enumerate([36, 70, 48, 84, 30]):
        d.rounded_rectangle([bx + i * 24, cy - h // 2, bx + i * 24 + 12, cy + h // 2], 6, fill=(255, 140, 170))
    r = 18
    cx = x0 + pw - 52
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(22, 20, 32), outline=(60, 58, 80), width=3)
    img.alpha_composite(pill)
    t = reels.extruded("Islet", 210, (255, 255, 255), (200, 196, 220), depth=15)
    img.alpha_composite(t, ((S - t.width) // 2, 540))
    img.convert("RGB").save(path, optimize=True)


def youtube(frame, path):
    """The install tutorial's YouTube thumbnail (1280 x 720): few, big words that read when small."""
    W, H = 1280, 720
    img = background(W, H, (0.76, 0.45))
    c = card(frame, 0.56)
    img.alpha_composite(c, (W - c.width + 40, (H - c.height) // 2 + 10))
    x = 64
    d = ImageDraw.Draw(img)
    # A yellow tag, then the title, then where it runs.
    tag = "FREE · 3 MIN"
    tf = mv.font(34, "Bold")
    tw = int(d.textlength(tag, font=tf))
    d.rounded_rectangle([x, 70, x + tw + 44, 70 + 62], 18, fill=(255, 214, 10))
    d.text((x + 22, 101), tag, font=tf, fill=(10, 6, 22), anchor="lm")
    a = reels.extruded("HOW TO", 92, (255, 255, 255), (200, 196, 220), depth=8)
    b = reels.extruded("INSTALL", 128, (255, 140, 170), PINK, depth=10)
    img.alpha_composite(a, (x - 12, 160))
    img.alpha_composite(b, (x - 14, 262))
    d.text((x, 470), "Islet", font=mv.font(84, "Bold"), fill=(255, 255, 255))
    d.text((x, 590), "Android · Windows · Mac · Linux", font=mv.font(38, "SemiBold"), fill=GREY)
    img.convert("RGB").save(path, optimize=True)


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "--youtube":
        os.makedirs(sys.argv[3], exist_ok=True)
        youtube(sys.argv[2], os.path.join(sys.argv[3], "islet-youtube-thumbnail.png"))
        return 0
    if len(sys.argv) == 3 and sys.argv[1] == "--profile":
        os.makedirs(sys.argv[2], exist_ok=True)
        profile(os.path.join(sys.argv[2], "islet-profile.png"))
        return 0
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    frame, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    cover(frame, os.path.join(out, "islet-pro-cover.png"))
    thumbnail(frame, os.path.join(out, "islet-pro-thumbnail.png"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
