#!/usr/bin/env bash
# M12-2 disk-imager smoke: the `clone` command with mandatory SHA-256
# verification. One bounded boot on the rescue profile (CONFIG_IMAGER=y)
# with four AHCI disks:
#   blk0  the delivered test disk (boot + ESP autorun, not a clone target)
#   blk1  a 1 MiB deterministic pattern (source)
#   blk2  empty, larger  -> verified happy-path round trip
#   blk3  empty, smaller -> hard size-gate refusal
# The ESP autorun transcript answers the YES gate (NO then YES). Assertions:
# the plan/size/YES transcripts, the kernel's source and destination hashes
# against the host sha256, the destination image equal to the source after
# the run, and the untouched destination tail.
#
# The read-only destination case (i686's blk_write stub returns -1; the
# shared imager prints "destination is read-only on this build") is not
# reachable at runtime here: QEMU refuses a readonly=on IDE backend and
# ignores blkdebug write injection on the AHCI path, and i686 has no shell.
# This smoke asserts the branch is linked in the tested rescue ELF; the i686
# build and the stub are documented in docs/M12_TOOLS_HW.md section 2a.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

# The imager is enabled by the rescue profile (CONFIG_IMAGER=y, M12-2).
python3 tools/kconfig.py --profile rescue >/dev/null || { echo "SMOKE FAIL (imager) — config"; exit 1; }
./tools/build.sh >/dev/null 2>&1 || { echo "SMOKE FAIL (imager) — pre-build"; exit 1; }

# Deterministic fixtures (run.sh leaves existing empties alone, so the
# pre-hash below is what the guest starts from).
python3 tools/mkdisk.py --imager build/imager-src.img >/dev/null
python3 tools/mkdisk.py --pattern 2048 build/imager-pattern.img >/dev/null
python3 tools/mkdisk.py --empty 4096 build/imager-dst.img >/dev/null
python3 tools/mkdisk.py --empty 1024 build/imager-small.img >/dev/null
SRC_BYTES=$((2048 * 512))
SRC_HOST=$(sha256sum build/imager-pattern.img | cut -d' ' -f1)
DST_BEFORE=$(head -c "$SRC_BYTES" build/imager-dst.img | sha256sum | cut -d' ' -f1)

LOG="build/smoke-imager.log"
rm -f "$LOG"
timeout --signal=KILL "${SMOKE_IMAGER_TIMEOUT:-120}" ./tools/run.sh --imager > "$LOG" < /dev/null 2>&1 || true

# Kernel-reported hashes.
KHASH=$(grep -a "clone: source sha256" "$LOG" | sed -E 's/.*sha256 ([0-9a-f]{64}).*/\1/' | tail -1)
DHASH=$(grep -a "clone: destination sha256" "$LOG" | tail -1 | sed -E 's/.*sha256 ([0-9a-f]{64}).*/\1/')
DST_AFTER=$(head -c "$SRC_BYTES" build/imager-dst.img | sha256sum | cut -d' ' -f1)

OK=1
ELF="target/x86_64-unknown-none/release/fantuan-kernel"
grep -q "clone: plan src=blk1 (ahci) 2048 sectors" "$LOG" || { echo "missing: source plan"; OK=0; }
grep -q "clone: plan dst=blk2 (ahci) 4096 sectors" "$LOG" || { echo "missing: destination plan"; OK=0; }
grep -q "clone: plan hashes: sha256 of the source" "$LOG" || { echo "missing: hash plan"; OK=0; }
grep -q "confirm> NO" "$LOG" || { echo "missing: NO answer"; OK=0; }
grep -q "clone: confirmation not YES — aborted (nothing written)" "$LOG" || { echo "missing: YES-gate abort"; OK=0; }
grep -q "clone: plan dst=blk3 (ahci) 1024 sectors" "$LOG" || { echo "missing: small destination plan"; OK=0; }
grep -q "clone: refusing: source 2048 sectors > destination 1024 sectors — destination is too small" "$LOG" || { echo "missing: size-gate refusal"; OK=0; }
grep -q "clone: verify ok — source and destination hashes match" "$LOG" || { echo "missing: verify ok"; OK=0; }
grep -q "clone: done (2048 sectors copied and sha256-verified)" "$LOG" || { echo "missing: done line"; OK=0; }
# grep must consume all input: under pipefail an early -q exit SIGPIPEs strings.
strings -a "$ELF" | grep "destination is read-only on this build" > /dev/null \
  || { echo "read-only branch not linked"; OK=0; }
[ "$KHASH" = "$SRC_HOST" ] || { echo "kernel source hash != host sha256"; OK=0; }
[ "$DHASH" = "$SRC_HOST" ] || { echo "kernel destination hash != host sha256"; OK=0; }
[ "$DST_BEFORE" != "$DST_AFTER" ] || { echo "destination image did not change"; OK=0; }
[ "$DST_AFTER" = "$SRC_HOST" ] || { echo "host destination sha256 != source"; OK=0; }

# The destination is larger by 2048 sectors: the tail must stay untouched.
TAIL_BYTES=$((2048 * 512))
if ! head -c "$TAIL_BYTES" /dev/zero | cmp -s - <(tail -c +$((SRC_BYTES + 1)) build/imager-dst.img); then
  echo "destination tail was not left untouched"
  OK=0
fi

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (imager: verified round trip, size gate, YES gate, read-only branch linked)"
  grep -aE "clone: (plan src|plan dst=blk2|refusing|confirmation|source sha256|verify ok|done)" "$LOG" | head -10
  exit 0
fi
echo "SMOKE FAIL (imager) — log tail:"
tail -30 "$LOG"
exit 1
