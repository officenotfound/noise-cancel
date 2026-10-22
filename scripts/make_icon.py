#!/usr/bin/env python3
"""Draw the QuietMic app icon (a Windows 95-style speaker on a bevelled grey tile) as 1024x1024 PNG, pure Python."""
import struct, sys, zlib

N = 32                      # pixel-art grid, upscaled with nearest neighbour for a crisp retro look
FACE, HI, SH, DK = (192, 192, 192), (255, 255, 255), (128, 128, 128), (0, 0, 0)
NAVY, TEAL, RED = (0, 0, 128), (0, 128, 128), (255, 0, 0)
g = [[TEAL] * N for _ in range(N)]      # teal "desktop"

def rect(x0, y0, x1, y1, c):
    for y in range(y0, y1):
        for x in range(x0, x1):
            g[y][x] = c

# raised grey window tile
rect(2, 2, 30, 30, FACE)
rect(2, 2, 30, 3, HI); rect(2, 2, 3, 30, HI)
rect(2, 29, 30, 30, DK); rect(29, 2, 30, 30, DK)
rect(3, 28, 29, 29, SH); rect(28, 3, 29, 29, SH)
# navy title bar
rect(4, 4, 28, 8, NAVY)
rect(24, 5, 27, 7, FACE)
# speaker body and cone
rect(7, 14, 11, 21, DK)
for i in range(6):
    rect(11 + i, 14 - i, 12 + i, 21 + i, DK)
# sound waves
for r, col in ((3, DK), (6, DK)):
    for dy in range(-r, r + 1):
        x = 19 + r - abs(dy) // 2 - (1 if r == 3 else 0)
        y = 17 + dy
        if 9 <= y < 27 and x < 28:
            g[y][x] = col
# red record dot
rect(22, 23, 26, 27, RED)

scale = 1024 // N
rows = []
for y in range(N):
    row = bytearray()
    for x in range(N):
        row += bytes(g[y][x]) * scale
    rows += [b"\x00" + bytes(row)] * scale

def chunk(t, d):
    c = struct.pack(">I", len(d)) + t + d
    return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1024, 1024, 8, 2, 0, 0, 0)) + \
      chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1] if len(sys.argv) > 1 else "icon-1024.png", "wb").write(png)
