# M13 Design — Graphics, Input, KMS-like Contract, Repair IPC

> Status: design of record for v0.0.5, written before code (DESIGN §13.5).
> Roadmap: `ROADMAP_v0.0.2+.md` §5. This milestone freezes the interfaces
> that XFCE/Qt consume in M15; it does not ship a desktop.

## 1. Goal

A stable, documented platform surface for GUI software:

1. framebuffer + input APIs inside the kernel (drivers underneath),
2. a **KMS-like ioctl contract** that an unmodified Xorg modesetting driver
   can target from userspace (M14/M15),
3. a **repair-service IPC contract** that Qt frontends speak (M15),
4. a demo that proves the contracts without a userspace yet.

## 2. Framebuffer API (kernel)

- `struct fb_info { base, size, width, height, stride, format, bpp }`,
  sourced from GOP (x86), VBE (BIOS path) or virtio-gpu (later); the existing
  GOP console becomes one client.
- Operations: `blit(src, dst_rect)`, `fill(rect, pixel)`, `copy_rect`,
  `pan(offset)` and a **damage list** (bounded, coalesced rectangles).
- **Double buffering**: an off-screen buffer from the frame allocator when
  memory allows; otherwise direct rendering with damage-tracked updates.
  GOP has no flip, so `flip()` is a damage-list present; when a KMS-like
  device exists (M15 virtio-gpu), flip becomes a real page flip.
- Formats: start with the GOP `Bgrx8888`/`Bgra8888` set plus 16-bit
  fallback; conversion helpers for XRGB/BGR.

### 2.1 Implemented in M13-1 (V-a)

The core landed as `kernel-core::graphics` (gated by `CONFIG_GRAPHICS`,
`no_std`, platform-neutral, no allocation, assert-free):

- `FbInfo { base, width, height, pitch, format }` — `base` is a
  kernel-visible address (each kernel supplies it: GOP via `phys_to_virt`,
  VBE via the `0xBFC00000` PSE slot); `pitch` is bytes per scanline;
  `format` is `Bpp32 | Bpp24 | Bpp16` (the three modes the consoles render).
  `Format::{store,load,xor}` centralise the per-bpp 0x00RRGGBB conversion.
- `Rect { x, y, w, h }` with `intersect`/`union` (the hard-clip primitive).
- `FbInfo::{fill, fill_xor, blit, copy_rect, blit_glyph}` — each hard-clips
  to the surface and returns the clipped `Rect` actually drawn (`None` when
  it clips to nothing). `fill` = solid rect, `fill_xor` = cursor overlay,
  `blit` = raw same-format source pixels with a source pitch, `copy_rect` =
  overlap-safe (memmove) move for scrolling, `blit_glyph` = an 8x16 1bpp
  font blit (the "font blitting = blit" path).
- `Damage` — a bounded (16-entry) dirty-rect list with overlap coalescing; a
  full list merges into the entry whose union grows least, so the dirty
  region stays fully covered. `rects()`/`clear()` are the flush/clear seam.
- `Present` trait + `DirectPresent` — the scanout seam. Single-buffered, the
  render surface IS the device, so `DirectPresent` just clears the damage
  list. Both consoles mark damage after every operation and call `present`
  per character.

### 2.2 Extension points (frozen for M13-2 / M13-4)

- **M13-2 double buffer** *(landed in V-b, §2.3)*: allocate an off-screen
  `FbInfo` from the frame allocator, point the console at it, and replace
  `DirectPresent` with a `BufferedPresent` that copies each damaged rect from
  the off-screen surface to the device `FbInfo` and then clears the list. No
  console change is needed beyond swapping the `&'static dyn Present` in the
  console state.
- **M13-4 dumb buffers / ADDFB**: `FbInfo::blit` (raw same-format source +
  source pitch) is the primitive a userspace dumb buffer maps to; `Format`
  and `Rect` are the values `DRM_IOCTL_MODE_ADDFB` carries into the KMS
  contract of §4. New formats are additive enum variants.

### 2.3 Implemented in M13-2 (V-b)

The double-buffer seam landed exactly on the §2.2 shape:

- `BufferedPresent` (kernel-core) holds the device `FbInfo`; `present(fb,
  damage)` copies each damaged rect from the off-screen `fb` to the device
  with `FbInfo::blit` and clears the list. `blit`'s source pointer is the
  source *rect's* top-left (not the surface base), so `BufferedPresent`
  offsets each rect by `(r.y * fb.pitch + r.x * bpp)`.
- Both consoles start single-buffered (`DirectPresent`, the surface IS the
  scanout) and swap to `BufferedPresent` via `upgrade_buffered` once the
  frame allocator is up: the off-screen frame is `alloc_contiguous`'d to the
  mode's exact byte size (bounded; failure keeps `DirectPresent`), the boot
  banner is copied device→off-screen, and the render surface + present are
  swapped. The back buffer memory source is the frame allocator (accessed
  through the phys→virt alias on both kernels), not a `.bss` static, so the
  i686 `510 MiB usable` boot report is unchanged.
- `Damage` gained `bbox()` (union of dirty rects) and `area()` for the
  demo's damage accounting, and `selftest` gained a `BufferedPresent`
  round-trip (a drawn rect is copied to the device surface, the list is
  cleared, untouched pixels stay put).
- The kernel demo app (§6) drives the seam on both backends.

## 3. Input event API (kernel)

- `struct input_event { type, code, value }` (Linux evdev-compatible names
  and numbers so the userspace input stack needs no translation):
  `EV_KEY`, `EV_REL`, `EV_ABS`, `EV_SYN`.
- Sources: PS/2 keyboard (existing) + PS/2 mouse (new), virtio-input
  (MMIO/PCI) later; each source is an `input_ops` provider feeding one
  bounded ring buffer per device.
- Clients: the kernel console reads scancodes as today; in M14 the ring
  graduates to `/dev/input/event0` with `read()` returning event records.

## 4. KMS-like ioctl contract (frozen in this milestone)

The subset Xorg's `modesetting` driver needs, with Linux-compatible
numbers/structs where they are stable enough to copy field-for-field:

| ioctl | purpose |
|---|---|
| `DRM_IOCTL_VERSION` | driver identity |
| `DRM_IOCTL_MODE_GETRESOURCES` | CRTCs/connectors/encoders/FBs lists |
| `DRM_IOCTL_MODE_GETCONNECTOR` | connection state + modes + EDID blob |
| `DRM_IOCTL_MODE_GETCRTC` | current mode/framebuffer |
| `DRM_IOCTL_MODE_CREATEDUMB` + `MAP_DUMB` | dumb buffer alloc + mmap offset |
| `DRM_IOCTL_MODE_ADDFB` / `RMFB` | framebuffer objects |
| `DRM_IOCTL_MODE_SETCRTC` | mode set / page flip (with a flip event) |
| `DRM_IOCTL_MODE_PAGE_FLIP` | async flip + completion event |

- **EDID**: from GOP/VBE when available; otherwise a built-in 1024x768
  fallback blob, clearly marked in the connector properties.
- **Events**: a bounded event queue (flip completion, hotplug later)
  readable via `read()` on the DRM fd.
- The contract is documented in `GRAPHICS_API.md` (written in this
  milestone) with exact structs and semantics; breaking it later requires an
  append-only extension, never a renumber.

## 5. Repair-service IPC contract (frozen in this milestone)

- Transport: an `AF_UNIX`-style socket pair exposed by the kernel broker
  (M14 provides sockets; M13 defines and demos the protocol over an
  in-kernel pipe).
- Messages (little-endian, versioned header):
  `HELLO`/`HELLO_ACK`, `QUERY_DISKS`, `DIAGNOSE`, `REPAIR_PLAN`,
  `REPAIR_APPLY` (requires a confirmation token), `EVENTS` (progress),
  `CANCEL`, `BYE`.
- **Security**: the broker performs exactly the shell's operations through
  the same `RepairToken` gate; a frontend can never gain extra privilege.
  Cancel is honored between chunks (the imager and surface scan already
  poll a cancel flag).
- Documented in `REPAIR_IPC.md` with the message schema and the
  no-privilege-escalation statement.

## 6. Demo (proof of contract, no userspace)

- A kernel-side framebuffer demo driven by the damage list, with a screendump
  diff in the smoke. **V-b implementation**: a bounded kernel task
  (`kernel/src/gfx_demo.rs`, `kernel-i686/src/gfx_demo.rs`) that runs 40
  frames on the GOP (x86_64 UEFI) and VBE (i686) consoles — a 32×32 white
  box sweeping a fixed bottom band plus an 8-bit green counter (the shared
  `kernel-core::graphics::demo_frame`) — then exits and is reaped. Each frame
  reports its damage rect over the **no-mirror** serial sink (so the demo
  never draws its own text on the screen): `graphics: demo frame=N
  dmg=X,Y,W,H`, plus `graphics: demo start/stop`,
  `graphics: demo damage total=X,Y,W,H rects=R area=A` and
  `graphics: idle damage empty ok`. To make the animation the only thing on
  screen it is spawned after the boot's other tasks have quieted (i686: after
  the 500-tick wait; x86_64: kmain waits for the userland/proc-test tasks to
  be reaped and then freezes the GOP mirror for the demo's duration, which
  the demo unfreezes on exit). riscv/aarch64 have no display, so the demo
  task is x86_64/i686-only and those kernels skip it with no demo marker.
- A kernel-side IPC client that runs `QUERY_DISKS` + `DIAGNOSE` and prints
  the exchange, proving the protocol shape that Qt will speak.

## 7. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M13-1 | `fb_info` + blit/fill/damage core; GOP console refactored onto it | landed (V-a) |
| M13-2 | Double buffer + present; demo app (kernel task) | landed (V-b) |
| M13-3 | Input event ring + PS/2 mouse; console + demo consumers |
| M13-4 | Dumb-buffer objects, ADDFB/SETCRTC/PAGE_FLIP semantics + events |
| M13-5 | EDID sourcing (GOP/VBE) + fallback blob + connector properties |
| M13-6 | Repair broker + protocol + in-kernel client demo |
| M13-7 | Freeze docs (`GRAPHICS_API.md`, `REPAIR_IPC.md`), smoke phases |

## 8. Verification

- `tools/smoke-graphics.sh` (M13-1): boots the x86_64 UEFI image with
  `-vga std` and the i686 VBE image, takes headless screendumps and asserts
  non-blank pixel statistics, the in-kernel `graphics: damage self-test ok`
  and the i686 serial parity (identical to `-vga none` after the `fb:`
  header lines).
- Demo smoke: two timed screendumps per backend (GOP + VBE) differ while the
  demo runs; the bounding box of the differing pixels lies inside the
  reported damage region (`graphics: demo damage bbox=…`), and an idle screen
  reports an empty damage set (`graphics: idle damage empty ok`).
- Input: injected PS/2 events (existing QEMU monitor path) reach the ring
  and the demo reacts; no lost SYNs under a bounded burst.
- KMS contract: a struct-size/offset self-test compiled against the
  documented layout (catches accidental ABI drift in CI).
- IPC: the in-kernel client completes HELLO/QUERY_DISKS/DIAGNOSE and a
  cancelled REPAIR_APPLY leaves nothing written (negative check).

## 9. Risks

| Risk | Mitigation |
|---|---|
| Xorg modesetting needs more than the subset | the table above is the documented minimum; additions are appended with tests before M15 |
| Struct ABI drift | self-test on sizes/offsets in CI; docs are the source of truth |
| Wrong EDID breaks mode selection | built-in fallback plus explicit "synthetic EDID" property |
| IPC misuse escalates privilege | broker uses the same gate as the shell; stated in REPAIR_IPC.md |
| Memory for double buffering | fall back to direct + damage when the frame allocator cannot spare a full buffer |
