#!/usr/bin/env python3
"""Build assets/quietmic.res (a Windows resource file holding the app icon) from assets/icon-1024.png.
The linker takes .res files directly, so no resource compiler is needed (works when cross-compiling on a Mac).
usage: make_win_icon.py   (needs macOS `sips` for resizing)"""
import os, struct, subprocess, tempfile

SIZES = [16, 24, 32, 48, 64, 128, 256]
src = "assets/icon-1024.png"
tmp = tempfile.mkdtemp()
pngs = []
for s in SIZES:
    out = os.path.join(tmp, f"{s}.png")
    subprocess.run(["sips", "-z", str(s), str(s), src, "--out", out], check=True, capture_output=True)
    pngs.append(open(out, "rb").read())

def res(rtype, name, data):
    hdr = struct.pack("<HH", 0xFFFF, rtype) + struct.pack("<HH", 0xFFFF, name)
    hdr += struct.pack("<IHHII", 0, 0x1030, 0x0409, 0, 0)  # data version, memory flags, language en-US, version, characteristics
    header_size = 8 + len(hdr)
    blob = struct.pack("<II", len(data), header_size) + hdr + data
    return blob + b"\0" * (-len(blob) % 4)

out = res(0, 0, b"")  # mandatory empty first resource
group = struct.pack("<HHH", 0, 1, len(SIZES))
for i, (s, p) in enumerate(zip(SIZES, pngs), start=1):
    out += res(3, i, p)
    group += struct.pack("<BBBBHHIH", s % 256, s % 256, 0, 0, 1, 32, len(p), i)
out += res(14, 1, group)
open("assets/quietmic.res", "wb").write(out)
print("wrote assets/quietmic.res", len(out), "bytes")
