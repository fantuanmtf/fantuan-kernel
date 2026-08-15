#!/usr/bin/env bash
# Regenerate kernel/src/font.rs from NetBSD's Spleen 8x16 (BSD-2-Clause).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
URL="https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/spleen8x16.h"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP"

python3 - "$TMP" "$ROOT/kernel/src/font.rs" <<'PY'
import re
import sys

src = open(sys.argv[1]).read()
m = re.search(r"spleen8x16_data[] = {(.*?)};", src, re.S)
assert m, "font data array not found"
vals = [int(x, 16) for x in re.findall(r"0x[0-9a-fA-F]{2}", m.group(1))]
assert len(vals) == (256 - 32) * 16, f"unexpected glyph count: {len(vals)}"

out = []
out.append("//! Embedded 8x16 console font: Spleen by Frederic Cambus (c) 2018-2020,")
out.append("//! BSD-2-Clause license. Generated from NetBSD")
out.append("//! sys/dev/wsfont/spleen8x16.h by tools/gen_font.sh — do not edit by hand.")
out.append("//! One byte per row, MSB = leftmost pixel; glyphs 0x20..=0xFF.")
out.append("")
out.append("pub const FONT_8X16: [[u8; 16]; 256] = [")
for g in range(32):
    out.append("    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,")
    out.append(f"     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 0x{g:02X}")
for i in range(224):
    row = vals[i * 16 : (i + 1) * 16]
    hexs = ", ".join(f"0x{b:02X}" for b in row)
    out.append(f"    [{hexs}], // 0x{i + 32:02X}")
out.append("];")
open(sys.argv[2], "w").write("
".join(out) + "
")
print(f"font.rs written: {len(vals)} bytes")
PY
