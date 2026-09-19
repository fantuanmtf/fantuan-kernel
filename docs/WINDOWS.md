# Windows / PE Is Not Supported

Status: permanent non-goal, recorded since `M10_BOOT_32BIT.md` §2 and
`ROADMAP_v0.0.2+.md` §2. This is not a deferral to a later milestone.

## Scope

fantuan-kernel diagnoses and repairs **Linux and BSD boot chains only**.
The Windows boot chain (BCD store, `bootmgr`, `winload`) is closed source
and undocumented, so a repair tool for it could not be written or verified
responsibly. Windows entries seen on the ESP are reported by
`grub-fix diagnose` and `lsos` as identified-not-repaired, for context.

## No writes to Windows volumes

- The only disk write path (FAT32 repair on x86_64/riscv, behind
  `RepairToken` + an explicit `YES`) targets the EFI System Partition and
  the fallback loader; it never touches NTFS.
- The planned NTFS driver (M12) is read-only by design: MFT/attribute/
  runlist reads only, no write path at all (`ROADMAP_v0.0.2+.md` §4).
- The planned disk imager (M12) copies a whole disk to another disk or an
  image file, but only behind the same repair gate and confirmation.

## If a Windows machine does not boot

Use the tools Microsoft and the vendor provide:

1. Boot the Windows installation media or a **WinPE** image on the broken
   machine (the installation ISO's repair environment is the same thing).
2. In the recovery command prompt, the built-in commands cover the common
   failures:
   - `bootrec /fixmbr`, `bootrec /fixboot`, `bootrec /rebuildbcd`
   - `bcdboot C:\Windows` recreates the BCD store and its firmware boot
     entry from a working Windows installation.
3. On a vendor machine, the vendor recovery media / recovery partition is
   the supported path when the Windows recovery environment itself is
   damaged.

fantuan-kernel can still help alongside those tools for the Linux/BSD side
of a multi-boot disk: partition tables, ESP contents, `grub.cfg`/`fstab`
cross-checks and firmware boot-order diagnosis are read-only by default.
