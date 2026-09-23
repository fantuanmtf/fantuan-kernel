#!/usr/bin/env bash
# M13-3 input smoke: the input event ring, the PS/2 mouse and the demo's
# pointer/key consumers.
#   x86_64 UEFI: boot with -vga std, assert the ring self-test and mouse init
#                 markers, inject `mouse_move 12 5` over the QEMU monitor and
#                 assert the demo consumed it and moved its cursor sprite to
#                 (28,699) (QEMU inverts the PS/2 Y axis); screendumps
#                 before/after must differ and their diff must stay inside the
#                 reported damage. `sendkey q` must stop the demo early.
# Timing is deterministic: the cursor marker appears only after the pointer
# event is consumed, and the key-stop marker only after `q` reaches the ring.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (input): $*"; exit 1; }

# CONFIG_GRAPHICS is enabled by the rescue profile (M13-1/M13-3).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "x86_64 build"

# A and B differ, and the diff bounding box must lie inside the union of every
# reported per-frame damage rect (the cursor + box drawing never escapes the
# damage tracker).
diff_contained() { # a.ppm b.ppm log
  python3 - "$1" "$2" "$3" <<'PY'
import re, sys
def read_ppm(p):
    data = open(p, 'rb').read()
    assert data[:2] == b'P6', 'not a P6 PPM'
    parts = data.split(b'\n', 3)
    w, h = map(int, parts[1].split())
    px = parts[3]
    if px.startswith(b'#'):
        px = px.split(b'\n', 1)[1]
    return w, h, px
w, h, a = read_ppm(sys.argv[1])
w2, h2, b = read_ppm(sys.argv[2])
assert (w, h) == (w2, h2), 'screendump size mismatch'
log = open(sys.argv[3], 'rb').read()
pat = re.compile(rb'graphics: demo frame=(\d+) dmg=(\d+),(\d+),(\d+),(\d+)')
rects = [tuple(map(int, m.groups()[1:])) for m in pat.finditer(log)]
if not rects:
    print("damage: no rects reported"); sys.exit(1)
ux = min(r[0] for r in rects); uy = min(r[1] for r in rects)
ux2 = max(r[0] + r[2] for r in rects); uy2 = max(r[1] + r[3] for r in rects)
minx = miny = 10**9; maxx = maxy = -1; n = 0
for y in range(h):
    for x in range(w):
        o = (y * w + x) * 3
        if a[o:o + 3] != b[o:o + 3]:
            n += 1
            minx = min(minx, x); maxx = max(maxx, x)
            miny = min(miny, y); maxy = max(maxy, y)
if n == 0:
    print("diff: images identical"); sys.exit(1)
dx = maxx - minx + 1; dy = maxy - miny + 1
inside = minx >= ux and miny >= uy and minx + dx <= ux2 and miny + dy <= uy2
print(f"diff: {n} pixels bbox={minx},{miny},{dx},{dy} damage={ux},{uy},{ux2-ux},{uy2-uy} "
      f"{'contained' if inside else 'OUTSIDE'}")
sys.exit(0 if inside else 1)
PY
}

mon() { # socket command
  printf '%s\n' "$2" | timeout 10 socat -t 1 - UNIX-CONNECT:"$1" >/dev/null 2>&1
}

OK=1
note() { echo "missing: $1"; OK=0; }

CONTAINED=0
for attempt in 1 2 3; do
  rm -f build/mon.sock build/in-serial.sock build/in.log build/in-a.ppm build/in-b.ppm
  socat -u UNIX-LISTEN:build/in-serial.sock,unlink-early OPEN:build/in.log,creat,trunc &
  RELAY=$!
  (
    for _ in $(seq 1 300); do
      [ -S build/mon.sock ] && grep -qF "graphics: demo frame=2 dmg=" build/in.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/in-a.ppm"
    mon build/mon.sock "mouse_move 12 5"
    for _ in $(seq 1 300); do
      grep -qF "input: demo consumed dx=12 dy=-5 buttons=0" build/in.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/in-b.ppm"
    mon build/mon.sock "sendkey q"
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo stop" build/in.log 2>/dev/null && break
      sleep 0.1
    done
    sleep 1
    mon build/mon.sock "quit"
  ) &
  SENDER=$!
  timeout --signal=KILL 120 ./tools/run.sh --vga std --monitor --serial-unix build/in-serial.sock \
    < /dev/null > build/in-qemu.log 2>&1 || true
  wait "$SENDER" 2>/dev/null || true
  kill "$RELAY" 2>/dev/null || true

  if [ "$attempt" = "1" ]; then
    grep -q "input: ring self-test ok" build/in.log || note "ring self-test"
    grep -q "mouse: PS/2 aux enabled" build/in.log || note "mouse init"
    grep -q "graphics: demo start" build/in.log || note "demo start"
    grep -qE "panic|PANIC" build/in.log && { echo "panic in the input log"; OK=0; }
  fi
  grep -q "input: demo consumed dx=12 dy=-5 buttons=0" build/in.log || note "pointer consumed"
  grep -q "input: demo cursor x=28 y=699" build/in.log || note "cursor moved"
  grep -q "input: demo key stop scancode=16 ascii=113" build/in.log || note "key stop"
  grep -q "graphics: demo stop" build/in.log || note "demo stop"
  grep -q "graphics: idle damage empty ok" build/in.log || note "idle empty damage"

  if [ -s build/in-a.ppm ] && [ -s build/in-b.ppm ]; then
    if diff_contained build/in-a.ppm build/in-b.ppm build/in.log; then
      CONTAINED=1
      break
    fi
  else
    [ "$attempt" = "1" ] && note "screendumps produced"
  fi
done
[ "$CONTAINED" = "1" ] || note "screendump damage containment"

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (input: ring self-test + PS/2 mouse -> demo cursor + key stop)"
  grep -aE "input: ring self-test|mouse: PS/2|input: demo consumed|input: demo cursor|input: demo key stop" build/in.log | head -12
  exit 0
fi
echo "SMOKE FAIL (input)"
tail -25 build/in.log 2>/dev/null || true
exit 1
