"""Counts the pixels that differ between two PNG screenshots, using only the standard library.

    python3 scripts/pngdiff.py before.png after.png [threshold]

A pixel differs when any color channel moves by more than `threshold` (default 40). Prints the
count and the bounding box of the differing pixels. Handles the 8 bit RGB and RGBA PNGs
gearbox saves.
"""
import struct, sys, zlib

def load(path):
    data = open(path, 'rb').read()
    i, idat = 8, b''
    while i < len(data):
        n, = struct.unpack('>I', data[i:i + 4])
        kind, chunk = data[i + 4:i + 8], data[i + 8:i + 8 + n]
        i += 12 + n
        if kind == b'IHDR':
            width, height, _, color = struct.unpack('>IIBB', chunk[:10])
            bpp = {6: 4, 2: 3}[color]
        elif kind == b'IDAT':
            idat += chunk
    raw, stride = zlib.decompress(idat), width * bpp
    out, prev = bytearray(), bytearray(stride)
    for y in range(height):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b, c = prev[x], prev[x - bpp] if x >= bpp else 0
            if kind == 1: line[x] = (line[x] + a) & 255
            elif kind == 2: line[x] = (line[x] + b) & 255
            elif kind == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        out += line
        prev = line
    return width, height, bpp, out

w, h, bpp, a = load(sys.argv[1])
w2, h2, bpp2, b = load(sys.argv[2])
if (w, h, bpp) != (w2, h2, bpp2):
    sys.exit(f'sizes differ: {w}x{h} and {w2}x{h2}')
threshold = int(sys.argv[3]) if len(sys.argv) > 3 else 40
bad = [(i // bpp % w, i // bpp // w) for i in range(0, len(a), bpp) if max(abs(a[i + k] - b[i + k]) for k in range(3)) > threshold]
box = (min(x for x, _ in bad), min(y for _, y in bad), max(x for x, _ in bad), max(y for _, y in bad)) if bad else ''
print(len(bad), 'pixels differ', box)
