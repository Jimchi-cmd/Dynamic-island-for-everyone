#!/usr/bin/env python3
"""The YouTube install guide (1920x1080, about three minutes): every step
to install Islet on an Android phone, Windows, a Mac and Linux, and to pair
the phone with the computer, each with a screen, the exact button
highlighted and a finger that moves to it.

The phone and computer screens are simplified drawings, not screenshots
of anyone's software; Islet's own pages are the app's real renders
(android/app/build/renders/screens-dark.png). The background music is
made by music.py.

  python3 scripts/promo/make-tutorial.py --out promo-videos
"""

import argparse
import importlib.util
import math
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, file))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


mv = _load("make_videos", "make-videos.py")
music = _load("music", "music.py")
life = _load("life", "make-life-videos.py")

FPS, W, H = 30, 1920, 1080
F = mv.font
PS = (450, 960)  # a phone screen
LS = (960, 600)  # a computer screen
REL = "github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases"

BG = (24, 24, 28)
CARD = (40, 40, 46)
TEXT = (240, 240, 245)
MUTED = (160, 160, 170)
BLUE = (10, 132, 255)
GREEN = (52, 199, 89)


# ---- Drawing helpers for the simplified screens ------------------------------------------------


def screen(size=PS, bg=BG):
    img = Image.new("RGBA", size, (*bg, 255))
    return img, ImageDraw.Draw(img)


def header(d, title, size=PS, back=True):
    d.text((24 if not back else 64, 70), title, font=F(30, "Bold"), fill=TEXT, anchor="lm")
    if back:
        d.line([(34, 70), (22, 58)], fill=TEXT, width=4)
        d.line([(34, 70), (22, 82)], fill=TEXT, width=4)
        d.line([(22, 70), (46, 70)], fill=TEXT, width=0)


def row(d, y, title, note=None, toggle=None, value=None, w=PS[0]):
    d.rounded_rectangle([16, y, w - 16, y + (96 if note else 72)], 18, fill=CARD)
    d.text((36, y + (30 if note else 36)), title, font=F(24, "SemiBold"), fill=TEXT, anchor="lm")
    if note:
        d.text((36, y + 64), note, font=F(19, "Medium"), fill=MUTED, anchor="lm")
    cy = y + (48 if note else 36)
    if toggle is not None:
        x = w - 96
        d.rounded_rectangle([x, cy - 18, x + 64, cy + 18], 18, fill=GREEN if toggle else (90, 90, 96))
        kx = x + (46 if toggle else 18)
        d.ellipse([kx - 15, cy - 15, kx + 15, cy + 15], fill="white")
        return (x, cy - 18, 64, 36)
    if value:
        d.text((w - 40, cy), value, font=F(22, "Medium"), fill=MUTED, anchor="rm")
    return (16, y, w - 32, 96 if note else 72)


def button(d, x, y, label, primary=True, w=None):
    f = F(24, "Bold")
    tw = int(d.textlength(label, font=f)) + 48 if w is None else w
    d.rounded_rectangle([x, y, x + tw, y + 56], 28, fill=BLUE if primary else (70, 70, 78))
    d.text((x + tw / 2, y + 28), label, font=f, fill="white", anchor="mm")
    return (x, y, tw, 56)


def dialog(img, title, body, buttons, size=PS):
    """A plain dialog over a dimmed screen. Returns the buttons' rects."""
    img.alpha_composite(Image.new("RGBA", size, (0, 0, 0, 140)))
    d = ImageDraw.Draw(img)
    x0, x1 = 30, size[0] - 30
    lines = body.split("\n")
    h = 120 + 34 * len(lines) + 90
    y0 = size[1] // 2 - h // 2
    d.rounded_rectangle([x0, y0, x1, y0 + h], 30, fill=(48, 48, 54))
    d.text((x0 + 30, y0 + 50), title, font=F(28, "Bold"), fill=TEXT, anchor="lm")
    for i, line in enumerate(lines):
        d.text((x0 + 30, y0 + 104 + i * 34), line, font=F(21, "Medium"), fill=MUTED, anchor="lm")
    rects = []
    x = x1 - 24
    for label, primary in reversed(buttons):
        f = F(24, "Bold")
        tw = int(d.textlength(label, font=f)) + 40
        x -= tw
        rects.append(button(d, x, y0 + h - 80, label, primary, tw))
        x -= 14
    return list(reversed(rects))


def browser(size, url):
    img, d = screen(size, (250, 250, 252))
    d.rectangle([0, 0, size[0], 110], fill=(235, 235, 240))
    d.rounded_rectangle([20, 40, size[0] - 20, 90], 25, fill="white")
    d.text((44, 65), url, font=F(19, "Medium"), fill=(60, 60, 70), anchor="lm")
    return img, d


def assets(img, d, files, pick, top=140):
    """A release's file list; returns the picked file's rect."""
    w = img.width
    d.text((24, top), "Islet 0.4.5", font=F(34, "Bold"), fill=(20, 20, 24), anchor="lm")
    d.text((24, top + 44), "Latest", font=F(20, "Bold"), fill=(26, 127, 55), anchor="lm")
    d.text((24, top + 100), "▾ Assets", font=F(24, "Bold"), fill=(20, 20, 24), anchor="lm")
    target = None
    for i, name in enumerate(files):
        y = top + 140 + i * 58
        d.line([(16, y), (w - 16, y)], fill=(225, 225, 230), width=2)
        d.text((40, y + 29), name, font=F(20, "SemiBold"), fill=(9, 105, 218), anchor="lm")
        if i == pick:
            target = (16, y + 4, w - 32, 50)
    return target


def islet_page(index, crop_top=0):
    """One of Islet's real pages (Home, Set up, ...) from the render sheet."""
    path = os.path.join(ROOT, "android/app/build/renders/screens-dark.png")
    sheet = Image.open(path).convert("RGBA")
    pw = sheet.width // 6
    page = sheet.crop((pw * index, 0, pw * (index + 1), sheet.height))
    page = page.resize((PS[0], int(page.height * PS[0] / pw)), Image.BICUBIC)
    return page.crop((0, crop_top, PS[0], crop_top + PS[1]))


def island_frame(i, folder="life-gym", size=None):
    d = os.path.join(ROOT, "android/app/build/promo", folder)
    files = sorted(os.listdir(d)) if os.path.isdir(d) else []
    if not files:
        return None
    f = Image.open(os.path.join(d, files[i % len(files)])).convert("RGBA")
    f = f.crop((0, 0, f.width, int(f.height * 0.45)))
    return f.resize(size, Image.BICUBIC) if size else f


# ---- The steps ------------------------------------------------------------------------------


def s_phone_browser():
    img, d = browser(PS, REL)
    t = assets(img, d, ["Islet-0.4.5-android-debug.apk", "Islet_0.4.5_x64-setup.exe", "Islet_0.4.5_universal.dmg", "Islet_0.4.5_amd64.deb", "Islet_0.4.5_amd64.AppImage"], 0)
    return img, t


def s_unknown_apps():
    img, d = browser(PS, REL)
    assets(img, d, ["Islet-0.4.5-android-debug.apk"], 0)
    r = dialog(img, "Install blocked", "Your phone doesn't install apps from\nthis browser yet. You can allow it\nin Settings.", [("Cancel", False), ("Settings", True)])
    return img, r[1]


def s_allow_source():
    img, d = screen()
    header(d, "Install unknown apps")
    d.text((24, 150), "Chrome", font=F(26, "Bold"), fill=TEXT, anchor="lm")
    t = row(d, 190, "Allow from this source", toggle=False)
    return img, t


def s_install():
    img, d = screen()
    r = dialog(img, "Islet", "Do you want to install this app?", [("Cancel", False), ("Install", True)])
    return img, r[1]


def s_islet_home():
    return islet_page(0), (16, 158, PS[0] - 32, 56)


def s_islet_setup():
    return islet_page(1), (336, 166, 84, 50)


def s_notification_access():
    img, d = screen()
    header(d, "Notification access")
    d.text((24, 140), "Apps that can read notifications", font=F(20, "Medium"), fill=MUTED, anchor="lm")
    row(d, 170, "Phone Link", toggle=False)
    t = row(d, 254, "Islet", toggle=False)
    row(d, 338, "Wear OS", toggle=False)
    return img, t


def s_restricted():
    img, d = screen()
    header(d, "Accessibility")
    row(d, 150, "Islet island", "Off")
    r = dialog(img, "Restricted setting", "For your security, this setting is\ncurrently unavailable.", [("OK", True)])
    return img, r[0]


def s_app_info_menu():
    img, d = screen()
    header(d, "App info")
    d.rounded_rectangle([PS[0] / 2 - 50, 140, PS[0] / 2 + 50, 240], 26, fill=(10, 10, 12))
    d.ellipse([PS[0] / 2 + 14, 180, PS[0] / 2 + 34, 200], fill=(59, 147, 255))
    d.text((PS[0] / 2, 280), "Islet", font=F(30, "Bold"), fill=TEXT, anchor="mm")
    for i, (a, b) in enumerate([("Notifications", "On"), ("Permissions", ""), ("Storage", "20 MB")]):
        row(d, 340 + i * 84, a, value=b)
    # The ⋮ menu, open.
    for k in (-12, 0, 12):
        d.ellipse([PS[0] - 44, 70 + k - 4, PS[0] - 36, 70 + k + 4], fill=TEXT)
    d.rounded_rectangle([PS[0] - 330, 100, PS[0] - 20, 170], 18, fill=(60, 60, 66))
    d.text((PS[0] - 306, 135), "Allow restricted settings", font=F(22, "SemiBold"), fill=TEXT, anchor="lm")
    return img, (PS[0] - 330, 100, 310, 70)


def s_accessibility():
    img, d = screen()
    header(d, "Accessibility")
    d.text((24, 140), "Downloaded apps", font=F(20, "Medium"), fill=MUTED, anchor="lm")
    t = row(d, 170, "Islet island", "Off")
    return img, t


def s_accessibility_switch():
    img, d = screen()
    header(d, "Islet island")
    t = row(d, 150, "Use Islet island", toggle=False)
    d.multiline_text((30, 270), "Islet asks for no events and reads\nnothing on your screen. It places the\nisland round the camera, where taps\nreach it.", font=F(20, "Medium"), fill=MUTED, spacing=10)
    return img, t


def s_done_phone():
    img = mv.wallpaper(PS, (34, 197, 94), (14, 165, 233), 4).convert("RGBA")
    return img, None, "island"


def laptop_screen(bg=(30, 50, 110)):
    img = mv.wallpaper(LS, (60, 90, 200), (120, 40, 160), 9).convert("RGBA")
    d = ImageDraw.Draw(img)
    d.rectangle([0, LS[1] - 44, LS[0], LS[1]], fill=(20, 20, 26, 220))
    return img, d


def s_win_download():
    img, d = browser(LS, REL)
    t = assets(img, d, ["Islet-0.4.5-android-debug.apk", "Islet_0.4.5_x64-setup.exe", "Islet_0.4.5_x64_en-US.msi", "Islet_0.4.5_universal.dmg"], 1, top=130)
    return img, t


def s_smartscreen(stage):
    img, d = laptop_screen()
    x0, y0, x1, y1 = 180, 90, LS[0] - 180, LS[1] - 90
    d.rectangle([x0, y0, x1, y1], fill=(0, 90, 170))
    d.text((x0 + 30, y0 + 50), "Windows protected your PC", font=F(30, "Bold"), fill="white", anchor="lm")
    d.multiline_text((x0 + 30, y0 + 100), "An unrecognised app was stopped\nfrom starting.", font=F(20, "Medium"), fill=(220, 230, 255), spacing=8)
    if stage == 0:
        d.text((x0 + 30, y0 + 200), "More info", font=F(22, "Bold"), fill="white", anchor="lm")
        d.line([(x0 + 30, y0 + 214), (x0 + 130, y0 + 214)], fill="white", width=2)
        target = (x0 + 24, y0 + 182, 120, 40)
    else:
        d.text((x0 + 30, y0 + 190), "App: Islet_0.4.5_x64-setup.exe", font=F(20, "Medium"), fill=(220, 230, 255), anchor="lm")
        target = (x1 - 380, y1 - 70, 170, 50)
        d.rectangle(target[:2] + (target[0] + target[2], target[1] + target[3]), outline="white", width=2)
        d.text((target[0] + 85, target[1] + 25), "Run anyway", font=F(20, "Bold"), fill="white", anchor="mm")
    d.rectangle([x1 - 190, y1 - 70, x1 - 30, y1 - 20], fill=(200, 210, 230))
    d.text((x1 - 110, y1 - 45), "Don't run", font=F(20, "Bold"), fill=(20, 30, 60), anchor="mm")
    return img, target


def s_installer():
    img, d = laptop_screen()
    x0, y0, x1, y1 = 220, 110, LS[0] - 220, LS[1] - 110
    d.rounded_rectangle([x0, y0, x1, y1], 14, fill=(245, 245, 248))
    d.text((x0 + 30, y0 + 46), "Islet Setup", font=F(28, "Bold"), fill=(20, 20, 26), anchor="lm")
    d.text((x0 + 30, y0 + 100), "Islet will be installed for you.", font=F(20, "Medium"), fill=(70, 70, 80), anchor="lm")
    t = button(d, x1 - 170, y1 - 80, "Install", True, 140)
    return img, t


def s_desktop_island():
    img, d = laptop_screen()
    d = os.path.join(os.environ.get("ISLET_LAPTOP_FRAMES", ""), "f0060.png")
    if os.path.exists(d):
        f = Image.open(d).convert("RGBA")
        f = f.resize((int(LS[0] * 0.55), int(LS[0] * 0.55 * f.height / f.width)), Image.BICUBIC)
        img.alpha_composite(f, ((LS[0] - f.width) // 2, 0))
    return img, (LS[0] // 2 - 90, 6, 180, 40)


def s_mac_dmg():
    img, d = laptop_screen()
    d.rounded_rectangle([200, 120, LS[0] - 200, LS[1] - 120], 18, fill=(245, 245, 248))
    d.rounded_rectangle([280, 210, 400, 330], 28, fill=(10, 10, 12))
    d.ellipse([360, 260, 380, 280], fill=(59, 147, 255))
    d.text((340, 360), "Islet", font=F(22, "SemiBold"), fill=(20, 20, 26), anchor="mm")
    d.rounded_rectangle([LS[0] - 400, 210, LS[0] - 280, 330], 20, fill=(70, 140, 230))
    d.text((LS[0] - 340, 360), "Applications", font=F(22, "SemiBold"), fill=(20, 20, 26), anchor="mm")
    d.line([(430, 270), (LS[0] - 430, 270)], fill=(120, 120, 130), width=6)
    d.polygon([(LS[0] - 430, 255), (LS[0] - 405, 270), (LS[0] - 430, 285)], fill=(120, 120, 130))
    return img, (LS[0] - 400, 210, 120, 120)


def s_mac_privacy():
    img, d = laptop_screen()
    d.rounded_rectangle([140, 70, LS[0] - 140, LS[1] - 70], 18, fill=(40, 40, 46))
    d.text((180, 120), "Privacy & Security", font=F(30, "Bold"), fill=TEXT, anchor="lm")
    d.text((180, 190), "Security", font=F(22, "Bold"), fill=MUTED, anchor="lm")
    d.multiline_text((180, 240), "\"Islet\" was blocked to protect your Mac.", font=F(22, "Medium"), fill=TEXT)
    t = button(d, LS[0] - 400, 300, "Open Anyway", False, 220)
    return img, t


def s_linux():
    img, d = laptop_screen()
    d.rounded_rectangle([120, 90, LS[0] - 120, LS[1] - 90], 14, fill=(18, 18, 22))
    lines = [
        "$ cd ~/Downloads",
        "$ sudo apt install ./Islet_0.4.5_amd64.deb",
        "",
        "# or, on any Linux:",
        "$ chmod +x Islet_0.4.5_amd64.AppImage",
        "$ ./Islet_0.4.5_amd64.AppImage",
    ]
    mono = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
    from PIL import ImageFont
    f = ImageFont.truetype(mono, 24) if os.path.exists(mono) else F(24)
    for i, line in enumerate(lines):
        d.text((150, 140 + i * 48), line, font=f, fill=(140, 230, 160) if line.startswith("$") else (150, 150, 160))
    return img, (140, 166, 640, 44)


def s_pair_computer():
    img, d = laptop_screen()
    d.rounded_rectangle([120, 50, LS[0] - 120, LS[1] - 60], 18, fill=(36, 36, 42))
    d.text((160, 100), "Islet Settings", font=F(28, "Bold"), fill=TEXT, anchor="lm")
    d.text((160, 160), "Wi-Fi, Bluetooth & phone  ›  Your Android phone", font=F(20, "Medium"), fill=MUTED, anchor="lm")
    # A QR-like square (not a real code).
    import random
    rnd = random.Random(3)
    qx, qy, cell = LS[0] - 360, 200, 10
    d.rectangle([qx - 14, qy - 14, qx + 25 * cell + 14, qy + 25 * cell + 14], fill="white")
    for yy in range(25):
        for xx in range(25):
            corner = (xx < 7 and yy < 7) or (xx > 17 and yy < 7) or (xx < 7 and yy > 17)
            on = ((xx in (0, 6) or yy in (0, 6) or (2 <= xx <= 4 and 2 <= yy <= 4)) if corner else rnd.random() < 0.45)
            if corner and (xx > 17):
                on = (xx in (18, 24) or yy in (0, 6) or (20 <= xx <= 22 and 2 <= yy <= 4))
            if corner and yy > 17:
                on = (xx in (0, 6) or yy in (18, 24) or (2 <= xx <= 4 and 20 <= yy <= 22))
            if on:
                d.rectangle([qx + xx * cell, qy + yy * cell, qx + xx * cell + cell - 1, qy + yy * cell + cell - 1], fill="black")
    t = button(d, 160, 220, "Pair a phone", True, 220)
    return img, t


def s_pair_phone():
    page = islet_page(5)
    return page, (30, 256, 148, 52)


STEPS = [
    # (chapter, step title, lines, screen fn, phone/laptop, seconds)
    ("Welcome", "Install Islet, free", ["An island round your phone's camera,", "and at the top of your laptop's screen.", "This guide: Android, Windows, Mac and Linux."], "intro", None, 7),
    ("Android phone", "1. Open the download page", ["On your phone, open the browser and go to:", "github.com/Jimchi-cmd/", "Dynamic-island-for-everyone/releases", "Under Assets, tap the file ending in .apk"], s_phone_browser, "phone", 9),
    ("Android phone", "2. Allow your browser to install it", ["Android blocks apps from outside the Play Store", "the first time. Tap Settings…"], s_unknown_apps, "phone", 8),
    ("Android phone", "2. Allow your browser to install it", ["…turn on Allow from this source,", "then go back."], s_allow_source, "phone", 7),
    ("Android phone", "3. Install", ["Tap Install. If you had an older Islet,", "uninstall it first: test builds are signed differently."], s_install, "phone", 8),
    ("Android phone", "4. Open Islet and tap Set up", ["Open Islet from your apps.", "On the home page, tap Set up."], s_islet_home, "phone", 7),
    ("Android phone", "5. Allow notification access", ["Next to Notification access, tap Allow.", "Islet needs it for music, calls,", "directions and notifications."], s_islet_setup, "phone", 8),
    ("Android phone", "5. Allow notification access", ["In the list, turn on Islet,", "then press back to return to Islet."], s_notification_access, "phone", 7),
    ("Android phone", "6. Turn on Round the camera", ["Back in Set up, tap Open next to Round the camera.", "In Accessibility, find Downloaded apps", "and tap Islet island."], s_accessibility, "phone", 8),
    ("Android phone", "6. Turn on Round the camera", ["Turn on Use Islet island and confirm.", "This puts the island over the status bar,", "round the camera, where taps reach it."], s_accessibility_switch, "phone", 8),
    ("If a switch is greyed out", "Restricted setting?", ["Android 13 and later restrict apps installed", "outside the Play Store. Tap OK, then…"], s_restricted, "phone", 7),
    ("If a switch is greyed out", "Allow restricted settings", ["Open Settings → Apps → Islet.", "Tap the three-dot menu at the top right", "→ Allow restricted settings. Then try again."], s_app_info_menu, "phone", 9),
    ("Android phone", "Done!", ["The island now sits round your camera.", "Tap it: the strip, then your day. Hold it:", "the control centre. Swipe up to close."], s_done_phone, "phone", 8),
    ("Windows", "1. Download the installer", ["On your computer, go to the same page", "(github.com/Jimchi-cmd/", "Dynamic-island-for-everyone/releases)", "and click Islet_…_x64-setup.exe"], s_win_download, "laptop", 9),
    ("Windows", "2. Get past SmartScreen", ["Islet isn't code-signed yet, so Windows warns", "the first time. Click More info…"], lambda: s_smartscreen(0), "laptop", 8),
    ("Windows", "2. Get past SmartScreen", ["…then Run anyway."], lambda: s_smartscreen(1), "laptop", 6),
    ("Windows", "3. Install", ["Click Install. Islet starts by itself,", "and again each time you sign in."], s_installer, "laptop", 7),
    ("Windows", "The island", ["It sits at the top of your screen.", "Point at it for the weather, time and battery;", "click to open it; right-click for more."], s_desktop_island, "laptop", 8),
    ("Mac", "Install on a Mac", ["Download Islet_…_universal.dmg, open it,", "and drag Islet into Applications."], s_mac_dmg, "laptop", 8),
    ("Mac", "First open", ["macOS blocks it the first time. Open", "System Settings → Privacy & Security", "and click Open Anyway."], s_mac_privacy, "laptop", 9),
    ("Linux", "Install on Linux", ["Download the .deb (Ubuntu, Debian, Mint)", "or the .AppImage (any Linux), then:"], s_linux, "laptop", 9),
    ("Phone + computer", "Optional: pair them", ["Your phone's notifications, music and battery", "on your computer's island. In Islet Settings:", "Wi-Fi, Bluetooth & phone → Pair a phone."], s_pair_computer, "laptop", 9),
    ("Phone + computer", "Scan the code", ["On the phone, open Islet → Your computer", "→ Scan the code. Both must be", "on the same Wi-Fi."], s_pair_phone, "phone", 8),
    ("That's it", "Enjoy Islet", ["Free and open source.", "Islet Pro adds extras on your computer:", "looks, glow, stopwatch, notes, shortcuts…"], "outro", None, 8),
]


def device_frame(content, kind):
    """Put a screen into a phone or a laptop body."""
    if kind == "phone":
        body = mv.rounded((PS[0] + 36, PS[1] + 36), 70, (12, 12, 14, 255))
        m = Image.new("L", PS, 0)
        ImageDraw.Draw(m).rounded_rectangle([0, 0, PS[0] - 1, PS[1] - 1], 54, fill=255)
        c = content.copy()
        c.putalpha(Image.composite(c.getchannel("A"), Image.new("L", PS, 0), m))
        body.alpha_composite(c, (18, 18))
        return body
    body = mv.rounded((LS[0] + 40, LS[1] + 40), 26, (14, 14, 16, 255))
    body.alpha_composite(content, (20, 20))
    out = Image.new("RGBA", (body.width + 80, body.height + 30), (0, 0, 0, 0))
    out.alpha_composite(body, (40, 0))
    ImageDraw.Draw(out).rounded_rectangle([0, body.height, out.width - 1, out.height - 1], 14, fill=(70, 70, 78, 255))
    return out


def finger(img, x, y, press, alpha=1.0):
    """A fingertip: a ring, so the button's label stays readable under it."""
    d = ImageDraw.Draw(img)
    r = 24 - 5 * press
    a = int(230 * alpha)
    d.ellipse([x - r - 3, y - r - 3, x + r + 3, y + r + 3], outline=(0, 0, 0, a // 3), width=3)
    d.ellipse([x - r, y - r, x + r, y + r], outline=(255, 255, 255, a), width=5)


def make(out_path, laptop_frames):
    os.environ["ISLET_LAPTOP_FRAMES"] = laptop_frames or ""
    bg = mv.wallpaper((W + 300, H + 200), (40, 50, 120), (90, 30, 120), 5).convert("RGBA")
    total_s = sum(s[5] for s in STEPS)
    wav = tempfile.NamedTemporaryFile(suffix=".wav", delete=False).name
    music.write_wav(wav, music.track(total_s, seed=7, quiet=True))
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
         "-i", wav, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
         "-c:a", "aac", "-b:a", "160k", "-af", "loudnorm=I=-20:TP=-2:LRA=11", "-shortest", "-movflags", "+faststart", out_path],
        stdin=subprocess.PIPE,
    )
    chapters = []
    clock = 0.0
    frame_no = 0
    prev_dev = None
    for idx, (chapter, title, lines, fn, kind, secs) in enumerate(STEPS):
        if not chapters or chapters[-1][1] != chapter:
            chapters.append((clock, chapter))
        target = None
        island = None
        if fn == "intro" or fn == "outro":
            dev = None
        else:
            got = fn()
            content, target = got[0], got[1]
            island = got[2] if len(got) > 2 else None
            dev = device_frame(content, kind)
        tb = mv.text_block(title, 66 if kind != "laptop" else 58, pill=False)
        cb = mv.text_block(chapter.upper(), 28)
        lbs = [mv.text_block(line, 38 if kind != "laptop" else 34, pill=False) for line in lines]
        n = int(secs * FPS)
        for i in range(n):
            t = i / FPS
            gt = (frame_no) / FPS
            ox = int(150 + math.sin(gt * 0.2) * 140)
            oy = int(100 + math.cos(gt * 0.17) * 90)
            frame = bg.crop((ox, oy, ox + W, oy + H))
            a_in = life.smooth(t / 0.5)
            a_out = 1 - life.smooth((t - (secs - 0.45)) / 0.45)
            a = a_in * a_out
            text_x = (560 if kind == "phone" else 450) if dev is not None else W / 2
            mv.put_fading(frame, cb, text_x, 200, a)
            mv.put_fading(frame, tb, text_x, 300, a, rise=int(26 * (1 - life.back(t / 0.6))))
            for j, lb in enumerate(lbs):
                aj = life.smooth((t - 0.3 - j * 0.25) / 0.45) * a_out
                mv.put_fading(frame, lb, text_x, 430 + j * 60, aj, rise=int(16 * (1 - life.smooth((t - 0.3 - j * 0.25) / 0.45))))
            if dev is not None:
                # The device slides in from the right (or stays if it's the same kind).
                same = prev_dev == kind and idx > 0
                slide = 0 if same else 160 * (1 - life.back(t / 0.7))
                dx = 1310 if kind == "phone" else 1390
                layer = dev.copy()
                if island == "island":
                    # The island, live: music, then its card.
                    f = island_frame(15 + (i % 210), "life-gym", (PS[0], int(PS[0] * 0.45 * 1200 / 1080)))
                    if f is not None:
                        layer.alpha_composite(f, (18, 18))
                if target is not None:
                    tx0, ty0, tw, th = target
                    sx = (18 if kind == "phone" else 60)
                    sy = (18 if kind == "phone" else 20)
                    cx, cy = sx + tx0 + tw / 2, sy + ty0 + th / 2
                    # A pulsing ring round the button, then the finger glides to it and taps.
                    pulse = 0.5 + 0.5 * math.sin(t * 5)
                    dd = ImageDraw.Draw(layer)
                    if t > 0.8:
                        k = life.smooth((t - 0.8) / 0.4)
                        dd.rounded_rectangle([sx + tx0 - 6, sy + ty0 - 6, sx + tx0 + tw + 6, sy + ty0 + th + 6], 20,
                                             outline=(255, 214, 10, int(255 * k * (0.55 + 0.45 * pulse))), width=5)
                    move = life.smooth((t - 1.4) / 1.0)
                    away = life.smooth((t - 3.1) / 0.6)
                    fx = layer.width + 40 + (cx - layer.width - 40) * move + 70 * away
                    fy = layer.height * 0.8 + (cy - layer.height * 0.8) * move + 70 * away
                    press = max(0.0, 1 - abs(t - 2.7) / 0.2)
                    if t > 1.4:
                        finger(layer, fx, fy, press, 1 - 0.6 * away)
                    if 2.6 < t < 3.3:
                        mv.touch(layer, "tap", t - 2.6, cx, cy)
                if kind == "laptop":
                    layer = layer.resize((int(layer.width * 0.8), int(layer.height * 0.8)), Image.BICUBIC)
                mv.put_fading(frame, layer, dx + slide, H / 2 + 10, life.smooth(t / 0.4) * a_out if not same else a_out)
            elif fn in ("intro", "outro"):
                f = island_frame(15 + (i % 210), "life-gym")
                if f is not None:
                    f = f.resize((int(f.width * 0.58), int(f.height * 0.58)), Image.BICUBIC)
                    # Below the text: the island's top under the last line.
                    mv.put_fading(frame, f, W / 2, 640 + f.height / 2, a)
                if fn == "outro":
                    mv.put_fading(frame, mv.text_block(REL, 34), W / 2, 1020, life.smooth((t - 1.0) / 0.5) * a_out)
            # Progress through the guide, along the bottom.
            done = (clock + t) / total_s
            ImageDraw.Draw(frame).rounded_rectangle([0, H - 8, int(W * done), H], 4, fill=(255, 255, 255, 160))
            ff.stdin.write(frame.convert("RGB").tobytes())
            frame_no += 1
        clock += secs
        prev_dev = kind if dev is not None else None
    ff.stdin.close()
    ff.wait()
    os.unlink(wav)
    # YouTube chapters, for the description.
    lines = [f"{int(c // 60)}:{int(c % 60):02d} {name}" for c, name in chapters]
    with open(os.path.splitext(out_path)[0] + "-chapters.txt", "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {out_path} ({total_s:.0f} s)\n" + "\n".join(lines))
    return ff.returncode == 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="promo-videos")
    ap.add_argument("--laptop-frames", help="a folder of laptop island frames (promo_frames) for the Windows step")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    return 0 if make(os.path.join(a.out, "islet-install-guide-full.mp4"), a.laptop_frames) else 1


if __name__ == "__main__":
    sys.exit(main())
