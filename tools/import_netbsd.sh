#!/usr/bin/env bash
# import_netbsd.sh - vendor the curated NetBSD source slice for the M11
# network stack (docs/M11_NET.md sections 2 and 4).
#
# The slice is pinned to one exact commit of the GitHub mirror NetBSD/src
# (branch refs/heads/netbsd-10; the mirror exposes no release tags).  Files
# keep their upstream paths and headers; they are exempt from the 300-line
# rule and are registered per file in third_party/netbsd/MANIFEST.tsv:
#
#   path <TAB> sha256 <TAB> license <TAB> upstream-url
#
# Re-running with the same NETBSD_REV is a no-op: an existing file whose
# sha256 matches the manifest is not fetched again, and the manifest is only
# rewritten when its content changes.  A file that differs from its manifest
# entry is re-downloaded.  The manifest is persisted after every file so an
# interrupted run can be resumed.
#
# Usage: ./tools/import_netbsd.sh [--force]
#   --force  re-download every file, ignoring the manifest.
set -euo pipefail

NETBSD_REF="refs/heads/netbsd-10"
NETBSD_REV="e145e524ee8362fa7d14824b2921b0ba1b694bfe"
RAW="https://raw.githubusercontent.com/NetBSD/src/${NETBSD_REV}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/third_party/netbsd"
MANIFEST="$DEST/MANIFEST.tsv"
FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

# Curated file list: the R1 slice plus the machine-independent closure the
# build spike actually needed (header names are as resolved, not guessed;
# sys/sys/{type.h,libkern.h,synch.h} do not exist in NetBSD 10 and
# subr_pslist.c/kern_mbuf.c are spelled pslist.h and uipc_mbuf.c there).
# common/include/prop/plistref.h is outside sys/ but is a kernel-needed
# header (pulled in by sys/dkio.h via sys/ioctl.h).
FILES="
    common/include/prop/plistref.h sys/altq/if_altq.h sys/dev/lockstat.h \
    sys/kern/kern_condvar.c sys/kern/kern_lock.c sys/kern/kern_mutex.c \
    sys/kern/kern_rwlock.c sys/kern/kern_timeout.c sys/kern/subr_evcnt.c \
    sys/kern/subr_pool.c sys/kern/subr_psref.c sys/kern/uipc_mbuf.c \
    sys/lib/libkern/libkern.h sys/lib/libkern/strlist.h sys/net/dlt.h \
    sys/net/if_arp.h sys/net/if.h sys/net/if_stats.h \
    sys/net/net_stats.h sys/net/pfil.h sys/net/pktqueue.h \
    sys/net/radix.h sys/net/route.h sys/sys/aio.h \
    sys/sys/ansi.h sys/sys/asan.h sys/sys/atomic.h \
    sys/sys/bitops.h sys/sys/bufq.h sys/sys/callback.h \
    sys/sys/callout.h sys/sys/cdefs_elf.h sys/sys/cdefs.h \
    sys/sys/clock.h sys/sys/common_ansi.h sys/sys/common_int_const.h \
    sys/sys/common_int_fmtio.h sys/sys/common_int_limits.h sys/sys/common_int_mwgwtypes.h \
    sys/sys/common_int_types.h sys/sys/condvar.h sys/sys/cpu_data.h \
    sys/sys/cpu.h sys/sys/debug.h sys/sys/device_if.h \
    sys/sys/dkio.h sys/sys/domain.h sys/sys/endian.h \
    sys/sys/errno.h sys/sys/evcnt.h sys/sys/event.h \
    sys/sys/fault.h sys/sys/fd_set.h sys/sys/featuretest.h \
    sys/sys/filio.h sys/sys/hook.h sys/sys/idtype.h \
    sys/sys/intr.h sys/sys/inttypes.h sys/sys/ioccom.h \
    sys/sys/ioctl.h sys/sys/ipi.h sys/sys/kcpuset.h \
    sys/sys/kernel.h sys/sys/kmem.h sys/sys/kprintf.h \
    sys/sys/localcount.h sys/sys/lockdebug.h sys/sys/lock.h \
    sys/sys/lwp.h sys/sys/malloc.h sys/sys/mallocvar.h \
    sys/sys/mbuf.h sys/sys/module_hook.h sys/sys/mqueue.h \
    sys/sys/msan.h sys/sys/mutex.h sys/sys/null.h \
    sys/sys/param.h sys/sys/pcu.h sys/sys/percpu.h \
    sys/sys/percpu_types.h sys/sys/pool.h sys/sys/proc.h \
    sys/sys/protosw.h sys/sys/pserialize.h sys/sys/pslist.h \
    sys/sys/psref.h sys/sys/queue.h sys/sys/radixtree.h \
    sys/sys/rbtree.h sys/sys/resource.h sys/sys/resourcevar.h \
    sys/sys/rwlock.h sys/sys/sched.h sys/sys/sdt.h \
    sys/sys/select.h sys/sys/selinfo.h sys/sys/siginfo.h \
    sys/sys/signal.h sys/sys/signalvar.h sys/sys/sigtypes.h \
    sys/sys/sleepq.h sys/sys/sleeptab.h sys/sys/socket.h \
    sys/sys/socketvar.h sys/sys/sockio.h sys/sys/specificdata.h \
    sys/sys/spl.h sys/sys/stdarg.h sys/sys/stdbool.h \
    sys/sys/stdint.h sys/sys/syncobj.h sys/sys/sysctl.h \
    sys/sys/syslimits.h sys/sys/syslog.h sys/sys/systm.h \
    sys/sys/time.h sys/sys/timespec.h sys/sys/timevar.h \
    sys/sys/tree.h sys/sys/ttycom.h sys/sys/types.h \
    sys/sys/ucontext.h sys/sys/ucred.h sys/sys/uidinfo.h \
    sys/sys/uio.h sys/sys/vmem.h sys/sys/vmmeter.h \
    sys/sys/workqueue.h sys/sys/xcall.h sys/uvm/uvm_anon.h \
    sys/uvm/uvm_extern.h sys/uvm/uvm_map.h sys/uvm/uvm_pager.h \
    sys/uvm/uvm_param.h sys/uvm/uvm_physseg.h sys/uvm/uvm_pmap.h \
    sys/uvm/uvm_prot.h \
    sys/kern/subr_pserialize.c sys/net/bpf_stub.c sys/net/if.c \
    sys/net/if_loop.c sys/net/if_stats.c sys/net/radix.c \
    sys/net/route.c sys/net/rtbl.c \
    common/include/prop/prop_array.h common/include/prop/prop_bool.h \
    common/include/prop/prop_data.h common/include/prop/prop_dictionary.h \
    common/include/prop/prop_ingest.h common/include/prop/prop_number.h \
    common/include/prop/prop_object.h common/include/prop/prop_string.h \
    common/include/prop/proplib.h \
    sys/compat/net/if.h sys/compat/net/route.h sys/compat/sys/sockio.h \
    sys/compat/sys/time.h sys/compat/sys/time_types.h \
    sys/crypto/cprng_fast/cprng_fast.h \
    sys/crypto/nist_hash_drbg/nist_hash_drbg.h sys/net/bpf.h \
    sys/net/bpfjit.h sys/sys/compat_stub.h sys/sys/cprng.h \
    sys/sys/device.h sys/sys/kauth.h sys/sys/module.h \
    sys/net/ethertypes.h sys/net/if_dl.h sys/net/if_ether.h \
    sys/net/if_llatbl.h sys/net/if_media.h sys/net/if_module.h \
    sys/net/if_types.h sys/net/raw_cb.h sys/net80211/_ieee80211.h \
    sys/net80211/ieee80211.h sys/net80211/ieee80211_crypto.h \
    sys/net80211/ieee80211_ioctl.h sys/netinet/if_inarp.h \
    sys/netinet/in.h sys/netinet/in_offload.h sys/netinet/in_selsrc.h \
    sys/netinet/in_systm.h sys/netinet/in_var.h sys/netinet/ip.h \
    sys/netinet/ip_encap.h sys/netinet/ip_var.h \
    sys/netinet6/in6.h sys/netinet6/in6_var.h sys/netmpls/mpls.h \
    sys/secmodel/secmodel.h sys/sys/pmf.h sys/sys/stat.h"

# Files with no per-file license notice, accepted explicitly.  device_if.h
# is a committed NetBSD-generated header (no copyright block); it is part of
# NetBSD src and stays under NetBSD's BSD terms, see THIRD_PARTY.md.
# in_selsrc.h and cprng_fast.h are likewise NetBSD project headers whose
# notice lives in the source file, not the header.
NOHEADER_OK="sys/sys/device_if.h sys/netinet/in_selsrc.h \
    sys/crypto/cprng_fast/cprng_fast.h"

license_of() {
    local f="$1"
    if grep -q 'SPDX-License-Identifier:' "$f"; then
        grep -m1 'SPDX-License-Identifier:' "$f" |
            sed 's/.*SPDX-License-Identifier:[[:space:]]*//; s/[[:space:]]*\(\*\/\)*[[:space:]]*$//'
        return
    fi
    local head
    head="$(head -70 "$f")"
    if echo "$head" | grep -qi 'public domain'; then
        echo "Public-Domain"
    elif echo "$head" | grep -q 'Redistribution and use in source and binary forms'; then
        if echo "$head" | grep -q 'Neither the name'; then
            echo "BSD-3-Clause"
        else
            echo "BSD-2-Clause"
        fi
    elif echo "$head" | grep -qi 'Apache License'; then
        echo "Apache-2.0"
    elif echo "$head" | grep -q 'Carnegie-Mellon University'; then
        echo "MIT"          # CMU/MIT-style permissive notice
    else
        echo "UNKNOWN"
    fi
}

sha256() {
    sha256sum "$1" | cut -d' ' -f1
}

save_manifest() {
    sort -o "$1" "$1"
    if ! cmp -s "$1" "$MANIFEST" 2>/dev/null; then
        cp "$1" "$MANIFEST"
        manifest_changed=1
    fi
}

mkdir -p "$DEST"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

declare -A prev
if [ -f "$MANIFEST" ]; then
    while IFS=$'\t' read -r p h l u; do
        [ -n "$p" ] && prev["$p"]="$h"
    done < "$MANIFEST"
fi

downloaded=0 kept=0 failed=0
manifest_changed=0
: > "$tmp/MANIFEST.tsv"
for p in $FILES; do
    f="$DEST/$p"
    mkdir -p "$(dirname "$f")"
    if [ "$FORCE" -eq 0 ] && [ -s "$f" ]; then
        # No manifest entry yet: take the local file as-is (resume after an
        # interrupted run or a pre-seeded tree).  With an entry, the sha256
        # must match or the file is re-fetched.
        if [ -z "${prev[$p]:-}" ] || [ "$(sha256 "$f")" = "${prev[$p]}" ]; then
            kept=$((kept + 1))
        else
            rm -f "$f"
        fi
    fi
    if [ ! -s "$f" ]; then
        if ! curl -fsSL --retry 3 --retry-delay 2 --retry-all-errors \
            --max-time 60 "$RAW/$p" -o "$tmp/dl"; then
            echo "FAIL download: $p" >&2
            failed=$((failed + 1))
            continue
        fi
        mv "$tmp/dl" "$f"
        downloaded=$((downloaded + 1))
    fi
    lic="$(license_of "$f")"
    if [ "$lic" = "UNKNOWN" ] && ! echo "$NOHEADER_OK" | grep -qw "$p"; then
        echo "FAIL license header: $p" >&2
        failed=$((failed + 1))
        continue
    fi
    printf '%s\t%s\t%s\t%s\n' "$p" "$(sha256 "$f")" "$lic" "$RAW/$p" \
        >> "$tmp/MANIFEST.tsv"
done
save_manifest "$tmp/MANIFEST.tsv"

total=$(grep -c . "$MANIFEST" || true)
echo "netbsd import: rev=$NETBSD_REV ($NETBSD_REF)"
echo "  files=$total downloaded=$downloaded kept=$kept failed=$failed manifest_updated=$manifest_changed"
echo "  licenses:"
cut -f3 "$MANIFEST" | sort | uniq -c | sed 's/^/    /'
[ "$failed" -eq 0 ]
