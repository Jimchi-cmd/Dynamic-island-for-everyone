#!/usr/bin/env python3
"""Islet's launch reels (1080x1920, ~29 s): the island floating big on a
deep, lit background, a 3D title, kinetic type that fades in from a blur,
moves cut to the beat of an original 170 BPM track, an "And also…" stack,
an Islet Pro card and a Download on GitHub ending.

Frames of the island come from its own drawing code (see make-videos.py);
the music is made from scratch by music.py.

  python3 scripts/promo/make-reels.py --out promo-videos [--only class]
"""

import argparse
import colorsys
import importlib.util
import math
import os
import random
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter

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
BPM = 170
BAR = 4 * 60 / BPM
# The film, in bars: build-up and title, the story, "And also…", Pro, the link.
INTRO, STORY, ALSO, PRO, END = 2, 11, 2, 3, 2
TAIL = 1.0
GREY = (196, 196, 206)


def at_bar(b):
    return b * BAR


def smooth(x):
    x = max(0.0, min(1.0, x))
    return x * x * x * (x * (x * 6 - 15) + 10)


def back(x, k=1.7):
    x = max(0.0, min(1.0, x)) - 1
    return 1 + (k + 1) * x ** 3 + k * x ** 2


def expo(x):
    x = max(0.0, min(1.0, x))
    return 1 - 2 ** (-10 * x) if x < 1 else 1.0


# ---- Type -----------------------------------------------------------------------------------


def text_img(s, size, weight="ExtraBold", fill=(255, 255, 255), tracking=-0.02):
    """Text with tight tracking, as a tight RGBA image."""
    f = mv.font(size, weight)
    probe = ImageDraw.Draw(Image.new("RGBA", (8, 8)))
    widths = [probe.textlength(ch, font=f) for ch in s]
    w = int(sum(widths) + tracking * size * max(0, len(s) - 1)) + 8
    asc, desc = f.getmetrics()
    img = Image.new("RGBA", (max(w, 8), asc + desc + 8), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    x = 4
    for ch, cw in zip(s, widths):
        d.text((x, 4), ch, font=f, fill=fill)
        x += cw + tracking * size
    # Trim the sides only: every word keeps the same line height, so they share a baseline.
    box = img.getbbox() or (0, 0, 8, 8)
    return img.crop((box[0], 0, box[2], img.height))


class Kinetic:
    """A line of text whose words fade in from a blur, one after another."""

    LEVELS = (14, 7, 3, 0)

    def __init__(self, s, size, weight="ExtraBold", fill=(255, 255, 255), gap=0.26):
        self.words = []
        space = size * gap
        imgs = [text_img(w, size, weight, fill) for w in s.split(" ")]
        h = max(i.height for i in imgs)
        x = 0
        for img in imgs:
            pad = 30
            base = Image.new("RGBA", (img.width + 2 * pad, h + 2 * pad), (0, 0, 0, 0))
            base.alpha_composite(img, (pad, pad + (h - img.height)))
            blurred = {lv: (base.filter(ImageFilter.GaussianBlur(lv)) if lv else base) for lv in self.LEVELS}
            self.words.append((x - pad, blurred))
            x += img.width + space
        self.width = int(x - space)
        self.height = h

    def draw(self, canvas, cx, cy, t, out=0.0, stagger=0.07, dur=0.42):
        """t: seconds since it started; out: 0..1 as it leaves."""
        left = cx - self.width / 2
        for i, (dx, blurred) in enumerate(self.words):
            p = smooth((t - i * stagger) / dur)
            if p <= 0:
                continue
            q = p * (1 - out)
            lv = self.LEVELS[min(3, int(q * 3.999))] if q < 0.999 else 0
            img = blurred[lv]
            if q < 0.999:
                img = img.copy()
                img.putalpha(img.getchannel("A").point(lambda v, q=q: int(v * q)))
            y = cy - self.height / 2 + 26 * (1 - p) - 18 * out
            canvas.alpha_composite(img, (int(left + dx), int(y - 30)))


def tag(s, accent):
    f = mv.font(28, "Bold")
    probe = ImageDraw.Draw(Image.new("RGBA", (8, 8)))
    w = int(probe.textlength(s, font=f)) + 44
    img = Image.new("RGBA", (w, 52), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, w - 1, 51], 26, fill=(*accent, 46), outline=(*accent, 120), width=2)
    d.text((w / 2, 26), s, font=f, fill=tuple(min(255, int(c * 0.6 + 110)) for c in accent), anchor="mm")
    return img


def extruded(s, size, top, bottom, depth=22):
    """A chunky 3D title: an extruded body, a gradient face and a soft shadow."""
    mask = text_img(s, size, "ExtraBold").getchannel("A")
    w, h = mask.size
    img = Image.new("RGBA", (w + depth + 60, h + depth + 60), (0, 0, 0, 0))
    shadow = Image.new("RGBA", img.size, (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 150), (30 + depth, 30 + depth + 10), mask)
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18)))
    for k in range(depth, 0, -1):
        shade = tuple(int(c * (0.32 + 0.25 * (1 - k / depth))) for c in bottom)
        img.paste((*shade, 255), (30 + k, 30 + k), mask)
    grad = Image.new("RGBA", (w, h))
    gd = ImageDraw.Draw(grad)
    for y in range(h):
        f = y / max(1, h - 1)
        gd.line([(0, y), (w, y)], fill=tuple(int(top[i] * (1 - f) + bottom[i] * f) for i in range(3)) + (255,))
    face = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    face.paste(grad, (0, 0), mask)
    # A thin highlight along the top of the letters.
    hl = ImageChops.subtract(mask, mask.transform(mask.size, Image.AFFINE, (1, 0, 0, 0, 1, -3)))
    face.paste((255, 255, 255, 200), (0, 0), hl)
    img.alpha_composite(face, (30, 30))
    return img


# ---- Atmosphere ------------------------------------------------------------------------------


class Atmosphere:
    """A deep, lit background: a spotlight, streaks of light, floating glass shapes and grain."""

    def __init__(self, glow, deep, seed):
        rnd = random.Random(seed)
        y, x = np.mgrid[0:H // 4, 0:W // 4].astype(np.float32)
        cx, cy = W / 8, H / 4 * 0.33
        r = np.sqrt((x - cx) ** 2 + ((y - cy) * 0.8) ** 2) / (W / 4)
        light = np.clip(1.15 - r * 1.25, 0, 1) ** 2.2
        vign = np.clip(1.0 - (np.sqrt(((x - W / 8) / (W / 8)) ** 2 + ((y - H / 8) / (H / 8)) ** 2) - 0.75) * 0.9, 0.35, 1)
        base = np.zeros((H // 4, W // 4, 3), np.float32)
        for i in range(3):
            base[..., i] = (deep[i] + (glow[i] - deep[i]) * light * 0.75) * vign
        self.bg = Image.fromarray(np.clip(base, 0, 255).astype(np.uint8)).resize((W, H), Image.BICUBIC).convert("RGBA")
        # Streaks: thin lines flying out from the light.
        self.streaks = [(rnd.uniform(0, 2 * math.pi), rnd.uniform(0, 1), rnd.uniform(0.5, 1.4), rnd.uniform(0.3, 1)) for _ in range(46)]
        self.centre = (W / 2, H * 0.33)
        # Floating shapes: soft glassy spheres and a ring, in the scene's colours.
        self.shapes = []
        for k in range(5):
            size = rnd.randint(260, 520)
            c = glow if k % 2 == 0 else tuple(min(255, int(v * 0.8 + 70)) for v in glow)
            x = rnd.choice((rnd.uniform(-0.05, 0.22), rnd.uniform(0.78, 1.05))) * W
            self.shapes.append((self._bokeh(size, c, 0.1 + 0.12 * rnd.random()), x, rnd.uniform(0.1, 0.95) * H, rnd.uniform(0, 6.28), rnd.uniform(0.1, 0.25)))
        self.grain = [self._grain(s) for s in range(4)]

    @staticmethod
    def _bokeh(size, c, alpha):
        """Out-of-focus light: a soft disc with a faint brighter rim."""
        img = Image.new("RGBA", (size + 80, size + 80), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        d.ellipse([40, 40, 40 + size, 40 + size], fill=(*c, int(255 * alpha)))
        d.ellipse([40, 40, 40 + size, 40 + size], outline=(*c, int(255 * alpha * 1.8)), width=6)
        return img.filter(ImageFilter.GaussianBlur(size / 14))

    @staticmethod
    def _orb(size, c):
        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        for i in range(size // 2, 0, -2):
            f = i / (size / 2)
            col = tuple(int(c[j] * (0.35 + 0.65 * (1 - f) ** 0.6)) for j in range(3))
            d.ellipse([size / 2 - i, size / 2 - i - (1 - f) * size * 0.12, size / 2 + i, size / 2 + i - (1 - f) * size * 0.12], fill=(*col, int(150 + 80 * (1 - f))))
        hl = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        ImageDraw.Draw(hl).ellipse([size * 0.28, size * 0.16, size * 0.52, size * 0.34], fill=(255, 255, 255, 120))
        img.alpha_composite(hl.filter(ImageFilter.GaussianBlur(size / 18)))
        return img.filter(ImageFilter.GaussianBlur(1.2))

    @staticmethod
    def _grain(seed):
        n = np.random.default_rng(seed).normal(128, 22, (H // 2, W // 2)).clip(0, 255).astype(np.uint8)
        g = Image.fromarray(n, "L").resize((W, H), Image.NEAREST)
        layer = Image.new("RGBA", (W, H), (255, 255, 255, 0))
        layer.putalpha(g.point(lambda v: max(0, v - 128) // 6))
        return layer

    def frame(self, t, warp=1.0, shapes=1.0):
        img = self.bg.copy()
        d = ImageDraw.Draw(img)
        cx, cy = self.centre
        for ang, phase, speed, bright in self.streaks:
            p = (phase + t * 0.22 * speed * warp) % 1.0
            r0 = 80 + p * p * 1500
            ln = 20 + p * 160 * warp
            # Faint near the middle, where the words are, and gone at the edges.
            a = int(70 * bright * smooth((p - 0.25) / 0.3) * (1 - p) * min(1.6, warp))
            if a <= 4:
                continue
            x0, y0 = cx + math.cos(ang) * r0, cy + math.sin(ang) * r0
            x1, y1 = cx + math.cos(ang) * (r0 + ln), cy + math.sin(ang) * (r0 + ln)
            d.line([(x0, y0), (x1, y1)], fill=(255, 255, 255, a), width=1)
        if shapes > 0:
            for orb, x, y, ph, sp in self.shapes:
                ox = x + math.sin(t * sp + ph) * 40
                oy = y + math.cos(t * sp * 0.8 + ph) * 60
                o = orb if shapes >= 0.999 else mv.put_fading  # noqa
                if shapes >= 0.999:
                    img.alpha_composite(orb, (int(ox - orb.width / 2), int(oy - orb.height / 2)))
                else:
                    mv.put_fading(img, orb, ox, oy, shapes)
        img.alpha_composite(self.grain[int(t * FPS) % 4])
        return img


# ---- The films ------------------------------------------------------------------------------

LIFE = [
    dict(name="meeting", song_bar=5, frames="laptop:meeting", tag="IN A MEETING", glow=(70, 90, 255), deep=(8, 8, 24), accent=(255, 90, 80), seed=0,
         beats=[(0.2, "Meeting in 5", "A countdown to your next call"), (2.4, "One-click join", "Meet, Zoom and Teams links"),
                (6.8, "On a call", "A live timer, right up top"), (11.0, "Camera & mic", "Always know what's on"),
                (13.2, "Mute", "Straight from the island")],
         also=["Calendar & deadlines", "Clipboard history", "Every device's battery", "Timers & focus rounds"]),
    dict(name="class", song_bar=33, frames="phone:life-class", tag="IN CLASS", glow=(40, 170, 230), deep=(4, 12, 24), accent=(100, 210, 255), seed=1,
         beats=[(0.3, "Class in 6 min", "Your timetable, counting down"), (2.6, "Room & time left", "Tap for the whole lesson"),
                (6.0, "Quiet in class", "Do Not Disturb, by itself"), (8.2, "Deadlines", "From your calendar, counted down"),
                (11.6, "Focus streak", "Pomodoro rounds, every day")],
         also=["Notifications with Reply", "Weather & world clocks", "To-do & quick notes", "Alarms"]),
    dict(name="commute", song_bar=77, frames="phone:life-commute", tag="ON THE WAY", glow=(40, 200, 120), deep=(4, 18, 14), accent=(60, 220, 110), seed=2,
         beats=[(0.5, "Next turn", "Directions at a glance"), (2.4, "200 m", "Tap for the whole step"),
                (5.6, "Music joins in", "Two things, side by side"), (7.6, "Incoming call", "Answer from the island"),
                (10.6, "Swipe to switch", "Calls, music, maps")],
         also=["Payments show 'Paid'", "Downloads with a ring", "Control centre on a hold", "Wi-Fi & Bluetooth"]),
    dict(name="gym", song_bar=49, frames="phone:life-gym", tag="AT THE GYM", glow=(150, 60, 255), deep=(12, 4, 24), accent=(190, 120, 255), seed=3,
         beats=[(0.5, "Your playlist", "Any music app"), (2.0, "Buds connected", "Pops up as they connect"),
                (4.2, "Live lyrics", "Synced, line by line"), (8.0, "Stopwatch", "With laps"),
                (12.8, "Rest timer", "It tells you when to go")],
         also=["Sleep timer for music", "Live bars that dance", "Battery health", "Speed test"]),
    dict(name="desk", song_bar=13, frames="laptop:desk", tag="AT YOUR DESK", glow=(255, 130, 40), deep=(20, 8, 4), accent=(255, 170, 60), seed=4,
         beats=[(0.8, "Hover it", "Processor, weather, time, battery"), (4.0, "Earbuds", "Battery the moment they connect"),
                (7.0, "Lyrics", "Scrolling through the island"), (10.4, "Full player", "One click away"),
                (14.6, "Every computer", "Windows · Mac · Linux")],
         also=["Per-app volume", "Clipboard history", "File shelf", "Your phone's notifications"]),
]

PRO_ITEMS = ["Glass & tinted looks", "Album-colour glow", "Stopwatch & world clocks", "Quick notes & to-do", "Your own shortcuts", "Clock faces"]
URL = "github.com/Jimchi-cmd/Dynamic-island-for-everyone"
# Where Islet Pro is sold (named on the Pro card and at the end).
PRO_STORE = "Gumroad"


def icon_tile(kind, colour, size=64):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, size - 1, size - 1], size * 0.28, fill=(*colour, 255))
    c = size / 2
    w = max(3, size // 14)
    white = (255, 255, 255, 255)
    if kind == 0:  # sparkle
        d.polygon([(c, c - 18), (c + 5, c - 5), (c + 18, c), (c + 5, c + 5), (c, c + 18), (c - 5, c + 5), (c - 18, c), (c - 5, c - 5)], fill=white)
    elif kind == 1:  # ring / glow
        d.ellipse([c - 16, c - 16, c + 16, c + 16], outline=white, width=w)
    elif kind == 2:  # stopwatch
        d.ellipse([c - 15, c - 12, c + 15, c + 18], outline=white, width=w)
        d.line([(c, c + 3), (c, c - 6)], fill=white, width=w)
        d.line([(c - 5, c - 18), (c + 5, c - 18)], fill=white, width=w)
    elif kind == 3:  # note
        d.rounded_rectangle([c - 14, c - 17, c + 14, c + 17], 5, outline=white, width=w)
        for k in (-6, 1, 8):
            d.line([(c - 8, c + k), (c + 8, c + k)], fill=white, width=w - 1)
    elif kind == 4:  # shortcut
        d.rounded_rectangle([c - 16, c - 16, c - 2, c - 2], 4, fill=white)
        d.rounded_rectangle([c + 2, c - 16, c + 16, c - 2], 4, fill=white)
        d.rounded_rectangle([c - 16, c + 2, c - 2, c + 16], 4, fill=white)
        d.rounded_rectangle([c + 2, c + 2, c + 16, c + 16], 4, outline=white, width=w - 1)
    else:  # clock
        d.ellipse([c - 17, c - 17, c + 17, c + 17], outline=white, width=w)
        d.line([(c, c), (c, c - 10)], fill=white, width=w)
        d.line([(c, c), (c + 8, c + 4)], fill=white, width=w)
    return img


def glass_card(title, sub, colour, kind, width=860, font=42):
    h = 128 if sub else 118
    img = Image.new("RGBA", (width, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, width - 1, h - 1], 34, fill=(28, 28, 36, 225), outline=(255, 255, 255, 34), width=2)
    img.alpha_composite(icon_tile(kind, colour, 68), (26, (h - 68) // 2))
    d.text((118, h / 2 - (14 if sub else 0)), title, font=mv.font(font, "Bold"), fill="white", anchor="lm")
    if sub:
        d.text((118, h / 2 + 26), sub, font=mv.font(27, "Medium"), fill=GREY, anchor="lm")
    return img


def tilt(img, deg):
    return img.rotate(deg, resample=Image.BICUBIC, expand=True)


def glow_under(w, h, colour):
    img = Image.new("RGBA", (w + 300, h + 300), (0, 0, 0, 0))
    ImageDraw.Draw(img).rounded_rectangle([150, 150, 150 + w, 150 + h], min(h // 2, 120), fill=(*colour, 120))
    return img.filter(ImageFilter.GaussianBlur(70))


def zoom_blur(frame, amount):
    """A quick radial zoom-blur, for cuts between parts."""
    if amount <= 0.01:
        return frame
    out = frame
    for k, s in enumerate((1 + 0.04 * amount, 1 + 0.08 * amount, 1 + 0.12 * amount)):
        w, h = int(W * s), int(H * s)
        big = frame.resize((w, h), Image.BILINEAR).crop(((w - W) // 2, (h - H) // 2, (w - W) // 2 + W, (h - H) // 2 + H))
        out = Image.blend(out, big, 0.5 / (k + 1))
    return out


class Clock:
    """Where the bars and beats fall in the video, and how hard the music hits
    at each moment: from a real song's analysis (beats.py), or for the
    built-in track, from its own steady grid."""

    def __init__(self, story_bars, song=None, analysis=None, start_bar=0):
        bars = (INTRO, story_bars, ALSO, PRO, END)
        if analysis is None:
            self.t0 = 0.0
            self.beat = 60 / BPM
            self.bars_at = [BAR * k for k in range(sum(bars) + 1)]
            self.beats = [self.beat * k for k in range(4 * sum(bars) + 4)]
            self.env = None
        else:
            db = analysis["downbeats"]
            self.t0 = db[start_bar]
            self.bars_at = [db[start_bar + k] - self.t0 for k in range(sum(bars) + 1)]
            self.beat = 60 / analysis["bpm"]
            self.beats = [b - self.t0 for b in analysis["beats"] if b >= self.t0 - 0.01]
            self.env = {k: analysis[k] for k in ("kick", "snap", "loud", "hit")}
        edges = [0]
        for n in bars:
            edges.append(edges[-1] + n)
        self.t_story, self.t_also, self.t_pro, self.t_end, self.t_out = (self.bars_at[e] for e in edges[1:])
        self.total = self.t_out + TAIL
        self._smooth = {}

    def level(self, name, t):
        """0..1, quick to rise and slower to fall, so a hit reads as a punch."""
        if self.env is None:
            # The built-in track: a kick on every beat, a snare on 2 and 4.
            ph = (t % self.beat) / self.beat
            if name == "kick":
                return math.exp(-ph * 7)
            if name == "snap":
                beat_no = int(t / self.beat) % 4
                return math.exp(-ph * 9) if beat_no in (1, 3) else 0.0
            return 0.8
        arr = self.env[name]
        i = int((self.t0 + t) * FPS)
        raw = arr[i] if 0 <= i < len(arr) else 0.0
        prev = self._smooth.get(name, 0.0)
        val = raw if raw > prev else prev * (0.78 if name != "loud" else 0.95) + raw * (0.22 if name != "loud" else 0.05)
        self._smooth[name] = val
        return min(1.0, val)

    def snap_to_beat(self, t, prefer_bar=0.45):
        """The beat nearest `t`, or the bar line if one is close enough."""
        bar = min(self.bars_at, key=lambda b: abs(b - t))
        if abs(bar - t) <= prefer_bar:
            return bar
        return min(self.beats, key=lambda b: abs(b - t)) if self.beats else t


def story_timing(clock, anchors):
    """Map video time to the reel's own time so each of its moments (a tap,
    a card opening) lands on a beat: between anchors the footage plays a
    little faster or slower (never by more than about a fifth)."""
    vid, reel = [clock.t_story], [0.0]
    for e in anchors:
        want = clock.snap_to_beat(clock.t_story + e)
        span_r = e - reel[-1]
        lo, hi = vid[-1] + span_r / 1.22, vid[-1] + span_r / 0.82
        if span_r <= 0.25:
            continue
        v = min(max(want, lo), hi)
        # Prefer a beat inside the allowed range.
        inside = [b for b in clock.beats if lo <= b <= hi]
        if inside:
            v = min(inside, key=lambda b: abs(b - want))
        vid.append(v)
        reel.append(e)

    def reel_time(t):
        if t <= vid[-1]:
            return float(np.interp(t, vid, reel))
        return reel[-1] + (t - vid[-1])

    return reel_time, dict(zip(reel[1:], vid[1:]))


def make(film, frames_dir, out_path, song=None, analysis=None):
    files = sorted(f for f in os.listdir(frames_dir) if f.endswith(".png"))
    laptop = film["frames"].startswith("laptop")
    clock = Clock(STORY + (1 if laptop else 0), song, analysis, film.get("song_bar", 0))
    t_story, t_also, t_pro, t_end = clock.t_story, clock.t_also, clock.t_pro, clock.t_end
    total_s = clock.total
    total = int(total_s * FPS)
    beat = clock.beat
    wav = tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
    if song is None:
        sections = (INTRO + STORY, INTRO + STORY + ALSO, INTRO + STORY + ALSO + PRO)
        music.write_wav(wav, music.banger(total_s, seed=film["seed"], bars_intro=INTRO, sections=sections, bpm=BPM))
    else:
        # The song, from two bars before its drop, faded out at the end.
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", f"{clock.t0:.3f}", "-t", f"{total_s:.3f}", "-i", song,
                        "-af", f"afade=t=in:d=0.04,afade=t=out:st={total_s - 1.3:.3f}:d=1.3", "-ar", "44100", "-ac", "2", wav], check=True)
    reel_time, landed = story_timing(clock, [t0 for t0, _, _ in film["beats"]])
    atm = Atmosphere(film["glow"], film["deep"], film["seed"])
    accent = film["accent"]
    title3d = extruded("Islet", 250, (255, 255, 255), tuple(min(255, int(c * 0.8 + 60)) for c in film["glow"]))
    tag_img = tag(film["tag"], accent)
    # Each caption appears when its moment lands (on a beat).
    heads = [(landed.get(t0, t_story + t0), Kinetic(h, 96), Kinetic(s, 40, "Medium", GREY, gap=0.28)) for t0, h, s in film["beats"]]
    also_title = Kinetic("And also…", 96)
    also_cards = [glass_card(a, None, tuple(int(v) for v in colorsys.hsv_to_rgb((i * 0.17 + 0.55) % 1, 0.6, 0.95 * 255)), i % 6) for i, a in enumerate(film["also"])]
    pro_title = extruded("Islet Pro", 150, (255, 236, 200), (255, 120, 170), depth=16)
    pro_sub = Kinetic("One-time purchase · for your computer", 38, "Medium", GREY)
    pro_cards = [glass_card(p, None, c, i, width=496, font=31) for i, (p, c) in enumerate(zip(PRO_ITEMS, [(255, 159, 10), (255, 55, 95), (90, 200, 250), (52, 199, 89), (175, 82, 222), (10, 132, 255)]))]
    end_title = Kinetic("Get Islet", 120)
    end_sub = Kinetic("Free · open source · no ads", 40, "Medium", GREY)
    button = Image.new("RGBA", (560, 120), (0, 0, 0, 0))
    bd = ImageDraw.Draw(button)
    bd.rounded_rectangle([0, 0, 559, 119], 60, fill=(255, 255, 255, 255))
    bd.line([(70, 38), (70, 74)], fill=(10, 10, 14), width=7)
    bd.polygon([(56, 64), (70, 82), (84, 64)], fill=(10, 10, 14))
    bd.text((110, 60), "Download on GitHub", font=mv.font(40, "Bold"), fill=(10, 10, 14), anchor="lm")
    url = Kinetic(URL, 30, "SemiBold", GREY)
    get_pro = mv.text_block(f"Get Islet Pro on {PRO_STORE}", 46)
    pro_line = Kinetic(f"Islet Pro on {PRO_STORE}", 36, "Bold", (255, 205, 140))
    plats = Kinetic("Android · Windows · Mac · Linux", 34, "SemiBold", (220, 220, 230))
    hero_s, hero_cy, hero_h = 1.0, 760.0, 200.0
    # Words land on the half-beats; list cards one per beat.
    word_step = beat / 2
    title_land = clock.bars_at[1]  # the intro's second bar line
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
         "-i", wav, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "slow", "-crf", "21", "-pix_fmt", "yuv420p",
         "-c:a", "aac", "-b:a", "192k", "-af", "loudnorm=I=-10:TP=-1:LRA=8", "-shortest", "-movflags", "+faststart", out_path],
        stdin=subprocess.PIPE,
    )
    for i in range(total):
        t = i / FPS
        kick = clock.level("kick", t)
        snap = clock.level("snap", t)
        loud = clock.level("loud", t)
        punch = 0.035 * kick if t >= t_story else 0.012 * kick
        build = smooth(t / max(t_story, 0.1)) if t < t_story else 0.0
        warp = 1.0 + 1.6 * snap + 1.8 * build + 2.5 * (1 - smooth((t - t_end) / 1.2)) * (t >= t_end)
        frame = atm.frame(t, warp=warp)
        # ---- Intro: the island grows from a dot; the 3D title lands on the bar line.
        if t < t_story + 0.4:
            k_out = smooth((t - t_story + 0.2) / 0.4)
            pw = int(40 + 520 * back((t - 0.15) / 0.9))
            ph = int(40 + 100 * back((t - 0.15) / 0.9))
            pill = Image.new("RGBA", (pw + 120, ph + 120), (0, 0, 0, 0))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([60, 60, 60 + pw, 60 + ph], ph // 2, fill=(*film["glow"], int(110 + 110 * kick)))
            pill = pill.filter(ImageFilter.GaussianBlur(26))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([60, 60, 60 + pw, 60 + ph], ph // 2, fill=(0, 0, 0, 255), outline=(214, 204, 255, 200), width=3)
            mv.put_fading(frame, pill, W / 2, 620, smooth(t / 0.3) * (1 - k_out))
            land = back((t - title_land) / 0.45)
            if t > title_land - 0.12:
                k = (1.25 - 0.25 * land) * (1 + 0.02 * kick)
                ti = title3d.resize((max(1, int(title3d.width * k)), max(1, int(title3d.height * k))), Image.BICUBIC)
                mv.put_fading(frame, ti, W / 2, 1000, smooth((t - title_land + 0.12) / 0.2) * (1 - k_out))
            mv.put_fading(frame, tag("THE ISLAND FOR ANDROID & PC", accent), W / 2, 820, smooth((t - beat) / 0.3) * (1 - k_out))
        # ---- The story: the island, big, its moments on the beat, with kinetic type below it.
        if t_story - 0.1 <= t < t_also + 0.3:
            st = reel_time(max(t, t_story))
            fi = min(len(files) - 1, max(0, int(st * FPS)))
            isl = Image.open(os.path.join(frames_dir, files[fi])).convert("RGBA")
            box = isl.getbbox() or (0, 0, isl.width, 10)
            isl = isl.crop((max(0, box[0] - 20), max(0, box[1] - 20), min(isl.width, box[2] + 20), min(isl.height, box[3] + 20)))
            want = min(1.45 if not laptop else 1.25, 1000 / max(1, isl.width))
            hero_s += (want - hero_s) * 0.18
            hero_h += (isl.height * hero_s - hero_h) * 0.25
            want_cy = min(700, 1130 - hero_h / 2)
            hero_cy += (want_cy - hero_cy) * 0.2
            sc = hero_s * (1 + punch)
            hero = isl.resize((max(1, int(isl.width * sc)), max(1, int(isl.height * sc))), Image.BICUBIC)
            leave = smooth((t - t_also + 0.1) / 0.3)
            enter = smooth((t - t_story) / 0.3)
            a = enter * (1 - leave)
            # The glow breathes with the music's loudness and flares on each kick.
            mv.put_fading(frame, glow_under(hero.width, hero.height, film["glow"]), W / 2, hero_cy + 20, a * min(1.0, 0.45 + 0.35 * loud + 0.35 * kick))
            mv.put_fading(frame, hero, W / 2, hero_cy, a)
            text_top = hero_cy + hero_h / 2 + 110
            mv.put_fading(frame, tag_img, W / 2, text_top - 20, a)
            for j, (v0, head, sub) in enumerate(heads):
                v1 = heads[j + 1][0] if j + 1 < len(heads) else t_also + 1
                if v0 - 0.05 <= t < v1 + 0.3:
                    out = smooth((t - v1) / 0.22)
                    head.draw(frame, W / 2, text_top + 90, t - v0, out=max(out, leave), stagger=word_step, dur=min(0.42, word_step * 1.6))
                    sub.draw(frame, W / 2, text_top + 175, t - v0 - beat, out=max(out, leave), stagger=0.03)
        # ---- And also… (a card on each beat)
        if t_also - 0.1 <= t < t_pro + 0.3:
            at_ = t - t_also
            leave = smooth((t - t_pro + 0.1) / 0.3)
            also_title.draw(frame, W / 2, 470, at_, out=leave, stagger=word_step)
            for j, card in enumerate(also_cards):
                on = beat * (j + 1)
                p = back((at_ - on) / 0.4)
                if p <= 0:
                    continue
                c = tilt(card, -3 + j * 1.5)
                mv.put_fading(frame, c, W / 2 + (j % 2) * 30 - 15, 690 + j * 175 + 140 * (1 - p), smooth((at_ - on) / 0.2) * (1 - leave))
        # ---- Islet Pro (its extras one per beat).
        if t_pro - 0.1 <= t < t_end + 0.3:
            pt = t - t_pro
            leave = smooth((t - t_end + 0.1) / 0.3)
            r, g, b = (int(v * 255) for v in colorsys.hsv_to_rgb((pt * 0.4) % 1, 0.6, 1))
            pill = Image.new("RGBA", (720, 260), (0, 0, 0, 0))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([90, 70, 630, 190], 60, fill=(r, g, b, int(120 + 130 * kick)))
            pill = pill.filter(ImageFilter.GaussianBlur(30))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([90, 70, 630, 190], 60, fill=(0, 0, 0, 255), outline=(r, g, b, 255), width=4)
            pd.rounded_rectangle([118, 92, 194, 168], 18, fill=(r, g, b, 255))
            for k in range(5):
                # Bars that jump with the music.
                hh = 18 + 50 * min(1.0, 0.25 + 0.75 * (kick if k % 2 == 0 else snap) * (0.6 + 0.4 * math.sin(pt * 9 + k)))
                x = 520 + k * 18
                pd.rounded_rectangle([x, 130 - hh / 2, x + 9, 130 + hh / 2], 4, fill=(r, g, b, 255))
            mv.put_fading(frame, pill, W / 2, 380, smooth(pt / 0.25) * (1 - leave))
            land = back(pt / 0.45)
            k = (1.2 - 0.2 * land) * (1 + 0.02 * kick)
            pti = pro_title.resize((max(1, int(pro_title.width * k)), max(1, int(pro_title.height * k))), Image.BICUBIC)
            mv.put_fading(frame, pti, W / 2, 610, smooth(pt / 0.2) * (1 - leave))
            pro_sub.draw(frame, W / 2, 740, pt - beat, out=leave, stagger=0.03)
            # Where to get it, after the last extra, with a pop on its beat.
            gp = back((pt - beat * 9) / 0.4)
            if gp > 0:
                k = (0.85 + 0.15 * gp) * (1 + 0.025 * kick)
                gi = get_pro.resize((max(1, int(get_pro.width * k)), max(1, int(get_pro.height * k))), Image.BICUBIC)
                mv.put_fading(frame, gi, W / 2, 1330, smooth((pt - beat * 9) / 0.2) * (1 - leave))
            for j, card in enumerate(pro_cards):
                col, row = j % 2, j // 2
                on = beat * (2 + j)
                p = back((pt - on) / 0.4)
                if p <= 0:
                    continue
                s2 = 0.7 + 0.3 * p
                c = card.resize((int(card.width * s2), int(card.height * s2)), Image.BICUBIC)
                mv.put_fading(frame, c, W / 2 + (col - 0.5) * 512, 880 + row * 140, smooth((pt - on) / 0.2) * (1 - leave))
        # ---- The link: the pill grows on the bar line, then a piece per beat.
        if t >= t_end - 0.1:
            et = t - t_end
            grow = back(et / 0.6)
            pw, ph = int(60 + 480 * grow), 110
            pill = Image.new("RGBA", (pw + 200, ph + 200), (0, 0, 0, 0))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([100, 100, 100 + pw, 100 + ph], ph // 2, fill=(*film["glow"], int(130 + 100 * kick)))
            pill = pill.filter(ImageFilter.GaussianBlur(34))
            pd = ImageDraw.Draw(pill)
            pd.rounded_rectangle([100, 100, 100 + pw, 100 + ph], ph // 2, fill=(0, 0, 0, 255), outline=(214, 204, 255, 220), width=3)
            if pw > 200:
                pd.ellipse([100 + pw - 80, 136, 100 + pw - 42, 174], fill=(59, 147, 255, 255))
            mv.put_fading(frame, pill, W / 2, 470, smooth(et / 0.2))
            end_title.draw(frame, W / 2, 720, et - beat, stagger=word_step)
            end_sub.draw(frame, W / 2, 815, et - 2 * beat, stagger=0.04)
            bp = back((et - 3 * beat) / 0.4)
            if bp > 0:
                k = (0.8 + 0.2 * bp) * (1 + 0.03 * kick)
                bt = button.resize((max(1, int(button.width * k)), max(1, int(button.height * k))), Image.BICUBIC)
                mv.put_fading(frame, bt, W / 2, 990, smooth((et - 3 * beat) / 0.2))
            url.draw(frame, W / 2, 1110, et - 4 * beat, stagger=0.0)
            plats.draw(frame, W / 2, 1190, et - 5 * beat, stagger=0.04)
            pro_line.draw(frame, W / 2, 1275, et - 6 * beat, stagger=word_step)
        # A zoom-blur on each cut, which all fall on bar lines.
        for cut in (t_story, t_also, t_pro, t_end):
            d = abs(t - cut)
            if d < 0.16:
                frame = zoom_blur(frame, 1 - d / 0.16)
        ff.stdin.write(frame.convert("RGB").tobytes())
    ff.stdin.close()
    ff.wait()
    os.unlink(wav)
    ok = ff.returncode == 0
    if ok and song is not None:
        # And a silent copy, to post with the app's own licensed sound (same timing).
        silent = os.path.splitext(out_path)[0] + "-silent.mp4"
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", out_path, "-an", "-c:v", "copy", silent], check=True)
    print(f"wrote {out_path} ({total_s:.1f} s{', song from %.2f s' % clock.t0 if song else ''})")
    return ok


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="promo-videos")
    ap.add_argument("--phone-frames", default=os.path.join(ROOT, "android/app/build/promo"))
    ap.add_argument("--laptop-frames", default=os.path.join(ROOT, "build"), help="holds promo-laptop-<reel> folders")
    ap.add_argument("--only")
    ap.add_argument("--song", help="cut every reel to this song (its beats are found automatically)")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    analysis = None
    if a.song:
        analysis = _load("beats", "beats.py").analyse(a.song)
        print(f"song: {analysis['bpm']:.1f} BPM, {len(analysis['downbeats'])} bars")
    ok = True
    for film in LIFE:
        if a.only and film["name"] != a.only:
            continue
        kind, name = film["frames"].split(":")
        src = os.path.join(a.phone_frames, name) if kind == "phone" else os.path.join(a.laptop_frames, f"promo-laptop-{name}")
        ok &= make(film, src, os.path.join(a.out, f"islet-{film['name']}.mp4"), a.song, analysis)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
