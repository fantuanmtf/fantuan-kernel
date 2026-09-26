#!/usr/bin/env bash
# M12-3 bad-sector smoke: `clone --continue`, the zero-fill policy and the
# clone report. One bounded boot on the rescue profile (CONFIG_IMAGER=y) with
# four AHCI disks:
#   blk0  the delivered test disk (ESP autorun transcript)
#   blk1  a 1 MiB pattern source whose bad ranges are injected by QEMU
#         blkdebug as real AHCI read errors (tools/mkimagerdisks.sh --bad)
#   blk2  a 6 MiB empty destination
#   blk3  a 4 MiB clean pattern source (the --quick path)
# The autorun runs three clones: the quick happy path, the default abort on the
# first bad sector, then `--continue` zero-fill. Assertions: the abort
# transcript, the partial completion, the report ranges/counts/hashes and the
# host-side zero-filled expectation for the destination prefix.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

BAD="100:4,700:2"
SRC_BYTES=$((2048 * 512))

# The imager is enabled by the rescue profile (CONFIG_IMAGER=y, M12-2).
python3 tools/kconfig.py --profile rescue >/dev/null || { echo "SMOKE FAIL (imager-bad) — config"; exit 1; }
./tools/build.sh >/dev/null 2>&1 || { echo "SMOKE FAIL (imager-bad) — pre-build"; exit 1; }

# Deterministic fixture + the zero-filled expectation (the pattern with every
# injected range zeroed). run.sh regenerates the same source bytes from BAD.
python3 tools/mkdisk.py --pattern 2048 build/imager-bad.img --badclusters "$BAD" >/dev/null
python3 tools/mkdisk.py --imager-bad build/imager-badsrc.img >/dev/null
cp build/imager-bad.img build/imager-bad-exp.img
while read -r lba count; do
  dd if=/dev/zero of=build/imager-bad-exp.img bs=512 seek="$lba" count="$count" conv=notrunc status=none
done < build/imager-bad.img.bad
EXPECTED=$(sha256sum build/imager-bad-exp.img | cut -d' ' -f1)
BAD_RANGES=$(wc -l < build/imager-bad.img.bad | tr -d ' ')
BAD_SECTORS=$(awk '{s += $2} END {print s}' build/imager-bad.img.bad)

LOG="build/smoke-imager-bad.log"
rm -f "$LOG" build/smoke-report-*.txt
env BADCLUSTERS="$BAD" ./tools/run.sh --no-build --imager-bad > "$LOG" < /dev/null 2>&1 &
QPID=$!
i=0
while [ "$i" -lt "${SMOKE_IMAGER_BAD_TIMEOUT:-180}" ]; do
  i=$((i + 1))
  sleep 1
  [ "$(grep -ac 'clone-report: end' "$LOG" 2>/dev/null || true)" -ge 3 ] && break
  kill -0 "$QPID" 2>/dev/null || break
done
kill "$QPID" 2>/dev/null
wait "$QPID" 2>/dev/null || true

DST_HOST=$(head -c "$SRC_BYTES" build/imager-baddst.img | sha256sum | cut -d' ' -f1)

# Report blocks in transcript order: 1 = --quick, 2 = default abort,
# 3 = --continue (the one the assertions below read).
awk -v dir=build '/^clone-report v1/ {n++; f=sprintf("%s/smoke-report-%d.txt", dir, n)} f {print > f} /^clone-report: end/ {f=""}' "$LOG"

OK=1
want() { grep -qF "$2" "$1" || { echo "missing in $1: $2"; OK=0; }; }
for f in build/smoke-report-1.txt build/smoke-report-2.txt build/smoke-report-3.txt; do
  [ -f "$f" ] || { echo "missing report block $f"; OK=0; }
done

# 1) Default policy: the copy aborts at the exact bad sector and writes nothing.
want "$LOG" "clone: read failed at LBA 100 after 3 retries while hashing source"
want "$LOG" "clone: FAILED — source is not readable; nothing was written"
want build/smoke-report-2.txt "clone-report: policy continue=no quick=no retries=3"
want build/smoke-report-2.txt "clone-report: bad-range lba=100 count=1 errors=1 retries=3"
want build/smoke-report-2.txt "clone-report: verdict failed"

# 2) --continue: completes, zero-fills and reports the partial verdict.
want "$LOG" "clone: chunk at LBA 0 unreadable after 3 retries — isolating sectors"
want "$LOG" "clone: bad sector LBA 103 after 3 retries — zero-filled"
want "$LOG" "clone: verify ok — destination matches the zero-filled source (${BAD_SECTORS} bad sectors zero-filled)"
want "$LOG" "clone: done (2048 sectors copied; ${BAD_SECTORS} bad sector(s) zero-filled; PARTIAL — not a byte-for-byte source copy)"
want "$LOG" "clone: report -> /tmp/clone-report.txt"
grep -q "tmpfs write failed" "$LOG" && { echo "report tmpfs write failed"; OK=0; }
want build/smoke-report-3.txt "clone-report: source blk1 ahci 2048 sectors 1048576 bytes"
want build/smoke-report-3.txt "clone-report: destination blk2 ahci 12288 sectors 6291456 bytes"
want build/smoke-report-3.txt "clone-report: policy continue=yes quick=no retries=3"
want build/smoke-report-3.txt "clone-report: verify full"
i=0
while read -r lba count; do
  i=$((i + 1))
  want build/smoke-report-3.txt "clone-report: bad-range lba=$lba count=$count errors=$count retries=$((count * 3))"
done < build/imager-bad.img.bad
want build/smoke-report-3.txt "clone-report: bad-ranges $BAD_RANGES"
want build/smoke-report-3.txt "clone-report: errors $BAD_SECTORS"
want build/smoke-report-3.txt "clone-report: retries $((BAD_SECTORS * 3))"
want build/smoke-report-3.txt "clone-report: source-match yes"
want build/smoke-report-3.txt "clone-report: verdict partial"
RSRC=$(sed -n 's/^clone-report: source-sha256 //p' build/smoke-report-3.txt | tr -d '\r')
RSTREAM=$(sed -n 's/^clone-report: stream-sha256 //p' build/smoke-report-3.txt | tr -d '\r')
RDST=$(sed -n 's/^clone-report: destination-sha256 //p' build/smoke-report-3.txt | tr -d '\r')
[ "$RSRC" = "$EXPECTED" ] || { echo "report source hash != zero-filled expectation"; OK=0; }
[ "$RSTREAM" = "$EXPECTED" ] || { echo "report stream hash != zero-filled expectation"; OK=0; }
[ "$RDST" = "$EXPECTED" ] || { echo "report destination hash != zero-filled expectation"; OK=0; }

# 3) --quick: the sampled verification path reports its own verdict.
want build/smoke-report-1.txt "clone-report: policy continue=no quick=yes retries=3"
want build/smoke-report-1.txt "clone-report: verify quick"
want build/smoke-report-1.txt "clone-report: bad-ranges 0"
want build/smoke-report-1.txt "clone-report: verdict verified"
want "$LOG" "clone: done (8192 sectors copied and sha256-verified, quick sample)"

# Host-side destination: the first 2048 sectors equal the zero-filled pattern.
[ "$DST_HOST" = "$EXPECTED" ] || { echo "host destination prefix != zero-filled expectation"; OK=0; }

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (imager-bad: abort, --continue zero-fill, report ranges/counts, quick verify)"
  echo "--- continue report ---"
  cat build/smoke-report-3.txt
  exit 0
fi
echo "SMOKE FAIL (imager-bad) — log tail:"
tail -40 "$LOG"
exit 1
