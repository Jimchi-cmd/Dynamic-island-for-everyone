#!/usr/bin/env python3
"""Is the island drawn where it should be? Compares two screenshots (before
Islet started, and while it runs) along the screen's centre column: how many
pixels in the top band (where the pill hangs) turned dark, and how many in a
band lower down (where a vertically flipped frame would put it).

    scripts/png-probe.py before.png after.png [window_height_logical]

Standard library only (CI's macOS Python has no imaging packages).
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat, w = 8, b"", None
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert depth == 8 and ctype in (2, 6) and interlace == 0, (depth, ctype, interlace)
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    raw = zlib.decompress(idat)
    stride = w * bpp
    rows, prev = [], bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, h, bpp, rows


def dark_changed(before, after, x, y0, y1):
    _, _, bpp_b, rb = before
    _, _, bpp_a, ra = after
    count = 0
    for y in range(y0, y1):
        pb = rb[y][x * bpp_b:x * bpp_b + 3]
        pa = ra[y][x * bpp_a:x * bpp_a + 3]
        if max(pa) < 40 and sum(pb) - sum(pa) > 60:
            count += 1
    return count


def main():
    before, after = read_png(sys.argv[1]), read_png(sys.argv[2])
    window_h = float(sys.argv[3]) if len(sys.argv) > 3 else 260.0
    w, h = after[0], after[1]
    # Screen scale: macOS screenshots are in physical pixels.
    scale = max(1, round(w / 1920)) if w >= 2560 else 1
    x = w // 2
    top = dark_changed(before, after, x, 0, int(36 * scale))
    low_from = int((window_h - 40) * scale)
    low = dark_changed(before, after, x, low_from, min(h, int(window_h * scale)))
    print(f"screen {w}x{h} scale~{scale}: dark at the top {top}, dark near the window's bottom {low}")
    print("ISLAND_AT_TOP" if top >= 8 else "ISLAND_NOT_AT_TOP")


if __name__ == "__main__":
    main()
