# fantuan-kernel

A self-written operating system kernel targeting **live rescue systems**:
diagnose hardware and boot-chain problems, then repair them — Linux first,
BSD diagnosis, Windows deferred.

All repository artifacts are in English. The authoritative design lives in
[docs/DESIGN.md](docs/DESIGN.md) — change it before changing code.

## Current state: M3

M0 boot chain (self-written UEFI bootloader, handshake, serial/GOP console),
M1 interrupts (IDT, PIC, PIT timer, TSS/IST, TSC sleep) and M2 memory
(higher-half kernel at PHYS_OFFSET, kernel-owned page tables, bitmap frame
allocator) are in. M3: kernel tasks with a round-robin scheduler (assembly
context switch, 100 ms quantum) and the own syscall ABI v1 (INT 0x60,
versioned dispatch).

- `boot/` — self-written UEFI bootloader in Rust (`x86_64-unknown-uefi`),
  hand-rolled against the UEFI spec: GOP, RSDP, memory map, kernel load via
  Simple File System Protocol, ExitBootServices with map-key retry.
- `boot/entry.S` — the "forever assembly": kernel entry stub (GDT, stack,
  BSS zeroing).
- `kernel/` — minimal `no_std` Rust kernel (`x86_64-unknown-none`):
  BootInfo handshake validation, 16550 serial, PIT sleep + PC-speaker beeps,
  GOP framebuffer console with the embedded Spleen 8x16 font.
- `abi/` — the versioned BootInfo ABI shared by both sides.

## Quickstart

Requirements: Rust (stable) with targets `x86_64-unknown-uefi` +
`x86_64-unknown-none`, QEMU, OVMF (`edk2-ovmf`), binutils (objcopy),
python3 (font regeneration only).

```sh
rustup target add x86_64-unknown-uefi x86_64-unknown-none
tools/run.sh              # headless: everything on the serial console
tools/run.sh --graphics   # with a window (framebuffer console + serial on stdio)
tools/smoke.sh            # CI-style: bounded run, greps for the handshake
```

In QEMU, quit with `Ctrl-A X` (headless) or close the window.
