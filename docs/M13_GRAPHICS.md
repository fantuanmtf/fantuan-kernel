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

### 3.1 Implemented in M13-3 (V-c)

The core landed as `kernel-core::input_ring` and `kernel-core::mouse`, both
gated by `CONFIG_GRAPHICS` (so the `minimal`/`net` profiles link neither and
carry no `input:`/`PS/2 aux` markers). The design doc's per-device ring and
the evdev `{type, code, value}` tuple are a later refinement; M13-3 ships one
frozen shared ring whose consumers (M13-4 / V-f) compile against these
exact offsets:

- **`InputEvent`** — a `#[repr(C)]`, 32-byte layout (the **M13-3 contract**):

  | offset | size | field      | meaning                              |
  |--------|------|------------|--------------------------------------|
  | 0      | 8    | `ts`       | monotonic nanoseconds (0 = no clock) |
  | 8      | 2    | `kind`     | 1 = key, 2 = pointer                 |
  | 10     | 1    | `flags`    | key: 1 pressed / 0 released          |
  | 11     | 1    | `set`      | key: scancode set (1 after translation) |
  | 12     | 1    | `scancode` | key: make code                       |
  | 13     | 1    | `ascii`    | key: translated ASCII (0 = none)     |
  | 14     | 1    | `buttons`  | pointer: bit 0 left / 1 right / 2 middle |
  | 16     | 2    | `dx`       | pointer: signed relative X           |
  | 18     | 2    | `dy`       | pointer: signed relative Y           |
  | 20     | 12   | `_reserved`| zero; reserved for extension         |

  ABI drift is guarded by a `size_of::<InputEvent>() == 32` check inside the
  ring self-test, run at boot (`input: ring self-test ok`).

- **Ring policy** — one static, lock-protected ring of `RING_CAPACITY = 64`
  events, no allocation. Producers are the PS/2 keyboard/mouse IRQ handlers;
  there is one consumer (the demo; `/dev/input/event0` later). The lock
  disables interrupts so an IRQ cannot preempt a holder (single CPU). When
  full, **the oldest event is dropped** and the `dropped()` counter
  increments (monotonic, never reset). `push_key`/`push_pointer`/`pop` are
  the producer/consumer seam.

- **PS/2 mouse coverage** — x86_64 (`kernel/src/mouse.rs`) and i686
  (`kernel-i686/src/mouse.rs`) bring up the 8042 aux port (enable aux +
  IRQ12, controller config byte preserved from the keyboard init, reset with
  bounded ACK/self-test/ID reads, set defaults, enable 3-byte data
  reporting), decode the standard packet through the shared
  `kernel-core::mouse` decoder (sign + overflow bits, sync-bit resync), and
  feed `push_pointer(dx, dy, buttons)`. The existing keyboard IRQ1 keeps its
  ASCII path into the shell unchanged and additionally pushes key events
  (`scancode/set`, pressed/released, ASCII) into the ring. riscv/aarch64
  have no PS/2 and stay serial-only. The i686 runtime input path is built and
  booted but not driven by a smoke (QEMU-only acceptance for the pointer).

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
   **V-c addition**: the demo also consumes the input ring — pointer events
   move a magenta cursor sprite (a solid 8×8 square drawn in the demo's
   bottom band, so the damage bbox stays tight) and a `q`/`Esc` key press
   stops the animation early, reported as
   `input: demo consumed dx=.. dy=.. buttons=..`, `input: demo cursor x=.. y=..`
   and `input: demo key stop scancode=.. ascii=..` over the no-mirror sink.
   The ring self-test prints `input: ring self-test ok` at boot.
   **V-d addition**: the demo hands the screen to a second task,
   `kernel/src/kms_demo.rs`, once the console is double-buffered (so the
   console's render surface is off-screen and survives a flip). `gfx_demo`
   spawns it and leaves the serial/GOP mirror frozen; the KMS demo owns
   unfreezing on every exit path. It allocates two dumb buffers at the mode
   geometry, registers both with `addfb`, `setcrtc`s the first, then runs
   eight render/`page_flip` cycles, each confirmed by popping a
   `FLIP_COMPLETE` from the event ring: `gfx: addfb id=1 w=.. h=..`,
   `gfx: setcrtc fb=1`, `gfx: page_flip fb=2 event=1` … `fb=1 event=8`,
   `gfx: flip loop ok frames=8`. A negative case proves the geometry check
   (`gfx: setcrtc mismatch ok`), then `rmfb`/`dumb_destroy` return the frames
   and the demo asserts `dumb_used()==0` and that the allocator's usable MiB
   is back to the pre-demo baseline (`gfx: cleanup ok`), restores the console
   surface and exits. The ring self-test prints `gfx: event ring self-test ok`.
- A kernel-side IPC client that runs `QUERY_DISKS` + `DIAGNOSE` and prints
  the exchange, proving the protocol shape that Qt will speak.

## 7. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M13-1 | `fb_info` + blit/fill/damage core; GOP console refactored onto it | landed (V-a) |
| M13-2 | Double buffer + present; demo app (kernel task) | landed (V-b) |
| M13-3 | Input event ring + PS/2 mouse; console + demo consumers | landed (V-c) |
| M13-4 | Dumb-buffer objects, ADDFB/SETCRTC/PAGE_FLIP semantics + events | landed (V-d) |
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
- KMS (M13-4): `tools/smoke-kms.sh` boots the `rescue` profile with
  `--vga std`, relays the serial log over a unix socket (socat) so the
  `gfx:` markers are not buffered behind QEMU's stdio, and asserts the whole
  V-d sequence: the event-ring self-test, both `addfb` lines, `setcrtc fb=1`,
  exactly eight `page_flip` lines with alternating `fb=`/`event=` pairs, the
  `setcrtc mismatch` negative, `cleanup ok` and the absence of a panic. Two
  screendumps taken at the flip markers must differ and their differing
  pixels must lie inside the reported framebuffer geometry; the screendump
  races the demo's 100 ms inter-flip sleep, so the check retries (3
  attempts), matching the project's known QEMU monitor timing flake.
- KMS contract: the M13-4 event-ring/struct ABI self-test runs at boot
  (`gfx: event ring self-test ok`, size/order/drop-oldest guarded). The
  ioctl-level struct-size/offset test against the §4 table arrives with the
  userland boundary (M13-5 / M14).
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
