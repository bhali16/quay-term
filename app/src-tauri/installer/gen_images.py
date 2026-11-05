#!/usr/bin/env python3
"""Regenerates the NSIS installer artwork (header.bmp, sidebar.bmp) in the app icon's
palette, with only the standard library. Run: python3 gen_images.py"""
import math, os, struct

OUT = os.path.dirname(os.path.abspath(__file__))
TOP, BOT = (0x27, 0x32, 0x5C), (0x10, 0x15, 0x30)
TEAL, SKY = (0x5E, 0xEA, 0xD4), (0x38, 0xBD, 0xF8)
SS = 3  # supersampling per axis


def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def seg_dist(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0, min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def rrect(px, py, x, y, w, h, r):
    qx, qy = abs(px - (x + w / 2)) - (w / 2 - r), abs(py - (y + h / 2)) - (h / 2 - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def mark(w, h, ox, oy, s):
    """Returns a shader for the icon mark (chevron, cursor, wave) in a 1024 box scaled by s."""
    def at(x, y):
        u, v = (x - ox) / s, (y - oy) / s
        col, a = None, 0.0
        # water line
        wave = abs(v - (830 + 22 * math.sin((u - 170) / 171 * math.pi * 2))) - 15
        if 170 <= u <= 854 and wave < 0:
            col, a = SKY, 0.5
        # cursor
        if rrect(u, v, 580, 648, 190, 84, 42) < 0:
            col, a = SKY, 1.0
        # chevron
        d = min(seg_dist(u, v, 290, 330, 520, 510), seg_dist(u, v, 520, 510, 290, 690))
        if d < 42:
            col, a = lerp(TEAL, SKY, max(0, min(1, (u - 290) / 230))), 1.0
        return col, a
    return at


def render(w, h, shade, path):
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            r = g = b = 0.0
            for sy in range(SS):
                for sx in range(SS):
                    c = shade(x + (sx + .5) / SS, y + (sy + .5) / SS)
                    r += c[0]; g += c[1]; b += c[2]
            n = SS * SS
            row += bytes((round(b / n), round(g / n), round(r / n)))
        row += b"\0" * (-len(row) % 4)
        rows.append(bytes(row))
    data = b"".join(reversed(rows))
    hdr = b"BM" + struct.pack("<IHHI", 54 + len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(data), 2835, 2835, 0, 0)
    with open(path, "wb") as f:
        f.write(hdr + data)


def compose(w, h, mk, glow):
    def shade(x, y):
        base = lerp(TOP, BOT, y / h)
        gx, gy = glow
        k = max(0, 1 - math.hypot(x - gx, y - gy) / (0.9 * max(w, h)))
        base = lerp(base, (0x2B, 0x6F, 0x9A), 0.22 * k * k)
        col, a = mk(x, y)
        return lerp(base, col, a) if col else base
    return shade


# Welcome/finish pages: 164x314
render(164, 314, compose(164, 314, mark(164, 314, 22, 70, 0.12), (30, 40)),
       os.path.join(OUT, "sidebar.bmp"))
# Page header: 150x57
render(150, 57, compose(150, 57, mark(150, 57, 94, 0, 0.055), (130, 10)),
       os.path.join(OUT, "header.bmp"))
