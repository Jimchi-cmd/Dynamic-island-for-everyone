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
