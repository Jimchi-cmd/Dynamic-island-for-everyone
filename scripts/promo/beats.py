#!/usr/bin/env python3
"""Find a song's beats, bars and loud parts, so a video can move with it.

  python3 scripts/promo/beats.py song.mp3 [--json out.json]

Prints the tempo, where the bars start, and each bar's loudness; the
reels use the same analysis (analyse()) to cut, punch and glow on the
music. Needs numpy and ffmpeg.
"""

import argparse
import json
import subprocess
import sys

import numpy as np

SR = 22050
HOP = 256


def load(path):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "s16le", "-"],
                         check=True, capture_output=True).stdout
    return np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768


def spectra(x, n=2048):
    win = np.hanning(n).astype(np.float32)
    frames = np.lib.stride_tricks.sliding_window_view(np.pad(x, (n // 2, n // 2)), n)[::HOP]
    return np.abs(np.fft.rfft(frames * win, axis=1))


def onset_envelope(mag, lo=None, hi=None):
    f = np.fft.rfftfreq((mag.shape[1] - 1) * 2, 1 / SR)
    band = mag if lo is None else mag[:, (f >= lo) & (f < (hi or SR))]
    logm = np.log1p(band * 10)
    flux = np.maximum(logm[1:] - logm[:-1], 0).sum(axis=1)
    flux = np.concatenate([[0], flux])
    # Remove the slow trend, keep the hits.
    k = int(0.4 * SR / HOP)
    trend = np.convolve(flux, np.ones(k) / k, mode="same")
    env = np.maximum(flux - trend, 0)
    return env / (env.max() or 1)


def tempo(env, lo=80, hi=180, prior=125):
    fps = SR / HOP
    ac = np.correlate(env, env, "full")[len(env) - 1:]
    best, score = None, -1
    for bpm10 in range(lo * 10, hi * 10 + 1, 2):
        bpm = bpm10 / 10
        lag = fps * 60 / bpm
        s = sum(np.interp(lag * m, np.arange(len(ac)), ac) * w for m, w in ((1, 1.0), (2, 0.5), (4, 0.25)))
        # A gentle preference for the usual pop range.
        s *= np.exp(-0.5 * (np.log2(bpm / prior) / 0.9) ** 2)
        if s > score:
            best, score = bpm, s
    return best


def track_beats(env, bpm, tightness=120):
    """Dynamic-programming beat tracker (after Ellis 2007)."""
    fps = SR / HOP
    period = fps * 60 / bpm
    n = len(env)
    score = env.copy()
    back = -np.ones(n, dtype=int)
    lo, hi = int(round(period / 2)), int(round(period * 2))
    for t in range(hi, n):
        prev = np.arange(t - hi, t - lo)
        cost = -tightness * (np.log((t - prev) / period)) ** 2
        cand = score[prev] + cost
        i = int(np.argmax(cand))
        score[t] = env[t] + cand[i]
        back[t] = prev[i]
    # Start from the best ending near the end, follow it back.
    t = int(np.argmax(score[-int(period):])) + n - int(period)
    beats = []
    while t >= 0:
        beats.append(t)
        t = back[t]
    return np.array(beats[::-1]) / fps


def analyse(path):
    x = load(path)
    mag = spectra(x)
    env = onset_envelope(mag)
    low = onset_envelope(mag, 30, 160)   # kicks
    high = onset_envelope(mag, 2500, 9000)  # snares, claps, hats
    bpm = tempo(env)
    beats = track_beats(env, bpm)
    fps = SR / HOP
    # Downbeats: the bar phase whose beats carry the most low-end hits.
    idx = np.clip((beats * fps).astype(int), 0, len(low) - 1)
    phase = max(range(4), key=lambda p: low[idx[p::4]].mean())
    downbeats = beats[phase::4]
    # Loudness per bar (RMS), and the whole-song loudness envelope at 30 fps.
    rms = np.sqrt(np.convolve(x ** 2, np.ones(int(0.05 * SR)) / int(0.05 * SR), mode="same"))
    bar_loud = []
    for a, b in zip(downbeats, np.append(downbeats[1:], len(x) / SR)):
        bar_loud.append(float(rms[int(a * SR):int(b * SR)].mean()))
    vfps = 30
    t = np.arange(0, len(x) / SR, 1 / vfps)
    frame_of = np.clip((t * fps).astype(int), 0, len(env) - 1)
    return dict(
        bpm=float(bpm),
        duration=len(x) / SR,
        beats=[float(b) for b in beats],
        downbeats=[float(b) for b in downbeats],
        bar_loudness=bar_loud,
        # Per video frame: overall hits, kick hits, snare/hat hits, loudness (0..1).
        hit=[float(v) for v in env[frame_of]],
        kick=[float(v) for v in low[frame_of]],
        snap=[float(v) for v in high[frame_of]],
        loud=[float(v) for v in (rms[(t * SR).astype(int).clip(0, len(rms) - 1)] / (rms.max() or 1))],
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("song")
    ap.add_argument("--json")
    a = ap.parse_args()
    r = analyse(a.song)
    print(f"tempo {r['bpm']:.1f} BPM, {len(r['beats'])} beats, {len(r['downbeats'])} bars, {r['duration']:.1f} s")
    loud = np.array(r["bar_loudness"])
    for i, (d, l) in enumerate(zip(r["downbeats"], loud)):
        print(f"bar {i:3d} at {d:7.2f} s  {'#' * int(40 * l / loud.max())}")
    if a.json:
        with open(a.json, "w") as f:
            json.dump(r, f)
    return 0


if __name__ == "__main__":
    sys.exit(main())
