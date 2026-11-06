# Islet design spec

Goal: someone glancing at the top of the screen should think "that's the Dynamic Island, on my laptop". Fidelity comes from three things, in this order: **the motion**, **the shape**, **restraint in content**.

All numbers are in **logical px** (1 px = 1 iOS point). They are approximations of the publicly visible iOS behaviour (Apple's Live Activities guidelines, and frame-by-frame observation of screen recordings), tuned by eye. They are not taken from Apple assets or code. Each is a named constant in `islet-core`, so tuning one is a one-line change.

## 1. Rules

- **Pure black** `#000` fill. No border and no shadow in any collapsed state.
- **Continuous corners.** Corners are squircles with smoothing 0.6 (`islet-render::Squircle`), never plain circular arcs. Capsules keep a perfectly round tip and ease in along the long edge.
- **Typography.** The island on iPhone uses Apple's San Francisco. Its licence allows it only on Apple platforms and forbids redistribution, so:
  - **macOS:** the OS system font, which *is* San Francisco. It is loaded from the system at runtime and never copied into the app.
  - **Windows and Linux:** **Inter** (SIL Open Font License 1.1) is bundled. It is an independent interface typeface with a very similar feel, and its tabular figures suit timers and percentages. It is subset to Latin, Greek and Cyrillic (about 300 KB), and other scripts fall back to the OS fonts. Its licence ships in `THIRD_PARTY_LICENSES`.
  - Settings → Appearance can switch to the OS font (Segoe UI Variable or the fontconfig default).
  - San Francisco is never bundled, and no other font is presented as it.
- **Icons:** original vector drawings made in code on a 24 × 24 grid (`islet-render/src/elements.rs`). No SF Symbols and no Apple glyphs, sounds or images.
- **Animate only transforms and opacity, conceptually.** The renderer re-rasterises the shape every frame, but layout never changes during an animation: every animated property is a spring value.

## 2. Placement

| Setting | Value |
|---|---|
| Horizontal | centred on the chosen display |
| Top gap | **0 px** by default (flush, as if emerging from the bezel); "Floating" option: 8 px |
| Overlay window | 420 × 260, sized to contain the largest state plus spring overshoot (lists show five rows, or four above a footer, and scroll) |
| Notched MacBook | the idle island *is* the notch: width = notch width, height = menu-bar height, top corners square. Compact and expanded states grow out of it sideways and downwards with the same black, so the notch appears to expand |
| Fullscreen apps and games | hidden (the next notification only peeks) |

## 3. States and sizes

| State | Size (w × h) | Corner radius | Content |
|---|---|---|---|
| **Idle** | 126 × 37 | 18.5 (capsule) | the time, centred (14 px semibold, tabular; a 12-hour suffix at 11 px, 70 %); fades as the pill grows. Redrawn once a minute, never while hidden. Off: nothing |
| **Hover peek** (desktop only) | 136 × 39 | capsule | nothing; says "I'm interactive" |
| **Compact** | 236 × 37 | capsule | leading: 25 × 25 artwork/icon (radius 7) at 6 px inset; trailing: indicator (waveform, timer, ETA) at 10 px inset |
| **Compact with lyrics** | 340 × 37 | capsule | artwork and waveform as Compact; between them a strip of lyric lines (13.5 px semibold, 26 px apart) sliding left as the song goes on: the line being sung at full white, the others at 38 %, fading out over 14 px at each end. A line that fits is centred; a longer one scrolls across while it's sung. Each slide takes 0.4 s (ease-out, 60 fps); otherwise 30 fps with the waveform |
| **Minimal** (2 activities) | pill 162 × 37 + detached circle Ø 37, 8 px gap | capsule / circle | pill: primary activity's leading view; circle: secondary activity's icon |
| **Expanded** | 372 × 160 (shorter when the content needs less, min 84) | 44 | the activity's expanded view, 18 px padding |
| **Large** | 384 × 230 | 48 | full controls (scrubbing, volume, queue) |

Activity cards (Now Playing, timer, navigation, battery, calendar, notifications, script activities) start with a 26 px strip holding a "‹ Control Centre" capsule (24 px tall, 14 % white) at the top left; the card's content sits below it. Control Centre opened that way shows a "Now Playing ›" capsule above the clock to go back; closing the island forgets it. Lists (Wi-Fi, Bluetooth, shelf, clipboard, system) have a round 28 px back button with a chevron. With lyrics on, the Now Playing card is 44 px taller: two centred 15 px lines, the sung one lit, rolling up as each new line starts.

Hover-only mode: the idle island fades out (0.22 s, critically damped) and fades back in as the pointer arrives and it peeks. It keeps an alpha of 1/255, so Windows still hit-tests it; X11, Wayland and macOS use the input region. Anything live keeps it visible.

Spring overshoot is at most 3 % of a change, so Large peaks at about 391 × 236, inside the 420 × 260 window.

### Content inside the pill

| Element | Spec |
|---|---|
| Title | 15 px, semibold, white, one line; marquee after 1.5 s if truncated (Compact: 13 px) |
| Subtitle | 13 px, regular, white at 60 % |
| Album art | squircle radius 10 (Expanded) / 7 (Compact); on High tier, the art's dominant colour tints the waveform and progress bar |
| Waveform | 4 bars (Compact) / 6 bars (Expanded), 3 px wide, 2.5 px gap, height 4–15 px, rounded caps; follows playback; static on Low tier |
| Progress bar | 4 px tall, fully rounded, track white at 20 %, fill white (or dominant colour); 8 px tall while scrubbing |
| Buttons | 32 × 32 hit areas (44 × 44 in Large); Lucide icons at 20 px, white |
| Time labels | 11 px, tabular figures, white at 60 % |

### Device connected / battery (system activity)

The iPhone's "AirPods connected" moment, for any device that reports a battery level to the OS: headphones, earbuds, mice, keyboards, game controllers, and the laptop itself.

| Trigger | Presentation | Duration |
|---|---|---|
| Device connects | Compact-wide pill (300 × 37): device icon (Lucide `headphones`, `mouse`, `keyboard`, `gamepad-2`, `bluetooth`) on the left; battery ring (Ø 22, 3 px stroke) and `87%` on the right | 3.5 s, then back to the previous state |
| Battery drops to 20 % or 10 % | same, with the ring and text in amber (20 %) or red (10 %) | 5 s |
| Charger plugged in or unplugged (laptop) | same, with the laptop's own battery and a bolt icon when charging | 3 s |
| Hover / click during the peek | Expanded card listing every connected device, with left/right/case levels when the device reports them | until dismissed |

Ring colour: white above 20 %, amber at or below 20 %, red at or below 10 %, green while charging. The percentage uses tabular figures, so the text doesn't jiggle as it changes. Events are rate-limited so a flaky Bluetooth connection can't make the island bounce repeatedly: one peek per device per 30 s.

## 4. Motion

Springs use the iOS parameterisation (`response` = period in seconds, `damping` = damping ratio). They are solved analytically, so they are frame-rate independent and an interrupted animation keeps its velocity (`islet-core::spring`).

| Transition | Response | Damping | Notes |
|---|---|---|---|
| Expand (idle/compact → expanded/large) | 0.50 s | 0.76 | visible overshoot, about 3 % |
| Collapse (→ idle/compact) | 0.42 s | 0.88 | almost no bounce |
| Compact appear/disappear | 0.45 s | 0.80 | width grows from the centre |
| Minimal split/merge | 0.55 s | 0.72 | the circle buds off with a metaball neck that stretches and snaps |
| Hover peek | 0.30 s | 0.80 | |
| Press | 0.20 s | 1.00 | the whole pill scales to 0.97 while held |

**Shape morph.** Width, height, radius and position are separate springs on the same curve, so the outline morphs continuously and never cross-fades. The radius is clamped to `min(w, h) / 2`, so expanded corners flow into capsule ends with no discontinuity.

**Content choreography.**
- *In:* starts 60 ms after the shape. Opacity goes 0 → 1, scale 0.92 → 1, and on High tier a blur of 6 px → 0. Spring: response 0.35, damping 1.0.
- *Out:* finishes within 120 ms of the start of a collapse, so content never spills outside the shrinking shape. Content is also clipped to the current shape every frame.

**Metaball split (Minimal).** The secondary circle starts inside the pill's right end. Its centre moves outward on the split spring. While the gap is small, a neck joins the two shapes: two cubic curves tangent to both outlines, with their width tapering as the distance grows (`islet-render`, Phase 1). The neck snaps when the gap passes 1.2 × the circle radius. Merging is the same in reverse.

**Tiers.**

| | Low | Balanced | High |
|---|---|---|---|
| Frame rate while animating | 30 fps | 60 fps | 60 fps |
| Springs | same curves (sampled at 30 fps) | same | same |
| Content blur-in | no (opacity and scale only) | no | yes |
| Metaball neck | no (the circle slides out) | yes | yes |
| Waveform | static bars | animated, 30 fps | animated, 60 fps, dominant colour |
| Album-art colour | cached only | cached | computed per track, cached |

**Reduced motion** (OS setting or in-app option): every transition becomes a 150 ms opacity cross-fade between the start and end layouts, with no overshoot, no scale and no blur.

## 5. States and input

```
            hover              click / tap               hold ≥ 450 ms
   Idle ───────────▶ Peek ────────────────▶ Expanded ───────────────────▶ Large
    ▲  ◀─────────── (leave)        ▲            │  ▲                         │
    │                              │            │  └──── click / Esc ────────┘
    │  activity starts             │            │
    ├────────────────▶ Compact ────┘ click      │ Esc / click outside / 5 s idle
    │  last ends    ◀──            ◀────────────┘   (back to Compact if an activity is live)
    │
    └── second activity starts ──▶ Minimal (pill + circle) ── click pill / circle ──▶ Expanded (that activity)
```

| Input | Effect |
|---|---|
| Hover | peek (Idle) or slight grow (Compact) |
| Click | expand the current activity; click again or press Esc to collapse |
| Click-and-hold (≥ 450 ms) | Large |
| Scroll or two-finger swipe over the pill | switch to the next or previous activity |
| Click outside | collapse |
| Auto-collapse | 5 s after the pointer leaves an expanded island (never while scrubbing) |
| Global hotkeys (configurable) | toggle expand; hide/show the island |
| Keyboard | when focused (via the hotkey): Tab between controls, Space/Enter to activate, arrows to scrub or change volume, Esc to collapse |

## 6. Accessibility

- An AccessKit tree exposes the island as a group named after the current activity ("Now playing: Song, Artist"), with each control as a labelled button or slider.
- Labels and announcements are localised strings (`t("media.play")`).
- High-contrast mode adds a 1 px white outline at 70 % so the pill stays visible against dark wallpapers.
- Every animation honours reduced motion. Nothing flashes more than 3 times per second.

## 7. Light and dark wallpapers

The pill is opaque black, so it always reads on light wallpapers. On dark wallpapers it blends in like iPhone hardware, which is intended. Users who want it visible can turn on *Subtle outline* (1 px inner highlight, white at 8 %), which is off by default.
