#!/usr/bin/env python3
"""mkdisk_fat.py — FAT32 ESP fixture writer for tools/mkdisk.py: BPB, FATs,
HELLO/INFO, the boot-repair ESP tree (fstab/grub.cfg, Secure Boot certs,
shell autorun) and the M7.9 systemd-boot/UKI stubs; not standalone."""

import base64
import struct

SECTOR = 512

# EFI authenticated-variable bundle for EFI/fantuan/PK.AUTH (M8.1b): 16-byte
# EFI_TIME + WIN_CERTIFICATE_UEFI_GUID (PKCS#7 SignedData) + the new value,
# generated once with OpenSSL and re-checked in tools/gen_auth_fixture.sh.
AUTH_BLOB = base64.b64decode(
    b"AAAAAAAAAAAAAAAAAAAAAPkEAAAAAvEOndKvSt9o7kmKqTR9N1ZlpzCCBN0GCSqGSIb3DQEHAqCCBM4wggTKAgEBMQ8wDQYJ"
    b"YIZIAWUDBAIBBQAwMwYJKoZIhvcNAQcBoCYEJGZhbnR1YW4gYXV0aCBwYXlsb2FkICh2YXJpYWJsZSBkYXRhKaCCAxswggMX"
    b"MIIB/6ADAgECAhQ0+2hbpAfwC8ZKF2vBt8ij/QiZETANBgkqhkiG9w0BAQsFADAbMRkwFwYDVQQDDBBGYW50dWFuIFRlc3Qg"
    b"S0VLMB4XDTI2MDkxNzE3NDc1MFoXDTM2MDkxNDE3NDc1MFowGzEZMBcGA1UEAwwQRmFudHVhbiBUZXN0IEtFSzCCASIwDQYJ"
    b"KoZIhvcNAQEBBQADggEPADCCAQoCggEBAI9bFoqfNTlk6nCs4rRbzL4zhnNudejeN8HFp/e5lfu7dT8CSeRhLfcgi6ipDkXK"
    b"DxUkTm/ujMzhCMQ3CIgyrn8kaY6DTgWfdYOPkWxNiGbLatOqUDl/VRFWDK4FAspsJ9RRTTn6S/b5v3c7jv0RH7xPCz1dKN/q"
    b"39zE4I8P69wtHaqRUEpGPmy9JhUfGOmqOQ20fWWNFYBANrNmm2Si3YRh9UrpK4OhhkhE7hB22D9bmgPflt/I1G5AtgSLTJ/R"
    b"GCg2CxTx/HObvUhCl4Z4l4f4Wp4/CN1fQsaTcfVC2ib3SiI4VEeKxCfCu0avtlVuNlukd/rEiolUxyBIm4LiI7kCAwEAAaNT"
    b"MFEwHQYDVR0OBBYEFOePBrs7uOio4fGcq4jK576rhs/mMB8GA1UdIwQYMBaAFOePBrs7uOio4fGcq4jK576rhs/mMA8GA1Ud"
    b"EwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEBAI1f994bVuubKRRaXr+aDXPzBqwtPD6FGTVCYq9+kJDPMgjgLPz578Gg"
    b"531o40Y3Qsuyg0W0/EvwqxKdndDI0pLoCdkGlXAgXAi81ZHX72T+DmsbeXT1QEG9QOlodJTpF+ZhpUA1d1L5TTkOUKKdQTqX"
    b"cAoHARSBf2Qv74Od0hp4rRryAvFCsulz/+lLMB0BSrZkVW91TYivCEoMapOdYncl9Dzf9hOYksky5LtZJokmu0KlKyw16pgi"
    b"uBs+3gacVxu5veV1iFiGu9BUAa9kmYRq3txD5fLy0AshOUvYHYgl587aWOGr+zB/8IQ4fZyBgfqX/jV7crAGmDwGcMl/IfIx"
    b"ggFeMIIBWgIBATAzMBsxGTAXBgNVBAMMEEZhbnR1YW4gVGVzdCBLRUsCFDT7aFukB/ALxkoXa8G3yKP9CJkRMA0GCWCGSAFl"
    b"AwQCAQUAMA0GCSqGSIb3DQEBAQUABIIBAHZOiyVXDpR684QaRzWtgSvJrDBgarENb+2KR6p/rxoma0LAgxeTT2RSeXYiEas3"
    b"/NWiRtFFwU3ZnehFX0jn4rbOKlfo81fahXINhw4Lgji4z3Nic+aPgMFwyl7tRFS83aZ1oxqwK4lkHbvNwScyxH0h0Mod/lQM"
    b"6PVhm+CccnqtHO4JlIW2H6fN1yBmpOMtzldK81+6MV/lIqg5Ib2Yci5/M+jvPYxe1baYE/6SR3jeQW/eQysnT+P++ZsQb/C9"
    b"5sR3qJdJ11CR1x/YvvLWV267WjdX8yVyVOFo25lrLcVPs3H7xwbCdpEbqyRL77UlzN5SgjhxFrckjB5LPh2e/u1mYW50dWFu"
    b"IGF1dGggcGF5bG9hZCAodmFyaWFibGUgZGF0YSk="
)


def build(out, part_lba, part_sectors, flags, fstab):
    """Write the FAT32 partition into OUT; returns (spc, spf, clusters)."""
    broken = flags["broken"]
    noshim = flags["noshim"]
    keys = flags["keys"]
    two_fs = flags["two_fs"]
    shell_repair = flags["shell_repair"]
    grub_regen = flags["grub_regen"]
    imager = flags["imager"]

    def wsect(lba, data):
        out[lba * SECTOR:(lba + 1) * SECTOR] = data

    RESERVED = 32
    N_FATS = 2
    # --bigcluster uses 4 KiB clusters. The FAT holds 4-byte FAT32 entries and
    # must cover the whole data area, so size it from the cluster count (a
    # fixed size only ever fit the old 15 MiB partition), and mkdisk.py sizes
    # the partition above UEFI's 0xFFF5-cluster FAT32 minimum.
    SPC = 8 if flags["bigcluster"] else 1
    SPF = 1
    while True:
        clusters = (part_sectors - RESERVED - N_FATS * SPF) // SPC
        need = ((clusters + 2) * 4 + SECTOR - 1) // SECTOR
        if need <= SPF:
            break
        SPF = need
    CLUSTERS = (part_sectors - RESERVED - N_FATS * SPF) // SPC
    DATA_START = part_lba + RESERVED + N_FATS * SPF

    bpb = bytearray(SECTOR)
    bpb[0:3] = b"\xEB\x58\x90"
    bpb[3:11] = b"FANTUAN1"
    bpb[11:13] = struct.pack('<H', SECTOR)
    bpb[13] = SPC
    bpb[14:16] = struct.pack('<H', RESERVED)
    bpb[16] = N_FATS
    bpb[19:21] = struct.pack('<H', 0)          # root entries (FAT32: 0)
    bpb[21] = 0xF8
    bpb[28:32] = struct.pack('<I', 0)          # hidden sectors
    bpb[32:36] = struct.pack('<I', part_sectors)
    bpb[36:40] = struct.pack('<I', SPF)
    bpb[44:48] = struct.pack('<I', 2)          # root cluster
    bpb[0x52:0x5A] = b"FAT32   "               # filesystem type string
    bpb[48:50] = struct.pack('<H', 1)          # FSInfo sector
    bpb[50:52] = struct.pack('<H', 6)          # backup boot sector
    bpb[64] = 0x80
    bpb[66] = 0x29
    bpb[510:512] = b"\x55\xAA"
    wsect(part_lba, bpb)

    # FATs (identical copies)
    fat = bytearray(SPF * SECTOR)
    fat[0:4] = b"\xF8\xFF\xFF\x0F"
    fat[4:8] = b"\xFF\xFF\xFF\x0F"
    fat[2 * 4:2 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # cluster 2 = EOC (root dir)
    fat[3 * 4:3 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # cluster 3 = EOC (HELLO.TXT)
    fat[5 * 4:5 * 4 + 4] = struct.pack('<I', 6)          # 5 -> 6
    fat[6 * 4:6 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # 6 = EOC (INFO.TXT chain)
    for n in range(7, 28):                          # 7..27 = ESP/systemd/UKI/auth structure, all EOC
        fat[n * 4:n * 4 + 4] = b"\xFF\xFF\xFF\x0F"
    if keys:
        # PK.AUTH spans clusters 25..27 (the M8.1b .auth fixture).
        fat[25 * 4:25 * 4 + 4] = struct.pack('<I', 26)
        fat[26 * 4:26 * 4 + 4] = struct.pack('<I', 27)
    # The FAT spans many sectors: write it directly. wsect() would SPLICE a
    # longer payload into the bytearray and silently grow the disk image.
    for base in (part_lba + RESERVED, part_lba + RESERVED + SPF):
        start = base * SECTOR
        out[start:start + len(fat)] = fat

    def cluster_sector(n):
        return DATA_START + (n - 2) * SPC

    def dent(name8, ext3, cluster, size, attrs=0x20):
        e = bytearray(32)
        e[0:8] = name8.ljust(8).encode()
        e[8:11] = ext3.ljust(3).encode()
        e[11] = attrs
        e[20:22] = struct.pack('<H', (cluster >> 16) & 0xFFFF)
        e[26:28] = struct.pack('<H', cluster & 0xFFFF)
        e[28:32] = struct.pack('<I', size)
        return e

    def dotdot(parent):
        return dent(".", "   ", parent if parent else 0, 0, attrs=0x10)

    def self_entry(cluster):
        return dent(".", "   ", cluster, 0, attrs=0x10)

    def put_file(cluster, data):
        sec = bytearray(SECTOR)
        sec[0:len(data)] = data
        wsect(cluster_sector(cluster), sec)

    def put_file_chain(clusters, data):
        for i, c in enumerate(clusters):
            put_file(c, data[i * SECTOR:(i + 1) * SECTOR])

    HELLO = b"Hello from the fantuan-kernel VFS!\n"
    INFO = b"X" * 1000
    FANTUAN_CONF = b"title Fantuan test entry\nlinux /boot/vmlinuz-6.6.0-fantuan\n"

    # M7 boot-repair fixture: grub.cfg copy + boot payloads.
    GRUBCFG = (
        b"search.fs_uuid 12345678-1234-1234-1234-123456789abc root\n"
        b"set prefix=($root)'/boot/grub'\n"
        b"set root='hd0,gpt1'\n"
    )
    BOOTX64 = b"FANTUAN FALLBACK EFI APP (dummy)\n"
    SHIM = b"FANTUAN SHIM (dummy)\n"
    GRUBX64 = b"FANTUAN GRUB (dummy)\n"

    root = bytearray(SECTOR)
    HELLO_SIZE = 4096 if flags["liar"] else len(HELLO)
    root[0:32] = dent("HELLO", "TXT", 3, HELLO_SIZE)
    root[32:64] = dent("INFO", "TXT", 5, len(INFO))
    root[64:96] = dent("EFI", "   ", 7, 0, attrs=0x10)
    root[96:128] = dent("FSTAB", "   ", 14, len(fstab))
    if two_fs:
        # systemd-boot config lives at the ESP root (M7.9 detection fixture).
        root[128:160] = dent("loader", "   ", 22, 0, attrs=0x10)
    wsect(cluster_sector(2), root)

    hello = bytearray(SECTOR)
    hello[0:len(HELLO)] = HELLO
    wsect(cluster_sector(3), hello)

    i0 = bytearray(SECTOR)
    i0[0:512] = INFO[0:512]
    wsect(cluster_sector(5), i0)
    i1 = bytearray(SECTOR)
    i1[0:488] = INFO[512:1000]
    if SPC == 1:
        # One sector per cluster: the second half lives in the next cluster.
        wsect(cluster_sector(6), i1)
    else:
        # Both halves fit the same cluster (the 5->6 chain stays unused).
        wsect(cluster_sector(5) + 1, i1)

    # --- ESP structure (M7 boot-repair fixture) ---------------------------
    # Cluster map: 7=EFI/ 8=EFI/BOOT/ 9=EFI/ubuntu/ 10=BOOTX64.EFI 11=grub.cfg
    # 12=shimx64.efi 13=grubx64.efi 14=fstab; --keys adds 15=EFI/fantuan/,
    # 16=PK.cer 17=KEK.cer 18=db.cer 19=SHELL.CMD (M7.7 + §10 autorun).

    EFI_DIR = bytearray(SECTOR)
    EFI_DIR[0:32] = self_entry(7)
    EFI_DIR[32:64] = dotdot(0)
    EFI_DIR[64:96] = dent("BOOT", "   ", 8, 0, attrs=0x10)
    EFI_DIR[96:128] = dent("ubuntu", "   ", 9, 0, attrs=0x10)
    if keys:
        EFI_DIR[128:160] = dent("fantuan", "   ", 15, 0, attrs=0x10)
    if two_fs:
        # EFI-stub / UKI fixture (M7.9): EFI/Linux/ holds bootable EFI images.
        off = 160 if keys else 128
        EFI_DIR[off:off + 32] = dent("Linux", "   ", 20, 0, attrs=0x10)
    wsect(cluster_sector(7), EFI_DIR)

    BOOT_DIR = bytearray(SECTOR)
    BOOT_DIR[0:32] = self_entry(8)
    BOOT_DIR[32:64] = dotdot(7)
    if broken:
        BOOT_DIR[64] = 0xE5  # deleted: simulate a missing fallback loader
    else:
        BOOT_DIR[64:96] = dent("BOOTX64", "EFI", 10, len(BOOTX64))
    wsect(cluster_sector(8), BOOT_DIR)

    UBUNTU_DIR = bytearray(SECTOR)
    UBUNTU_DIR[0:32] = self_entry(9)
    UBUNTU_DIR[32:64] = dotdot(7)
    UBUNTU_DIR[64:96] = dent("GRUB", "CFG", 11, len(GRUBCFG))
    if noshim:
        UBUNTU_DIR[96] = 0xE5  # deleted: simulate a missing shim
    else:
        UBUNTU_DIR[96:128] = dent("SHIMX64", "EFI", 12, len(SHIM))
    UBUNTU_DIR[128:160] = dent("GRUBX64", "EFI", 13, len(GRUBX64))
    wsect(cluster_sector(9), UBUNTU_DIR)

    # M7.7: platform-key fixtures; Setup Mode accepts the DER-ish blob.
    CERT = b"\x30\x82\x00\x40" + (b"FANTUAN TEST CERTIFICATE " * 4)
    # §10 shell autorun script. --shell-repair swaps in the confirmation-
    # gated repair sequence (the shell feeds the next script line as the YES
    # answer); --grub-regen runs the M7.9 install path instead.
    if imager:
        # M12-2 clone transcript: YES-gate abort, size-gate refusal and the
        # verified happy path. blk0 is the mounted boot disk, blk1 the small
        # pattern source, blk2 the larger empty destination and blk3 the
        # smaller one (the size gate).
        SHELL_CMD = (
            b"clone blk1 blk2\n"
            b"NO\n"
            b"clone blk1 blk3 --yes\n"
            b"clone blk1 blk2 --verify\n"
            b"YES\n"
        )
    elif flags.get("kbd_test"):
        SHELL_CMD = b"help\nlsmnt\n"
    elif grub_regen:
        SHELL_CMD = (
            b"grub-fix install\n"
            b"YES\n"
            b"cat /EFI/ubuntu/grub.cfg\n"
        )
    elif shell_repair:
        SHELL_CMD = (
            b"grub-fix repair\n"
            b"YES\n"
            b"cat /EFI/BOOT/BOOTX64.EFI\n"
        )
    else:
        cmds = [
            b"help",
            b"lsdev",
            b"lsos",
            b"lsmnt",
            b"cat /HELLO.TXT",
            b"bootinfo",
            b"diskhealth",
        ]
        if two_fs:
            # Ext4 phase: keep the script fast and deterministic (the surface
            # scan would eat the phase budget on the 25 MiB disk); the scan
            # itself is covered by the default --keys phase.
            cmds.append(b"cat /etc/fstab")
        else:
            cmds.append(b"diskhealth --scan")
        cmds.append(b"crypto-selftest")
        SHELL_CMD = b"".join(c + b"\n" for c in cmds)
    if keys:
        FANTUAN_DIR = bytearray(SECTOR)
        FANTUAN_DIR[0:32] = self_entry(15)
        FANTUAN_DIR[32:64] = dotdot(7)
        FANTUAN_DIR[64:96] = dent("PK", "CER", 16, len(CERT))
        FANTUAN_DIR[96:128] = dent("KEK", "CER", 17, len(CERT))
        FANTUAN_DIR[128:160] = dent("DB", "CER", 18, len(CERT))
        FANTUAN_DIR[160:192] = dent("SHELL", "CMD", 19, len(SHELL_CMD))
        # 8.3 short alias of "PK.auth" (extensions are 3 chars on FAT).
        FANTUAN_DIR[192:224] = dent("PK", "AUT", 25, len(AUTH_BLOB))
        wsect(cluster_sector(15), FANTUAN_DIR)
        put_file(16, CERT)
        put_file(17, CERT)
        put_file(18, CERT)
        put_file(19, SHELL_CMD)
        put_file_chain([25, 26, 27], AUTH_BLOB)

    if not broken:
        put_file(10, BOOTX64)
    put_file(11, GRUBCFG)
    if not noshim:
        put_file(12, SHIM)
    put_file(13, GRUBX64)
    put_file(14, fstab)

    if two_fs:
        # systemd-boot config tree (M7.9): /loader/entries/FANTUAN.CON stands
        # in for a *.conf entry's 8.3 alias; EFI/Linux/FANTUAN.EFI is a UKI.
        LOADER_DIR = bytearray(SECTOR)
        LOADER_DIR[0:32] = self_entry(22)
        LOADER_DIR[32:64] = dotdot(0)
        LOADER_DIR[64:96] = dent("entries", "   ", 23, 0, attrs=0x10)
        wsect(cluster_sector(22), LOADER_DIR)
        ENTRIES_DIR = bytearray(SECTOR)
        ENTRIES_DIR[0:32] = self_entry(23)
        ENTRIES_DIR[32:64] = dotdot(22)
        ENTRIES_DIR[64:96] = dent("FANTUAN", "CON", 24, len(FANTUAN_CONF))
        wsect(cluster_sector(23), ENTRIES_DIR)
        put_file(24, FANTUAN_CONF)
        LINUX_DIR = bytearray(SECTOR)
        LINUX_DIR[0:32] = self_entry(20)
        LINUX_DIR[32:64] = dotdot(7)
        LINUX_DIR[64:96] = dent("FANTUAN", "EFI", 21, len(BOOTX64))
        wsect(cluster_sector(20), LINUX_DIR)
        put_file(21, BOOTX64)

    return SPC, SPF, CLUSTERS
