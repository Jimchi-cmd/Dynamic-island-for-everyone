#!/usr/bin/env python3
"""Make Islet's promo videos: vertical 1080x1920 clips for TikTok,
Instagram Reels and YouTube Shorts, and a 1920x1080 install guide.

The island in every clip is drawn by the island's own code, frame by
frame, with its real springs and timings:

  # phone frames (android/app/build/promo/<reel>/f0000.png ...)
  cd android && ISLET_PROMO=1 gradle :app:testDebugUnitTest --tests '*PromoFrames*' --rerun
  # laptop frames
  ISLET_PROMO_DIR=build/promo-laptop cargo test -p islet --bin islet promo_frames
  # then lay them out with captions and encode them (needs Pillow and ffmpeg)
  python3 scripts/promo/make-videos.py --out promo-videos

The videos are silent on purpose: add a sound in TikTok, Instagram or
YouTube when you post, from their own licensed libraries.
"""

import argparse
import math
import os
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

FPS = 30
W, H = 1080, 1920
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DOWNLOAD = "github.com/Jimchi-cmd/Dynamic-island-for-everyone"


def font(size, weight="Bold"):
    for path in (
        f"/usr/share/fonts/opentype/inter/InterDisplay-{weight}.otf",
        f"/usr/share/fonts/opentype/inter/Inter-{weight}.otf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ):
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


# Each reel: frames folder, hook (first seconds), captions (start, end, text),
# taps (time, "tap" or "hold"), wallpaper colours, and which device it shows.
REELS = [
    dict(name="01-music", hook="Your camera hole\nplays music now", hue=((224, 68, 106), (91, 42, 214)),
         captions=[(1.0, 4.0, "Any song, any app"), (4.0, 11.0, "Tap: cover, controls\n& live lyrics"), (11.0, 13.0, "Swipe up to close")],
         taps=[(3.9, "tap"), (10.9, "swipe")]),
    dict(name="02-calls", hook="Calls, but cooler", hue=((40, 160, 90), (16, 70, 60)),
         captions=[(1.0, 5.0, "Answer right from\nthe island"), (5.0, 8.0, "A green timer\nwhile you talk"), (8.0, 11.0, "Tap for the call card")],
         taps=[(4.9, "tap"), (7.9, "tap")]),
    dict(name="03-notifications", hook="Notifications\nthat pop", hue=((60, 120, 255), (120, 40, 200)),
         captions=[(1.0, 7.4, "Messages, with Reply\nright there"), (7.4, 9.6, "Payments show\nwhat you paid"), (9.6, 12.0, "Earbuds connected")],
         taps=[]),
    dict(name="04-tap-the-island", hook="Tap your camera.\nSeriously.", hue=((255, 150, 40), (200, 40, 90)),
         captions=[(1.2, 3.4, "Tap once:\nbattery & time"), (3.4, 6.6, "Tap again: your day"), (6.6, 9.6, "Calendar &\nquick timers"), (9.6, 13.0, "A timer, right on\nthe island")],
         taps=[(1.1, "tap"), (3.3, "tap"), (6.5, "tap"), (9.5, "tap")]),
    dict(name="05-control-centre", hook="Now hold it.", hue=((90, 90, 110), (30, 30, 50)),
         captions=[(1.2, 5.6, "Hold: your own\ncontrol centre"), (6.8, 9.0, "Do Not Disturb\nat a glance"), (9.0, 11.0, "Silent mode too")],
         taps=[(0.6, "hold"), (5.5, "swipe")]),
    dict(name="06-students", hook="Made for students", hue=((40, 180, 220), (30, 60, 160)),
         captions=[(0.5, 3.0, "Your next class,\ncounting down"), (3.0, 6.4, "Room, time\n& time left"), (7.6, 13.0, "Deadlines from\nyour calendar")],
         taps=[(2.9, "tap"), (7.5, "tap")]),
    dict(name="07-directions", hook="Never miss a turn", hue=((50, 200, 120), (20, 90, 140)),
         captions=[(0.8, 3.2, "Directions on\nthe island"), (3.2, 6.8, "The next turn\nat a glance"), (7.8, 11.0, "Two things at once:\nswipe to switch")],
         taps=[(3.1, "tap"), (6.7, "swipe"), (9.5, "swipe-side")]),
    dict(name="08-focus-streak", hook="Focus, but fun", hue=((255, 160, 20), (150, 60, 0)),
         captions=[(0.8, 3.0, "Focus rounds\non the island"), (3.0, 7.0, "Keep your\nstreak going"), (8.4, 11.0, "Break time!")],
         taps=[(2.9, "tap"), (6.9, "swipe")]),
    dict(name="09-quick-tour", hook="One island.\nEverything.", hue=((200, 60, 220), (40, 40, 160)),
         captions=[(0.6, 3.0, "Music"), (3.6, 5.0, "Charging"), (5.0, 6.6, "Messages"), (6.6, 8.4, "Controls"), (8.4, 12.0, "Your calendar")],
         taps=[(1.5, "tap"), (6.5, "hold"), (8.3, "tap")]),
    dict(name="10-laptop", hook="It's on your\nlaptop too", hue=((80, 110, 255), (20, 30, 90)), device="laptop",
         captions=[(1.0, 3.4, "Hover: weather,\ntime, battery"), (4.4, 7.0, "Lyrics scroll\nthrough the island"), (7.0, 11.4, "Click for the\nfull player"), (11.4, 15.0, "Windows · Mac · Linux")],
         taps=[(1.0, "hover"), (6.9, "tap")]),
]

HOOK_FOR = 2.4  # seconds the opening line stays
END_FOR = 2.2  # seconds of the closing card


def wallpaper(size, c1, c2, seed):
    """A soft gradient with blurred colour blobs (our own, nothing borrowed)."""
    w, h = size
    img = Image.new("RGB", (w // 4, h // 4))
    d = ImageDraw.Draw(img)
    for y in range(h // 4):
        f = y / (h // 4)
        d.line([(0, y), (w // 4, y)], fill=tuple(int(c1[i] * (1 - f) * 0.55 + c2[i] * f * 0.55) for i in range(3)))
    blobs = Image.new("RGBA", img.size, (0, 0, 0, 0))
    bd = ImageDraw.Draw(blobs)
    import random
    rnd = random.Random(seed)
    for k in range(5):
        r = rnd.randint(40, 110)
        x, y = rnd.randint(0, w // 4), rnd.randint(0, h // 4)
        c = c1 if k % 2 else c2
        bd.ellipse([x - r, y - r, x + r, y + r], fill=(*c, 150))
    blobs = blobs.filter(ImageFilter.GaussianBlur(40))
    img = Image.alpha_composite(img.convert("RGBA"), blobs)
    return img.resize(size, Image.BICUBIC)


def rounded(size, radius, fill):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius, fill=255)
    layer = Image.new("RGBA", size, fill)
    layer.putalpha(m)
    return layer


def status_bar(draw, x0, x1, y, scale=1.0):
    """A plain status bar: the time on the left, signal, Wi-Fi and battery on the right (drawn shapes)."""
    f = font(int(34 * scale), "SemiBold")
    draw.text((x0, y), "10:08", font=f, fill="white", anchor="lm")
    bx = x1
    draw.rounded_rectangle([bx - 56 * scale, y - 13 * scale, bx - 6 * scale, y + 13 * scale], 6 * scale, outline="white", width=int(3 * scale))
    draw.rounded_rectangle([bx - 51 * scale, y - 8 * scale, bx - 18 * scale, y + 8 * scale], 3 * scale, fill="white")
    draw.rectangle([bx - 4 * scale, y - 5 * scale, bx - 1 * scale, y + 5 * scale], fill="white")
    sx = bx - 120 * scale
    for i in range(4):
        hgt = (8 + i * 6) * scale
        draw.rectangle([sx + i * 11 * scale, y + 13 * scale - hgt, sx + i * 11 * scale + 7 * scale, y + 13 * scale], fill="white")


def phone_base(reel):
    """Background, a phone filling most of the frame, its wallpaper and some soft app tiles."""
    c1, c2 = reel["hue"]
    bg = wallpaper((W, H), tuple(int(v * 0.35) for v in c1), tuple(int(v * 0.25) for v in c2), 1).convert("RGBA")
    px0, py0, px1 = 54, 380, W - 54
    phone = rounded((px1 - px0, H - py0 + 200), 110, (14, 14, 18, 255))
    bg.alpha_composite(phone, (px0, py0))
    sx0, sy0 = px0 + 16, py0 + 16
    screen_w = px1 - px0 - 32
    screen = wallpaper((screen_w, H - sy0 + 120), c1, c2, 7)
    mask = Image.new("L", screen.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, screen.size[0] - 1, screen.size[1] - 1], 96, fill=255)
    screen.putalpha(mask)
    d = ImageDraw.Draw(screen)
    # App tiles: plain frosted squares, no real apps.
    tile, gap = 150, (screen_w - 4 * 150) // 5
    for row in range(5):
        for col in range(4):
            x = gap + col * (tile + gap)
            y = 300 + row * (tile + 70)
            d.rounded_rectangle([x, y, x + tile, y + tile], 38, fill=(255, 255, 255, 46))
    status_bar(d, 64, screen_w - 34, 62, scale=0.82)
    bg.alpha_composite(screen, (sx0, sy0))
    return bg, (sx0, sy0, screen_w)


def laptop_base(reel):
    c1, c2 = reel["hue"]
    bg = wallpaper((W, H), tuple(int(v * 0.35) for v in c1), tuple(int(v * 0.25) for v in c2), 3).convert("RGBA")
    # A laptop lid, seen from the front: wide screen, dark bezel, a hinge.
    lx0, ly0, lx1, ly1 = 30, 470, W - 30, 1200
    bg.alpha_composite(rounded((lx1 - lx0, ly1 - ly0), 36, (20, 20, 24, 255)), (lx0, ly0))
    sw, sh = lx1 - lx0 - 36, ly1 - ly0 - 44
    screen = wallpaper((sw, sh), c1, c2, 11)
    d = ImageDraw.Draw(screen)
    # A plain menu bar (the island shows the time).
    d.rectangle([0, 0, sw, 40], fill=(0, 0, 0, 90))
    # Two soft windows on the desktop.
    d.rounded_rectangle([60, 210, sw * 0.62, sh - 60], 18, fill=(255, 255, 255, 40))
    d.rounded_rectangle([sw * 0.55, 260, sw - 60, sh - 110], 18, fill=(255, 255, 255, 55))
    bg.alpha_composite(screen.convert("RGBA"), (lx0 + 18, ly0 + 18))
    base = Image.new("RGBA", (W + 120, 38), (0, 0, 0, 0))
    ImageDraw.Draw(base).rounded_rectangle([0, 0, W + 119, 37], 18, fill=(60, 60, 68, 255))
    bg.alpha_composite(base.crop((60, 0, 60 + W - 20, 38)), (10, ly1))
    return bg, (lx0 + 18, ly0 + 18, sw)


_text_cache = {}


def text_block(text, size, pad=34, pill=True):
    key = (text, size, pill)
    if key in _text_cache:
        return _text_cache[key]
    f = font(size, "ExtraBold" if size >= 70 else "Bold")
    probe = ImageDraw.Draw(Image.new("RGBA", (10, 10)))
    box = [int(math.floor(v)) if k < 2 else int(math.ceil(v)) for k, v in enumerate(
        probe.multiline_textbbox((0, 0), text, font=f, align="center", spacing=int(size * 0.18)))]
    tw, th = box[2] - box[0], box[3] - box[1]
    img = Image.new("RGBA", (tw + pad * 2, th + pad * 2), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if pill:
        d.rounded_rectangle([0, 0, img.width - 1, img.height - 1], min(48, img.height // 2), fill=(0, 0, 0, 170))
    else:
        shadow = Image.new("RGBA", img.size, (0, 0, 0, 0))
        ImageDraw.Draw(shadow).multiline_text((pad - box[0], pad - box[1] + 4), text, font=f, fill=(0, 0, 0, 200), align="center", spacing=int(size * 0.18))
        img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(8)))
        d = ImageDraw.Draw(img)
    d.multiline_text((pad - box[0], pad - box[1]), text, font=f, fill="white", align="center", spacing=int(size * 0.18))
    _text_cache[key] = img
    return img


def ease(x):
    x = max(0.0, min(1.0, x))
    return 1 - (1 - x) ** 3


def put_fading(canvas, img, cx, cy, alpha, rise=0):
    if alpha <= 0:
        return
    if alpha < 1:
        img = img.copy()
        img.putalpha(img.getchannel("A").point(lambda a: int(a * alpha)))
    canvas.alpha_composite(img, (int(cx - img.width / 2), int(cy - img.height / 2 + rise)))


def touch(canvas, kind, age, x, y):
    """A finger: a ripple for a tap, a filling ring for a hold, a moving dot for a swipe."""
    d = ImageDraw.Draw(canvas)
    if kind == "tap" and age < 0.5:
        r = 26 + 60 * ease(age / 0.5)
        a = int(200 * (1 - age / 0.5))
        d.ellipse([x - r, y - r, x + r, y + r], outline=(255, 255, 255, a), width=6)
        if age < 0.15:
            d.ellipse([x - 26, y - 26, x + 26, y + 26], fill=(255, 255, 255, 150))
    elif kind == "hold" and age < 0.9:
        d.ellipse([x - 30, y - 30, x + 30, y + 30], fill=(255, 255, 255, 120))
        d.arc([x - 48, y - 48, x + 48, y + 48], -90, -90 + 360 * min(1, age / 0.6), fill=(255, 255, 255, 230), width=7)
    elif kind in ("swipe", "swipe-side") and age < 0.45:
        f = ease(age / 0.45)
        dx, dy = (-160 * f, 0) if kind == "swipe-side" else (0, -150 * f)
        a = int(200 * (1 - f))
        d.ellipse([x + dx - 28, y + 120 + dy - 28, x + dx + 28, y + 120 + dy + 28], fill=(255, 255, 255, a))
    elif kind == "hover" and age < 1.2:
        # An arrow pointer resting on the island.
        p = [(x, y), (x, y + 46), (x + 12, y + 35), (x + 22, y + 56), (x + 30, y + 52), (x + 20, y + 32), (x + 36, y + 32)]
        d.polygon(p, fill="white", outline="black")


def end_card(canvas, f):
    """The closing card: what it is, and where to get it."""
    shade = Image.new("RGBA", canvas.size, (8, 8, 12, int(225 * ease(f * 2))))
    canvas.alpha_composite(shade)
    a = ease(f * 2.2)
    pill = rounded((300, 92), 46, (0, 0, 0, 255))
    ImageDraw.Draw(pill).ellipse([300 - 70, 30, 300 - 38, 62], fill=(59, 147, 255, 255))
    ImageDraw.Draw(pill).rounded_rectangle([1, 1, 298, 90], 45, outline=(214, 204, 255, 255), width=3)
    put_fading(canvas, pill, W / 2, 640, a)
    put_fading(canvas, text_block("Islet", 120, pill=False), W / 2, 800, a)
    put_fading(canvas, text_block("Free & open source", 54, pill=False), W / 2, 930, a)
    put_fading(canvas, text_block("Android · Windows · Mac · Linux", 42, pill=False), W / 2, 1010, a)
    put_fading(canvas, text_block("Download: link in bio", 56), W / 2, 1180, ease(f * 2.2 - 0.3))


def reel_video(reel, frames_dir, out_path):
    files = sorted(f for f in os.listdir(frames_dir) if f.endswith(".png"))
    if not files:
        print(f"no frames in {frames_dir}", file=sys.stderr)
        return False
    laptop = reel.get("device") == "laptop"
    base, (sx, sy, sw) = (laptop_base if laptop else phone_base)(reel)
    first = Image.open(os.path.join(frames_dir, files[0]))
    scale = sw / first.width
    isl_size = (sw, int(first.height * scale))
    # Where the island's middle is, for the finger.
    tap_x, tap_y = sx + sw / 2, sy + (24 if laptop else 70) * scale
    n = len(files)
    seconds = n / FPS
    hold_last = int(END_FOR * FPS)
    total = n + hold_last
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
         "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart", out_path],
        stdin=subprocess.PIPE,
    )
    hook = text_block(reel["hook"], 92, pill=False)
    for i in range(total):
        t = i / FPS
        frame = base.copy()
        isl = Image.open(os.path.join(frames_dir, files[min(i, n - 1)])).convert("RGBA").resize(isl_size, Image.BICUBIC)
        frame.alpha_composite(isl, (sx, sy))
        for at, kind in reel["taps"]:
            if at <= t:
                touch(frame, kind, t - at, tap_x, tap_y)
        # The opening line, above the phone.
        if t < HOOK_FOR + 0.4:
            a = ease(t / 0.35) * (1 - ease((t - HOOK_FOR) / 0.4))
            # Below the apps' own top bar (TikTok's tabs, Instagram's title).
            put_fading(frame, hook, W / 2, 245, a, rise=int(20 * (1 - ease(t / 0.35))))
        # The caption for this moment, in the middle of the screen.
        for c0, c1, text in reel["captions"]:
            if c0 <= t < c1 + 0.25:
                a = ease((t - c0) / 0.25) * (1 - ease((t - c1) / 0.25))
                # Above the apps' own captions and buttons at the bottom.
                put_fading(frame, text_block(text, 64), W / 2, 1360 if not laptop else 1420, a, rise=int(30 * (1 - ease((t - c0) / 0.25))))
        if t >= seconds - 0.2:
            end_card(frame, (t - (seconds - 0.2)) / END_FOR)
        ff.stdin.write(frame.convert("RGB").tobytes())
    ff.stdin.close()
    ff.wait()
    print(f"wrote {out_path} ({total / FPS:.1f} s)")
    return ff.returncode == 0


# ---- The install guide (1920x1080, for YouTube) ------------------------------------------------

GW, GH = 1920, 1080


def guide_slides(screens_png):
    """Each slide: (seconds, title, lines, picture or None)."""
    setup = None
    if screens_png and os.path.exists(screens_png):
        sheet = Image.open(screens_png).convert("RGBA")
        page_w = sheet.width // 6
        setup = sheet.crop((page_w, 0, page_w * 2, sheet.height))
    return [
        (5, "Install Islet (free)", ["An island round your phone's camera,", "and on your laptop too."], "island"),
        (6, "1. Open the download page", [DOWNLOAD, "→ Releases → the newest version"], None),
        (6, "2. On your phone", ["Download  Islet-…-android-debug.apk", "Open it, and allow \"Install unknown apps\""], None),
        (7, "3. Open Islet → Set up", ["Allow Notification access", "Turn on Round the camera", "(Accessibility → Islet island)"], setup),
        (7, "Switch greyed out?", ["Settings → Apps → Islet → ⋮", "→ Allow restricted settings, then try again"], None),
        (7, "4. On Windows", ["Download  Islet_…_x64-setup.exe and run it", "SmartScreen: More info → Run anyway"], None),
        (7, "Mac & Linux", ["Mac: the .dmg → Privacy & Security → Open Anyway", "Linux: the .deb, .rpm or .AppImage"], None),
        (5, "That's it!", ["Tap the island · hold it · swipe it", "Free & open source"], "island"),
    ]


def guide_video(out_path, screens_png, island_dir):
    base = wallpaper((GW, GH), (40, 50, 110), (90, 30, 120), 5).convert("RGBA")
    island_frames = sorted(f for f in os.listdir(island_dir) if f.endswith(".png")) if island_dir and os.path.isdir(island_dir) else []
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{GW}x{GH}", "-r", str(FPS), "-i", "-",
         "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-movflags", "+faststart", out_path],
        stdin=subprocess.PIPE,
    )
    k = 0
    for secs, title, lines, pic in guide_slides(screens_png):
        tb = text_block(title, 84, pill=False)
        lbs = [text_block(l, 46, pill=False) for l in lines]
        n = int(secs * FPS)
        for i in range(n):
            t = i / FPS
            a = ease(t / 0.4) * (1 - ease((t - (secs - 0.4)) / 0.4))
            frame = base.copy()
            has_pic = isinstance(pic, Image.Image)
            cx = GW * 0.36 if has_pic else GW / 2
            put_fading(frame, tb, cx, 330, a, rise=int(24 * (1 - ease(t / 0.4))))
            for j, lb in enumerate(lbs):
                put_fading(frame, lb, cx, 500 + j * 90, ease((t - 0.25 - j * 0.2) / 0.4) * a)
            if has_pic:
                shot = pic.resize((int(pic.width * 860 / pic.height), 860), Image.BICUBIC)
                m = Image.new("L", shot.size, 0)
                ImageDraw.Draw(m).rounded_rectangle([0, 0, shot.width - 1, shot.height - 1], 40, fill=255)
                shot.putalpha(m)
                put_fading(frame, shot, GW * 0.78, GH / 2, a)
            elif pic == "island" and island_frames:
                f = Image.open(os.path.join(island_dir, island_frames[(k + i) % len(island_frames)])).convert("RGBA")
                f = f.crop((0, 0, f.width, int(f.height * 0.55)))
                # Below the text: the crop's top is the island's top.
                put_fading(frame, f, GW / 2, 720 + f.height / 2, a)
            ff.stdin.write(frame.convert("RGB").tobytes())
        k += n
    ff.stdin.close()
    ff.wait()
    print(f"wrote {out_path}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="promo-videos")
    ap.add_argument("--phone-frames", default=os.path.join(ROOT, "android/app/build/promo"))
    ap.add_argument("--laptop-frames", default=os.path.join(ROOT, "build/promo-laptop"))
    ap.add_argument("--screens", default=os.path.join(ROOT, "android/app/build/renders/screens-dark.png"))
    ap.add_argument("--only", help="make just the reels whose name starts with this")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    ok = True
    for reel in REELS:
        if args.only and not reel["name"].startswith(args.only):
            continue
        src = args.laptop_frames if reel.get("device") == "laptop" else os.path.join(args.phone_frames, reel["name"])
        ok &= reel_video(reel, src, os.path.join(args.out, f"islet-{reel['name']}.mp4"))
    if not args.only or args.only == "guide":
        guide_video(os.path.join(args.out, "islet-install-guide.mp4"), args.screens, os.path.join(args.phone_frames, "01-music"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
