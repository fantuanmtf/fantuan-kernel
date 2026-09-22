#!/usr/bin/env bash
# M13-1 graphics smoke: the shared framebuffer core drives both consoles.
#   x86_64 UEFI:  boot with -vga std, take a headless screendump, assert
#                 non-blank pixels + the damage self-test on serial.
#   i686 VBE:      boot the BIOS image with -vga std, take a headless
#                 screendump, assert non-blank pixels + the fb: lines.
#   serial parity: the i686 serial log with -vga none is byte-identical to
#                 the VBE run once the fb: lines are stripped.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (graphics): $*"; exit 1; }

# CONFIG_GRAPHICS is enabled by the rescue profile (M12-6 / M13-1).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "x86_64 build"

pixel_stats() { # ppm -> "WxH bright=N"
  python3 - "$1" <<'PY'
import sys
data = open(sys.argv[1], 'rb').read()
assert data[:2] == b'P6', 'not a P6 PPM'
parts = data.split(b'\n', 3)
w, h = map(int, parts[1].split())
px = parts[3]
if px.startswith(b'#'):
    px = px.split(b'\n', 1)[1]
bright = 0
for y in range(0, h, 4):
    for x in range(0, w, 4):
        off = (y * w + x) * 3
        r, g, b = px[off], px[off + 1], px[off + 2]
        if r > 100 and g > 100 and b > 100:
            bright += 1
print(f"{w}x{h} bright={bright}")
PY
}

mon() { # socket command
  printf '%s\n' "$2" | timeout 10 socat - UNIX-CONNECT:"$1" >/dev/null 2>&1
}

OK=1
note() { echo "missing: $1"; OK=0; }

# --- x86_64 UEFI GOP screendump ---------------------------------------------
rm -f build/mon.sock build/gfx-uefi.log build/gfx-uefi.ppm
(
  for _ in $(seq 1 120); do
    [ -S build/mon.sock ] && grep -q "shell: ready" build/gfx-uefi.log 2>/dev/null && break
    sleep 1
  done
  sleep 3
  mon build/mon.sock "screendump build/gfx-uefi.ppm"
  sleep 2
  mon build/mon.sock "quit"
) &
SENDER=$!
timeout --signal=KILL 120 ./tools/run.sh --vga std --monitor \
  < /dev/null > build/gfx-uefi.log 2>&1 || true
wait "$SENDER" 2>/dev/null || true

grep -q "graphics: damage self-test ok" build/gfx-uefi.log || note "damage self-test"
grep -q "shell: ready" build/gfx-uefi.log || note "shell ready"
grep -qE "panic|PANIC" build/gfx-uefi.log && { echo "panic in the UEFI log"; OK=0; }
if [ -s build/gfx-uefi.ppm ]; then
  UEFI_STATS="$(pixel_stats build/gfx-uefi.ppm)"
  echo "uefi screendump: $UEFI_STATS"
  BRIGHT="${UEFI_STATS##*bright=}"
  [ "$BRIGHT" -gt 200 ] || note "uefi screendump non-blank (bright=$BRIGHT)"
else
  note "uefi screendump produced"
fi

# --- i686 VBE screendump -----------------------------------------------------
./tools/build-bios.sh --arch i686 >/dev/null 2>&1 || fail "i686 bios build"
rm -f build/gfx-mon.sock build/gfx-vbe.log build/gfx-vbe.ppm
(
  for _ in $(seq 1 60); do
    grep -q "i686: M10-4b2a interrupts complete" build/gfx-vbe.log 2>/dev/null && break
    sleep 1
  done
  sleep 2
  mon build/gfx-mon.sock "screendump build/gfx-vbe.ppm"
  sleep 2
  mon build/gfx-mon.sock "quit"
) &
SENDER=$!
timeout --signal=KILL 40 qemu-system-x86_64 -machine pc -m 512M \
  -drive format=raw,file=build/bios-i686.img \
  -vga std -display none \
  -serial file:build/gfx-vbe.log \
  -monitor "unix:build/gfx-mon.sock,server,nowait" \
  -no-reboot < /dev/null > build/gfx-vbe-qemu.log 2>&1 || true
wait "$SENDER" 2>/dev/null || true

grep -q "fb: 1024x768x32 pitch=4096 at 0xBFC00000" build/gfx-vbe.log || note "vbe geometry line"
grep -q "fb: console up" build/gfx-vbe.log || note "vbe console up"
grep -q "handshake ok: arch=3" build/gfx-vbe.log || note "i686 handshake"
if [ -s build/gfx-vbe.ppm ]; then
  VBE_STATS="$(pixel_stats build/gfx-vbe.ppm)"
  echo "vbe screendump: $VBE_STATS"
  BRIGHT="${VBE_STATS##*bright=}"
  [ "$BRIGHT" -gt 1000 ] || note "vbe screendump non-blank (bright=$BRIGHT)"
  DIMS="${VBE_STATS%% *}"
  [ "$DIMS" = "1024x768" ] || note "vbe screendump dimensions ($DIMS)"
else
  note "vbe screendump produced"
fi

# --- i686 serial parity: -vga none must only drop the fb: lines --------------
rm -f build/gfx-none.log
timeout --signal=KILL 40 qemu-system-x86_64 -machine pc -m 512M \
  -drive format=raw,file=build/bios-i686.img \
  -vga none -display none \
  -serial file:build/gfx-none.log \
  -no-reboot < /dev/null > build/gfx-none-qemu.log 2>&1 || true
grep -q "fb: unavailable (serial console)" build/gfx-none.log || note "vbe fallback line"
grep -v 'fb:' build/gfx-vbe.log > build/gfx-vbe-nofb.txt
grep -v 'fb:' build/gfx-none.log > build/gfx-none-nofb.txt
if diff -q build/gfx-vbe-nofb.txt build/gfx-none-nofb.txt >/dev/null; then
  echo "serial parity: identical after fb: lines"
else
  note "serial parity (diff after fb: lines)"
  diff build/gfx-vbe-nofb.txt build/gfx-none-nofb.txt | head -10 || true
fi

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (graphics: GOP + VBE screendumps non-blank, damage self-test, serial parity)"
  grep -aE "graphics: damage self-test|fb: (1024|console up|unavailable)" \
    build/gfx-uefi.log build/gfx-vbe.log build/gfx-none.log | head -8
  exit 0
fi
echo "SMOKE FAIL (graphics)"
exit 1
