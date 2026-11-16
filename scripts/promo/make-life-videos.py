#!/usr/bin/env python3
""""Islet in real life": five vertical videos (1080x1920) that show the
island in use (a meeting, a class, a commute, the gym, a day at the desk)
with smooth camera moves, captions, an original soundtrack, then an Islet
Pro card and the GitHub link.

Frames come from the island's own drawing code (see make-videos.py for how
to draw them); the soundtrack comes from music.py, made from scratch.

  python3 scripts/promo/make-life-videos.py --out promo-videos
"""

import argparse
import importlib.util
import math
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, file))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mv = _load("make_videos", "make-videos.py")
music = _load("music", "music.py")

FPS, W, H = 30, 1080, 1920
PRO_FOR, END_FOR = 5.6, 3.8

# Islet Pro's extras (they're for the computer; the phone app is all free).
PRO_FEATURES = [
    "Glass & tinted looks",
    "A glow in your album's colour",
    "Stopwatch with laps & world clocks",
    "Quick notes & a to-do list",
    "Your own shortcut buttons",
    "Clock faces with the date or seconds",
]

LIFE = [
    dict(name="meeting", device="laptop", frames="laptop:meeting", chip="IN A MEETING", accent=(255, 69, 58),
         title="Never be late\nto a meeting", hue=((80, 110, 255), (20, 30, 90)), seed=0,
         captions=[(0.2, 2.4, "A countdown to\nyour next meeting"), (2.4, 6.0, "Join with one click"),
                   (6.8, 11.0, "On a call: a live timer"), (11.0, 13.2, "Camera & mic on?\nYou'll always know"),
                   (13.2, 17.0, "Mute from the island")],
         taps=[(2.3, "tap"), (10.9, "tap"), (13.1, "tap")],
         zoom=[(0, 1.0), (0.6, 1.25), (2.0, 1.25), (2.6, 1.08), (6.0, 1.08), (6.6, 1.28), (10.6, 1.28), (11.2, 1.08), (15.6, 1.08), (16.2, 1.28)]),
    dict(name="class", device="phone", frames="phone:life-class", chip="IN CLASS", accent=(100, 210, 255),
         title="Your school day,\nright on your camera", hue=((40, 180, 220), (30, 60, 160)), seed=1,
         captions=[(0.3, 2.6, "Your next class,\ncounting down"), (2.6, 5.4, "Room, time\n& time left"),
                   (6.0, 8.2, "Do Not Disturb\nduring lessons"), (8.2, 11.6, "Deadlines from\nyour calendar"),
                   (11.6, 16.0, "Focus rounds\n& a daily streak")],
         taps=[(2.5, "tap"), (5.3, "swipe"), (8.1, "tap"), (13.1, "tap")],
         zoom=[(0, 1.0), (0.6, 1.3), (2.4, 1.3), (3.0, 1.06), (5.4, 1.06), (6.0, 1.3), (8.0, 1.3), (8.6, 0.9), (11.4, 0.9), (12.0, 1.3), (13.0, 1.3), (13.6, 1.04)]),
    dict(name="commute", device="phone", frames="phone:life-commute", chip="ON THE WAY", accent=(50, 215, 75),
         title="Eyes on the road,\nnot on your phone", hue=((50, 200, 120), (20, 90, 140)), seed=2,
         captions=[(0.5, 2.4, "Directions at a glance"), (2.4, 5.0, "The next turn,\none tap away"),
                   (5.6, 7.6, "Music joins in"), (7.6, 10.6, "Answer calls\nright from the island"),
                   (10.6, 16.0, "All at once:\nswipe to switch")],
         taps=[(2.3, "tap"), (4.9, "swipe"), (10.5, "tap"), (13.1, "swipe-side")],
         zoom=[(0, 1.0), (0.6, 1.3), (2.2, 1.3), (2.8, 1.04), (4.9, 1.04), (5.5, 1.3), (7.4, 1.3), (8.0, 1.04), (10.5, 1.04), (11.1, 1.3)]),
    dict(name="gym", device="phone", frames="phone:life-gym", chip="AT THE GYM", accent=(34, 197, 94),
         title="Your workout,\nup top", hue=((34, 197, 94), (14, 165, 233)), seed=3,
         captions=[(0.5, 2.0, "Your playlist"), (2.0, 4.2, "Earbuds connected"), (4.2, 7.4, "Live lyrics"),
                   (8.0, 12.2, "A stopwatch with laps"), (12.8, 16.0, "And a rest timer")],
         taps=[(4.1, "tap"), (7.3, "swipe"), (9.5, "tap"), (10.9, "tap")],
         zoom=[(0, 1.0), (0.6, 1.3), (4.0, 1.3), (4.6, 1.04), (7.4, 1.04), (7.8, 1.3), (8.0, 1.3), (8.5, 1.04), (12.2, 1.04), (12.7, 1.3)]),
    dict(name="desk", device="laptop", frames="laptop:desk", chip="AT YOUR DESK", accent=(255, 159, 10),
         title="Your laptop,\nbut smarter", hue=((255, 150, 40), (120, 40, 160)), seed=4,
         captions=[(0.8, 3.2, "Hover: processor, weather,\ntime & battery"), (4.0, 7.0, "Earbuds battery,\nthe moment they connect"),
                   (7.0, 10.4, "Lyrics scroll\nthrough the island"), (10.4, 14.6, "Click for\nthe full player"),
                   (14.6, 17.0, "Windows · Mac · Linux")],
         taps=[(0.8, "hover"), (10.3, "tap")],
         zoom=[(0, 1.0), (0.6, 1.28), (10.2, 1.28), (10.8, 1.06), (14.6, 1.06), (15.2, 1.28)]),
]


def smooth(x):
    """Ease in and out (smootherstep): no jolts at either end."""
    x = max(0.0, min(1.0, x))
    return x * x * x * (x * (x * 6 - 15) + 10)


def back(x, k=1.6):
    """Ease out with a little overshoot, like the island's own spring."""
    x = max(0.0, min(1.0, x)) - 1
    return 1 + (k + 1) * x ** 3 + k * x ** 2


def keyed(keys, t):
    """A value from (time, value) keys, eased between them."""
    if t <= keys[0][0]:
        return keys[0][1]
    for (t0, v0), (t1, v1) in zip(keys, keys[1:]):
        if t <= t1:
            return v0 + (v1 - v0) * smooth((t - t0) / max(t1 - t0, 1e-6))
    return keys[-1][1]


def fade(img, a):
    if a >= 0.999:
        return img
    img = img.copy()
    img.putalpha(img.getchannel("A").point(lambda v: int(v * max(0.0, a))))
    return img


def scaled(img, s):
    if abs(s - 1) < 0.002:
        return img
    return img.resize((max(1, int(img.width * s)), max(1, int(img.height * s))), Image.BICUBIC)


def chip(text, accent):
    f = mv.font(34, "Bold")
    probe = ImageDraw.Draw(Image.new("RGBA", (10, 10)))
    w = int(probe.textlength(text, font=f)) + 92
    img = Image.new("RGBA", (w, 70), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, w - 1, 69], 35, fill=(255, 255, 255, 40), outline=(255, 255, 255, 90), width=2)
    d.ellipse([26, 24, 48, 46], fill=(*accent, 255))
    d.text((62, 35), text, font=f, fill="white", anchor="lm")
    return img


def drifting_background(c1, c2, seed):
    """A wallpaper larger than the frame, panned slowly so the background breathes."""
    return mv.wallpaper((int(W * 1.25), int(H * 1.25)), tuple(int(v * 0.42) for v in c1), tuple(int(v * 0.3) for v in c2), seed).convert("RGBA")


def device_layer(life, frames_dir):
    """The device on a transparent layer, and where its island sits."""
    reel = dict(hue=life["hue"])
    base, (sx, sy, sw) = (mv.laptop_base if life["device"] == "laptop" else mv.phone_base)(reel)
    # Keep only the device (drop the plain background those helpers paint).
    mask = Image.new("L", (W, H), 0)
    d = ImageDraw.Draw(mask)
    if life["device"] == "laptop":
        d.rounded_rectangle([30, 470, W - 30, 1238], 36, fill=255)
    else:
        d.rounded_rectangle([54, 380, W - 54, H + 200], 110, fill=255)
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    layer.paste(base, (0, 0), mask)
    first = Image.open(os.path.join(frames_dir, sorted(os.listdir(frames_dir))[0]))
    scale = sw / first.width
    focus = (sx + sw / 2, sy + (24 if life["device"] == "laptop" else 70) * scale)
    return layer, (sx, sy, sw, int(first.height * scale)), focus


def pro_panel(t, accent):
    """The Islet Pro card at time t (0 = it starts)."""
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    a = smooth(t / 0.5)
    # A glowing island, its rim cycling through album colours.
    hue = (t * 0.35) % 1.0
    import colorsys
    r, g, b = (int(v * 255) for v in colorsys.hsv_to_rgb(hue, 0.65, 1.0))
    pw, ph = int(560 * back(t / 0.7)), 112
    if pw > 10:
        pill = Image.new("RGBA", (pw + 80, ph + 80), (0, 0, 0, 0))
        pd = ImageDraw.Draw(pill)
        for k, alpha in ((24, 40), (14, 70), (6, 140)):
            pd.rounded_rectangle([40 - k, 40 - k, 40 + pw + k, 40 + ph + k], ph // 2 + k, outline=(r, g, b, alpha), width=8)
        pill = pill.filter(ImageFilter.GaussianBlur(8))
        pd = ImageDraw.Draw(pill)
        pd.rounded_rectangle([40, 40, 40 + pw, 40 + ph], ph // 2, fill=(0, 0, 0, 255), outline=(r, g, b, 255), width=4)
        if pw > 300:
            # Moving bars: one of Pro's looks.
            for i in range(5):
                hgt = 18 + 30 * (0.5 + 0.5 * math.sin(t * 9 + i * 1.3))
                x = 40 + pw - 120 + i * 18
                pd.rounded_rectangle([x, 40 + ph / 2 - hgt / 2, x + 9, 40 + ph / 2 + hgt / 2], 4, fill=(r, g, b, 255))
            pd.rounded_rectangle([70, 40 + 26, 70 + 60, 40 + 86], 14, fill=(r, g, b, 255))
        mv.put_fading(img, pill, W / 2, 330, a)
    mv.put_fading(img, mv.text_block("Islet Pro", 118, pill=False), W / 2, 520, a, rise=int(40 * (1 - back(t / 0.6))))
    mv.put_fading(img, mv.text_block("One-time purchase · for your computer", 40, pill=False), W / 2, 625, smooth((t - 0.3) / 0.5))
    # The extras, one by one, sliding in.
    for i, feat in enumerate(PRO_FEATURES):
        ti = t - 0.7 - i * 0.32
        if ti <= 0:
            continue
        row = Image.new("RGBA", (860, 104), (0, 0, 0, 0))
        rd = ImageDraw.Draw(row)
        rd.rounded_rectangle([0, 0, 859, 103], 30, fill=(255, 255, 255, 34))
        rd.ellipse([24, 26, 76, 78], fill=(*accent, 255))
        rd.line([(38, 53), (48, 63), (64, 42)], fill="white", width=7, joint="curve")
        rd.text((100, 52), feat, font=mv.font(42, "SemiBold"), fill="white", anchor="lm")
        x = W / 2 + 120 * (1 - back(ti / 0.45))
        mv.put_fading(img, row, x, 760 + i * 124, smooth(ti / 0.35))
    mv.put_fading(img, mv.text_block("The phone app is all free", 40, pill=False), W / 2, 1530, smooth((t - 3.0) / 0.5))
    return img


def github_panel(t):
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    # The mark grows out of a dot, like the island.
    grow = back(t / 0.8)
    pw = int(60 + 260 * grow)
    pill = mv.rounded((max(pw, 10), 96), 48, (0, 0, 0, 255))
    pd = ImageDraw.Draw(pill)
    pd.rounded_rectangle([1, 1, pill.width - 2, 94], 47, outline=(214, 204, 255, 255), width=3)
    if pw > 120:
        pd.ellipse([pill.width - 74, 32, pill.width - 42, 64], fill=(59, 147, 255, 255))
    mv.put_fading(img, pill, W / 2, 470, smooth(t / 0.3))
    mv.put_fading(img, mv.text_block("Islet", 140, pill=False), W / 2, 650, smooth((t - 0.25) / 0.45), rise=int(30 * (1 - back((t - 0.25) / 0.6))))
    mv.put_fading(img, mv.text_block("Download it free on GitHub", 52, pill=False), W / 2, 790, smooth((t - 0.5) / 0.45))
    link = mv.text_block("github.com/Jimchi-cmd/\nDynamic-island-for-everyone", 50)
    mv.put_fading(img, scaled(link, 0.9 + 0.1 * back((t - 0.8) / 0.6)), W / 2, 960, smooth((t - 0.8) / 0.45))
    mv.put_fading(img, mv.text_block("Android · Windows · Mac · Linux", 40, pill=False), W / 2, 1120, smooth((t - 1.2) / 0.45))
    mv.put_fading(img, mv.text_block("Link in bio", 48), W / 2, 1270, smooth((t - 1.5) / 0.45))
    return img


def make(life, frames_dir, out_path):
    files = sorted(f for f in os.listdir(frames_dir) if f.endswith(".png"))
    scene = len(files) / FPS
    total_s = scene + PRO_FOR + END_FOR
    total = int(total_s * FPS)
    bg = drifting_background(*life["hue"], life["seed"] + 20)
    device, (sx, sy, sw, sh), (fx, fy) = device_layer(life, frames_dir)
    chip_img = chip(life["chip"], life["accent"])
    title = mv.text_block(life["title"], 78, pill=False)
    # The soundtrack: built to this length, with its riser landing on the Pro card.
    wav = tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
    music.write_wav(wav, music.track(total_s, seed=life["seed"], drop=scene))
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
         "-i", wav, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p",
         "-c:a", "aac", "-b:a", "192k", "-af", "loudnorm=I=-14:TP=-1.5:LRA=11", "-shortest", "-movflags", "+faststart", out_path],
        stdin=subprocess.PIPE,
    )
    for i in range(total):
        t = i / FPS
        # The background drifts in a slow circle.
        ox = int((bg.width - W) / 2 + math.sin(t * 0.25) * (bg.width - W) / 2.2)
        oy = int((bg.height - H) / 2 + math.cos(t * 0.21) * (bg.height - H) / 2.2)
        frame = bg.crop((ox, oy, ox + W, oy + H))
        # The device and its island, eased in, with the camera pushing in on the island.
        layer = device.copy()
        isl = Image.open(os.path.join(frames_dir, files[min(i, len(files) - 1)])).convert("RGBA").resize((sw, sh), Image.BICUBIC)
        layer.alpha_composite(isl, (sx, sy))
        for at, kind in life["taps"]:
            if at <= t < scene:
                mv.touch(layer, kind, t - at, fx, fy)
        enter = smooth(t / 0.7)
        z = keyed(life["zoom"], t) * (0.92 + 0.08 * enter)
        leaving = smooth((t - scene + 0.35) / 0.6)  # into the Pro card
        z *= 1 + 0.35 * leaving
        # Keep the island where it is on screen as the camera moves in, nudged down a little.
        tx, ty = W / 2, 560 + 40 * (z - 1)
        a, e = 1 / z, 1 / z
        c = fx - tx / z
        f_ = fy - ty / z
        moved = layer.transform((W, H), Image.AFFINE, (a, 0, c, 0, e, f_), resample=Image.BICUBIC)
        frame.alpha_composite(fade(moved, enter * (1 - leaving)))
        # The context: a chip and a headline, in from the top.
        head = smooth(t / 0.5) * (1 - leaving)
        mv.put_fading(frame, chip_img, W / 2, 150 - 40 * (1 - back(t / 0.6)), head)
        mv.put_fading(frame, title, W / 2, 285 - 30 * (1 - back((t - 0.15) / 0.6)), smooth((t - 0.15) / 0.5) * (1 - leaving))
        # The caption for this moment: springs in, floats out.
        for c0, c1, text in life["captions"]:
            if c0 <= t < c1 + 0.3:
                k_in = back((t - c0) / 0.45)
                k_out = smooth((t - c1) / 0.3)
                cap = scaled(mv.text_block(text, 62), 0.86 + 0.14 * k_in)
                mv.put_fading(frame, cap, W / 2, 1520 - 50 * k_out + 30 * (1 - k_in), smooth((t - c0) / 0.25) * (1 - k_out) * (1 - leaving))
        # Islet Pro, then the link.
        if t >= scene - 0.2:
            tp = t - scene + 0.2
            shade = smooth(tp / 0.5) * 0.55
            frame.alpha_composite(Image.new("RGBA", (W, H), (6, 6, 12, int(255 * shade))))
            out_pro = smooth((t - scene - PRO_FOR + 0.3) / 0.4)
            if t < scene + PRO_FOR + 0.2:
                panel = pro_panel(tp, life["accent"])
                frame.alpha_composite(fade(scaled(panel, 1 + 0.06 * out_pro).crop((0, 0, W, H)) if out_pro > 0 else panel, 1 - out_pro))
            if t >= scene + PRO_FOR - 0.2:
                frame.alpha_composite(github_panel(t - scene - PRO_FOR + 0.2))
        ff.stdin.write(frame.convert("RGB").tobytes())
    ff.stdin.close()
    ff.wait()
    os.unlink(wav)
    print(f"wrote {out_path} ({total_s:.1f} s)")
    return ff.returncode == 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="promo-videos")
    ap.add_argument("--phone-frames", default=os.path.join(ROOT, "android/app/build/promo"))
    ap.add_argument("--laptop-frames", default=os.path.join(ROOT, "build"), help="holds promo-laptop-<reel> folders")
    ap.add_argument("--only")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    ok = True
    for life in LIFE:
        if a.only and life["name"] != a.only:
            continue
        kind, name = life["frames"].split(":")
        src = os.path.join(a.phone_frames, name) if kind == "phone" else os.path.join(a.laptop_frames, f"promo-laptop-{name}")
        ok &= make(life, src, os.path.join(a.out, f"islet-life-{life['name']}.mp4"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
