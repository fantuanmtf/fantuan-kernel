#!/usr/bin/env bash
# M12-4/M12-5 NTFS smoke: the read-only reader on a deterministic hand-built
# fixture (tools/mkntfs.py). One bounded boot on the rescue profile
# (CONFIG_NTFS=y) with the NTFS partition on the delivered AHCI disk; the
# ESP autorun lists and reads /mnt/win0 through the kernel shell and, via
# `sh -c`, through the POSIX layer (userland ls/cat + a write attempt).
#
# Assertions: the mount line and volume facts (clusters, MFT record size),
# listing equality for the root and a subdirectory, the full-file SHA-256
# of the resident and non-resident (3-run, fragmented) files against the
# host fixture hashes, the corrupt-record rejection, the read-only write
# error, and that the NTFS image is byte-identical after the run (no write
# reached the volume).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (ntfs): $*"; exit 1; }

# The NTFS reader is enabled by the rescue profile (CONFIG_NTFS=y, M12-4).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "pre-build"

# Expected fixture content and hashes (the same bytes mkntfs.py encoded).
python3 tools/mkntfs.py --extract build/ntfs-fixture >/dev/null || fail "extract fixture"
HELLO_HOST=$(sha256sum "build/ntfs-fixture/hello.txt" | cut -d' ' -f1)
FRAG_HOST=$(sha256sum "build/ntfs-fixture/frag.bin" | cut -d' ' -f1)
RESUME_HOST=$(sha256sum "build/ntfs-fixture/résumé.txt" | cut -d' ' -f1)
ALICE_HOST=$(sha256sum build/ntfs-fixture/Users/alice.txt | cut -d' ' -f1)

LOG="build/smoke-ntfs.log"
rm -f "$LOG"
timeout --signal=KILL "${SMOKE_NTFS_TIMEOUT:-150}" ./tools/run.sh --ntfs > "$LOG" < /dev/null 2>&1 || true

OK=1
note() { echo "missing: $1"; OK=0; }

# --- volume facts and the mount -------------------------------------------------
grep -q "ntfs: mounted ro — label 'FANTUANNTFS', 4096 clusters of 4096 B, MFT record 1024 B at LCN 4" "$LOG" \
  || note "volume facts line"
grep -q "ntfs: mounted ro at /mnt/win0 (part 2)" "$LOG" || note "mount line"
grep -q "  /mnt/win0  part 2 (NTFS, ro, label 'FANTUANNTFS')" "$LOG" || note "lsmnt entry"
grep -q "part 2: NTFS (mounted ro)" "$LOG" || note "probe graduation"

# --- listing equality ------------------------------------------------------------
block_names() { # start-text -> sorted last-column names between header and prompt
  awk -v pat="$1" 'index($0, pat) == 1 { f = 1; next } /^root@/ { f = 0 } f' "$LOG" \
    | tr -d '\r' | grep -E '^  [df] ' | awk '{ print $NF }' | sort
}
EXPECT_ROOT=$(printf 'corrupt.txt\nfrag.bin\nhello.txt\nrésumé.txt\nUsers\n' | sort)
ROOT_NAMES=$(block_names 'ls: /mnt/win0 (NTFS ro)')
[ "$ROOT_NAMES" = "$EXPECT_ROOT" ] || { echo "root listing mismatch:"; echo "$ROOT_NAMES"; OK=0; }
EXPECT_USERS=$(printf 'alice.txt\nlogs\n' | sort)
USER_NAMES=$(block_names 'ls: /mnt/win0/Users (NTFS ro)')
[ "$USER_NAMES" = "$EXPECT_USERS" ] || { echo "Users listing mismatch:"; echo "$USER_NAMES"; OK=0; }
grep -q "sh -c 'ls /mnt/win0/Users/logs;cat /mnt/win0/hello.txt;echo x > /mnt/win0/new.txt'" "$LOG" \
  || note "userland shell transcript"
grep -aq "boot.log" "$LOG" || note "nested boot.log entry via userland ls"

# --- read equality: resident, non-resident/fragmented, non-ASCII, nested --------
grep -q "cat: 10000 bytes, truncated to 4096" "$LOG" || note "fragmented file size/truncation"
KHASHES=$(grep -a "cat: sha256" "$LOG" | sed -E 's/.*sha256 ([0-9a-f]{64}).*/\1/')
[ "$(printf '%s\n' "$KHASHES" | grep -c .)" = "4" ] || note "four file hashes"
[ "$(printf '%s\n' "$KHASHES" | sed -n 1p)" = "$HELLO_HOST" ] || note "hello.txt hash != host sha256"
[ "$(printf '%s\n' "$KHASHES" | sed -n 2p)" = "$FRAG_HOST" ] || note "frag.bin hash != host sha256"
[ "$(printf '%s\n' "$KHASHES" | sed -n 3p)" = "$RESUME_HOST" ] || note "résumé.txt hash != host sha256"
[ "$(printf '%s\n' "$KHASHES" | sed -n 4p)" = "$ALICE_HOST" ] || note "alice.txt hash != host sha256"

# --- corrupt-record rejection + graceful missing-file error ----------------------
grep -q "cat: NTFS: record corrupt (update sequence mismatch)" "$LOG" || note "corrupt record rejection"
grep -q "cat: NTFS: not found" "$LOG" || note "missing path error"
grep -qE "panic|PANIC" "$LOG" && { echo "panic in the log"; OK=0; }

# --- read-only enforcement -------------------------------------------------------
grep -q "Read-only file system" "$LOG" || note "write attempt did not hit EROFS"
grep -q "/mnt/win0/new.txt" "$LOG" || note "write attempt path"
python3 tools/mkdisk.py --ntfs build/ntfs-check.img >/dev/null || fail "rebuild the reference image"
cmp -s build/test.img build/ntfs-check.img || { echo "the NTFS image changed: a write reached the volume"; OK=0; }

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (ntfs: ro mount, listing equality, resident + fragmented hashes, corrupt rejection, no writes)"
  grep -aE "ntfs: mounted ro —|ntfs: mounted ro at|part 2: NTFS|cat: 10000|cat: sha256|record corrupt|Read-only file system" "$LOG" | head -14
  exit 0
fi
echo "SMOKE FAIL (ntfs) — log tail:"
tail -30 "$LOG"
exit 1
