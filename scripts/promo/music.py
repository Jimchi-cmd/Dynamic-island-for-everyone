#!/usr/bin/env python3
"""An upbeat, original backing track for the promo videos, made from
scratch with numpy: four-on-the-floor kick, claps, hi-hats, a sidechained
bass, bright detuned chords, a plucked melody, a riser and a final hit.

Every sound is synthesised here, so the music is ours to use anywhere.

  python3 scripts/promo/music.py out.wav --seconds 24 --seed 3 --drop 17.5
"""

import argparse
import wave

import numpy as np

SR = 44100

# Chord progressions (semitones from the key's root, as triads).
PROGRESSIONS = [
    [(9, 12, 16), (5, 9, 12), (0, 4, 7), (7, 11, 14)],  # vi IV I V
    [(0, 4, 7), (7, 11, 14), (9, 12, 16), (5, 9, 12)],  # I V vi IV
    [(9, 12, 16), (7, 11, 14), (5, 9, 12), (7, 11, 14)],  # vi V IV V
    [(5, 9, 12), (0, 4, 7), (7, 11, 14), (9, 12, 16)],  # IV I V vi
]


def hz(midi):
    return 440.0 * 2 ** ((midi - 69) / 12)


def env(n, attack, decay, sustain=0.0, curve=4.0):
    """Attack then exponential-ish decay to `sustain`."""
    t = np.arange(n) / SR
    a = np.clip(t / max(attack, 1e-4), 0, 1)
    d = sustain + (1 - sustain) * np.exp(-curve * np.maximum(t - attack, 0) / max(decay, 1e-4))
    return a * d


def saw(freq, n, phase=0.0):
    t = np.arange(n) / SR
    x = (freq * t + phase) % 1.0
    return 2 * x - 1


def lowpass(x, cutoff):
    """A gentle low-pass done in the frequency domain (fine for whole notes)."""
    if len(x) < 8:
        return x
    spec = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    spec *= 1 / np.sqrt(1 + (f / cutoff) ** 4)
    return np.fft.irfft(spec, len(x))


def highpass(x, cutoff):
    spec = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    spec *= 1 / np.sqrt(1 + (cutoff / np.maximum(f, 1)) ** 4)
    return np.fft.irfft(spec, len(x))


def kick(n):
    t = np.arange(n) / SR
    f = 48 + 110 * np.exp(-t * 28)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-t * 7.5)
    click = np.random.default_rng(1).standard_normal(n) * np.exp(-t * 400) * 0.25
    return np.tanh((body + click) * 1.6)


def clap(n, rng):
    noise = highpass(rng.standard_normal(n), 900)
    t = np.arange(n) / SR
    bursts = sum(np.exp(-np.maximum(t - d, 0) * 90) * (t >= d) for d in (0, 0.011, 0.022))
    tail = np.exp(-t * 18) * 0.6
    return noise * np.maximum(bursts, tail) * 0.5


def hat(n, rng, open_=False):
    noise = highpass(rng.standard_normal(n), 7000)
    t = np.arange(n) / SR
    return noise * np.exp(-t * (14 if open_ else 70)) * 0.35


def track(seconds, seed=0, bpm=None, key=None, drop=None, quiet=False):
    """The mix as a stereo float array. `drop`: where the ending section (the Pro card) starts."""
    rng = np.random.default_rng(seed)
    bpm = bpm or [118, 122, 124, 120, 126][seed % 5]
    key = key if key is not None else [57, 60, 55, 62, 59][seed % 5]  # A, C, G, D, B
    prog = PROGRESSIONS[seed % len(PROGRESSIONS)]
    beat = 60.0 / bpm
    n = int(seconds * SR)
    L = np.zeros(n)
    R = np.zeros(n)
    duck = np.ones(n)

    def add(sig, at, gain=1.0, pan=0.0):
        i = int(at * SR)
        if i >= n:
            return
        sig = sig[: n - i] * gain
        L[i : i + len(sig)] += sig * (1 - max(0, pan))
        R[i : i + len(sig)] += sig * (1 + min(0, pan))

    beats = int(seconds / beat) + 1
    intro = 4 if not quiet else 0  # beats before the drums come in
    drop = drop if drop is not None else seconds
    k = kick(int(0.45 * SR))
    for b in range(beats):
        at = b * beat
        bar = b // 4
        if at >= seconds:
            break
        full = b >= intro
        if full and not quiet:
            add(k, at, 0.72)
            # Side-chain: everything else dips under each kick.
            i = int(at * SR)
            m = min(n - i, int(beat * SR))
            duck[i : i + m] = np.minimum(duck[i : i + m], 1 - 0.55 * np.exp(-np.arange(m) / SR * 9))
            if b % 4 in (1, 3):
                add(clap(int(0.3 * SR), rng), at, 0.55)
            for s in range(2 if quiet else 4):
                hat_at = at + s * beat / 4
                if s == 2:
                    add(hat(int(0.25 * SR), rng, open_=True), hat_at, 0.32, pan=0.3)
                else:
                    add(hat(int(0.06 * SR), rng), hat_at, 0.3 + 0.08 * (s % 2), pan=-0.25)
        # Chords: one per bar, detuned saws through a low-pass, wide.
        if b % 4 == 0:
            chord = prog[bar % len(prog)]
            dur = int(4 * beat * SR)
            pad_l = np.zeros(dur)
            pad_r = np.zeros(dur)
            for note in chord:
                f = hz(key + note)
                for det, side in ((-0.12, 0), (0.0, 2), (0.12, 1)):
                    v = saw(f * 2 ** (det / 12), dur, rng.random())
                    if side in (0, 2):
                        pad_l += v
                    if side in (1, 2):
                        pad_r += v
            shape = env(dur, 0.02 if full else 0.6, 4 * beat, sustain=0.55, curve=1.5)
            cut = 5200 if full else 1400
            add(lowpass(pad_l * shape, cut), at, 0.12, pan=-0.6)
            add(lowpass(pad_r * shape, cut), at, 0.12, pan=0.6)
            # Bass: the root, on the off-beats (a driving pulse).
            if full:
                root = hz(key + chord[0] - 24)
                for e in range(8):
                    if e % 2 == 0 and e != 0:
                        continue
                    blen = int(beat / 2 * SR)
                    t = np.arange(blen) / SR
                    tone = np.sin(2 * np.pi * root * t) + 0.35 * saw(root, blen)
                    add(lowpass(tone * env(blen, 0.005, beat / 2, 0.3), 900), at + e * beat / 2, 0.3)
        # The melody: a plucked arpeggio of the chord, 16ths, bright and bouncy.
        if full and b * beat < drop:
            chord = prog[bar % len(prog)]
            pattern = [0, 1, 2, 1, 2, 0, 1, 2] if (bar % 2 == 0) else [2, 1, 0, 1, 2, 2, 1, 0]
            for s in range(2):
                note = chord[pattern[(b % 4) * 2 + s]] + 12
                plen = int(0.18 * SR)
                f = hz(key + note)
                pl = (np.sign(np.sin(2 * np.pi * f * np.arange(plen) / SR)) * 0.5 + saw(f * 2, plen) * 0.5)
                pl = lowpass(pl * env(plen, 0.002, 0.12, 0, curve=3), 7000)
                add(pl, at + s * beat / 2, 0.2, pan=0.35 if s else -0.35)
    # A riser into the ending, then a hit on it.
    if drop < seconds - 0.5 and not quiet:
        rise = 2.0
        rn = int(rise * SR)
        t = np.arange(rn) / SR
        noise = highpass(rng.standard_normal(rn), 2000) * (t / rise) ** 2 * 0.28
        sweep = np.sin(2 * np.pi * np.cumsum(300 + 2500 * (t / rise) ** 2) / SR) * (t / rise) ** 3 * 0.06
        add(noise + sweep, max(0, drop - rise), 1.0)
        hit_n = int(2.5 * SR)
        th = np.arange(hit_n) / SR
        boom = np.sin(2 * np.pi * (40 + 60 * np.exp(-th * 10)) * th) * np.exp(-th * 2.2)
        crash = highpass(rng.standard_normal(hit_n), 5000) * np.exp(-th * 1.6) * 0.25
        add(np.tanh(boom * 1.5) * 0.9 + crash, drop, 1.0)
    L *= duck
    R *= duck
    # Fade in, fade out, gentle glue (soft clip) and level.
    fade_in = int(0.25 * SR)
    fade_out = int(1.2 * SR)
    for ch in (L, R):
        ch[:fade_in] *= np.linspace(0, 1, fade_in)
        ch[-fade_out:] *= np.linspace(1, 0, fade_out) ** 1.5
    mix = np.stack([L, R], axis=1)
    mix = np.tanh(mix * 1.3)
    peak = np.max(np.abs(mix)) or 1
    return mix / peak * (0.5 if quiet else 0.89)


def write_wav(path, mix):
    data = (np.clip(mix, -1, 1) * 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--seconds", type=float, default=24)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--drop", type=float)
    ap.add_argument("--quiet", action="store_true", help="a calm bed for the tutorial")
    a = ap.parse_args()
    write_wav(a.out, track(a.seconds, a.seed, drop=a.drop, quiet=a.quiet))
    print(f"wrote {a.out}")


if __name__ == "__main__":
    main()


# ---- An upbeat track: 170 BPM breakbeat, stabs and a lead hook -------------------------------

SCALE = [0, 2, 4, 5, 7, 9, 11]  # major


def degree(key, d):
    """The scale degree d (0 = root; may be negative or above 7) as a MIDI note."""
    octave, step = divmod(d, 7)
    return key + 12 * octave + SCALE[step]


def supersaw(f, n, rng, voices=5, spread=0.18):
    out = np.zeros(n)
    for v in range(voices):
        det = (v - (voices - 1) / 2) / ((voices - 1) / 2) * spread
        out += saw(f * 2 ** (det / 12), n, rng.random())
    return out / voices


def banger(seconds, seed=0, bars_intro=2, sections=(), bpm=170, key=None):
    """An energetic, original track. `sections`: bar numbers where a new part starts
    (a crash and a riser into each). The first `bars_intro` bars build up."""
    rng = np.random.default_rng(seed + 100)
    key = key if key is not None else [57, 60, 62, 55, 59][seed % 5]  # A, C, D, G, B (major)
    prog = [(5, 7, 9), (3, 5, 7), (0, 2, 4), (4, 6, 8)]  # vi IV I V as scale-degree triads
    beat = 60.0 / bpm
    bar = 4 * beat
    step = beat / 4
    n = int(seconds * SR)
    L = np.zeros(n)
    R = np.zeros(n)
    duck = np.ones(n)

    def add(sig, at, gain=1.0, pan=0.0):
        i = int(at * SR)
        if i >= n or i < 0:
            return
        sig = sig[: n - i] * gain
        L[i : i + len(sig)] += sig * (1 - max(0, pan))
        R[i : i + len(sig)] += sig * (1 + min(0, pan))

    bars = int(seconds / bar) + 1
    k = kick(int(0.4 * SR))
    snare_n = int(0.25 * SR)

    def snare():
        t = np.arange(snare_n) / SR
        body = np.sin(2 * np.pi * 190 * t) * np.exp(-t * 30) * 0.6
        noise = lowpass(highpass(rng.standard_normal(snare_n), 1200), 6000) * np.exp(-t * 22) * 0.7
        return np.tanh((body + noise) * 1.4)

    sn = snare()
    # The hook: two bars of scale degrees per step (None = rest), over the chords.
    hooks = [
        [7, None, 9, 7, None, 6, 7, None, 9, None, 11, 9, None, 7, 6, None,
         4, None, 6, 4, None, 2, 4, None, 6, None, 7, 6, None, 4, 2, None],
        [9, 9, None, 11, None, 9, 7, None, 6, None, 7, None, 9, None, 7, None,
         4, 4, None, 6, None, 7, 9, None, 7, None, 6, None, 4, None, 2, None],
        [11, None, 9, None, 7, None, 9, 11, None, 12, None, 11, 9, None, 7, None,
         7, None, 6, None, 4, None, 6, 7, None, 9, None, 7, 6, None, 4, None],
    ]
    hook = hooks[seed % len(hooks)]
    for b in range(bars):
        at = b * bar
        if at >= seconds:
            break
        building = b < bars_intro
        chord = prog[b % 4]
        root = degree(key, chord[0]) - 12
        # Drums: a breakbeat (kick on 1 and the "and" of 3, snare on 2 and 4), hats in 8ths.
        if not building:
            for s, g in ((0, 1.0), (10, 0.85)):
                add(k, at + s * step, 0.55 * g)
                i = int((at + s * step) * SR)
                m = min(n - i, int(beat * 1.2 * SR))
                if m > 0:
                    duck[i : i + m] = np.minimum(duck[i : i + m], 1 - 0.6 * np.exp(-np.arange(m) / SR * 10))
            for s in (4, 12):
                add(sn, at + s * step, 0.62)
            add(sn, at + 7 * step, 0.12)  # a ghost note
            for s in range(0, 16, 2):
                add(hat(int((0.18 if s % 4 == 2 else 0.05) * SR), rng, open_=s % 4 == 2), at + s * step, 0.13 if s % 4 else 0.1, pan=0.25)
            for s in range(1, 16, 2):
                add(hat(int(0.03 * SR), rng), at + s * step, 0.06, pan=-0.25)
        else:
            # The build: a snare roll speeding up through the last intro bar.
            if b == bars_intro - 1:
                for s in range(16):
                    add(sn, at + s * step, 0.15 + 0.4 * s / 15)
            add(k, at, 0.5)
        # Bass: a driving 8th-note pulse on the root, ducked under the kick.
        if not building:
            for e in range(8):
                blen = int(beat / 2 * SR * 0.9)
                f = hz(root - 12)
                t = np.arange(blen) / SR
                tone = 0.6 * np.sin(2 * np.pi * f * t) + 0.8 * lowpass(supersaw(f * 2, blen, rng, 3, 0.25), 1100)
                add(tone * env(blen, 0.004, beat / 2, 0.5), at + e * beat / 2, 0.3)
        # Chord stabs on the off-beats: bright, wide, pumping.
        for s in (2, 6, 10, 14) if not building else (0,):
            dur = int((step * 1.6 if not building else bar) * SR)
            stab_l = np.zeros(dur)
            stab_r = np.zeros(dur)
            for d in chord:
                f = hz(degree(key, d))
                stab_l += supersaw(f, dur, rng, 4, 0.2)
                stab_r += supersaw(f * 1.002, dur, rng, 4, 0.2)
            shape = env(dur, 0.003, step * 1.2 if not building else bar, 0.0 if not building else 0.6, curve=3)
            cut = 3800 if not building else 900 + 2600 * (b / max(1, bars_intro))
            add(lowpass(stab_l * shape, cut), at + s * step, 0.2, pan=-0.55)
            add(lowpass(stab_r * shape, cut), at + s * step, 0.2, pan=0.55)
        # A warm pad under it all: the chord an octave down, for body.
        if not building:
            dur = int(bar * SR)
            pad = np.zeros(dur)
            for d in chord:
                pad += supersaw(hz(degree(key, d) - 12), dur, rng, 3, 0.15)
            add(lowpass(pad * env(dur, 0.05, bar, 0.8, 1), 1200), at, 0.18)
        # The lead hook (not during the build): a plucky saw, an octave doubled.
        if not building:
            phrase = hook[(b % 2) * 16 : (b % 2) * 16 + 16]
            for s, d in enumerate(phrase):
                if d is None:
                    continue
                f = hz(degree(key, d) + 12)
                ln = int(step * 1.8 * SR)
                t = np.arange(ln) / SR
                vib = 1 + 0.004 * np.sin(2 * np.pi * 5.5 * t)
                tone = saw(f, ln) * 0.6 + np.sign(np.sin(2 * np.pi * f * 2 * t * vib)) * 0.25
                add(lowpass(tone * env(ln, 0.003, step * 1.4, 0.15, curve=3), 4200), at + s * step, 0.36, pan=0.15 if s % 2 else -0.15)
            # A quiet 16th arpeggio behind it.
            for s in range(16):
                d = chord[s % 3] + (7 if s % 6 >= 3 else 0)
                ln = int(step * 0.9 * SR)
                add(lowpass(saw(hz(degree(key, d)), ln) * env(ln, 0.002, step * 0.8, 0, 4), 5000), at + s * step, 0.08, pan=0.6 if s % 2 else -0.6)
    # Risers into each section, and a crash on it.
    for sec in (bars_intro,) + tuple(sections):
        s_at = sec * bar
        rise = bar
        rn = int(rise * SR)
        t = np.arange(rn) / SR
        noise = highpass(rng.standard_normal(rn), 1800) * (t / rise) ** 2.5 * 0.3
        add(noise, s_at - rise, 1.0)
        cn = int(2.0 * SR)
        tc = np.arange(cn) / SR
        crash = highpass(rng.standard_normal(cn), 4500) * np.exp(-tc * 1.8) * 0.3
        boom = np.tanh(np.sin(2 * np.pi * (45 + 70 * np.exp(-tc * 9)) * tc) * np.exp(-tc * 3) * 1.6) * 0.7
        add(crash + boom, s_at, 1.0)
    L *= duck
    R *= duck
    fade_out = int(1.0 * SR)
    for ch in (L, R):
        ch[: int(0.05 * SR)] *= np.linspace(0, 1, int(0.05 * SR))
        ch[-fade_out:] *= np.linspace(1, 0, fade_out) ** 1.5
    mix = np.tanh(np.stack([L, R], axis=1) * 1.6)
    return mix / (np.max(np.abs(mix)) or 1) * 0.9
