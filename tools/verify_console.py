#!/usr/bin/env python3
"""Verify the GOP framebuffer console: parse a PPM screendump and render the
text region as ASCII art (white glyph pixels on black background)."""
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "build/screen.ppm"
rows = int(sys.argv[2]) if len(sys.argv) > 2 else 14
sx = int(sys.argv[3]) if len(sys.argv) > 3 else 4   # horizontal sample
sy = int(sys.argv[4]) if len(sys.argv) > 4 else 8   # vertical sample

with open(path, "rb") as f:
    data = f.read()

assert data[:2] == b"P6", "not a PPM P6 file"
parts = data.split(b"\n", 3)
w, h = map(int, parts[1].split())
pixels = parts[3]
# skip possible single comment line
if pixels.startswith(b"#"):
    pixels = pixels.split(b"\n", 1)[1]

print(f"image {w}x{h}, {len(pixels)} bytes, expect {w*h*3}")
assert len(pixels) >= w * h * 3, "truncated PPM"

def px(x, y):
    off = (y * w + x) * 3
    return pixels[off], pixels[off + 1], pixels[off + 2]

# Render the top-left console text region as ASCII art.
for ty in range(rows):
    line = []
    for tx in range(w // sx):
        y = ty * sy
        x = tx * sx
        r, g, b = px(x, y)
        bright = r > 100 and g > 100 and b > 100
        line.append("#" if bright else " ")
    print("".join(line).rstrip())

# Pixel statistics: how many bright pixels total (text should be a small %).
bright = 0
for y in range(0, h, 4):
    for x in range(0, w, 4):
        r, g, b = px(x, y)
        if r > 100 and g > 100 and b > 100:
            bright += 1
total = (w // 4) * (h // 4)
print(f"bright pixels: {bright}/{total} ({100.0 * bright / total:.2f}%)")
